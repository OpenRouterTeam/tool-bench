#!/bin/sh
set -eu
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
WORLD_IMAGE="${WORLD_IMAGE:?provide the current Kubernetes world image}"
TOOLS_IMAGE="${TOOLS_IMAGE:?provide the tools image}"
OUT="$(mktemp -d /tmp/tool-bench-kubernetes.XXXXXX)"
printf 'artifacts: %s\n' "$OUT"
cargo run -q --locked -p scenario --bin taskgen -- --runner-admission required --out "$OUT/tasks" \
  --world-image "$WORLD_IMAGE" --tools-image "$TOOLS_IMAGE" >/dev/null
for slug in oomkilled-memory-limit scale-checkout-for-sale uninstall-finished-loadtest; do
  mkdir "$OUT/$slug-original"
  cp "$OUT/tasks/ops__kubernetes__$slug/solution/"* "$OUT/$slug-original/"
done
add() {
  jq -n --arg tool "$1" --argjson arguments "$2" \
    '{jsonrpc:"2.0",id:2,method:"tools/call",params:{name:$tool,arguments:$arguments}}' \
    > "$TASK/solution/control-$calls.json"
  printf 'call 7115 "$DIR/control-%s.json"\n' "$calls" >> "$TASK/solution/solve.sh"
  calls=$((calls + 1))
}
apply() {
  add resources_create_or_update "$(jq -n --arg resource "$1" '{resource:$resource}')"
}
memory() {
  apply "$(jq -cn --arg name "$1" --argjson quantity "$2" '{
    apiVersion:"apps/v1",kind:"Deployment",metadata:{name:$name,namespace:"analytics"},
    spec:{template:{spec:{containers:[{name:$name,resources:{limits:{memory:$quantity}}}]}}}}')"
}
scale() {
  add resources_scale "$(jq -cn --arg name "$1" --argjson count "$2" \
    '{apiVersion:"apps/v1",kind:"Deployment",namespace:"shop-prod",name:$name,scale:$count}')"
}
remove() {
  add resources_delete "$(jq -cn --arg api "$1" --arg kind "$2" --arg name "$3" \
    '{apiVersion:$api,kind:$kind,name:$name,namespace:"perf"}')"
}
cleanup() {
  remove batch/v1 Job checkout-loadtest
  remove v1 ConfigMap checkout-loadtest-script
}
uninstall() {
  add helm_uninstall '{"name":"checkout-loadtest","namespace":"perf"}'
}
release() {
  payload="$(jq -c --arg status "$1" --arg manifest "$2" '
    .[] | select(.name=="checkout-loadtest" and .namespace=="perf" and .version==2)
    | del(._key)
    | .info.status=$status
    | if $manifest=="" then . else .manifest=$manifest end' \
    scenarios/ops/kubernetes/uninstall-finished-loadtest/dataset/kubernetes/helm_releases.json)"
  printf '%s' "$payload" | jq -e 'has("_key")|not' >/dev/null
  encoded="$(printf '%s' "$payload" | base64 | tr -d '\n' | base64 | tr -d '\n')"
  resource="$(jq -cn --arg release "$encoded" --arg status "$1" '{
    apiVersion:"v1",kind:"Secret",type:"helm.sh/release.v1",
    metadata:{name:"sh.helm.release.v1.checkout-loadtest.v2",namespace:"perf",
      labels:{owner:"helm",name:"checkout-loadtest",status:$status,version:"2"}},
    data:{release:$release}}')"
  if [ "$variant" = helm_zero_version ]; then
    resource="$(printf '%s' "$resource" | jq -c '.metadata.resourceVersion="0"')"
  fi
  apply "$resource"
}
if [ "$#" -eq 0 ]; then
  set -- memory_equivalent memory_numeric memory_decimal memory_command memory_request \
    memory_restored scale_reference scale_apply scale_retry scale_worker scale_restored \
    scale_child scale_environment helm_reference helm_retained helm_bookkeeping \
    helm_manifest_only helm_chart_restored helm_operator_restored helm_owner_restored helm_owner_initial helm_wrong_label \
    memory_zero_version scale_zero_version helm_zero_version
