#!/bin/sh
set -eu
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
WORLD_IMAGE="${WORLD_IMAGE:?provide the current AWS world image}"
TOOLS_IMAGE="${TOOLS_IMAGE:?provide the tools image}"
OUT="$(mktemp -d /tmp/tool-bench-aws.XXXXXX)"
printf 'artifacts: %s\n' "$OUT"
if [ -n "${REVIEW_POLICY:-}" ]; then
  cargo run -q --locked -p scenario --bin taskgen -- --runner-admission required --out "$OUT/tasks" \
    --world-image "$WORLD_IMAGE" --tools-image "$TOOLS_IMAGE" --review-policy "$REVIEW_POLICY" >/dev/null
else
  cargo run -q --locked -p scenario --bin taskgen -- --runner-admission required --out "$OUT/tasks" \
    --world-image "$WORLD_IMAGE" --tools-image "$TOOLS_IMAGE" >/dev/null
fi
for slug in grant-auditor-access orders-api-cpu-alarm stop-legacy-reporting-worker; do
  mkdir "$OUT/$slug-original"
  cp "$OUT/tasks/cloud__aws__$slug/solution/"* "$OUT/$slug-original/"
done
auditor=priya.raman-audit
policy=arn:aws:iam::aws:policy/SecurityAudit
instance=i-07b9d1f3a5c8e2046
alarm=orders-api-high-cpu
definition='{"AlarmName":"orders-api-high-cpu","Namespace":"AWS/EC2","MetricName":"CPUUtilization","Dimensions":[{"Name":"InstanceId","Value":"i-04c1a7e2b9d35f018"}],"Statistic":"Average","Period":300,"EvaluationPeriods":3,"Threshold":80,"ComparisonOperator":"GreaterThanThreshold","AlarmActions":["arn:aws:sns:us-east-1:481562039417:ops-pager"]}'
metric="$(printf '%s' "$definition" | jq -c '
  del(.Namespace,.MetricName,.Dimensions,.Statistic,.Period)
  | .Metrics=[{Id:"cpu",ReturnData:true,MetricStat:{Metric:{Namespace:"AWS/EC2",
      MetricName:"CPUUtilization",Dimensions:[{Name:"InstanceId",Value:"i-04c1a7e2b9d35f018"}]},
      Period:300,Stat:"Average",Unit:"Percent"}}]')"
alias="$(printf '%s' "$metric" | jq -c '.Metrics[0].ReturnData=false
  | .Metrics += [{Id:"condition",Expression:"cpu",ReturnData:true}]')"
math="$(printf '%s' "$alias" | jq -c '.Metrics[1].Expression="IF(cpu > 80, 100, 0)"')"
add() {
  printf '%s\n' "$1" >> "$commands"
}
grant() {
  add "aws iam attach-user-policy --user-name $auditor --policy-arn $policy"
  add "aws iam add-user-to-group --user-name $auditor --group-name auditors"
}
shutdown() {
  add "aws ec2 stop-instances --instance-ids $instance"
  add "aws ec2 create-tags --resources $instance --tags Key=DecommissionTicket,Value=OPS-4821"
}
put() {
  parameters="$(printf '%s' "$1" | jq -r '
    def option_name:
      gsub("(?<lower>[a-z0-9])(?<upper>[A-Z])"; "\(.lower)-\(.upper)") | ascii_downcase;
    to_entries | map(
      (.key|option_name) as $option
      | if (.value|type)=="boolean" then
          "--" + (if .value then "" else "no-" end) + $option
        else "--" + $option + " " + (.value|tostring|@sh)
        end
    ) | join(" ")')"
  add "aws cloudwatch put-metric-alarm $parameters --region ${2:-us-east-1}"
}
if [ "$#" -eq 0 ]; then
  set -- iam_reference iam_reverse_retry iam_missing_group iam_wrong_principal iam_extra_policy \
    iam_inline iam_member_restored iam_existing_restored iam_tags \
    alarm_direct alarm_metric alarm_alias alarm_read_filter alarm_retry alarm_percent \
    alarm_disabled alarm_two_of_three alarm_wrong_unit alarm_wrong_instance alarm_foreign_account \
    alarm_wrong_period alarm_wrong_comparison alarm_wrong_threshold alarm_collateral \
    alarm_restored alarm_extra alarm_west_only alarm_regions alarm_manual \
    alarm_math alarm_output_default alarm_math_collateral \
    stop_reference stop_reverse_retry stop_wrong_target stop_extra stop_instance_type \
    stop_volume_tag stop_restart stop_tag_restored stop_existing_tag
