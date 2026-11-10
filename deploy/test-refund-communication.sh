#!/bin/sh
set -eu
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
WORLD_IMAGE="${WORLD_IMAGE:?provide the current world image}"
TOOLS_IMAGE="${TOOLS_IMAGE:?provide the tools image}"
OUT="$(mktemp -d /tmp/tool-bench-refund-communication.XXXXXX)"
printf 'artifacts: %s\n' "$OUT"
cargo run -q --locked -p scenario --bin taskgen -- --out "$OUT/tasks" \
  --world-image "$WORLD_IMAGE" --tools-image "$TOOLS_IMAGE" >/dev/null
TASK="$OUT/tasks/business__multi__zendesk-refund-via-stripe"
mkdir "$OUT/original"
cp "$TASK/solution/"*.json "$OUT/original/"
sed '/^bound_call [0-9]/,$d' "$TASK/solution/solve.sh" > "$OUT/prefix.sh"
if [ "$#" = 0 ]; then
  set -- grounded minimal_reply bare_reference denied_refund unreviewed wrong_reference split_refund split_reordered atomic_reply premature_split wrong_ticket
fi
for variant in "$@"; do
  reward=0
  outcome=6
  adjudication=1
  body='Hannah, the full duplicate payment of $480.00 for order #1077 has been refunded. Stripe refund references: ${refund_id}. The original payment is unchanged.'
  cp "$OUT/original/"*.json "$TASK/solution/"
  cp "$OUT/prefix.sh" "$TASK/solution/solve.sh"
  case "$variant" in
    grounded|split_refund|split_reordered|atomic_reply) reward=1; outcome=0; adjudication=0 ;;
    minimal_reply) body='Your duplicate payment was refunded. Stripe refund references: ${refund_id}.'; reward=1; outcome=0; adjudication=0 ;;
    bare_reference) body='${refund_id}' ;;
    denied_refund) body='The duplicate payment has not been refunded. Stripe refund reference: ${refund_id}.' ;;
    unreviewed) body='Hannah, the entire extra $480 payment for #1077 was returned. You can reference ${refund_id}; the original is untouched.' ;;
    wrong_reference) body='Your refund re_unrelated was issued.'; outcome=1; adjudication=0 ;;
    premature_split) outcome=1 ;;
    wrong_ticket) outcome=1; adjudication=0 ;;
    *) printf 'unknown variant: %s\n' "$variant" >&2; exit 2 ;;
  esac
  jq --arg body "$body" '.params.arguments.comment = $body' \
    "$OUT/original/call-5.json" > "$TASK/solution/call-5.json"
  for index in 0 1 2 3 4; do
    port=7121
    if [ "$index" -lt 2 ]; then port=7124; fi
    printf 'bound_call %s "$DIR/call-%s.json" "$DIR/call-%s.capture.json" 0\n' \
      "$port" "$index" "$index" >> "$TASK/solution/solve.sh"
  done
  if [ "$variant" = split_refund ] || [ "$variant" = split_reordered ] || [ "$variant" = premature_split ]; then
    jq '.params.arguments.parameters.amount = 18000' "$OUT/original/call-4.json" > "$TASK/solution/call-4.json"
    jq '.[0].name = "refund_first"' "$OUT/original/call-4.capture.json" > "$TASK/solution/call-4.capture.json"
    jq '.params.arguments.parameters.amount = 30000' "$OUT/original/call-4.json" > "$TASK/solution/call-7.json"
    jq '.[0].name = "refund_second"' "$OUT/original/call-4.capture.json" > "$TASK/solution/call-7.capture.json"
    if [ "$variant" = premature_split ]; then
      printf '%s\n' 'jq ".refund_id = .refund_first" "$WORK/bindings.json" > "$WORK/next-bindings.json"' \
        'mv "$WORK/next-bindings.json" "$WORK/bindings.json"' \
        'bound_call 7124 "$DIR/call-5.json" "$DIR/call-5.capture.json" 1' >> "$TASK/solution/solve.sh"
    fi
    printf '%s\n' 'bound_call 7121 "$DIR/call-7.json" "$DIR/call-7.capture.json" 0' \
      'jq ".refund_id = ([.refund_first,.refund_second] | sort | join(\", \"))" "$WORK/bindings.json" > "$WORK/next-bindings.json"' \
      'mv "$WORK/next-bindings.json" "$WORK/bindings.json"' >> "$TASK/solution/solve.sh"
    if [ "$variant" = split_reordered ]; then
      printf '%s\n' 'jq ".refund_id = ([.refund_first,.refund_second] | sort | reverse | join(\", \"))" "$WORK/bindings.json" > "$WORK/next-bindings.json"' \
        'mv "$WORK/next-bindings.json" "$WORK/bindings.json"' >> "$TASK/solution/solve.sh"
    fi
  fi
  if [ "$variant" = atomic_reply ]; then
    jq --arg body "$body" '.params.name = "update_ticket" |
      .params.arguments = {ticket_id:5102, status:"solved", comment:{body:$body, public:true}}' \
      "$OUT/original/call-5.json" > "$TASK/solution/call-5.json"
  fi
  if [ "$variant" = wrong_ticket ]; then
    jq '.params.arguments.ticket_id = 5088' "$TASK/solution/call-5.json" > "$OUT/wrong-ticket.json"
    mv "$OUT/wrong-ticket.json" "$TASK/solution/call-5.json"
  fi
  if [ "$variant" != premature_split ]; then
    printf '%s\n' 'bound_call 7124 "$DIR/call-5.json" "$DIR/call-5.capture.json" 1' >> "$TASK/solution/solve.sh"
  fi
  printf '%s\n' 'bound_call 7124 "$DIR/call-6.json" "$DIR/call-6.capture.json" 0' >> "$TASK/solution/solve.sh"
  sh deploy/run-trial.sh -p "$TASK" -a oracle -o "$OUT/$variant" > "$OUT/$variant.log" 2>&1
  result="$(find "$OUT/$variant" -path '*/verifier/reward.json' -print -quit)"
  test -n "$result"
  jq -e '.environment_ok == 1 and .contract_ok == 1 and .integrity_ok == 1
    and .runner_admission_checked == 1 and .runner_admission_ok == 1
    and .preservation_checked == 1 and .history_checked == 1' "$result" >/dev/null
  if ! jq -e --argjson reward "$reward" --argjson outcome "$outcome" --argjson adjudication "$adjudication" \
    '.reward == $reward and .outcome_code == $outcome and .adjudication_needed == $adjudication' "$result" >/dev/null; then
    jq '{reward,outcome_code,adjudication_needed,state_ok,records_ok,relations_ok,ordering_ok}' "$result" >&2
    printf '%s\n' "$variant: communication or legitimate-plan disposition is incorrect" >&2
    exit 1
  fi
  printf '%s\n' "$variant: source-backed communication and execution admission passed"
done
if [ -d "$OUT/unreviewed" ] && [ -d "$OUT/grounded" ]; then
  if cargo run -q --locked -p report --bin bench-report -- \
      "$OUT/unreviewed" "$OUT/grounded" --allow-agent-difference --json \
      > "$OUT/comparison.json" 2> "$OUT/comparison.log"; then
    printf '%s\n' 'unreviewed identity-backed prose produced a final paired score' >&2
    exit 1
  fi
  test ! -s "$OUT/comparison.json"
  grep -q 'require adjudication' "$OUT/comparison.log"
  printf '%s\n' 'identity-backed uncertainty cannot silently become a measured model failure or final score'
fi
