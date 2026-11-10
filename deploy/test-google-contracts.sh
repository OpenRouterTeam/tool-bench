#!/bin/sh
set -eu
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
WORLD_IMAGE="${WORLD_IMAGE:?provide the declared Google40B world image}"
TOOLS_IMAGE="${TOOLS_IMAGE:?provide the matching tools image}"
OUT="${CONTROL_OUT:-$(mktemp -d /tmp/tool-bench-google-controls.XXXXXXXX)}"
test -d "$OUT"
test -z "$(find "$OUT" -mindepth 1 -maxdepth 1 -print -quit)"
printf 'artifacts: %s\n' "$OUT"
if [ -n "${TASKGEN_BINARY:-}" ]; then
  "$TASKGEN_BINARY" --out "$OUT/tasks" --world-image "$WORLD_IMAGE" --tools-image "$TOOLS_IMAGE" >/dev/null
else
  cargo run -q --locked -p scenario --bin taskgen -- --out "$OUT/tasks" \
    --world-image "$WORLD_IMAGE" --tools-image "$TOOLS_IMAGE" >/dev/null
fi
for slug in mark-northwind-renewed schedule-q1-planning share-soc2-with-contoso; do
  mkdir "$OUT/$slug-original"
  cp "$OUT/tasks/collab__google__$slug/solution/"* "$OUT/$slug-original/"
done
SCAFFOLD="$OUT/tasks/ops__grafana__silence-replica-maintenance/solution/solve.sh"
BOOK=1p91PmGrfYlol_BkGN9Op8j9dWzibNwgg
REPORT=150dluRbaiTmEP9Spu-wAk0p_lQupubYY
add() {
  jq -n --arg tool "$1" --argjson arguments "$2" \
    '{jsonrpc:"2.0",id:2,method:"tools/call",params:{name:$tool,arguments:$arguments}}' \
    > "$TASK/solution/control-$calls.json"
  printf '%s\n' '[]' > "$TASK/solution/control-$calls.capture.json"
  printf 'bound_call 7130 "$DIR/control-%s.json" "$DIR/control-%s.capture.json" 0\n' \
    "$calls" "$calls" >> "$TASK/solution/solve.sh"
  calls=$((calls + 1))
}
reference() {
  printf '%s\n' 'sh "$DIR/reference-solve.sh"' >> "$TASK/solution/solve.sh"
}
renew() {
  add modify_sheet_values "$(jq -cn --arg book "$BOOK" --argjson changes "$1" \
    '{spreadsheet_id:$book,range_name:"Renewals!C2:D2",values:[["2027-11-30","Renewed"]]}+$changes')"
}
plan() {
  add manage_event "$(jq -cn --argjson changes "$1" \
    '{action:"create",summary:"Q1 planning sync",start_time:"2026-11-03T19:00:00Z",
      end_time:"2026-11-03T19:45:00Z",
      attendees:["priya.natarajan@octo-corp.example","marcus.webb@octo-corp.example"],
      add_google_meet:true}+$changes')"
}
share() {
  add manage_drive_access "$(jq -cn --arg report "$REPORT" --argjson changes "$1" \
    '{file_id:$report,action:"grant",share_with:"dana.whitfield@contoso.example",
      share_type:"user",role:"reader"}+$changes')"
}
followup_meet() {
  cat >> "$TASK/solution/solve.sh" <<'PROGRAM'
mkdir -p /logs/artifacts/google-control
ARTIFACTS=/logs/artifacts/google-control
sh "$DIR/reference-solve.sh" > "$ARTIFACTS/reference.raw"
cat "$ARTIFACTS/reference.raw"
jq -sre '
  [.[] | .result.structuredContent.result? | select(type=="string")
    | select(startswith("Successfully created event "))]
  | if length!=1 then error("one actual creation confirmation required") else .[0] end
  | capture("Link: https://www[.]google[.]com/calendar/event[?]eid=(?<encoded>[^\\r\\n]+)").encoded
  | @base64d | split(" ")
  | if length!=2 or .[1]!="ops@octo-corp.example" or .[0]==""
    then error("actual event identity/calendar not established") else .[0] end
' "$ARTIFACTS/reference.raw" > "$ARTIFACTS/created-event-id.txt"
jq -n --rawfile identity "$ARTIFACTS/created-event-id.txt" '
  {jsonrpc:"2.0",id:2,method:"tools/call",params:{name:"manage_event",
    arguments:{action:"update",event_id:($identity|rtrimstr("\n")),
      add_google_meet:true,send_updates:"none"}}}
' > "$ARTIFACTS/followup.json"
call 7130 "$ARTIFACTS/followup.json"
PROGRAM
}
if [ "$#" -eq 0 ]; then
  set -- renew_reference renew_serial renew_us_date renew_split renew_clear_status renew_reads \
    renew_wrong_date renew_wrong_status renew_wrong_row renew_old_book renew_neighbor_restore renew_notes \
    plan_reference plan_utc plan_offset_zone plan_guest_objects plan_none plan_external \
    plan_silent_meet plan_description plan_wrong_instant plan_wrong_end plan_missing_guest \
    plan_extra_guest plan_no_meet plan_duplicate plan_recurring plan_wrong_calendar \
    plan_cancelled_description plan_existing_restore \
    share_reference share_uppercase share_batch share_no_notification share_reads share_role_noop \
    share_writer share_commenter share_wrong_principal share_old_report share_anyone \
    share_extra_recipient share_owner_restore
