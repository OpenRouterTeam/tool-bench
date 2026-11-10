#!/bin/sh
set -eu
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
WORLD_IMAGE="${WORLD_IMAGE:?provide the admitted publication world image}"
TOOLS_IMAGE="${TOOLS_IMAGE:?provide the composed publication tools image}"
OUT="$(mktemp -d /tmp/tool-bench-publication.XXXXXX)"
printf 'artifacts: %s\n' "$OUT"
cargo run -q --locked -p scenario --bin taskgen -- --runner-admission required --out "$OUT/tasks" \
  --world-image "$WORLD_IMAGE" --tools-image "$TOOLS_IMAGE" >/dev/null
TASK="$OUT/tasks/business__shopify__relaunch-tote"
for index in 3 4 5; do cp "$TASK/solution/call-$index.json" "$OUT/original-$index.json"; done
for variant in good activation_first activation_only wrong_channel visible_before_price other_color; do
  for index in 3 4 5; do cp "$OUT/original-$index.json" "$TASK/solution/call-$index.json"; done
  reward=0
  case "$variant" in
    good) reward=1 ;;
    activation_first)
      reward=1
      cp "$OUT/original-4.json" "$TASK/solution/call-3.json"
      cp "$OUT/original-3.json" "$TASK/solution/call-4.json"
      ;;
    activation_only)
      jq '.params.name = "get-product-by-id" | .params.arguments = {productId: "${tote_id}"}' \
        "$OUT/original-5.json" > "$TASK/solution/call-5.json"
      ;;
    wrong_channel)
      jq '.params.arguments.publicationIds = ["gid://shopify/Publication/3002"]' \
        "$OUT/original-5.json" > "$TASK/solution/call-5.json"
      ;;
    visible_before_price)
      cp "$OUT/original-5.json" "$TASK/solution/call-3.json"
      cp "$OUT/original-3.json" "$TASK/solution/call-5.json"
      ;;
    other_color)
      jq '.params.arguments.variants += [{id: "gid://shopify/ProductVariant/4202", price: "1.00"}]' \
        "$OUT/original-3.json" > "$TASK/solution/call-3.json"
      ;;
  esac
  sh deploy/run-trial.sh -p "$TASK" -a oracle -o "$OUT/$variant" > "$OUT/$variant.log" 2>&1
  result="$(find "$OUT/$variant" -path '*/verifier/reward.json' -print -quit)"
  test -n "$result"
  outcome=1
  [ "$reward" != 1 ] || outcome=0
  jq -e --argjson reward "$reward" --argjson outcome "$outcome" \
    '.reward == $reward and .outcome_code == $outcome
      and .environment_ok == 1 and .contract_ok == 1 and .integrity_ok == 1
      and .runner_admission_checked == 1 and .runner_admission_ok == 1
      and .adjudication_needed == 0' "$result" >/dev/null
  if [ "$variant" = visible_before_price ]; then
    jq -e '.ordering_ok == 0' "$result" >/dev/null
  fi
  printf '%s\n' "$variant: publication outcome, causal history and admission passed"
done
