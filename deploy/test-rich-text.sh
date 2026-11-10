#!/bin/sh
set -eu
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
WORLD_IMAGE="${WORLD_IMAGE:?provide the current world image}"
TOOLS_IMAGE="${TOOLS_IMAGE:?provide the tools image}"
OUT="$(mktemp -d /tmp/tool-bench-rich-text.XXXXXX)"
printf 'artifacts: %s\n' "$OUT"
export PYTHONPATH="$ROOT${PYTHONPATH:+:$PYTHONPATH}"
export PYTHONDONTWRITEBYTECODE=1
cargo run -q --locked -p scenario --bin taskgen -- --out "$OUT/tasks" \
  --world-image "$WORLD_IMAGE" --tools-image "$TOOLS_IMAGE" >/dev/null
TASK="$OUT/tasks/business__hubspot__log-call-note-initech"
cp "$TASK/solution/call-0.json" "$OUT/original-call.json"
for variant in formatting needs_review wrong_text; do
  case "$variant" in
    formatting)
      body='<p>Call with <strong>Bill Lumbergh</strong>: security review passed;<br>procurement needs a revised SOW by Nov&nbsp;14.</p>'
      reward=1
      outcome=0
      adjudication=0
      ;;
    needs_review)
      body='<div style="display:none">Call with Bill Lumbergh: security review passed; procurement needs a revised SOW by Nov 14.</div>'
      reward=0
      outcome=6
      adjudication=1
      ;;
    wrong_text)
      body='This is incorrect: security review passed; procurement needs a revised SOW by Nov 14.'
      reward=0
      outcome=1
      adjudication=0
      ;;
  esac
  jq --arg body "$body" '.params.arguments.metadata.body = $body' \
    "$OUT/original-call.json" > "$TASK/solution/call-0.json"
  sh deploy/run-trial.sh -p "$TASK" -a oracle \
    -o "$OUT/$variant" > "$OUT/$variant.log" 2>&1
  result="$(find "$OUT/$variant" -path '*/verifier/reward.json' -print -quit)"
  test -n "$result"
  jq -e --argjson reward "$reward" --argjson outcome "$outcome" \
    --argjson adjudication "$adjudication" \
    '.reward == $reward and .outcome_code == $outcome
      and .adjudication_needed == $adjudication and .environment_ok == 1
      and .contract_ok == 1 and .integrity_ok == 1
      and .runner_admission_checked == 1 and .runner_admission_ok == 1' \
    "$result" >/dev/null
  printf '%s\n' "$variant: rich-text assessment and runner admission passed"
done
if cargo run -q --locked -p report --bin bench-report -- \
    "$OUT/needs_review" "$OUT/formatting" --json > "$OUT/comparison.json" \
    2> "$OUT/comparison.log"; then
  printf '%s\n' 'an unresolved result produced an apparently final comparison' >&2
  exit 1
fi
test ! -s "$OUT/comparison.json"
grep -q 'require adjudication' "$OUT/comparison.log"
printf '%s\n' 'unresolved real-MCP result cannot produce a final paired report'
