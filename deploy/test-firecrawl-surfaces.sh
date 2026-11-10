#!/bin/sh
set -eu
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
WORLD_IMAGE="${WORLD_IMAGE:?provide the revision42 world candidate}"
TOOLS_IMAGE="${TOOLS_IMAGE:?provide the matching current-catalog tools candidate}"
OUT="${CONTROL_OUT:-$(mktemp -d /tmp/tool-bench-firecrawl-surfaces.XXXXXXXX)}"
test -d "$OUT"
test -z "$(find "$OUT" -mindepth 1 -maxdepth 1 -print -quit)"
printf 'artifacts: %s\n' "$OUT"
if [ -n "${TASKGEN_BINARY:-}" ]; then
  "$TASKGEN_BINARY" --out "$OUT/tasks" --world-image "$WORLD_IMAGE" --tools-image "$TOOLS_IMAGE" >/dev/null
else
  cargo run -q --locked -p scenario --bin taskgen -- --runner-admission required --out "$OUT/tasks" \
    --world-image "$WORLD_IMAGE" --tools-image "$TOOLS_IMAGE" >/dev/null
fi
TASK="$OUT/tasks/search__firecrawl__monitor-competitor-pricing"
cp "$TASK/solution/solve.sh" "$TASK/solution/reference-solve.sh"
jq -n '{jsonrpc:"2.0",id:2,method:"tools/call",
  params:{name:"firecrawl_monitor_list",arguments:{}}}' > "$TASK/solution/surface-read.json"
jq -n '{jsonrpc:"2.0",id:2,method:"tools/call",
  params:{name:"firecrawl_monitor_get",arguments:{id:"not-a-surface-monitor"}}}' \
  > "$TASK/solution/surface-error.json"
sed -e '/^bound_call [0-9]/,$d' -e '/^call [0-9]/,$d' \
  "$OUT/tasks/ops__grafana__silence-replica-maintenance/solution/solve.sh" > "$TASK/solution/solve.sh"
cp "$OUT/tasks/ops__grafana__silence-replica-maintenance/solution/"*.jq "$TASK/solution/"
cat >> "$TASK/solution/solve.sh" <<'PROGRAM'
export ORACLE_REQUEST_CAPTURE_DIR=/logs/artifacts/mcp-requests
mkdir -p /logs/artifacts/surfaces
SURFACES=/logs/artifacts/surfaces
sh "$DIR/reference-solve.sh" > "$SURFACES/reference.raw"
cat "$SURFACES/reference.raw"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
curl -fsS -D "$WORK/headers" -H 'content-type: application/json' \
  -H 'accept: application/json, text/event-stream' -d "$INIT" http://tools:7102/mcp \
  > "$SURFACES/initialize.raw"
sid="$(tr -d '\r' < "$WORK/headers" | awk -F': ' 'tolower($1)=="mcp-session-id" {print $2}')"
test -n "$sid"
jq -Rse --argjson expected_id 1 --arg method initialize -f "$DIR/response.jq" \
  "$SURFACES/initialize.raw" > "$SURFACES/initialize.json"
curl -fsS -o /dev/null -H 'content-type: application/json' -H "mcp-session-id: $sid" \
  -d '{"jsonrpc":"2.0","method":"notifications/initialized"}' http://tools:7102/mcp
curl -fsS -H 'content-type: application/json' -H 'accept: application/json, text/event-stream' \
  -H "mcp-session-id: $sid" -d '{"jsonrpc":"2.0","id":2,"method":"tools/list"}' \
  http://tools:7102/mcp > "$SURFACES/tools.raw"
jq -Rse --arg method tools/list -f "$DIR/response.jq" \
  "$SURFACES/tools.raw" > "$SURFACES/tools.json"
for item in read error; do
  cp "$DIR/surface-$item.json" "$SURFACES/$item-request.json"
  curl -fsS -H 'content-type: application/json' -H 'accept: application/json, text/event-stream' \
    -H "mcp-session-id: $sid" --data-binary @"$SURFACES/$item-request.json" \
    http://tools:7102/mcp > "$SURFACES/$item.raw"
