#!/bin/sh
set -eu
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
WORLD_IMAGE="${WORLD_IMAGE:?provide the revision-30 candidate world image}"
TOOLS_IMAGE="${TOOLS_IMAGE:?provide the revision-30 candidate tools image}"
OUT="$(mktemp -d /tmp/tool-bench-sentry.XXXXXX)"
printf 'artifacts: %s\n' "$OUT"
cargo run -q --locked -p scenario --bin taskgen -- --out "$OUT/tasks" \
  --world-image "$WORLD_IMAGE" --tools-image "$TOOLS_IMAGE" >/dev/null
for slug in create-notification-project resolve-worker-pool-timeouts triage-storefront-map-error; do
  mkdir "$OUT/$slug-original"
  cp "$OUT/tasks/ops__sentry__$slug/solution/"* "$OUT/$slug-original/"
done
add() {
  captures="${3:-[]}"
  template="${4:-0}"
  jq -n --arg tool "$1" --argjson arguments "$2" \
    '{jsonrpc:"2.0",id:2,method:"tools/call",params:{name:$tool,arguments:$arguments}}' \
    > "$TASK/solution/control-$calls.json"
  printf '%s\n' "$captures" > "$TASK/solution/control-$calls.capture.json"
  printf 'bound_call 7113 "$DIR/control-%s.json" "$DIR/control-%s.capture.json" %s\n' \
    "$calls" "$calls" "$template" >> "$TASK/solution/solve.sh"
  calls=$((calls + 1))
}
catalog() {
  add execute_sentry_tool "$(jq -cn --arg name "$1" --argjson arguments "$2" \
    '{name:$name,arguments:$arguments}')" "${3:-[]}" "${4:-0}"
}
project() {
  catalog create_project "$(jq -cn --arg team "$1" --arg slug "$SLUG" --argjson platform "$2" \
    '{organizationSlug:"lumen-labs",teamSlug:$team,name:"notification-service",slug:$slug,platform:$platform}')"
}
dsn() {
  catalog find_dsns "$(jq -cn --arg slug "$1" \
    '{organizationSlug:"lumen-labs",projectSlug:$slug}')" \
    '[{"name":"dsn","pointer":"/structuredContent/dsns","select":{"matching":{"name":"Default"},"pointer":"/dsn"}},
      {"name":"key_id","pointer":"/structuredContent/dsns","select":{"matching":{"name":"Default"},"pointer":"/id"}}]'
}
update() {
  add update_issue "$(jq -cn --arg issue "$1" --argjson body "$2" \
    '{organizationSlug:"lumen-labs",issueId:$issue}+$body')"
}
worker() { update PAYMENTS-WORKER-1K '{"status":"resolved"}'; }
storefront() {
  update STOREFRONT-WEB-8D '{"status":"resolvedInNextRelease","assignedTo":"team:4507214"}'
}
visible() {
  printf '%s\n' 'jq -Rse -f "$DIR/response.jq" "$WORK/response.json" > "$WORK/visible.json"' \
    >> "$TASK/solution/solve.sh"
  printf 'jq -e --arg token "%s" '\''[.content[]?|select(.type=="text")|.text]|any(.[];contains($token))'\'' "$WORK/visible.json" >/dev/null\n' \
    "$1" >> "$TASK/solution/solve.sh"
}
if [ "$#" -eq 0 ]; then
  set -- create_reference create_alt_slug create_rename create_custom_key create_wrong_team \
    create_extra_team create_team_restored create_wrong_key create_wrong_project create_wrong_origin \
    create_inactive create_other_key create_unreviewed create_contradictory \
    worker_reference worker_retry worker_next_release worker_assignee worker_other_issue worker_reason \
    storefront_reference storefront_split storefront_reverse storefront_wrong_team storefront_immediate storefront_other_issue
