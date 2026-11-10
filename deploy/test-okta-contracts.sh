#!/bin/sh
set -eu
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
WORLD_IMAGE="${WORLD_IMAGE:?provide the revision-33 Okta world image}"
TOOLS_IMAGE="${TOOLS_IMAGE:?provide the declared Okta tools image}"
OUT="$(mktemp -d /tmp/tool-bench-okta.XXXXXX)"
printf 'artifacts: %s\n' "$OUT"
cargo run -q --locked -p scenario --bin taskgen -- --out "$OUT/tasks" \
  --world-image "$WORLD_IMAGE" --tools-image "$TOOLS_IMAGE" >/dev/null
for slug in transfer-to-platform-team offboard-contractor contractor-self-service-reset; do
  mkdir "$OUT/$slug-original"
  cp "$OUT/tasks/cloud__okta__$slug/solution/"* "$OUT/$slug-original/"
done
add() {
  jq -n --arg tool "$1" --argjson arguments "$2" --argjson replacements "${5:-[]}" \
    '{jsonrpc:"2.0",id:2,method:"tools/call",params:{name:$tool,arguments:$arguments}}
      + if ($replacements|length)>0 then {argument_replacements:$replacements} else {} end' \
    > "$TASK/solution/control-$calls.json"
  printf '%s\n' "${3:-[]}" > "$TASK/solution/control-$calls.capture.json"
  printf 'bound_call 7120 "$DIR/control-%s.json" "$DIR/control-%s.capture.json" %s\n' \
    "$calls" "$calls" "${4:-0}" >> "$TASK/solution/solve.sh"
  calls=$((calls + 1))
}
reference() {
  printf '%s\n' 'sh "$DIR/reference-solve.sh"' >> "$TASK/solution/solve.sh"
}
profile() {
  add update_user "$(jq -cn --arg user "${2:-00u1mrgnzlz6Wx3Yz4x7}" \
    --argjson profile "$1" '{user_id:$user,profile:$profile}')"
}
memberships() {
  add add_user_to_group '{"group_id":"00g2pltfrm7Gh8Ij9x7","user_id":"00u1mrgnzlz6Wx3Yz4x7"}'
  add remove_user_from_group '{"group_id":"00g2spprt12Ac5Ef6x7","user_id":"00u1mrgnzlz6Wx3Yz4x7"}'
}
transfer() {
  profile '{"title":"Site Reliability Engineer","department":"Platform Engineering"}' "${1:-00u1mrgnzlz6Wx3Yz4x7}"
}
remove_salesforce() {
  add remove_user_from_group '{"group_id":"00g2cntrsf6Op3Qr4x7","user_id":"00u1alxkmct4St1Uv2x7"}'
}
deactivate() {
  add deactivate_user "$(jq -cn --arg user "${1:-00u1alxkmct4St1Uv2x7}" '{user_id:$user}')"
}
policy() {
  binding="rule_$calls"
  policy_id="${2:-00p4pwcntr2Uv9Wx0x7}"
  rule_id="${3:-0pr5rule0tr2Uv9Wx0x7}"
  add get_policy_rule "$(jq -cn --arg policy "$policy_id" --arg rule "$rule_id" \
    '{policy_id:$policy,rule_id:$rule}')" \
    "$(jq -cn --arg binding "$binding" '[{name:$binding,pointer:"/content/0/text",json_pointer:""}]')"
  add update_policy_rule "$(jq -cn --arg policy "$policy_id" --arg rule "$rule_id" \
    --arg binding "$binding" '{policy_id:$policy,rule_id:$rule,rule_data:("${"+$binding+"}")}')" \
    '[]' 1 "$1"
}
denials='[{"pointer":"/rule_data/actions/selfServicePasswordReset/access","value":"DENY"},{"pointer":"/rule_data/actions/selfServiceUnlock/access","value":"DENY"}]'
if [ "$#" -eq 0 ]; then
  set -- transfer_reference transfer_reverse transfer_login transfer_full transfer_extra_reads transfer_attributes \
    transfer_restored transfer_other_user transfer_extra_group transfer_other_member transfer_profile_only \
    offboard_reference offboard_reverse offboard_login offboard_keep_group offboard_wrong \
    offboard_delete offboard_profile policy_reference policy_split policy_retry \
    policy_scope policy_priority policy_new_policy policy_employee policy_disabled