fi
for variant do
  calls=0
  reward=0
  case "$variant" in
    renew_*) slug=mark-northwind-renewed ;;
    plan_*) slug=schedule-q1-planning ;;
    share_*) slug=share-soc2-with-contoso ;;
    *) printf 'unsupported Google control: %s\n' "$variant" >&2; exit 2 ;;
  esac
  TASK="$OUT/tasks/collab__google__$slug"
  rm -rf "$TASK/solution"
  mkdir "$TASK/solution"
  cp "$OUT/$slug-original/"* "$TASK/solution/"
  cp "$TASK/solution/solve.sh" "$TASK/solution/reference-solve.sh"
  cp "$OUT/tasks/ops__grafana__silence-replica-maintenance/solution/"*.jq "$TASK/solution/"
  sed -e '/^bound_call [0-9]/,$d' -e '/^call [0-9]/,$d' "$SCAFFOLD" > "$TASK/solution/solve.sh"
  case "$variant" in
    renew_reference|plan_reference|share_reference) reference; reward=1 ;;
    renew_serial) renew '{"values":"[[46721,\"Renewed\"]]"}'; reward=1 ;;
    renew_us_date) renew '{"values":[["11/30/2027","Renewed"]]}'; reward=1 ;;
    renew_split)
      renew '{"range_name":"Renewals!D2","values":[["Renewed"]]}'
      renew '{"range_name":"Renewals!C2","values":[["2027-11-30"]]}'
      reward=1 ;;
    renew_clear_status)
      renew '{"range_name":"Renewals!D2","clear_values":true,"values":[]}'
      renew '{}'
      reward=1 ;;
    renew_reads)
      add read_sheet_values "$(jq -cn --arg book "$BOOK" '{spreadsheet_id:$book,range_name:"Archive!A1:E10"}')"
      reference
      add read_sheet_values "$(jq -cn --arg book "$BOOK" '{spreadsheet_id:$book,range_name:"Renewals!A1:E10"}')"
      reward=1 ;;
    renew_wrong_date) renew '{"values":[["2040-01-01","Renewed"]]}' ;;
    renew_wrong_status) renew '{"values":[["2027-11-30","Pending renewal"]]}' ;;
    renew_wrong_row) renew '{"range_name":"Renewals!C3:D3"}' ;;
    renew_old_book) renew '{"spreadsheet_id":"1_x8cbfTBtuujAmxfx9ZG2B5yGE5HDyhh"}' ;;
    renew_neighbor_restore)
      reference
      renew '{"range_name":"Renewals!C4","values":[["2040-01-01"]]}'
      renew '{"range_name":"Renewals!C4","values":[["2027-03-31"]]}' ;;
    renew_notes)
      reference
      renew '{"range_name":"Renewals!E2","values":[["Unrequested note"]]}' ;;
    plan_utc) plan '{}'; reward=1 ;;
    plan_offset_zone)
      plan '{"start_time":"2026-11-03T14:00:00-05:00","end_time":"2026-11-03T14:45:00-05:00","timezone":"Asia/Tokyo"}'
      reward=1 ;;
    plan_guest_objects)
      plan '{"attendees":[{"email":"MARCUS.WEBB@OCTO-CORP.EXAMPLE"},{"email":"priya.natarajan@octo-corp.example"}]}'
      reward=1 ;;
    plan_none) plan '{"send_updates":"none"}'; reward=1 ;;
    plan_external) plan '{"send_updates":"externalOnly"}'; reward=1 ;;
    plan_silent_meet)
      jq '.params.arguments.add_google_meet=false' "$TASK/solution/call-2.json" \
        > "$TASK/solution/call-2.updated.json"
      mv "$TASK/solution/call-2.updated.json" "$TASK/solution/call-2.json"
      followup_meet
      reward=1 ;;
    plan_description) plan '{"description":"Q1 planning sync with Priya and Marcus."}'; reward=1 ;;
    plan_wrong_instant) plan '{"start_time":"2026-11-03T19:00:00.000000001Z"}' ;;
    plan_wrong_end) plan '{"end_time":"2026-11-03T19:44:59.999999999Z"}' ;;
    plan_missing_guest) plan '{"attendees":["priya.natarajan@octo-corp.example"]}' ;;
    plan_extra_guest)
      plan '{"attendees":["priya.natarajan@octo-corp.example","marcus.webb@octo-corp.example","sam.ortiz@octo-corp.example"]}' ;;
    plan_no_meet) plan '{"add_google_meet":false}' ;;
    plan_duplicate) plan '{}'; plan '{}' ;;
    plan_recurring) plan '{"recurrence":["RRULE:FREQ=WEEKLY;COUNT=2"]}' ;;
    plan_wrong_calendar) plan '{"calendar_id":"c_ops_team@group.calendar.google.com"}' ;;
    plan_cancelled_description) plan '{"description":"Meeting cancelled."}' ;;
    plan_existing_restore)
      reference
      add manage_event '{"action":"update","event_id":"ae2a3372cbeee330eecbe86872","summary":"Unrequested title","send_updates":"none"}'
      add manage_event '{"action":"update","event_id":"ae2a3372cbeee330eecbe86872","summary":"Q4 planning sync","send_updates":"none"}' ;;
    share_uppercase) share '{"share_with":"DANA.WHITFIELD@CONTOSO.EXAMPLE"}'; reward=1 ;;
    share_batch)
      share '{"action":"grant_batch","recipients":[{"email":"dana.whitfield@contoso.example","role":"reader","share_type":"user"}]}'
      reward=1 ;;
    share_no_notification)
      share '{"send_notification":false}'
      add get_drive_file_permissions "$(jq -cn --arg file "$REPORT" '{file_id:$file}')"
      reward=1 ;;
    share_reads)
      add get_drive_file_permissions "$(jq -cn --arg file "$REPORT" '{file_id:$file}')"
      reference
      add get_drive_shareable_link "$(jq -cn --arg file "$REPORT" '{file_id:$file}')"
      reward=1 ;;
    share_role_noop)
      reference
      add manage_drive_access "$(jq -cn --arg report "$REPORT" \
        '{file_id:$report,action:"update",permission_id:"16118283490155935341",role:"owner"}')"
      reward=1 ;;
    share_writer) share '{"role":"writer"}' ;;
    share_commenter) share '{"role":"commenter"}' ;;
    share_wrong_principal) share '{"share_with":"dana.whitfield@octo-corp.example"}' ;;
    share_old_report) share '{"file_id":"12klLxpfyaiox7A1n8sigN7b3qTt3DYXh"}' ;;
    share_anyone) share '{"share_type":"anyone"}' ;;
    share_extra_recipient)
      reference
      share '{"share_with":"sam.ortiz@octo-corp.example"}' ;;
    share_owner_restore)
      reference
      add manage_drive_access "$(jq -cn --arg report "$REPORT" \
        '{file_id:$report,action:"update",permission_id:"16118283490155935341",role:"writer"}')"
      add manage_drive_access "$(jq -cn --arg report "$REPORT" \
        '{file_id:$report,action:"update",permission_id:"16118283490155935341",role:"owner"}')" ;;
    *) printf 'unsupported Google control: %s\n' "$variant" >&2; exit 2 ;;
  esac
  sh -n "$TASK/solution/solve.sh"
  cp -R "$TASK/solution" "$OUT/program-$variant"
  cargo run -q --locked -p scenario --bin programcheck -- google-workspace "$OUT/program-$variant" \
    > "$OUT/$variant-schema-admission.json"
  jq -n --arg variant "$variant" --arg slug "$slug" --argjson reward "$reward" \
    '{variant:$variant,endpoint:"google-workspace",scenario:("collab/google/"+$slug),
      expected_reward:$reward,scope:"scripted_selected_replica_not_sdk_hosted_or_model_receipt"}' \
    > "$OUT/program-$variant/control.json"
  if [ "${GENERATE_ONLY:-0}" = 1 ]; then
    printf '%s\n' "$variant: schema-checked program generated; no runtime validation claimed"
    continue
  fi
  sh deploy/run-trial.sh -p "$TASK" -a oracle -o "$OUT/$variant" > "$OUT/$variant.log" 2>&1
  assessment="$(find "$OUT/$variant" -path '*/verifier/reward.json' -print -quit)"
  test -n "$assessment"
  jq -e --argjson expected "$reward" '
    .reward==$expected and .evaluator_version==40 and .contract_version==2
      and .outcome_code==(if $expected==1 then 0 else 1 end)
      and .environment_ok==1 and .contract_ok==1 and .integrity_ok==1
      and .runner_admission_checked==1 and .runner_admission_ok==1
      and .preservation_checked==1 and .history_checked==1
      and ($expected==0 or (.history_ok==1 and .preservation_ok==1))
      and .adjudication_needed==0' "$assessment" >/dev/null
  trial="$(dirname "$(dirname "$assessment")")"
  if [ "$variant" = plan_silent_meet ]; then
    cargo run -q --locked -p scenario --bin programcheck -- google-workspace \
      "$trial/artifacts/logs/artifacts/google-control" > "$OUT/$variant-runtime-schema.json"
  fi
  PYTHONDONTWRITEBYTECODE=1 "${GOOGLE_AUDIT_PYTHON:-python3}" deploy/audit-google-business.py \
    "$trial/verifier" > "$OUT/$variant-independent-audit.log" 2>&1
  printf '%s\n' "$variant: native program completed with expected independent business/history agreement"
done
