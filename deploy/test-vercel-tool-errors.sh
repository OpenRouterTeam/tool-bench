#!/bin/sh
set -eu
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
WORLD_IMAGE="${WORLD_IMAGE:?provide the revision-31 Vercel world image}"
TOOLS_IMAGE="${TOOLS_IMAGE:?provide the declared tools image}"
OUT="$(mktemp -d /tmp/tool-bench-vercel-errors.XXXXXX)"
printf 'artifacts: %s\n' "$OUT"
cargo run -q --locked -p scenario --bin taskgen -- --out "$OUT/tasks" \
  --world-image "$WORLD_IMAGE" --tools-image "$TOOLS_IMAGE" >/dev/null
TASK="$OUT/tasks/code__vercel__enable-gift-cards-in-previews"
mkdir "$OUT/original"
cp "$TASK/solution/"* "$OUT/original/"
for variant in missing_project partial_batch missing_delete unsupported_source missing_deployment; do
  rm -rf "$TASK/solution"
  mkdir "$TASK/solution"
  cp "$OUT/original/"* "$TASK/solution/"
  case "$variant" in
    missing_project)
      tool=vercel-create-environment-variables
      arguments='{"projectId":"missing-project","environmentVariables":[{"key":"EXAMPLE","value":"value","target":["preview"],"type":"plain"}]}' ;;
    partial_batch)
      tool=vercel-create-environment-variables
      arguments='{"projectId":"storefront","environmentVariables":[
        {"key":"NEW_PARTIAL_ITEM","value":"created","target":["preview"],"type":"plain"},
        {"key":"PAYMENTS_API_URL","value":"not-written","target":["production"],"type":"plain"}]}' ;;
    missing_delete)
      tool=vercel-remove-environment-variable
      arguments='{"idOrName":"storefront","id":"missing-variable"}' ;;
    unsupported_source)
      tool=vercel-create-deployment
      arguments='{"name":"storefront","deploymentId":"dpl_8o704GjcwumkLAaxPwjoxb4V6QDF","withLatestCommit":true}' ;;
    missing_deployment)
      tool=vercel-get-deployment
      arguments='{"idOrUrl":"dpl_missing"}' ;;
  esac
  jq -n --arg tool "$tool" --argjson arguments "$arguments" \
    '{jsonrpc:"2.0",id:2,method:"tools/call",params:{name:$tool,arguments:$arguments}}' \
    > "$TASK/solution/error.json"
  printf '%s\n' 'printf "%s\n" VERCEL_BEFORE_EXPECTED_ERROR' \
    'call 7110 "$DIR/error.json"' 'printf "%s\n" VERCEL_REACHED_LATER_CALL' \
    'call 7110 "$DIR/error.json"' >> "$TASK/solution/solve.sh"
  cp -R "$TASK/solution" "$OUT/program-$variant"
  if sh deploy/run-trial.sh -p "$TASK" -a oracle -o "$OUT/$variant" > "$OUT/$variant.log" 2>&1; then
    printf '%s\n' "$variant: a failed replica tool result completed the reference" >&2
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
  grep -q VERCEL_BEFORE_EXPECTED_ERROR "$log"
  if grep -q VERCEL_REACHED_LATER_CALL "$log"; then exit 1; fi
  jq -Rse '[split("\n")[]|if startswith("data: ") then .[6:] else . end
      | fromjson?|select(type=="object" and .jsonrpc=="2.0" and .id==2)]
    | any(.[];.result.isError==true)' "$log" >/dev/null
  printf '%s\n' "$variant: actual replica failure stops later calls and authoritative assessment"
done
