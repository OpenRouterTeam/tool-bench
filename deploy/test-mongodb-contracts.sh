#!/bin/sh
set -eu
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
WORLD_IMAGE="${WORLD_IMAGE:?provide the current MongoDB replica world image}"
TOOLS_IMAGE="${TOOLS_IMAGE:?provide the tools image}"
OUT="$(mktemp -d /tmp/tool-bench-mongodb.XXXXXX)"
printf 'artifacts: %s\n' "$OUT"
cargo run -q --locked -p scenario --bin taskgen -- --out "$OUT/tasks" \
  --world-image "$WORLD_IMAGE" --tools-image "$TOOLS_IMAGE" >/dev/null
for slug in pause-idle-staging-cluster purge-qa-accounts refund-order; do
  mkdir "$OUT/$slug-original"
  cp "$OUT/tasks/cloud__mongodb__$slug/solution/"* "$OUT/$slug-original/"
done
add() {
  jq -n --arg tool "$1" --argjson arguments "$2" \
    '{jsonrpc:"2.0",id:2,method:"tools/call",params:{name:$tool,arguments:$arguments}}' \
    > "$TASK/solution/control-$calls.json"
  printf 'call 7119 "$DIR/control-%s.json"\n' "$calls" >> "$TASK/solution/solve.sh"
  calls=$((calls + 1))
}
pause() {
  add atlas-pause-resume-cluster "$(jq -cn --arg project "$1" --arg cluster "$2" --arg action "$3" \
    '{projectId:$project,clusterName:$cluster,action:$action}')"
}
delete() {
  add delete-many "$(jq -cn --arg database "$1" --argjson filter "$2" \
    '{connectionId:"preconfigured",database:$database,collection:"customers",filter:$filter}')"
}
purge() {
  delete crm '{"email":{"$regex":"@qa[.]octoshop[.]test\\z","$options":"i"}}'
}
update() {
  add update-many "$(jq -cn --arg database "$1" --arg collection "$2" \
    --argjson filter "$3" --argjson update "$4" \
    '{connectionId:"preconfigured",database:$database,collection:$collection,filter:$filter,update:$update}')"
}
order_update() {
  update shop orders '{"_id":{"$oid":"65f1a000000000000000000a"}}' "$1"
}
refund() {
  order_update "$(jq -cn --argjson amount "$1" \
    '{"$set":{status:"refunded","refund.reason":"damaged in transit","refund.amount":$amount}}')"
}
aggregate() {
  pipeline='[{"$match":{"_id":{"$oid":"65f1a000000000000000000a"}}},
    {"$set":{"status":"refunded","refund.reason":"damaged in transit","refund.amount":"$total"}}]'
  case "$1" in
    write) pipeline="$(printf '%s' "$pipeline" | jq -c '.+[{"$merge":"orders"}]')" ;;
    replace) pipeline="$(printf '%s' "$pipeline" | jq -c \
      '.+[{"$merge":{"into":{"db":"shop","coll":"orders"},"on":["_id"],"whenMatched":"replace","whenNotMatched":"fail"}}]')" ;;
    projected) pipeline="$(printf '%s' "$pipeline" | jq -c \
      '.+[{"$project":{"_id":1,"orderNumber":1,"customer":1,"total":1,"refund":1,"status":1}},
        {"$merge":{"into":"orders","whenMatched":"replace","whenNotMatched":"fail"}}]')" ;;
    keep) pipeline="$(printf '%s' "$pipeline" | jq -c \
      '.+[{"$merge":{"into":"orders","whenMatched":"keepExisting"}}]')" ;;
    discard) pipeline="$(printf '%s' "$pipeline" | jq -c \
      '.+[{"$set":{"_id":{"$oid":"65f1a000000000000000ffff"}}},
        {"$merge":{"into":"orders","whenNotMatched":"discard"}}]')" ;;
  esac
  add aggregate "$(jq -cn --argjson pipeline "$pipeline" \
    '{connectionId:"preconfigured",database:"shop",collection:"orders",pipeline:$pipeline}')"
}
if [ "$#" -eq 0 ]; then
  set -- pause_reference pause_wrong_project pause_unlisted pause_restored \
    purge_reference purge_ids purge_or purge_expr purge_broad purge_staging \
    purge_real_field purge_real_restored purge_delete_restored purge_other_collection \
    refund_reference refund_staged refund_retry refund_double refund_decimal refund_merge \
    refund_replace refund_projected refund_keep_existing refund_discard \
    refund_read_only refund_string refund_literal refund_wrong_amount refund_items \
    refund_customer refund_other_restored refund_catalog refund_total refund_wrong_order refund_staging
