#!/bin/sh
set -eu
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
WORLD_IMAGE="${WORLD_IMAGE:?provide the source-qualified Sentry world image}"
TOOLS_IMAGE="${TOOLS_IMAGE:?provide the source-qualified Sentry tools image}"
SCENARIOS="${SCENARIOS:-$ROOT/scenarios}"
PUBLIC_ORIGIN="${PUBLIC_ORIGIN:-https://sentry.lumenlabs.dev}"
INGESTION_ORIGIN="${INGESTION_ORIGIN:-$PUBLIC_ORIGIN}"
OUT="$(mktemp -d /tmp/tool-bench-sentry-surfaces.XXXXXX)"
printf 'artifacts: %s\n' "$OUT"
cargo run -q --locked -p scenario --bin taskgen -- --runner-admission required --scenarios "$SCENARIOS" --out "$OUT/tasks" \
  --world-image "$WORLD_IMAGE" --tools-image "$TOOLS_IMAGE" --inventory "$OUT/source-inventory.json" >/dev/null
TASK="$OUT/tasks/ops__sentry__create-notification-project"
cp "$TASK/solution/solve.sh" "$TASK/solution/reference-solve.sh"
sed -e '/^bound_call [0-9]/,$d' -e '/^call [0-9]/,$d' \
  "$TASK/solution/reference-solve.sh" > "$TASK/solution/solve.sh"
for item in identity organizations projects keys; do
  case "$item" in
    identity) tool=execute_sentry_tool; arguments='{"name":"whoami","arguments":{}}' ;;
    organizations) tool=find_organizations; arguments='{}' ;;
    projects) tool=find_projects; arguments='{"organizationSlug":"lumen-labs"}' ;;
    keys) tool=execute_sentry_tool; arguments='{"name":"find_dsns","arguments":{"organizationSlug":"lumen-labs","projectSlug":"notification-service"}}' ;;
  esac
  jq -n --arg tool "$tool" --argjson arguments "$arguments" \
    '{jsonrpc:"2.0",id:2,method:"tools/call",params:{name:$tool,arguments:$arguments}}' \
    > "$TASK/solution/surface-$item.json"
done
cat >> "$TASK/solution/solve.sh" <<'PROGRAM'
sh "$DIR/reference-solve.sh"
mkdir -p /logs/artifacts/surfaces
curl -fsS -D "$WORK/init-headers" -H 'content-type: application/json' \
  -H 'accept: application/json, text/event-stream' -d "$INIT" \
  http://tools:7113/mcp > /logs/artifacts/surfaces/initialize.raw
sid="$(tr -d '\r' < "$WORK/init-headers" | awk -F': ' 'tolower($1)=="mcp-session-id" {print $2}')"
cp "$WORK/init-headers" /logs/artifacts/surfaces/initialize.headers
jq -Rse --argjson expected_id 1 --arg method initialize -f "$DIR/response.jq" \
  /logs/artifacts/surfaces/initialize.raw > /logs/artifacts/surfaces/initialize.json
curl -fsS -o /dev/null -H 'content-type: application/json' -H "mcp-session-id: $sid" \
  -d '{"jsonrpc":"2.0","method":"notifications/initialized"}' http://tools:7113/mcp
curl -fsS -H 'content-type: application/json' -H 'accept: application/json, text/event-stream' \
  -H "mcp-session-id: $sid" -d '{"jsonrpc":"2.0","id":2,"method":"tools/list"}' \
  http://tools:7113/mcp > /logs/artifacts/surfaces/tools.raw
jq -Rse --arg method tools/list -f "$DIR/response.jq" \
  /logs/artifacts/surfaces/tools.raw > /logs/artifacts/surfaces/tools.json
for item in identity organizations projects keys; do
  call 7113 "$DIR/surface-$item.json" > "$WORK/response.json"
  cp "$WORK/response.json" "/logs/artifacts/surfaces/$item.raw"
  jq -Rse -f "$DIR/response.jq" "$WORK/response.json" > "/logs/artifacts/surfaces/$item.json"
