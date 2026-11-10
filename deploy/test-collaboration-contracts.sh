#!/bin/sh
set -eu
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
WORLD_IMAGE="${WORLD_IMAGE:?provide the current collaboration world image}"
TOOLS_IMAGE="${TOOLS_IMAGE:?provide the tools image}"
OUT="$(mktemp -d /tmp/tool-bench-collaboration.XXXXXX)"
printf 'artifacts: %s\n' "$OUT"
cargo run -q --locked -p scenario --bin taskgen -- --runner-admission required --out "$OUT/tasks" \
  --world-image "$WORLD_IMAGE" --tools-image "$TOOLS_IMAGE" >/dev/null
for task in linear__escalate-right-safari-issue linear__file-safari-crash-bug linear__pull-issue-into-cycle \
    slack__cutover-thread-summary slack__launch-checklist-item slack__release-date-dm-designer; do
  mkdir "$OUT/$task-original"
  cp "$OUT/tasks/collab__$task/solution/"* "$OUT/$task-original/"
done
CAPTURING="$OUT/tasks/business__multi__zendesk-refund-via-stripe/solution"
for variant in linear_refine linear_keyword escalation_alternative escalation_keyword escalation_collateral \
    cycle_combined cycle_not_current dm_minimal dm_denial dm_wrong_person summary_minimal summary_missing_lead \
    summary_wrong_thread checklist_refine checklist_wrong_status; do
  reward=0
  outcome=1
  adjudication=0
  case "$variant" in
    linear_*) task=linear__file-safari-crash-bug ;;
    escalation_*) task=linear__escalate-right-safari-issue ;;
    cycle_*) task=linear__pull-issue-into-cycle ;;
    dm_*) task=slack__release-date-dm-designer ;;
    summary_*) task=slack__cutover-thread-summary ;;
    checklist_*) task=slack__launch-checklist-item ;;
  esac
  TASK="$OUT/tasks/collab__$task"
  cp "$OUT/$task-original/"* "$TASK/solution/"
  case "$variant" in
    linear_keyword)
      outcome=6; adjudication=1
      jq '.params.arguments.description = "Safari"' "$OUT/$task-original/call-2.json" > "$TASK/solution/call-2.json"
      ;;
    escalation_alternative|escalation_keyword)
      body='Acme Retail escalated this Safari checkout issue.'
      reward=1; outcome=0
      if [ "$variant" = escalation_keyword ]; then body='Acme'; reward=0; outcome=6; adjudication=1; fi
      jq --arg body "$body" '.params.arguments.body = $body' "$OUT/$task-original/call-2.json" > "$TASK/solution/call-2.json"
      ;;
    escalation_collateral)
      jq '.params.arguments.estimate = 1' "$OUT/$task-original/call-1.json" > "$TASK/solution/call-1.json"
      ;;
    cycle_combined)
      reward=1; outcome=0
      jq '.params.name = "linear_updateIssue" | .params.arguments = {
        id:"PAY-121",cycleId:"627fb4e1-47ac-567a-99cd-3e236709e3b0",stateId:"41688e9c-05a3-5fce-be65-9c7049e13037"
      }' "$OUT/$task-original/call-1.json" > "$TASK/solution/call-1.json"
      ;;
    cycle_not_current)
      jq '.params.arguments.cycleId = "c70c9185-b7fb-567b-86c1-7aab564bfd93"' \
        "$OUT/$task-original/call-1.json" > "$TASK/solution/call-1.json"
      ;;
    dm_minimal|dm_denial)
      body='The web 4.12 release is scheduled for 2026-11-10.'
      reward=1; outcome=0
      if [ "$variant" = dm_denial ]; then body='The release is not on 2026-11-10.'; reward=0; outcome=6; adjudication=1; fi
      jq --arg body "$body" '.params.arguments.message = $body' "$OUT/$task-original/call-2.json" > "$TASK/solution/call-2.json"
      ;;
    dm_wrong_person)
      jq '.params.arguments.channel_id = "U04ARIVERA9"' "$OUT/$task-original/call-2.json" > "$TASK/solution/call-2.json"
      ;;
    summary_minimal|summary_missing_lead)
      body='Summary: 2026-11-07, 22:00-01:00 CT; Sam is the cutover lead.'
      reward=1; outcome=0
      if [ "$variant" = summary_missing_lead ]; then body='2026-11-07 22:00-01:00 CT'; reward=0; outcome=6; adjudication=1; fi
      jq --arg body "$body" '.params.arguments.message = $body' "$OUT/$task-original/call-2.json" > "$TASK/solution/call-2.json"
      ;;
    summary_wrong_thread)
      jq 'del(.params.arguments.thread_ts)' "$OUT/$task-original/call-2.json" > "$TASK/solution/call-2.json"
      ;;
    checklist_wrong_status)
      jq '.params.arguments.columns.Status = "Done"' "$OUT/$task-original/call-1.json" > "$TASK/solution/call-1.json"
      ;;
    linear_refine|checklist_refine)
      reward=1; outcome=0
      sed '/^bound_call [0-9]/,$d' "$CAPTURING/solve.sh" > "$TASK/solution/solve.sh"
      cp "$CAPTURING/"*.jq "$TASK/solution/"
      if [ "$variant" = linear_refine ]; then
        port=7127; last=2
        jq '.params.arguments.priority = 0 | .params.arguments.labelIds = []' \
          "$OUT/$task-original/call-2.json" > "$TASK/solution/call-2.json"
        printf '%s\n' '[{"name":"actual_issue","pointer":"/content/0/text","json_pointer":"/id"}]' \
          > "$TASK/solution/call-2.capture.json"
        jq -n '{jsonrpc:"2.0",id:2,method:"tools/call",params:{
          name:"linear_updateIssue",arguments:{id:"${actual_issue}",priority:2,
          addedLabelIds:["b528a42a-7d96-511c-ab34-1b8cb0bf0c8d","75dbca3d-6368-502e-8e56-0c82837ea691"]}
        }}' > "$TASK/solution/call-3.json"
      else
        port=7126; last=1
        jq '.params.arguments.columns = {Task:"Rotate webhook signing secrets"}' \
          "$OUT/$task-original/call-1.json" > "$TASK/solution/call-1.json"
        printf '%s\n' '[{"name":"list_reply","pointer":"/content/0/text","json_pointer":"/result"}]' \
          > "$TASK/solution/call-1.capture.json"
        jq -n '{jsonrpc:"2.0",id:2,method:"tools/call",params:{
          name:"slack_update_list_record",arguments:{list_id:"F0PAYV2LST1",record_id:"${actual_record}",
          updated_columns:{Owner:"U03PRIYAN4T",Status:"In progress","Due date":"2026-11-05"}}
        }}' > "$TASK/solution/call-2.json"
      fi
      index=0
      while [ "$index" -le "$last" ]; do
        if [ "$index" -lt "$last" ]; then printf '%s\n' '[]' > "$TASK/solution/call-$index.capture.json"; fi
        printf 'bound_call %s "$DIR/call-%s.json" "$DIR/call-%s.capture.json" 0\n' \
          "$port" "$index" "$index" >> "$TASK/solution/solve.sh"
        index=$((index+1))
      done
      if [ "$variant" = checklist_refine ]; then
        printf '%s\n' 'jq ".actual_record = (.list_reply | capture(\"Record ID: (?<id>Rec[A-Za-z0-9]+)\").id)" "$WORK/bindings.json" > "$WORK/next-bindings.json"' \
          'mv "$WORK/next-bindings.json" "$WORK/bindings.json"' >> "$TASK/solution/solve.sh"
      fi
      next=$((last+1))
      printf '%s\n' '[]' > "$TASK/solution/call-$next.capture.json"
      printf 'bound_call %s "$DIR/call-%s.json" "$DIR/call-%s.capture.json" 1\n' \
        "$port" "$next" "$next" >> "$TASK/solution/solve.sh"
      ;;
  esac
  sh deploy/run-trial.sh -p "$TASK" -a oracle -o "$OUT/$variant" > "$OUT/$variant.log" 2>&1
  result="$(find "$OUT/$variant" -path '*/verifier/reward.json' -print -quit)"
  test -n "$result"
  jq -e --argjson reward "$reward" --argjson outcome "$outcome" --argjson adjudication "$adjudication" \
    '.reward == $reward and .outcome_code == $outcome and .adjudication_needed == $adjudication
      and .environment_ok == 1 and .contract_ok == 1 and .integrity_ok == 1
      and .runner_admission_checked == 1 and .runner_admission_ok == 1
      and .preservation_checked == 1 and .history_checked == 1' "$result" >/dev/null
  printf '%s\n' "$variant: collaboration outcome, preservation, text disposition and admission passed"
done
if cargo run -q --locked -p report --bin bench-report -- --require-attested "$OUT/dm_denial" "$OUT/dm_minimal" \
    --allow-agent-difference --json > "$OUT/comparison.json" 2> "$OUT/comparison.log"; then
  printf '%s\n' 'unreviewed Slack wording produced a final paired score' >&2
  exit 1
fi
test ! -s "$OUT/comparison.json"
grep -q 'require adjudication' "$OUT/comparison.log"
printf '%s\n' 'collaboration text uncertainty cannot silently become a final model score'
