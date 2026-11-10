#!/bin/sh
set -eu
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
WORLD_IMAGE="${WORLD_IMAGE:?provide the current world image}"
TOOLS_IMAGE="${TOOLS_IMAGE:?provide the tools image}"
OUT="$(mktemp -d /tmp/tool-bench-communication-history.XXXXXX)"
printf 'artifacts: %s\n' "$OUT"
cargo run -q --locked -p scenario --bin taskgen -- --out "$OUT/tasks" \
  --world-image "$WORLD_IMAGE" --tools-image "$TOOLS_IMAGE" >/dev/null
TASK="$OUT/tasks/collab__jira__ship-refund-retries"
CAPTURING="$OUT/tasks/business__multi__zendesk-refund-via-stripe/solution"
sed '/^bound_call [0-9]/,$d' "$CAPTURING/solve.sh" > "$TASK/solution/solve.sh"
cp "$CAPTURING/"*.jq "$TASK/solution/"
for index in 0 1 2 3 4 5; do
  printf '%s\n' '[]' > "$TASK/solution/call-$index.capture.json"
  template=0
  if [ "$index" = 3 ]; then
    printf '%s\n' '[{"name":"comment_id","pointer":"/content/0/text","json_pointer":"/id"}]' \
      > "$TASK/solution/call-3.capture.json"
  fi
  if [ "$index" -gt 3 ]; then
    template=1
    body='The refund webhook retry story did not ship in release 2026.11.'
    if [ "$index" = 5 ]; then body='Shipped in release 2026.11.'; fi
    jq -n --arg body "$body" \
      '{jsonrpc:"2.0",id:2,method:"tools/call",params:{name:"jira_edit_comment",arguments:{issue_key:"PAY-121",comment_id:"${comment_id}",body:$body}}}' \
      > "$TASK/solution/call-$index.json"
  fi
  printf 'bound_call 7128 "$DIR/call-%s.json" "$DIR/call-%s.capture.json" %s\n' \
    "$index" "$index" "$template" >> "$TASK/solution/solve.sh"
done
sh deploy/run-trial.sh -p "$TASK" -a oracle -o "$OUT/restored_claim" > "$OUT/restored_claim.log" 2>&1
result="$(find "$OUT/restored_claim" -path '*/verifier/reward.json' -print -quit)"
test -n "$result"
jq -e '.environment_ok == 1 and .contract_ok == 1 and .integrity_ok == 1
  and .runner_admission_checked == 1 and .runner_admission_ok == 1
  and .preservation_checked == 1 and .history_checked == 1' "$result" >/dev/null
if ! jq -e '.reward == 0 and .outcome_code == 6 and .adjudication_needed == 1' "$result" >/dev/null; then
  jq '{reward,outcome_code,adjudication_needed,records_ok,history_ok,preservation_ok}' "$result" >&2
  printf '%s\n' 'a restored final comment concealed an earlier unreviewed committed claim' >&2
  exit 1
fi
printf '%s\n' 'restoring a final positive example cannot erase unresolved committed communication'
TASK="$OUT/tasks/business__shopify__concierge-note"
call_line="$(tail -n 1 "$TASK/solution/solve.sh")"
for index in 3 4; do
  body='Leave outdoors overnight'
  if [ "$index" = 4 ]; then body='Leave with building concierge'; fi
  jq --arg body "$body" '.params.arguments.note = $body' \
    "$TASK/solution/call-2.json" > "$TASK/solution/call-$index.json"
  printf '%s\n' "$call_line" | sed "s/call-2.json/call-$index.json/" >> "$TASK/solution/solve.sh"
done
sh deploy/run-trial.sh -p "$TASK" -a oracle -o "$OUT/restored_literal" > "$OUT/restored_literal.log" 2>&1
result="$(find "$OUT/restored_literal" -path '*/verifier/reward.json' -print -quit)"
test -n "$result"
jq -e '.reward == 0 and .outcome_code == 6 and .adjudication_needed == 1
  and .state_ok == 0 and .environment_ok == 1 and .contract_ok == 1 and .integrity_ok == 1
  and .runner_admission_checked == 1 and .runner_admission_ok == 1
  and .preservation_checked == 1 and .history_checked == 1' "$result" >/dev/null
printf '%s\n' 'restoring the requested literal cannot erase a different committed delivery instruction'
if cargo run -q --locked -p report --bin bench-report -- \
    "$OUT/restored_claim" "$OUT/restored_claim" --json \
    > "$OUT/comparison.json" 2> "$OUT/comparison.log"; then
  printf '%s\n' 'unresolved committed communication produced a final paired score' >&2
  exit 1
fi
test ! -s "$OUT/comparison.json"
grep -q 'require adjudication' "$OUT/comparison.log"
printf '%s\n' 'reports reproduce the historical uncertainty and refuse final comparison'
