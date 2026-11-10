#!/bin/sh
set -eu
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
WORLD_IMAGE="${WORLD_IMAGE:?provide the revision-32 Argo world image}"
TOOLS_IMAGE="${TOOLS_IMAGE:?provide the composed Argo tools image}"
OUT="$(mktemp -d /tmp/tool-bench-argocd.XXXXXX)"
printf 'artifacts: %s\n' "$OUT"
cargo run -q --locked -p scenario --bin taskgen -- --out "$OUT/tasks" \
  --world-image "$WORLD_IMAGE" --tools-image "$TOOLS_IMAGE" >/dev/null
for slug in restart-indexer-worker roll-back-billing-worker sync-prod-payments-api; do
  mkdir "$OUT/$slug-original"
  cp "$OUT/tasks/code__argocd__$slug/solution/"* "$OUT/$slug-original/"
done
add() {
  jq -n --arg tool "$1" --argjson arguments "$2" --argjson replacements "${5:-[]}" \
    '{jsonrpc:"2.0",id:2,method:"tools/call",params:{name:$tool,arguments:$arguments}}
      + if ($replacements|length)>0 then {argument_replacements:$replacements} else {} end' \
    > "$TASK/solution/control-$calls.json"
  printf '%s\n' "${3:-[]}" > "$TASK/solution/control-$calls.capture.json"
  printf 'bound_call 7108 "$DIR/control-%s.json" "$DIR/control-%s.capture.json" %s\n' \
    "$calls" "$calls" "${4:-0}" >> "$TASK/solution/solve.sh"
  calls=$((calls + 1))
}
observe() {
  add get_application "$(jq -cn --arg app "$1" --arg namespace "$2" \
    '{applicationName:$app,applicationNamespace:$namespace}')"
  add get_application "$(jq -cn --arg app "$1" --arg namespace "$2" \
    '{applicationName:$app,applicationNamespace:$namespace}')"
}
action() {
  binding="action_uid_$calls"
  add get_application_resource_tree "$(jq -cn --arg app "$1" \
    '{applicationName:$app,applicationNamespace:"argocd"}')" \
    "$(jq -cn --arg binding "$binding" --arg name "$2" --arg namespace "$3" \
      '[{name:$binding,pointer:"/content/0/text",json_pointer:"/nodes",
        select:{matching:{group:"apps",kind:"Deployment",namespace:$namespace,name:$name},pointer:"/uid"}}]')"
  add run_resource_action "$(jq -cn --arg app "$1" --arg name "$2" --arg namespace "$3" \
    --arg action "$4" --arg binding "$binding" \
    '{applicationName:$app,applicationNamespace:"argocd",
      resourceRef:{uid:("${"+$binding+"}"),group:"apps",version:"v1",kind:"Deployment",
        namespace:$namespace,name:$name},action:$action}')" '[]' 1
}
restart() {
  action search-indexer search-indexer-worker search restart
  add get_application_resource_tree '{"applicationName":"search-indexer","applicationNamespace":"argocd"}'
}
pin() {
  binding="spec_$calls"
  add get_application '{"applicationName":"billing-worker","applicationNamespace":"argocd"}' \
    "$(jq -cn --arg binding "$binding" \
      '[{name:$binding,pointer:"/content/0/text",json_pointer:"/spec"}]')"
  if [ "${3:-spec}" = legacy ]; then
    arguments="$(jq -cn --arg binding "$binding" \
      '{applicationName:"billing-worker",application:{
        metadata:{name:"billing-worker",namespace:"argocd"},spec:("${"+$binding+"}")}}')"
    pointer=/application/spec/source/targetRevision
    tool=update_application
  else
    arguments="$(jq -cn --arg binding "$binding" \
      '{applicationName:"billing-worker",applicationNamespace:"argocd",spec:("${"+$binding+"}")}')"
    pointer=/spec/source/targetRevision
    tool=update_application_spec
  fi
  replacements="$(jq -cn --arg pointer "$pointer" --arg tag "$1" --argjson extra "${2:-[]}" \
    '[{pointer:$pointer,value:$tag}]+$extra')"
  add "$tool" "$arguments" '[]' 1 "$replacements"
  observe billing-worker argocd
}
payments() {
  add sync_application "$1"
  observe payments-api "${2:-payments}"
}
if [ "$#" -eq 0 ]; then
  set -- restart_reference restart_retry restart_api restart_staging restart_paused restart_sync \
    billing_spec billing_legacy billing_manual billing_dry_run billing_disabled billing_sha billing_older billing_other_app \
    payments_reference payments_sha payments_preliminary payments_dry_run payments_no_prune payments_staging payments_delete
