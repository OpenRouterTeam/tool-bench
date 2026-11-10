#!/bin/sh
set -eu
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
WORLD_IMAGE="${WORLD_IMAGE:?provide the frozen Grafana35 world candidate}"
TOOLS_IMAGE="${TOOLS_IMAGE:?provide the matching Grafana tools candidate}"
OUT="$(mktemp -d /tmp/tool-bench-grafana-controls.XXXXXX)"
printf 'artifacts: %s\n' "$OUT"
if [ -n "${TASKGEN_BINARY:-}" ]; then
  "$TASKGEN_BINARY" --out "$OUT/tasks" --world-image "$WORLD_IMAGE" --tools-image "$TOOLS_IMAGE" >/dev/null
else
  cargo run -q --locked -p scenario --bin taskgen -- --out "$OUT/tasks" \
    --world-image "$WORLD_IMAGE" --tools-image "$TOOLS_IMAGE" >/dev/null
fi
for slug in ack-checkout-page-oncall annotate-checkout-deploy silence-replica-maintenance; do
  mkdir "$OUT/$slug-original"
  cp "$OUT/tasks/ops__grafana__$slug/solution/"* "$OUT/$slug-original/"
done
add() {
  jq -n --arg tool "$1" --argjson arguments "$2" \
    '{jsonrpc:"2.0",id:2,method:"tools/call",params:{name:$tool,arguments:$arguments}}' \
    > "$TASK/solution/control-$calls.json"
  printf '%s\n' "${3:-[]}" > "$TASK/solution/control-$calls.capture.json"
  printf 'bound_call 7114 "$DIR/control-%s.json" "$DIR/control-%s.capture.json" %s\n' \
    "$calls" "$calls" "${4:-0}" >> "$TASK/solution/solve.sh"
  calls=$((calls + 1))
}
reference() {
  printf '%s\n' 'sh "$DIR/reference-solve.sh"' >> "$TASK/solution/solve.sh"
}
api() {
  add grafana_api_request "$(jq -cn --arg endpoint "$1" --arg method "$2" \
    --argjson body "${3:-null}" \
    '{endpoint:$endpoint,method:$method} + (if $body==null then {} else {body:($body|tojson)} end)')" \
    "${4:-[]}"
}
literal_answer() {
  quoted="$(jq -nr --arg answer "$1" '$answer|@sh')"
  printf 'printf "%%s\\n" %s > /app/answer.md\n' "$quoted" >> "$TASK/solution/solve.sh"
}
current_users() {
  add get_current_oncall_users '{"scheduleId":"SCK1PRIM"}' \
    '[{"name":"native_users","pointer":"/content/0/text","json_pointer":"/users"},
      {"name":"native_schedule","pointer":"/content/0/text","json_pointer":"/scheduleId"},
      {"name":"native_schedule_name","pointer":"/content/0/text","json_pointer":"/scheduleName"}]'
}
annotate() {
  add create_annotation "$(jq -cn --argjson changes "$1" \
    '{dashboardUid:"checkout-prod",time:1794492300000,text:"Deploy checkout v4.19.0",
      tags:["deploy","checkout"]} + $changes')"
}
create_silence() {
  add alerting_silences_write "$(jq -cn --argjson changes "$1" \
    '{operation:"create",starts_at:"2026-11-13T02:00:00Z",ends_at:"2026-11-13T04:00:00Z",
      comment:"CHG-7781 storage maintenance on pg-replica-2",created_by:"sre",
      matchers:[{name:"alertname",value:"PostgresReplicationLag"},
        {name:"instance",value:"pg-replica-2"}]} + $changes')"
}
if [ "$#" -eq 0 ]; then
  set -- ack_reference ack_extra_reads ack_username_answer ack_structured_answer \
    ack_wrong_answer ack_wrong_group ack_resolve_restore ack_datasource_edit \
    ann_reference ann_numeric_dashboard ann_uid_precedence ann_latency_panel ann_extra_tags \
    ann_wrong_panel ann_region ann_wrong_text ann_extra_annotation ann_restore_old \
    sil_reference sil_offset_regex sil_redundant sil_other_creator sil_broad \
    sil_narrow sil_wrong_ticket sil_extra sil_existing_edit
