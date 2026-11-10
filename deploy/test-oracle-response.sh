#!/bin/sh
set -eu
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
FILTER="$ROOT/crates/scenario/assets/oracle-response.jq"
for variant in json sse multiline; do
  case "$variant" in
    json) printf '%s\n' '{"jsonrpc":"2.0","id":2,"result":{"content":[],"value":"actual"}}' ;;
    sse) printf 'event: message\ndata: {"jsonrpc":"2.0","id":2,"result":{"content":[],"value":"actual"}}\n\n' ;;
    multiline)
      printf ': heartbeat\n\nevent: message\ndata: {"jsonrpc":"2.0","method":"notifications/progress","params":{}}\n\nevent: message\ndata: {"jsonrpc":"2.0","id":2,\ndata: "result":{"content":[],"value":"actual"}}\n\n'
      ;;
  esac > "$WORK/input"
  jq -Rse -f "$FILTER" "$WORK/input" > "$WORK/result"
  jq -e '.value == "actual"' "$WORK/result" >/dev/null
done
for method in initialize tools/list; do
  case "$method" in
    initialize)
      printf '%s\n' '{"jsonrpc":"2.0","id":1,"result":{"serverInfo":{"name":"fixture","version":"1"},"capabilities":{},"protocolVersion":"2025-06-18"}}' ;;
    tools/list)
      printf 'data: {"jsonrpc":"2.0","id":1,"result":{"tools":[]}}\n\n' ;;
  esac > "$WORK/input"
  jq -Rse --argjson expected_id 1 --arg method "$method" -f "$FILTER" \
    "$WORK/input" > "$WORK/result"
  test -s "$WORK/result"
  if jq -Rse -f "$FILTER" "$WORK/input" > "$WORK/invalid" 2> "$WORK/error"; then
    printf '%s\n' 'metadata response was accepted as a tool call result' >&2
    exit 1
  fi
done
for variant in duplicate truncated wrong_id error missing_result malformed tool_error scalar_result missing_content scalar_content malformed_structured; do
  case "$variant" in
    duplicate) printf 'data: {"jsonrpc":"2.0","id":2,"result":{}}\n\ndata: {"jsonrpc":"2.0","id":2,"result":{}}\n\n' ;;
    truncated) printf 'data: {"jsonrpc":"2.0","id":2,"result":{}}\n' ;;
    wrong_id) printf '%s\n' '{"jsonrpc":"2.0","id":9,"result":{}}' ;;
    error) printf '%s\n' '{"jsonrpc":"2.0","id":2,"error":{"code":-1}}' ;;
    missing_result) printf '%s\n' '{"jsonrpc":"2.0","id":2}' ;;
    malformed) printf 'data: not-json\n\n' ;;
    tool_error) printf '%s\n' '{"jsonrpc":"2.0","id":2,"result":{"content":[{"type":"text","text":"denied"}],"isError":true}}' ;;
    scalar_result) printf '%s\n' '{"jsonrpc":"2.0","id":2,"result":"not a CallToolResult"}' ;;
    missing_content) printf '%s\n' '{"jsonrpc":"2.0","id":2,"result":{}}' ;;
    scalar_content) printf '%s\n' '{"jsonrpc":"2.0","id":2,"result":{"content":"not a content array"}}' ;;
    malformed_structured) printf '%s\n' '{"jsonrpc":"2.0","id":2,"result":{"content":[],"structuredContent":[]}}' ;;
  esac > "$WORK/input"
  if jq -Rse -f "$FILTER" "$WORK/input" > "$WORK/result" 2> "$WORK/error"; then
    printf '%s\n' "$variant: an invalid MCP response supplied a capture" >&2
    exit 1
  fi
  test ! -s "$WORK/result"
done
printf '%s\n' 'oracle response decoder accepts JSON/SSE and rejects malformed, duplicate and partial responses'
