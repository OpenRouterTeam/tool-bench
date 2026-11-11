"""Exercise the production MCP readiness path against a task's live tools sidecar, no model.

The TrialConfig is the planner's claude-code config (job_config + expand_trials), passed through
the worker's real load_config, which forces the environment class and injects mcp_expected_tools
from expected_tool_names. Then, and only for this side experiment, the agent import path is
swapped to mcp_probe_agent:McpProbeAgent (the worker's McpReadyWaitMixin on a no-model agent)
and the trial runs through worker.run_trial (Trial.create(config).run()).

Not production-faithful, stated plainly: execute_attempt is not used (its allow-list rejects the
probe agent), the session proxy is not started, and no harness runs. What is real: the probe
shell loop, its 120 s bound and infra-fail policy, the 32 servers of the task's own image, the
expected-tool comparison, the forced environment class, collect hooks and the verifier.

usage: PYTHONPATH=<this dir> $VENV/bin/python probe_mcp.py --tasks-root DIR --task NAME --out DIR
"""

import argparse
import asyncio
import json
import os
import sys
import warnings
from pathlib import Path
from uuid import uuid4

import gcp_harbor_trial.plan as planner
from gcp_harbor_trial.admin_token import issue_trial_admin_token
from gcp_harbor_trial.agents.claude_code import CLAUDE_CODE_IMPORT_PATH
from gcp_harbor_trial.benchmarks import BENCHMARKS
from gcp_harbor_trial.config import load_config
from gcp_harbor_trial.proxy import proxy_env
from gcp_harbor_trial.run_spec import RunSpec
from gcp_harbor_trial.worker import run_trial

PROBE_AGENT = "mcp_probe_agent:McpProbeAgent"


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--tasks-root", type=Path, required=True)
    parser.add_argument("--task", required=True)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--hold-sec", type=int, default=30)
    args = parser.parse_args()
    warnings.simplefilter("ignore")

    root = args.tasks_root.resolve()
    benchmark = BENCHMARKS["tool-bench"]
    spec = RunSpec.model_validate(
        {
            "runId": "tb-local-mcp-probe",
            "benchmarkId": "tool-bench",
            "agent": {"name": "claude-code", "modelName": "scripted/no-model-calls"},
        }
    )
    task = planner.plan_task(root / args.task, benchmark.kind)
    configs = planner.expand_trials(
        planner.job_config(spec, benchmark),
        [task],
        tasks_mount=root,
        source=spec.benchmark_id,
        job_id=uuid4(),
        run_id=spec.run_id,
        experiment_ids=spec.experiment_ids,
        request_overlay=spec.effective_request_overlay(),
        base_url=spec.base_url,
    )
    ((name, planned),) = configs.items()
    attempt_id = f"mcp-{name}"[:48].rstrip("-")
    attempt_dir = args.out.resolve() / attempt_id
    attempt_dir.mkdir(parents=True)
    config_path = attempt_dir / "planned-config.json"
    config_path.write_text(planned.model_dump_json(indent=2))

    config = load_config(config_path, attempt_dir, attempt_id, tasks_root=root)
    assert config.agent.import_path == CLAUDE_CODE_IMPORT_PATH
    config.agent.import_path = PROBE_AGENT
    config.agent.kwargs["hold_sec"] = args.hold_sec

    os.environ["HARBOR_TELEMETRY"] = "0"
    os.environ["OPENROUTER_API_KEY"] = "sk-or-v1-placeholder-not-a-real-key"
    issue_trial_admin_token(os.environ)
    os.environ.update(proxy_env("http://127.0.0.1:9"))

    result = asyncio.run(run_trial(config))
    trial_dir = config.trials_dir / config.trial_name
    probe = trial_dir / "agent" / "mcp-probe.json"
    print(
        json.dumps(
            {
                "attempt": attempt_id,
                "agent_kwargs_from_load_config": {
                    key: value for key, value in config.agent.kwargs.items() if key != "hold_sec"
                },
                "probe": json.loads(probe.read_text()) if probe.exists() else None,
                "exception": result.exception_info.exception_type
                if result.exception_info
                else None,
                "rewards": result.verifier_result.rewards.get("reward")
                if result.verifier_result and result.verifier_result.rewards
                else None,
                "trial_dir": str(trial_dir),
            },
            indent=2,
        )
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