done
jq -Rse -f "$DIR/response.jq" "$SURFACES/read.raw" > "$SURFACES/read.json"
jq -e -L "$DIR" --arg endpoint 7102 --slurpfile request "$DIR/surface-read.json" \
  --slurpfile contracts "$DIR/result-contracts.json" -f "$DIR/tool-result.jq" \
  "$SURFACES/read.json" >/dev/null
if jq -Rse -f "$DIR/response.jq" "$SURFACES/error.raw" \
    > "$WORK/unexpected" 2> "$SURFACES/expected-error.log"; then
  printf '%s\n' 'A failed native monitor read passed the common error guard' >&2
  exit 1
fi
test ! -s "$WORK/unexpected"
printf '%s\n' FIRECRAWL_SURFACE_CAPTURE_COMPLETED
PROGRAM
sh -n "$TASK/solution/solve.sh"
cp -R "$TASK/solution" "$OUT/program"
if [ "${GENERATE_ONLY:-0}" = 1 ]; then
  printf '%s\n' 'surface program generated; no runtime capture claimed'
  exit 0
fi
sh deploy/run-trial.sh -p "$TASK" -a oracle -o "$OUT/capture" > "$OUT/capture.log" 2>&1
reward="$(find "$OUT/capture" -path '*/verifier/reward.json' -print -quit)"
test -n "$reward"
trial="$(dirname "$(dirname "$reward")")"
surfaces="$trial/artifacts/logs/artifacts/surfaces"
jq -e '.reward==1 and .evaluator_version==42 and .contract_version==2
  and .environment_ok==1 and .contract_ok==1 and .integrity_ok==1
  and .runner_admission_checked==1 and .runner_admission_ok==1
  and .preservation_checked==1 and .history_checked==1
  and .preservation_ok==1 and .history_ok==1 and .adjudication_needed==0' "$reward" >/dev/null
jq -S '.tools|sort_by(.name)' "$surfaces/tools.json" > "$OUT/actual-tools.json"
jq -S '.tools|sort_by(.name)' servers/snapshots/firecrawl.json > "$OUT/expected-tools.json"
cmp "$OUT/actual-tools.json" "$OUT/expected-tools.json"
jq -e --slurpfile expected servers/snapshots/firecrawl.json '
  .tools|map(.name)==($expected[0].tools|map(.name)) and length==28' "$surfaces/tools.json" >/dev/null
jq -e --slurpfile expected servers/snapshots/firecrawl.json '
  .serverInfo.name==$expected[0].server_info.name
    and .serverInfo.version==$expected[0].server_info.version' "$surfaces/initialize.json" >/dev/null
jq -e '.structuredContent.success==true and (.structuredContent.data|length)==3
  and ([.structuredContent.data[]|select(.name=="Fabrikam pricing")]|length)==1' \
  "$surfaces/read.json" >/dev/null
jq -e '.entries|map(select(.service=="firecrawl"))|length>0
  and all(.[];.scheme=="https" and .host=="api.firecrawl.dev")' "$trial/verifier/journal.json" >/dev/null
if rg -n 'world:8102|world:9000|/_bench/' "$surfaces/"*.json; then
  printf '%s\n' 'Owned backend infrastructure appeared in the selected native surface' >&2
  exit 1
fi
PYTHONDONTWRITEBYTECODE=1 "${FIRECRAWL_AUDIT_PYTHON:-python3}" deploy/audit-firecrawl-business.py \
  "$trial/verifier" > "$OUT/independent-audit.log" 2>&1
cat "$OUT/independent-audit.log"
printf '%s\n' 'Native initialization, 28 ordered definitions, handled API error, HTTPS and independent business audit passed.'
printf '%s\n' 'This is not model receipt, hosted conformance or an awareness experiment.'
