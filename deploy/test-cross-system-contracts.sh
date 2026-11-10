#!/bin/sh
set -eu
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
WORLD_IMAGE="${WORLD_IMAGE:?provide the current cross-system world image}"
TOOLS_IMAGE="${TOOLS_IMAGE:?provide the tools image}"
OUT="$(mktemp -d /tmp/tool-bench-cross-system.XXXXXX)"
printf 'artifacts: %s\n' "$OUT"
cargo run -q --locked -p scenario --bin taskgen -- --runner-admission required --out "$OUT/tasks" \
  --world-image "$WORLD_IMAGE" --tools-image "$TOOLS_IMAGE" >/dev/null
for identity in business__multi__hubspot-won-to-salesforce-opportunity \
  business__multi__shopify-vip-stripe-coupon search__multi__es-duplicate-charge-stripe-refund; do
  mkdir "$OUT/$identity-original"
  cp "$OUT/tasks/$identity/solution/"* "$OUT/$identity-original/"
done
CAPTURING="$OUT/tasks/business__multi__zendesk-refund-via-stripe/solution"
make_call() {
  jq -n --arg name "$2" --argjson arguments "$3" \
    '{jsonrpc:"2.0",id:2,method:"tools/call",params:{name:$name,arguments:$arguments}}' \
    > "$TASK/solution/call-$1.json"
}
if [ "$#" -eq 0 ]; then
  set -- crm_reference crm_numeric crm_staged crm_association crm_premature crm_wrong_amount \
    crm_wrong_deal crm_collateral crm_bare crm_denied crm_restored \
    vip_reversed vip_alias vip_lost_tag vip_tag_substring vip_wrong_coupon vip_gift vip_extra_gift vip_contact \
    refund_split refund_intent refund_partial refund_wrong_reason refund_original refund_extra_original