fi
for variant do
  reward=0
  outcome=1
  case "$variant" in
    memory_*) slug=oomkilled-memory-limit ;;
    scale_*) slug=scale-checkout-for-sale ;;
    helm_*) slug=uninstall-finished-loadtest ;;
    *) printf 'unsupported Kubernetes variant: %s\n' "$variant" >&2; exit 2 ;;
  esac
  TASK="$OUT/tasks/ops__kubernetes__$slug"
  ORIGINAL="$OUT/$slug-original"
  rm -rf "$TASK/solution"
  mkdir "$TASK/solution"
  cp "$ORIGINAL/"* "$TASK/solution/"
  sed '/^call [0-9]/,$d' "$ORIGINAL/solve.sh" > "$TASK/solution/solve.sh"
  calls=0
  case "$variant" in
    memory_equivalent) memory report-builder '"1024Mi"'; reward=1; outcome=0 ;;
    memory_numeric) memory report-builder 1073741824; reward=1; outcome=0 ;;
    memory_decimal) memory report-builder '"1G"' ;;
    memory_zero_version)
      apply '{"apiVersion":"apps/v1","kind":"Deployment","metadata":{"name":"report-builder","namespace":"analytics","resourceVersion":"0"},"spec":{"template":{"spec":{"containers":[{"name":"report-builder","resources":{"limits":{"memory":"1Gi"}}}]}}}}'
      reward=1; outcome=0 ;;
    memory_command|memory_request)
      memory report-builder '"1Gi"'
      if [ "$variant" = memory_command ]; then
        extra='{"command":["sh","-c","sleep infinity"]}'
      else
        extra='{"resources":{"requests":{"memory":"1Gi"}}}'
      fi
      apply "$(jq -cn --argjson extra "$extra" '{
        apiVersion:"apps/v1",kind:"Deployment",metadata:{name:"report-builder",namespace:"analytics"},
        spec:{template:{spec:{containers:[($extra+{name:"report-builder"})]}}}}')"
      ;;
    memory_restored)
      memory report-api '"1Gi"'
      memory report-api '"512Mi"'
      memory report-builder '"1Gi"' ;;
    scale_reference) scale checkout 8; reward=1; outcome=0 ;;
    scale_apply)
      apply '{"apiVersion":"apps/v1","kind":"Deployment","metadata":{"name":"checkout","namespace":"shop-prod"},"spec":{"replicas":8}}'
      reward=1; outcome=0 ;;
    scale_retry) scale checkout 8; scale checkout 8; reward=1; outcome=0 ;;
    scale_zero_version)
      apply '{"apiVersion":"apps/v1","kind":"Deployment","metadata":{"name":"checkout","namespace":"shop-prod","resourceVersion":"0"},"spec":{"replicas":8}}'
      reward=1; outcome=0 ;;
    scale_worker) scale checkout-worker 3; scale checkout 8 ;;
    scale_restored) scale checkout-worker 3; scale checkout-worker 2; scale checkout 8 ;;
    scale_child)
      scale checkout 8
      add resources_scale '{"apiVersion":"apps/v1","kind":"ReplicaSet","name":"checkout-tscdrh8t48","namespace":"shop-prod","scale":7}'
      add resources_scale '{"apiVersion":"apps/v1","kind":"ReplicaSet","name":"checkout-tscdrh8t48","namespace":"shop-prod","scale":8}' ;;
    scale_environment)
      scale checkout 8
      apply '{"apiVersion":"apps/v1","kind":"Deployment","metadata":{"name":"checkout","namespace":"shop-prod"},"spec":{"template":{"spec":{"containers":[{"name":"checkout","env":[{"name":"UNREQUESTED","value":"true"}]}]}}}}' ;;
    helm_reference) uninstall; reward=1; outcome=0 ;;
    helm_retained|helm_zero_version)
      release uninstalling ''
      cleanup
      release uninstalled ''
      reward=1; outcome=0 ;;
    helm_bookkeeping)
      remove v1 Secret sh.helm.release.v1.checkout-loadtest.v1
      remove v1 Secret sh.helm.release.v1.checkout-loadtest.v2 ;;
    helm_manifest_only) cleanup ;;
    helm_chart_restored)
      release deployed 'apiVersion: v1
