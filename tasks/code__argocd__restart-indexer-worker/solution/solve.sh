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

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
printf '%s\n' '{}' > "$WORK/bindings.json"
bound_call() {
  if [ "$4" = 1 ]; then
    jq --slurpfile bindings "$WORK/bindings.json" -f "$DIR/render.jq" "$2" > "$WORK/call.json"
  else
    cp "$2" "$WORK/call.json"
  fi
  if [ -n "${5:-}" ]; then
    if [ ! -f "$5" ]; then
      printf '%s\n' "oracle: the locked input schema for captured arguments was not generated; refusing to send the call" >&2
      exit 1
    fi
    jq -e -L "$DIR" --slurpfile schema "$5" -f "$DIR/argument-schema.jq" "$WORK/call.json" > /dev/null
  fi
  call "$1" "$WORK/call.json" > "$WORK/response.json"
  cat "$WORK/response.json"
  printf '\n'
  count=$(jq -er 'if type == "array" then length else error("capture definitions must be an array") end' "$3")
  if [ "$count" -gt 0 ]; then
    jq -Rse -f "$DIR/response.jq" "$WORK/response.json" > "$WORK/result.json"
    jq -n --slurpfile bindings "$WORK/bindings.json" --slurpfile captures "$3" \
      --slurpfile result "$WORK/result.json" -f "$DIR/capture.jq" > "$WORK/new-bindings.json"
    jq -e -L "$DIR" --slurpfile captures "$3" -f "$DIR/capture-constraints.jq" \
      "$WORK/new-bindings.json" > /dev/null
    mv "$WORK/new-bindings.json" "$WORK/bindings.json"
  fi
}
bound_call 7108 "$DIR/call-0.json" "$DIR/call-0.capture.json" 0 ""
bound_call 7108 "$DIR/call-1.json" "$DIR/call-1.capture.json" 1 "$DIR/call-1.arguments-schema.json"
bound_call 7108 "$DIR/call-2.json" "$DIR/call-2.capture.json" 1 "$DIR/call-2.arguments-schema.json"
bound_call 7108 "$DIR/call-3.json" "$DIR/call-3.capture.json" 0 ""
