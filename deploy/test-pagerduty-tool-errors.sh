#!/bin/sh
set -eu
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
WORLD_IMAGE="${WORLD_IMAGE:?provide the PagerDuty37 world candidate}"
TOOLS_IMAGE="${TOOLS_IMAGE:?provide the matching tools candidate}"
OUT="$(mktemp -d /tmp/tool-bench-pagerduty-errors.XXXXXX)"
printf 'artifacts: %s\n' "$OUT"
if [ -n "${TASKGEN_BINARY:-}" ]; then
  "$TASKGEN_BINARY" --out "$OUT/tasks" --world-image "$WORLD_IMAGE" --tools-image "$TOOLS_IMAGE" >/dev/null
else
  cargo run -q --locked -p scenario --bin taskgen -- --out "$OUT/tasks" \
    --world-image "$WORLD_IMAGE" --tools-image "$TOOLS_IMAGE" >/dev/null
fi
for slug in cover-shift-override reassign-replication-lag resolve-prod-checkout; do
  mkdir "$OUT/$slug-original"
  cp "$OUT/tasks/ops__pagerduty__$slug/solution/"* "$OUT/$slug-original/"
done
SCAFFOLD="$OUT/tasks/ops__grafana__silence-replica-maintenance/solution/solve.sh"
if [ "$#" -eq 0 ]; then
  set -- missing_incident missing_note_parent empty_management nested_override_failure \
    unknown_override_user reversed_override collapsed_override combined_commit_then_error
fi
for variant do
  slug=cover-shift-override
  tool=create_schedule_override
  case "$variant" in
    missing_incident)
      slug=resolve-prod-checkout
      tool=get_incident
      arguments='{"incident_id":"not-a-real-incident"}' ;;
    missing_note_parent)
      slug=resolve-prod-checkout
      tool=add_note_to_incident
      arguments='{"incident_id":"not-a-real-incident","note":"Unreachable note"}' ;;
    empty_management)
      slug=reassign-replication-lag
      tool=manage_incidents
      arguments='{"manage_request":{"incident_ids":["Q3K8ORDBLAG2RX"],"assignment":{"id":"PDNGYN2"}}}' ;;
    nested_override_failure)
      arguments='{"schedule_id":"PSPLTPR","override_request":{"overrides":[{"start":"2026-10-31T09:00:00Z","end":"2026-10-31T17:00:00Z","user_id":"PJ0RLEE"},{"start":"2026-10-31T13:00:00Z","end":"2026-10-31T13:00:00Z","user_id":"PJ0RLEE"}]}}' ;;
    unknown_override_user)
      arguments='{"schedule_id":"PSPLTPR","override_request":{"overrides":[{"start":"2026-10-31T09:00:00Z","end":"2026-10-31T17:00:00Z","user_id":"not-a-real-user"}]}}' ;;
    reversed_override)
      arguments='{"schedule_id":"PSPLTPR","override_request":{"overrides":[{"start":"2026-10-31T17:00:00Z","end":"2026-10-31T09:00:00Z","user_id":"PJ0RLEE"}]}}' ;;
    collapsed_override)
      arguments='{"schedule_id":"PSPLTPR","override_request":{"overrides":[{"start":"2026-10-31T09:00:00.000000001Z","end":"2026-10-31T09:00:00.000000999Z","user_id":"PJ0RLEE"}]}}' ;;
    combined_commit_then_error)
      slug=reassign-replication-lag
      tool=manage_incidents
      arguments='{"manage_request":{"incident_ids":["Q3K8ORDBLAG2RX"],"status":"resolved","assignement":{"id":"PDNGYN2"}}}' ;;
    *) printf 'unsupported PagerDuty error control: %s\n' "$variant" >&2; exit 2 ;;
  esac
  TASK="$OUT/tasks/ops__pagerduty__$slug"
  rm -rf "$TASK/solution"
  mkdir "$TASK/solution"
  cp "$OUT/$slug-original/"* "$TASK/solution/"
  cp "$TASK/solution/solve.sh" "$TASK/solution/reference-solve.sh"
  cp "$OUT/tasks/ops__grafana__silence-replica-maintenance/solution/"*.jq "$TASK/solution/"
  sed -e '/^bound_call [0-9]/,$d' -e '/^call [0-9]/,$d' "$SCAFFOLD" > "$TASK/solution/solve.sh"
  jq -n --arg tool "$tool" --argjson arguments "$arguments" \
    '{jsonrpc:"2.0",id:2,method:"tools/call",params:{name:$tool,arguments:$arguments}}' \
    > "$TASK/solution/expected-error.json"
  printf '%s\n' '[]' > "$TASK/solution/expected-error.capture.json"
  printf '%s\n' \
    'sh "$DIR/reference-solve.sh"' \
    'printf "%s\n" PAGERDUTY_BEFORE_EXPECTED_ERROR' \
    'bound_call 7111 "$DIR/expected-error.json" "$DIR/expected-error.capture.json" 0' \
    'printf "%s\n" PAGERDUTY_REACHED_LATER_CALL' \
    'bound_call 7111 "$DIR/expected-error.json" "$DIR/expected-error.capture.json" 0' \
    >> "$TASK/solution/solve.sh"
  sh -n "$TASK/solution/solve.sh"
  cp -R "$TASK/solution" "$OUT/program-$variant"
  if [ "${GENERATE_ONLY:-0}" = 1 ]; then
    printf '%s\n' "$variant: program generated; no runtime validation claimed"
    continue
  fi
  if sh deploy/run-trial.sh -p "$TASK" -a oracle -o "$OUT/$variant" > "$OUT/$variant.log" 2>&1; then
    printf '%s\n' "$variant: failed native operation incorrectly completed the program" >&2
    exit 1
  fi
  result="$(find "$OUT/$variant" -name result.json -print -quit)"
  receipt="$(find "$OUT/$variant" -name runner-admission.json -print -quit)"
  test -n "$result" && test -n "$receipt"
  jq -e '.exception_info!=null and .verifier_result==null' "$result" >/dev/null
  jq -e '.phase=="failed" and .failure!=null and .evidence_sha256==null
    and .main_artifacts_sha256==null' "$receipt" >/dev/null
  test -z "$(find "$OUT/$variant" -path '*/verifier/reward.json' -print -quit)"
  test -z "$(find "$OUT/$variant" -path '*/artifacts/world/evidence.json' -print -quit)"
  output="$(dirname "$result")/agent/oracle.txt"
  rg -q PAGERDUTY_BEFORE_EXPECTED_ERROR "$output"
  if rg -q PAGERDUTY_REACHED_LATER_CALL "$output"; then
    printf '%s\n' "$variant: later call ran after native failure" >&2
    exit 1
  fi
  printf '%s\n' "$variant: native failure stops later calls and authoritative assessment"
done
