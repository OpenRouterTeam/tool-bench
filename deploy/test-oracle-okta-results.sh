#!/bin/sh
set -eu
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
(cd "$ROOT" && cargo run -q --locked -p scenario --bin taskgen -- --runner-admission required \
  --out "$WORK/tasks" --world-image fixture:world --tools-image fixture:tools) >/dev/null
ORIGINAL="$WORK/tasks/cloud__okta__contractor-self-service-reset/solution"
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
    jq -e '.params.arguments.rule_data
      | .name=="Contractors ${literal_name}" and .conditions.network.connection=="ANYWHERE"
        and .actions.selfServicePasswordReset.access=="DENY"
        and .actions.selfServiceUnlock.access=="DENY"
        and .actions.selfServicePasswordReset.requirement.accessControl=="LEGACY"' "$request" >/dev/null
  fi
fi
jq -cn --slurpfile request "$request" --arg mode "$mode" '
  $request[0].params.name as $tool
  | {id:"selected-user",name:"Contractors ${literal_name}",type:"PASSWORD",
      conditions:{network:{connection:"ANYWHERE"}},
      actions:{passwordChange:{access:"ALLOW"},selfServicePasswordReset:{
        access:"ALLOW",requirement:{accessControl:"LEGACY"}},selfServiceUnlock:{access:"ALLOW"}}} as $rule
  | (if $tool=="deactivate_user" then {message:"User selected-user deactivated successfully."}
      else $rule end) as $success
  | (if $mode=="native_error" then {error:"native client failed"}
      elif $mode=="exception" then {exception:"SDK rejected the operation"}
      elif $mode=="serialization" then {ok:false,error:{type:"empty_body",message:"no body",tool:$tool},status_code:null,raw:{}}
      elif $mode=="cancel" then {message:"User deactivation cancelled by user."}
      elif $mode=="wrong_user" then {message:"User someone-else deactivated successfully."}
      else $success end) as $payload
  | {jsonrpc:"2.0",id:2,result:{content:[{type:"text",text:(
      if $mode=="malformed" then "not JSON" else ($payload|tojson) end)}]}}
  | if $mode=="mismatch" then .result.structuredContent={result:{id:"not-the-returned-record"}} else . end
'
CURL
chmod +x "$WORK/bin/curl"
for form in direct captured; do
  directory="$WORK/$form"
  mkdir "$directory"
  cp "$ORIGINAL/"* "$directory/"
  for mode in native_error exception serialization malformed mismatch cancel wrong_user valid; do
    case "$form:$mode" in captured:cancel|captured:wrong_user) continue ;; esac
    sed -e '/^bound_call [0-9]/,$d' -e '/^call [0-9]/,$d' \
      "$ORIGINAL/solve.sh" > "$directory/solve.sh"
    if [ "$form" = captured ]; then
      printf '%s\n' \
        'bound_call 7120 "$DIR/call-2.json" "$DIR/call-2.capture.json" 0' \
        'bound_call 7120 "$DIR/call-3.json" "$DIR/call-3.capture.json" 1' \
        >> "$directory/solve.sh"
    else
      jq -n '{jsonrpc:"2.0",id:2,method:"tools/call",params:{
        name:"deactivate_user",arguments:{user_id:"selected-user"}}}' > "$directory/first.json"
      jq -n '{jsonrpc:"2.0",id:2,method:"tools/call",params:{
        name:"get_user",arguments:{user_id:"selected-user"}}}' > "$directory/second.json"
      printf '%s\n' 'call 7120 "$DIR/first.json"' 'call 7120 "$DIR/second.json"' \
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
    printf '%s\n' "$form/$mode: generated helper enforces native result, binding and completion boundaries"
  done
done
