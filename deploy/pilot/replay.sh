#!/usr/bin/env bash
# replay.sh <slot>
#
# Operator only: replays the reference solution of the slot's task through mcp.py, one
# `mcp.py call` per reference call, to confirm the harness and the task agree. The server
# port, call order and captured values are read from solution/solve.sh, and values captured
# from earlier results are bound with the solution's own capture.jq and render.jq. A
# reference answer, if the task has one, is written to /tmp/tb-pilot/agents/<slot>/app/answer.md.
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
PILOT_HOME=/tmp/tb-pilot
PY="${PILOT_PYTHON:-$HOME/.local/bin/python3.12}"
[ -x "$PY" ] || PY=python3
die() { printf 'replay.sh: %s\n' "$*" >&2; exit 2; }

[ $# -eq 1 ] || die "usage: replay.sh <slot>"
slot="$1"
case "$slot" in ''|*[!0-9]*) die "slot must be a number from 0 to 19" ;; esac
slot=$((10#$slot))
slot_dir="$PILOT_HOME/slots/$slot"
[ -f "$slot_dir/slot.env" ] && [ -f "$slot_dir/servers.json" ] || die "slot $slot is not running; run up.sh first"
# shellcheck disable=SC1091
. "$slot_dir/slot.env"
solution="$TASK_DIR/solution"
[ -f "$solution/solve.sh" ] || die "$TASK_DIR has no solution/solve.sh"
export PILOT_SLOT="$slot"

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
printf '{}\n' > "$work/bindings.json"
# solve.sh lines: `call PORT "$DIR/call-N.json"`, or
# `bound_call PORT "$DIR/call-N.json" "$DIR/call-N.capture.json" RENDER "SCHEMA"`.
sed -nE 's/^(bound_call|call) ([0-9]+) "\$DIR\/(call-[0-9]+\.json)"( "\$DIR\/(call-[0-9]+\.capture\.json)" ([01]))?.*/\2 \3 \5 \6/p' \
  "$solution/solve.sh" > "$work/steps"
[ "$(wc -l < "$work/steps")" -eq "$(grep -cE '^(bound_call|call) [0-9]' "$solution/solve.sh")" ] \
  || die "solution/solve.sh has a call line this script does not understand"

while read -r port call capture render; do
  host_port=$((20000 + slot * 100 + port - 7100))
  server="$(jq -r --arg port ":$host_port/" '.servers[] | select(.url | contains($port)) | .name' "$slot_dir/servers.json")"
  [ -n "$server" ] || die "no MCP server is published for tools port $port"
  if [ "${render:-0}" = 1 ]; then
    jq --slurpfile bindings "$work/bindings.json" -f "$solution/render.jq" "$solution/$call" > "$work/call.json"
  else
    cp "$solution/$call" "$work/call.json"
  fi
  tool="$(jq -r '.params.name' "$work/call.json")"
  jq -c '.params.arguments' "$work/call.json" > "$work/arguments.json"
  if ! "$PY" "$HERE/mcp.py" call --raw "$server" "$tool" "@$work/arguments.json" > "$work/result.json"; then
    cat "$work/result.json" >&2
    die "reference call $call ($server $tool) failed"
  fi
  printf 'replayed %s: %s %s\n' "$call" "$server" "$tool"
  if [ -n "$capture" ] && [ "$(jq 'length' "$solution/$capture")" -gt 0 ]; then
    jq -n --slurpfile bindings "$work/bindings.json" --slurpfile captures "$solution/$capture" \
      --slurpfile result "$work/result.json" -f "$solution/capture.jq" > "$work/new-bindings.json"
    mv "$work/new-bindings.json" "$work/bindings.json"
  fi
done < "$work/steps"

answer="$PILOT_HOME/agents/$slot/app/answer.md"
mkdir -p "$(dirname "$answer")"
rm -f "$answer"
if grep -q 'answer\.template\.json' "$solution/solve.sh"; then
  jq --slurpfile bindings "$work/bindings.json" -f "$solution/render.jq" "$solution/answer.template.json" \
    | jq -j '.params.arguments | if type == "string" then . else tojson end' > "$answer"
elif grep -q '/answer\.md" /app/answer\.md' "$solution/solve.sh"; then
  cp "$solution/answer.md" "$answer"
fi
if [ -f "$answer" ]; then
  printf 'answer: %s\n' "$answer"
fi
