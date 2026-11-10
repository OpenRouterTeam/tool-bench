#!/bin/sh
set -eu
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
WORK="$(mktemp -d /tmp/tool-bench-google-helper.XXXXXXXX)"
cleanup() {
  result=$?
  if [ "$result" = 0 ]; then
    rm -rf "$WORK"
  else
    printf 'failed Google helper fixtures retained: %s\n' "$WORK" >&2
    find "$WORK" -name '*.log' -exec tail -8 {} \; >&2
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
if [ "$(wc -l < "$CALLS" | tr -d ' ')" -gt 1 ]; then
  jq -e --rawfile expected "$EXPECTED" '.params.arguments.query == ("receipt " + $expected)' "$request" >/dev/null
  printf '%s\n' '{"jsonrpc":"2.0","id":2,"result":{"content":[{"type":"text","text":"received"}]}}'
  exit 0
fi
jq -cn --slurpfile request "$request" --arg mode "$MODE" '
  $request[0].params.name as $tool
  | $request[0].params.arguments as $arguments
  | (if $tool == "modify_sheet_values" then
      "Successfully updated range \u0027Renewals!C2:D2\u0027 in spreadsheet actual-book for ops@octo-corp.example. Updated: 2 cells, 1 rows, 2 columns."
    elif $tool == "manage_event" then
      ("actual-returned-event ops@octo-corp.example" | @base64 | rtrimstr("=") | rtrimstr("=")) as $encoded
      | "Successfully created event \u0027Error budget planning\u0027 for ops@octo-corp.example. Link: https://www.google.com/calendar/event?eid="
        + $encoded + "\nSaved start: 2026-11-03T14:00:00-05:00 [America/New_York; weekday: Tuesday; ISO weekday: 2]"
        + "\nSaved end: 2026-11-03T14:45:00-05:00 [America/New_York; weekday: Tuesday; ISO weekday: 2]"
        + "\nElapsed duration: 45 minutes (2700 seconds) Google Meet: https://meet.google.com/abc-defg-hij"
    elif $tool == "manage_drive_access" and $arguments.action == "grant_batch" then
      "Batch share results for \u0027Error budget.pdf\u0027\n\nSummary: 2 succeeded, 0 failed\n\nResults:"
        + "\n  - User: dana.whitfield@contoso.example (reader) [id: actual-permission]"
        + "\n  - User: other@example.test (commenter) [id: other-permission]"
        + "\n\nView link: https://drive.google.com/file/d/actual-report/view"
    elif $tool == "manage_drive_access" then
      "Successfully shared \u0027Error budget.pdf\u0027\n\nPermission created:"
        + "\n  - User: dana.whitfield@contoso.example (reader) [id: actual-permission]"
        + "\n\nView link: https://drive.google.com/file/d/actual-report/view"
    elif $tool == "get_drive_file_permissions" then
      "File: Error budget.pdf\nID: actual-report\nType: application/pdf\nSharing Status:\n  Shared: False"
        + "\n  Number of permissions: 1\n  Permissions:\n    - User: ops@octo-corp.example (owner) [id: actual-owner]"
        + "\n\nURLs:\n  View Link: https://drive.google.com/file/d/actual-report/view"
        + "\n\n❌ This file is NOT shared with \u0027Anyone with the link\u0027"
    else
      "File: Error budget.pdf\nID: actual-report\nType: application/pdf\nShared: False\n\nLinks:"
        + "\n  View: https://drive.google.com/file/d/actual-report/view"
    end) as $success
  | (if $mode == "wrong_binding" then
      if $tool == "manage_event" then $success | sub("Error budget planning";"wrong title")
      elif $tool == "modify_sheet_values" then $success | sub("actual-book";"wrong-book")
      else $success | gsub("actual-report";"wrong-report") end
    elif $mode == "reported_failure" then
      if $arguments.action == "grant_batch" then $success | sub("2 succeeded, 0 failed";"1 succeeded, 1 failed")
      else "Error performing operation: failed" end
    elif $mode == "malformed" then "{}"
    else $success end) as $text
  | {jsonrpc:"2.0",id:2,result:{isError:false,
      content:[{type:"text",text:$text}],structuredContent:{result:$text}}}
  | if $mode == "rpc_error" then del(.result) | .error={code:-32603,message:"failure"}
    elif $mode == "tool_error" then .result.isError=true
    elif $mode == "structured" then .result.structuredContent.result="conflicting result"
    elif $mode == "empty" then .result.content=[]
    elif $mode == "valid_without_flag" then del(.result.isError)
    else . end
' > "$PAYLOAD"
jq -j '.result.structuredContent.result // ""' "$PAYLOAD" > "$EXPECTED"
cat "$PAYLOAD"
CURL
chmod +x "$WORK/bin/curl"
for form in sheet event grant batch permissions link; do
  directory="$WORK/$form"
  mkdir "$directory"
  cp "$ORIGINAL/"* "$directory/"
  case "$form" in
    sheet) tool=modify_sheet_values
      arguments='{"spreadsheet_id":"actual-book","range_name":"Renewals!C2:D2","values":[["2027-11-30","Renewed"]]}' ;;
    event) tool=manage_event
      arguments='{"action":"create","summary":"Error budget planning","start_time":"2026-11-03T14:00:00","end_time":"2026-11-03T14:45:00","timezone":"America/New_York","add_google_meet":true}' ;;
    grant) tool=manage_drive_access
      arguments='{"action":"grant","file_id":"actual-report","share_with":"dana.whitfield@contoso.example","role":"reader","share_type":"user"}' ;;
    batch) tool=manage_drive_access
      arguments='{"action":"grant_batch","file_id":"actual-report","recipients":[{"email":"dana.whitfield@contoso.example","role":"reader"},{"email":"other@example.test","role":"commenter"}]}' ;;
    permissions) tool=get_drive_file_permissions
      arguments='{"file_id":"actual-report"}' ;;
    link) tool=get_drive_shareable_link
      arguments='{"file_id":"actual-report"}' ;;
  esac
  jq -n --arg tool "$tool" --argjson arguments "$arguments" \
    '{jsonrpc:"2.0",id:2,method:"tools/call",params:{name:$tool,arguments:$arguments}}' \
    > "$directory/first.json"
  printf '%s\n' '[{"name":"completion_text","pointer":"/structuredContent/result"}]' > "$directory/first.capture.json"
  jq -n '{jsonrpc:"2.0",id:2,method:"tools/call",params:{name:"search_gmail_messages",
    arguments:{query:"receipt ${completion_text}"}}}' > "$directory/second.json"
  printf '%s\n' '[]' > "$directory/second.capture.json"
  for mode in reported_failure wrong_binding rpc_error tool_error malformed structured empty valid valid_without_flag; do
    sed -e '/^bound_call [0-9]/,$d' -e '/^call [0-9]/,$d' \
      "$ORIGINAL/solve.sh" > "$directory/solve.sh"
    printf '%s\n' \
      'bound_call 7130 "$DIR/first.json" "$DIR/first.capture.json" 0' \
      'bound_call 7130 "$DIR/second.json" "$DIR/second.capture.json" 1' \
      'printf "%s\n" completed > "$COMPLETION"' \
      'printf "%s\n" answer > "$ANSWER"' >> "$directory/solve.sh"
    rm -f "$WORK/calls" "$WORK/completion" "$WORK/answer" "$WORK/expected" "$WORK/payload"
    if MODE="$mode" CALLS="$WORK/calls" COMPLETION="$WORK/completion" ANSWER="$WORK/answer" \
        EXPECTED="$WORK/expected" PAYLOAD="$WORK/payload" PATH="$WORK/bin:$PATH" \
        sh "$directory/solve.sh" > "$WORK/$form-$mode.log" 2>&1; then
      program_status=0
    else
      program_status=$?
    fi
    case "$mode" in
      valid|valid_without_flag)
        test "$program_status" = 0
        test "$(wc -l < "$WORK/calls" | tr -d ' ')" = 2
        test -s "$WORK/completion" && test -s "$WORK/answer" ;;
      *)
        test "$program_status" != 0
        test "$(wc -l < "$WORK/calls" | tr -d ' ')" = 1
        test ! -e "$WORK/completion" && test ! -e "$WORK/answer" ;;
    esac
    printf 'Google generated helper: %s/%s verified\n' "$form" "$mode"
  done
done