done
PROGRAM
cp -R "$TASK/solution" "$OUT/program"
sh deploy/run-trial.sh -p "$TASK" -a oracle -o "$OUT/jobs" > "$OUT/trial.log" 2>&1
reward="$(find "$OUT/jobs" -path '*/verifier/reward.json' -print -quit)"
test -n "$reward"
trial="$(dirname "$(dirname "$reward")")"
surfaces="$trial/artifacts/logs/artifacts/surfaces"
jq -e '.reward==1 and .evaluator_version==30 and .environment_ok==1 and .contract_ok==1
  and .integrity_ok==1 and .runner_admission_checked==1 and .runner_admission_ok==1
  and .preservation_checked==1 and .history_checked==1' "$reward" >/dev/null
jq -S '.tools|sort_by(.name)' "$surfaces/tools.json" > "$OUT/actual-tools.json"
jq -S '.tools|sort_by(.name)' servers/snapshots/sentry.json > "$OUT/expected-tools.json"
cmp "$OUT/actual-tools.json" "$OUT/expected-tools.json"
jq -e --slurpfile expected servers/snapshots/sentry.json '
  .serverInfo.name==$expected[0].server_info.name
    and .serverInfo.version==$expected[0].server_info.version
    and .protocolVersion==$expected[0].protocol_version' "$surfaces/initialize.json" >/dev/null
jq -e '[.content[]?|select(.type=="text")|.text]
  | any(.[];contains("priya@lumenlabs.dev"))' "$surfaces/identity.json" >/dev/null
jq -e --arg ingestion "$INGESTION_ORIGIN" '.structuredContent.dsns|length>0
  and all(.[];.dsn|startswith("https://") and contains("@"+($ingestion|sub("^https://";""))+"/"))' \
  "$surfaces/keys.json" >/dev/null
for item in organizations projects; do
  jq -e '[.content[]?|select(.type=="text")|.text] as $texts
    | all($texts[];(contains("world:8113")|not))' "$surfaces/$item.json" >/dev/null
done
jq -e --arg origin "$PUBLIC_ORIGIN" '.structuredContent.organizations|length>0
  and all(.[];.webUrl==$origin and .regionUrl==null)' \
  "$surfaces/organizations.json" >/dev/null
jq -e '.structuredContent.projects|length>0
  and all(.[];.slug|type=="string")
  and any(.[];.slug=="notification-service")' "$surfaces/projects.json" >/dev/null
jq -e --arg host "${PUBLIC_ORIGIN#https://}" \
  '[.events[]|select(.kind=="request_head_received" and .operation.service=="sentry")] as $heads
  | ($heads|length)>0 and all($heads[];.head.scheme=="https" and .head.host==$host)' \
  "$trial/artifacts/world/evidence.json" >/dev/null
jq -e '.images.tools.connector_env_sha256==.context.connector_env_sha256
  and .images.tools.connector_env_sha256.SENTRY_URL!=null' "$trial/realized-manifest.json" >/dev/null
docker run --rm --network none --entrypoint /opt/bin/catalog "$TOOLS_IMAGE" \
  --file /etc/tool-bench/services.toml launch sentry > "$OUT/declared-launch.json"
jq -e '.args==["--all-skills"] and .env.SENTRY_URL=="https://sentry.lumenlabs.dev/"
  and ([.env.OPENAI_API_KEY,.env.ANTHROPIC_API_KEY,.env.OPENROUTER_API_KEY,
        .env.EMBEDDED_AGENT_PROVIDER,.env.SENTRY_DSN,.env.DEFAULT_SENTRY_DSN]|all(.==""))' \
  "$OUT/declared-launch.json" >/dev/null
printf '%s\n' 'Sentry native initialization, definitions, identity, URLs, HTTPS requests and declared no-auxiliary-model launch checked'
printf '%s\n' 'This surface audit is not a resolved native-prompt audit or proof of model unawareness.'
