#!/bin/sh
set -eu
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
CASES="$ROOT/crates/scenario/tests/fixtures/oracle-bindings.json"
ASSETS="$ROOT/crates/scenario/assets"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
command -v jq >/dev/null
jq -e '(.render | length > 0) and (.capture | length > 0)' "$CASES" >/dev/null
for index in $(jq -r '.render | keys[]' "$CASES"); do
  jq ".render[$index].bindings" "$CASES" > "$WORK/bindings.json"
  jq ".render[$index] | {params: {arguments: .arguments}}" "$CASES" > "$WORK/input.json"
  jq --slurpfile bindings "$WORK/bindings.json" -f "$ASSETS/oracle-render.jq" \
    "$WORK/input.json" > "$WORK/actual.json"
  jq -e --slurpfile actual "$WORK/actual.json" \
    ".render[$index].expected == \$actual[0].params.arguments" "$CASES" >/dev/null
done
for index in $(jq -r '.replacement_render | keys[]' "$CASES"); do
  jq ".replacement_render[$index].bindings" "$CASES" > "$WORK/bindings.json"
  jq ".replacement_render[$index] | {params:{arguments:.arguments},argument_replacements:.replacements}" \
    "$CASES" > "$WORK/input.json"
  jq --slurpfile bindings "$WORK/bindings.json" -f "$ASSETS/oracle-render.jq" \
    "$WORK/input.json" > "$WORK/actual.json"
  jq -e --slurpfile actual "$WORK/actual.json" \
    ".replacement_render[$index].expected == \$actual[0].params.arguments
      and (\$actual[0]|has(\"argument_replacements\")|not)" "$CASES" >/dev/null
done
for index in $(jq -r '.invalid_replacements | keys[]' "$CASES"); do
  jq ".invalid_replacements[$index].bindings" "$CASES" > "$WORK/bindings.json"
  jq ".invalid_replacements[$index] | {params:{arguments:.arguments},argument_replacements:.replacements}" \
    "$CASES" > "$WORK/input.json"
  if jq --slurpfile bindings "$WORK/bindings.json" -f "$ASSETS/oracle-render.jq" \
      "$WORK/input.json" > "$WORK/invalid.json" 2> "$WORK/invalid.log"; then
    printf '%s\n' 'invalid argument replacements were silently accepted' >&2
    exit 1
  fi
done
for index in $(jq -r '.answers | keys[]' "$CASES"); do
  jq ".answers[$index].bindings" "$CASES" > "$WORK/bindings.json"
  jq ".answers[$index] | {params: {arguments: .template}}" "$CASES" > "$WORK/input.json"
  jq --slurpfile bindings "$WORK/bindings.json" -f "$ASSETS/oracle-render.jq" \
    "$WORK/input.json" | jq -j '.params.arguments | if type=="string" then . else tojson end' \
    > "$WORK/answer.md"
  jq -e --rawfile answer "$WORK/answer.md" \
    ".answers[$index].expected == \$answer" "$CASES" >/dev/null
done
for index in $(jq -r '.invalid_render | keys[]' "$CASES"); do
  jq ".invalid_render[$index].bindings" "$CASES" > "$WORK/bindings.json"
  jq ".invalid_render[$index] | {params: {arguments: .arguments}}" "$CASES" > "$WORK/input.json"
  if jq --slurpfile bindings "$WORK/bindings.json" -f "$ASSETS/oracle-render.jq" \
      "$WORK/input.json" > "$WORK/invalid.json" 2> "$WORK/invalid.log"; then
    printf '%s\n' 'invalid oracle template was silently accepted' >&2
    exit 1
  fi
done
for index in $(jq -r '.capture | keys[]' "$CASES"); do
  jq ".capture[$index].bindings" "$CASES" > "$WORK/bindings.json"
  jq ".capture[$index].captures" "$CASES" > "$WORK/captures.json"
  jq ".capture[$index].result" "$CASES" > "$WORK/result.json"
  jq -n --slurpfile bindings "$WORK/bindings.json" --slurpfile captures "$WORK/captures.json" \
    --slurpfile result "$WORK/result.json" -f "$ASSETS/oracle-capture.jq" > "$WORK/actual.json"
  jq -e --slurpfile actual "$WORK/actual.json" \
    ".capture[$index].expected == \$actual[0]" "$CASES" >/dev/null
done
mkdir -p "$WORK/bin"
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
  method=$(printf '%s' "$inline" | jq -r '.method')
  if [ "$method" = initialize ]; then
    printf 'HTTP/1.1 200 OK\r\nmcp-session-id: fixture-session\r\n\r\n'
  fi
  exit 0
fi
name=$(jq -r '.params.name' "$request")
if [ "$name" = stripe_api_write ]; then
  jq -n '{jsonrpc: "2.0", id: 2, result: {content: [{type: "text", text: "{\"id\":\"re_transport_returned\"}"}]}}'
elif [ "$name" = create_ticket_comment ]; then
  jq -e '.params.arguments.comment | contains("re_transport_returned")' "$request" >/dev/null
  printf 'posted\n' > "$ORACLE_BINDINGS_RECEIPT"
  jq -n '{jsonrpc: "2.0", id: 2, result: {content: [{type: "text", text: "created"}]}}'
else
  jq -n '{jsonrpc: "2.0", id: 2, result: {content: [{type: "text", text: "{}"}]}}'
fi
CURL
chmod +x "$WORK/bin/curl"
ORACLE_REQUEST_CAPTURE_DIR="$WORK/observed-requests" \
  ORACLE_BINDINGS_RECEIPT="$WORK/posted" PATH="$WORK/bin:$PATH" \
  "$ROOT/tasks/business__multi__zendesk-refund-via-stripe/solution/solve.sh" \
  > "$WORK/transport.log"
test -s "$WORK/posted"
jq -s -e '
  any(.[]; .params.name=="create_ticket_comment"
    and (.params.arguments.comment|contains("re_transport_returned")))
  and all(.[]; (tostring|contains("${")|not))
' "$WORK/observed-requests"/*/request.json >/dev/null
for observation in "$WORK/observed-requests"/*; do
  test -s "$observation/endpoint.txt"
done
printf 'oracle binding parity: all shared render and capture fixtures passed\n'
printf 'generated oracle: returned ID reached the subsequent MCP request\n'
printf 'scripted request capture: rendered input bytes and endpoint retained before dispatch\n'
