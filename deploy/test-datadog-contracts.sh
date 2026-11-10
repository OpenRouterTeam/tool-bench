#!/bin/sh
set -eu
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
WORLD_IMAGE="${WORLD_IMAGE:?provide the Datadog38 world candidate}"
TOOLS_IMAGE="${TOOLS_IMAGE:?provide the matching tools candidate}"
OUT="${CONTROL_OUT:-$(mktemp -d /tmp/tool-bench-datadog-controls.XXXXXX)}"
test -d "$OUT"
test -z "$(find "$OUT" -mindepth 1 -maxdepth 1 -print -quit)"
printf 'artifacts: %s\n' "$OUT"
if [ -n "${TASKGEN_BINARY:-}" ]; then
  "$TASKGEN_BINARY" --out "$OUT/tasks" --world-image "$WORLD_IMAGE" --tools-image "$TOOLS_IMAGE" >/dev/null
else
  cargo run -q --locked -p scenario --bin taskgen -- --out "$OUT/tasks" \
    --world-image "$WORLD_IMAGE" --tools-image "$TOOLS_IMAGE" >/dev/null
fi
for slug in cancel-called-off-maintenance mute-host-from-error-logs schedule-replica-downtime; do
  mkdir "$OUT/$slug-original"
  cp "$OUT/tasks/ops__datadog__$slug/solution/"* "$OUT/$slug-original/"
done
SCAFFOLD="$OUT/tasks/ops__grafana__silence-replica-maintenance/solution/solve.sh"
add() {
  jq -n --arg tool "$1" --argjson arguments "$2" \
    '{jsonrpc:"2.0",id:2,method:"tools/call",params:{name:$tool,arguments:$arguments}}' \
    > "$TASK/solution/control-$calls.json"
  printf '%s\n' '[]' > "$TASK/solution/control-$calls.capture.json"
  printf 'bound_call 7112 "$DIR/control-%s.json" "$DIR/control-%s.capture.json" 0\n' \
    "$calls" "$calls" >> "$TASK/solution/solve.sh"
  calls=$((calls + 1))
}
reference() {
  printf '%s\n' 'sh "$DIR/reference-solve.sh"' >> "$TASK/solution/solve.sh"
}
logs() {
  add get_logs "$(jq -cn --arg query "$1" --argjson limit "${2:-100}" \
    '{query:$query,from:1793365200,to:1793370600,limit:$limit}')"
}
evidence() {
  logs 'service:checkout-api env:prod status:error "connection reset by peer"'
}
mute() {
  add mute_host "$(jq -cn --argjson changes "$1" \
    '{hostname:"checkout-api-prod-1b",end:1793383200,message:"Draining INC-2291"}+$changes')"
}
schedule() {
  add schedule_downtime "$(jq -cn --argjson changes "$1" \
    '{scope:"host:pg-prod-replica-2",monitorId:7301,start:1793412000,end:1793419200,
      message:"Replica rebuild CHG-5012"}+$changes')"
}
if [ "$#" -eq 0 ]; then
  set -- cancel_reference cancel_direct cancel_repeated cancel_extra_reads \
    cancel_wrong_staging cancel_old_only cancel_other_staging cancel_other_old cancel_extra_downtime \
    mute_reference mute_broad_logs mute_single_log mute_lower_message mute_ticket_link mute_override mute_extra_reads \
    mute_no_logs mute_late_logs mute_empty_logs mute_wrong_logs mute_wrong_host mute_wrong_end \
    mute_wrong_message mute_collateral_restore \
    schedule_reference schedule_redundant_scope schedule_timezone schedule_no_timezone \
    schedule_lower_message schedule_ticket_link schedule_direct \
    schedule_broad_host schedule_contradictory_host schedule_wrong_environment schedule_wrong_monitor \
    schedule_fractional_start schedule_early_end schedule_duplicate schedule_recurring \
    schedule_wrong_message schedule_collateral_mute schedule_existing_cancel
