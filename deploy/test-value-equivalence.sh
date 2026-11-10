#!/bin/sh
set -eu
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
WORLD_IMAGE="${WORLD_IMAGE:?provide the current world image}"
TOOLS_IMAGE="${TOOLS_IMAGE:?provide the tools image}"
OUT="$(mktemp -d /tmp/tool-bench-value-equivalence.XXXXXX)"
printf 'artifacts: %s\n' "$OUT"
cargo run -q --locked -p scenario --bin taskgen -- --out "$OUT/tasks" \
  --world-image "$WORLD_IMAGE" --tools-image "$TOOLS_IMAGE" >/dev/null
DEAL="$OUT/tasks/business__salesforce__close-won-kestrel-renewal"
CALL="$OUT/tasks/business__salesforce__log-call-on-contact"
cp "$DEAL/solution/call-1.json" "$OUT/deal.json"
cp "$CALL/solution/call-1.json" "$OUT/call.json"
for variant in equivalent_amount changed_amount nullable_call_type; do
  reward=1
  outcome=0
  case "$variant" in
    equivalent_amount|changed_amount)
      task="$DEAL"
      amount=184000
      if [ "$variant" = changed_amount ]; then amount=184001; reward=0; outcome=1; fi
      jq --arg amount "$amount" '.params.arguments.records[0].Amount = $amount' \
        "$OUT/deal.json" > "$DEAL/solution/call-1.json"
      ;;
    nullable_call_type)
      task="$CALL"
      jq '.params.arguments.records[0].TaskSubtype = null | .params.arguments.records[0].Type = "Call"' \
        "$OUT/call.json" > "$CALL/solution/call-1.json"
      ;;
  esac
  sh deploy/run-trial.sh -p "$task" -a oracle -o "$OUT/$variant" > "$OUT/$variant.log" 2>&1
  result="$(find "$OUT/$variant" -path '*/verifier/reward.json' -print -quit)"
  test -n "$result"
  jq -e --argjson reward "$reward" --argjson outcome "$outcome" \
    '.reward == $reward and .outcome_code == $outcome
      and .environment_ok == 1 and .contract_ok == 1 and .integrity_ok == 1
      and .runner_admission_checked == 1 and .runner_admission_ok == 1
      and .adjudication_needed == 0' "$result" >/dev/null
  printf '%s\n' "$variant: exact value semantics and runner admission passed"
done
