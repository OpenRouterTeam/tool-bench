#!/bin/sh
set -eu
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
WORLD_IMAGE="${WORLD_IMAGE:?provide the revision-34 Terraform world image}"
TOOLS_IMAGE="${TOOLS_IMAGE:?provide the declared Terraform tools image}"
OUT="$(mktemp -d /tmp/tool-bench-terraform.XXXXXX)"
printf 'artifacts: %s\n' "$OUT"
cargo run -q --locked -p scenario --bin taskgen -- --runner-admission required --out "$OUT/tasks" \
  --world-image "$WORLD_IMAGE" --tools-image "$TOOLS_IMAGE" >/dev/null
for slug in approve-nat-gateway-change scale-checkout-staging unlock-orders-prod; do
  mkdir "$OUT/$slug-original"
  cp "$OUT/tasks/code__terraform__$slug/solution/"* "$OUT/$slug-original/"
done
add() {
  jq -n --arg tool "$1" --argjson arguments "$2" \
    '{jsonrpc:"2.0",id:2,method:"tools/call",params:{name:$tool,arguments:$arguments}}' \
    > "$TASK/solution/control-$calls.json"
  printf '%s\n' "${3:-[]}" > "$TASK/solution/control-$calls.capture.json"
  printf 'bound_call 7109 "$DIR/control-%s.json" "$DIR/control-%s.capture.json" %s\n' \
    "$calls" "$calls" "${4:-0}" >> "$TASK/solution/solve.sh"
  calls=$((calls + 1))
}
reference() {
  printf '%s\n' 'sh "$DIR/reference-solve.sh"' >> "$TASK/solution/solve.sh"
}
variable() {
  add update_workspace_variable "$(jq -cn --arg value "$1" --arg variable "${2:-var-4eRpJG8P0JYsXLTJ}" \
    --arg key "${3:-desired_count}" --arg workspace "${4:-checkout-service-staging}" \
    '{terraform_org_name:"acme-platform",workspace_name:$workspace,variable_id:$variable,key:$key,value:$value}')"
}
queue() {
  binding="created_run_$calls"
  add create_run "$(jq -cn --arg workspace "$1" --arg mode "$2" --arg message "$3" \
    --arg org "${4:-acme-platform}" \
    '{terraform_org_name:$org,workspace_name:$workspace,run_type:$mode,message:$message}')" \
    "$(jq -cn --arg binding "$binding" \
      '[{name:$binding,pointer:"/content/0/text",json_pointer:"/data/id"}]')"
  add get_run_details "$(jq -cn --arg binding "$binding" '{run_id:("${"+$binding+"}")}')" '[]' 1
}
approve() {
  add action_run "$(jq -cn --arg comment "$1" --arg run "${2:-run-1q2oy8I2MqSd5oM4}" \
    '{run_id:$run,run_action:"apply",comment:$comment}')"
  add get_run_details "$(jq -cn --arg run "${2:-run-1q2oy8I2MqSd5oM4}" '{run_id:$run}')"
}
unlock() {
  add force_unlock_workspace "$(jq -cn --arg workspace "${1:-ws-gOHPQz7D1ZRgqBJq}" '{workspace_id:$workspace}')"
}
if [ "$#" -eq 0 ]; then
  set -- approve_reference approve_extra_reads approve_wrong_comment approve_staging approve_unrelated_cancel \
    scale_reference scale_equivalent scale_retry_variable scale_extra_reads scale_wrong_message scale_destroy \
    scale_speculative scale_refresh scale_auto_apply scale_restored scale_count_restored scale_prod \
    unlock_reference unlock_cancel unlock_extra_reads unlock_no_plan unlock_legacy unlock_legacy_edit unlock_normal
