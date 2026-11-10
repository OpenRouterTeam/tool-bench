#!/bin/sh
set -eu
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
WORLD_IMAGE="${WORLD_IMAGE:?provide the revision-33 Okta world image}"
TOOLS_IMAGE="${TOOLS_IMAGE:?provide the declared Okta tools image}"
OUT="$(mktemp -d /tmp/tool-bench-okta-surfaces.XXXXXX)"
printf 'artifacts: %s\n' "$OUT"
cargo run -q --locked -p scenario --bin taskgen -- --out "$OUT/tasks" \
  --world-image "$WORLD_IMAGE" --tools-image "$TOOLS_IMAGE" >/dev/null
TASK="$OUT/tasks/cloud__okta__transfer-to-platform-team"
cp "$TASK/solution/solve.sh" "$TASK/solution/reference-solve.sh"
for item in user brands missing; do
  case "$item" in
    user) identifier=3; tool=get_user; arguments='{"user_id":"00u1mrgnzlz6Wx3Yz4x7"}' ;;
    brands) identifier=4; tool=list_brands; arguments='{}' ;;
    missing) identifier=5; tool=get_user; arguments='{"user_id":"not-a-real-user"}' ;;
  esac
  jq -n --argjson identifier "$identifier" --arg tool "$tool" --argjson arguments "$arguments" \
    '{jsonrpc:"2.0",id:$identifier,method:"tools/call",params:{name:$tool,arguments:$arguments}}' \
    > "$TASK/solution/surface-$item.json"
done
sed -e '/^bound_call [0-9]/,$d' -e '/^call [0-9]/,$d' \
  "$TASK/solution/reference-solve.sh" > "$TASK/solution/solve.sh"
cat >> "$TASK/solution/solve.sh" <<'PROGRAM'
sh "$DIR/reference-solve.sh"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
mkdir -p /logs/artifacts/surfaces
curl -fsS -D "$WORK/init-headers" -H 'content-type: application/json' \
  -H 'accept: application/json, text/event-stream' -d "$INIT" \
  http://tools:7120/mcp > /logs/artifacts/surfaces/initialize.raw
sid="$(tr -d '\r' < "$WORK/init-headers" | awk -F': ' 'tolower($1)=="mcp-session-id" {print $2}')"
jq -Rse --argjson expected_id 1 --arg method initialize -f "$DIR/response.jq" \
  /logs/artifacts/surfaces/initialize.raw > /logs/artifacts/surfaces/initialize.json
curl -fsS -o /dev/null -H 'content-type: application/json' -H "mcp-session-id: $sid" \
  -d '{"jsonrpc":"2.0","method":"notifications/initialized"}' http://tools:7120/mcp
curl -fsS -H 'content-type: application/json' -H 'accept: application/json, text/event-stream' \
  -H "mcp-session-id: $sid" -d '{"jsonrpc":"2.0","id":2,"method":"tools/list"}' \
  http://tools:7120/mcp > /logs/artifacts/surfaces/tools.raw
jq -Rse --arg method tools/list -f "$DIR/response.jq" \
  /logs/artifacts/surfaces/tools.raw > /logs/artifacts/surfaces/tools.json
for item in user brands missing; do
  identifier="$(jq -er '.id' "$DIR/surface-$item.json")"
  curl -fsS -H 'content-type: application/json' -H 'accept: application/json, text/event-stream' \
    -H "mcp-session-id: $sid" -d @"$DIR/surface-$item.json" http://tools:7120/mcp \
    > /logs/artifacts/surfaces/"$item".raw
  jq -Rse --argjson expected_id "$identifier" -f "$DIR/response.jq" \
    /logs/artifacts/surfaces/"$item".raw > /logs/artifacts/surfaces/"$item".json
done
PROGRAM
cp -R "$TASK/solution" "$OUT/program"
sh deploy/run-trial.sh -p "$TASK" -a oracle -o "$OUT/jobs" > "$OUT/trial.log" 2>&1
reward="$(find "$OUT/jobs" -path '*/verifier/reward.json' -print -quit)"
test -n "$reward"
trial="$(dirname "$(dirname "$reward")")"
surfaces="$trial/artifacts/logs/artifacts/surfaces"
jq -e '.reward==1 and .evaluator_version==33 and .environment_ok==1 and .contract_ok==1
  and .integrity_ok==1 and .runner_admission_checked==1 and .runner_admission_ok==1
  and .preservation_checked==1 and .history_checked==1' "$reward" >/dev/null
jq -S '.tools|sort_by(.name)' "$surfaces/tools.json" > "$OUT/actual-tools.json"
jq -S '.tools|sort_by(.name)' servers/snapshots/okta.json > "$OUT/expected-tools.json"
cmp "$OUT/actual-tools.json" "$OUT/expected-tools.json"
protocol="$(sed -n "s/^INIT='\\(.*\\)'$/\\1/p" "$OUT/program/solve.sh" | jq -er '.params.protocolVersion')"
jq -e --arg protocol "$protocol" --slurpfile expected servers/snapshots/okta.json '
  .serverInfo.name==$expected[0].server_info.name
    and .serverInfo.version==$expected[0].server_info.version
    and .protocolVersion==$protocol' "$surfaces/initialize.json" >/dev/null
jq -er '.content[0].text|fromjson
  | .id=="00u1mrgnzlz6Wx3Yz4x7" and .profile.login=="maria.gonzalez@octo-corp.example"
    and .profile.title=="Site Reliability Engineer"
    and .type.id=="oty8uc4n2Yd6Ws7Rt0x7"' "$surfaces/user.json" >/dev/null
jq -e '.content|length>0' "$surfaces/brands.json" >/dev/null
jq -er '.content[0].text|fromjson|keys==["error"] and (.error|type)=="string"' \
  "$surfaces/missing.json" >/dev/null
jq -e '[.entries[]|select(.service=="okta")]|length>0 and all(.[];
  .host=="octo-corp.okta.com" and .scheme=="https")' "$trial/verifier/journal.json" >/dev/null
jq -e 'any(.entries[];.path=="/api/v1/users/not-a-real-user" and .status==404)' \
  "$trial/verifier/journal.json" >/dev/null
for item in initialize tools user brands missing; do
  if grep -Eiq 'tool.?bench|/_bench/|world:8120|world:9000' "$surfaces/$item.json"; then
    printf '%s\n' "$item: explicit internal-origin or benchmark marker in captured response" >&2
    exit 1
  fi
done
printf '%s\n' 'Native initialization, 113 definitions, actual HTTPS authority, account/type/default-brand and typed-error surfaces pass.'
printf '%s\n' 'This bounded response audit is not a native-model session audit or an awareness experiment.'