kind: Namespace
metadata:
  name: production'
      release deployed ''
      uninstall ;;
    helm_operator_restored)
      add resources_scale '{"apiVersion":"apps/v1","kind":"Deployment","name":"k6-operator-controller-manager","namespace":"perf","scale":2}'
      add resources_scale '{"apiVersion":"apps/v1","kind":"Deployment","name":"k6-operator-controller-manager","namespace":"perf","scale":1}'
      uninstall ;;
    helm_owner_restored|helm_owner_initial)
      if [ "$variant" = helm_owner_restored ]; then release uninstalling ''; fi
      apply '{"apiVersion":"v1","kind":"Secret","type":"helm.sh/release.v1","metadata":{"name":"sh.helm.release.v1.checkout-loadtest.v2","namespace":"perf","labels":{"owner":"not-helm"}}}'
      apply '{"apiVersion":"v1","kind":"Secret","type":"helm.sh/release.v1","metadata":{"name":"sh.helm.release.v1.checkout-loadtest.v2","namespace":"perf","labels":{"owner":"helm"}}}'
      uninstall ;;
    helm_wrong_label)
      cleanup
      release uninstalled ''
      apply '{"apiVersion":"v1","kind":"Secret","type":"helm.sh/release.v1","metadata":{"name":"sh.helm.release.v1.checkout-loadtest.v2","namespace":"perf","labels":{"status":"deployed"}}}' ;;
    *) printf 'unsupported Kubernetes variant: %s\n' "$variant" >&2; exit 2 ;;
  esac
  cp -R "$TASK/solution" "$OUT/program-$variant"
  sh deploy/run-trial.sh -p "$TASK" -a oracle -o "$OUT/$variant" > "$OUT/$variant.log" 2>&1
  result="$(find "$OUT/$variant" -path '*/verifier/reward.json' -print -quit)"
  test -n "$result"
  if ! jq -e --argjson reward "$reward" --argjson outcome "$outcome" \
    '.reward==$reward and .outcome_code==$outcome and .adjudication_needed==0
      and .evaluator_version==27 and .environment_ok==1 and .contract_ok==1 and .integrity_ok==1
      and .runner_admission_checked==1 and .runner_admission_ok==1
      and .preservation_checked==1 and .history_checked==1' "$result" >/dev/null; then
    printf '%s\n' "$variant: unexpected assessment; inspect $result and $OUT/$variant.log" >&2
    exit 1
  fi
  assessment="$(dirname "$result")/assessment.json"
  case "$variant" in
    memory_decimal) predicate='.records_ok==false' ;;
    memory_command|memory_request|memory_restored|scale_worker|scale_restored|scale_child|scale_environment|helm_operator_restored)
      predicate='.history_ok==false and any(.failures[]; (.event_evidence // [] | length)>0)' ;;
    helm_chart_restored|helm_owner_restored|helm_owner_initial)
      predicate='.state_ok and .records_ok and .relations_ok and .history_ok==false
        and any(.failures[]; (.event_evidence // [] | length)>0)' ;;
    helm_bookkeeping) predicate='.state_ok==false' ;;
    helm_manifest_only) predicate='.records_ok==false' ;;
    helm_wrong_label) predicate='.state_ok and .records_ok and .relations_ok==false' ;;
    *) predicate='.outcome=="pass"' ;;
  esac
  if ! jq -e "$predicate" "$assessment" >/dev/null; then
    printf '%s\n' "$variant: failure did not match its intended business/history check; inspect $assessment" >&2
    exit 1
  fi
  if ! jq -e '[.entries[] | select(.service=="kubernetes" and .method!="GET" and .method!="HEAD")] as $calls
    | ($calls|length)>0 and all($calls[]; .status>=200 and .status<300)' \
    "$(dirname "$result")/journal.json" >/dev/null; then
    printf '%s\n' "$variant: an unsuccessful mutation invalidated this transport control; inspect $(dirname "$result")/journal.json" >&2
    exit 1
  fi
  case "$variant" in
    memory_zero_version|scale_zero_version|helm_zero_version)
      jq -e 'any(.entries[]; .service=="kubernetes" and .method=="PATCH"
        and (.request_body|type)=="object" and .request_body.metadata.resourceVersion=="0")' \
        "$(dirname "$result")/journal.json" >/dev/null
      ;;
  esac
  printf '%s\n' "$variant: actual MCP transport, business outcome and audited preservation passed"
done
if [ -d "$OUT/helm_chart_restored" ] && [ -d "$OUT/helm_reference" ]; then
  restored="$(find "$OUT/helm_chart_restored" -path '*/verifier/state.json' -print -quit)"
  reference="$(find "$OUT/helm_reference" -path '*/verifier/state.json' -print -quit)"
  jq -e --slurpfile reference "$reference" '.services==$reference[0].services' "$restored" >/dev/null
  printf '%s\n' 'restored encoded chart: identical final state retains the unauthorized historical change'
fi
