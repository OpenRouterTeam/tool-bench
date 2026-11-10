#!/bin/sh
set -eu
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
WORLD_IMAGE="${WORLD_IMAGE:?provide the revision-31 Vercel world image}"
TOOLS_IMAGE="${TOOLS_IMAGE:?provide the declared tools image}"
OUT="$(mktemp -d /tmp/tool-bench-vercel.XXXXXX)"
printf 'artifacts: %s\n' "$OUT"
cargo run -q --locked -p scenario --bin taskgen -- --runner-admission required --out "$OUT/tasks" \
  --world-image "$WORLD_IMAGE" --tools-image "$TOOLS_IMAGE" >/dev/null
for slug in enable-gift-cards-in-previews rotate-checkout-stripe-key rollback-storefront-production; do
  mkdir "$OUT/$slug-original"
  cp "$OUT/tasks/code__vercel__$slug/solution/"* "$OUT/$slug-original/"
done
add() {
  jq -n --arg tool "$1" --argjson arguments "$2" \
    '{jsonrpc:"2.0",id:2,method:"tools/call",params:{name:$tool,arguments:$arguments}}' \
    > "$TASK/solution/control-$calls.json"
  printf '%s\n' "${3:-[]}" > "$TASK/solution/control-$calls.capture.json"
  printf 'bound_call 7110 "$DIR/control-%s.json" "$DIR/control-%s.capture.json" %s\n' \
    "$calls" "$calls" "${4:-0}" >> "$TASK/solution/solve.sh"
  calls=$((calls + 1))
}
variable() {
  add vercel-create-environment-variables "$(jq -cn --arg project "$1" --arg key "$2" \
    --arg value "$3" --argjson targets "$4" --arg kind "$5" \
    '{projectId:$project,environmentVariables:[{key:$key,value:$value,target:$targets,type:$kind}]}')"
}
flags() {
  variable storefront NEXT_PUBLIC_ENABLE_GIFT_CARDS true '["preview","development"]' plain
  variable storefront PAYMENTS_API_URL https://sandbox.pay.acme-shop.com '["preview"]' plain
}
rotate() {
  project="$1"
  add vercel-get-environments "$(jq -cn --arg project "$project" '{idOrName:$project}')" \
    '[{"name":"production_key","pointer":"/content/1/text","json_pointer":"","select":{"matching":{"key":"STRIPE_SECRET_KEY","type":"sensitive"},"pointer":"/id"}}]'
  add vercel-remove-environment-variable \
    "$(jq -cn --arg project "$project" '{idOrName:$project,id:"${production_key}"}')" '[]' 1
  variable "$project" STRIPE_SECRET_KEY "$3" '["production"]' "$2"
}
redeploy() {
  extra="${2:-}"
  if [ -z "$extra" ]; then extra='{}'; fi
  add vercel-create-deployment "$(jq -cn --arg source "$1" --argjson extra "$extra" \
    '{name:"storefront",deploymentId:$source,target:"production"}+$extra')"
}
recover() {
  add vercel-find-project '{"idOrName":"storefront"}' \
    '[{"name":"predecessor","pointer":"/content/1/text","json_pointer":"/lastAliasRequest/fromDeploymentId"}]'
  add vercel-create-deployment '{"name":"storefront","deploymentId":"${predecessor}","target":"production"}' \
    '[{"name":"new_deployment","pointer":"/content/1/text","json_pointer":"/id"}]' 1
  add vercel-get-deployment '{"idOrUrl":"${new_deployment}"}' '[]' 1
}
if [ "$#" -eq 0 ]; then
  set -- flag_reference flag_split_encrypted flag_reverse flag_branch flag_production flag_docs flag_existing_remove \
    rotate_reference rotate_plain rotate_mask rotate_test_changed rotate_other_project \
    rollback_reference rollback_retry rollback_bad rollback_failed rollback_unrouted rollback_preview \
    rollback_spoof rollback_regions rollback_other_project rollback_environment
