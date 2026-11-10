#!/bin/sh
set -eu
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
WORK="$(mktemp -d)"
cleanup() {
  result=$?
  if [ "$result" = 0 ]; then
    rm -rf "$WORK"
  else
    printf 'failed helper fixtures retained: %s\n' "$WORK" >&2
    find "$WORK" -name '*.log' -exec tail -12 {} \; >&2
  fi
}
trap cleanup EXIT
(cd "$ROOT" && cargo run -q --locked -p scenario --bin taskgen -- \
  --out "$WORK/tasks" --world-image fixture:world --tools-image fixture:tools) >/dev/null
ORIGINAL="$WORK/tasks/code__terraform__scale-checkout-staging/solution"
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
  if [ "$FORM" = captured ]; then
    jq -e '.params.arguments.run_id=="run-captured-identity"' "$request" >/dev/null
  fi
fi
jq -cn --slurpfile request "$request" --arg mode "$mode" '
  $request[0].params.name as $tool
  | $request[0].params.arguments as $arguments
  | (if $tool=="action_run" then
       "Run approved and applied successfully, run the `get_run_details` tool to get more information about the run."
     elif $tool=="update_workspace_variable" then "Updated variable desired_count with ID var-selected"
     elif $tool=="force_unlock_workspace" then "Workspace \"ws-selected\" is now unlocked"
     else {
       data:{id:(if $tool=="create_run" then "run-captured-identity" else $arguments.run_id end),
         type:"runs",attributes:{status:"planned",message:$arguments.message}}
     }|tojson end) as $success
  | (if $mode=="native_error" then ({errors:[{detail:"SDK rejected the request"}]}|tojson)
     elif $mode=="malformed" then "not a matching native result"
     elif $mode=="wrong_identity" then
       if $tool=="create_run" then ({data:{id:"",type:"runs",attributes:{status:"planned"}}}|tojson)
       elif $tool=="get_run_details" then ({data:{id:"run-someone-else",type:"runs",attributes:{status:"planned"}}}|tojson)
       else "Updated someone else" end
     else $success end) as $payload
  | {jsonrpc:"2.0",id:2,result:{content:[{type:"text",text:$payload}]}}
  | if $mode=="rpc_error" then del(.result)|.error={code:-32603,message:"failure"}
    elif $mode=="tool_error" then .result.isError=true
    elif $mode=="structured" then .result.structuredContent={data:{id:"unbound"}}
    elif $mode=="empty" then .result.content=[]
    else . end
'
CURL
chmod +x "$WORK/bin/curl"
for form in action variable unlock run captured; do
  directory="$WORK/$form"
  mkdir "$directory"
  cp "$ORIGINAL/"* "$directory/"
  for mode in native_error malformed wrong_identity rpc_error tool_error structured empty valid; do
    sed -e '/^bound_call [0-9]/,$d' -e '/^call [0-9]/,$d' \
      "$ORIGINAL/solve.sh" > "$directory/solve.sh"
    case "$form" in
      action) tool=action_run; arguments='{"run_id":"run-selected","run_action":"apply"}' ;;
      variable) tool=update_workspace_variable
        arguments='{"terraform_org_name":"selected","workspace_name":"selected","variable_id":"var-selected","key":"desired_count","value":"4"}' ;;
      unlock) tool=force_unlock_workspace; arguments='{"workspace_id":"ws-selected"}' ;;
      run) tool=get_run_details; arguments='{"run_id":"run-selected"}' ;;
      captured) tool=create_run
        arguments='{"terraform_org_name":"selected","workspace_name":"selected","run_type":"plan_and_apply","message":"Review requested change"}' ;;
    esac
    jq -n --arg tool "$tool" --argjson arguments "$arguments" \
      '{jsonrpc:"2.0",id:2,method:"tools/call",params:{name:$tool,arguments:$arguments}}' \
      > "$directory/first.json"
    if [ "$form" = captured ]; then
      printf '%s\n' '[{"name":"run_identity","pointer":"/content/0/text","json_pointer":"/data/id"}]' \
        > "$directory/first.capture.json"
      jq -n '{jsonrpc:"2.0",id:2,method:"tools/call",params:{
        name:"get_run_details",arguments:{run_id:"${run_identity}"}}}' > "$directory/second.json"
      printf '%s\n' '[]' > "$directory/second.capture.json"
      printf '%s\n' \
        'bound_call 7109 "$DIR/first.json" "$DIR/first.capture.json" 0' \
        'bound_call 7109 "$DIR/second.json" "$DIR/second.capture.json" 1' >> "$directory/solve.sh"
    else
      jq -n '{jsonrpc:"2.0",id:2,method:"tools/call",params:{
        name:"get_run_details",arguments:{run_id:"run-selected"}}}' > "$directory/second.json"
      printf '%s\n' 'call 7109 "$DIR/first.json"' 'call 7109 "$DIR/second.json"' \
        >> "$directory/solve.sh"
    fi
    printf '%s\n' 'printf "%s\n" completed > "$COMPLETION"' \
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
    printf 'Terraform generated helper: %s/%s verified\n' "$form" "$mode"
  done
done