fi
for variant do
  calls=0
  reward=0
  case "$variant" in
    cancel_*) slug=cancel-called-off-maintenance ;;
    mute_*) slug=mute-host-from-error-logs ;;
    schedule_*) slug=schedule-replica-downtime ;;
    *) printf 'unsupported Datadog control: %s\n' "$variant" >&2; exit 2 ;;
  esac
  TASK="$OUT/tasks/ops__datadog__$slug"
  rm -rf "$TASK/solution"
  mkdir "$TASK/solution"
  cp "$OUT/$slug-original/"* "$TASK/solution/"
  cp "$TASK/solution/solve.sh" "$TASK/solution/reference-solve.sh"
  cp "$OUT/tasks/ops__grafana__silence-replica-maintenance/solution/"*.jq "$TASK/solution/"
  sed -e '/^bound_call [0-9]/,$d' -e '/^call [0-9]/,$d' "$SCAFFOLD" > "$TASK/solution/solve.sh"
  case "$variant" in
    cancel_reference|mute_reference|schedule_reference) reference; reward=1 ;;
    cancel_direct) add cancel_downtime '{"downtimeId":2207731}'; reward=1 ;;
    cancel_repeated) reference; add cancel_downtime '{"downtimeId":2207731}'; reward=1 ;;
    cancel_extra_reads)
      add get_monitors '{"name":"payments"}'
      reference
      add list_downtimes '{}'
      reward=1 ;;
    cancel_wrong_staging) add cancel_downtime '{"downtimeId":2207752}' ;;
    cancel_old_only) add cancel_downtime '{"downtimeId":2207690}' ;;
    cancel_other_staging) reference; add cancel_downtime '{"downtimeId":2207752}' ;;
    cancel_other_old) reference; add cancel_downtime '{"downtimeId":2207690}' ;;
    cancel_extra_downtime)
      reference
      add schedule_downtime '{"scope":"env:staging","start":1793400000,"end":1793403600,"message":"Unrequested maintenance"}' ;;
    mute_broad_logs) logs 'env:prod'; mute '{}'; reward=1 ;;
    mute_single_log) logs 'host:checkout-api-prod-1b status:error' 1; mute '{}'; reward=1 ;;
    mute_lower_message) evidence; mute '{"message":"inc-2291: replacement"}'; reward=1 ;;
    mute_ticket_link)
      evidence
      mute '{"message":"Draining: https://ops.example/incidents/INC-2291-postmortem"}'
      reward=1 ;;
    mute_override) reference; mute '{"override":true,"message":"Replacement INC-2291"}'; reward=1 ;;
    mute_extra_reads)
      add list_hosts '{"filter":"env:prod"}'
      reference
      add list_hosts '{"filter":"host:checkout-api-prod-1b","include_muted_hosts_data":true}'
      reward=1 ;;
    mute_no_logs) mute '{}' ;;
    mute_late_logs) mute '{}'; evidence ;;
    mute_empty_logs) logs 'host:missing-host'; mute '{}' ;;
    mute_wrong_logs) logs 'host:checkout-api-staging-1b'; mute '{}' ;;
    mute_wrong_host) evidence; mute '{"hostname":"checkout-api-prod-1a"}' ;;
    mute_wrong_end) evidence; mute '{"end":1793383199}' ;;
    mute_wrong_message) evidence; mute '{"message":"Working on INC-22910"}' ;;
    mute_collateral_restore)
      reference
      add mute_host '{"hostname":"checkout-api-prod-1a","end":1793383200,"message":"Unrequested suppression"}'
      add unmute_host '{"hostname":"checkout-api-prod-1a"}' ;;
    schedule_redundant_scope)
      schedule '{"scope":"role:replica,host:pg-prod-replica-2,env:prod,host:pg-prod-*"}'
      reward=1 ;;
    schedule_timezone)
      schedule '{"timezone":"America/Argentina/Salta"}'
      reward=1 ;;
    schedule_no_timezone) schedule '{}'; reward=1 ;;
    schedule_lower_message) schedule '{"message":"Replica rebuild (chg-5012)"}'; reward=1 ;;
    schedule_ticket_link)
      schedule '{"message":"https://ops.example/changes/CHG-5012-rebuild"}'
      reward=1 ;;
    schedule_direct) schedule '{"timezone":"UTC"}'; reward=1 ;;
    schedule_broad_host) schedule '{"scope":"host:pg-prod-*"}' ;;
    schedule_contradictory_host) schedule '{"scope":"host:pg-prod-replica-2,host:pg-prod-replica-1"}' ;;
    schedule_wrong_environment) schedule '{"scope":"host:pg-prod-replica-2,env:staging"}' ;;
    schedule_wrong_monitor) schedule '{"monitorId":7302}' ;;
    schedule_fractional_start) schedule '{"start":1793412000.75}' ;;
    schedule_early_end) schedule '{"end":1793419199}' ;;
    schedule_duplicate) schedule '{}'; schedule '{}' ;;
    schedule_recurring) schedule '{"recurrence":{"type":"days","period":1}}' ;;
    schedule_wrong_message) schedule '{"message":"CHG-50120 approved"}' ;;
    schedule_collateral_mute)
      reference
      add mute_host '{"hostname":"pg-prod-primary-1","end":1793419200,"message":"Unrequested suppression"}' ;;
    schedule_existing_cancel) reference; add cancel_downtime '{"downtimeId":2318849}' ;;
    *) printf 'unsupported Datadog control: %s\n' "$variant" >&2; exit 2 ;;
  esac
  sh -n "$TASK/solution/solve.sh"
  cp -R "$TASK/solution" "$OUT/program-$variant"
  jq -n --arg variant "$variant" --arg slug "$slug" --argjson reward "$reward" \
    '{variant:$variant,endpoint:"datadog",scenario:("ops/datadog/"+$slug),expected_reward:$reward}' \
    > "$OUT/program-$variant/control.json"
  if [ "${GENERATE_ONLY:-0}" = 1 ]; then
    printf '%s\n' "$variant: program generated; no runtime validation claimed"
    continue
  fi
  sh deploy/run-trial.sh -p "$TASK" -a oracle -o "$OUT/$variant" > "$OUT/$variant.log" 2>&1
  assessment="$(find "$OUT/$variant" -path '*/verifier/reward.json' -print -quit)"
  test -n "$assessment"
  jq -e --argjson expected "$reward" '
    .reward==$expected and .evaluator_version==38 and .contract_version==2
      and .outcome_code==(if $expected==1 then 0 else 1 end)
      and .environment_ok==1 and .contract_ok==1 and .integrity_ok==1
      and .runner_admission_checked==1 and .runner_admission_ok==1
      and .preservation_checked==1 and .history_checked==1
      and ($expected==0 or .history_ok==1)
      and .adjudication_needed==0' "$assessment" >/dev/null
  printf '%s\n' "$variant: native program completed with expected audited business outcome"
done
