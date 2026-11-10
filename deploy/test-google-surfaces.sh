#!/bin/sh
set -eu
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
WORLD_IMAGE="${WORLD_IMAGE:?provide the declared Google40B world image}"
TOOLS_IMAGE="${TOOLS_IMAGE:?provide the declared tools image}"
OUT="${CONTROL_OUT:-$(mktemp -d /tmp/tool-bench-google-surfaces.XXXXXXXX)}"
test -d "$OUT"
test -z "$(find "$OUT" -mindepth 1 -maxdepth 1 -print -quit)"
printf 'artifacts: %s\n' "$OUT"
if [ -n "${TASKGEN_BINARY:-}" ]; then
  "$TASKGEN_BINARY" --out "$OUT/tasks" --world-image "$WORLD_IMAGE" --tools-image "$TOOLS_IMAGE" >/dev/null
else
  cargo run -q --locked -p scenario --bin taskgen -- --out "$OUT/tasks" \
    --world-image "$WORLD_IMAGE" --tools-image "$TOOLS_IMAGE" >/dev/null
fi
TASK="$OUT/tasks/collab__google__schedule-q1-planning"
cp "$TASK/solution/solve.sh" "$TASK/solution/reference-solve.sh"
for item in read bare-error rpc-error; do
  case "$item" in
    read) tool=get_drive_file_permissions; arguments='{"file_id":"150dluRbaiTmEP9Spu-wAk0p_lQupubYY"}' ;;
    bare-error) tool=get_drive_file_permissions; arguments='{"file_id":"missing-surface-probe"}' ;;
    rpc-error) tool=unknown_surface_probe; arguments='{}' ;;
  esac
  jq -n --arg tool "$tool" --argjson arguments "$arguments" \
    '{jsonrpc:"2.0",id:2,method:"tools/call",params:{name:$tool,arguments:$arguments}}' \
    > "$TASK/solution/surface-$item.json"
done
sed -e '/^bound_call [0-9]/,$d' -e '/^call [0-9]/,$d' \
  "$OUT/tasks/ops__grafana__silence-replica-maintenance/solution/solve.sh" > "$TASK/solution/solve.sh"
cp "$OUT/tasks/ops__grafana__silence-replica-maintenance/solution/"*.jq "$TASK/solution/"
cat >> "$TASK/solution/solve.sh" <<'PROGRAM'
mkdir -p /logs/artifacts/surfaces
SURFACES=/logs/artifacts/surfaces
sh "$DIR/reference-solve.sh" > "$SURFACES/reference.raw"
cat "$SURFACES/reference.raw"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
jq -sre '
  [.[] | .result.structuredContent.result? | select(type=="string")
    | select(startswith("Successfully created event "))]
  | if length!=1 then error("one actual creation confirmation required") else .[0] end
  | capture("Link: https://www[.]google[.]com/calendar/event[?]eid=(?<encoded>[^\\r\\n]+)").encoded
  | @base64d | split(" ")
  | if length!=2 or .[1]!="ops@octo-corp.example" or .[0]==""
    then error("actual created identity/calendar not established") else .[0] end
' "$SURFACES/reference.raw" > "$SURFACES/created-event-id.txt"
jq -n --rawfile identity "$SURFACES/created-event-id.txt" '
  {jsonrpc:"2.0",id:2,method:"tools/call",params:{name:"manage_event",
    arguments:{action:"update",event_id:($identity|rtrimstr("\n")),
      end_time:"2026-11-03T18:00:00Z",add_google_meet:true}}}
' > "$SURFACES/event-error-request.json"
curl -fsS -D "$WORK/headers" -H 'content-type: application/json' \
  -H 'accept: application/json, text/event-stream' -d "$INIT" \
  http://tools:7130/mcp > "$SURFACES/initialize.raw"
sid="$(tr -d '\r' < "$WORK/headers" | awk -F': ' 'tolower($1)=="mcp-session-id" {print $2}')"
test -n "$sid"
jq -Rse --argjson expected_id 1 --arg method initialize -f "$DIR/response.jq" \
  "$SURFACES/initialize.raw" > "$SURFACES/initialize.json"
curl -fsS -o /dev/null -H 'content-type: application/json' -H "mcp-session-id: $sid" \
  -d '{"jsonrpc":"2.0","method":"notifications/initialized"}' http://tools:7130/mcp
curl -fsS -H 'content-type: application/json' -H 'accept: application/json, text/event-stream' \
  -H "mcp-session-id: $sid" -d '{"jsonrpc":"2.0","id":2,"method":"tools/list"}' \
  http://tools:7130/mcp > "$SURFACES/tools.raw"
jq -Rse --arg method tools/list -f "$DIR/response.jq" \
  "$SURFACES/tools.raw" > "$SURFACES/tools.json"