fi
for variant do
  calls=0
  reward=0
  outcome=1
  case "$variant" in
    transfer_*) slug=transfer-to-platform-team ;;
    offboard_*) slug=offboard-contractor ;;
    policy_*) slug=contractor-self-service-reset ;;
    *) printf 'unsupported Okta control: %s\n' "$variant" >&2; exit 2 ;;
  esac
  TASK="$OUT/tasks/cloud__okta__$slug"
  rm -rf "$TASK/solution"
  mkdir "$TASK/solution"
  cp "$OUT/$slug-original/"* "$TASK/solution/"
  cp "$TASK/solution/solve.sh" "$TASK/solution/reference-solve.sh"
  cp "$OUT/contractor-self-service-reset-original/"*.jq "$TASK/solution/"
  sed -e '/^bound_call [0-9]/,$d' -e '/^call [0-9]/,$d' \
    "$OUT/contractor-self-service-reset-original/solve.sh" > "$TASK/solution/solve.sh"
  case "$variant" in
    transfer_reference|offboard_reference|policy_reference)
      reference; reward=1; outcome=0 ;;
    transfer_reverse)
      memberships; transfer; reward=1; outcome=0 ;;
    transfer_login)
      transfer maria.gonzalez@octo-corp.example
      memberships; reward=1; outcome=0 ;;
    transfer_full)
      full_profile="$(jq -c '.[]|select(.id=="00u1mrgnzlz6Wx3Yz4x7")|.profile
        +{title:"Site Reliability Engineer",department:"Platform Engineering"}' \
        scenarios/cloud/okta/transfer-to-platform-team/dataset/okta/users.json)"
      profile "$full_profile"
      memberships; reward=1; outcome=0 ;;
    transfer_extra_reads)
      add list_brands '{}'
      reference; reward=1; outcome=0 ;;
    transfer_attributes)
      add get_user_profile_attributes '{}'
      reference; reward=1; outcome=0 ;;
    transfer_restored)
      profile '{"displayName":"Temporary alias"}'
      reference
      profile '{"displayName":"Maria Gonzalez"}' ;;
    transfer_other_user)
      profile '{"title":"Unrequested account change"}' 00u1alxkmem2Op9Qr0x7
      reference ;;
    transfer_extra_group)
      reference
      add add_user_to_group '{"group_id":"00g2cntrct4Kl1Mn2x7","user_id":"00u1mrgnzlz6Wx3Yz4x7"}' ;;
    transfer_other_member)
      reference
      add remove_user_from_group '{"group_id":"00g2spprt12Ac5Ef6x7","user_id":"00u1tylrbrk0Ef7Gh8x7"}' ;;
    transfer_profile_only) transfer ;;
    offboard_reverse)
      remove_salesforce; deactivate; reward=1; outcome=0 ;;
    offboard_login)
      deactivate alex.kim@northwind-consulting.example
      remove_salesforce; reward=1; outcome=0 ;;
    offboard_keep_group) deactivate ;;
    offboard_wrong)
      deactivate 00u1alxkmem2Op9Qr0x7
      reference ;;
    offboard_delete)
      add delete_deactivated_user '{"user_id":"00u1alxkmct4St1Uv2x7"}'
      remove_salesforce ;;
    offboard_profile)
      reference
      profile '{"displayName":"Archived generic account"}' 00u1alxkmct4St1Uv2x7 ;;
    policy_split)
      policy '[{"pointer":"/rule_data/actions/selfServicePasswordReset/access","value":"DENY"}]'
      policy '[{"pointer":"/rule_data/actions/selfServiceUnlock/access","value":"DENY"}]'
      reward=1; outcome=0 ;;
    policy_retry)
      reference; reference; reward=1; outcome=0 ;;
    policy_scope)
      replacements="$(jq -cn --argjson denials "$denials" '$denials+[
        {pointer:"/rule_data/conditions",value:{
          network:{connection:"ANYWHERE"},people:{users:{exclude:["00u1alxkmct4St1Uv2x7"]}}}}]')"
      policy "$replacements" ;;
    policy_priority)
      replacements="$(jq -cn --argjson denials "$denials" \
        '$denials+[{pointer:"/rule_data/priority",value:0}]')"
      policy "$replacements" ;;
    policy_new_policy)
      reference
      new_policy="$(jq -c '.[]|select(.id=="00p4pwcntr2Uv9Wx0x7")
        |{name:"Extra contractor policy",type,priority:0,conditions,settings}' \
        scenarios/cloud/okta/contractor-self-service-reset/dataset/okta/policies.json)"
      add create_policy "$(jq -cn --argjson policy "$new_policy" '{policy_data:$policy}')" ;;
    policy_employee)
      policy "$denials" 00p4pwempl4Yz1Ab2x7 0pr5rule1pl4Yz1Ab2x7
      reference ;;
    policy_disabled)
      reference
      add deactivate_policy_rule '{"policy_id":"00p4pwcntr2Uv9Wx0x7","rule_id":"0pr5rule0tr2Uv9Wx0x7"}' ;;
    *) printf 'unsupported Okta control: %s\n' "$variant" >&2; exit 2 ;;
  esac
  cp -R "$TASK/solution" "$OUT/program-$variant"
  sh deploy/run-trial.sh -p "$TASK" -a oracle -o "$OUT/$variant" > "$OUT/$variant.log" 2>&1
  result="$(find "$OUT/$variant" -path '*/verifier/reward.json' -print -quit)"
  test -n "$result"
  jq -e --argjson reward "$reward" --argjson outcome "$outcome" '
    .reward==$reward and .outcome_code==$outcome and .evaluator_version==33
      and .environment_ok==1 and .contract_ok==1 and .integrity_ok==1
      and .runner_admission_checked==1 and .runner_admission_ok==1
      and .preservation_checked==1 and .history_checked==1' "$result" >/dev/null
  printf '%s\n' "$variant: native control reward=$reward outcome=$outcome"
done
