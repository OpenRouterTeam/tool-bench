#!/bin/sh
set -eu
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
WORLD_IMAGE="${WORLD_IMAGE:?provide the revision-31 Vercel world image}"
TOOLS_IMAGE="${TOOLS_IMAGE:?provide the declared tools image}"
OUT="$(mktemp -d /tmp/tool-bench-vercel-surfaces.XXXXXX)"
printf 'artifacts: %s\n' "$OUT"
cargo run -q --locked -p scenario --bin taskgen -- --runner-admission required --out "$OUT/tasks" \
  --world-image "$WORLD_IMAGE" --tools-image "$TOOLS_IMAGE" >/dev/null
TASK="$OUT/tasks/code__vercel__enable-gift-cards-in-previews"
cp "$TASK/solution/solve.sh" "$TASK/solution/reference-solve.sh"
jq -n '{jsonrpc:"2.0",id:2,method:"tools/call",params:{
  name:"vercel-get-deployment",arguments:{idOrUrl:"acme-shop.com"}}}' \
  > "$TASK/solution/surface-deployment.json"
jq -n '{jsonrpc:"2.0",id:2,method:"tools/call",params:{
  name:"vercel-find-project",arguments:{idOrName:"storefront"}}}' \
  > "$TASK/solution/surface-project.json"
jq -n '{jsonrpc:"2.0",id:2,method:"tools/call",params:{
  name:"vercel-get-environments",arguments:{idOrName:"checkout-api-next"}}}' \
  > "$TASK/solution/surface-environment.json"
sed -e '/^bound_call [0-9]/,$d' -e '/^call [0-9]/,$d' \
  "$TASK/solution/reference-solve.sh" > "$TASK/solution/solve.sh"
cat >> "$TASK/solution/solve.sh" <<'PROGRAM'
sh "$DIR/reference-solve.sh"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
mkdir -p /logs/artifacts/surfaces
curl -fsS -D "$WORK/init-headers" -H 'content-type: application/json' \
  -H 'accept: application/json, text/event-stream' -d "$INIT" \
  http://tools:7110/mcp > /logs/artifacts/surfaces/initialize.raw
sid="$(tr -d '\r' < "$WORK/init-headers" | awk -F': ' 'tolower($1)=="mcp-session-id" {print $2}')"
jq -Rse --argjson expected_id 1 --arg method initialize -f "$DIR/response.jq" \
  /logs/artifacts/surfaces/initialize.raw > /logs/artifacts/surfaces/initialize.json
curl -fsS -o /dev/null -H 'content-type: application/json' -H "mcp-session-id: $sid" \
  -d '{"jsonrpc":"2.0","method":"notifications/initialized"}' http://tools:7110/mcp
curl -fsS -H 'content-type: application/json' -H 'accept: application/json, text/event-stream' \
  -H "mcp-session-id: $sid" -d '{"jsonrpc":"2.0","id":2,"method":"tools/list"}' \
  http://tools:7110/mcp > /logs/artifacts/surfaces/tools.raw
jq -Rse --arg method tools/list -f "$DIR/response.jq" \
  /logs/artifacts/surfaces/tools.raw > /logs/artifacts/surfaces/tools.json
for item in deployment project environment; do
  call 7110 "$DIR/surface-$item.json" > /logs/artifacts/surfaces/"$item".raw
  jq -Rse -f "$DIR/response.jq" /logs/artifacts/surfaces/"$item".raw \
    > /logs/artifacts/surfaces/"$item".json
done
PROGRAM
cp -R "$TASK/solution" "$OUT/program"
sh deploy/run-trial.sh -p "$TASK" -a oracle -o "$OUT/jobs" > "$OUT/trial.log" 2>&1
reward="$(find "$OUT/jobs" -path '*/verifier/reward.json' -print -quit)"
test -n "$reward"
trial="$(dirname "$(dirname "$reward")")"
surfaces="$trial/artifacts/logs/artifacts/surfaces"
jq -e '.reward==1 and .evaluator_version==31 and .environment_ok==1 and .contract_ok==1
  and .integrity_ok==1 and .runner_admission_checked==1 and .runner_admission_ok==1
  and .preservation_checked==1 and .history_checked==1' "$reward" >/dev/null
jq -S '.tools|sort_by(.name)' "$surfaces/tools.json" > "$OUT/actual-tools.json"
jq -S '.tools|sort_by(.name)' servers/snapshots/vercel.json > "$OUT/expected-tools.json"
cmp "$OUT/actual-tools.json" "$OUT/expected-tools.json"
protocol="$(sed -n "s/^INIT='\\(.*\\)'$/\\1/p" "$OUT/program/solve.sh" | jq -er '.params.protocolVersion')"
jq -e --arg protocol "$protocol" --slurpfile expected servers/snapshots/vercel.json '
  .serverInfo.name==$expected[0].server_info.name
    and .serverInfo.version==$expected[0].server_info.version
    and .protocolVersion==$protocol' "$surfaces/initialize.json" >/dev/null
jq -er '.content[0].text|fromjson|.projectId=="prj_3dw2VoUa166qmaLV3P6dm828isAX"' \
  "$surfaces/deployment.json" >/dev/null
jq -er '.content[1].text|fromjson|.name=="storefront" and .targets.production.readyState=="READY"' \
  "$surfaces/project.json" >/dev/null
jq -er '.content[1].text|fromjson|any(.[];.key=="STRIPE_SECRET_KEY" and .type=="sensitive" and (has("value")|not))' \
  "$surfaces/environment.json" >/dev/null
for item in initialize tools deployment project environment; do
  if grep -Eiq 'tool.?bench|/_bench/|world:8110|world:9000' "$surfaces/$item.json"; then
    printf '%s\n' "$item: explicit benchmark/internal-origin marker in captured response" >&2
    exit 1
  fi
done
printf '%s\n' 'Replica initialization, fifteen definitions, live-domain reads and secret redaction match the declared surface.'
printf '%s\n' 'This bounded response audit is not a model-awareness experiment or a complete native-session audit.'
