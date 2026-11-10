#!/usr/bin/env bash
# selftest.sh [slot-a slot-b]      (default slots 18 and 19; tasks from $PILOT_TASKS)
#
# Checks the harness end to end on three kinds of task (a lookup with an answer file, a
# record creation, a multi-service write). In each of three rounds two slots run at once on
# different tasks: slot-a replays its task's reference solution through mcp.py and must
# grade reward 1; slot-b is left untouched and must grade reward 0 with no API calls, which
# also shows that nothing slot-a did reached slot-b's world. Every task is used once each way.
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
PILOT_HOME=/tmp/tb-pilot
export PILOT_TASKS="${PILOT_TASKS:-$PILOT_HOME/tasks}"
A="${1:-18}"
B="${2:-19}"
TASKS=(
  business__stripe__customer-mrr
  code__github__file-login-bug
  ops__multi__pagerduty-sentry-github-issue
)
die() { printf 'selftest.sh: %s\n' "$*" >&2; exit 2; }

[ "$A" != "$B" ] || die "the two slots must differ"
for slot in "$A" "$B"; do
  [ -z "$(docker ps -aq --filter "label=com.docker.compose.project=tbpilot-$slot")" ] \
    || die "slot $slot is in use; pass two free slots: selftest.sh <slot-a> <slot-b>"
done
cleanup() {
  "$HERE/down.sh" "$A" > /dev/null 2>&1 || true
  "$HERE/down.sh" "$B" > /dev/null 2>&1 || true
}
trap cleanup EXIT

failed=0
# check <round> <slot> <role> <wanted reward> : prints one line from the slot's assessment.
check() {
  local assessment="$PILOT_HOME/slots/$2/verifier/assessment.json" reward calls outcome verdict=ok
  reward="$(jq -r '.reward' "$assessment" 2> /dev/null || echo none)"
  calls="$(jq -r '.api_calls' "$assessment" 2> /dev/null || echo 0)"
  outcome="$(jq -r '.outcome' "$assessment" 2> /dev/null || echo none)"
  [ "$reward" = "$4" ] || verdict=FAILED
  # An untouched world must be a graded failure, not a trial the verifier could not assess.
  if [ "$3" = reference ]; then
    [ "$outcome" = pass ] && [ "$calls" -gt 0 ] || verdict=FAILED
  else
    [ "$outcome" = fail ] && [ "$calls" -eq 0 ] || verdict=FAILED
  fi
  [ "$verdict" = ok ] || failed=1
  # shellcheck disable=SC1090
  printf 'round %s  slot %-2s  %-9s  %-45s reward=%s (want %s)  api_calls=%-3s %s\n' "$1" "$2" "$3" \
    "$(. "$PILOT_HOME/slots/$2/slot.env"; printf '%s' "$TASK_NAME")" "$reward" "$4" "$calls" "$verdict"
}

for index in 0 1 2; do
  round=$((index + 1))
  "$HERE/up.sh" "${TASKS[$index]}" "$A" > "$PILOT_HOME/selftest-up-a.log" 2>&1 &
  pid_a=$!
  "$HERE/up.sh" "${TASKS[$(((index + 1) % 3))]}" "$B" > "$PILOT_HOME/selftest-up-b.log" 2>&1 &
  pid_b=$!
  wait "$pid_a" || { cat "$PILOT_HOME/selftest-up-a.log" >&2; die "slot $A did not start"; }
  wait "$pid_b" || { cat "$PILOT_HOME/selftest-up-b.log" >&2; die "slot $B did not start"; }
  head -n 1 "$PILOT_HOME/selftest-up-a.log" "$PILOT_HOME/selftest-up-b.log" | grep '^slot'

  "$HERE/replay.sh" "$A" > "$PILOT_HOME/selftest-replay.log" 2>&1 \
    || { cat "$PILOT_HOME/selftest-replay.log" >&2; die "replaying the reference solution on slot $A failed"; }
  answer="$PILOT_HOME/agents/$A/app/answer.md"
  # A grade.sh failure is judged by check below, from the assessment it left (or did not).
  if [ -f "$answer" ]; then
    "$HERE/grade.sh" "$A" "$answer" 2>&1 | sed 's/^/    /' || true
  else
    "$HERE/grade.sh" "$A" 2>&1 | sed 's/^/    /' || true
  fi
  "$HERE/grade.sh" "$B" 2>&1 | sed 's/^/    /' || true
  check "$round" "$A" reference 1.0
  check "$round" "$B" untouched 0.0
  if [ "$index" -eq 0 ]; then
    again="$("$HERE/grade.sh" "$A" "$answer" 2>&1 || true)"
    if [[ "$again" == *"already finalized"* && "$again" == *"reward=1.0"* ]]; then
      printf 'round %s  slot %-2s  a second grade.sh call reports the finalized world  ok\n' "$round" "$A"
    else
      printf 'round %s  slot %-2s  a second grade.sh call did not report the finalized world  FAILED\n' "$round" "$A"
      failed=1
    fi
  fi
  "$HERE/down.sh" "$A" > /dev/null &
  "$HERE/down.sh" "$B" > /dev/null
  wait
done
rm -f "$PILOT_HOME"/selftest-up-a.log "$PILOT_HOME"/selftest-up-b.log "$PILOT_HOME"/selftest-replay.log

if [ "$failed" -eq 0 ]; then
  printf 'selftest: PASS (3 reference replays graded 1, 3 untouched worlds graded 0, two slots at once)\n'
else
  printf 'selftest: FAILED\n'
  exit 1
fi
