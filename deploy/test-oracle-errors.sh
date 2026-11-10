#!/bin/sh
set -eu
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
mkdir "$WORK/bin"
cat > "$WORK/bin/curl" <<'EOF'
#!/bin/sh
set -eu
request=""
inline=""
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
printf 'called\n' >> "$CALLS"
case "$MODE" in
  tool_error) printf '%s' '{"jsonrpc":"2.0","id":2,"result":{"content":[],"isError":true}}' ;;
  rpc_error) printf '%s' '{"jsonrpc":"2.0","id":2,"error":{"code":-1}}' ;;
  scalar) printf '%s' '{"jsonrpc":"2.0","id":2,"result":"wrong type"}' ;;
  malformed_flag) printf '%s' '{"jsonrpc":"2.0","id":2,"result":{"isError":"false","content":[]}}' ;;
  missing_content) printf '%s' '{"jsonrpc":"2.0","id":2,"result":{}}' ;;
  scalar_content) printf '%s' '{"jsonrpc":"2.0","id":2,"result":{"content":"malformed"}}' ;;
  malformed_structured) printf '%s' '{"jsonrpc":"2.0","id":2,"result":{"content":[],"structuredContent":1}}' ;;
  truncated) printf 'data: {"jsonrpc":"2.0","id":2,"result":{"content":[]}}\n' ;;
  valid|valid_error_data)
    name="$(jq -r '.params.name' "$request")"
    text='ordinary successful tool data'
    if [ "$MODE" = valid_error_data ]; then text='Error: this is requested log data, not a tool failure'; fi
    if [ "$name" = stripe_api_write ]; then text='{"id":"re_checked_return"}'; fi
    jq -cn --arg text "$text" '{jsonrpc:"2.0",id:2,result:{isError:false,content:[{type:"text",text:$text}]}}'
    ;;
esac
EOF
chmod +x "$WORK/bin/curl"
for task in collab__slack__launch-checklist-item business__multi__zendesk-refund-via-stripe business__stripe__customer-mrr; do
  mkdir "$WORK/$task"
  cp "$ROOT/tasks/$task/solution/"* "$WORK/$task/"
  sed "s|/app/answer.md|$WORK/answer.md|g" "$ROOT/tasks/$task/solution/solve.sh" > "$WORK/$task/solve.sh"
  for mode in tool_error rpc_error scalar malformed_flag missing_content scalar_content malformed_structured truncated valid valid_error_data; do
    rm -f "$WORK/calls" "$WORK/answer.md"
    if PATH="$WORK/bin:$PATH" MODE="$mode" CALLS="$WORK/calls" \
        sh "$WORK/$task/solve.sh" > "$WORK/$task-$mode.log" 2>&1; then
      status=0
    else
      status=$?
    fi
    case "$mode" in
      valid|valid_error_data)
        test "$status" = 0
        if [ "$task" = business__stripe__customer-mrr ]; then test -s "$WORK/answer.md"; fi
        ;;
      *)
        if [ "$status" = 0 ] || [ "$(wc -l < "$WORK/calls" | tr -d ' ')" != 1 ] || [ -e "$WORK/answer.md" ]; then
          printf '%s\n' "$task / $mode: failed MCP result was ignored or later calls executed" >&2
          exit 1
        fi
        ;;
    esac
  done
done
printf '%s\n' 'capturing, noncapturing and answer-writing oracles reject RPC/tool/shape/framing failures before later calls or answers, without grepping error words in valid data'
