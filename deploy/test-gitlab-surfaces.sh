#!/bin/sh
set -eu
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
WORLD_IMAGE="${WORLD_IMAGE:?provide the account-origin world image}"
TOOLS_IMAGE="${TOOLS_IMAGE:?provide the account-origin tools image}"
OUT="$(mktemp -d /tmp/tool-bench-gitlab-surfaces.XXXXXX)"
printf 'artifacts: %s\n' "$OUT"
cargo run -q --locked -p scenario --bin taskgen -- --runner-admission required --out "$OUT/tasks" \
  --world-image "$WORLD_IMAGE" --tools-image "$TOOLS_IMAGE" >/dev/null
CAPTURING="$OUT/tasks/business__multi__zendesk-refund-via-stripe/solution"
for account in northwind lumen; do
  case "$account" in
    northwind)
      task=code__gitlab__diagnose-failed-pipeline
      origin=https://gitlab.northwind.internal
      project=northwind/payments/checkout-api
      username=sam.okafor
      ;;
    lumen)
      task=code__multi__argocd-degraded-gitlab-commit
      origin=https://gitlab.lumen.internal
      project=lumen-retail/storefront/storefront-deploy
      username=maria.okafor
      ;;
  esac
  TASK="$OUT/tasks/$task"
  cp "$TASK/solution/solve.sh" "$TASK/solution/reference-solve.sh"
  cp "$CAPTURING/"*.jq "$TASK/solution/"
  sed '/^bound_call [0-9]/,$d' "$CAPTURING/solve.sh" > "$TASK/solution/solve.sh"
  jq -n --arg project "$project" \
    '{jsonrpc:"2.0",id:2,method:"tools/call",params:{name:"get_project",arguments:{project_id:$project}}}' > "$TASK/solution/call-10.json"
  for page in 1 2; do
    jq -n --argjson page "$page" \
      '{jsonrpc:"2.0",id:2,method:"tools/call",params:{name:"list_projects",arguments:{per_page:1,page:$page,order_by:"id",sort:"asc"}}}' \
      > "$TASK/solution/call-$((10+page)).json"
  done
  jq -n --arg username "$username" \
    '{jsonrpc:"2.0",id:2,method:"tools/call",params:{name:"get_users",arguments:{usernames:[$username]}}}' > "$TASK/solution/call-13.json"
  cat >> "$TASK/solution/solve.sh" <<'EOF'
sh "$DIR/reference-solve.sh"
mkdir -p /logs/artifacts/surfaces
curl -fsS -D "$WORK/init-headers" -H 'content-type: application/json' \
  -H 'accept: application/json, text/event-stream' -d "$INIT" \
  http://tools:7107/mcp > /logs/artifacts/surfaces/initialize.raw
sid="$(tr -d '\r' < "$WORK/init-headers" | awk -F': ' 'tolower($1)=="mcp-session-id" {print $2}')"
test -n "$sid"
cp "$WORK/init-headers" /logs/artifacts/surfaces/initialize.headers
jq -e 'if .jsonrpc == "2.0" and .id == 1 and (.result.serverInfo|type) == "object"
  then .result else error("invalid initialization") end' \
  /logs/artifacts/surfaces/initialize.raw > /logs/artifacts/surfaces/initialize.json
curl -fsS -o /dev/null -H 'content-type: application/json' -H "mcp-session-id: $sid" \
  -d '{"jsonrpc":"2.0","method":"notifications/initialized"}' http://tools:7107/mcp
curl -fsS -H 'content-type: application/json' -H "mcp-session-id: $sid" \
  -d '{"jsonrpc":"2.0","id":2,"method":"tools/list"}' http://tools:7107/mcp \
  > /logs/artifacts/surfaces/tools.raw
jq -e 'if .jsonrpc == "2.0" and .id == 2 and (.result.tools|type) == "array"
  then .result else error("invalid tool definitions") end' \
  /logs/artifacts/surfaces/tools.raw > /logs/artifacts/surfaces/tools.json
EOF
  for index in 10 11 12 13; do
    printf '%s\n' '[]' > "$TASK/solution/call-$index.capture.json"
    printf 'bound_call 7107 "$DIR/call-%s.json" "$DIR/call-%s.capture.json" 0\n' \
      "$index" "$index" >> "$TASK/solution/solve.sh"
    printf 'cp "$WORK/response.json" /logs/artifacts/surfaces/call-%s.raw\n' "$index" >> "$TASK/solution/solve.sh"
    printf 'jq -Rse -f "$DIR/response.jq" "$WORK/response.json" > /logs/artifacts/surfaces/call-%s.json\n' \
      "$index" >> "$TASK/solution/solve.sh"
  done
  cp -R "$TASK/solution" "$OUT/program-$account"
  sh deploy/run-trial.sh -p "$TASK" -a oracle -o "$OUT/$account" > "$OUT/$account.log" 2>&1
  reward="$(find "$OUT/$account" -path '*/verifier/reward.json' -print -quit)"
  test -n "$reward"
  trial="$(dirname "$(dirname "$reward")")"
  surfaces="$trial/artifacts/logs/artifacts/surfaces"
  jq -e '.reward == 1 and .environment_ok == 1 and .contract_ok == 1 and .integrity_ok == 1
    and .runner_admission_checked == 1 and .runner_admission_ok == 1
    and .preservation_checked == 1 and .history_checked == 1' "$reward" >/dev/null
  jq -S '.tools|sort_by(.name)' "$surfaces/tools.json" > "$OUT/$account-tools.json"
  jq -S '.tools|sort_by(.name)' servers/snapshots/gitlab.json > "$OUT/reference-tools.json"
  cmp "$OUT/$account-tools.json" "$OUT/reference-tools.json"
  jq -e --slurpfile expected servers/snapshots/gitlab.json \
    '.serverInfo.name == $expected[0].server_info.name and .serverInfo.version == $expected[0].server_info.version' \
    "$surfaces/initialize.json" >/dev/null
  for index in 10 11 12 13; do
    jq -e --arg origin "$origin" '
      [.content[] | select(.type == "text") | .text | fromjson] as $payloads
      | [$payloads[] | .. | objects | .web_url? | select(type == "string")] as $links
      | ($links | length) > 0 and all($links[]; startswith($origin + "/"))
    ' "$surfaces/call-$index.json" >/dev/null
  done
  jq -e --arg host "${origin#https://}" '
    [.events[] | select(.kind == "request_head_received" and .operation.service == "gitlab")] as $heads
    | ($heads|length) > 0 and all($heads[]; .head.host == $host and .head.scheme == "https")
      and any($heads[]; .operation.path == "/api/v4/projects" and (.head.query|contains("page=2")))
  ' "$trial/artifacts/world/evidence.json" >/dev/null
  jq -e '.images.tools.connector_env_sha256 == .context.connector_env_sha256
    and .images.tools.connector_env_sha256.GITLAB_API_URL != null
    and .images.tools.connector_env_sha256.GITLAB_GRAPHQL_URL != null' \
    "$trial/realized-manifest.json" >/dev/null
  printf '%s\n' "$account: authentic definitions, initialization, identities, URLs, pagination and actual connection provenance passed"
done
printf '%s\n' 'account-origin surface audit passed; this is not native prompt auditing or certified model unawareness'
