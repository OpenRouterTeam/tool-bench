#!/bin/sh
set -eu
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
WORLD_IMAGE="${WORLD_IMAGE:?provide the revision-34 Terraform world image}"
TOOLS_IMAGE="${TOOLS_IMAGE:?provide the declared Terraform tools image}"
OUT="$(mktemp -d /tmp/tool-bench-terraform-errors.XXXXXX)"
printf 'artifacts: %s\n' "$OUT"
cargo run -q --locked -p scenario --bin taskgen -- --runner-admission required --out "$OUT/tasks" \
  --world-image "$WORLD_IMAGE" --tools-image "$TOOLS_IMAGE" >/dev/null
for slug in scale-checkout-staging unlock-orders-prod; do
  mkdir "$OUT/$slug-original"
  cp "$OUT/tasks/code__terraform__$slug/solution/"* "$OUT/$slug-original/"
done
for variant in missing_run missing_variable locked_workspace already_unlocked speculative_apply; do
  slug=scale-checkout-staging
  template=0
  case "$variant" in
    missing_run) tool=get_run_details; arguments='{"run_id":"not-a-real-run"}' ;;
    missing_variable)
      tool=update_workspace_variable
      arguments='{"terraform_org_name":"acme-platform","workspace_name":"checkout-service-staging","variable_id":"not-a-real-variable","key":"desired_count","value":"4"}' ;;
    locked_workspace)
      tool=create_run
      arguments='{"terraform_org_name":"acme-platform","workspace_name":"orders-api-prod","run_type":"plan_only","message":"Inspect locked workspace"}' ;;
    already_unlocked) tool=force_unlock_workspace; arguments='{"workspace_id":"ws-FxCPzmnaeIMOUbSd"}' ;;
    speculative_apply)
      slug=unlock-orders-prod
      tool=action_run
      arguments='{"run_id":"${verification_run}","run_action":"apply","comment":"This speculative run cannot be applied"}'
      template=1 ;;
  esac
  TASK="$OUT/tasks/code__terraform__$slug"
  rm -rf "$TASK/solution"
  mkdir "$TASK/solution"
  cp "$OUT/$slug-original/"* "$TASK/solution/"
  jq -n --arg tool "$tool" --argjson arguments "$arguments" \
    '{jsonrpc:"2.0",id:2,method:"tools/call",params:{name:$tool,arguments:$arguments}}' \
    > "$TASK/solution/error.json"
  printf '%s\n' '[]' > "$TASK/solution/error.capture.json"
  printf '%s\n' 'printf "%s\n" TERRAFORM_BEFORE_EXPECTED_ERROR' >> "$TASK/solution/solve.sh"
  printf 'bound_call 7109 "$DIR/error.json" "$DIR/error.capture.json" %s\n' "$template" \
    >> "$TASK/solution/solve.sh"
  printf '%s\n' 'printf "%s\n" TERRAFORM_REACHED_LATER_CALL' \
    'call 7109 "$DIR/error.json"' >> "$TASK/solution/solve.sh"
  cp -R "$TASK/solution" "$OUT/program-$variant"
  if sh deploy/run-trial.sh -p "$TASK" -a oracle -o "$OUT/$variant" > "$OUT/$variant.log" 2>&1; then
    printf '%s\n' "$variant: native failure falsely completed the program" >&2
    exit 1
  fi
  result="$(find "$OUT/$variant" -name result.json -print -quit)"
  receipt="$(find "$OUT/$variant" -name runner-admission.json -print -quit)"
  test -n "$result" && test -n "$receipt"
  jq -e '.exception_info!=null and .verifier_result==null' "$result" >/dev/null
  jq -e '.phase=="failed" and .failure!=null and .evidence_sha256==null
    and .main_artifacts_sha256==null' "$receipt" >/dev/null
  test -z "$(find "$OUT/$variant" -path '*/verifier/reward.json' -print -quit)"
  test -z "$(find "$OUT/$variant" -path '*/artifacts/world/evidence.json' -print -quit)"
  output="$(dirname "$result")/agent/oracle.txt"
  grep -q TERRAFORM_BEFORE_EXPECTED_ERROR "$output"
  if grep -q TERRAFORM_REACHED_LATER_CALL "$output"; then
    printf '%s\n' "$variant: later call ran after native failure" >&2
    exit 1
  fi
  printf '%s\n' "$variant: native failure stops later calls and authoritative assessment"
done
