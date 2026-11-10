"""Static checks of every task directory against the production worker's own functions.

No containers. For each task under --tasks-root this calls, unmodified:
  config.validate_task_dir, plan.plan_task, plan.trial_name_prefix, sizing.sandbox_quantities,
  expected_tools.expected_tool_names, agents.mcp_ready.mcp_ready_command,
and for the whole set: plan.select_tasks (the matrix's task globs), config.load_config on a
planner-built TrialConfig for a scripted agent and for claude-code (the MCP-ready path that
injects mcp_expected_tools), and harbor.Trial.create under the forced environment class.

Run with the worker venv: $VENV/bin/python scale_checks.py --tasks-root DIR [--json OUT]
"""

import argparse
import asyncio
import collections
import json
import os
import re
import sys
import tempfile
import tomllib
import warnings
from pathlib import Path, PurePosixPath
from uuid import uuid4

from harbor import Trial
from harbor.models.task.config import TaskConfig as TaskToml

import gcp_harbor_trial.plan as planner
from gcp_harbor_trial.admin_token import issue_trial_admin_token
from gcp_harbor_trial.agents.mcp_ready import (
    DEFAULT_MCP_READY_TIMEOUT_SEC,
    MCP_EXPECTED_TOOLS_KWARG,
    mcp_ready_command,
)
from gcp_harbor_trial.benchmarks import BENCHMARKS
from gcp_harbor_trial.config import load_config, validate_task_dir
from gcp_harbor_trial.expected_tools import CALL_LINE, expected_tool_names, server_ports
from gcp_harbor_trial.proxy import proxy_env
from gcp_harbor_trial.run_spec import RunSpec
from gcp_harbor_trial.sizing import _environment_quantities, sandbox_quantities

BENCHMARK_ID = "tool-bench"
# Any solve.sh line that replays a tools/call body, whatever helper it goes through.
ANY_CALL_LINE = re.compile(r'^(\w+)\s+(\d+)\s+"\$DIR/(call-\d+\.json)"')


def spec_for(agent: str, model: str, **extra) -> RunSpec:
    return RunSpec.model_validate(
        {
            "runId": f"scale-{agent}",
            "benchmarkId": BENCHMARK_ID,
            "agent": {"name": agent, "modelName": model},
            **extra,
        }
    )


