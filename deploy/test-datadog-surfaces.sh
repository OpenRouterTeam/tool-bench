#!/bin/sh
set -eu
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
WORLD_IMAGE="${WORLD_IMAGE:?provide the Datadog38 world candidate}"
TOOLS_IMAGE="${TOOLS_IMAGE:?provide the matching tools candidate}"
OUT="${CONTROL_OUT:-$(mktemp -d /tmp/tool-bench-datadog-surfaces.XXXXXX)}"
test -d "$OUT"
test -z "$(find "$OUT" -mindepth 1 -maxdepth 1 -print -quit)"
printf 'artifacts: %s\n' "$OUT"
if [ -n "${TASKGEN_BINARY:-}" ]; then
  "$TASKGEN_BINARY" --out "$OUT/tasks" --world-image "$WORLD_IMAGE" --tools-image "$TOOLS_IMAGE" >/dev/null
else
  cargo run -q --locked -p scenario --bin taskgen -- --runner-admission required --out "$OUT/tasks" \
    --world-image "$WORLD_IMAGE" --tools-image "$TOOLS_IMAGE" >/dev/null
fi
TASK="$OUT/tasks/ops__datadog__schedule-replica-downtime"
cp "$TASK/solution/solve.sh" "$TASK/solution/reference-solve.sh"
jq -n '{jsonrpc:"2.0",id:2,method:"tools/call",params:{name:"list_downtimes",arguments:{}}}' \
  > "$TASK/solution/surface-read.json"
jq -n '{jsonrpc:"2.0",id:2,method:"tools/call",params:{name:"get_logs",
  arguments:{query:"\"",from:1793365200,to:1793370600,limit:1}}}' \
  > "$TASK/solution/surface-error.json"
sed -e '/^bound_call [0-9]/,$d' -e '/^call [0-9]/,$d' \
  "$OUT/tasks/ops__grafana__silence-replica-maintenance/solution/solve.sh" > "$TASK/solution/solve.sh"
cp "$OUT/tasks/ops__grafana__silence-replica-maintenance/solution/"*.jq "$TASK/solution/"
cat >> "$TASK/solution/solve.sh" <<'PROGRAM'
sh "$DIR/reference-solve.sh"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
mkdir -p /logs/artifacts/surfaces
curl -fsS -D "$WORK/headers" -H 'content-type: application/json' \
  -H 'accept: application/json, text/event-stream' -d "$INIT" \
  http://tools:7112/mcp > /logs/artifacts/surfaces/initialize.raw
sid="$(tr -d '\r' < "$WORK/headers" | awk -F': ' 'tolower($1)=="mcp-session-id" {print $2}')"
jq -Rse --argjson expected_id 1 --arg method initialize -f "$DIR/response.jq" \
  /logs/artifacts/surfaces/initialize.raw > /logs/artifacts/surfaces/initialize.json
curl -fsS -o /dev/null -H 'content-type: application/json' -H "mcp-session-id: $sid" \
  -d '{"jsonrpc":"2.0","method":"notifications/initialized"}' http://tools:7112/mcp
curl -fsS -H 'content-type: application/json' -H 'accept: application/json, text/event-stream' \
  -H "mcp-session-id: $sid" -d '{"jsonrpc":"2.0","id":2,"method":"tools/list"}' \
  http://tools:7112/mcp > /logs/artifacts/surfaces/tools.raw
jq -Rse --arg method tools/list -f "$DIR/response.jq" \
  /logs/artifacts/surfaces/tools.raw > /logs/artifacts/surfaces/tools.json
for item in read error; do
  curl -fsS -H 'content-type: application/json' -H 'accept: application/json, text/event-stream' \
    -H "mcp-session-id: $sid" -d @"$DIR/surface-$item.json" http://tools:7112/mcp \
    > /logs/artifacts/surfaces/"$item".raw
