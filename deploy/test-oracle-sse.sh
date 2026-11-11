#!/bin/sh
set -eu
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
mkdir -p "$WORK/bin"
cat > "$WORK/bin/curl" <<'EOF'
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
    printf 'HTTP/1.1 200 OK\r\nmcp-session-id: fixture-session\r\n\r\n'
  fi
  exit 0
fi
name="$(jq -r '.params.name' "$request")"
if [ "$name" = stripe_api_write ]; then
  message='{"jsonrpc":"2.0","id":2,"result":{"content":[{"type":"text","text":"{\"id\":\"re_sse_returned\"}"}]}}'
  case "$MODE" in
    json) printf '%s' "$message" ;;
    sse) printf 'event: message\ndata: %s\n\n' "$message" ;;
    truncated) printf 'event: message\ndata: %s\n' "$message" ;;
  esac
elif [ "$name" = create_ticket_comment ]; then
  jq -e '.params.arguments.comment | contains("re_sse_returned")' "$request" >/dev/null
  printf '%s\n' posted > "$RECEIPT"
  printf '%s\n' '{"jsonrpc":"2.0","id":2,"result":{"content":[]}}'
else
  printf '%s\n' '{"jsonrpc":"2.0","id":2,"result":{"content":[]}}'
fi
EOF
chmod +x "$WORK/bin/curl"
for mode in json sse truncated; do
  rm -f "$WORK/posted"
  if PATH="$WORK/bin:$PATH" MODE="$mode" RECEIPT="$WORK/posted" \
      sh "$ROOT/tasks/business__multi__zendesk-refund-via-stripe/solution/solve.sh" > "$WORK/$mode.log" 2>&1; then
    status=0
  else
    status=$?
  fi
  if [ "$mode" = truncated ]; then
    test "$status" != 0
    test ! -e "$WORK/posted"
  else
    test "$status" = 0
    test -s "$WORK/posted"
  fi
done
printf '%s\n' 'generated oracle binds actual JSON/SSE IDs and never completes a truncated event by adding a newline'
