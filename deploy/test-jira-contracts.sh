#!/bin/sh
set -eu
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
WORLD_IMAGE="${WORLD_IMAGE:?provide the current Jira world image}"
TOOLS_IMAGE="${TOOLS_IMAGE:?provide the tools image}"
OUT="$(mktemp -d /tmp/tool-bench-jira.XXXXXX)"
printf 'artifacts: %s\n' "$OUT"
cargo run -q --locked -p scenario --bin taskgen -- --out "$OUT/tasks" \
  --world-image "$WORLD_IMAGE" --tools-image "$TOOLS_IMAGE" >/dev/null
for task in assign-go-no-go-review file-apple-pay-bug ship-refund-retries; do
  mkdir "$OUT/$task-original"
  cp "$OUT/tasks/collab__jira__$task/solution/"*.json "$OUT/$task-original/"
done
for variant in assign_atomic assign_premature assign_archived_owner assign_collateral \
    bug_summary_only bug_short_summary bug_contradiction bug_unreviewed bug_wrong_assignee bug_wrong_label \
    ship_atomic ship_comment_first ship_identifier_only ship_wrong_issue ship_wrong_resolution; do
  reward=0
  outcome=1
  adjudication=0
  case "$variant" in
    assign_*) task=assign-go-no-go-review ;;
    bug_*) task=file-apple-pay-bug ;;
    ship_*) task=ship-refund-retries ;;
  esac
  TASK="$OUT/tasks/collab__jira__$task"
  cp "$OUT/$task-original/"*.json "$TASK/solution/"
  case "$variant" in
    assign_atomic)
      reward=1
      outcome=0
      jq '.params.name = "jira_get_issue" | .params.arguments = {issue_key:"PAY-124"}' \
        "$OUT/$task-original/call-3.json" > "$TASK/solution/call-3.json"
      jq '.params.arguments.fields = "{\"assignee\":{\"name\":\"lena.fischer@octo-corp.example\"}}"' \
        "$OUT/$task-original/call-4.json" > "$TASK/solution/call-4.json"
      ;;
    assign_premature)
      cp "$OUT/$task-original/call-4.json" "$TASK/solution/call-3.json"
      cp "$OUT/$task-original/call-3.json" "$TASK/solution/call-4.json"
      ;;
    assign_archived_owner)
      jq '.params.arguments.assignee = "marcus.webb"' \
        "$OUT/$task-original/call-3.json" > "$TASK/solution/call-3.json"
      ;;
    assign_collateral)
      jq '.params.name = "jira_update_issue" | .params.arguments = {issue_key:"PAY-124", fields:"{\"assignee\":\"lena.fischer\",\"priority\":{\"name\":\"Low\"}}"}' \
        "$OUT/$task-original/call-3.json" > "$TASK/solution/call-3.json"
      ;;
    bug_summary_only)
      reward=1
      outcome=0
      jq '.params.arguments.description = null' \
        "$OUT/$task-original/call-1.json" > "$TASK/solution/call-1.json"
      ;;
    bug_short_summary)
      reward=1
      outcome=0
      jq '.params.arguments.summary = "Missing Apple Pay button" | .params.arguments.description = "At checkout in Safari on iOS 18, the Apple Pay button is missing."' \
        "$OUT/$task-original/call-1.json" > "$TASK/solution/call-1.json"
      ;;
    bug_contradiction)
      outcome=6
      adjudication=1
      jq '.params.arguments.description = "Actually the button works correctly and no bug exists."' \
        "$OUT/$task-original/call-1.json" > "$TASK/solution/call-1.json"
      ;;
    bug_unreviewed)
      outcome=6
      adjudication=1
      jq '.params.arguments.summary = "Apple Pay unavailable" | .params.arguments.description = "Checkout in iOS 18 Safari does not offer the Apple Pay button."' \
        "$OUT/$task-original/call-1.json" > "$TASK/solution/call-1.json"
      ;;
    bug_wrong_assignee)
      jq '.params.arguments.assignee = "alex.kimball"' \
        "$OUT/$task-original/call-1.json" > "$TASK/solution/call-1.json"
      ;;
    bug_wrong_label)
      jq '.params.arguments.additional_fields |= (fromjson | .labels = ["ios18"] | tojson)' \
        "$OUT/$task-original/call-1.json" > "$TASK/solution/call-1.json"
      ;;
    ship_atomic)
      reward=1
      outcome=0
      jq '.params.arguments.comment = "Shipped in release 2026.11."' \
        "$OUT/$task-original/call-2.json" > "$TASK/solution/call-2.json"
      jq '.params.name = "jira_get_issue" | .params.arguments = {issue_key:"PAY-121"}' \
        "$OUT/$task-original/call-3.json" > "$TASK/solution/call-3.json"
      ;;
    ship_comment_first)
      reward=1
      outcome=0
      cp "$OUT/$task-original/call-3.json" "$TASK/solution/call-2.json"
      cp "$OUT/$task-original/call-2.json" "$TASK/solution/call-3.json"
      ;;
    ship_identifier_only)
      outcome=6
      adjudication=1
      jq '.params.arguments.body = "2026.11"' \
        "$OUT/$task-original/call-3.json" > "$TASK/solution/call-3.json"
      ;;
    ship_wrong_issue)
      jq '.params.arguments.issue_key = "PAY-122"' \
        "$OUT/$task-original/call-3.json" > "$TASK/solution/call-3.json"
      ;;
    ship_wrong_resolution)
      jq '.params.arguments.fields = "{\"resolution\":{\"name\":\"Won'\''t Do\"}}"' \
        "$OUT/$task-original/call-2.json" > "$TASK/solution/call-2.json"
      ;;
  esac
  sh deploy/run-trial.sh -p "$TASK" -a oracle -o "$OUT/$variant" > "$OUT/$variant.log" 2>&1
  result="$(find "$OUT/$variant" -path '*/verifier/reward.json' -print -quit)"
  test -n "$result"
  jq -e --argjson reward "$reward" --argjson outcome "$outcome" --argjson adjudication "$adjudication" \
    '.reward == $reward and .outcome_code == $outcome
      and .adjudication_needed == $adjudication
      and .environment_ok == 1 and .contract_ok == 1 and .integrity_ok == 1
      and .preservation_checked == 1 and .history_checked == 1
      and .runner_admission_checked == 1 and .runner_admission_ok == 1' "$result" >/dev/null
  case "$variant" in
    assign_premature) jq -e '.ordering_ok == 0' "$result" >/dev/null ;;
    assign_collateral) jq -e '.preservation_ok == 0' "$result" >/dev/null ;;
  esac
  printf '%s\n' "$variant: Jira outcome, content disposition and admission passed"
done
if cargo run -q --locked -p report --bin bench-report -- \
    "$OUT/bug_unreviewed" "$OUT/bug_short_summary" --allow-agent-difference --json \
    > "$OUT/comparison.json" 2> "$OUT/comparison.log"; then
  printf '%s\n' 'an unreviewed Jira content projection produced a final paired score' >&2
  exit 1
fi
test ! -s "$OUT/comparison.json"
grep -q 'require adjudication' "$OUT/comparison.log"
printf '%s\n' 'unreviewed multi-field prose cannot silently become a model failure or final score'