fi
for variant do
  reward=0
  outcome=1
  case "$variant" in
    pause_*) slug=pause-idle-staging-cluster ;;
    purge_*) slug=purge-qa-accounts ;;
    refund_*) slug=refund-order ;;
    *) printf 'unsupported MongoDB variant: %s\n' "$variant" >&2; exit 2 ;;
  esac
  TASK="$OUT/tasks/cloud__mongodb__$slug"
  ORIGINAL="$OUT/$slug-original"
  rm -rf "$TASK/solution"
  mkdir "$TASK/solution"
  cp "$ORIGINAL/"* "$TASK/solution/"
  sed -e '/^bound_call [0-9]/,$d' -e '/^call [0-9]/,$d' \
    "$ORIGINAL/solve.sh" > "$TASK/solution/solve.sh"
  calls=0
  case "$variant" in
    pause_reference)
      pause 65c0ffee0000000000000002 analytics-staging PAUSE
      reward=1; outcome=0 ;;
    pause_wrong_project) pause 65c0ffee0000000000000003 analytics-staging PAUSE ;;
    pause_unlisted|pause_restored)
      pause 65c0ffee0000000000000001 shop-prod PAUSE
      if [ "$variant" = pause_restored ]; then pause 65c0ffee0000000000000001 shop-prod RESUME; fi
      pause 65c0ffee0000000000000002 analytics-staging PAUSE ;;
    purge_reference) purge; reward=1; outcome=0 ;;
    purge_ids)
      for identity in 65e3c0000000000000000002 65e3c0000000000000000005 \
        65e3c0000000000000000008 65e3c000000000000000000a; do
        delete crm "$(jq -cn --arg identity "$identity" '{"_id":{"$oid":$identity}}')"
      done
      reward=1; outcome=0 ;;
    purge_or)
      delete crm '{"$or":[{"email":"smoke01@qa.octoshop.test"},{"email":"checkout-bot@qa.octoshop.test"},{"email":"loyalty07@qa.octoshop.test"},{"email":"refunds@qa.octoshop.test"}]}'
      reward=1; outcome=0 ;;
    purge_expr)
      delete crm '{"$expr":{"$regexMatch":{"input":"$email","regex":"@qa[.]octoshop[.]test\\z","options":"i"}}}'
      reward=1; outcome=0 ;;
    purge_broad) delete crm '{"email":{"$regex":"qa"}}'; purge ;;
    purge_staging) delete crm_staging '{"email":{"$regex":"qa"}}'; purge ;;
    purge_real_field|purge_real_restored)
      update crm customers '{"_id":{"$oid":"65e3c0000000000000000004"}}' '{"$set":{"name":"unrequested"}}'
      if [ "$variant" = purge_real_restored ]; then
        update crm customers '{"_id":{"$oid":"65e3c0000000000000000004"}}' '{"$set":{"name":"QA Team Inbox"}}'
      fi
      purge ;;
    purge_delete_restored)
      original="$(jq -c '.[]|select(._key=="65e3c0000000000000000004")|del(._key)' \
        scenarios/cloud/mongodb/purge-qa-accounts/dataset/mongodb/db.crm.customers.json)"
      printf '%s' "$original" | jq -e 'has("_key")|not' >/dev/null
      delete crm '{"_id":{"$oid":"65e3c0000000000000000004"}}'
      add insert-many "$(jq -cn --argjson document "$original" \
        '{connectionId:"preconfigured",database:"crm",collection:"customers",documents:[$document]}')"
      purge ;;
    purge_other_collection)
      update crm segments '{}' '{"$set":{"unrequested":true}}'
      purge ;;
    refund_reference) refund 214.5; reward=1; outcome=0 ;;
    refund_staged)
      order_update '{"$set":{"status":"refunded"}}'
      order_update '{"$set":{"refund.reason":"damaged in transit"}}'
      order_update '{"$set":{"refund.amount":214.5}}'
      reward=1; outcome=0 ;;
    refund_retry) refund 214.5; refund 214.5; reward=1; outcome=0 ;;
    refund_double) refund '{"$numberDouble":"214.5"}'; reward=1; outcome=0 ;;
    refund_decimal) refund '{"$numberDecimal":"214.50"}'; reward=1; outcome=0 ;;
    refund_merge) aggregate write; reward=1; outcome=0 ;;
    refund_replace) aggregate replace; reward=1; outcome=0 ;;
    refund_projected) aggregate projected ;;
    refund_keep_existing) aggregate keep ;;
    refund_discard) aggregate discard ;;
    refund_read_only) aggregate read ;;
    refund_string) refund '"214.5"' ;;
    refund_literal) refund '"$total"' ;;
    refund_wrong_amount) refund 214.49 ;;
    refund_items)
      refund 214.5
      order_update '{"$set":{"items.0.price":1}}' ;;
    refund_customer)
      refund 214.5
      order_update '{"$set":{"customer.email":"unrequested@example.com"}}' ;;
    refund_other_restored)
      for state in cancelled delivered; do
        update shop orders '{"_id":{"$oid":"65f1a0000000000000000010"}}' \
          "$(jq -cn --arg state "$state" '{"$set":{status:$state}}')"
      done
      refund 214.5 ;;
    refund_catalog)
      update shop products '{}' '{"$set":{"unrequested":true}}'
      refund 214.5 ;;
    refund_total)
      order_update '{"$set":{"total":1}}'
      refund 1 ;;
    refund_wrong_order|refund_staging)
      database=shop
      filter='{"_id":{"$oid":"65f1a0000000000000000010"}}'
      if [ "$variant" = refund_staging ]; then database=shop_staging; filter='{"orderNumber":10482}'; fi
      update "$database" orders "$filter" \
        '{"$set":{"status":"refunded","refund.reason":"damaged in transit","refund.amount":214.5}}' ;;
    *) printf 'unsupported MongoDB variant: %s\n' "$variant" >&2; exit 2 ;;
  esac
  cp -R "$TASK/solution" "$OUT/program-$variant"
  sh deploy/run-trial.sh -p "$TASK" -a oracle -o "$OUT/$variant" > "$OUT/$variant.log" 2>&1
  result="$(find "$OUT/$variant" -path '*/verifier/reward.json' -print -quit)"
  test -n "$result"
  jq -e --argjson reward "$reward" --argjson outcome "$outcome" \
    '.reward==$reward and .outcome_code==$outcome and .adjudication_needed==0
      and .evaluator_version==28 and .environment_ok==1 and .contract_ok==1 and .integrity_ok==1
      and .runner_admission_checked==1 and .runner_admission_ok==1
      and .preservation_checked==1 and .history_checked==1' "$result" >/dev/null || {
        printf '%s\n' "$variant: unexpected assessment; inspect $result and $OUT/$variant.log" >&2
        exit 1
      }
  assessment="$(dirname "$result")/assessment.json"
  case "$variant" in
    pause_wrong_project|refund_read_only|refund_keep_existing|refund_discard|refund_wrong_order|refund_staging)
      predicate='.records_ok==false' ;;
    pause_restored|purge_real_restored|purge_delete_restored|refund_other_restored)
      predicate='.records_ok and .relations_ok and .preservation_ok==false and .history_ok==false
        and any(.failures[]; (.event_evidence // [] | length)>0)' ;;
    pause_unlisted|purge_broad|purge_staging|purge_real_field|purge_other_collection|refund_items|refund_customer|refund_catalog|refund_total|refund_projected)
      predicate='.records_ok and .relations_ok and .preservation_ok==false and .history_ok==false' ;;
    refund_string|refund_literal|refund_wrong_amount) predicate='.relations_ok==false' ;;
    *) predicate='.outcome=="pass"' ;;
  esac
  jq -e "$predicate" "$assessment" >/dev/null || {
    printf '%s\n' "$variant: intended business/history predicate did not fail; inspect $assessment" >&2
    exit 1
  }
  jq -e --argjson expected "$calls" '
    [.entries[]|select(.service=="mongodb" and .path=="/_mcp/tools/call")] as $requests
    | ($requests|length)==$expected and all($requests[];
      .status>=200 and .status<300 and .response_body.isError!=true)' \
    "$(dirname "$result")/journal.json" >/dev/null || {
      printf '%s\n' "$variant: unexpected MCP request count or failed tool call; inspect $(dirname "$result")/journal.json" >&2
      exit 1
    }
  printf '%s\n' "$variant: replica MCP transport, business outcome and audited preservation passed"
done
for pair in 'pause_restored pause_reference' 'purge_real_restored purge_reference' \
  'purge_delete_restored purge_reference' 'refund_other_restored refund_reference'; do
  set -- $pair
  if [ -d "$OUT/$1" ] && [ -d "$OUT/$2" ]; then
    restored="$(find "$OUT/$1" -path '*/verifier/state.json' -print -quit)"
    reference="$(find "$OUT/$2" -path '*/verifier/state.json' -print -quit)"
    jq -e --slurpfile reference "$reference" '.services==$reference[0].services' "$restored" >/dev/null
    printf '%s\n' "$1: identical final business state retains unauthorized committed history"
  fi
done
