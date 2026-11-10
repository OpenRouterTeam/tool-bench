#!/bin/sh
set -eu
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
WORLD_IMAGE="${WORLD_IMAGE:?provide the revision42 world candidate}"
TOOLS_IMAGE="${TOOLS_IMAGE:?provide the matching current-catalog tools candidate}"
OUT="${CONTROL_OUT:-$(mktemp -d /tmp/tool-bench-firecrawl-controls.XXXXXXXX)}"
test -d "$OUT"
test -z "$(find "$OUT" -mindepth 1 -maxdepth 1 -print -quit)"
printf 'artifacts: %s\n' "$OUT"
if [ -n "${TASKGEN_BINARY:-}" ]; then
  "$TASKGEN_BINARY" --out "$OUT/tasks" --world-image "$WORLD_IMAGE" --tools-image "$TOOLS_IMAGE" >/dev/null
else
  cargo run -q --locked -p scenario --bin taskgen -- --runner-admission required --out "$OUT/tasks" \
    --world-image "$WORLD_IMAGE" --tools-image "$TOOLS_IMAGE" >/dev/null
fi
for slug in crawl-api-reference monitor-competitor-pricing pause-broken-pricing-monitor; do
  mkdir "$OUT/$slug-original"
  cp "$OUT/tasks/search__firecrawl__$slug/solution/"* "$OUT/$slug-original/"
done
SCAFFOLD="$OUT/tasks/ops__grafana__silence-replica-maintenance/solution/solve.sh"
add() {
  jq -n --arg tool "$1" --argjson arguments "$2" \
    '{jsonrpc:"2.0",id:2,method:"tools/call",params:{name:$tool,arguments:$arguments}}' \
    > "$TASK/solution/control-$calls.json"
  printf '%s\n' "${3:-[]}" > "$TASK/solution/control-$calls.capture.json"
  printf 'bound_call 7102 "$DIR/control-%s.json" "$DIR/control-%s.capture.json" %s\n' \
    "$calls" "$calls" "${4:-0}" >> "$TASK/solution/solve.sh"
  calls=$((calls + 1))
}
crawl() {
  extra="${1:-}"
  [ -n "$extra" ] || extra='{}'
  captures="${2:-}"
  [ -n "$captures" ] || captures='[{"name":"new_crawl","pointer":"/structuredContent/id"}]'
  add firecrawl_crawl "$(jq -cn --argjson extra "$extra" '
    {url:"https://docs.lumenpay.dev/api/overview",crawlEntireDomain:true,
      includePaths:["^/api/.*"],limit:50}+$extra')" \
    "$captures"
}
monitor() {
  extra="${1:-}"
  [ -n "$extra" ] || extra='{}'
  captures="${2:-}"
  [ -n "$captures" ] || captures='[{"name":"new_monitor","pointer":"/structuredContent/data/id"}]'
  add firecrawl_monitor_create "$(jq -cn --argjson extra "$extra" '
    {body:({name:"Fabrikam pricing",schedule:{text:"daily",timezone:"UTC"},
      targets:[{type:"scrape",urls:["https://www.fabrikam.io/pricing"]}],
      goal:"Alert when Fabrikam changes any plan price or plan limit.",
      notification:{email:{enabled:true,recipients:["pricing-intel@contoso.example"],includeDiffs:false}}}+$extra)}')" \
    "$captures"
}
update_new() {
  add firecrawl_monitor_update "$(jq -cn --argjson body "$1" \
    '{id:"${new_monitor}",body:$body}')" '[]' 1
}
pause() {
  extra="${1:-}"
  [ -n "$extra" ] || extra='{}'
  add firecrawl_monitor_update "$(jq -cn --argjson extra "$extra" \
    '{id:"0c7e4b21-9d3a-4e6f-a1b2-5c8d7e6f9a11",body:({status:"paused"}+$extra)}')"
}
run_healthy() {
  captures="${1:-}"
  [ -n "$captures" ] || captures='[{"name":"new_check","pointer":"/structuredContent/data/id"}]'
  add firecrawl_monitor_run '{"id":"0c7e4b21-9d3a-4e6f-a1b2-5c8d7e6f9a12"}' \
    "$captures"
}
poll_healthy() {
  for observation in 1 2; do
    add firecrawl_monitor_check \
      '{"id":"0c7e4b21-9d3a-4e6f-a1b2-5c8d7e6f9a12","checkId":"${new_check}"}' '[]' 1
  done
}
reference() {
  printf '%s\n' 'sh "$DIR/reference-solve.sh"' >> "$TASK/solution/solve.sh"
}
if [ "$#" -eq 0 ]; then
  set -- crawl_reference crawl_direct crawl_case crawl_readback crawl_mixed crawl_extra \
    create_reference create_broad create_timezone create_staged_email create_clock \
    create_paused_clock create_initial_check create_disabled_email create_extra_recipient \
    create_wrong_target create_weekdays create_twice_daily create_contradictory \
    create_unknown_filter create_restored_filter create_existing_run create_duplicate \
    pause_reference pause_direct pause_readback pause_wrong_order pause_extra_goal \
    pause_extra_monitor pause_duplicate_check pause_restored_goal