def solution_calls(task_dir: Path) -> dict[str, set[str]]:
    """Ground truth independent of the worker: every call-N.json a solve.sh line replays."""
    ports = server_ports(task_dir)
    called: dict[str, set[str]] = collections.defaultdict(set)
    for line in (task_dir / "solution" / "solve.sh").read_text().splitlines():
        match = ANY_CALL_LINE.match(line)
        if match is None:
            continue
        body = json.loads((task_dir / "solution" / match[3]).read_text())
        called[ports.get(int(match[2]), f"port-{match[2]}")].add(body["params"]["name"])
    return called


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--tasks-root", type=Path, required=True)
    parser.add_argument("--json", type=Path)
    args = parser.parse_args()
    warnings.simplefilter("ignore")  # Harbor's "allowlist ignored, policy is public" warning

    root = args.tasks_root.resolve()
    task_dirs = sorted(p for p in root.iterdir() if p.is_dir())
    benchmark = BENCHMARKS[BENCHMARK_ID]
    nop = spec_for("nop", "scripted/no-model-calls")
    overrides = planner.job_config(nop, benchmark).environment
    report: dict = {"tasks": len(task_dirs)}

    # 1. validate_task_dir + plan_task + sizing --------------------------------------------
    failures: dict[str, str] = {}
    planned: list[planner.PlannedTask] = []
    sizes = collections.Counter()
    for task_dir in task_dirs:
        try:
            validate_task_dir(task_dir, root, overrides)
            planned.append(planner.plan_task(task_dir, benchmark.kind))
            toml = TaskToml.model_validate_toml((task_dir / "task.toml").read_text())
            agent = _environment_quantities(toml.environment)
            verifier = _environment_quantities(toml.verifier.environment)
            sizes[(agent, verifier, sandbox_quantities(toml))] += 1
        except Exception as error:  # report every rejection, never stop at the first
            failures[task_dir.name] = f"{type(error).__name__}: {error}"
    prefixes = collections.Counter(task.trial_name_prefix for task in planned)
    report["validate_and_plan"] = {
        "accepted": len(planned),
        "rejected": failures,
        "trial_name_collisions": sorted(p for p, n in prefixes.items() if n > 1),
        "truncated_trial_names": sum(
            task.trial_name_prefix != re.sub(r"[^a-z0-9]+", "-", task.name.lower()).strip("-")
            for task in planned
        ),
        "pod_spec_patches": len({json.dumps(t.pod_spec_patch, sort_keys=True) for t in planned}),
        "isolation": dict(collections.Counter(task.isolation.value for task in planned)),
    }
    report["sizing"] = [
        {
            "tasks": count,
            "agent_env": agent.resources(),
            "verifier_env": verifier.resources(),
            "dind_container": sandbox.resources(),
            "verifier_changes_it": sandbox != agent,
        }
        for (agent, verifier, sandbox), count in sizes.items()
    ]

    # 2. expected_tool_names against what solve.sh really calls ------------------------------
    full = partial = empty = 0
    missed_tasks: dict[str, dict] = {}
    helper_lines = collections.Counter()
    for task_dir in task_dirs:
        truth = solution_calls(task_dir)
        got = expected_tool_names(task_dir)
        for line in (task_dir / "solution" / "solve.sh").read_text().splitlines():
            match = ANY_CALL_LINE.match(line)
            if match:
                helper_lines[(match[1], bool(CALL_LINE.match(line)))] += 1
        missing = {
            server: sorted(tools - set(got.get(server, [])))
            for server, tools in truth.items()
            if tools - set(got.get(server, []))
        }
        if not missing:
            full += 1
        else:
            missed_tasks[task_dir.name] = {"returned": got, "missed": missing}
            empty += not got
            partial += bool(got)
    report["expected_tool_names"] = {
        "complete": full,
        "partial": partial,
        "empty": empty,
        "solve_sh_call_lines": {
            f"{helper} (worker regex {'matches' if ok else 'MISSES'})": n
            for (helper, ok), n in sorted(helper_lines.items())
        },
        "tasks_with_missed_tools": missed_tasks,
    }

    # 3. the readiness probe command builds for every task's server list --------------------
    probe_failures: dict[str, str] = {}
    server_counts = collections.Counter()
    for task_dir in task_dirs:
        toml = TaskToml.model_validate_toml((task_dir / "task.toml").read_text())
        servers = toml.environment.mcp_servers
        server_counts[(len(servers), tuple(sorted({s.transport for s in servers})))] += 1
        try:
            command = mcp_ready_command(
                servers, DEFAULT_MCP_READY_TIMEOUT_SEC, PurePosixPath("/logs/agent/mcp-ready.log")
            )
            if command is None:
                probe_failures[task_dir.name] = "no remote servers: probe skipped"
        except Exception as error:
            probe_failures[task_dir.name] = f"{type(error).__name__}: {error}"
    report["mcp_ready_command"] = {
        "builds": len(task_dirs) - len(probe_failures),
        "failures": probe_failures,
        "servers_per_task": {f"{n} servers {list(t)}": c for (n, t), c in server_counts.items()},
    }

    # 4. select_tasks with the matrix's task selections -------------------------------------
    selections = {}
    for label, tasks in {
        "tb (no filter)": {},
        "tbs (include search__*, limit 15)": {"include": ["search__*"], "limit": 15},
        "group glob business__*": {"include": ["business__*"]},
    }.items():
        spec = spec_for("nop", "scripted/no-model-calls", tasks=tasks)
        selected = asyncio.run(planner.select_tasks(spec, root))
        selections[label] = len(selected)
    report["select_tasks"] = selections

    # 5. load_config + Trial.create on planner-built configs, scripted and MCP-ready agents --
    # Harbor resolves the planner's ${VAR} env templates when the trial is created; give them
    # the values the worker's main() and trial_session_proxy would (placeholders, no secrets).
    os.environ["OPENROUTER_API_KEY"] = "sk-or-v1-placeholder-not-a-real-key"
    issue_trial_admin_token(os.environ)
    os.environ.update(proxy_env("http://127.0.0.1:9"))
    with tempfile.TemporaryDirectory() as scratch:
        scratch_dir = Path(scratch)
        for agent, model in (("nop", "scripted/no-model-calls"), ("claude-code", "x/y")):
            spec = spec_for(agent, model)
            configs = planner.expand_trials(
                planner.job_config(spec, benchmark),
                planned,
                tasks_mount=root,
                source=spec.benchmark_id,
                job_id=uuid4(),
                run_id=spec.run_id,
                experiment_ids=spec.experiment_ids,
                request_overlay=spec.effective_request_overlay(),
                base_url=spec.base_url,
            )
            loaded = created = with_expected = 0
            errors: dict[str, str] = {}
            for name, config in configs.items():
                path = scratch_dir / f"{agent}-{name}.json"
                path.write_text(config.model_dump_json())
                try:
                    attempt_dir = scratch_dir / agent / name
                    final = load_config(path, attempt_dir, name, tasks_root=root)
                    loaded += 1
                    with_expected += MCP_EXPECTED_TOOLS_KWARG in final.agent.kwargs
                    if agent == "nop":  # constructing a model agent needs its CLI settings
                        asyncio.run(Trial.create(final))
                        created += 1
                except Exception as error:
                    errors[name] = f"{type(error).__name__}: {error}"
            report[f"load_config[{agent}]"] = {
                "configs": len(configs),
                "load_config_ok": loaded,
                "trial_create_ok": created if agent == "nop" else "not run",
                "with_mcp_expected_tools_kwarg": with_expected,
                "errors": {
                    message: [n for n, m in errors.items() if m == message][:3]
                    for message in sorted(set(errors.values()))
                },
                "error_count": len(errors),
            }

    text = json.dumps(report, indent=2, default=str)
    if args.json:
        args.json.write_text(text + "\n")
    brief = json.loads(text)
    brief["expected_tool_names"]["tasks_with_missed_tools"] = sorted(
        brief["expected_tool_names"]["tasks_with_missed_tools"]
    )
    print(json.dumps(brief, indent=2))
    return 0


if __name__ == "__main__":
    sys.exit(main())
