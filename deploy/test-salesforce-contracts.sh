#!/bin/sh
set -eu
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
WORLD_IMAGE="${WORLD_IMAGE:?provide the current world image}"
TOOLS_IMAGE="${TOOLS_IMAGE:?provide the tools image}"
OUT="$(mktemp -d /tmp/tool-bench-salesforce.XXXXXX)"
printf 'artifacts: %s\n' "$OUT"
cargo run -q --locked -p scenario --bin taskgen -- --runner-admission required --out "$OUT/tasks" \
  --world-image "$WORLD_IMAGE" --tools-image "$TOOLS_IMAGE" >/dev/null
TASK="$OUT/tasks/business__salesforce__log-call-on-contact"
cp "$TASK/solution/call-1.json" "$OUT/original-call.json"
for variant in known_wording unreviewed_wording contradiction wrong_contact invented_duration; do
  reward=0
  outcome=6
  adjudication=1
  patch='{}'
  case "$variant" in
    known_wording)
      patch='{"TaskSubtype":"Task","Type":"Call","OwnerId":"0055g00000MZfGEAA1","Priority":"High","Description":"Dana requested a three-year renewal quote by May 22, 2026."}'
      reward=1
      outcome=0
      adjudication=0
      ;;
    unreviewed_wording)
      patch='{"Description":"Dana would like an offer for a three-year term by the twenty-second of May."}'
      ;;
    contradiction)
      patch='{"Description":"Dana does not want a 3-year quote by May 22."}'
      ;;
    wrong_contact)
      patch='{"WhoId":"0035g00000my8VHAAY","Description":"quote"}'
      outcome=1
      adjudication=0
      ;;
    invented_duration)
      patch='{"CallDurationInSeconds":3600}'
      outcome=1
      adjudication=0
      ;;
  esac
  jq --argjson patch "$patch" '.params.arguments.records[0] += $patch' \
    "$OUT/original-call.json" > "$TASK/solution/call-1.json"
  sh deploy/run-trial.sh -p "$TASK" -a oracle -o "$OUT/$variant" \
    > "$OUT/$variant.log" 2>&1
  result="$(find "$OUT/$variant" -path '*/verifier/reward.json' -print -quit)"
  test -n "$result"
  jq -e --argjson reward "$reward" --argjson outcome "$outcome" \
    --argjson adjudication "$adjudication" \
    '.reward == $reward and .outcome_code == $outcome
      and .adjudication_needed == $adjudication
      and .environment_ok == 1 and .contract_ok == 1 and .integrity_ok == 1
      and .runner_admission_checked == 1 and .runner_admission_ok == 1' "$result" >/dev/null
  printf '%s\n' "$variant: Salesforce outcome, text disposition and admission passed"
done
if cargo run -q --locked -p report --bin bench-report -- --require-attested \
    "$OUT/unreviewed_wording" "$OUT/known_wording" --json \
    > "$OUT/comparison.json" 2> "$OUT/comparison.log"; then
  printf '%s\n' 'unreviewed natural wording produced an apparently final paired score' >&2
  exit 1
fi
test ! -s "$OUT/comparison.json"
grep -q 'require adjudication' "$OUT/comparison.log"
printf '%s\n' 'unreviewed natural wording cannot silently become a model failure or final score'