fi
for variant do
  calls=0
  expected=fail
  reward=0
  case "$variant" in
    crawl_*) slug=crawl-api-reference ;;
    create_*) slug=monitor-competitor-pricing ;;
    pause_*) slug=pause-broken-pricing-monitor ;;
    *) printf 'unsupported Firecrawl control: %s\n' "$variant" >&2; exit 2 ;;
  esac
  TASK="$OUT/tasks/search__firecrawl__$slug"
  rm -rf "$TASK/solution"
  mkdir "$TASK/solution"
  cp "$OUT/$slug-original/"* "$TASK/solution/"
  cp "$TASK/solution/solve.sh" "$TASK/solution/reference-solve.sh"
  cp "$OUT/tasks/ops__grafana__silence-replica-maintenance/solution/"*.jq "$TASK/solution/"
  sed -e '/^bound_call [0-9]/,$d' -e '/^call [0-9]/,$d' "$SCAFFOLD" > "$TASK/solution/solve.sh"
  printf '%s\n' 'export ORACLE_REQUEST_CAPTURE_DIR=/logs/artifacts/mcp-requests' \
    >> "$TASK/solution/solve.sh"
  case "$variant" in
    crawl_reference|create_reference|pause_reference) reference; expected=pass ;;
    crawl_direct) crawl; expected=pass ;;
    crawl_case) crawl '{"url":"https://DOCS.LUMENPAY.DEV:443/api/overview#reference"}'; expected=pass ;;
    crawl_readback)
      crawl
      add firecrawl_check_crawl_status '{"id":"${new_crawl}"}' '[]' 1
      expected=pass ;;
    crawl_mixed)
      crawl '{"url":"https://docs.lumenpay.dev","includePaths":["^/$","^/api/overview$","^/guides/.*"]}' ;;
    crawl_extra) crawl; crawl '{}' '[]' ;;
    create_broad) monitor '{"goal":null,"judgeEnabled":false}'; expected=pass ;;
    create_timezone)
      monitor '{"schedule":{"cron":"15 17 1-31 1-12 0-7","timezone":"America/New_York"}}'
      expected=pass ;;
    create_staged_email)
      monitor '{"notification":{},"goal":null,"judgeEnabled":false}'
      update_new '{"notification":{"email":{"enabled":true,"recipients":["PRICING-INTEL@CONTOSO.EXAMPLE"],"includeDiffs":false}}}'
      expected=pass ;;
    create_clock)
      monitor
      update_new '{"schedule":{"cron":"0 9 * * *","timezone":"America/New_York"}}'
      expected=pass ;;
    create_paused_clock)
      monitor
      update_new '{"status":"paused"}'
      update_new '{"schedule":{"cron":"0 0,12 * * *","timezone":"UTC"}}'
      update_new '{"schedule":{"cron":"0 7 * * *","timezone":"Europe/Berlin"}}'
      update_new '{"status":"active"}'
      expected=pass ;;
    create_initial_check)
      monitor
      add firecrawl_monitor_run '{"id":"${new_monitor}"}' \
        '[{"name":"new_check","pointer":"/structuredContent/data/id"}]' 1
      for observation in 1 2; do
        add firecrawl_monitor_check '{"id":"${new_monitor}","checkId":"${new_check}"}' '[]' 1
      done
      expected=pass ;;
    create_disabled_email)
      monitor '{"notification":{"email":{"enabled":false,"recipients":["pricing-intel@contoso.example"]}}}' ;;
    create_extra_recipient)
      monitor '{"notification":{"email":{"enabled":true,"recipients":["pricing-intel@contoso.example","other@example.test"]}}}' ;;
    create_wrong_target) monitor '{"targets":[{"type":"scrape","urls":["https://www.fabrikam.io/pricing-backup"]}]}' ;;
    create_weekdays) monitor '{"schedule":{"cron":"0 9 * * 1-5","timezone":"UTC"}}' ;;
    create_twice_daily) monitor '{"schedule":{"cron":"0 0,12 * * *","timezone":"UTC"}}' ;;
    create_contradictory) monitor '{"goal":"Only alert about prices, never plan limits."}' ;;
    create_unknown_filter)
      monitor '{"goal":"Watch meaningful price and limit revisions."}'
      expected=unresolved ;;
    create_restored_filter)
      monitor
      update_new '{"goal":"Only alert about prices, never plan limits."}'
      update_new '{"goal":"Notify when prices or plan limits change."}' ;;
    create_existing_run)
      monitor
      add firecrawl_monitor_run '{"id":"3f6b2a10-5c4e-4d21-9b7a-1e0c2d3f4a02"}' ;;
    create_duplicate) monitor; monitor '{}' '[]' ;;
    pause_direct) pause; run_healthy; expected=pass ;;
    pause_readback) pause; run_healthy; poll_healthy; expected=pass ;;
    pause_wrong_order) run_healthy; pause ;;
    pause_extra_goal) pause '{"goal":"An unrequested criterion"}'; run_healthy ;;
    pause_extra_monitor)
      pause
      run_healthy
      add firecrawl_monitor_run '{"id":"0c7e4b21-9d3a-4e6f-a1b2-5c8d7e6f9a13"}' ;;
    pause_duplicate_check) pause; run_healthy; poll_healthy; run_healthy '[]' ;;
    pause_restored_goal)
      pause '{"goal":"Changed without permission"}'
      pause '{"goal":"Alert when Tailspin Toys changes plan prices or discounts."}'
      run_healthy ;;
    *) printf 'unsupported Firecrawl control: %s\n' "$variant" >&2; exit 2 ;;
  esac
  [ "$expected" != pass ] || reward=1
  sh -n "$TASK/solution/solve.sh"
  cp -R "$TASK/solution" "$OUT/program-$variant"
  jq -n --arg variant "$variant" --arg slug "$slug" --arg expected "$expected" \
    '{variant:$variant,endpoint:"firecrawl",scenario:("search/firecrawl/"+$slug),expected_outcome:$expected,
      scope:"scripted_native_client_control_not_model_performance"}' > "$OUT/program-$variant/control.json"
  if [ "${GENERATE_ONLY:-0}" = 1 ]; then
    printf '%s\n' "$variant: program generated; rendered request validation requires execution"
    continue
  fi
  sh deploy/run-trial.sh -p "$TASK" -a oracle -o "$OUT/$variant" > "$OUT/$variant.log" 2>&1
  assessment="$(find "$OUT/$variant" -path '*/verifier/reward.json' -print -quit)"
  test -n "$assessment"
  jq -e --arg expected "$expected" --argjson reward "$reward" '
    (["pass","fail","unresolved"]|index($expected))!=null
      and .reward==$reward and .evaluator_version==42 and .contract_version==2
      and .outcome_code==(if $expected=="pass" then 0 elif $expected=="fail" then 1 else 6 end)
      and .environment_ok==1 and .contract_ok==1 and .integrity_ok==1
      and .runner_admission_checked==1 and .runner_admission_ok==1
      and .preservation_checked==1 and .history_checked==1
      and ($expected!="pass" or (.history_ok==1 and .preservation_ok==1))
      and .adjudication_needed==(if $expected=="unresolved" then 1 else 0 end)
  ' "$assessment" >/dev/null
  verifier="$(dirname "$assessment")"
  PYTHONDONTWRITEBYTECODE=1 "${FIRECRAWL_AUDIT_PYTHON:-python3}" deploy/audit-firecrawl-business.py "$verifier" \
    > "$OUT/$variant-independent-audit.log" 2>&1
  cat "$OUT/$variant-independent-audit.log"
  requests="$(dirname "$verifier")/artifacts/logs/artifacts/mcp-requests"
  test -d "$requests"
  for observed in "$requests"/*; do
    test "$(cat "$observed/endpoint.txt")" = 7102
  done
  if [ -n "${PROGRAMCHECK_BINARY:-}" ]; then
    "$PROGRAMCHECK_BINARY" firecrawl "$requests"/* > "$OUT/$variant-actual-schemas.json"
  else
    cargo run -q --locked -p scenario --bin programcheck -- firecrawl "$requests"/* \
      > "$OUT/$variant-actual-schemas.json"
  fi
  printf '%s\n' "$variant: native completion, source-bound business/history and observed request schemas agree"
done
