#!/bin/sh
# Oracle: replays the scenario's tool calls through the real MCP endpoints in the tools container.
set -eu
DIR="$(cd "$(dirname "$0")" && pwd)"
INIT='{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"tool-bench-oracle","version":"1"}}}'
call() (
  url="http://tools:$1/mcp"
  sid=$(curl -fsS -D - -o /dev/null -H 'content-type: application/json' -H 'accept: application/json, text/event-stream' -d "$INIT" "$url" | tr -d '\r' | awk -F': ' 'tolower($1)=="mcp-session-id" {print $2}')
  curl -fsS -o /dev/null -H 'content-type: application/json' -H "mcp-session-id: $sid" -d '{"jsonrpc":"2.0","method":"notifications/initialized"}' "$url"
  response="$(mktemp)"
  decoded="$(mktemp)"
  trap 'rm -f "$response" "$decoded"' EXIT
  if [ -n "${ORACLE_REQUEST_CAPTURE_DIR:-}" ]; then
    mkdir -p "$ORACLE_REQUEST_CAPTURE_DIR"
    request_capture="$(mktemp -d "$ORACLE_REQUEST_CAPTURE_DIR/call.XXXXXX")"
    cp "$2" "$request_capture/request.json"
    printf '%s\n' "$1" > "$request_capture/endpoint.txt"
  fi
  curl -fsS -H 'content-type: application/json' -H 'accept: application/json, text/event-stream' -H "mcp-session-id: $sid" --data-binary @"$2" "$url" > "$response"
  if ! jq -Rse -f "$DIR/response.jq" "$response" > "$decoded"; then
    cat "$response" >&2
    exit 1
  fi
  if ! jq -e -L "$DIR" --arg endpoint "$1" --slurpfile request "$2" \
      --slurpfile contracts "$DIR/result-contracts.json" -f "$DIR/tool-result.jq" "$decoded" > /dev/null; then
    cat "$response" >&2
    exit 1
  fi
  cat "$response"
)
call 7115 "$DIR/call-0.json"
call 7115 "$DIR/call-1.json"
call 7111 "$DIR/call-2.json"
call 7111 "$DIR/call-3.json"
cp "$DIR/answer.md" /app/answer.md