fi
for variant do
  calls=0
  reward=0
  outcome=1
  case "$variant" in
    ack_*) slug=ack-checkout-page-oncall ;;
    ann_*) slug=annotate-checkout-deploy ;;
    sil_*) slug=silence-replica-maintenance ;;
    *) printf 'unsupported Grafana control: %s\n' "$variant" >&2; exit 2 ;;
  esac
  TASK="$OUT/tasks/ops__grafana__$slug"
  rm -rf "$TASK/solution"
  mkdir "$TASK/solution"
  cp "$OUT/$slug-original/"* "$TASK/solution/"
  cp "$TASK/solution/solve.sh" "$TASK/solution/reference-solve.sh"
  cp "$OUT/silence-replica-maintenance-original/"*.jq "$TASK/solution/"
  sed -e '/^bound_call [0-9]/,$d' -e '/^call [0-9]/,$d' \
    "$OUT/silence-replica-maintenance-original/solve.sh" > "$TASK/solution/solve.sh"
  case "$variant" in
    ack_reference|ann_reference|sil_reference) reference; reward=1; outcome=0 ;;
    ack_extra_reads)
      add get_alert_group '{"alertGroupId":"IHQ4K2M7"}'
      reference
      api /api/datasources GET
      reward=1; outcome=0 ;;
    ack_username_answer|ack_structured_answer)
      add update_alert_group '{"alertGroupId":"IHQ4K2M7","state":"acknowledged"}'
      current_users
      if [ "$variant" = ack_username_answer ]; then
        printf '%s\n' 'jq -er '"'"'.native_users | if length==1 then .[0].username else error("control requires one observed assignee") end'"'"' "$WORK/bindings.json" > /app/answer.md' \
          >> "$TASK/solution/solve.sh"
      else
        printf '%s\n' 'jq -c '"'"'{scheduleId:.native_schedule,scheduleName:.native_schedule_name,users:.native_users}'"'"' "$WORK/bindings.json" > /app/answer.md' \
          >> "$TASK/solution/solve.sh"
      fi
      reward=1; outcome=0 ;;
    ack_wrong_answer)
      reference
      literal_answer 'Brennan is not on call. The primary on-call person is Mei Chen.' ;;
    ack_wrong_group)
      add update_alert_group '{"alertGroupId":"IST7B3N1","state":"acknowledged"}'
      reference ;;
    ack_resolve_restore)
      reference
      api /oncall/api/v1/alert_groups/IHQ4K2M7/resolve/ POST
      api /oncall/api/v1/alert_groups/IHQ4K2M7/unresolve/ POST ;;
    ack_datasource_edit)
      api /api/datasources/uid/loki-prod PUT '{"name":"Unrequested rename"}'
      reference ;;
    ann_numeric_dashboard)
      api /api/annotations POST \
        '{"dashboardId":101,"time":1794492300000,"text":"Deploy checkout v4.19.0","tags":["checkout","deploy"]}'
      reward=1; outcome=0 ;;
    ann_uid_precedence)
      api /api/annotations POST \
        '{"dashboardUID":"checkout-prod","dashboardId":102,"time":1794492300000,"text":"Deploy checkout v4.19.0","tags":["deploy","checkout"]}'
      reward=1; outcome=0 ;;
    ann_latency_panel) annotate '{"panelId":3}'; reward=1; outcome=0 ;;
    ann_extra_tags) annotate '{"tags":["checkout","release","deploy"]}'; reward=1; outcome=0 ;;
    ann_wrong_panel) annotate '{"panelId":1}' ;;
    ann_region) annotate '{"timeEnd":1794492360000}' ;;
    ann_wrong_text)
      annotate '{"text":"Do not deploy checkout v4.19.0; the release was canceled"}' ;;
    ann_extra_annotation)
      reference
      annotate '{"dashboardUid":"orders-db","text":"Unrequested extra annotation"}' ;;
    ann_restore_old)
      api /api/annotations/38 GET null \
        '[{"name":"prior_text","pointer":"/content/0/text","json_pointer":"/data/text"}]'
      add update_annotation '{"id":38,"text":"Unrequested old annotation rewrite"}'
      reference
      add update_annotation '{"id":38,"text":"${prior_text}"}' '[]' 1 ;;
    sil_offset_regex)
      create_silence '{"starts_at":"2026-11-13T03:00:00+01:00","ends_at":"2026-11-13T05:00:00+01:00",
        "matchers":[{"name":"instance","value":"^(?:pg-replica-2)$","isRegex":true},
          {"name":"alertname","value":"PostgresReplicationLag"}]}'
      reward=1; outcome=0 ;;
    sil_redundant)
      create_silence '{"matchers":[{"name":"alertname","value":"PostgresReplicationLag"},
        {"name":"instance","value":"pg-replica-2"},
        {"name":"instance","value":"pg-replica-.*","isRegex":true},
        {"name":"instance","value":"pg-replica-1","isEqual":false}]}'
      reward=1; outcome=0 ;;
    sil_other_creator)
      create_silence '{"created_by":"Operations automation"}'
      reward=1; outcome=0 ;;
    sil_broad)
      create_silence '{"matchers":[{"name":"alertname","value":"PostgresReplicationLag"},
        {"name":"instance","value":"pg-replica-.*","isRegex":true}]}' ;;
    sil_narrow)
      create_silence '{"matchers":[{"name":"alertname","value":"PostgresReplicationLag"},
        {"name":"instance","value":"pg-replica-2"},{"name":"environment","value":"prod"}]}' ;;
    sil_wrong_ticket) create_silence '{"comment":"CHG-77810 unrelated maintenance"}' ;;
    sil_extra)
      reference
      create_silence '{"matchers":[{"name":"alertname","value":"PostgresReplicationLag"}]}' ;;
    sil_existing_edit)
      add alerting_silences_read '{"operation":"get","silence_id":"8d2e4f6a-1b3c-4d5e-9f0a-7b8c9d0e1f2a"}' \
        '[{"name":"prior_matchers","pointer":"/content/0/text","json_pointer":"/matchers"},
          {"name":"prior_start","pointer":"/content/0/text","json_pointer":"/startsAt"},
          {"name":"prior_end","pointer":"/content/0/text","json_pointer":"/endsAt"},
          {"name":"prior_comment","pointer":"/content/0/text","json_pointer":"/comment"}]'
      add alerting_silences_write \
        '{"operation":"update","silence_id":"8d2e4f6a-1b3c-4d5e-9f0a-7b8c9d0e1f2a",
          "matchers":"${prior_matchers}","starts_at":"${prior_start}","ends_at":"${prior_end}",
          "comment":"${prior_comment}","created_by":"Unrequested replacement author"}' '[]' 1
      reference ;;
    *) printf 'unsupported Grafana control: %s\n' "$variant" >&2; exit 2 ;;
  esac
  sh -n "$TASK/solution/solve.sh"
  cp -R "$TASK/solution" "$OUT/program-$variant"
  if [ "${GENERATE_ONLY:-0}" = 1 ]; then
    printf '%s\n' "$variant: generated program syntax checked"
    continue
  fi
  sh deploy/run-trial.sh -p "$TASK" -a oracle -o "$OUT/$variant" > "$OUT/$variant.log" 2>&1
  result="$(find "$OUT/$variant" -path '*/verifier/reward.json' -print -quit)"
  test -n "$result"
  jq -e --argjson reward "$reward" --argjson outcome "$outcome" '
    .reward==$reward and .outcome_code==$outcome and .evaluator_version==35
      and .environment_ok==1 and .contract_ok==1 and .integrity_ok==1
      and .runner_admission_checked==1 and .runner_admission_ok==1
      and .preservation_checked==1 and .history_checked==1 and .adjudication_needed==0' "$result" >/dev/null
  printf '%s\n' "$variant: native control reward=$reward outcome=$outcome"
done
