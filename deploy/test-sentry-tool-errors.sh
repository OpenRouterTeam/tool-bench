#!/bin/sh
set -eu
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
WORLD_IMAGE="${WORLD_IMAGE:?provide the revision-30 Sentry world image}"
TOOLS_IMAGE="${TOOLS_IMAGE:?provide the revision-30 Sentry tools image}"
OUT="$(mktemp -d /tmp/tool-bench-sentry-errors.XXXXXX)"
printf 'artifacts: %s\n' "$OUT"
cargo run -q --locked -p scenario --bin taskgen -- --runner-admission required --out "$OUT/tasks" \
  --world-image "$WORLD_IMAGE" --tools-image "$TOOLS_IMAGE" >/dev/null
TASK="$OUT/tasks/ops__sentry__create-notification-project"
mkdir "$OUT/original"
cp "$TASK/solution/"* "$OUT/original/"
for variant in conflict last_team unknown_tool invalid_actor; do
  rm -rf "$TASK/solution"
  mkdir "$TASK/solution"
  cp "$OUT/original/"* "$TASK/solution/"
  tool=execute_sentry_tool
  case "$variant" in
    conflict)
      arguments='{"name":"create_project","arguments":{"organizationSlug":"lumen-labs","teamSlug":"payments","name":"notification-service","platform":"node"}}' ;;
    last_team)
      arguments='{"name":"remove_team_from_project","arguments":{"organizationSlug":"lumen-labs","projectSlug":"notification-service","teamSlug":"payments"}}' ;;
    unknown_tool) arguments='{"name":"not_a_registered_catalog_tool","arguments":{}}' ;;
    invalid_actor)
      tool=update_issue
      arguments='{"organizationSlug":"lumen-labs","issueId":"PAYMENTS-WORKER-1K","assignedTo":"team:not-a-team"}' ;;
  esac
  jq -n --arg tool "$tool" --argjson arguments "$arguments" \
    '{jsonrpc:"2.0",id:2,method:"tools/call",params:{name:$tool,arguments:$arguments}}' \
    > "$TASK/solution/error.json"
  printf '%s\n' 'printf "%s\n" SENTRY_BEFORE_EXPECTED_ERROR' \
    'call 7113 "$DIR/error.json"' 'printf "%s\n" SENTRY_REACHED_LATER_CALL' \
    'call 7113 "$DIR/error.json"' >> "$TASK/solution/solve.sh"
  cp -R "$TASK/solution" "$OUT/program-$variant"
  if sh deploy/run-trial.sh -p "$TASK" -a oracle -o "$OUT/$variant" > "$OUT/$variant.log" 2>&1; then
    printf '%s\n' "$variant: a failed native tool result completed the reference" >&2
    exit 1
  fi
  result="$(find "$OUT/$variant" -name result.json -print -quit)"
  receipt="$(find "$OUT/$variant" -name runner-admission.json -print -quit)"
  test -n "$result"
  test -n "$receipt"
  jq -e '.exception_info!=null and .verifier_result==null' "$result" >/dev/null
  jq -e '.phase=="failed" and .failure!=null and .evidence_sha256==null
    and .main_artifacts_sha256==null' "$receipt" >/dev/null
  test -z "$(find "$OUT/$variant" -path '*/verifier/reward.json' -print -quit)"
  test -z "$(find "$OUT/$variant" -path '*/artifacts/world/evidence.json' -print -quit)"
  log="$(dirname "$result")/agent/oracle.txt"
  grep -q SENTRY_BEFORE_EXPECTED_ERROR "$log"
  if grep -q SENTRY_REACHED_LATER_CALL "$log"; then exit 1; fi
  jq -Rse '[split("\n")[]|if startswith("data: ") then .[6:] else . end
      | fromjson?|select(type=="object" and .jsonrpc=="2.0" and .id==2)]
    | any(.[];.result.isError==true)' "$log" >/dev/null
  printf '%s\n' "$variant: actual native failure stops later calls and authoritative assessment"
done
