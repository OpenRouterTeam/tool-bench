#!/bin/sh
set -eu
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
WORLD_IMAGE="${WORLD_IMAGE:?provide the Datadog38 world candidate}"
TOOLS_IMAGE="${TOOLS_IMAGE:?provide the matching tools candidate}"
OUT="${CONTROL_OUT:-$(mktemp -d /tmp/tool-bench-datadog-errors.XXXXXX)}"
test -d "$OUT"
test -z "$(find "$OUT" -mindepth 1 -maxdepth 1 -print -quit)"
printf 'artifacts: %s\n' "$OUT"
if [ -n "${TASKGEN_BINARY:-}" ]; then
  "$TASKGEN_BINARY" --out "$OUT/tasks" --world-image "$WORLD_IMAGE" --tools-image "$TOOLS_IMAGE" >/dev/null
else
  cargo run -q --locked -p scenario --bin taskgen -- --out "$OUT/tasks" \
    --world-image "$WORLD_IMAGE" --tools-image "$TOOLS_IMAGE" >/dev/null
fi
for slug in cancel-called-off-maintenance mute-host-from-error-logs schedule-replica-downtime; do
  mkdir "$OUT/$slug-original"
  cp "$OUT/tasks/ops__datadog__$slug/solution/"* "$OUT/$slug-original/"
done
SCAFFOLD="$OUT/tasks/ops__grafana__silence-replica-maintenance/solution/solve.sh"
if [ "$#" -eq 0 ]; then
  set -- missing_cancel missing_host unknown_monitor reversed_interval invalid_timezone \
    combined_selector empty_scope native_date_after_commit
fi
for variant do
  slug=schedule-replica-downtime
  tool=schedule_downtime
  case "$variant" in
    missing_cancel)
      slug=cancel-called-off-maintenance
      tool=cancel_downtime
      arguments='{"downtimeId":999999999}' ;;
    missing_host)
      slug=mute-host-from-error-logs
      tool=mute_host
      arguments='{"hostname":"missing-host","end":1793383200}' ;;
    unknown_monitor)
      arguments='{"scope":"host:pg-prod-replica-2","monitorId":999999999,"start":1793412000,"end":1793419200}' ;;
    reversed_interval)
      arguments='{"scope":"host:pg-prod-replica-2","monitorId":7301,"start":1793419200,"end":1793412000}' ;;
    invalid_timezone)
      arguments='{"scope":"host:pg-prod-replica-2","monitorId":7301,"start":1793412000,"end":1793419200,"timezone":"Mars/Olympus_Mons"}' ;;
    combined_selector)
      arguments='{"scope":"host:pg-prod-replica-2","monitorId":7301,"monitorTags":["env:prod"],"start":1793412000,"end":1793419200}' ;;
    empty_scope)
      arguments='{"scope":",,","monitorId":7301,"start":1793412000,"end":1793419200}' ;;
    native_date_after_commit)
      slug=mute-host-from-error-logs
      tool=mute_host
      arguments='{"hostname":"checkout-api-prod-1b","end":8640000000001,"message":"Native date range probe INC-2291","override":true}' ;;
    *) printf 'unsupported Datadog error control: %s\n' "$variant" >&2; exit 2 ;;
  esac
  TASK="$OUT/tasks/ops__datadog__$slug"
  rm -rf "$TASK/solution"
  mkdir "$TASK/solution"
  cp "$OUT/$slug-original/"* "$TASK/solution/"
  cp "$TASK/solution/solve.sh" "$TASK/solution/reference-solve.sh"
  cp "$OUT/tasks/ops__grafana__silence-replica-maintenance/solution/"*.jq "$TASK/solution/"
  sed -e '/^bound_call [0-9]/,$d' -e '/^call [0-9]/,$d' "$SCAFFOLD" > "$TASK/solution/solve.sh"
  jq -n --arg tool "$tool" --argjson arguments "$arguments" \
    '{jsonrpc:"2.0",id:2,method:"tools/call",params:{name:$tool,arguments:$arguments}}' \
    > "$TASK/solution/expected-error.json"
  printf '%s\n' '[]' > "$TASK/solution/expected-error.capture.json"
  printf '%s\n' \
    'sh "$DIR/reference-solve.sh"' \
    'printf "%s\n" DATADOG_BEFORE_EXPECTED_ERROR' \
    'bound_call 7112 "$DIR/expected-error.json" "$DIR/expected-error.capture.json" 0' \
    'printf "%s\n" DATADOG_REACHED_LATER_CALL' \
    'bound_call 7112 "$DIR/expected-error.json" "$DIR/expected-error.capture.json" 0' \
    >> "$TASK/solution/solve.sh"
  sh -n "$TASK/solution/solve.sh"
  cp -R "$TASK/solution" "$OUT/program-$variant"
  if [ "${GENERATE_ONLY:-0}" = 1 ]; then
    printf '%s\n' "$variant: program generated; no runtime validation claimed"
    continue
  fi
  if sh deploy/run-trial.sh -p "$TASK" -a oracle -o "$OUT/$variant" > "$OUT/$variant.log" 2>&1; then
    printf '%s\n' "$variant: failed native operation incorrectly completed the program" >&2
    exit 1
  fi
  result="$(find "$OUT/$variant" -name result.json -print -quit)"
  receipt="$(find "$OUT/$variant" -name runner-admission.json -print -quit)"
  test -n "$result" && test -n "$receipt"
  jq -e '.exception_info!=null and .verifier_result==null' "$result" >/dev/null
  jq -e '.phase=="failed" and .failure!=null and .evidence_sha256==null
    and .main_artifacts_sha256==null' "$receipt" >/dev/null
  test -z "$(find "$OUT/$variant" -path '*/verifier/reward.json' -print -quit)"
  test -z "$(find "$OUT/$variant" -path '*/artifacts/world/evidence.json' -print -quit)"
  output="$(dirname "$result")/agent/oracle.txt"
  rg -q DATADOG_BEFORE_EXPECTED_ERROR "$output"
  if rg -q DATADOG_REACHED_LATER_CALL "$output"; then
    printf '%s\n' "$variant: later call ran after native failure" >&2
    exit 1
  fi
  printf '%s\n' "$variant: native failure stops later calls and authoritative assessment"
done