done
jq -Rse -f "$DIR/response.jq" /logs/artifacts/surfaces/read.raw \
  > /logs/artifacts/surfaces/read.json
jq -e '.content|length==1 and .[0].type=="text"
  and (.[0].text|startswith("Listed downtimes:\n"))' \
  /logs/artifacts/surfaces/read.json >/dev/null
if jq -Rse -f "$DIR/response.jq" /logs/artifacts/surfaces/error.raw \
    > "$WORK/unexpected-success" 2> /logs/artifacts/surfaces/expected-rejection.log; then
  printf '%s\n' 'Native read failure incorrectly passed the common response guard' >&2
  exit 1
fi
test ! -s "$WORK/unexpected-success"
jq -Rse '
  . as $raw | (try fromjson catch null) as $direct
  | (if ($direct|type)=="object" then [$direct]
     else $raw|split("\n")|map(select(startswith("data:"))|sub("^data: ?";"")|fromjson) end)
  | map(select(type=="object" and .jsonrpc=="2.0" and .id==2))
  | length==1 and .[0].result==null and .[0].error.code==400
    and (.[0].error.message|type)=="string" and (.[0].error.message|length)>0
' /logs/artifacts/surfaces/error.raw >/dev/null
printf '%s\n' DATADOG_SURFACE_CAPTURE_COMPLETED
PROGRAM
sh -n "$TASK/solution/solve.sh"
cp -R "$TASK/solution" "$OUT/program-schedule-replica-downtime"
if [ "${GENERATE_ONLY:-0}" = 1 ]; then
  printf '%s\n' 'surface program generated; no runtime validation claimed'
  exit 0
fi
sh deploy/run-trial.sh -p "$TASK" -a oracle -o "$OUT/capture" > "$OUT/capture.log" 2>&1
reward="$(find "$OUT/capture" -path '*/verifier/reward.json' -print -quit)"
test -n "$reward"
trial="$(dirname "$(dirname "$reward")")"
surfaces="$trial/artifacts/logs/artifacts/surfaces"
jq -e '.reward==1 and .evaluator_version==38 and .contract_version==2
  and .environment_ok==1 and .contract_ok==1 and .integrity_ok==1
  and .runner_admission_checked==1 and .runner_admission_ok==1
  and .preservation_checked==1 and .history_checked==1 and .history_ok==1
  and .adjudication_needed==0' "$reward" >/dev/null
jq -S '.tools|sort_by(.name)' "$surfaces/tools.json" > "$OUT/actual-tools.json"
jq -S '.tools|sort_by(.name)' servers/snapshots/datadog.json > "$OUT/expected-tools.json"
cmp "$OUT/actual-tools.json" "$OUT/expected-tools.json"
jq -e '.tools|length==21' "$surfaces/tools.json" >/dev/null
jq -e --slurpfile expected servers/snapshots/datadog.json '
  .serverInfo.name==$expected[0].server_info.name
    and .serverInfo.version==$expected[0].server_info.version' "$surfaces/initialize.json" >/dev/null
jq -e '.content[0].text|ltrimstr("Listed downtimes:\n")|fromjson
  | map(select(.monitorId==7301 and .start==1793412000 and .end==1793419200))
  | length==1 and .[0].creatorId==1445416' "$surfaces/read.json" >/dev/null
jq -e '.entries|map(select(.service=="datadog"))|length>0
  and all(.[];.host=="api.datadoghq.com" and .scheme=="https")' "$trial/verifier/journal.json" >/dev/null
for item in initialize tools read; do
  if rg -iq 'tool.?bench|/_bench/|world:8112|world:9000' "$surfaces/$item.json"; then
    printf 'Unexpected owned infrastructure marker in %s\n' "$item" >&2
    exit 1
  fi
done
printf '%s\n' 'native initialization, 21 definitions, local actor convention, HTTPS and RPC error verified'
printf '%s\n' 'This is not hosted identity, native-model prompt admission or an awareness experiment.'
