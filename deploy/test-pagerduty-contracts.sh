#!/bin/sh
set -eu
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
WORLD_IMAGE="${WORLD_IMAGE:?provide the PagerDuty37 world candidate}"
TOOLS_IMAGE="${TOOLS_IMAGE:?provide the matching tools candidate}"
OUT="$(mktemp -d /tmp/tool-bench-pagerduty-controls.XXXXXX)"
printf 'artifacts: %s\n' "$OUT"
if [ -n "${TASKGEN_BINARY:-}" ]; then
  "$TASKGEN_BINARY" --out "$OUT/tasks" --world-image "$WORLD_IMAGE" --tools-image "$TOOLS_IMAGE" >/dev/null
else
  cargo run -q --locked -p scenario --bin taskgen -- --runner-admission required --out "$OUT/tasks" \
    --world-image "$WORLD_IMAGE" --tools-image "$TOOLS_IMAGE" >/dev/null
fi
for slug in cover-shift-override reassign-replication-lag resolve-prod-checkout; do
  mkdir "$OUT/$slug-original"
  cp "$OUT/tasks/ops__pagerduty__$slug/solution/"* "$OUT/$slug-original/"
done
SCAFFOLD="$OUT/tasks/ops__grafana__silence-replica-maintenance/solution/solve.sh"
add() {
  jq -n --arg tool "$1" --argjson arguments "$2" \
    '{jsonrpc:"2.0",id:2,method:"tools/call",params:{name:$tool,arguments:$arguments}}' \
    > "$TASK/solution/control-$calls.json"
  printf '%s\n' '[]' > "$TASK/solution/control-$calls.capture.json"
  printf 'bound_call 7111 "$DIR/control-%s.json" "$DIR/control-%s.capture.json" 0\n' \
    "$calls" "$calls" >> "$TASK/solution/solve.sh"
  calls=$((calls + 1))
}
reference() {
  printf '%s\n' 'sh "$DIR/reference-solve.sh"' >> "$TASK/solution/solve.sh"
}
note() {
  add add_note_to_incident "$(jq -cn --arg incident "$1" --arg note "$2" \
    '{incident_id:$incident,note:$note}')"
}
manage() {
  add manage_incidents "$(jq -cn --argjson changes "$1" \
    '{manage_request:({incident_ids:["Q3K8ORDBLAG2RX"]}+$changes)}')"
}
resolve() {
  add manage_incidents '{"manage_request":{"incident_ids":["Q7CHKAP15PCT1D"],"status":"resolved"}}'
}
cover() {
  add create_schedule_override "$(jq -cn --arg schedule "$1" --argjson overrides "$2" \
    '{schedule_id:$schedule,override_request:{overrides:$overrides}}')"
}
handoff_note='Handing off to DBRE: replica-2 disk is at 100%, needs a volume resize.'
resolve_note='Resolved by rolling back release 2026.10.29.3.'
whole='[{"start":"2026-10-31T09:00:00Z","end":"2026-10-31T17:00:00Z","user_id":"PJ0RLEE"}]'
if [ "$#" -eq 0 ]; then
  set -- cover_reference cover_extra_reads cover_offsets cover_split cover_overlap cover_repeated \
    cover_fractional_split cover_native_truncation cover_native_offset_truncation cover_fractional_reads \
    cover_micro_gap cover_micro_late_start cover_micro_late_end cover_native_no_carry \
    cover_wrong_user cover_wrong_schedule cover_gap cover_early cover_late cover_other_schedule \
    handoff_reference handoff_note_first handoff_extra_reads handoff_repeated_note \
    handoff_wrong_primary handoff_ack_restore handoff_urgency handoff_urgency_restore \
    handoff_weak_note handoff_other_note \
    resolve_reference resolve_first resolve_acknowledge resolve_paraphrase resolve_repeated_note \
    resolve_wrong_note resolve_extra_incident resolve_ack_only resolve_other_note
