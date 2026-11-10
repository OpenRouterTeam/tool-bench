#!/bin/sh
set -eu
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
WORLD_IMAGE="${WORLD_IMAGE:?provide the revision-32 Argo world image}"
TOOLS_IMAGE="${TOOLS_IMAGE:?provide the composed Argo tools image}"
OUT="$(mktemp -d /tmp/tool-bench-argocd-errors.XXXXXX)"
printf 'artifacts: %s\n' "$OUT"
cargo run -q --locked -p scenario --bin taskgen -- --out "$OUT/tasks" \
  --world-image "$WORLD_IMAGE" --tools-image "$TOOLS_IMAGE" >/dev/null
TASK="$OUT/tasks/code__argocd__restart-indexer-worker"
mkdir "$OUT/original"
cp "$TASK/solution/"* "$OUT/original/"
for variant in missing_application invalid_resource concurrent_sync wrong_namespace; do
  rm -rf "$TASK/solution"
  mkdir "$TASK/solution"
  cp "$OUT/original/"* "$TASK/solution/"
  case "$variant" in
    missing_application)
      tool=get_application
      arguments='{"applicationName":"missing-application","applicationNamespace":"argocd"}' ;;
    invalid_resource)
      tool=run_resource_action
      arguments='{"applicationName":"search-indexer","applicationNamespace":"argocd","resourceRef":{"uid":"not-an-incarnation-precondition","group":"apps","version":"v1","kind":"Deployment","namespace":"search","name":"missing-worker"},"action":"restart"}' ;;
    concurrent_sync)
      tool=sync_application
      arguments='{"applicationName":"search-indexer","applicationNamespace":"argocd"}' ;;
    wrong_namespace)
      tool=sync_application
      arguments='{"applicationName":"search-indexer","applicationNamespace":"missing-namespace"}' ;;
  esac
  jq -n --arg tool "$tool" --argjson arguments "$arguments" \
    '{jsonrpc:"2.0",id:2,method:"tools/call",params:{name:$tool,arguments:$arguments}}' \
    > "$TASK/solution/error.json"
  if [ "$variant" = concurrent_sync ]; then
    printf '%s\n' 'call 7108 "$DIR/error.json"' >> "$TASK/solution/solve.sh"
  fi
  printf '%s\n' 'printf "%s\n" ARGOCD_BEFORE_EXPECTED_ERROR' \
    'call 7108 "$DIR/error.json"' 'printf "%s\n" ARGOCD_REACHED_LATER_CALL' \
    'call 7108 "$DIR/error.json"' >> "$TASK/solution/solve.sh"
  cp -R "$TASK/solution" "$OUT/program-$variant"
  if sh deploy/run-trial.sh -p "$TASK" -a oracle -o "$OUT/$variant" > "$OUT/$variant.log" 2>&1; then
    printf '%s\n' "$variant: failed native request completed the reference" >&2
    exit 1
  fi
  result="$(find "$OUT/$variant" -name result.json -print -quit)"
  receipt="$(find "$OUT/$variant" -name runner-admission.json -print -quit)"
  test -n "$result"
  test -n "$receipt"
  jq -e '.exception_info!=null and .verifier_result==null' "$result" >/dev/null
  jq -e '.phase=="failed" and .failure!=null and .evidence_sha256==null
    and .main_artifacts_sha256==null' "$receipt" >/dev/null
  test -z "$(find "$OUT/$variant" -path '*/verifier/reward.json' -print -quit)"
  test -z "$(find "$OUT/$variant" -path '*/artifacts/world/evidence.json' -print -quit)"
  log="$(dirname "$result")/agent/oracle.txt"
  grep -q ARGOCD_BEFORE_EXPECTED_ERROR "$log"
  if grep -q ARGOCD_REACHED_LATER_CALL "$log"; then exit 1; fi
  jq -Rse '[split("\n")[]|if startswith("data: ") then .[6:] else . end
      | fromjson?|select(type=="object" and .jsonrpc=="2.0" and .id==2)]
    | any(.[];.result.isError==true)' "$log" >/dev/null
  printf '%s\n' "$variant: actual native error stops later calls and authoritative assessment"
done
