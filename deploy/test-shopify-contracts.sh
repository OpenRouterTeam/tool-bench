#!/bin/sh
set -eu
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
WORLD_IMAGE="${WORLD_IMAGE:?provide the current world image}"
TOOLS_IMAGE="${TOOLS_IMAGE:?provide the tools image}"
OUT="$(mktemp -d /tmp/tool-bench-shopify.XXXXXX)"
printf 'artifacts: %s\n' "$OUT"
cargo run -q --locked -p scenario --bin taskgen -- --out "$OUT/tasks" \
  --world-image "$WORLD_IMAGE" --tools-image "$TOOLS_IMAGE" >/dev/null
NOTE="$OUT/tasks/business__shopify__concierge-note"
VIP="$OUT/tasks/business__shopify__tag-vip-latest-order"
cp "$NOTE/solution/call-2.json" "$OUT/note.json"
cp "$VIP/solution/call-2.json" "$OUT/customer-tags.json"
cp "$VIP/solution/call-3.json" "$OUT/order-tags.json"
for variant in note_whitespace note_negated vip_reordered vip_removal vip_extra activation_only; do
  reward=0
  outcome=1
  environment=1
  case "$variant" in
    note_whitespace|note_negated)
      task="$NOTE"
      note='Do not Leave with building concierge'
      if [ "$variant" = note_whitespace ]; then
        note=' Leave with
building concierge '
        reward=1
        outcome=0
      fi
      jq --arg note "$note" '.params.arguments.note = $note' "$OUT/note.json" > "$NOTE/solution/call-2.json"
      ;;
    vip_reordered|vip_removal|vip_extra)
      task="$VIP"
      customer='["newsletter","wholesale","VIP"]'
      if [ "$variant" = vip_reordered ]; then
        customer='["vIp","wholesale","newsletter"]'
        reward=1
        outcome=0
      elif [ "$variant" = vip_removal ]; then
        customer='["VIP"]'
      else
        customer='["newsletter","wholesale","VIP","unrequested"]'
      fi
      jq --argjson tags "$customer" '.params.arguments.tags = $tags' \
        "$OUT/customer-tags.json" > "$VIP/solution/call-2.json"
      jq '.params.arguments.tags = ["VIP","gift"]' \
        "$OUT/order-tags.json" > "$VIP/solution/call-3.json"
      ;;
    activation_only)
      task="$OUT/tasks/business__shopify__relaunch-tote"
      jq '.params.name = "get-product-by-id" | .params.arguments = {productId: "gid://shopify/Product/8201"}' \
        "$task/solution/call-5.json" > "$OUT/no-publication.json"
      cp "$OUT/no-publication.json" "$task/solution/call-5.json"
      ;;
  esac
  sh deploy/run-trial.sh -p "$task" -a oracle -o "$OUT/$variant" > "$OUT/$variant.log" 2>&1
  result="$(find "$OUT/$variant" -path '*/verifier/reward.json' -print -quit)"
  test -n "$result"
  jq -e --argjson reward "$reward" --argjson outcome "$outcome" --argjson environment "$environment" \
    '.reward == $reward and .outcome_code == $outcome
      and .environment_ok == $environment and .contract_ok == 1
      and .integrity_ok == 1 and .adjudication_needed == 0
      and .runner_admission_checked == 1 and .runner_admission_ok == 1' "$result" >/dev/null
  if [ "$variant" = activation_only ]; then
    evidence="$(find "$OUT/$variant" -path '*/artifacts/world/evidence.json' -print -quit)"
    jq -e '.state.services.shopify.products["8201"].status == "ACTIVE"
      and .state.services.shopify.products["8201"].publishedAt == null' "$evidence" >/dev/null
  fi
  printf '%s\n' "$variant: Shopify outcome, environment and runner checks passed"
done
cargo run -q --locked -p report --bin bench-report -- \
    "$OUT/activation_only" "$OUT/activation_only" --json \
    > "$OUT/comparison.json" 2> "$OUT/comparison.log"
jq -e '.paired_tasks == 1 and .mean_a == 0 and .mean_b == 0' "$OUT/comparison.json" >/dev/null
printf '%s\n' 'activation-only is a proven task failure under the genuinely capable condition'
