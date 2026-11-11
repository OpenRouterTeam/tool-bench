#!/bin/sh
set -eu
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
WORLD_IMAGE="${WORLD_IMAGE:?provide the revision-33 Okta world image}"
TOOLS_IMAGE="${TOOLS_IMAGE:?provide the declared Okta tools image}"
OUT="$(mktemp -d /tmp/tool-bench-okta-errors.XXXXXX)"
printf 'artifacts: %s\n' "$OUT"
cargo run -q --locked -p scenario --bin taskgen -- --runner-admission required --out "$OUT/tasks" \
  --world-image "$WORLD_IMAGE" --tools-image "$TOOLS_IMAGE" >/dev/null
TASK="$OUT/tasks/cloud__okta__transfer-to-platform-team"
mkdir "$OUT/original"
cp "$TASK/solution/"* "$OUT/original/"
for variant in missing_user ambiguous_user invalid_rule already_deprovisioned; do
  rm -rf "$TASK/solution"
  mkdir "$TASK/solution"
  cp "$OUT/original/"* "$TASK/solution/"
  case "$variant" in
    missing_user)
      tool=get_user
      arguments='{"user_id":"not-a-real-user"}' ;;
    ambiguous_user)
      tool=get_user
      arguments='{"user_id":"alex.kim"}' ;;
    invalid_rule)
      tool=update_policy_rule
      arguments='{"policy_id":"00p4pwcntr2Uv9Wx0x7","rule_id":"0pr5rule0tr2Uv9Wx0x7","rule_data":{"name":"Contractors rule","type":"PASSWORD","actions":{"selfServicePasswordReset":{"access":"deny"}}}}' ;;
    already_deprovisioned)
      tool=deactivate_user
      arguments='{"user_id":"00u1tylrbrk0Ef7Gh8x7"}' ;;
  esac
  jq -n --arg tool "$tool" --argjson arguments "$arguments" \
    '{jsonrpc:"2.0",id:2,method:"tools/call",params:{name:$tool,arguments:$arguments}}' \
    > "$TASK/solution/error.json"
  printf '%s\n' 'printf "%s\n" OKTA_BEFORE_EXPECTED_ERROR' \
    'call 7120 "$DIR/error.json"' 'printf "%s\n" OKTA_REACHED_LATER_CALL' \
    'call 7120 "$DIR/error.json"' >> "$TASK/solution/solve.sh"
  cp -R "$TASK/solution" "$OUT/program-$variant"
  if sh deploy/run-trial.sh -p "$TASK" -a oracle -o "$OUT/$variant" > "$OUT/$variant.log" 2>&1; then
    printf '%s\n' "$variant: failed native operation completed the program" >&2
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
  output="$(dirname "$result")/agent/oracle.txt"
  grep -q 'OKTA_BEFORE_EXPECTED_ERROR' "$output"
  if grep -q 'OKTA_REACHED_LATER_CALL' "$output"; then
    printf '%s\n' "$variant: a later operation ran after native failure" >&2
    exit 1
  fi
  printf '%s\n' "$variant: actual native failure stops later calls and authoritative assessment"
done
