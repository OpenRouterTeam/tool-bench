#!/bin/sh
set -eu
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
WORLD_IMAGE="${WORLD_IMAGE:?provide the current GitHub world image}"
TOOLS_IMAGE="${TOOLS_IMAGE:?provide the tools image}"
OUT="$(mktemp -d /tmp/tool-bench-github.XXXXXX)"
printf 'artifacts: %s\n' "$OUT"
cargo run -q --locked -p scenario --bin taskgen -- --runner-admission required --out "$OUT/tasks" \
  --world-image "$WORLD_IMAGE" --tools-image "$TOOLS_IMAGE" >/dev/null
for slug in file-login-bug triage-duplicate-charge approve-merge-dependency-bump; do
  mkdir "$OUT/$slug-original"
  cp "$OUT/tasks/code__github__$slug/solution/"* "$OUT/$slug-original/"
done
jq -rj '.[] | select(.number == 57) | .body' \
  scenarios/code/github/approve-merge-dependency-bump/dataset/github/pulls.json > "$OUT/pull-body"
for variant in creation_minimal creation_fragment creation_duplicate creation_wrong_repo creation_assignment \
    triage_split triage_extra_assignee triage_remove_restore triage_substring_label \
    merge_review_text merge_pending_review merge_custom_message merge_wrong_method merge_other_review \
    merge_unreviewed merge_restored_body; do
  reward=0
  outcome=1
  adjudication=0
  case "$variant" in
    creation_*) slug=file-login-bug; last=0 ;;
    triage_*) slug=triage-duplicate-charge; last=2 ;;
    merge_*) slug=approve-merge-dependency-bump; last=1 ;;
  esac
  ORIGINAL="$OUT/$slug-original"
  TASK="$OUT/tasks/code__github__$slug"
  cp "$ORIGINAL/"* "$TASK/solution/"
  sed '/^call [0-9]/,$d' "$ORIGINAL/solve.sh" > "$TASK/solution/solve.sh"
  case "$variant" in
    creation_minimal)
      reward=1; outcome=0
      jq '.params.arguments.body = "Observed after deploy 412."' "$ORIGINAL/call-0.json" > "$TASK/solution/call-0.json"
      ;;
    creation_fragment)
      outcome=6; adjudication=1
      jq '.params.arguments.body = "412"' "$ORIGINAL/call-0.json" > "$TASK/solution/call-0.json"
      ;;
    creation_duplicate)
      last=1
      cp "$ORIGINAL/call-0.json" "$TASK/solution/call-1.json"
      ;;
    creation_wrong_repo)
      jq '.params.arguments.repo = "payments-api"' "$ORIGINAL/call-0.json" > "$TASK/solution/call-0.json"
      ;;
    creation_assignment)
      jq '.params.arguments.assignees = ["sam"]' "$ORIGINAL/call-0.json" > "$TASK/solution/call-0.json"
      ;;
    triage_split)
      reward=1; outcome=0; last=3
      jq 'del(.params.arguments.assignees)' "$ORIGINAL/call-2.json" > "$TASK/solution/call-2.json"
      jq 'del(.params.arguments.labels)' "$ORIGINAL/call-2.json" > "$TASK/solution/call-3.json"
      ;;
    triage_extra_assignee)
      jq '.params.arguments.assignees = ["sam","lin"]' "$ORIGINAL/call-2.json" > "$TASK/solution/call-2.json"
      ;;
    triage_remove_restore)
      last=3
      jq '.params.arguments.labels = ["sev-1"]' "$ORIGINAL/call-2.json" > "$TASK/solution/call-2.json"
      cp "$ORIGINAL/call-2.json" "$TASK/solution/call-3.json"
      ;;
    triage_substring_label)
      jq '.params.arguments.labels = ["bug","customer-reported","sev-10"]' "$ORIGINAL/call-2.json" > "$TASK/solution/call-2.json"
      ;;
    merge_review_text)
      reward=1; outcome=0
      jq '.params.arguments.body = "LGTM."' "$ORIGINAL/call-0.json" > "$TASK/solution/call-0.json"
      ;;
    merge_pending_review)
      reward=1; outcome=0; last=2
      jq 'del(.params.arguments.event)' "$ORIGINAL/call-0.json" > "$TASK/solution/call-0.json"
      jq '.params.arguments.method = "submit_pending"' "$ORIGINAL/call-0.json" > "$TASK/solution/call-1.json"
      cp "$ORIGINAL/call-1.json" "$TASK/solution/call-2.json"
      ;;
    merge_custom_message)
      reward=1; outcome=0
      jq '.params.arguments.commit_title = "Apply the lodash security update"
        | .params.arguments.commit_message = "Bump lodash to 4.17.21."' \
        "$ORIGINAL/call-1.json" > "$TASK/solution/call-1.json"
      ;;
    merge_wrong_method)
      jq '.params.arguments.merge_method = "rebase"' "$ORIGINAL/call-1.json" > "$TASK/solution/call-1.json"
      ;;
    merge_other_review)
      last=2
      jq '.params.arguments.pullNumber = 55' "$ORIGINAL/call-0.json" > "$TASK/solution/call-0.json"
      cp "$ORIGINAL/call-0.json" "$TASK/solution/call-1.json"
      cp "$ORIGINAL/call-1.json" "$TASK/solution/call-2.json"
      ;;
    merge_unreviewed)
      outcome=6; adjudication=1
      jq '.params.arguments.body = "The update looks fine to me."' "$ORIGINAL/call-0.json" > "$TASK/solution/call-0.json"
      ;;
    merge_restored_body)
      last=3
      jq '.params.arguments.body = "LGTM."' "$ORIGINAL/call-0.json" > "$TASK/solution/call-0.json"
      jq -n '{jsonrpc:"2.0",id:2,method:"tools/call",params:{
        name:"update_pull_request",arguments:{owner:"octo-org",repo:"web",pullNumber:57,
        body:"This is an unauthorized replacement of the pull request description."}
      }}' > "$TASK/solution/call-2.json"
      jq --rawfile body "$OUT/pull-body" '.params.arguments.body = $body' \
        "$TASK/solution/call-2.json" > "$TASK/solution/call-3.json"
      ;;
  esac
  index=0
  while [ "$index" -le "$last" ]; do
    printf 'call 7101 "$DIR/call-%s.json"\n' "$index" >> "$TASK/solution/solve.sh"
    index=$((index+1))
  done
  sh deploy/run-trial.sh -p "$TASK" -a oracle -o "$OUT/$variant" > "$OUT/$variant.log" 2>&1
  result="$(find "$OUT/$variant" -path '*/verifier/reward.json' -print -quit)"
  test -n "$result"
  jq -e --argjson reward "$reward" --argjson outcome "$outcome" --argjson adjudication "$adjudication" \
    '.reward == $reward and .outcome_code == $outcome and .adjudication_needed == $adjudication
      and .environment_ok == 1 and .contract_ok == 1 and .integrity_ok == 1
      and .runner_admission_checked == 1 and .runner_admission_ok == 1
      and .preservation_checked == 1 and .history_checked == 1' "$result" >/dev/null
  case "$variant" in
    triage_remove_restore) reference=triage_split ;;
    merge_restored_body) reference=merge_review_text ;;
    *) reference="" ;;
  esac
  if [ -n "$reference" ]; then
    jq -e '.state_ok == 1 and .records_ok == 1 and .relations_ok == 1
      and .history_ok == 0 and .preservation_ok == 0' "$result" >/dev/null
    reference_state="$(find "$OUT/$reference" -path '*/verifier/state.json' -print -quit)"
    jq -e --slurpfile reference "$reference_state" \
      '.services.github == $reference[0].services.github' "$(dirname "$result")/state.json" >/dev/null
  fi
  printf '%s\n' "$variant: GitHub outcome, preservation, ordering, content and admission passed"
done
if cargo run -q --locked -p report --bin bench-report -- --require-attested "$OUT/merge_unreviewed" "$OUT/merge_review_text" \
    --allow-agent-difference --json > "$OUT/comparison.json" 2> "$OUT/comparison.log"; then
  printf '%s\n' 'unreviewed optional review text produced a final paired score' >&2
  exit 1
fi
test ! -s "$OUT/comparison.json"
grep -q 'require adjudication' "$OUT/comparison.log"
printf '%s\n' 'optional review narration cannot bypass adjudication or produce a provisional final score'
