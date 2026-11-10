#!/bin/sh
# Optional observations beyond rehearse.sh's graded trials. Needs rehearse.sh's plans.
#  1. One more oracle attempt (the next retry) while observe-network.sh records the network the
#     agent container and the separate verifier container really get.
#  2. The production MCP readiness probe against live tools sidecars, driven by a no-model agent
#     that then holds one MCP session per server open so tools-container memory can be sampled.
#  3. One oracle trial on a task copy whose four containers stop on SIGKILL (stop_signal in the
#     compose file, STOPSIGNAL in tests/Dockerfile): the wall clock without Docker's 10 s waits.
set -eu
HERE=$(cd "$(dirname "$0")" && pwd)
. "$HERE/env.sh"
: "${RUN_PREFIX:=tb-local}"
: "${OBSERVE_TRIAL:=search-brave-hosting-sla-lookup-a1}"
: "${PROBE_TASKS:=business__stripe__customer-mrr ops__multi__pagerduty-sentry-github-issue}"
: "${HOLD_SEC:=30}"
: "${STOPFIX_TASK:=business__stripe__customer-mrr}"

"$HERE/observe-network.sh" "$OBSERVE_TRIAL" "$WORK/network-observation.txt" 900 &
observer=$!
"$HERE/run-plan.sh" "$RUN_PREFIX-oracle" "$OBSERVE_TRIAL"
wait "$observer" || true
echo "== network observation"; cat "$WORK/network-observation.txt"

probe_dir="$RUNS/$RUN_PREFIX-mcp-probe"
mkdir -p "$probe_dir"
"$HERE/monitor.sh" start "$probe_dir/monitor"
trap '"$HERE/monitor.sh" stop "$probe_dir/monitor"' EXIT INT TERM
for task in $PROBE_TASKS; do
  out="$probe_dir/$(date +%Y%m%dT%H%M%S)"
  PYTHONPATH="$HERE" "$VENV/bin/python" "$HERE/probe_mcp.py" --tasks-root "$WORK/plans/$RUN_PREFIX-oracle/tasks" \
    --task "$task" --out "$out" --hold-sec "$HOLD_SEC" | tee -a "$probe_dir/probe-results.jsonl"
done
echo "== peak memory per container while MCP sessions were held (MiB)"
awk -F'\t' '$2 ~ /^mcp-/ { split($3, m, " "); v = m[1]; u = v; gsub(/[0-9.]/, "", u); sub(/[A-Za-z]+$/, "", v); v += 0;
  if (u == "GiB") v *= 1024; else if (u == "KiB") v /= 1024; else if (u == "B") v /= 1048576;
  if (v > peak[$2]) peak[$2] = v } END { for (n in peak) printf "%8.0f  %s\n", peak[n], n }' \
  "$probe_dir/monitor/stats.tsv" | sort -k2

echo "== stop-signal experiment"
"$VENV/bin/python" "$HERE/plan.py" --agent oracle --run-id "$RUN_PREFIX-stopfix" --tasks-src "$TASKS_SRC" \
  --out "$WORK/plans/$RUN_PREFIX-stopfix" --world-image "$WORLD_IMAGE" --tools-image "$TOOLS_IMAGE" \
  --include "$STOPFIX_TASK" --stop-signal-kill "$STOPFIX_TASK"
"$HERE/monitor.sh" stop "$probe_dir/monitor"
"$HERE/run-plan.sh" "$RUN_PREFIX-stopfix"
"$VENV/bin/python" "$HERE/classify.py" --root "$WORK" --plan "$WORK/plans/$RUN_PREFIX-stopfix" 2>/dev/null
"$VENV/bin/python" "$HERE/timeline.py" "$RUNS/$RUN_PREFIX-stopfix"
