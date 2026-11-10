#!/bin/sh
set -eu
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
WORLD_IMAGE="${WORLD_IMAGE:?provide the frozen Grafana35 world candidate}"
TOOLS_IMAGE="${TOOLS_IMAGE:?provide the matching Grafana tools candidate}"
OUT="$(mktemp -d /tmp/tool-bench-grafana-errors.XXXXXX)"
printf 'artifacts: %s\n' "$OUT"
if [ -n "${TASKGEN_BINARY:-}" ]; then
  "$TASKGEN_BINARY" --out "$OUT/tasks" --world-image "$WORLD_IMAGE" --tools-image "$TOOLS_IMAGE" >/dev/null
else
  cargo run -q --locked -p scenario --bin taskgen -- --runner-admission required --out "$OUT/tasks" \
    --world-image "$WORLD_IMAGE" --tools-image "$TOOLS_IMAGE" >/dev/null
fi
for slug in ack-checkout-page-oncall annotate-checkout-deploy silence-replica-maintenance; do
  mkdir "$OUT/$slug-original"
  cp "$OUT/tasks/ops__grafana__$slug/solution/"* "$OUT/$slug-original/"
done
for variant in missing_group duplicate_ack invalid_annotation_type missing_silence unqualified_regex; do
  slug=ack-checkout-page-oncall
  case "$variant" in
    missing_group)
      tool=get_alert_group; arguments='{"alertGroupId":"not-a-real-group"}' ;;
    duplicate_ack)
      tool=update_alert_group; arguments='{"alertGroupId":"IHQ4K2M7","state":"acknowledged"}' ;;
    invalid_annotation_type)
      slug=annotate-checkout-deploy
      tool=grafana_api_request
      arguments="$(jq -cn '{endpoint:"/api/annotations",method:"POST",
        body:({dashboardUID:"checkout-prod",time:"not a timestamp",text:"Invalid additional event"}|tojson)}')" ;;
    missing_silence)
      slug=silence-replica-maintenance
      tool=alerting_silences_read; arguments='{"operation":"get","silence_id":"not-a-real-silence"}' ;;
    unqualified_regex)
      slug=silence-replica-maintenance
      tool=grafana_api_request
      arguments="$(jq -cn '{endpoint:"/api/alertmanager/grafana/api/v2/silences",method:"POST",
        body:({startsAt:"2026-11-13T02:00:00Z",endsAt:"2026-11-13T04:00:00Z",
          comment:"CHG-7781 unsupported language probe",createdBy:"sre",
          matchers:[{name:"instance",value:"\\p{L}+",isRegex:true,isEqual:true}]}|tojson)}')" ;;
  esac
  TASK="$OUT/tasks/ops__grafana__$slug"
  rm -rf "$TASK/solution"
  mkdir "$TASK/solution"
  cp "$OUT/$slug-original/"* "$TASK/solution/"
  jq -n --arg tool "$tool" --argjson arguments "$arguments" \
    '{jsonrpc:"2.0",id:2,method:"tools/call",params:{name:$tool,arguments:$arguments}}' \
    > "$TASK/solution/expected-error.json"
  printf '%s\n' '[]' > "$TASK/solution/expected-error.capture.json"
  if [ "$slug" = annotate-checkout-deploy ]; then
    printf '%s\n' 'printf "%s\n" GRAFANA_BEFORE_EXPECTED_ERROR' \
      'call 7114 "$DIR/expected-error.json"' >> "$TASK/solution/solve.sh"
  else
    printf '%s\n' 'printf "%s\n" GRAFANA_BEFORE_EXPECTED_ERROR' \
      'bound_call 7114 "$DIR/expected-error.json" "$DIR/expected-error.capture.json" 0' \
      >> "$TASK/solution/solve.sh"
  fi
  printf '%s\n' 'printf "%s\n" GRAFANA_REACHED_LATER_CALL' \
    'call 7114 "$DIR/expected-error.json"' >> "$TASK/solution/solve.sh"
  sh -n "$TASK/solution/solve.sh"
  cp -R "$TASK/solution" "$OUT/program-$variant"
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
  grep -q GRAFANA_BEFORE_EXPECTED_ERROR "$output"
  if grep -q GRAFANA_REACHED_LATER_CALL "$output"; then
    printf '%s\n' "$variant: later call ran after native failure" >&2
    exit 1
  fi
  printf '%s\n' "$variant: native failure stops later calls and authoritative assessment"
done
