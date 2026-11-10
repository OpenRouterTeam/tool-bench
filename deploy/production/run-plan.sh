#!/bin/sh
# Run every trial of one plan, sequentially, through the production worker entry point.
# usage: run-plan.sh <plan-name> [trial-name ...]      (plan at $WORK/plans/<plan-name>)
# Output mirrors the production artifact layout so finalize's own collect_results can read it:
#   $RUNS/<run-id>/trials/<trial>/<retry>/<attempt-id>/{status.json,runner.log,trials/<attempt-id>/result.json}
# PROXY=real on a host that shares dockerd's network namespace; default loopback (see run_trial.py).
set -eu
HERE=$(cd "$(dirname "$0")" && pwd)
. "$HERE/env.sh"
: "${PROXY:=loopback}"
plan_name="$1"; shift
plan_dir="$WORK/plans/$plan_name"
run_id=$("$VENV/bin/python" -c 'import json,sys; print(json.load(open(sys.argv[1]))["runId"])' "$plan_dir/plan/plan.json")
run_dir="$RUNS/$run_id"
mkdir -p "$run_dir"
"$HERE/monitor.sh" start "$run_dir/monitor"
trap '"$HERE/monitor.sh" stop "$run_dir/monitor"' EXIT INT TERM

if [ "$#" -gt 0 ]; then trials="$*"; else
  trials=$(cd "$plan_dir/plan/trial-configs" && ls | sed 's/\.json$//')
fi
for trial in $trials; do
  base="$run_dir/trials/$trial"
  mkdir -p "$base"
  retry=$(find "$base" -mindepth 1 -maxdepth 1 -type d | wc -l | tr -d ' ')   # Argo retry number
  out="$base/$retry"
  mkdir -p "$out"
  start=$(date +%s)
  code=0
  "$VENV/bin/python" "$HERE/run_trial.py" --proxy "$PROXY" \
    --config "$plan_dir/plan/trial-configs/$trial.json" \
    --output-dir "$out" --attempt-id "$trial" --tasks-root "$plan_dir/tasks" \
    2>> "$run_dir/worker.jsonl" || code=$?
  end=$(date +%s)
  printf '%s\t%s\t%s\t%s\t%s\t%s\n' "$run_id" "$trial" "$retry" "$start" "$end" "$code" >> "$run_dir/attempts.tsv"
  echo "$run_id $trial retry=$retry worker-exit=$code wall=$((end - start))s $(cat "$out/$trial/status.json" 2>/dev/null)"
done