fi
for variant do
  calls=0
  reward=0
  outcome=1
  case "$variant" in
    approve_*) slug=approve-nat-gateway-change ;;
    scale_*) slug=scale-checkout-staging ;;
    unlock_*) slug=unlock-orders-prod ;;
    *) printf 'unsupported Terraform control: %s\n' "$variant" >&2; exit 2 ;;
  esac
  TASK="$OUT/tasks/code__terraform__$slug"
  rm -rf "$TASK/solution"
  mkdir "$TASK/solution"
  cp "$OUT/$slug-original/"* "$TASK/solution/"
  cp "$TASK/solution/solve.sh" "$TASK/solution/reference-solve.sh"
  cp "$OUT/scale-checkout-staging-original/"*.jq "$TASK/solution/"
  sed -e '/^bound_call [0-9]/,$d' -e '/^call [0-9]/,$d' \
    "$OUT/scale-checkout-staging-original/solve.sh" > "$TASK/solution/solve.sh"
  case "$variant" in
    approve_reference|scale_reference|unlock_reference) reference; reward=1; outcome=0 ;;
    approve_extra_reads)
      add get_plan_details '{"plan_id":"plan-vOqCAeqHb08EsDyk"}'
      reference
      add list_state_versions '{"terraform_org_name":"acme-platform","workspace_name":"network-core-prod"}'
      reward=1; outcome=0 ;;
    approve_wrong_comment) approve "Rejected CHG-2291" ;;
    approve_staging) approve "Approved per CHG-2291" run-blTtGlrNXHe62r7y; reference ;;
    approve_unrelated_cancel)
      add action_run '{"run_id":"run-jCcDubzVP5ob0NM7","run_action":"cancel","comment":"Unrequested recovery"}'
      reference ;;
    scale_equivalent)
      variable "4.000"
      queue checkout-service-staging plan_and_apply "Scale checkout staging to 4 tasks for load test"
      reward=1; outcome=0 ;;
    scale_retry_variable)
      variable 4; reference; reward=1; outcome=0 ;;
    scale_extra_reads)
      add get_workspace_details '{"terraform_org_name":"acme-platform","workspace_name":"checkout-service-prod"}'
      reference; reward=1; outcome=0 ;;
    scale_wrong_message)
      variable 4
      queue checkout-service-staging plan_and_apply "Scale checkout staging to 4 tasks; approval revoked" ;;
    scale_destroy|scale_speculative|scale_refresh|scale_auto_apply)
      case "$variant" in
        scale_destroy) mode=is_destroy ;;
        scale_speculative) mode=plan_only ;;
        scale_refresh) mode=refresh_state ;;
        scale_auto_apply) mode=auto_approve ;;
      esac
      variable 4
      queue checkout-service-staging "$mode" "Scale checkout staging to 4 tasks for load test" ;;
    scale_restored)
      variable 99 var-w6vOLf60kvxWb9hr max_capacity
      variable 6 var-w6vOLf60kvxWb9hr max_capacity
      reference ;;
    scale_count_restored) variable 99; reference ;;
    scale_prod) variable 4 var-FwwYKSi27hH9ISlj desired_count checkout-service-prod; reference ;;
    unlock_cancel)
      add action_run '{"run_id":"run-jCcDubzVP5ob0NM7","run_action":"cancel","comment":"Recover crashed Commerce run"}'
      queue orders-api-prod plan_only "Inspect drift after recovery"
      reward=1; outcome=0 ;;
    unlock_extra_reads)
      add get_workspace_details '{"terraform_org_name":"acme-platform","workspace_name":"orders-api-prod-legacy"}'
      reference; reward=1; outcome=0 ;;
    unlock_no_plan) unlock ;;
    unlock_legacy) unlock ws-QMfGO0wb8mhyWgrq; reference ;;
    unlock_legacy_edit)
      add update_workspace '{"terraform_org_name":"acme-platform","workspace_name":"orders-api-prod-legacy","description":"Unrequested legacy modification"}'
      reference ;;
    unlock_normal) unlock; queue orders-api-prod plan_and_apply "Verify drift" ;;
  esac
  cp -R "$TASK/solution" "$OUT/program-$variant"
  sh deploy/run-trial.sh -p "$TASK" -a oracle -o "$OUT/$variant" > "$OUT/$variant.log" 2>&1
  result="$(find "$OUT/$variant" -path '*/verifier/reward.json' -print -quit)"
  test -n "$result"
  jq -e --argjson reward "$reward" --argjson outcome "$outcome" '
    .reward==$reward and .outcome_code==$outcome and .evaluator_version==34
      and .environment_ok==1 and .contract_ok==1 and .integrity_ok==1
      and .runner_admission_checked==1 and .runner_admission_ok==1
      and .preservation_checked==1 and .history_checked==1' "$result" >/dev/null
  printf '%s\n' "$variant: native control reward=$reward outcome=$outcome"
done
