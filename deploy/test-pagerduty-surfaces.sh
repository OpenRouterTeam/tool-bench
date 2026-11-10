#!/bin/sh
set -eu
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
WORLD_IMAGE="${WORLD_IMAGE:?provide the PagerDuty37 world candidate}"
TOOLS_IMAGE="${TOOLS_IMAGE:?provide the matching tools candidate}"
OUT="$(mktemp -d /tmp/tool-bench-pagerduty-surfaces.XXXXXX)"
printf 'artifacts: %s\n' "$OUT"
if [ -n "${TASKGEN_BINARY:-}" ]; then
  "$TASKGEN_BINARY" --out "$OUT/tasks" --world-image "$WORLD_IMAGE" --tools-image "$TOOLS_IMAGE" >/dev/null
else
  cargo run -q --locked -p scenario --bin taskgen -- --runner-admission required --out "$OUT/tasks" \
    --world-image "$WORLD_IMAGE" --tools-image "$TOOLS_IMAGE" >/dev/null
fi
for slug in cover-shift-override resolve-prod-checkout; do
  TASK="$OUT/tasks/ops__pagerduty__$slug"
  cp "$TASK/solution/solve.sh" "$TASK/solution/reference-solve.sh"
  jq -n '{jsonrpc:"2.0",id:2,method:"tools/call",params:{name:"get_user_data",arguments:{}}}' \
    > "$TASK/solution/surface-user.json"
  jq -n '{jsonrpc:"2.0",id:2,method:"tools/call",params:{
    name:"create_schedule_override",arguments:{
      schedule_id:"PSPLTPR",override_request:{overrides:[{
        start:"2026-10-31T09:00:00Z",end:"2026-10-31T17:00:00Z",user_id:"not-a-real-user"
      }]}}}}' > "$TASK/solution/surface-error.json"
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
  http://tools:7111/mcp > /logs/artifacts/surfaces/initialize.raw
sid="$(tr -d '\r' < "$WORK/headers" | awk -F': ' 'tolower($1)=="mcp-session-id" {print $2}')"
jq -Rse --argjson expected_id 1 --arg method initialize -f "$DIR/response.jq" \
  /logs/artifacts/surfaces/initialize.raw > /logs/artifacts/surfaces/initialize.json
curl -fsS -o /dev/null -H 'content-type: application/json' -H "mcp-session-id: $sid" \
  -d '{"jsonrpc":"2.0","method":"notifications/initialized"}' http://tools:7111/mcp
curl -fsS -H 'content-type: application/json' -H 'accept: application/json, text/event-stream' \
  -H "mcp-session-id: $sid" -d '{"jsonrpc":"2.0","id":2,"method":"tools/list"}' \
  http://tools:7111/mcp > /logs/artifacts/surfaces/tools.raw
jq -Rse --arg method tools/list -f "$DIR/response.jq" \
  /logs/artifacts/surfaces/tools.raw > /logs/artifacts/surfaces/tools.json
for item in user error; do
  curl -fsS -H 'content-type: application/json' -H 'accept: application/json, text/event-stream' \
    -H "mcp-session-id: $sid" -d @"$DIR/surface-$item.json" http://tools:7111/mcp \
    > /logs/artifacts/surfaces/"$item".raw
  jq -Rse -f "$DIR/response.jq" /logs/artifacts/surfaces/"$item".raw \
    > /logs/artifacts/surfaces/"$item".json
done
jq -e '.isError!=true and (.structuredContent.result|length)==1
  and .structuredContent.result[0].status==400
  and (.structuredContent.result[0].errors|length)>0' /logs/artifacts/surfaces/error.json >/dev/null
if jq -e --arg endpoint 7111 --slurpfile request "$DIR/surface-error.json" \
    --slurpfile contracts "$DIR/result-contracts.json" -f "$DIR/tool-result.jq" \
    /logs/artifacts/surfaces/error.json > "$WORK/unexpected-success" 2> /logs/artifacts/surfaces/expected-rejection.log; then
  printf '%s\n' 'Nested failed override incorrectly passed native result validation' >&2
  exit 1
fi
test ! -s "$WORK/unexpected-success"
printf '%s\n' PAGERDUTY_SURFACE_CAPTURE_COMPLETED
PROGRAM
  sh -n "$TASK/solution/solve.sh"
  cp -R "$TASK/solution" "$OUT/program-$slug"
  sh deploy/run-trial.sh -p "$TASK" -a oracle -o "$OUT/$slug" > "$OUT/$slug.log" 2>&1
  reward="$(find "$OUT/$slug" -path '*/verifier/reward.json' -print -quit)"
  test -n "$reward"
  trial="$(dirname "$(dirname "$reward")")"
  surfaces="$trial/artifacts/logs/artifacts/surfaces"
  jq -e '.reward==1 and .evaluator_version==37 and .contract_version==2
    and .environment_ok==1 and .contract_ok==1 and .integrity_ok==1
    and .runner_admission_checked==1 and .runner_admission_ok==1
    and .preservation_checked==1 and .history_checked==1 and .adjudication_needed==0' "$reward" >/dev/null
  jq -S '.tools|sort_by(.name)' "$surfaces/tools.json" > "$OUT/$slug-actual-tools.json"
  jq -S '.tools|sort_by(.name)' servers/snapshots/pagerduty.json > "$OUT/$slug-expected-tools.json"
  cmp "$OUT/$slug-actual-tools.json" "$OUT/$slug-expected-tools.json"
  jq -e '.tools|length==101' "$surfaces/tools.json" >/dev/null
  jq -e --slurpfile expected servers/snapshots/pagerduty.json '
    .serverInfo.name==$expected[0].server_info.name
      and .serverInfo.version==$expected[0].server_info.version' "$surfaces/initialize.json" >/dev/null
  case "$slug" in
    cover-shift-override) identity=PA1EXKM; email=alex.kim@tidewater.io ;;
    resolve-prod-checkout) identity=PLWEBR5; email=lena.weber@tidewater.io ;;
  esac
  jq -e --arg identity "$identity" --arg email "$email" \
    '.structuredContent.id==$identity and .structuredContent.email==$email
      and (.content[0].text|fromjson)==.structuredContent' "$surfaces/user.json" >/dev/null
  jq -e '.entries|map(select(.service=="pagerduty"))|length>0
    and all(.[];.host=="api.pagerduty.com" and .scheme=="https")' "$trial/verifier/journal.json" >/dev/null
  for item in initialize tools user; do
    if rg -iq 'tool.?bench|/_bench/|world:8111|world:9000' "$surfaces/$item.json"; then
      printf 'Unexpected owned infrastructure marker in %s\n' "$item" >&2
      exit 1
    fi
  done
  printf '%s\n' "$slug: native initialization, 101 definitions, account actor, HTTPS and nested failure verified"
done
printf '%s\n' 'This bounded capture is not hosted certification, resolved native prompt auditing or an awareness experiment.'
