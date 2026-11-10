#!/bin/sh
# Plan, run and classify: TRIAL_TASKS x (oracle, nop), plus one unassessable oracle trial whose
# task copy has a truncated tests/expect.json. Trials run one at a time (shared Docker daemon).
# The fifth default task has one of the longest names: its trial name is truncated to 47
# characters, which makes Harbor hash the verifier's compose project name.
set -eu
HERE=$(cd "$(dirname "$0")" && pwd)
. "$HERE/env.sh"
: "${TRIAL_TASKS:=business__stripe__customer-mrr code__github__file-login-bug ops__multi__pagerduty-sentry-github-issue search__brave__hosting-sla-lookup business__multi__hubspot-largest-deal-salesforce-owner}"
: "${UNASSESSABLE_TASK:=business__stripe__customer-mrr}"
: "${RUN_PREFIX:=tb-local}"
for image in "$WORLD_IMAGE" "$TOOLS_IMAGE"; do
  docker image inspect "$image" >/dev/null 2>&1 || { echo "missing local image $image" >&2; exit 2; }
done

include=""
for task in $TRIAL_TASKS; do include="$include --include $task"; done
plan() { # <name> <agent> [plan.py args]; plan and run directory are both named <prefix>-<name>
  name="$1"; agent="$2"; shift 2
  "$VENV/bin/python" "$HERE/plan.py" --agent "$agent" --run-id "$RUN_PREFIX-$name" \
    --tasks-src "$TASKS_SRC" --out "$WORK/plans/$RUN_PREFIX-$name" \
    --world-image "$WORLD_IMAGE" --tools-image "$TOOLS_IMAGE" "$@"
}
# shellcheck disable=SC2086
plan oracle oracle $include
# shellcheck disable=SC2086
plan nop nop $include
plan unassessable oracle --include "$UNASSESSABLE_TASK" --corrupt-expect "$UNASSESSABLE_TASK"

plans=""
for name in oracle nop unassessable; do
  "$HERE/run-plan.sh" "$RUN_PREFIX-$name"
  plans="$plans --plan $WORK/plans/$RUN_PREFIX-$name"
done

echo "== classification (worker finalize + result_row)"
# shellcheck disable=SC2086
"$VENV/bin/python" "$HERE/classify.py" --root "$WORK" $plans --json "$WORK/classification.json"
echo "== timing and memory"
for name in oracle nop unassessable; do
  "$VENV/bin/python" "$HERE/timeline.py" "$RUNS/$RUN_PREFIX-$name" --json "$RUNS/$RUN_PREFIX-$name/timeline.json"
done