fi
for variant do
  reward=0
  outcome=1
  adjudication=0
  case "$variant" in
    crm_*) identity=business__multi__hubspot-won-to-salesforce-opportunity; last=3 ;;
    vip_*) identity=business__multi__shopify-vip-stripe-coupon; last=5 ;;
    refund_*) identity=search__multi__es-duplicate-charge-stripe-refund; last=3 ;;
    *) printf 'unsupported cross-system case: %s\n' "$variant" >&2; exit 2 ;;
  esac
  ORIGINAL="$OUT/$identity-original"
  TASK="$OUT/tasks/$identity"
  rm -rf "$TASK/solution"
  mkdir "$TASK/solution"
  cp "$ORIGINAL/"* "$TASK/solution/"
  cp "$CAPTURING/"*.jq "$TASK/solution/"
  sed '/^bound_call [0-9]/,$d' "$CAPTURING/solve.sh" > "$TASK/solution/solve.sh"
  for index in 0 1 2 3 4 5 6; do printf '%s\n' '[]' > "$TASK/solution/call-$index.capture.json"; done
  case "$variant" in
    crm_reference) reward=1; outcome=0 ;;
    crm_numeric)
      reward=1; outcome=0
      jq '.params.arguments.records[0].Amount = "176500.00"
        | .params.arguments.records[0].Id = "0065g00000IDWm1"' \
        "$ORIGINAL/call-2.json" > "$TASK/solution/call-2.json"
      jq '.params.arguments.metadata.body = "Synced to Salesforce opportunity 0065g00000IDWm1."' \
        "$ORIGINAL/call-3.json" > "$TASK/solution/call-3.json"
      ;;
    crm_staged)
      reward=1; outcome=0; last=5
      jq '.params.arguments.records[0] |= {Id,CloseDate}' "$ORIGINAL/call-2.json" > "$TASK/solution/call-2.json"
      jq '.params.arguments.records[0] |= {Id,Amount}' "$ORIGINAL/call-2.json" > "$TASK/solution/call-3.json"
      jq '.params.arguments.records[0] |= {Id,StageName}' "$ORIGINAL/call-2.json" > "$TASK/solution/call-4.json"
      cp "$ORIGINAL/call-3.json" "$TASK/solution/call-5.json"
      ;;
    crm_association)
      reward=1; outcome=0; last=5
      make_call 3 hubspot-batch-create-objects '{"objectType":"notes","inputs":[{"properties":{"hs_timestamp":"2026-10-29T14:00:00Z","hs_note_body":""}}]}'
      printf '%s\n' '[{"name":"actual_note","pointer":"/content/0/text","json_pointer":"/results/0/id"}]' > "$TASK/solution/call-3.capture.json"
      make_call 4 hubspot-batch-update-objects '{"objectType":"notes","inputs":[{"id":"${actual_note}","properties":{"hs_note_body":"<p>Synced to Salesforce opportunity <strong>0065g00000IDWm1AAH</strong>.</p>"}}]}'
      make_call 5 hubspot-batch-create-associations '{"fromObjectType":"notes","toObjectType":"deals","types":[{"associationCategory":"HUBSPOT_DEFINED","associationTypeId":214}],"inputs":[{"from":{"id":"${actual_note}"},"to":{"id":"9100104"}}]}'
      ;;
    crm_premature)
      cp "$ORIGINAL/call-3.json" "$TASK/solution/call-2.json"
      cp "$ORIGINAL/call-2.json" "$TASK/solution/call-3.json"
      ;;
    crm_wrong_amount)
      jq '.params.arguments.records[0].Amount = 184000' "$ORIGINAL/call-2.json" > "$TASK/solution/call-2.json"
      ;;
    crm_wrong_deal)
      jq '.params.arguments.associations.dealIds = [9100105]' "$ORIGINAL/call-3.json" > "$TASK/solution/call-3.json"
      ;;
    crm_collateral)
      last=4
      jq '.params.arguments.records[0] = {Id:"0065g00000IRdw1AAD",StageName:"Closed Won"}' \
        "$ORIGINAL/call-2.json" > "$TASK/solution/call-4.json"
      ;;
    crm_bare|crm_denied|crm_restored)
      outcome=6; adjudication=1
      body="The Salesforce opportunity 0065g00000IDWm1AAH has NOT been synced."
      if [ "$variant" = crm_bare ]; then body="0065g00000IDWm1AAH"; fi
      if [ "$variant" = crm_restored ]; then
        last=5
        printf '%s\n' '[{"name":"actual_note","pointer":"/content/0/text","json_pointer":"/engagement/engagement/id"}]' > "$TASK/solution/call-3.capture.json"
        make_call 4 hubspot-update-engagement '{"engagementId":"${actual_note}","metadata":{"body":"The Salesforce opportunity 0065g00000IDWm1AAH has NOT been synced."},"associations":{"dealIds":[9100104]}}'
        make_call 5 hubspot-update-engagement '{"engagementId":"${actual_note}","metadata":{"body":"Synced to Salesforce: opportunity 0065g00000IDWm1AAH marked Closed Won ($176,500, closed 2026-10-28)."},"associations":{"dealIds":[9100104]}}'
      else
        jq --arg body "$body" '.params.arguments.metadata.body = $body' "$ORIGINAL/call-3.json" > "$TASK/solution/call-3.json"
      fi
      ;;
    vip_reversed)
      reward=1; outcome=0
      cp "$ORIGINAL/call-5.json" "$TASK/solution/call-1.json"
      jq '.params.arguments.tags = ["NEWSLETTER","vip"]' "$ORIGINAL/call-1.json" > "$TASK/solution/call-5.json"
      ;;
    vip_alias)
      reward=1; outcome=0
      jq '.params.arguments.parameters |= (.subscription_exposed_id = .id | del(.id))' \
        "$ORIGINAL/call-5.json" > "$TASK/solution/call-5.json"
      ;;
    vip_lost_tag|vip_tag_substring)
      tags='["VIP"]'
      if [ "$variant" = vip_tag_substring ]; then tags='["newsletter","vip-review"]'; fi
      jq --argjson tags "$tags" '.params.arguments.tags = $tags' "$ORIGINAL/call-1.json" > "$TASK/solution/call-1.json"
      ;;
    vip_wrong_coupon)
      jq '.params.arguments.parameters.discounts[0].coupon = "WELCOME10"' "$ORIGINAL/call-5.json" > "$TASK/solution/call-5.json"
      ;;
    vip_gift)
      jq '.params.arguments.parameters.id = "sub_PrRmOutlkGift00000000001"' "$ORIGINAL/call-5.json" > "$TASK/solution/call-5.json"
      ;;
    vip_extra_gift)
      last=6
      jq '.params.arguments.parameters.id = "sub_PrRmOutlkGift00000000001"' "$ORIGINAL/call-5.json" > "$TASK/solution/call-6.json"
      ;;
    vip_contact)
      last=6
      jq '.params.arguments.stripe_api_operation_id = "PostCustomersCustomer"
        | .params.arguments.parameters = {id:"cus_PrRmOutlk01Ab",email:"changed@example.org"}' \
        "$ORIGINAL/call-5.json" > "$TASK/solution/call-6.json"
      ;;
    refund_split)
      reward=1; outcome=0; last=4
      jq '.params.arguments.parameters.amount = 4000' "$ORIGINAL/call-3.json" > "$TASK/solution/call-3.json"
      jq '.params.arguments.parameters.amount = 4640' "$ORIGINAL/call-3.json" > "$TASK/solution/call-4.json"
      ;;
    refund_intent)
      reward=1; outcome=0
      jq '.params.arguments.parameters |= (del(.charge) | .payment_intent = "pi_3Qb2MjLee109003bXz4aE")' \
        "$ORIGINAL/call-3.json" > "$TASK/solution/call-3.json"
      ;;
    refund_partial)
      jq '.params.arguments.parameters.amount = 8639' "$ORIGINAL/call-3.json" > "$TASK/solution/call-3.json"
      ;;
    refund_wrong_reason)
      jq '.params.arguments.parameters.reason = "requested_by_customer"' "$ORIGINAL/call-3.json" > "$TASK/solution/call-3.json"
      ;;
    refund_original)
      jq '.params.arguments.parameters.charge = "ch_3Qb2KjLee109002aPo7tB"' "$ORIGINAL/call-3.json" > "$TASK/solution/call-3.json"
      ;;
    refund_extra_original)
      last=4
      jq '.params.arguments.parameters.charge = "ch_3Qb2KjLee109002aPo7tB"
        | .params.arguments.parameters.amount = 1' "$ORIGINAL/call-3.json" > "$TASK/solution/call-4.json"
      ;;
  esac
  index=0
  while [ "$index" -le "$last" ]; do
    template=0
    case "$variant:$index" in crm_association:4|crm_association:5|crm_restored:4|crm_restored:5) template=1 ;; esac
    tool="$(jq -r '.params.name' "$TASK/solution/call-$index.json")"
    case "$tool" in
      hubspot-*) endpoint=7123 ;;
      salesforce_*) endpoint=7122 ;;
      stripe_api_*) endpoint=7121 ;;
      get-customers|update-customer) endpoint=7125 ;;
      list_indices|search) endpoint=7106 ;;
      *) printf 'unmapped tool: %s\n' "$tool" >&2; exit 2 ;;
    esac
    printf 'bound_call %s "$DIR/call-%s.json" "$DIR/call-%s.capture.json" %s\n' \
      "$endpoint" "$index" "$index" "$template" >> "$TASK/solution/solve.sh"
    index=$((index+1))
  done
  cp -R "$TASK/solution" "$OUT/program-$variant"
  sh deploy/run-trial.sh -p "$TASK" -a oracle -o "$OUT/$variant" > "$OUT/$variant.log" 2>&1
  result="$(find "$OUT/$variant" -path '*/verifier/reward.json' -print -quit)"
  test -n "$result"
  jq -e --argjson reward "$reward" --argjson outcome "$outcome" --argjson adjudication "$adjudication" \
    '.reward == $reward and .outcome_code == $outcome and .adjudication_needed == $adjudication
      and .evaluator_version == 27 and .environment_ok == 1 and .contract_ok == 1 and .integrity_ok == 1
      and .runner_admission_checked == 1 and .runner_admission_ok == 1
      and .preservation_checked == 1 and .history_checked == 1' "$result" >/dev/null
  printf '%s\n' "$variant: cross-system outcome, identity, history and preservation admission passed"
done
if [ -d "$OUT/crm_restored" ] && [ -d "$OUT/crm_reference" ]; then
  reference="$(find "$OUT/crm_reference" -path '*/verifier/state.json' -print -quit)"
  restored="$(find "$OUT/crm_restored" -path '*/verifier/state.json' -print -quit)"
  jq -e --slurpfile reference "$reference" '.services == $reference[0].services' "$restored" >/dev/null
  if cargo run -q --locked -p report --bin bench-report -- --require-attested "$OUT/crm_restored" "$OUT/crm_reference" \
    --allow-agent-difference --json > "$OUT/comparison.json" 2> "$OUT/comparison.log"; then
    printf '%s\n' 'restored sync contradiction produced a final comparison' >&2
    exit 1
  fi
  test ! -s "$OUT/comparison.json"
  grep -q 'require adjudication' "$OUT/comparison.log"
  printf '%s\n' 'restored sync communication remains unresolved and cannot publish a final comparison'
fi
