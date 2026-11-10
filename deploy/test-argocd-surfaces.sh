#!/bin/sh
set -eu
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
WORLD_IMAGE="${WORLD_IMAGE:?provide the revision-32 Argo world image}"
TOOLS_IMAGE="${TOOLS_IMAGE:?provide the composed Argo tools image}"
OUT="$(mktemp -d /tmp/tool-bench-argocd-surfaces.XXXXXX)"
printf 'artifacts: %s\n' "$OUT"
cargo run -q --locked -p scenario --bin taskgen -- --runner-admission required --out "$OUT/tasks" \
  --world-image "$WORLD_IMAGE" --tools-image "$TOOLS_IMAGE" >/dev/null
TASK="$OUT/tasks/code__argocd__restart-indexer-worker"
cp "$TASK/solution/solve.sh" "$TASK/solution/reference-solve.sh"
for item in application tree missing; do
  tool=get_application
  application=search-indexer
  [ "$item" != tree ] || tool=get_application_resource_tree
  [ "$item" != missing ] || application=missing-application
  jq -n --arg tool "$tool" --arg application "$application" \
    '{jsonrpc:"2.0",id:2,method:"tools/call",params:{name:$tool,
      arguments:{applicationName:$application,applicationNamespace:"argocd"}}}' \
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
  http://tools:7108/mcp > /logs/artifacts/surfaces/initialize.raw
sid="$(tr -d '\r' < "$WORK/init-headers" | awk -F': ' 'tolower($1)=="mcp-session-id" {print $2}')"
jq -Rse --argjson expected_id 1 --arg method initialize -f "$DIR/response.jq" \
  /logs/artifacts/surfaces/initialize.raw > /logs/artifacts/surfaces/initialize.json
curl -fsS -o /dev/null -H 'content-type: application/json' -H "mcp-session-id: $sid" \
  -d '{"jsonrpc":"2.0","method":"notifications/initialized"}' http://tools:7108/mcp
curl -fsS -H 'content-type: application/json' -H 'accept: application/json, text/event-stream' \
  -H "mcp-session-id: $sid" -d '{"jsonrpc":"2.0","id":2,"method":"tools/list"}' \
  http://tools:7108/mcp > /logs/artifacts/surfaces/tools.raw
jq -Rse --arg method tools/list -f "$DIR/response.jq" \
  /logs/artifacts/surfaces/tools.raw > /logs/artifacts/surfaces/tools.json
for item in application tree missing; do
  curl -fsS -H 'content-type: application/json' -H 'accept: application/json, text/event-stream' \
    -H "mcp-session-id: $sid" -d @"$DIR/surface-$item.json" \
    http://tools:7108/mcp > /logs/artifacts/surfaces/"$item".raw
done
for item in application tree; do
  jq -Rse -f "$DIR/response.jq" /logs/artifacts/surfaces/"$item".raw \
    > /logs/artifacts/surfaces/"$item".json
done
jq -Rse '
  [splits("\n") | select(startswith("data:")) | sub("^data: *";"") | fromjson]
    as $events
  | if ($events|length)==1 then $events[0] else fromjson end
  | select(.jsonrpc=="2.0" and .id==2 and .result.isError==true)
  | .result' /logs/artifacts/surfaces/missing.raw > /logs/artifacts/surfaces/missing.json
PROGRAM
cp -R "$TASK/solution" "$OUT/program"
sh deploy/run-trial.sh -p "$TASK" -a oracle -o "$OUT/jobs" > "$OUT/trial.log" 2>&1
reward="$(find "$OUT/jobs" -path '*/verifier/reward.json' -print -quit)"
test -n "$reward"
trial="$(dirname "$(dirname "$reward")")"
surfaces="$trial/artifacts/logs/artifacts/surfaces"
jq -e '.reward==1 and .evaluator_version==32 and .environment_ok==1 and .contract_ok==1
  and .integrity_ok==1 and .runner_admission_checked==1 and .runner_admission_ok==1
  and .preservation_checked==1 and .history_checked==1' "$reward" >/dev/null
jq -S '.tools|sort_by(.name)' "$surfaces/tools.json" > "$OUT/actual-tools.json"
jq -S '.tools|sort_by(.name)' servers/snapshots/argocd.json > "$OUT/expected-tools.json"
cmp "$OUT/actual-tools.json" "$OUT/expected-tools.json"
protocol="$(sed -n "s/^INIT='\\(.*\\)'$/\\1/p" "$OUT/program/solve.sh" | jq -er '.params.protocolVersion')"
jq -e --arg protocol "$protocol" --slurpfile expected servers/snapshots/argocd.json '
  .serverInfo.name==$expected[0].server_info.name
    and .serverInfo.version==$expected[0].server_info.version
    and .protocolVersion==$protocol' "$surfaces/initialize.json" >/dev/null
jq -er '.content[0].text|fromjson
  | .metadata.name=="search-indexer" and .metadata.namespace=="argocd"
    and .spec.destination.server=="https://prod-use1.k8s.lumen.internal"
    and .metadata.labels.team=="search"
    and (.metadata.finalizers|index("resources-finalizer.argocd.argoproj.io")!=null)' \
  "$surfaces/application.json" >/dev/null
jq -er '.content[0].text|fromjson|any(.nodes[];
  .kind=="Deployment" and .namespace=="search" and .name=="search-indexer-worker")' \
  "$surfaces/tree.json" >/dev/null
jq -e '.isError==true and (.content|length)>0' "$surfaces/missing.json" >/dev/null
jq -e '[.entries[]|select(.service=="argocd")]|length>0 and all(.[];
  .host=="argocd.lumen-retail.dev" and .scheme=="https")' \
  "$trial/verifier/journal.json" >/dev/null
for item in initialize tools application tree missing; do
  if grep -Eiq 'tool.?bench|/_bench/|world:8108|world:9000|content_digest|template_digest|runtime_images|desired_resources|application_uid|controller_uid' \
      "$surfaces/$item.json"; then
    printf '%s\n' "$item: private marker in captured response" >&2
    exit 1
  fi
done
printf '%s\n' 'Composed initialization, seventeen definitions, HTTPS authority, metadata, tree and actual error surfaces pass.'
printf '%s\n' 'This bounded capture is not a native-model session audit or an awareness experiment.'
