#!/bin/sh
set -eu
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
WORLD_IMAGE="${WORLD_IMAGE:?provide the current Zendesk world image}"
TOOLS_IMAGE="${TOOLS_IMAGE:?provide the tools image}"
OUT="$(mktemp -d /tmp/tool-bench-zendesk.XXXXXX)"
printf 'artifacts: %s\n' "$OUT"
cargo run -q --locked -p scenario --bin taskgen -- --runner-admission required --out "$OUT/tasks" \
  --world-image "$WORLD_IMAGE" --tools-image "$TOOLS_IMAGE" >/dev/null
REPLY='Marcus, billing has issued the refund for the duplicate October charge. Reference RF-88213; it should reach your card within 5-7 business days.'
for task in create-ticket-for-caller escalate-outage-ticket reply-refund-and-solve; do
  mkdir "$OUT/$task-original"
  cp "$OUT/tasks/business__zendesk__$task/solution/"*.json "$OUT/$task-original/"
done
for variant in refund_formatting refund_atomic refund_premature refund_private \
    refund_wrong_ticket refund_reference_only refund_contradiction refund_hidden \
    escalation_atomic escalation_tag_loss escalation_reference_only \
    create_alternative create_wrong_requester create_place_only; do
  reward=0
  outcome=1
  adjudication=0
  task=reply-refund-and-solve
  case "$variant" in
    escalation_*) task=escalate-outage-ticket ;;
    create_*) task=create-ticket-for-caller ;;
  esac
  TASK="$OUT/tasks/business__zendesk__$task"
  cp "$OUT/$task-original/"*.json "$TASK/solution/"
  case "$variant" in
    refund_formatting)
      reward=1
      outcome=0
      jq --arg body "Marcus, billing has issued the refund for the duplicate October charge. Reference **RF-88213**; it should reach your card within 5-7 business days." \
        '.params.arguments.comment = $body' "$OUT/$task-original/call-2.json" > "$TASK/solution/call-2.json"
      ;;
    refund_atomic)
      reward=1
      outcome=0
      jq --arg body "$REPLY" \
        '.params.name = "update_ticket" | .params.arguments = {ticket_id:4217, status:"solved", comment:{body:$body, public:true}}' \
        "$OUT/$task-original/call-2.json" > "$TASK/solution/call-2.json"
      jq '.params.name = "get_ticket" | .params.arguments = {ticket_id:4217}' \
        "$OUT/$task-original/call-3.json" > "$TASK/solution/call-3.json"
      ;;
    refund_premature)
      cp "$OUT/$task-original/call-3.json" "$TASK/solution/call-2.json"
      cp "$OUT/$task-original/call-2.json" "$TASK/solution/call-3.json"
      ;;
    refund_private)
      jq '.params.arguments.public = false' "$OUT/$task-original/call-2.json" > "$TASK/solution/call-2.json"
      ;;
    refund_wrong_ticket)
      jq '.params.arguments.ticket_id = 4219' "$OUT/$task-original/call-2.json" > "$TASK/solution/call-2.json"
      ;;
    refund_reference_only|refund_contradiction)
      outcome=6
      adjudication=1
      body='RF-88213'
      if [ "$variant" = refund_contradiction ]; then
        body='The duplicate October charge has not been refunded. RF-88213, 5-7 business days.'
      fi
      jq --arg body "$body" '.params.arguments.comment = $body' \
        "$OUT/$task-original/call-2.json" > "$TASK/solution/call-2.json"
      ;;
    refund_hidden)
      outcome=6
      adjudication=1
      jq --arg body "<p hidden>$REPLY</p>" \
        '.params.name = "update_ticket" | .params.arguments = {ticket_id:4217, comment:{html_body:$body, public:true}}' \
        "$OUT/$task-original/call-2.json" > "$TASK/solution/call-2.json"
      ;;
    escalation_atomic)
      reward=1
      outcome=0
      jq '.params.arguments += {comment:{body:"Escalated the Northwind Health patient portal outage to on-call engineering under incident INC-4471.", public:false}}' \
        "$OUT/$task-original/call-2.json" > "$TASK/solution/call-2.json"
      jq '.params.name = "get_ticket" | .params.arguments = {ticket_id:6230}' \
        "$OUT/$task-original/call-3.json" > "$TASK/solution/call-3.json"
      ;;
    escalation_tag_loss)
      jq '.params.arguments.tags = ["escalated"]' "$OUT/$task-original/call-2.json" > "$TASK/solution/call-2.json"
      ;;
    escalation_reference_only)
      outcome=6
      adjudication=1
      jq '.params.arguments.comment = "INC-4471"' "$OUT/$task-original/call-3.json" > "$TASK/solution/call-3.json"
      ;;
    create_alternative)
      reward=1
      outcome=0
      jq '.params.arguments.description = "Elena Petrova of Fjord Outfitters reports that the barcode scanners in the Bergen warehouse stopped syncing inventory counts after this morning'\''s app update."' \
        "$OUT/$task-original/call-1.json" > "$TASK/solution/call-1.json"
      ;;
    create_wrong_requester)
      jq '.params.arguments.requester_id = 9010302' "$OUT/$task-original/call-1.json" > "$TASK/solution/call-1.json"
      ;;
    create_place_only)
      outcome=6
      adjudication=1
      jq '.params.arguments.description = "Bergen"' "$OUT/$task-original/call-1.json" > "$TASK/solution/call-1.json"
      ;;
  esac
  sh deploy/run-trial.sh -p "$TASK" -a oracle -o "$OUT/$variant" > "$OUT/$variant.log" 2>&1
  result="$(find "$OUT/$variant" -path '*/verifier/reward.json' -print -quit)"
  test -n "$result"
  jq -e --argjson reward "$reward" --argjson outcome "$outcome" --argjson adjudication "$adjudication" \
    '.reward == $reward and .outcome_code == $outcome
      and .adjudication_needed == $adjudication
      and .environment_ok == 1 and .contract_ok == 1 and .integrity_ok == 1
      and .preservation_checked == 1 and .history_checked == 1
      and .runner_admission_checked == 1 and .runner_admission_ok == 1' "$result" >/dev/null
  case "$variant" in
    refund_premature) jq -e '.ordering_ok == 0' "$result" >/dev/null ;;
    escalation_tag_loss) jq -e '.preservation_ok == 0' "$result" >/dev/null ;;
  esac
  printf '%s\n' "$variant: Zendesk outcome, text disposition and admission passed"
done
if cargo run -q --locked -p report --bin bench-report -- --require-attested \
    "$OUT/refund_reference_only" "$OUT/refund_formatting" --allow-agent-difference --json \
    > "$OUT/comparison.json" 2> "$OUT/comparison.log"; then
  printf '%s\n' 'an unresolved Zendesk reply produced an apparently final comparison' >&2
  exit 1
fi
test ! -s "$OUT/comparison.json"
grep -q 'require adjudication' "$OUT/comparison.log"
printf '%s\n' 'unresolved communication cannot silently become a final paired score'