fi
for variant do
  reward=0
  outcome=1
  adjudication=0
  case "$variant" in
    iam_*) slug=grant-auditor-access ;;
    alarm_*) slug=orders-api-cpu-alarm ;;
    stop_*) slug=stop-legacy-reporting-worker ;;
    *) printf 'unsupported AWS variant: %s\n' "$variant" >&2; exit 2 ;;
  esac
  commands="$OUT/$variant.commands"
  : > "$commands"
  case "$variant" in
    iam_reference) grant; reward=1; outcome=0 ;;
    iam_reverse_retry)
      add "aws iam add-user-to-group --user-name $auditor --group-name auditors"
      grant; grant; reward=1; outcome=0 ;;
    iam_missing_group)
      add "aws iam attach-user-policy --user-name $auditor --policy-arn $policy" ;;
    iam_wrong_principal)
      add "aws iam attach-user-policy --user-name praman --policy-arn $policy"
      add 'aws iam add-user-to-group --user-name praman --group-name auditors' ;;
    iam_extra_policy)
      grant
      add 'aws iam attach-user-policy --user-name jchen --policy-arn arn:aws:iam::aws:policy/AdministratorAccess' ;;
    iam_inline)
      grant
      add "aws iam put-user-policy --user-name $auditor --policy-name extra-admin --policy-document '{\"Version\":\"2012-10-17\",\"Statement\":[{\"Effect\":\"Allow\",\"Action\":\"*\",\"Resource\":\"*\"}]}'" ;;
    iam_member_restored)
      add 'aws iam add-user-to-group --user-name praman --group-name admins'
      add 'aws iam remove-user-from-group --user-name praman --group-name admins'
      grant ;;
    iam_existing_restored)
      add 'aws iam remove-user-from-group --user-name lnovak --group-name auditors'
      add 'aws iam add-user-to-group --user-name lnovak --group-name auditors'
      grant ;;
    iam_tags)
      grant
      add "aws iam tag-user --user-name $auditor --tags Key=company,Value=Other" ;;
    alarm_direct) put "$definition"; reward=1; outcome=0 ;;
    alarm_metric) put "$metric"; reward=1; outcome=0 ;;
    alarm_alias) put "$alias"; reward=1; outcome=0 ;;
    alarm_read_filter)
      add 'aws cloudwatch describe-alarms --state-value ALARM'
      put "$definition"; reward=1; outcome=0 ;;
    alarm_retry)
      put "$definition"; put "$metric"; put "$definition"; reward=1; outcome=0 ;;
    alarm_percent)
      put "$(printf '%s' "$definition" | jq -c '.Unit="Percent" | .DatapointsToAlarm=3 | .ActionsEnabled=true')"
      reward=1; outcome=0 ;;
    alarm_disabled) put "$(printf '%s' "$definition" | jq -c '.ActionsEnabled=false')" ;;
    alarm_two_of_three) put "$(printf '%s' "$definition" | jq -c '.DatapointsToAlarm=2')" ;;
    alarm_wrong_unit) put "$(printf '%s' "$definition" | jq -c '.Unit="Count"')" ;;
    alarm_wrong_instance)
      put "$(printf '%s' "$metric" | jq -c '.Metrics[0].MetricStat.Metric.Dimensions[0].Value="i-0e4a6c8b2d1f35790"')" ;;
    alarm_foreign_account) put "$(printf '%s' "$metric" | jq -c '.Metrics[0].AccountId="999999999999"')" ;;
    alarm_wrong_period) put "$(printf '%s' "$definition" | jq -c '.Period=60')" ;;
    alarm_wrong_comparison) put "$(printf '%s' "$definition" | jq -c '.ComparisonOperator="GreaterThanOrEqualToThreshold"')" ;;
    alarm_wrong_threshold) put "$(printf '%s' "$definition" | jq -c '.Threshold=80.5')" ;;
    alarm_collateral)
      add 'aws cloudwatch disable-alarm-actions --alarm-names orders-api-status-check'
      put "$definition" ;;
    alarm_restored)
      add 'aws cloudwatch disable-alarm-actions --alarm-names orders-api-status-check'
      add 'aws cloudwatch enable-alarm-actions --alarm-names orders-api-status-check'
      put "$definition" ;;
    alarm_extra)
      put "$(printf '%s' "$definition" | jq -c '.AlarmName="unrequested-extra-alarm"')"
      put "$definition" ;;
    alarm_west_only)
      put "$(printf '%s' "$definition" | jq -c '.AlarmActions=[]')" us-west-2 ;;
    alarm_regions)
      put "$definition"
      put "$(printf '%s' "$definition" | jq -c '.Threshold=90 | .AlarmActions=[]')" us-west-2
      add "aws cloudwatch describe-alarms --alarm-names $alarm --region us-east-1"
      add "aws cloudwatch describe-alarms --alarm-names $alarm --region us-west-2" ;;
    alarm_manual)
      put "$definition"
      add "aws cloudwatch set-alarm-state --alarm-name $alarm --state-value ALARM --state-reason ''" ;;
    alarm_math) put "$math"; outcome=6; adjudication=1 ;;
    alarm_output_default)
      put "$(printf '%s' "$metric" | jq -c 'del(.Metrics[0].ReturnData)')"
      outcome=6; adjudication=1 ;;
    alarm_math_collateral)
      put "$math"
      add 'aws cloudwatch disable-alarm-actions --alarm-names orders-api-status-check'
      adjudication=1 ;;
    stop_reference) shutdown; reward=1; outcome=0 ;;
    stop_reverse_retry)
      add "aws ec2 create-tags --resources $instance --tags Key=DecommissionTicket,Value=OPS-4821"
      shutdown; shutdown; reward=1; outcome=0 ;;
    stop_wrong_target)
      add 'aws ec2 stop-instances --instance-ids i-0e4a6c8b2d1f35790'
      shutdown ;;
    stop_extra)
      add 'aws ec2 stop-instances --instance-ids i-0a3f5c7e9b1d24680'
      shutdown ;;
    stop_instance_type)
      shutdown
      add "aws ec2 modify-instance-attribute --instance-id $instance --instance-type Value=t3.small" ;;
    stop_volume_tag)
      add 'aws ec2 create-tags --resources vol-07c9e1a3b5d7f2046 --tags Key=DeleteAfter,Value=2026-03-14'
      shutdown ;;
    stop_restart)
      shutdown
      add "aws ec2 start-instances --instance-ids $instance"
      add "aws ec2 stop-instances --instance-ids $instance" ;;
    stop_tag_restored)
      add "aws ec2 create-tags --resources $instance --tags Key=DecommissionTicket,Value=WRONG"
      shutdown ;;
    stop_existing_tag)
      add "aws ec2 create-tags --resources $instance --tags Key=Team,Value=other-team"
      shutdown ;;
    *) printf 'unsupported AWS variant: %s\n' "$variant" >&2; exit 2 ;;
  esac
  TASK="$OUT/tasks/cloud__aws__$slug"
  ORIGINAL="$OUT/$slug-original"
  rm -rf "$TASK/solution"
  mkdir "$TASK/solution"
  cp "$ORIGINAL/"* "$TASK/solution/"
  sed '/^call [0-9]/,$d' "$ORIGINAL/solve.sh" > "$TASK/solution/solve.sh"
  printf '%s\n' 'call 7116 "$DIR/call-0.json"' >> "$TASK/solution/solve.sh"
  jq -n --rawfile commands "$commands" \
    '{jsonrpc:"2.0",id:2,method:"tools/call",params:{name:"call_aws",
      arguments:{cli_command:($commands|split("\n")|map(select(length>0)))}}}' \
    > "$TASK/solution/call-0.json"
  cp -R "$TASK/solution" "$OUT/program-$variant"
  sh deploy/run-trial.sh -p "$TASK" -a oracle -o "$OUT/$variant" > "$OUT/$variant.log" 2>&1
  result="$(find "$OUT/$variant" -path '*/verifier/reward.json' -print -quit)"
  test -n "$result"
  jq -e --argjson reward "$reward" --argjson outcome "$outcome" --argjson adjudication "$adjudication" \
    '.reward == $reward and .outcome_code == $outcome and .adjudication_needed == $adjudication
      and .evaluator_version == 27 and .environment_ok == 1 and .contract_ok == 1 and .integrity_ok == 1
      and .runner_admission_checked == 1 and .runner_admission_ok == 1
      and .preservation_checked == 1 and .history_checked == 1' "$result" >/dev/null
  jq -e '[.entries[] | select(.service=="aws")] as $calls
    | ($calls|length)>0 and all($calls[]; .status==200
      and ((.response_body|tostring|contains("<Error>"))|not))' \
    "$(dirname "$result")/journal.json" >/dev/null
  printf '%s\n' "$variant: actual SDK transport, scoped AWS outcomes and committed preservation passed"
done
for pair in 'iam_member_restored iam_reference' 'iam_existing_restored iam_reference' \
    'alarm_restored alarm_direct' 'stop_tag_restored stop_reference'; do
  set -- $pair
  if [ -d "$OUT/$1" ] && [ -d "$OUT/$2" ]; then
    restored="$(find "$OUT/$1" -path '*/verifier/state.json' -print -quit)"
    reference="$(find "$OUT/$2" -path '*/verifier/state.json' -print -quit)"
    jq -e --slurpfile reference "$reference" '.services == $reference[0].services' "$restored" >/dev/null
    printf '%s\n' "$1: identical final business state cannot erase earlier unauthorized changes"
  fi
done