fi
for variant do
  reward=0
  outcome=1
  calls=0
  case "$variant" in
    flag_*) slug=enable-gift-cards-in-previews ;;
    rotate_*) slug=rotate-checkout-stripe-key ;;
    rollback_*) slug=rollback-storefront-production ;;
    *) printf 'unsupported Vercel variant: %s\n' "$variant" >&2; exit 2 ;;
  esac
  TASK="$OUT/tasks/code__vercel__$slug"
  rm -rf "$TASK/solution"
  mkdir "$TASK/solution"
  cp "$OUT/$slug-original/"* "$TASK/solution/"
  cp "$OUT/rotate-checkout-stripe-key-original/"*.jq "$TASK/solution/"
  sed -e '/^bound_call [0-9]/,$d' -e '/^call [0-9]/,$d' \
    "$OUT/rotate-checkout-stripe-key-original/solve.sh" > "$TASK/solution/solve.sh"
  case "$variant" in
    flag_reference) flags; reward=1; outcome=0 ;;
    flag_split_encrypted)
      variable storefront NEXT_PUBLIC_ENABLE_GIFT_CARDS true '["development"]' encrypted
      variable storefront NEXT_PUBLIC_ENABLE_GIFT_CARDS true '["preview"]' encrypted
      variable storefront PAYMENTS_API_URL https://sandbox.pay.acme-shop.com '["preview"]' encrypted
      reward=1; outcome=0 ;;
    flag_reverse)
      variable storefront PAYMENTS_API_URL https://sandbox.pay.acme-shop.com '["preview"]' plain
      variable storefront NEXT_PUBLIC_ENABLE_GIFT_CARDS true '["development","preview"]' plain
      reward=1; outcome=0 ;;
    flag_branch)
      add vercel-create-environment-variables '{"projectId":"storefront","environmentVariables":[
        {"key":"NEXT_PUBLIC_ENABLE_GIFT_CARDS","value":"true","target":["preview"],"gitBranch":"feat/gift-cards","type":"plain"},
        {"key":"NEXT_PUBLIC_ENABLE_GIFT_CARDS","value":"true","target":["development"],"type":"plain"},
        {"key":"PAYMENTS_API_URL","value":"https://sandbox.pay.acme-shop.com","target":["preview"],"type":"plain"}]}' ;;
    flag_production)
      flags
      variable storefront NEXT_PUBLIC_ENABLE_GIFT_CARDS true '["production"]' plain ;;
    flag_docs)
      flags
      variable docs UNREQUESTED_SETTING changed '["production"]' plain ;;
    flag_existing_remove)
      add vercel-remove-environment-variable '{"idOrName":"storefront","id":"9oK7J2kVMUskD5Tg"}'
      flags ;;
    rotate_reference)
      rotate checkout-api-next sensitive sk_live_51NvCheckoutRot8Hq2
      reward=1; outcome=0 ;;
    rotate_plain) rotate checkout-api-next plain sk_live_51NvCheckoutRot8Hq2 ;;
    rotate_mask) rotate checkout-api-next sensitive '********' ;;
    rotate_test_changed)
      rotate checkout-api-next sensitive sk_live_51NvCheckoutRot8Hq2
      add vercel-remove-environment-variable '{"idOrName":"checkout-api-next","id":"JSGgKKujZeGcSdw8"}'
      variable checkout-api-next STRIPE_SECRET_KEY different-test-key '["preview","development"]' encrypted ;;
    rotate_other_project)
      variable docs UNREQUESTED_SETTING changed '["production"]' plain
      rotate checkout-api-next sensitive sk_live_51NvCheckoutRot8Hq2 ;;
    rollback_reference) recover; reward=1; outcome=0 ;;
    rollback_retry)
      recover
      redeploy dpl_8o704GjcwumkLAaxPwjoxb4V6QDF '{"meta":{"recoveryAttempt":"reconciled-retry"}}'
      reward=1; outcome=0 ;;
    rollback_bad) redeploy dpl_7wC1vKiBnuwDgHLXzdLRklqAYci5 ;;
    rollback_failed) redeploy dpl_PHkhNPm61EU1aF79kakZb9UhQNNQ ;;
    rollback_unrouted)
      redeploy dpl_8o704GjcwumkLAaxPwjoxb4V6QDF '{"autoAssignCustomDomains":false}' ;;
    rollback_preview) redeploy dpl_8o704GjcwumkLAaxPwjoxb4V6QDF '{"target":"preview"}' ;;
    rollback_spoof)
      redeploy dpl_7wC1vKiBnuwDgHLXzdLRklqAYci5 \
        '{"meta":{"githubCommitSha":"c3a9f12e8b47d05a6e21f9b3c7d84e50a1b6f2d9"}}' ;;
    rollback_regions) redeploy dpl_8o704GjcwumkLAaxPwjoxb4V6QDF '{"regions":["sfo1"]}' ;;
    rollback_other_project)
      recover
      add vercel-create-deployment '{"name":"checkout-api","deploymentId":"dpl_1ijIq6r015f3b89pqQsLeT07whaO","target":"preview"}' ;;
    rollback_environment)
      recover
      variable storefront UNREQUESTED_SETTING changed '["production"]' plain ;;
    *) printf 'unsupported Vercel variant: %s\n' "$variant" >&2; exit 2 ;;
  esac
  cp -R "$TASK/solution" "$OUT/program-$variant"
  sh deploy/run-trial.sh -p "$TASK" -a oracle -o "$OUT/$variant" > "$OUT/$variant.log" 2>&1
  result="$(find "$OUT/$variant" -path '*/verifier/reward.json' -print -quit)"
  test -n "$result"
  jq -e --argjson reward "$reward" --argjson outcome "$outcome" '
    .reward==$reward and .outcome_code==$outcome and .evaluator_version==31
      and .environment_ok==1 and .contract_ok==1 and .integrity_ok==1
      and .adjudication_needed==0 and .runner_admission_checked==1 and .runner_admission_ok==1
      and .preservation_checked==1 and .history_checked==1' "$result" >/dev/null
  printf '%s\n' "$variant: replica control reward=$reward outcome=$outcome"
done