fi
for variant do
  reward=0
  outcome=1
  calls=0
  case "$variant" in
    restart_*) slug=restart-indexer-worker ;;
    billing_*) slug=roll-back-billing-worker ;;
    payments_*) slug=sync-prod-payments-api ;;
    *) printf 'unsupported Argo variant: %s\n' "$variant" >&2; exit 2 ;;
  esac
  TASK="$OUT/tasks/code__argocd__$slug"
  rm -rf "$TASK/solution"
  mkdir "$TASK/solution"
  cp "$OUT/$slug-original/"* "$TASK/solution/"
  cp "$OUT/roll-back-billing-worker-original/"*.jq "$TASK/solution/"
  sed -e '/^bound_call [0-9]/,$d' -e '/^call [0-9]/,$d' \
    "$OUT/roll-back-billing-worker-original/solve.sh" > "$TASK/solution/solve.sh"
  case "$variant" in
    restart_reference) restart; reward=1; outcome=0 ;;
    restart_retry) restart; restart; reward=1; outcome=0 ;;
    restart_api) action search-indexer search-indexer-api search restart; restart ;;
    restart_staging) action search-indexer-staging search-indexer-worker search restart; restart ;;
    restart_paused) action search-indexer search-indexer-worker search pause; restart ;;
    restart_sync)
      add sync_application '{"applicationName":"search-indexer","applicationNamespace":"argocd"}'
      observe search-indexer argocd
      restart ;;
    billing_spec) pin v1.42.0; reward=1; outcome=0 ;;
    billing_legacy) pin v1.42.0 '[]' legacy; reward=1; outcome=0 ;;
    billing_manual)
      pin v1.42.0
      add sync_application '{"applicationName":"billing-worker","applicationNamespace":"argocd","revision":"v1.42.0"}'
      observe billing-worker argocd
      reward=1; outcome=0 ;;
    billing_dry_run)
      add sync_application '{"applicationName":"billing-worker","applicationNamespace":"argocd","revision":"v1.42.0","dryRun":true}'
      observe billing-worker argocd
      pin v1.42.0; reward=1; outcome=0 ;;
    billing_disabled)
      pin v1.42.0 '[{"pointer":"/spec/syncPolicy/automated","value":{"enabled":false,"prune":true,"selfHeal":true}}]' ;;
    billing_sha) pin b0826e986c9c73e0088c6ecb2a14458d247d2205 ;;
    billing_older) pin v1.41.0 ;;
    billing_other_app)
      action billing-api billing-api billing restart
      pin v1.42.0 ;;
    payments_reference)
      payments '{"applicationName":"payments-api","applicationNamespace":"payments","prune":true}'
      reward=1; outcome=0 ;;
    payments_sha)
      payments '{"applicationName":"payments-api","applicationNamespace":"payments","revision":"fe01f33d2a5392fa17c1c35f25d294d24075c28b","prune":true}'
      reward=1; outcome=0 ;;
    payments_preliminary)
      payments '{"applicationName":"payments-api","applicationNamespace":"payments"}'
      payments '{"applicationName":"payments-api","applicationNamespace":"payments","prune":true}'
      reward=1; outcome=0 ;;
    payments_dry_run)
      payments '{"applicationName":"payments-api","applicationNamespace":"payments","prune":true,"dryRun":true}' ;;
    payments_no_prune)
      payments '{"applicationName":"payments-api","applicationNamespace":"payments"}' ;;
    payments_staging)
      payments '{"applicationName":"payments-api","applicationNamespace":"payments-staging","prune":true}' payments-staging
      payments '{"applicationName":"payments-api","applicationNamespace":"payments","prune":true}' ;;
    payments_delete)
      payments '{"applicationName":"payments-api","applicationNamespace":"payments","prune":true}'
      add delete_application '{"applicationName":"payments-api","applicationNamespace":"payments","cascade":false}' ;;
    *) printf 'unsupported Argo variant: %s\n' "$variant" >&2; exit 2 ;;
  esac
  cp -R "$TASK/solution" "$OUT/program-$variant"
  sh deploy/run-trial.sh -p "$TASK" -a oracle -o "$OUT/$variant" > "$OUT/$variant.log" 2>&1
  result="$(find "$OUT/$variant" -path '*/verifier/reward.json' -print -quit)"
  test -n "$result"
  jq -e --argjson reward "$reward" --argjson outcome "$outcome" '
    .reward==$reward and .outcome_code==$outcome and .evaluator_version==32
      and .environment_ok==1 and .contract_ok==1 and .integrity_ok==1
      and .runner_admission_checked==1 and .runner_admission_ok==1
      and .preservation_checked==1 and .history_checked==1' "$result" >/dev/null
  printf '%s\n' "$variant: native control reward=$reward outcome=$outcome"
done
