#!/bin/sh
set -eu
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
WORLD_IMAGE="${WORLD_IMAGE:?provide the revision42 world candidate}"
TOOLS_IMAGE="${TOOLS_IMAGE:?provide the matching current-catalog tools candidate}"
OUT="${CONTROL_OUT:-$(mktemp -d /tmp/tool-bench-firecrawl-tool-errors.XXXXXXXX)}"
test -d "$OUT"
test -z "$(find "$OUT" -mindepth 1 -maxdepth 1 -print -quit)"
printf 'artifacts: %s\n' "$OUT"
if [ "$#" -eq 0 ]; then
  set -- missing_read paused_run overlapping_run invalid_zone invalid_cron program_exit
fi
for variant do
  mkdir "$OUT/generated-$variant"
  case "$variant" in
    paused_run|overlapping_run) source_control=pause_direct; slug=pause-broken-pricing-monitor ;;
    *) source_control=create_clock; slug=monitor-competitor-pricing ;;
  esac
  CONTROL_OUT="$OUT/generated-$variant" GENERATE_ONLY=1 \
    sh deploy/test-firecrawl-contracts.sh "$source_control" > "$OUT/$variant-generation.log" 2>&1
  TASK="$OUT/generated-$variant/tasks/search__firecrawl__$slug"
  jq -n '{jsonrpc:"2.0",id:2,method:"tools/call",
    params:{name:"firecrawl_monitor_list",arguments:{}}}' > "$TASK/solution/later.json"
  printf '%s\n' 'printf "%s\n" FIRECRAWL_ERROR_CONTROL_PREFIX_COMPLETED' \
    >> "$TASK/solution/solve.sh"
  case "$variant" in
    missing_read)
      tool=firecrawl_monitor_get; arguments='{"id":"missing-after-configuration"}'; template=0 ;;
    paused_run)
      tool=firecrawl_monitor_run; arguments='{"id":"0c7e4b21-9d3a-4e6f-a1b2-5c8d7e6f9a11"}'; template=0 ;;
    overlapping_run)
      tool=firecrawl_monitor_run; arguments='{"id":"0c7e4b21-9d3a-4e6f-a1b2-5c8d7e6f9a12"}'; template=0 ;;
    invalid_zone)
      tool=firecrawl_monitor_update; template=1
      arguments='{"id":"${new_monitor}","body":{"schedule":{"cron":"0 9 * * *","timezone":"Mars/Olympus"}}}' ;;
    invalid_cron)
      tool=firecrawl_monitor_update; template=1
      arguments='{"id":"${new_monitor}","body":{"schedule":{"cron":"0 25 * * *","timezone":"UTC"}}}' ;;
    program_exit)
      printf '%s\n' 'exit 7' >> "$TASK/solution/solve.sh"
      tool=firecrawl_monitor_list; arguments='{}'; template=0 ;;
    *) printf 'unsupported Firecrawl error control: %s\n' "$variant" >&2; exit 2 ;;
  esac
  jq -n --arg tool "$tool" --argjson arguments "$arguments" \
    '{jsonrpc:"2.0",id:2,method:"tools/call",params:{name:$tool,arguments:$arguments}}' \
    > "$TASK/solution/error.json"
  printf '%s\n' '[]' > "$TASK/solution/error.capture.json"
  printf 'bound_call 7102 "$DIR/error.json" "$DIR/error.capture.json" %s\n' "$template" \
    >> "$TASK/solution/solve.sh"
  printf '%s\n' 'printf "%s\n" FIRECRAWL_ERROR_CONTROL_REACHED_LATER_CALL' \
    'call 7102 "$DIR/later.json"' >> "$TASK/solution/solve.sh"
  sh -n "$TASK/solution/solve.sh"
  cp -R "$TASK/solution" "$OUT/program-$variant"
  if [ "${GENERATE_ONLY:-0}" = 1 ]; then
    printf '%s\n' "$variant: error program generated; no execution or failure claim"
    continue
  fi
  if sh deploy/run-trial.sh -p "$TASK" -a oracle -o "$OUT/$variant" > "$OUT/$variant.log" 2>&1; then
    printf '%s\n' 'An intended failed native program incorrectly completed' >&2
    exit 1
  fi
  result="$(find "$OUT/$variant" -name result.json -print -quit)"
  receipt="$(find "$OUT/$variant" -name runner-admission.json -print -quit)"
  reward="$(find "$OUT/$variant" -path '*/verifier/reward.json' -print -quit)"
  test -n "$result" && test -n "$receipt" && test -z "$reward"
  jq -e '.exception_info!=null' "$result" >/dev/null
  jq -e '.phase=="failed" and .failure!=null and .evidence_sha256==null
    and .main_artifacts_sha256==null' "$receipt" >/dev/null
  log="$(dirname "$result")/agent/oracle.txt"
  rg -q FIRECRAWL_ERROR_CONTROL_PREFIX_COMPLETED "$log"
  if find "$OUT/$variant" -path '*/artifacts/world/evidence.json' -print -quit | rg -q .; then
    printf '%s\n' 'A failed native program supplied authoritative world evidence' >&2
    exit 1
  fi
  if rg -q FIRECRAWL_ERROR_CONTROL_REACHED_LATER_CALL "$log"; then
    printf '%s\n' 'A failed native request authorized a later call' >&2
    exit 1
  fi
  if [ "$variant" != program_exit ]; then
    PYTHONDONTWRITEBYTECODE=1 "${FIRECRAWL_AUDIT_PYTHON:-python3}" - "$log" <<'PY'
import json
import sys
from pathlib import Path

text = Path(sys.argv[1]).read_text().split("FIRECRAWL_ERROR_CONTROL_PREFIX_COMPLETED", 1)[1]
decoder = json.JSONDecoder()
observed = []
for position, character in enumerate(text):
    if character != "{":
        continue
    try:
        value, _ = decoder.raw_decode(text[position:])
    except ValueError:
        continue
    if isinstance(value, dict) and value.get("jsonrpc") == "2.0" and value.get("id") == 2:
        observed.append(value)
assert any(value.get("error") is not None
           or isinstance(value.get("result"), dict) and value["result"].get("isError") is True
           for value in observed), "no actual failed native MCP response after the successful prefix"
PY
  fi
  printf '%s\n' "$variant: actual failure stopped later calls, authoritative collection and numeric scoring"
done
