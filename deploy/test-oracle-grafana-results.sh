#!/bin/sh
set -eu
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
WORK="$(mktemp -d /tmp/tool-bench-grafana-helper.XXXXXX)"
cleanup() {
  result=$?
  if [ "$result" = 0 ]; then
    rm -rf "$WORK"
  else
    printf 'failed Grafana helper fixtures retained: %s\n' "$WORK" >&2
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
  if [ "$FORM" = annotation ]; then
    jq -e '.params.arguments.endpoint=="/api/annotations/108"' "$request" >/dev/null
  elif [ "$FORM" = silence ]; then
    jq -e '.params.arguments.silence_id=="actual-created-silence"' "$request" >/dev/null
  fi
fi
jq -cn --slurpfile request "$request" --arg mode "$mode" '
  $request[0].params.name as $tool
  | $request[0].params.arguments as $arguments
  | (if $tool=="create_annotation" then {Payload:{id:108,message:"Annotation added"}}
     elif $tool=="alerting_silences_write" then {silenceID:"actual-created-silence"}
     elif $tool=="alerting_silences_read" then {
       id:$arguments.silence_id,startsAt:"2026-11-13T02:00:00Z",endsAt:"2026-11-13T04:00:00Z",
       matchers:[{name:"instance",value:"pg-replica-2",isRegex:false}],status:{state:"pending"}}
     elif $tool=="update_alert_group" then {
       alertGroupId:$arguments.alertGroupId,action:"acknowledge",updated:true,
       stateWarning:"The acknowledgment succeeded but readback was unavailable"}
     elif $tool=="get_current_oncall_users" then {
       scheduleId:$arguments.scheduleId,scheduleName:"Checkout Primary",users:[{id:"sam",username:"sbrennan"}]}
     else {status:200,headers:{},data:{message:"Ordinary error-budget business data"}} end) as $success
  | (if $mode=="native_error" then {error:"Native request failed"}
     elif $mode=="wrong_payload" then
       if $tool=="create_annotation" then {Payload:{id:0,message:"Annotation added"}}
       elif $tool=="alerting_silences_write" then {silenceID:""}
       elif $tool=="update_alert_group" then $success|.updated=false
       elif $tool=="get_current_oncall_users" then $success|.scheduleId="another-schedule"
       else {status:400,data:{message:"Bad request"}} end
     else $success end) as $payload
  | {jsonrpc:"2.0",id:2,result:{content:[{type:"text",text:($payload|tojson)}]}}
  | if $mode=="rpc_error" then del(.result)|.error={code:-32603,message:"failure"}
    elif $mode=="tool_error" then .result.isError=true
    elif $mode=="malformed" then .result.content[0].text="not JSON"
    elif $mode=="structured" then .result.structuredContent={unbound:"different result"}
    elif $mode=="empty" then .result.content=[]
    else . end
'
CURL
chmod +x "$WORK/bin/curl"
for form in annotation silence generic acknowledge oncall; do
  directory="$WORK/$form"
  mkdir "$directory"
  cp "$ORIGINAL/"* "$directory/"
  for mode in native_error wrong_payload rpc_error tool_error malformed structured empty valid; do
    sed -e '/^bound_call [0-9]/,$d' -e '/^call [0-9]/,$d' \
      "$ORIGINAL/solve.sh" > "$directory/solve.sh"
    case "$form" in
      annotation) tool=create_annotation; arguments='{"text":"Deploy checkout v4.19.0"}'
        capture='[{"name":"actual_identity","pointer":"/content/0/text","json_pointer":"/Payload/id"}]'
        next_tool=grafana_api_request; next_arguments='{"endpoint":"/api/annotations/${actual_identity}"}' ;;
      silence) tool=alerting_silences_write; arguments='{"operation":"create"}'
        capture='[{"name":"actual_identity","pointer":"/content/0/text","json_pointer":"/silenceID"}]'
        next_tool=alerting_silences_read; next_arguments='{"operation":"get","silence_id":"${actual_identity}"}' ;;
      generic) tool=grafana_api_request; arguments='{"endpoint":"/api/annotations","method":"POST","body":"{}"}'
        capture='[]'; next_tool=grafana_api_request; next_arguments='{"endpoint":"/api/health"}' ;;
      acknowledge) tool=update_alert_group; arguments='{"alertGroupId":"production","state":"acknowledged"}'
        capture='[]'; next_tool=grafana_api_request; next_arguments='{"endpoint":"/api/health"}' ;;
      oncall) tool=get_current_oncall_users; arguments='{"scheduleId":"primary"}'
        capture='[]'; next_tool=grafana_api_request; next_arguments='{"endpoint":"/api/health"}' ;;
    esac
    jq -n --arg tool "$tool" --argjson arguments "$arguments" \
      '{jsonrpc:"2.0",id:2,method:"tools/call",params:{name:$tool,arguments:$arguments}}' \
      > "$directory/first.json"
    printf '%s\n' "$capture" > "$directory/first.capture.json"
    jq -n --arg tool "$next_tool" --argjson arguments "$next_arguments" \
      '{jsonrpc:"2.0",id:2,method:"tools/call",params:{name:$tool,arguments:$arguments}}' \
      > "$directory/second.json"
    printf '%s\n' '[]' > "$directory/second.capture.json"
    printf '%s\n' \
      'bound_call 7114 "$DIR/first.json" "$DIR/first.capture.json" 0' \
      'bound_call 7114 "$DIR/second.json" "$DIR/second.capture.json" 1' \
      'printf "%s\n" completed > "$COMPLETION"' \
      'printf "%s\n" answer > "$ANSWER"' >> "$directory/solve.sh"
    rm -f "$WORK/calls" "$WORK/completion" "$WORK/answer"
    if MODE="$mode" FORM="$form" CALLS="$WORK/calls" COMPLETION="$WORK/completion" \
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
    printf 'Grafana generated helper: %s/%s verified\n' "$form" "$mode"
  done
done
