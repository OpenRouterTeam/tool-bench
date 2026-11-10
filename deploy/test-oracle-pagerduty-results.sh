#!/bin/sh
set -eu
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
WORK="$(mktemp -d /tmp/tool-bench-pagerduty-helper.XXXXXX)"
cleanup() {
  result=$?
  if [ "$result" = 0 ]; then
    rm -rf "$WORK"
  else
    printf 'failed PagerDuty helper fixtures retained: %s\n' "$WORK" >&2
    find "$WORK" -name '*.log' -exec tail -12 {} \; >&2
  fi
}
trap cleanup EXIT
(cd "$ROOT" && cargo run -q --locked -p scenario --bin taskgen -- \
  --out "$WORK/tasks" --world-image fixture:world --tools-image fixture:tools) >/dev/null
ORIGINAL="$WORK/tasks/ops__grafana__silence-replica-maintenance/solution"
mkdir "$WORK/bin"
cat > "$WORK/bin/curl" <<'CURL'
#!/bin/sh
set -eu
inline=""
request=""
while [ "$#" -gt 0 ]; do
  case "$1" in
    -d) inline="$2"; shift 2 ;;
    --data-binary) request="${2#@}"; shift 2 ;;
    *) shift ;;
  esac
done
if [ -n "$inline" ]; then
  if [ "$(printf '%s' "$inline" | jq -r '.method')" = initialize ]; then
    printf 'HTTP/1.1 200 OK\r\nmcp-session-id: fixture\r\n\r\n'
  fi
  exit 0
fi
printf '%s\n' called >> "$CALLS"
mode="$MODE"
if [ "$(wc -l < "$CALLS" | tr -d ' ')" -gt 1 ]; then
  mode=valid
  jq -e --arg expected "Record follow-up for $IDENTITY" \
    '.params.arguments.note==$expected' "$request" >/dev/null
fi
jq -cn --slurpfile request "$request" --arg mode "$mode" '
  $request[0].params.name as $tool
  | $request[0].params.arguments as $arguments
  | (if $tool=="create_schedule_override" then
      [$arguments.override_request.overrides|to_entries[]|{
        status:201,override:{id:("actual-created-override-"+(.key|tostring)),
          start:.value.start,end:.value.end,user:{id:.value.user_id,type:"user_reference"}}}]
     elif $tool=="manage_incidents" then {
       response:[{id:$arguments.manage_request.incident_ids[0],incident_number:4812,
         title:"Error: ordinary incident data",service:{id:"selected-service"},
         status:"triggered",created_at:"2026-10-30T13:12:00Z",updated_at:"2026-10-30T14:00:00Z",
         assignments:[{at:"2026-10-30T14:00:00Z",assignee:{id:$arguments.manage_request.assignement.id}}]}]}
     else {id:"actual-created-note",content:$arguments.note,user:{id:"selected-actor"},
       created_at:"2026-10-30T14:00:00Z"} end) as $success
  | (if $mode=="native_error" then
       if $tool=="create_schedule_override" then $success|.[1].status=400|.[1].errors=["invalid interval"]
       elif $tool=="manage_incidents" then {response:[]}
       else {error:"SDK rejected the operation"} end
     elif $mode=="wrong_payload" then
       if $tool=="create_schedule_override" then $success|.[0].override.user.id="wrong-user"
       elif $tool=="manage_incidents" then $success|.response[0].id="wrong-incident"
       else $success|.content="Not the requested message" end
     else $success end) as $payload
  | (if $tool=="create_schedule_override" then {
       content:[$payload[]|{type:"text",text:tojson}],structuredContent:{result:$payload}}
     else {content:[{type:"text",text:($payload|tojson)}],structuredContent:$payload} end)
  | {jsonrpc:"2.0",id:2,result:.}
  | if $mode=="rpc_error" then del(.result)|.error={code:-32603,message:"failure"}
    elif $mode=="tool_error" then .result.isError=true
    elif $mode=="malformed" then .result.content[0].text="not JSON"
    elif $mode=="structured" then .result.structuredContent={unbound:"different result"}
    elif $mode=="empty" then .result.content=[]
    else . end
'
CURL
chmod +x "$WORK/bin/curl"
for form in override handoff note; do
  directory="$WORK/$form"
  mkdir "$directory"
  cp "$ORIGINAL/"* "$directory/"
  case "$form" in
    override) tool=create_schedule_override
      arguments='{"schedule_id":"selected-schedule","override_request":{"overrides":[{"start":"2026-10-31T09:00:00Z","end":"2026-10-31T13:00:00Z","user_id":"selected-user"},{"start":"2026-10-31T13:00:00Z","end":"2026-10-31T17:00:00Z","user_id":"selected-user"}]}}'
      capture='[{"name":"actual_identity","pointer":"/structuredContent/result/0/override/id"}]'
      identity=actual-created-override-0 ;;
    handoff) tool=manage_incidents
      arguments='{"manage_request":{"incident_ids":["selected-incident"],"assignement":{"id":"selected-user"}}}'
      capture='[{"name":"actual_identity","pointer":"/structuredContent/response/0/id"}]'
      identity=selected-incident ;;
    note) tool=add_note_to_incident
      arguments='{"incident_id":"selected-incident","note":"Error: ordinary requested business text"}'
      capture='[{"name":"actual_identity","pointer":"/structuredContent/id"}]'
      identity=actual-created-note ;;
  esac
  jq -n --arg tool "$tool" --argjson arguments "$arguments" \
    '{jsonrpc:"2.0",id:2,method:"tools/call",params:{name:$tool,arguments:$arguments}}' \
    > "$directory/first.json"
  printf '%s\n' "$capture" > "$directory/first.capture.json"
  jq -n '{jsonrpc:"2.0",id:2,method:"tools/call",params:{name:"add_note_to_incident",
    arguments:{incident_id:"selected-incident",note:"Record follow-up for ${actual_identity}"}}}' \
    > "$directory/second.json"
  printf '%s\n' '[]' > "$directory/second.capture.json"
  for mode in native_error wrong_payload rpc_error tool_error malformed structured empty valid; do
    sed -e '/^bound_call [0-9]/,$d' -e '/^call [0-9]/,$d' \
      "$ORIGINAL/solve.sh" > "$directory/solve.sh"
    printf '%s\n' \
      'bound_call 7111 "$DIR/first.json" "$DIR/first.capture.json" 0' \
      'bound_call 7111 "$DIR/second.json" "$DIR/second.capture.json" 1' \
      'printf "%s\n" completed > "$COMPLETION"' \
      'printf "%s\n" answer > "$ANSWER"' >> "$directory/solve.sh"
    rm -f "$WORK/calls" "$WORK/completion" "$WORK/answer"
    if MODE="$mode" IDENTITY="$identity" CALLS="$WORK/calls" COMPLETION="$WORK/completion" \
        ANSWER="$WORK/answer" PATH="$WORK/bin:$PATH" sh "$directory/solve.sh" \
        > "$WORK/$form-$mode.log" 2>&1; then
      program_status=0
    else
      program_status=$?
    fi
    if [ "$mode" = valid ]; then
      test "$program_status" = 0
      test "$(wc -l < "$WORK/calls" | tr -d ' ')" = 2
      test -s "$WORK/completion" && test -s "$WORK/answer"
    else
      test "$program_status" != 0
      test "$(wc -l < "$WORK/calls" | tr -d ' ')" = 1
      test ! -e "$WORK/completion" && test ! -e "$WORK/answer"
    fi
    printf 'PagerDuty generated helper: %s/%s verified\n' "$form" "$mode"
  done
done
