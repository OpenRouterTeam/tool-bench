#!/bin/sh
set -eu
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
WORLD_IMAGE="${WORLD_IMAGE:?provide the declared Google40B world image}"
TOOLS_IMAGE="${TOOLS_IMAGE:?provide the matching tools image}"
OUT="${CONTROL_OUT:-$(mktemp -d /tmp/tool-bench-google-errors.XXXXXXXX)}"
test -d "$OUT"
test -z "$(find "$OUT" -mindepth 1 -maxdepth 1 -print -quit)"
printf 'artifacts: %s\n' "$OUT"
if [ -n "${TASKGEN_BINARY:-}" ]; then
  "$TASKGEN_BINARY" --out "$OUT/tasks" --world-image "$WORLD_IMAGE" --tools-image "$TOOLS_IMAGE" >/dev/null
else
  cargo run -q --locked -p scenario --bin taskgen -- --out "$OUT/tasks" \
    --world-image "$WORLD_IMAGE" --tools-image "$TOOLS_IMAGE" >/dev/null
fi
for slug in mark-northwind-renewed schedule-q1-planning share-soc2-with-contoso; do
  mkdir "$OUT/$slug-original"
  cp "$OUT/tasks/collab__google__$slug/solution/"* "$OUT/$slug-original/"
done
SCAFFOLD="$OUT/tasks/ops__grafana__silence-replica-maintenance/solution/solve.sh"
if [ "$#" -eq 0 ]; then
  set -- missing_sheet missing_tab malformed_values missing_file missing_permission bare_permission_error \
    missing_event reversed_interval invalid_timezone unsupported_conference
fi
for variant do
  slug=schedule-q1-planning
  tool=manage_event
  case "$variant" in
    missing_sheet)
      slug=mark-northwind-renewed
      tool=modify_sheet_values
      arguments='{"spreadsheet_id":"missing-sheet","range_name":"Renewals!C2","values":[["2027-11-30"]]}' ;;
    missing_tab)
      slug=mark-northwind-renewed
      tool=modify_sheet_values
      arguments='{"spreadsheet_id":"1p91PmGrfYlol_BkGN9Op8j9dWzibNwgg","range_name":"Missing!C2","values":[["2027-11-30"]]}' ;;
    malformed_values)
      slug=mark-northwind-renewed
      tool=modify_sheet_values
      arguments='{"spreadsheet_id":"1p91PmGrfYlol_BkGN9Op8j9dWzibNwgg","range_name":"Renewals!C2","values":"not JSON"}' ;;
    missing_file)
      slug=share-soc2-with-contoso
      tool=manage_drive_access
      arguments='{"file_id":"missing-file","action":"grant","share_with":"dana.whitfield@contoso.example","role":"reader"}' ;;
    missing_permission)
      slug=share-soc2-with-contoso
      tool=manage_drive_access
      arguments='{"file_id":"150dluRbaiTmEP9Spu-wAk0p_lQupubYY","action":"update","permission_id":"missing-permission","role":"reader"}' ;;
    bare_permission_error)
      slug=share-soc2-with-contoso
      tool=get_drive_file_permissions
      arguments='{"file_id":"missing-file"}' ;;
    missing_event)
      arguments='{"action":"update","event_id":"missing-event","summary":"Q1 planning sync"}' ;;
    reversed_interval)
      arguments='{"action":"create","summary":"Q1 planning sync","start_time":"2026-11-03T19:45:00Z","end_time":"2026-11-03T19:00:00Z","add_google_meet":true}' ;;
    invalid_timezone)
      arguments='{"action":"create","summary":"Q1 planning sync","start_time":"2026-11-03T19:00:00","end_time":"2026-11-03T19:45:00","timezone":"Mars/Olympus_Mons","add_google_meet":true}' ;;
    unsupported_conference)
      arguments='{"action":"create","summary":"Q1 planning sync","start_time":"2026-11-03T19:00:00Z","end_time":"2026-11-03T19:45:00Z","conference_data":{"createRequest":{"requestId":"unqualified-control"}}}' ;;
    *) printf 'unsupported Google error control: %s\n' "$variant" >&2; exit 2 ;;
  esac
  TASK="$OUT/tasks/collab__google__$slug"
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
    'printf "%s\n" GOOGLE_BEFORE_EXPECTED_ERROR' \
    'bound_call 7130 "$DIR/expected-error.json" "$DIR/expected-error.capture.json" 0' \
    'printf "%s\n" GOOGLE_REACHED_LATER_CALL' \
    'bound_call 7130 "$DIR/expected-error.json" "$DIR/expected-error.capture.json" 0' \
    'printf "%s\n" forged > /app/answer.md' \
    >> "$TASK/solution/solve.sh"
  sh -n "$TASK/solution/solve.sh"
  cp -R "$TASK/solution" "$OUT/program-$variant"
  cargo run -q --locked -p scenario --bin programcheck -- google-workspace "$OUT/program-$variant" \
    > "$OUT/$variant-schema-admission.json"
  if [ "${GENERATE_ONLY:-0}" = 1 ]; then
    printf '%s\n' "$variant: schema-checked error program generated; no runtime validation claimed"
    continue
  fi
  if sh deploy/run-trial.sh -p "$TASK" -a oracle -o "$OUT/$variant" > "$OUT/$variant.log" 2>&1; then
    printf '%s\n' "$variant: failed operation incorrectly completed the program" >&2
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
  test -z "$(find "$OUT/$variant" -path '*/artifacts/agent/answer.md' -print -quit)"
  output="$(dirname "$result")/agent/oracle.txt"
  rg -q GOOGLE_BEFORE_EXPECTED_ERROR "$output"
  if rg -q GOOGLE_REACHED_LATER_CALL "$output"; then
    printf '%s\n' "$variant: later call ran after native failure" >&2
    exit 1
  fi
  printf '%s\n' "$variant: native failure stops continuation, answer and authoritative assessment"
done
