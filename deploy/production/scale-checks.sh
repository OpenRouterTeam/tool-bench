#!/bin/sh
# No containers: plan all tasks with the production planner, then run every task directory
# through the worker's validation, sizing, expected-tools and config-loading functions.
set -eu
HERE=$(cd "$(dirname "$0")" && pwd)
. "$HERE/env.sh"
"$VENV/bin/python" "$HERE/plan.py" --agent nop --run-id tb-local-all --tasks-src "$TASKS_SRC" \
  --out "$WORK/plans/tb-local-all" --world-image "$WORLD_IMAGE" --tools-image "$TOOLS_IMAGE"
"$VENV/bin/python" "$HERE/scale_checks.py" --tasks-root "$WORK/plans/tb-local-all/tasks" \
  --json "$WORK/scale-checks.json"
