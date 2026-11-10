# Production-worker rehearsal

Runs tool-bench tasks through the real production worker (`services/gcp-harbor-trial` in
openrouter-web, `harbor==0.23.0`) on a local Docker daemon, with Harbor's scripted `oracle` and
`nop` agents only. No model calls, no credentials. Results of the last run: `FINDINGS.md`.

## Parameters (`env.sh`; override any of them from the environment)

| Variable | Default | Meaning |
| --- | --- | --- |
| `OPENROUTER_WEB` | `/Users/luke/github/openrouter-web` | worker repo; only `git rev-parse` and `git archive` run on it |
| `WORKER_REF` | `origin/main` | ref whose worker is extracted (pin a SHA for repeatable runs) |
| `TASKS_SRC` | `/tmp/tb-stock/tasks` | generated tasks; read and copied, never edited |
| `WORLD_IMAGE`, `TOOLS_IMAGE` | `tool-bench-world:e45-std`, `tool-bench-tools:e45` | images the tasks must reference (checked, never rewritten) |
| `WORK` | `/tmp/tb-prod-worker` | everything created: `src-<sha>/`, `venv/`, `plans/`, `runs/` |
| `PROXY` | `loopback` | `real` on a host that shares dockerd's network namespace |

## Reproduce

```sh
cd deploy/production
export WORKER_REF=6a8640762df7a2302bb1e9519087ae34b38717f7   # the ref FINDINGS.md was run against
./setup-worker.sh     # 1 min: extract the worker, uv sync --frozen into $WORK/venv
./rehearse.sh         # 15 min: 5 tasks x (oracle, nop) + 1 unassessable, then classify + timings
./scale-checks.sh     # 10 s, no containers: all 150 tasks through the worker's own functions
./extras.sh           # 8 min, optional: network observation, MCP readiness probe, stop signals
./cleanup.sh          # remove the one image the worker builds; never prunes
```

`TRIAL_TASKS` (space separated task directory names) and `UNASSESSABLE_TASK` choose the tasks.
Trials run one at a time, about 75 s each. A rerun adds the next retry directory.

## What each script does

- `setup-worker.sh`: `git archive <ref> services/gcp-harbor-trial`, then the worker Dockerfile's
  `uv sync --frozen --no-dev --no-editable` into an isolated venv.
- `plan.py`: the worker's `plan_run` and `write_plan` (`RunSpec`, `BENCHMARKS["tool-bench"]`,
  `select_tasks`, `plan_task`, `job_config`, `expand_trials`). Writes `$WORK/plans/<run-id>/`:
  `plan/trial-configs/<trial>.json`, `plan/pod-spec-patches.json`, and `tasks/` (the copy
  production mounts at `/inputs/tasks`). `--corrupt-expect TASK` truncates that copy's
  `tests/expect.json`; `--stop-signal-kill TASK` adds SIGKILL stop signals to it.
- `run-plan.sh <run-id> [trial...]`, `run_trial.py`: one attempt per trial through
  `gcp_harbor_trial.trial.main()` with production's four arguments. Output has the artifact
  bucket's shape: `$WORK/runs/<run-id>/trials/<trial>/<retry>/<attempt-id>/`.
- `monitor.sh`: `docker events` and `docker stats` while trials run (read-only).
- `classify.py`: the worker's `finalize.collect_results`, `planned_attempts`, `aggregate`,
  `result_row.result_rows` and `to_parquet` over those directories.
- `timeline.py <run dir>`: wall clock, Harbor phases, verifier breakdown, stop waits, memory.
- `scale_checks.py`: `validate_task_dir`, `plan_task`, `sandbox_quantities`,
  `expected_tool_names`, `mcp_ready_command`, `select_tasks`, `load_config`, `Trial.create`.
- `extras.sh`: `observe-network.sh` (what network the agent and verifier containers really get),
  `probe_mcp.py` + `mcp_probe_agent.py` (the worker's MCP readiness mixin on a no-model agent
  that then holds 32 sessions open), and the stop-signal timing experiment.

## Substitutions (everything else is the worker's code, unmodified)

| Production piece | Local substitute | Where |
| --- | --- | --- |
| Session proxy bound to the Docker bridge gateway | same proxy bound to `127.0.0.1`; macOS cannot bind the gateway address. `PROXY=real` removes this | `run_trial.py` |
| `git clone` of OpenRouterTeam/tool-bench | copy of `TASKS_SRC` | `plan.py` |
| `RunSpec` agent `oracle` (rejected by the worker) | validated as `nop`, then renamed | `plan.py` |
| Argo workflow UID, retry number, attempt id | uuid4, directory count, planned trial name | `plan.py`, `run-plan.sh` |
| GCS artifacts bucket | directory shim with the same object names | `classify.py` |
| `OPENROUTER_API_KEY` | placeholder string, forced | `run_trial.py` |
| Per-pod dind with a registry mirror | the shared local daemon, images already present | none |

Not exercised: Argo, GKE scheduling of the pod spec patch, registry pulls and auth, ClickHouse
billing reconciliation, any model harness. `probe_mcp.py` does not go through `execute_attempt`
(its agent allow-list rejects the probe agent); it calls `load_config` and `worker.run_trial`.