for item in read bare-error rpc-error event-error; do
  request="$DIR/surface-$item.json"
  [ "$item" != event-error ] || request="$SURFACES/event-error-request.json"
  curl -fsS -H 'content-type: application/json' -H 'accept: application/json, text/event-stream' \
    -H "mcp-session-id: $sid" -d @"$request" http://tools:7130/mcp > "$SURFACES/$item.raw"
done
jq -Rse -f "$DIR/response.jq" "$SURFACES/read.raw" > "$SURFACES/read.json"
jq -e -L "$DIR" --arg endpoint 7130 --slurpfile request "$DIR/surface-read.json" \
  --slurpfile contracts "$DIR/result-contracts.json" -f "$DIR/tool-result.jq" "$SURFACES/read.json" >/dev/null
jq -Rse -f "$DIR/response.jq" "$SURFACES/bare-error.raw" > "$SURFACES/bare-error.json"
jq -e '.isError==false and (.content|length)==1
  and (.content[0].text|startswith("Error getting file permissions: "))
  and .structuredContent.result==.content[0].text' "$SURFACES/bare-error.json" >/dev/null
if jq -e -L "$DIR" --arg endpoint 7130 --slurpfile request "$DIR/surface-bare-error.json" \
    --slurpfile contracts "$DIR/result-contracts.json" -f "$DIR/tool-result.jq" \
    "$SURFACES/bare-error.json" > "$WORK/unexpected-success" 2> "$SURFACES/bare-error-rejected.log"; then
  printf '%s\n' 'An error string in a non-error MCP envelope passed result validation' >&2
  exit 1
fi
test ! -s "$WORK/unexpected-success"
for item in rpc-error event-error; do
  if jq -Rse -f "$DIR/response.jq" "$SURFACES/$item.raw" \
      > "$WORK/unexpected-success" 2> "$SURFACES/$item-rejected.log"; then
    printf '%s\n' 'The common response guard admitted a failed operation' >&2
    exit 1
  fi
  test ! -s "$WORK/unexpected-success"
done
jq -e '.error.code == -32602 and .result==null' "$SURFACES/rpc-error.raw" >/dev/null
jq -e '.result.isError==true and (.result.content[0].text|contains("time range is empty"))' \
  "$SURFACES/event-error.raw" >/dev/null
printf '%s\n' GOOGLE_REPLICA_SURFACE_CAPTURE_COMPLETED
PROGRAM
sh -n "$TASK/solution/solve.sh"
cp -R "$TASK/solution" "$OUT/program-calendar-surfaces"
printf '%s\n' 'Selected Rust replica MCP transport; not execution of upstream Python Workspace MCP, Google SDK or hosted Google.' > "$OUT/scope.txt"
if [ "${GENERATE_ONLY:-0}" = 1 ]; then
  printf '%s\n' 'surface program generated; no runtime validation claimed'
  exit 0
fi
sh deploy/run-trial.sh -p "$TASK" -a oracle -o "$OUT/capture" > "$OUT/capture.log" 2>&1
reward="$(find "$OUT/capture" -path '*/verifier/reward.json' -print -quit)"
test -n "$reward"
trial="$(dirname "$(dirname "$reward")")"
surfaces="$trial/artifacts/logs/artifacts/surfaces"
jq -e '.reward==1 and .evaluator_version==40 and .contract_version==2
  and .environment_ok==1 and .contract_ok==1 and .integrity_ok==1
  and .runner_admission_checked==1 and .runner_admission_ok==1
  and .preservation_checked==1 and .preservation_ok==1 and .history_checked==1 and .history_ok==1
  and .adjudication_needed==0' "$reward" >/dev/null
jq -S '.tools|sort_by(.name)' "$surfaces/tools.json" > "$OUT/actual-tools.json"
jq -S '.tools|sort_by(.name)' servers/snapshots/google-workspace.json > "$OUT/expected-tools.json"
cmp "$OUT/actual-tools.json" "$OUT/expected-tools.json"
jq -e '.tools|length==119' "$surfaces/tools.json" >/dev/null
jq -e --slurpfile expected servers/snapshots/google-workspace.json '
  .serverInfo.name==$expected[0].server_info.name
    and .serverInfo.version==$expected[0].server_info.version' "$surfaces/initialize.json" >/dev/null
PYTHONDONTWRITEBYTECODE=1 "${GOOGLE_AUDIT_PYTHON:-python3}" deploy/audit-google-business.py "$trial/verifier"
for item in initialize tools read bare-error rpc-error event-error; do
  if rg -iq 'tool.?bench|/_bench/|world:8130|world:9000|/opt/tool-bench' "$surfaces/$item.raw"; then
    printf '%s\n' 'Selected MCP surface contains an internal benchmark/origin marker; preserve authored/vendor text and review' >&2
    exit 1
  fi
done
printf '%s\n' 'Google selected-replica surface capture passed: 119 locked definitions, source-bound response/error forms and independent business/history agreement.'