fi
for variant do
  calls=0
  reward=0
  case "$variant" in
    cover_*) slug=cover-shift-override ;;
    handoff_*) slug=reassign-replication-lag ;;
    resolve_*) slug=resolve-prod-checkout ;;
    *) printf 'unsupported PagerDuty control: %s\n' "$variant" >&2; exit 2 ;;
  esac
  TASK="$OUT/tasks/ops__pagerduty__$slug"
  rm -rf "$TASK/solution"
  mkdir "$TASK/solution"
  cp "$OUT/$slug-original/"* "$TASK/solution/"
  cp "$TASK/solution/solve.sh" "$TASK/solution/reference-solve.sh"
  cp "$OUT/tasks/ops__grafana__silence-replica-maintenance/solution/"*.jq "$TASK/solution/"
  sed -e '/^bound_call [0-9]/,$d' -e '/^call [0-9]/,$d' "$SCAFFOLD" > "$TASK/solution/solve.sh"
  case "$variant" in
    cover_reference|handoff_reference|resolve_reference) reference; reward=1 ;;
    cover_extra_reads)
      add get_schedule '{"schedule_id":"PSPLTPR"}'
      reference
      add list_oncalls '{"schedule_ids":["PSPLTPR"],"since":"2026-10-31T08:59:59Z","until":"2026-10-31T17:00:01Z","earliest":false}'
      reward=1 ;;
    cover_offsets)
      cover PSPLTPR '[{"start":"2026-10-31T04:00:00-05:00","end":"2026-10-31T18:00:00+01:00","user_id":"PJ0RLEE"}]'
      reward=1 ;;
    cover_split)
      cover PSPLTPR '[{"start":"2026-10-31T09:00:00Z","end":"2026-10-31T13:00:00Z","user_id":"PJ0RLEE"},{"start":"2026-10-31T13:00:00Z","end":"2026-10-31T17:00:00Z","user_id":"PJ0RLEE"}]'
      reward=1 ;;
    cover_overlap)
      cover PSPLTPR '[{"start":"2026-10-31T13:00:00Z","end":"2026-10-31T17:00:00Z","user_id":"PJ0RLEE"},{"start":"2026-10-31T09:00:00Z","end":"2026-10-31T14:00:00Z","user_id":"PJ0RLEE"}]'
      reward=1 ;;
    cover_repeated) reference; cover PSPLTPR "$whole"; reward=1 ;;
    cover_fractional_split)
      cover PSPLTPR '[{"start":"2026-10-31T09:00:00Z","end":"2026-10-31T13:00:00.123456789Z","user_id":"PJ0RLEE"},{"start":"2026-10-31T13:00:00.123456789Z","end":"2026-10-31T17:00:00Z","user_id":"PJ0RLEE"}]'
      reward=1 ;;
    cover_native_truncation)
      cover PSPLTPR '[{"start":"2026-10-31T09:00:00.000000001Z","end":"2026-10-31T17:00:00.000000999Z","user_id":"PJ0RLEE"}]'
      reward=1 ;;
    cover_native_offset_truncation)
      cover PSPLTPR '[{"start":"2026-10-31T04:00:00.000000999-05:00","end":"2026-10-31T18:00:00.000000001+01:00","user_id":"PJ0RLEE"}]'
      reward=1 ;;
    cover_fractional_reads)
      reference
      add list_oncalls '{"schedule_ids":["PSPLTPR"],"since":"2026-10-31T09:00:00.123456789Z","until":"2026-10-31T17:00:00.000001Z","earliest":false}'
      reward=1 ;;
    cover_micro_gap)
      cover PSPLTPR '[{"start":"2026-10-31T09:00:00Z","end":"2026-10-31T13:00:00.123456Z","user_id":"PJ0RLEE"},{"start":"2026-10-31T13:00:00.123457Z","end":"2026-10-31T17:00:00Z","user_id":"PJ0RLEE"}]' ;;
    cover_micro_late_start)
      cover PSPLTPR "$(printf '%s' "$whole" | jq '.[0].start="2026-10-31T09:00:00.000001Z"')" ;;
    cover_micro_late_end)
      cover PSPLTPR "$(printf '%s' "$whole" | jq '.[0].end="2026-10-31T17:00:00.000001Z"')" ;;
    cover_native_no_carry)
      cover PSPLTPR "$(printf '%s' "$whole" | jq '.[0].start="2026-10-31T08:59:59.999999999Z"')" ;;
    cover_wrong_user) cover PSPLTPR "$(printf '%s' "$whole" | jq '.[0].user_id="PJ0RALV"')" ;;
    cover_wrong_schedule) cover PSPLTSC "$whole" ;;
    cover_gap)
      cover PSPLTPR '[{"start":"2026-10-31T09:00:00Z","end":"2026-10-31T13:00:00Z","user_id":"PJ0RLEE"},{"start":"2026-10-31T13:00:01Z","end":"2026-10-31T17:00:00Z","user_id":"PJ0RLEE"}]' ;;
    cover_early) cover PSPLTPR "$(printf '%s' "$whole" | jq '.[0].start="2026-10-31T08:59:59Z"')" ;;
    cover_late) cover PSPLTPR "$(printf '%s' "$whole" | jq '.[0].end="2026-10-31T17:00:01Z"')" ;;
    cover_other_schedule) reference; cover PSPAYPR "$whole" ;;
    handoff_note_first)
      note Q3K8ORDBLAG2RX "$handoff_note"
      manage '{"assignement":{"id":"PDNGYN2"}}'
      reward=1 ;;
    handoff_extra_reads)
      add get_incident '{"incident_id":"Q3K8ORDBLAG2RX"}'
      add list_oncalls '{"escalation_policy_ids":["PEPDBRE"],"earliest":false}'
      reference
      add list_incident_notes '{"incident_id":"Q3K8ORDBLAG2RX"}'
      reward=1 ;;
    handoff_repeated_note) reference; note Q3K8ORDBLAG2RX "$handoff_note"; reward=1 ;;
    handoff_wrong_primary)
      manage '{"assignement":{"id":"PS4MOKF"}}'
      note Q3K8ORDBLAG2RX "$handoff_note" ;;
    handoff_ack_restore)
      manage '{"status":"acknowledged"}'
      reference ;;
    handoff_urgency)
      manage '{"urgency":"low"}'
      reference ;;
    handoff_urgency_restore)
      manage '{"urgency":"low"}'
      manage '{"urgency":"high"}'
      reference ;;
    handoff_weak_note)
      manage '{"assignement":{"id":"PDNGYN2"}}'
      note Q3K8ORDBLAG2RX replica-2 ;;
    handoff_other_note) reference; note Q1INVDBLAG7QWE "$handoff_note" ;;
    resolve_first) resolve; note Q7CHKAP15PCT1D "$resolve_note"; reward=1 ;;
    resolve_acknowledge)
      add manage_incidents '{"manage_request":{"incident_ids":["Q7CHKAP15PCT1D"],"status":"acknowledged"}}'
      reference
      reward=1 ;;
    resolve_paraphrase)
      note Q7CHKAP15PCT1D 'Rolling back release 2026.10.29.3 resolved this incident.'
      resolve
      reward=1 ;;
    resolve_repeated_note) reference; note Q7CHKAP15PCT1D "$resolve_note"; reward=1 ;;
    resolve_wrong_note)
      note Q7CHKAP15PCT1D 'Release 2026.10.29.3 was not rolled back; this incident is still broken.'
      resolve ;;
    resolve_extra_incident)
      reference
      add manage_incidents '{"manage_request":{"incident_ids":["Q0AP1GW5XX9LMZ"],"status":"resolved"}}' ;;
    resolve_ack_only)
      note Q7CHKAP15PCT1D "$resolve_note"
      add manage_incidents '{"manage_request":{"incident_ids":["Q7CHKAP15PCT1D"],"status":"acknowledged"}}' ;;
    resolve_other_note) reference; note Q9CHKWEB5XX3VK "$resolve_note" ;;
    *) printf 'unsupported PagerDuty control: %s\n' "$variant" >&2; exit 2 ;;
  esac
  sh -n "$TASK/solution/solve.sh"
  cp -R "$TASK/solution" "$OUT/program-$variant"
  jq -n --arg variant "$variant" --arg slug "$slug" --argjson reward "$reward" \
    '{variant:$variant,scenario:("ops/pagerduty/"+$slug),expected_reward:$reward}' \
    > "$OUT/program-$variant/control.json"
  if [ "${GENERATE_ONLY:-0}" = 1 ]; then
    printf '%s\n' "$variant: program generated; no runtime validation claimed"
    continue
  fi
  sh deploy/run-trial.sh -p "$TASK" -a oracle -o "$OUT/$variant" > "$OUT/$variant.log" 2>&1
  assessment="$(find "$OUT/$variant" -path '*/verifier/reward.json' -print -quit)"
  test -n "$assessment"
  jq -e --argjson expected "$reward" '
    .reward==$expected and .evaluator_version==37 and .contract_version==2
      and .environment_ok==1 and .contract_ok==1 and .integrity_ok==1
      and .runner_admission_checked==1 and .runner_admission_ok==1
      and .preservation_checked==1 and .history_checked==1
      and ($expected==0 or .history_ok==1)
      and .adjudication_needed==0' "$assessment" >/dev/null
  printf '%s\n' "$variant: native program completed with expected audited business outcome"
done
