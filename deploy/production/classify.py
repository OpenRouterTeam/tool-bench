"""Show what production would record for local attempts, using the worker's own code.

Per plan this runs, unmodified: finalize.collect_results (status.json -> result.json for the
latest retry of each trial), finalize.planned_attempts, finalize.aggregate,
result_row.result_rows / score_value / skip_reason and result_row.to_parquet.

Substituted: the GCS artifacts bucket is a local directory. A two-method shim (list_blobs,
blob().download_as_text) serves $WORK as the bucket, so object names keep the production shape
runs/<workflow>/trials/<trial>/<retry>/<attempt>/... Nothing is uploaded; the parquet bytes are
written next to the run instead. ClickHouse billing reconciliation is not run (no model calls).

Run with the worker venv: $VENV/bin/python classify.py --root $WORK --plan $WORK/plans/oracle ...
"""

import argparse
import json
import sys
from datetime import UTC, datetime
from pathlib import Path
from uuid import UUID

from google.api_core.exceptions import NotFound

from gcp_harbor_trial.finalize import aggregate, collect_results, planned_attempts
from gcp_harbor_trial.outcome import Outcome, process_exit_code
from gcp_harbor_trial.plan import SUMMARY_FILE
from gcp_harbor_trial.result_row import (
    RESULT_PARQUET_FILE,
    RunMeta,
    primary_reward,
    result_rows,
    score_counts,
    to_parquet,
    trial_exception,
)


class LocalBlob:
    def __init__(self, root: Path, name: str) -> None:
        self.name = name
        self._path = root / name

    def download_as_text(self) -> str:
        try:
            return self._path.read_text()
        except FileNotFoundError as error:
            raise NotFound(self.name) from error


class LocalBucket:
    """The two storage.Bucket methods finalize.collect_results uses, over a directory."""

    def __init__(self, root: Path) -> None:
        self.root = root

    def list_blobs(self, prefix: str = ""):
        base = self.root / prefix
        for path in sorted(p for p in base.rglob("*") if p.is_file()):
            yield LocalBlob(self.root, path.relative_to(self.root).as_posix())

    def blob(self, name: str) -> LocalBlob:
        return LocalBlob(self.root, name)


def classify(root: Path, plan_out: Path) -> tuple[list[dict], list, list]:
    plan_dir = plan_out / "plan"
    summary = json.loads((plan_dir / SUMMARY_FILE).read_text())
    workflow = summary["runId"]  # production: the Argo workflow name
    job_id = UUID((plan_out / "job-id").read_text().strip())
    collected = collect_results(LocalBucket(root), f"runs/{workflow}/trials/")
    attempts = planned_attempts(plan_dir, collected)
    results = [trial.result for trial in collected.values() if trial.result is not None]
    started_at = min((r.started_at for r in results if r.started_at), default=datetime.now(UTC))
    job = aggregate(
        results, job_id=job_id, n_total_trials=int(summary["trialCount"]), started_at=started_at
    )
    meta = RunMeta(
        task=str(summary["benchmarkId"]),
        epochs=int(summary["attempts"]),
        created_at=job.finished_at or started_at,
        benchmark_config=summary,
    )
    rows = result_rows(attempts, meta=meta, billed_cost=None)
    run_dir = root / "runs" / workflow
    (run_dir / RESULT_PARQUET_FILE).write_bytes(to_parquet(rows))
    (run_dir / "result.json").write_text(job.model_dump_json(indent=2))

    table = []
    for attempt, row in zip(attempts, rows, strict=True):
        exception = trial_exception(attempt.result) if attempt.result is not None else None
        outcome = attempt.outcome
        table.append(
            {
                "run": workflow,
                "trial": attempt.trial_name,
                "status.json outcome": outcome,
                "pod exit code": process_exit_code(Outcome[outcome]) if outcome else None,
                "exception_type": exception.exception_type if exception else None,
                "reward": primary_reward(attempt.result),
                "score_value": row["score_value"],
                "explanation": row["explanation"],
                "run accuracy": row["accuracy"],
                "total_questions": row["total_questions"],
                "correct_answers": row["correct_answers"],
                "job n_errored_trials": job.stats.n_errored_trials,
            }
        )
    return table, attempts, results


def mixed_run(attempts: list, results: list) -> dict[str, dict]:
    """The same attempts as if one run held them all, per agent: what result.json (Harbor's
    JobResult, a None reward counts as 0) and result.parquet (skipped attempts drop out) say."""
    summary = {}
    for agent in sorted({attempt.agent.name for attempt in attempts}):
        mine = [attempt for attempt in attempts if attempt.agent.name == agent]
        scored = [attempt.result for attempt in mine if attempt.result is not None]
        job = aggregate(
            scored, job_id=UUID(int=0), n_total_trials=len(mine), started_at=datetime.now(UTC)
        )
        (metrics,) = [evals.metrics[0] for evals in job.stats.evals.values()]
        accuracy, total_questions, correct_answers = score_counts(mine)
        summary[agent] = {
            "attempts": len(mine),
            "result.json mean reward": round(metrics.get("reward", metrics.get("mean")), 3),
            "result.parquet accuracy": accuracy,
            "result.parquet total_questions": total_questions,
            "result.parquet correct_answers": correct_answers,
        }
    return summary


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--root", type=Path, required=True, help="stands in for the bucket")
    parser.add_argument("--plan", type=Path, action="append", required=True)
    parser.add_argument("--json", type=Path)
    args = parser.parse_args()
    table, attempts, results = [], [], []
    for plan in args.plan:
        rows, plan_attempts, plan_results = classify(args.root.resolve(), plan)
        table += rows
        attempts += plan_attempts
        results += plan_results
    mixed = mixed_run(attempts, results) if len(args.plan) > 1 else {}
    if args.json:
        args.json.write_text(json.dumps({"attempts": table, "as_one_run": mixed}, indent=2) + "\n")
    header = (
        "run", "trial", "status.json outcome", "pod exit code", "exception_type",
        "reward", "score_value", "explanation",
    )  # fmt: skip
    print(" | ".join(header))
    for row in table:
        print(" | ".join(str(row[key]) for key in header))
    for run in dict.fromkeys(row["run"] for row in table):
        first = next(row for row in table if row["run"] == run)
        print(
            f"{run}: accuracy={first['run accuracy']} total_questions={first['total_questions']} "
            f"correct_answers={first['correct_answers']} "
            f"job n_errored_trials={first['job n_errored_trials']}"
        )
    for agent, numbers in mixed.items():
        print(f"all plans as one run, agent {agent}: {numbers}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
