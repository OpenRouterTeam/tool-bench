"""Plan a tool-bench run the way production does, from a local tasks directory.

Runs the worker's own planner (gcp_harbor_trial.plan.plan_run + write_plan) end to end:
RunSpec validation, BENCHMARKS["tool-bench"], select_tasks, plan_task (sizing), the task copy
into <out>/tasks, the trial-name collision check, job_config and expand_trials.

Substituted, and nothing else:
  * clone_dataset: instead of `git clone https://github.com/OpenRouterTeam/tool-bench.git`, the
    "checkout" is a copy of --tasks-src. The recorded datasetCommit is a sha1 of that tree.
    --corrupt-expect TASK and --stop-signal-kill TASK edit that copy for two experiments.
  * agent name `oracle`: RunSpec's HarborAgentName literal has no `oracle` (production cannot
    plan an oracle run), so the spec is validated as `nop` and the name swapped afterwards.
  * WORKFLOW_UID: a fresh uuid4 stands in for the Argo workflow UID.

Run with the worker venv: $VENV/bin/python plan.py --agent oracle --run-id ... --out ...
"""

import argparse
import asyncio
import hashlib
import json
import shutil
import sys
from pathlib import Path
from uuid import UUID, uuid4

import gcp_harbor_trial.plan as planner
from gcp_harbor_trial.benchmarks import BENCHMARKS, Dataset
from gcp_harbor_trial.run_spec import RunSpec

BENCHMARK_ID = "tool-bench"


def tree_digest(root: Path) -> str:
    digest = hashlib.sha1()
    for path in sorted(p for p in root.rglob("*") if p.is_file()):
        digest.update(path.relative_to(root).as_posix().encode() + b"\0")
        digest.update(hashlib.sha1(path.read_bytes()).digest())
    return digest.hexdigest()


def check_image_references(task_dir: Path, world: str, tools: str) -> list[str]:
    """Tasks are generated against exact image references (tests/task-context.json records them
    and hashes environment/*), so they are checked, never rewritten."""
    problems = []
    compose = (task_dir / "environment" / "docker-compose.yaml").read_text()
    context = json.loads((task_dir / "tests" / "task-context.json").read_text())
    verifier_from = (task_dir / "tests" / "Dockerfile").read_text().splitlines()[0]
    if f"image: {world}\n" not in compose or context.get("world_reference") != world:
        problems.append(f"world image is not {world}")
    if f"image: {tools}\n" not in compose or context.get("tools_reference") != tools:
        problems.append(f"tools image is not {tools}")
    if verifier_from != f"FROM {world}":
        problems.append(f"tests/Dockerfile is not FROM {world}")
    return problems


def stop_with_sigkill(task_dir: Path, world: str, tools: str) -> None:
    """Experiment for FINDINGS item 3: make `docker stop` immediate for all four containers."""
    compose = task_dir / "environment" / "docker-compose.yaml"
    text = compose.read_text()
    for anchor in ("  main:\n", f"    image: {world}\n", f"    image: {tools}\n"):
        if text.count(anchor) != 1:
            raise ValueError(f"{compose}: expected exactly one {anchor!r}")
        text = text.replace(anchor, f"{anchor}    stop_signal: SIGKILL\n")
    compose.write_text(text)
    dockerfile = task_dir / "tests" / "Dockerfile"
    dockerfile.write_text(dockerfile.read_text().rstrip("\n") + "\nSTOPSIGNAL SIGKILL\n")


def local_checkout(
    tasks_src: Path,
    tasks_path: str,
    corrupt_expect: list[str],
    stop_signal_kill: list[str],
    images: tuple[str, str],
):
    def clone(dataset: Dataset, ref: str, destination: Path) -> str:
        tasks = destination / tasks_path
        tasks.parent.mkdir(parents=True, exist_ok=True)
        shutil.copytree(tasks_src, tasks, symlinks=True)
        for name in corrupt_expect:
            expect = tasks / name / "tests" / "expect.json"
            text = expect.read_text()
            # Deliberately unparseable contract: the first half of the file, cut mid-document.
            expect.write_text(text[: len(text) // 2])
        for name in stop_signal_kill:
            stop_with_sigkill(tasks / name, *images)
        return tree_digest(tasks)

    return clone


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--agent", required=True, choices=["oracle", "nop"])
    parser.add_argument("--run-id", required=True)
    parser.add_argument("--tasks-src", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True, help="gets plan/, tasks/, work/")
    parser.add_argument("--world-image", required=True)
    parser.add_argument("--tools-image", required=True)
    parser.add_argument("--include", action="append", default=[], help="task name glob")
    parser.add_argument("--limit", type=int)
    parser.add_argument("--attempts", type=int, default=1)
    parser.add_argument("--model", default="scripted/no-model-calls")
    parser.add_argument("--corrupt-expect", action="append", default=[], metavar="TASK")
    parser.add_argument("--stop-signal-kill", action="append", default=[], metavar="TASK")
    parser.add_argument("--job-id", type=UUID, default=None)
    args = parser.parse_args()

    bad = {
        task.name: problems
        for task in sorted(p for p in args.tasks_src.iterdir() if p.is_dir())
        if (problems := check_image_references(task, args.world_image, args.tools_image))
    }
    if bad:
        print(f"{len(bad)} tasks reference other images, e.g. {next(iter(bad.items()))}")
        print("Regenerate the tasks with taskgen for these tags; do not rewrite them in place.")
        return 2

    spec = RunSpec.model_validate(
        {
            "runId": args.run_id,
            "benchmarkId": BENCHMARK_ID,
            "agent": {"name": "nop", "modelName": args.model},
            "attempts": args.attempts,
            "tasks": {"include": args.include, "limit": args.limit},
        }
    )
    if args.agent == "oracle":
        spec = spec.model_copy(
            update={"agent": spec.agent.model_copy(update={"name": "oracle"})}
        )
    benchmark = BENCHMARKS[spec.benchmark_id]
    job_id = args.job_id or uuid4()

    shutil.rmtree(args.out, ignore_errors=True)
    tasks_out = (args.out / "tasks").resolve()
    planner.clone_dataset = local_checkout(
        args.tasks_src.resolve(),
        benchmark.dataset.tasks_path,
        args.corrupt_expect,
        args.stop_signal_kill,
        (args.world_image, args.tools_image),
    )
    plan = asyncio.run(
        planner.plan_run(
            spec,
            benchmark,
            work_dir=args.out / "work",
            tasks_out=tasks_out,
            tasks_mount=tasks_out,  # production: /inputs/tasks
            job_id=job_id,
            image_mirror=None,  # production: TASK_IMAGE_MIRROR (rewrites public.ecr.aws only)
        )
    )
    planner.write_plan(plan, spec, args.out / "plan")
    shutil.rmtree(args.out / "work", ignore_errors=True)
    (args.out / "job-id").write_text(f"{job_id}\n")
    print(
        f"planned {spec.run_id}: agent={spec.agent.name} tasks={len(plan.tasks)} "
        f"trials={len(plan.trial_configs)} unsupported={len(plan.unsupported)} "
        f"dataset={plan.dataset_commit} job_id={job_id}"
    )
    for name, reason in sorted(plan.unsupported.items()):
        print(f"  unsupported {name}: {reason}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
