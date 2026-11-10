#!/bin/sh
set -eu
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
WORLD_IMAGE="${WORLD_IMAGE:?provide the Grafana35 candidate world image}"
TOOLS_IMAGE="${TOOLS_IMAGE:?provide the matching Grafana tools image}"
OUT="$(mktemp -d /tmp/tool-bench-grafana-surfaces.XXXXXX)"
printf 'artifacts: %s\n' "$OUT"
if [ -n "${TASKGEN_BINARY:-}" ]; then
  "$TASKGEN_BINARY" --out "$OUT/tasks" --world-image "$WORLD_IMAGE" --tools-image "$TOOLS_IMAGE" >/dev/null
else
  cargo run -q --locked -p scenario --bin taskgen -- --out "$OUT/tasks" \
    --world-image "$WORLD_IMAGE" --tools-image "$TOOLS_IMAGE" >/dev/null
fi
TASK="$OUT/tasks/ops__grafana__annotate-checkout-deploy"
cp "$TASK/solution/solve.sh" "$TASK/solution/reference-solve.sh"
for item in user settings missing; do
  case "$item" in
    user) endpoint=/api/user ;;
    settings) endpoint=/api/plugins/grafana-irm-app/settings ;;
    missing) endpoint=/api/not-a-real-resource ;;
  esac
  jq -n --arg endpoint "$endpoint" \
    '{jsonrpc:"2.0",id:2,method:"tools/call",params:{name:"grafana_api_request",arguments:{endpoint:$endpoint}}}' \
    > "$TASK/solution/surface-$item.json"
done
sed -e '/^bound_call [0-9]/,$d' -e '/^call [0-9]/,$d' \
  "$TASK/solution/reference-solve.sh" > "$TASK/solution/solve.sh"
cat >> "$TASK/solution/solve.sh" <<'PROGRAM'
sh "$DIR/reference-solve.sh"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
mkdir -p /logs/artifacts/surfaces
curl -fsS -D "$WORK/headers" -H 'content-type: application/json' \
  -H 'accept: application/json, text/event-stream' -d "$INIT" \
  http://tools:7114/mcp > /logs/artifacts/surfaces/initialize.raw
sid="$(tr -d '\r' < "$WORK/headers" | awk -F': ' 'tolower($1)=="mcp-session-id" {print $2}')"
jq -Rse --argjson expected_id 1 --arg method initialize -f "$DIR/response.jq" \
  /logs/artifacts/surfaces/initialize.raw > /logs/artifacts/surfaces/initialize.json
curl -fsS -o /dev/null -H 'content-type: application/json' -H "mcp-session-id: $sid" \
  -d '{"jsonrpc":"2.0","method":"notifications/initialized"}' http://tools:7114/mcp
curl -fsS -H 'content-type: application/json' -H 'accept: application/json, text/event-stream' \
  -H "mcp-session-id: $sid" -d '{"jsonrpc":"2.0","id":2,"method":"tools/list"}' \
  http://tools:7114/mcp > /logs/artifacts/surfaces/tools.raw
jq -Rse --arg method tools/list -f "$DIR/response.jq" \
  /logs/artifacts/surfaces/tools.raw > /logs/artifacts/surfaces/tools.json
for item in user settings missing; do
  curl -fsS -H 'content-type: application/json' -H 'accept: application/json, text/event-stream' \
    -H "mcp-session-id: $sid" -d @"$DIR/surface-$item.json" http://tools:7114/mcp \
    > /logs/artifacts/surfaces/"$item".raw
  jq -Rse -f "$DIR/response.jq" /logs/artifacts/surfaces/"$item".raw \
    > /logs/artifacts/surfaces/"$item".json
done
jq -e '.content[0].text|fromjson|.status==404' /logs/artifacts/surfaces/missing.json >/dev/null
printf '%s\n' GRAFANA_SURFACE_CAPTURE_COMPLETED
PROGRAM
cp -R "$TASK/solution" "$OUT/program"
sh deploy/run-trial.sh -p "$TASK" -a oracle -o "$OUT/jobs" > "$OUT/trial.log" 2>&1
reward="$(find "$OUT/jobs" -path '*/verifier/reward.json' -print -quit)"
test -n "$reward"
trial="$(dirname "$(dirname "$reward")")"
surfaces="$trial/artifacts/logs/artifacts/surfaces"
jq -e '.reward==1 and .evaluator_version==35 and .environment_ok==1 and .contract_ok==1
  and .integrity_ok==1 and .runner_admission_checked==1 and .runner_admission_ok==1
  and .preservation_checked==1 and .history_checked==1' "$reward" >/dev/null
jq -S '.tools|sort_by(.name)' "$surfaces/tools.json" > "$OUT/actual-tools.json"
jq -S '.tools|sort_by(.name)' servers/snapshots/grafana.json > "$OUT/expected-tools.json"
cmp "$OUT/actual-tools.json" "$OUT/expected-tools.json"
jq -e '.tools|length==79' "$surfaces/tools.json" >/dev/null
jq -e --slurpfile expected servers/snapshots/grafana.json '
  .serverInfo.name==$expected[0].server_info.name
    and .serverInfo.version==$expected[0].server_info.version' "$surfaces/initialize.json" >/dev/null
jq -e '.content[0].text|fromjson|.status==200 and .data.login=="sa-1-mcp-assistant"
  and .data.isServiceAccount==true' "$surfaces/user.json" >/dev/null
jq -e '.content[0].text|fromjson|.status==200
  and .data.jsonData.onCallApiUrl=="https://grafana.octo-corp.com/oncall"' \
  "$surfaces/settings.json" >/dev/null
jq -e '.entries|map(select(.service=="grafana"))|length>0
  and all(.[];.host=="grafana.octo-corp.com" and .scheme=="https")' \
  "$trial/verifier/journal.json" >/dev/null
if jq -e --arg endpoint 7114 --slurpfile request "$TASK/solution/surface-missing.json" \
    --slurpfile contracts "$TASK/solution/result-contracts.json" -f "$TASK/solution/tool-result.jq" \
    "$surfaces/missing.json" > "$OUT/unexpected-error-success.json" 2> "$OUT/expected-error-rejection.log"; then
  printf '%s\n' 'generic HTTP 404 incorrectly passed native result validation' >&2
  exit 1
fi
test ! -s "$OUT/unexpected-error-success.json"
for item in initialize tools user settings; do
  if grep -Eiq 'tool.?bench|/_bench/|world:8114|world:9000' "$surfaces/$item.json"; then
    printf 'unexpected infrastructure marker in %s\n' "$item" >&2
    exit 1
  fi
done
printf '%s\n' 'Grafana native initialization, 79 definitions, ordinary origin and HTTP-error envelope verified'
