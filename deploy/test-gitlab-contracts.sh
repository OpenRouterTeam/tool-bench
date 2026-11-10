#!/bin/sh
set -eu
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
WORLD_IMAGE="${WORLD_IMAGE:?provide the current GitLab world image}"
TOOLS_IMAGE="${TOOLS_IMAGE:?provide the tools image}"
OUT="$(mktemp -d /tmp/tool-bench-gitlab.XXXXXX)"
printf 'artifacts: %s\n' "$OUT"
cargo run -q --locked -p scenario --bin taskgen -- --out "$OUT/tasks" \
  --world-image "$WORLD_IMAGE" --tools-image "$TOOLS_IMAGE" >/dev/null
for slug in file-circuit-breaker-issue answer-changelog-review-thread approve-and-merge-hotfix; do
  mkdir "$OUT/$slug-original"
  cp "$OUT/tasks/code__gitlab__$slug/solution/"* "$OUT/$slug-original/"
done
CAPTURING="$OUT/tasks/business__multi__zendesk-refund-via-stripe/solution"
make_call() {
  jq -n --arg name "$2" --argjson arguments "$3" \
    '{jsonrpc:"2.0",id:2,method:"tools/call",params:{name:$name,arguments:$arguments}}' \
    > "$TASK/solution/call-$1.json"
}
for variant in merge_squash thread_individual creation_staged thread_minimal creation_extra_label \
    merge_separate_delete merge_message creation_fragment creation_label_suffix creation_new_label \
    creation_wrong_milestone creation_wrong_person thread_fragment thread_early_resolve \
    thread_wrong_subject thread_restored_body merge_missing_removal merge_other_mr \
    merge_preclose merge_reopen creation_duplicate creation_wrong_project; do
  reward=0
  outcome=1
  adjudication=0
  case "$variant" in
    creation_*) slug=file-circuit-breaker-issue; last=0 ;;
    thread_*) slug=answer-changelog-review-thread; last=1 ;;
    merge_*) slug=approve-and-merge-hotfix; last=1 ;;
  esac
  ORIGINAL="$OUT/$slug-original"
  TASK="$OUT/tasks/code__gitlab__$slug"
  rm -rf "$TASK/solution"
  mkdir "$TASK/solution"
  cp "$ORIGINAL/"* "$TASK/solution/"
  cp "$CAPTURING/"*.jq "$TASK/solution/"
  sed '/^bound_call [0-9]/,$d' "$CAPTURING/solve.sh" > "$TASK/solution/solve.sh"
  index=0
  while [ "$index" -le 4 ]; do
    printf '%s\n' '[]' > "$TASK/solution/call-$index.capture.json"
    index=$((index+1))
  done
  case "$variant" in
    merge_squash)
      reward=1; outcome=0; last=3
      jq '.params.arguments.squash = true | .params.arguments.squash_commit_message = "Squash the reviewed refund retry fix"' \
        "$ORIGINAL/call-1.json" > "$TASK/solution/call-1.json"
      printf '%s\n' '[{"name":"actual_merge","pointer":"/content/0/text","json_pointer":"/merge_commit_sha"}]' > "$TASK/solution/call-1.capture.json"
      make_call 2 get_commit '{"project_id":"northwind/payments/ledger","sha":"${actual_merge}"}'
      printf '%s\n' '[{"name":"actual_squash","pointer":"/content/0/text","json_pointer":"/parent_ids/1"}]' > "$TASK/solution/call-2.capture.json"
      make_call 3 get_commit '{"project_id":"northwind/payments/ledger","sha":"${actual_squash}"}'
      ;;
    thread_individual)
      reward=1; outcome=0; last=3
      make_call 0 get_merge_request_discussion '{"project_id":"northwind/billing/billing-service","merge_request_iid":"88","discussion_id":"d7e8c986532452a2290d7e7255e2b2cae36e36ce"}'
      printf '%s\n' '[{"name":"actual_question","pointer":"/content/0/text","json_pointer":"/notes","select":{"matching":{"body":"Did you update CHANGELOG.md for the 4.2 bump? Support reads it before every billing release."},"pointer":"/id"}}]' > "$TASK/solution/call-0.capture.json"
      jq '.params.arguments.body = "The changelog entry was added in commit 24f172e5."' "$ORIGINAL/call-0.json" > "$TASK/solution/call-1.json"
      printf '%s\n' '[{"name":"actual_reply","pointer":"/content/0/text","json_pointer":"/id"}]' > "$TASK/solution/call-1.capture.json"
      make_call 2 update_merge_request_discussion_note '{"project_id":"northwind/billing/billing-service","merge_request_iid":"88","discussion_id":"d7e8c986532452a2290d7e7255e2b2cae36e36ce","note_id":"${actual_reply}","resolved":true}'
      make_call 3 update_merge_request_discussion_note '{"project_id":"northwind/billing/billing-service","merge_request_iid":"88","discussion_id":"d7e8c986532452a2290d7e7255e2b2cae36e36ce","note_id":"${actual_question}","resolved":true}'
      ;;
    creation_staged)
      reward=1; outcome=0; last=1
      jq '.params.arguments.description = "" | .params.arguments.labels = []
        | .params.arguments.milestone_id = "0" | .params.arguments.assignee_ids = []' \
        "$ORIGINAL/call-0.json" > "$TASK/solution/call-0.json"
      printf '%s\n' '[{"name":"actual_issue","pointer":"/content/0/text","json_pointer":"/iid"}]' > "$TASK/solution/call-0.capture.json"
      make_call 1 update_issue '{"project_id":"northwind/payments/checkout-api","issue_iid":"${actual_issue}","description":"Fraud-score latency spikes cascaded into checkout timeouts.","labels":["reliability","postmortem","backend"],"milestone_id":"7104","assignee_ids":[44]}'
      ;;
    thread_minimal)
      reward=1; outcome=0
      jq '.params.arguments.body = "The changelog entry was added in commit 24f172e5."' "$ORIGINAL/call-0.json" > "$TASK/solution/call-0.json"
      ;;
    creation_extra_label)
      reward=1; outcome=0
      jq '.params.arguments.labels = ["reliability","postmortem","backend"]' "$ORIGINAL/call-0.json" > "$TASK/solution/call-0.json"
      ;;
    merge_separate_delete)
      reward=1; outcome=0; last=2
      jq '.params.arguments.should_remove_source_branch = false' "$ORIGINAL/call-1.json" > "$TASK/solution/call-1.json"
      make_call 2 delete_branch '{"project_id":"northwind/payments/ledger","branch_name":"hotfix/double-post-refunds"}'
      ;;
    merge_message)
      reward=1; outcome=0
      jq '.params.arguments.merge_commit_message = "Apply the reviewed refund retry hotfix"' "$ORIGINAL/call-1.json" > "$TASK/solution/call-1.json"
      ;;
    creation_fragment)
      outcome=6; adjudication=1
      jq '.params.arguments.description = "fraud-score"' "$ORIGINAL/call-0.json" > "$TASK/solution/call-0.json"
      ;;
    creation_label_suffix)
      jq '.params.arguments.labels = ["backend","reliability-review"]' "$ORIGINAL/call-0.json" > "$TASK/solution/call-0.json"
      ;;
    creation_new_label)
      jq '.params.arguments.labels += ["invented-new-label"]' "$ORIGINAL/call-0.json" > "$TASK/solution/call-0.json"
      ;;
    creation_wrong_milestone)
      jq '.params.arguments.milestone_id = "7105"' "$ORIGINAL/call-0.json" > "$TASK/solution/call-0.json"
      ;;
    creation_wrong_person)
      jq '.params.arguments.assignee_ids = [45]' "$ORIGINAL/call-0.json" > "$TASK/solution/call-0.json"
      ;;
    thread_fragment)
      outcome=6; adjudication=1
      jq '.params.arguments.body = "24f172e5"' "$ORIGINAL/call-0.json" > "$TASK/solution/call-0.json"
      ;;
    thread_early_resolve)
      cp "$ORIGINAL/call-1.json" "$TASK/solution/call-0.json"
      jq '.params.arguments.body = "The changelog entry was added in commit 24f172e5."' "$ORIGINAL/call-0.json" > "$TASK/solution/call-1.json"
      ;;
    thread_wrong_subject)
      for index in 0 1; do
        jq '.params.arguments.project_id = "northwind/mobile/checkout-ios"
          | .params.arguments.merge_request_iid = "212"
          | .params.arguments.discussion_id = "4ee72f96249b1a5f90e25139fe989442004c070b"' \
          "$ORIGINAL/call-$index.json" > "$TASK/solution/call-$index.json"
      done
      ;;
    thread_restored_body)
      outcome=6; adjudication=1; last=3
      jq '.params.arguments.body = "The changelog entry was added in commit 24f172e5."' "$ORIGINAL/call-0.json" > "$TASK/solution/call-0.json"
      printf '%s\n' '[{"name":"actual_reply","pointer":"/content/0/text","json_pointer":"/id"}]' > "$TASK/solution/call-0.capture.json"
      make_call 2 update_merge_request_note '{"project_id":"northwind/billing/billing-service","merge_request_iid":"88","note_id":"${actual_reply}","body":"The changelog entry was NOT added in commit 24f172e5."}'
      jq '.params.arguments.body = "The changelog entry was added in commit 24f172e5."' \
        "$TASK/solution/call-2.json" > "$TASK/solution/call-3.json"
      ;;
    merge_missing_removal)
      jq '.params.arguments.should_remove_source_branch = false' "$ORIGINAL/call-1.json" > "$TASK/solution/call-1.json"
      ;;
    merge_other_mr)
      last=2
      jq '.params.arguments.merge_request_iid = "42"' "$ORIGINAL/call-0.json" > "$TASK/solution/call-0.json"
      cp "$ORIGINAL/call-0.json" "$TASK/solution/call-1.json"
      cp "$ORIGINAL/call-1.json" "$TASK/solution/call-2.json"
      ;;
    merge_preclose)
      last=2
      make_call 0 update_issue '{"project_id":"northwind/payments/ledger","issue_iid":"118","state_event":"close"}'
      cp "$ORIGINAL/call-0.json" "$TASK/solution/call-1.json"
      cp "$ORIGINAL/call-1.json" "$TASK/solution/call-2.json"
      ;;
    merge_reopen)
      last=2
      make_call 2 update_issue '{"project_id":"northwind/payments/ledger","issue_iid":"118","state_event":"reopen"}'
      ;;
    creation_duplicate)
      last=1
      cp "$ORIGINAL/call-0.json" "$TASK/solution/call-1.json"
      ;;
    creation_wrong_project)
      jq '.params.arguments.project_id = "northwind/mobile/checkout-ios"
        | .params.arguments.milestone_id = "7210" | .params.arguments.labels = ["ios","reliability"]' \
        "$ORIGINAL/call-0.json" > "$TASK/solution/call-0.json"
      ;;
  esac
  index=0
  while [ "$index" -le "$last" ]; do
    template=0
    case "$variant:$index" in
      merge_squash:2|merge_squash:3|thread_individual:2|thread_individual:3|creation_staged:1|thread_restored_body:2|thread_restored_body:3) template=1 ;;
    esac
    printf 'bound_call 7107 "$DIR/call-%s.json" "$DIR/call-%s.capture.json" %s\n' \
      "$index" "$index" "$template" >> "$TASK/solution/solve.sh"
    printf '%s\n' 'jq "with_entries(.value |= if type == \"number\" then tostring else . end)" "$WORK/bindings.json" > "$WORK/next-bindings.json"' \
      'mv "$WORK/next-bindings.json" "$WORK/bindings.json"' >> "$TASK/solution/solve.sh"
    index=$((index+1))
  done
  cp -R "$TASK/solution" "$OUT/program-$variant"
  sh deploy/run-trial.sh -p "$TASK" -a oracle -o "$OUT/$variant" > "$OUT/$variant.log" 2>&1
  result="$(find "$OUT/$variant" -path '*/verifier/reward.json' -print -quit)"
  test -n "$result"
  jq -e --argjson reward "$reward" --argjson outcome "$outcome" --argjson adjudication "$adjudication" \
    '.reward == $reward and .outcome_code == $outcome and .adjudication_needed == $adjudication
      and .evaluator_version == 27 and .environment_ok == 1 and .contract_ok == 1 and .integrity_ok == 1
      and .runner_admission_checked == 1 and .runner_admission_ok == 1
      and .preservation_checked == 1 and .history_checked == 1' "$result" >/dev/null
  case "$variant" in
    merge_squash)
      jq -e '
        [.entries[] | select(.service == "gitlab" and .method == "PUT"
          and (.path | endswith("/merge_requests/41/merge")))] as $merges
        | ($merges | length) == 1
          and any(.entries[]; .service == "gitlab" and .method == "GET"
            and (.path | endswith("/repository/commits/" + $merges[0].response_body.squash_commit_sha)))
      ' "$(dirname "$result")/journal.json" >/dev/null
      ;;
    thread_early_resolve|merge_preclose)
      jq -e '.state_ok == 1 and .records_ok == 1 and .relations_ok == 1 and .ordering_ok == 0' "$result" >/dev/null
      ;;
    thread_restored_body)
      reference="$(find "$OUT/thread_minimal" -path '*/verifier/state.json' -print -quit)"
      jq -e --slurpfile reference "$reference" \
        '.services.gitlab == $reference[0].services.gitlab' "$(dirname "$result")/state.json" >/dev/null
      ;;
  esac
  printf '%s\n' "$variant: GitLab outcome, identity, content, causal and preservation admission passed"
done
if cargo run -q --locked -p report --bin bench-report -- "$OUT/thread_restored_body" "$OUT/thread_minimal" \
    --allow-agent-difference --json > "$OUT/comparison.json" 2> "$OUT/comparison.log"; then
  printf '%s\n' 'restored GitLab reply produced a final paired score' >&2
  exit 1
fi
test ! -s "$OUT/comparison.json"
grep -q 'require adjudication' "$OUT/comparison.log"
printf '%s\n' 'restored GitLab communication remains unresolved and cannot publish a provisional final comparison'