fi
for variant do
  reward=0
  outcome=1
  adjudication=0
  answer_mode=actual
  calls=0
  SLUG=notification-service
  case "$variant" in
    create_*) slug=create-notification-project ;;
    worker_*) slug=resolve-worker-pool-timeouts ;;
    storefront_*) slug=triage-storefront-map-error ;;
    *) printf 'unsupported Sentry variant: %s\n' "$variant" >&2; exit 2 ;;
  esac
  TASK="$OUT/tasks/ops__sentry__$slug"
  ORIGINAL="$OUT/$slug-original"
  rm -rf "$TASK/solution"
  mkdir "$TASK/solution"
  cp "$ORIGINAL/"* "$TASK/solution/"
  sed -e '/^bound_call [0-9]/,$d' -e '/^call [0-9]/,$d' \
    "$OUT/create-notification-project-original/solve.sh" > "$TASK/solution/solve.sh"
  cp "$OUT/create-notification-project-original/"*.jq "$TASK/solution/"
  case "$variant" in
    create_reference)
      catalog find_teams '{"organizationSlug":"lumen-labs"}'
      visible payments
      project payments '"node"'
      dsn "$SLUG"; reward=1; outcome=0 ;;
    create_alt_slug)
      SLUG=notifications-node
      project payments null
      dsn "$SLUG"; reward=1; outcome=0 ;;
    create_rename)
      project payments '"node"'
      catalog update_project '{"organizationSlug":"lumen-labs","projectSlug":"notification-service","slug":"notifications-node"}'
      SLUG=notifications-node
      dsn "$SLUG"; reward=1; outcome=0 ;;
    create_custom_key)
      project payments '"node"'
      catalog create_dsn '{"organizationSlug":"lumen-labs","projectSlug":"notification-service","name":"Production SDK"}' \
        '[{"name":"dsn","pointer":"/structuredContent/dsn/dsn"}]'
      reward=1; outcome=0 ;;
    create_wrong_team) project payments-infra '"node"'; dsn "$SLUG" ;;
    create_extra_team|create_team_restored)
      project payments '"node"'
      catalog add_team_to_project '{"organizationSlug":"lumen-labs","projectSlug":"notification-service","teamSlug":"payments-infra"}'
      if [ "$variant" = create_team_restored ]; then
        catalog remove_team_from_project '{"organizationSlug":"lumen-labs","projectSlug":"notification-service","teamSlug":"payments-infra"}'
      fi
      dsn "$SLUG" ;;
    create_wrong_key) project payments '"node"'; dsn "$SLUG"; answer_mode=wrong_key ;;
    create_wrong_project) project payments '"node"'; dsn checkout-api ;;
    create_wrong_origin) project payments '"node"'; dsn "$SLUG"; answer_mode=wrong_origin ;;
    create_inactive)
      project payments '"node"'
      dsn "$SLUG"
      catalog update_dsn '{"organizationSlug":"lumen-labs","projectSlug":"notification-service","keyId":"${key_id}","isActive":false}' '[]' 1 ;;
    create_other_key)
      project payments '"node"'
      catalog create_dsn '{"organizationSlug":"lumen-labs","projectSlug":"checkout-api","name":"Unrequested"}'
      dsn "$SLUG" ;;
    create_unreviewed)
      project payments '"node"'; dsn "$SLUG"; answer_mode=unreviewed
      outcome=6; adjudication=1 ;;
    create_contradictory) project payments '"node"'; dsn "$SLUG"; answer_mode=contradictory ;;
    worker_reference)
      add search_issues '{"organizationSlug":"lumen-labs","projectSlugOrId":"payments-worker","query":"is:unresolved \"could not obtain connection from pool\""}'
      visible PAYMENTS-WORKER-1K
      worker; reward=1; outcome=0 ;;
    worker_retry) worker; worker; worker; reward=1; outcome=0 ;;
    worker_next_release) update PAYMENTS-WORKER-1K '{"status":"resolvedInNextRelease"}' ;;
    worker_assignee) update PAYMENTS-WORKER-1K '{"status":"resolved","assignedTo":"team:4507214"}' ;;
    worker_other_issue)
      update CHECKOUT-API-3F '{"status":"resolved"}'; worker ;;
    worker_reason) update PAYMENTS-WORKER-1K '{"status":"resolved","reason":"The connection-pool fix is deployed."}' ;;
    storefront_reference)
      add search_issues '{"organizationSlug":"lumen-labs","projectSlugOrId":"storefront-web","query":"is:unresolved firstRelease:storefront@5.3.0 TypeError"}'
      visible STOREFRONT-WEB-8D
      storefront; reward=1; outcome=0 ;;
    storefront_split)
      update STOREFRONT-WEB-8D '{"assignedTo":"team:4507214"}'
      update STOREFRONT-WEB-8D '{"status":"resolvedInNextRelease"}'
      reward=1; outcome=0 ;;
    storefront_reverse)
      update STOREFRONT-WEB-8D '{"status":"resolvedInNextRelease"}'
      update STOREFRONT-WEB-8D '{"assignedTo":"team:4507214"}'
      reward=1; outcome=0 ;;
    storefront_wrong_team) update STOREFRONT-WEB-8D '{"status":"resolvedInNextRelease","assignedTo":"team:4507223"}' ;;
    storefront_immediate) update STOREFRONT-WEB-8D '{"status":"resolved","assignedTo":"team:4507214"}' ;;
    storefront_other_issue)
      update STOREFRONT-WEB-7Q '{"status":"resolvedInNextRelease","assignedTo":"team:4507214"}'
      storefront ;;
    *) printf 'unsupported Sentry variant: %s\n' "$variant" >&2; exit 2 ;;
  esac
  case "$variant" in
    create_*)
      case "$answer_mode" in
        wrong_key) expression='.dsn|sub("://[^@]+@";"://00000000000000000000000000000000@")' ;;
        wrong_origin) expression='.dsn|sub("@[^/]+/";"@other.invalid/")' ;;
        unreviewed) expression='"Unfamiliar but potentially correct answer wording: "+.dsn' ;;
        contradictory) expression='"No project was created. DSN: "+.dsn' ;;
        *) expression='.dsn' ;;
      esac
      printf 'jq -j '\''%s'\'' "$WORK/bindings.json" > "$WORK/answer.md"\n' \
        "$expression" >> "$TASK/solution/solve.sh"
      printf '%s\n' 'cp "$WORK/answer.md" /app/answer.md' >> "$TASK/solution/solve.sh"
      ;;
  esac
  cp -R "$TASK/solution" "$OUT/program-$variant"
  sh deploy/run-trial.sh -p "$TASK" -a oracle -o "$OUT/$variant" > "$OUT/$variant.log" 2>&1
  result="$(find "$OUT/$variant" -path '*/verifier/reward.json' -print -quit)"
  test -n "$result"
  jq -e --argjson reward "$reward" --argjson outcome "$outcome" --argjson adjudication "$adjudication" '
    .reward==$reward and .outcome_code==$outcome and .adjudication_needed==$adjudication
      and .evaluator_version==30 and .environment_ok==1 and .contract_ok==1 and .integrity_ok==1
      and .runner_admission_checked==1 and .runner_admission_ok==1
      and .preservation_checked==1 and .history_checked==1' "$result" >/dev/null || {
        printf '%s\n' "$variant: unexpected assessment; inspect $result" >&2
        exit 1
      }
  journal="$(dirname "$result")/journal.json"
  jq -e '[.entries[]|select(.service=="sentry")] as $requests
    | ($requests|length)>0 and all($requests[];
      .status>=200 and .status<300 and .scheme=="https" and .host=="sentry.lumenlabs.dev")' \
    "$journal" >/dev/null
  assessment="$(dirname "$result")/assessment.json"
  case "$variant" in
    create_wrong_key|create_wrong_project|create_wrong_origin|create_inactive|create_contradictory)
      predicate='.answer_ok==false and .preservation_ok and .history_ok' ;;
    create_unreviewed)
      predicate='.outcome=="unresolved" and .records_ok and .relations_ok and .preservation_ok
        and .history_ok and any(.failures[];.check=="answer" and .disposition=="unresolved")' ;;
    create_team_restored|worker_assignee|worker_other_issue|worker_reason|storefront_other_issue)
      predicate='.records_ok and .preservation_ok==false and .history_ok==false' ;;
    create_wrong_team|create_extra_team|storefront_immediate)
      predicate='.records_ok==false and .preservation_ok==false' ;;
    storefront_wrong_team) predicate='.relations_ok==false and .preservation_ok==false' ;;
    worker_next_release) predicate='.records_ok==false' ;;
    create_other_key) predicate='.relations_ok==false and .preservation_ok==false' ;;
    *) predicate='.outcome=="pass"' ;;
  esac
  jq -e "$predicate" "$assessment" >/dev/null
  printf '%s\n' "$variant: native MCP result, HTTPS identity and business/history disposition checked"
done
if [ -d "$OUT/create_team_restored" ] && [ -d "$OUT/create_reference" ]; then
  restored="$(find "$OUT/create_team_restored" -path '*/verifier/state.json' -print -quit)"
  reference="$(find "$OUT/create_reference" -path '*/verifier/state.json' -print -quit)"
  jq -e --slurpfile reference "$reference" '.services==$reference[0].services' "$restored" >/dev/null
  printf '%s\n' 'restored extra-team access: identical final business state retains the committed violation'
fi
