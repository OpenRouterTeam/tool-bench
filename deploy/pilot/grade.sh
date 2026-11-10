#!/usr/bin/env bash
# grade.sh <slot> [answer-file]
#
# Finalizes the slot's world, saves the sealed evidence and runs the host bench-verify on it
# with the inputs tests/test.sh uses. Everything is written to
# /tmp/tb-pilot/slots/<slot>/verifier/. Prints one summary line, then one line per failure,
# then any notes. A finalized world cannot be reused: a later call says so and re-checks the
# saved evidence only.
# Exit status: 0 graded, 3 the trial could not be assessed, 1 or 2 grading did not run.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
PILOT_HOME=/tmp/tb-pilot
# prepare.sh pins a copy of the grader beside the tasks it generated, so a later build of
# the checkout cannot change how a running pilot is graded.
PINNED="${PILOT_HOME:-/tmp/tb-pilot}/bin/bench-verify"
[ -x "$PINNED" ] || PINNED="$ROOT/target/debug/bench-verify"
VERIFY="${BENCH_VERIFY:-$PINNED}"
die() { printf 'grade.sh: %s\n' "$*" >&2; exit 2; }

[ $# -ge 1 ] && [ $# -le 2 ] || die "usage: grade.sh <slot> [answer-file]"
slot="$1"
answer="${2:-}"
case "$slot" in ''|*[!0-9]*) die "slot must be a number from 0 to 19" ;; esac
slot=$((10#$slot))
slot_dir="$PILOT_HOME/slots/$slot"
[ -f "$slot_dir/slot.env" ] || die "slot $slot was never started (no $slot_dir/slot.env); run up.sh first"
# shellcheck disable=SC1091
. "$slot_dir/slot.env"
[ -x "$VERIFY" ] || die "$VERIFY is missing; run prepare.sh or: cargo build -q -p mock-core --bin bench-verify"
command -v jq > /dev/null || die "jq is required"
[ -z "$answer" ] || [ -f "$answer" ] || die "answer file $answer does not exist"

tests="$slot_dir/tests"
out="$slot_dir/verifier"
inputs="$out/inputs"
mkdir -p "$inputs"

refinalized=0
if [ -f "$inputs/evidence.json" ]; then
  refinalized=1
else
  world="$(docker ps -q --filter "label=com.docker.compose.project=$PROJECT" \
    --filter "label=com.docker.compose.service=world")"
  [ -n "$world" ] || die "slot $slot has no running world container and no saved evidence; nothing to grade"
  # The control API listens only on loopback inside the world container, which holds the token.
  if ! docker exec "$world" sh -c \
      'curl -fsS -X POST -H "authorization: Bearer $BENCH_ADMIN_TOKEN" http://localhost:9001/_bench/finalize' \
      > "$inputs/evidence.json.tmp"; then
    rm -f "$inputs/evidence.json.tmp"
    die "finalizing the world of slot $slot failed"
  fi
  mv "$inputs/evidence.json.tmp" "$inputs/evidence.json"
  date -u +%Y-%m-%dT%H:%M:%SZ > "$inputs/finalized-at.txt"
fi

# Start each verification from a clean output directory, keeping only the inputs.
find "$out" -mindepth 1 -maxdepth 1 ! -name inputs -exec rm -rf {} +
rm -f "$inputs/answer.md"
[ -z "$answer" ] || cp "$answer" "$inputs/answer.md"
[ ! -f "$tests/connections.json" ] || cp "$tests/connections.json" "$out/connections.json"

# The runner receipt, realized manifest and inference evidence are named as test.sh names
# them, but no runner wrote them here: the trial is graded as unattested.
set -- "$VERIFY" \
  --scenario "$(cat "$tests/scenario.txt")" \
  --contract "$tests/expect.json" \
  --task-source "$tests/task-source.json" \
  --task-context "$tests/task-context.json" \
  --evidence "$inputs/evidence.json" \
  --runner-policy "$tests/runner-policy.json" \
  --runner-admission "$inputs/runner-admission.json" \
  --realized-manifest "$inputs/realized-manifest.json" \
  --inference-evidence "$inputs/inference-evidence.json" \
  --output "$out" \
  --allow-unattested-runner \
  --withhold-invalid-reward
[ -z "$answer" ] || set -- "$@" --answer "$inputs/answer.md"

# Warn when the generated test.sh has gained a flag this script does not pass.
for flag in $(grep -oE -- '--[a-z][a-z-]+' "$tests/test.sh" | sort -u); do
  case " $* --answer " in
    *" $flag "*) ;;
    *) printf 'warning: tests/test.sh passes %s, which grade.sh does not\n' "$flag" >&2 ;;
  esac
done

status=0
"$@" > "$out/test-stdout.txt" 2> "$out/test-stderr.txt" || status=$?
if [ ! -f "$out/assessment.json" ]; then
  cat "$out/test-stderr.txt" >&2
  die "bench-verify exited $status without writing an assessment"
fi

if [ -n "$answer" ]; then answer_note=given; else answer_note=none; fi
jq -r --arg slot "$slot" --arg task "$TASK_NAME" --arg answer "$answer_note" '
  def yesno: if . then "yes" else "no" end;
  "slot \($slot) \($task): reward=\(.reward) outcome=\(.outcome) adjudication_needed=\(.adjudication_needed | yesno) api_calls=\(.api_calls) mcp_bypass=\(.mcp_bypass) answer=\($answer) failures=\(.failures | length)",
  (.failures[] | "  - \(.check) [\(.disposition)] at \(.location): \(.reason)")
' "$out/assessment.json"
if [ "$refinalized" -eq 1 ]; then
  printf 'note: slot %s was already finalized and its world cannot be reused; tool calls made after that are not in the evidence. This re-checked the saved evidence.\n' "$slot"
fi
if [ "$status" -ne 0 ]; then
  printf 'note: bench-verify exited %s (3 = unassessable; reward.json withheld): %s\n' \
    "$status" "$(tr '\n' ' ' < "$out/test-stderr.txt")"
fi
if jq -e '.outcome == "invalid_contract" or any(.failures[]; .location | test("^evidence/(contract|task)-source$"))' \
    "$out/assessment.json" > /dev/null; then
  printf 'hint: this usually means %s was built from a different tree than the task or its world image; re-run prepare.sh, or set BENCH_VERIFY to a matching grader\n' "$VERIFY"
fi
written="$PILOT_HOME/agents/$slot/app/answer.md"
if [ -z "$answer" ] && [ -f "$written" ]; then
  printf 'note: %s exists but was not graded; pass it as the second argument to grade it\n' "$written"
fi
printf 'details: %s\n' "$out"
exit "$status"
