#!/bin/sh
set -eu
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
(cd "$ROOT" && CARGO_INCREMENTAL=0 cargo run -q --locked -p scenario --bin taskgen -- \
  --include-examples --out "$WORK/tasks" --world-image fixture:world --tools-image fixture:tools) >/dev/null
NEGATIVE="$WORK/tasks/example__aws__missing-bucket-policy/solution"
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
  if [ "$FORM" = capturing ]; then
    expected=i_actual_returned
    [ "$MODE" != expected_notfound ] || expected=NoSuchBucketPolicy
    jq -e --arg expected "$expected" '.params.arguments.cli_command | contains($expected)' "$request" >/dev/null
  fi
fi
jq -cn --slurpfile request "$request" --arg mode "$mode" '
  $request[0].params.arguments.cli_command
  | (if type=="array" then . else [.] end)
  | map({cli_command:.,response:{error:null,status_code:200,error_code:null,
      as_json:({id:"i_actual_returned",message:"Error: requested business log data"}|tojson)},
      validation_failures:null,missing_context_failures:null,failed_constraints:[]})
  | if $mode=="wrapped_error" then .[0]={cli_command:.[0].cli_command,error:"client validation rejected the command"}
    elif $mode=="http_error" then .[0].response|={status_code:400,error_code:"MissingParameter",error:"EvaluationPeriods is required"}
    elif $mode=="validation" then .[0].validation_failures=[{reason:"invalid command"}]
    elif $mode=="context" then .[0].missing_context_failures=[{reason:"missing arguments"}]
    elif $mode=="constraint" then .[0].failed_constraints=["denied"]
    elif $mode=="partial" then .[0:1]
    elif $mode=="wrong_echo" then .[0].cli_command="aws iam get-user --user-name wrong"
    elif $mode=="status_string" then .[0].response.status_code="200"
    elif $mode=="status_fractional" then .[0].response.status_code=200.5
    elif $mode=="expected_notfound" or $mode=="wrong_notfound" then
      .[0].response={status_code:404,error_code:(if $mode=="expected_notfound"
        then "NoSuchBucketPolicy" else "NoSuchBucket" end),
        error:"The requested configuration is absent",as_json:null}
    else . end
  | {jsonrpc:"2.0",id:2,result:{isError:false,
      content:[{type:"text",text:tojson}],structuredContent:{result:.}}}
'
CURL
chmod +x "$WORK/bin/curl"
for form in direct capturing answer; do
  case "$form" in
    capturing) original="$ROOT/tasks/business__multi__zendesk-refund-via-stripe/solution" ;;
    *) original="$ROOT/tasks/cloud__aws__find-unencrypted-bucket/solution" ;;
  esac
  directory="$WORK/$form"
  mkdir "$directory"
  cp "$original/"* "$directory/"
  for mode in wrapped_error http_error validation context constraint partial wrong_echo status_string \
      status_fractional wrong_notfound valid expected_notfound empty_batch; do
    sed '/^bound_call [0-9]/,$d; /^call [0-9]/,$d' "$original/solve.sh" > "$directory/solve.sh"
    if [ "$form" = capturing ]; then
      jq -n '{jsonrpc:"2.0",id:2,method:"tools/call",params:{name:"call_aws",
        arguments:{cli_command:"aws ec2 describe-instances --instance-ids ${actual}"}}}' \
        > "$directory/call-1.json"
      printf '%s\n' '[{"name":"actual","pointer":"/structuredContent/result/0/response/as_json","json_pointer":"/id"}]' \
        > "$directory/call-0.capture.json"
      printf '%s\n' '[]' > "$directory/call-1.capture.json"
      printf '%s\n' 'bound_call 7116 "$DIR/call-0.json" "$DIR/call-0.capture.json" 0' \
        'bound_call 7116 "$DIR/call-1.json" "$DIR/call-1.capture.json" 1' >> "$directory/solve.sh"
    else
      jq -n '{jsonrpc:"2.0",id:2,method:"tools/call",params:{name:"call_aws",
        arguments:{cli_command:"aws iam list-users"}}}' > "$directory/call-1.json"
      printf '%s\n' 'call 7116 "$DIR/call-0.json"' 'call 7116 "$DIR/call-1.json"' \
        >> "$directory/solve.sh"
    fi
    command='aws ec2 describe-instances'
    case "$mode" in
      expected_notfound|wrong_notfound)
        command='aws s3api get-bucket-policy --bucket policy-audit-bucket' ;;
    esac
    jq -n --arg command "$command" --arg mode "$mode" \
      '{jsonrpc:"2.0",id:2,method:"tools/call",params:{name:"call_aws",arguments:{
        cli_command:(if $mode=="partial" then [$command,"aws iam list-users"]
          elif $mode=="empty_batch" then [] else $command end)}}}' > "$directory/call-0.json"
    if [ "$mode" = expected_notfound ]; then
      cp "$NEGATIVE/result-contracts.json" \
        "$directory/result-contracts.json"
      if [ "$form" = capturing ]; then
        printf '%s\n' '[{"name":"actual","pointer":"/structuredContent/result/0/response/error_code"}]' \
          > "$directory/call-0.capture.json"
        jq '.params.arguments.cli_command="aws logs filter-log-events --log-group-name fixture --filter-pattern ${actual}"' \
          "$directory/call-1.json" > "$directory/next-call.json"
        mv "$directory/next-call.json" "$directory/call-1.json"
      fi
    fi
    if [ "$form" = answer ]; then
      printf "printf 'completed\\n' > '%s/answer.md'\\n" "$WORK" >> "$directory/solve.sh"
    fi
    rm -f "$WORK/calls" "$WORK/answer.md"
    if PATH="$WORK/bin:$PATH" MODE="$mode" FORM="$form" CALLS="$WORK/calls" \
        sh "$directory/solve.sh" > "$directory-$mode.log" 2>&1; then
      status=0
    else
      status=$?
    fi
    case "$mode:$form" in
      valid:*|expected_notfound:*|empty_batch:direct|empty_batch:answer)
        test "$status" = 0
        test "$(wc -l < "$WORK/calls" | tr -d ' ')" = 2
        if [ "$form" = answer ]; then test -s "$WORK/answer.md"; fi
        ;;
      empty_batch:capturing)
        test "$status" != 0
        test "$(wc -l < "$WORK/calls" | tr -d ' ')" = 1
        ;;
      *)
        if [ "$status" = 0 ] || [ "$(wc -l < "$WORK/calls" | tr -d ' ')" != 1 ] || [ -e "$WORK/answer.md" ]; then
          printf '%s\n' "$form / $mode: a wrapped command failure authorized continuation or an answer" >&2
          exit 1
        fi
        ;;
    esac
  done
done
printf '%s\n' 'all generated oracle forms check wrapped AWS results before captures, later calls or answers; exact declared negative evidence and benign error words remain valid'
