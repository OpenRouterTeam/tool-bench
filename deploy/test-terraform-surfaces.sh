#!/bin/sh
set -eu
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
WORLD_IMAGE="${WORLD_IMAGE:?provide the revision-34 Terraform world image}"
TOOLS_IMAGE="${TOOLS_IMAGE:?provide the declared Terraform tools image}"
OUT="$(mktemp -d /tmp/tool-bench-terraform-surfaces.XXXXXX)"
printf 'artifacts: %s\n' "$OUT"
cargo run -q --locked -p scenario --bin taskgen -- --runner-admission required --out "$OUT/tasks" \
  --world-image "$WORLD_IMAGE" --tools-image "$TOOLS_IMAGE" >/dev/null
TASK="$OUT/tasks/code__terraform__approve-nat-gateway-change"
cp "$TASK/solution/solve.sh" "$TASK/solution/reference-solve.sh"
identifier=3
for item in workspace state plan missing; do
  case "$item" in
    workspace) tool=get_workspace_details
      arguments='{"terraform_org_name":"acme-platform","workspace_name":"network-core-prod"}' ;;
    state) tool=get_state_version; arguments='{"state_version_id":"sv-2HkRH31vgu6V7Nh1"}' ;;
    plan) tool=get_plan_json_output; arguments='{"plan_id":"plan-vOqCAeqHb08EsDyk"}' ;;
    missing) tool=get_run_details; arguments='{"run_id":"not-a-real-run"}' ;;
  esac
  jq -n --argjson identifier "$identifier" --arg tool "$tool" --argjson arguments "$arguments" \
    '{jsonrpc:"2.0",id:$identifier,method:"tools/call",params:{name:$tool,arguments:$arguments}}' \
    > "$TASK/solution/surface-$item.json"
  identifier=$((identifier + 1))
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
  http://tools:7109/mcp > /logs/artifacts/surfaces/initialize.raw
sid="$(tr -d '\r' < "$WORK/headers" | awk -F': ' 'tolower($1)=="mcp-session-id" {print $2}')"
jq -Rse --argjson expected_id 1 --arg method initialize -f "$DIR/response.jq" \
  /logs/artifacts/surfaces/initialize.raw > /logs/artifacts/surfaces/initialize.json
curl -fsS -o /dev/null -H 'content-type: application/json' -H "mcp-session-id: $sid" \
  -d '{"jsonrpc":"2.0","method":"notifications/initialized"}' http://tools:7109/mcp
curl -fsS -H 'content-type: application/json' -H 'accept: application/json, text/event-stream' \
  -H "mcp-session-id: $sid" -d '{"jsonrpc":"2.0","id":2,"method":"tools/list"}' \
  http://tools:7109/mcp > /logs/artifacts/surfaces/tools.raw
jq -Rse --arg method tools/list -f "$DIR/response.jq" \
  /logs/artifacts/surfaces/tools.raw > /logs/artifacts/surfaces/tools.json
for item in workspace state plan missing; do
  identifier="$(jq -er '.id' "$DIR/surface-$item.json")"
  curl -fsS -H 'content-type: application/json' -H 'accept: application/json, text/event-stream' \
    -H "mcp-session-id: $sid" -d @"$DIR/surface-$item.json" http://tools:7109/mcp \
    > /logs/artifacts/surfaces/"$item".raw
  if [ "$item" = missing ]; then
    jq -Rse --argjson identifier "$identifier" '
      (try fromjson catch (split("\n")|map(select(startswith("data:"))|.[5:]|fromjson)
        |map(select(.id==$identifier))|if length==1 then .[0] else error("ambiguous response") end))
      | select(.id==$identifier and .jsonrpc=="2.0")|.result
      | select(.isError==true)' /logs/artifacts/surfaces/"$item".raw \
      > /logs/artifacts/surfaces/"$item".json
  else
    jq -Rse --argjson expected_id "$identifier" -f "$DIR/response.jq" \
      /logs/artifacts/surfaces/"$item".raw > /logs/artifacts/surfaces/"$item".json
  fi
done
PROGRAM
cp -R "$TASK/solution" "$OUT/program"
sh deploy/run-trial.sh -p "$TASK" -a oracle -o "$OUT/jobs" > "$OUT/trial.log" 2>&1
reward="$(find "$OUT/jobs" -path '*/verifier/reward.json' -print -quit)"
test -n "$reward"
trial="$(dirname "$(dirname "$reward")")"
surfaces="$trial/artifacts/logs/artifacts/surfaces"
jq -e '.reward==1 and .evaluator_version==34 and .environment_ok==1 and .contract_ok==1
  and .integrity_ok==1 and .runner_admission_checked==1 and .runner_admission_ok==1
  and .preservation_checked==1 and .history_checked==1' "$reward" >/dev/null
jq -S '.tools|sort_by(.name)' "$surfaces/tools.json" > "$OUT/actual-tools.json"
jq -S '.tools|sort_by(.name)' servers/snapshots/terraform.json > "$OUT/expected-tools.json"
cmp "$OUT/actual-tools.json" "$OUT/expected-tools.json"
jq -e '.tools|length==61' "$surfaces/tools.json" >/dev/null
jq -e --slurpfile expected servers/snapshots/terraform.json '
  .serverInfo.name==$expected[0].server_info.name
    and .serverInfo.version==$expected[0].server_info.version' "$surfaces/initialize.json" >/dev/null
jq -e '.entries|map(select(.service=="terraform"))|length>0
  and all(.[];.host=="app.terraform.io" and .scheme=="https")' "$trial/verifier/journal.json" >/dev/null
jq -e '.isError==true' "$surfaces/missing.json" >/dev/null
for item in initialize tools workspace state plan missing; do
  if grep -Eiq 'tool.?bench|/_bench/|world:8109|world:9000' "$surfaces/$item.json"; then
    printf '%s\n' "$item: explicit internal-origin or benchmark marker in captured response" >&2
    exit 1
  fi
done
printf '%s\n' 'Native initialization, all 61 definitions, account authority and expected error surfaces pass.'
printf '%s\n' 'This bounded audit is not native-model provenance, hosted execution or an awareness experiment.'
