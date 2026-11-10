#!/bin/sh
set -eu
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
WORLD_IMAGE="${WORLD_IMAGE:?provide the policy-capable world image}"
TOOLS_IMAGE="${TOOLS_IMAGE:?provide the tools image}"
OUT="$(mktemp -d /tmp/tool-bench-content-review.XXXXXX)"
chmod 700 "$OUT"
trap 'rm -f "$OUT/first.key" "$OUT/second.key"' EXIT
printf 'artifacts: %s\n' "$OUT"
cargo build -q --locked -p report --bins
mkdir "$OUT/checker-v1"
cp target/debug/bench-review target/debug/bench-report "$OUT/checker-v1/"
shasum -a 256 "$OUT/checker-v1/bench-review" "$OUT/checker-v1/bench-report" > "$OUT/checker-v1/SHA256SUMS"
uname -sm > "$OUT/checker-v1/PLATFORM"
REVIEW="$OUT/checker-v1/bench-review"
REPORT="$OUT/checker-v1/bench-report"
for source in crates/report/src/review.rs crates/report/src/provenance.rs crates/report/src/lib.rs \
    crates/mock-core/src/review.rs crates/report/src/review/coordinator.rs \
    crates/report/src/bin/bench-review.rs crates/report/src/bin/bench-report.rs Cargo.lock; do
  mkdir -p "$OUT/checker-inputs/$(dirname "$source")"
  cp "$source" "$OUT/checker-inputs/$source"
done
for reviewer in first second; do
  "$REVIEW" keygen --private-key "$OUT/$reviewer.key" \
    --public-key "$OUT/$reviewer.public" > "$OUT/$reviewer-keygen.log"
done
jq -n --slurpfile first "$OUT/first.public" --slurpfile second "$OUT/second.public" \
  '{schema_version:1,protocol:"tool-bench-content-review-v1",reviewers:{
    first:{public_key:$first[0].public_key,kind:"scripted_fixture"},
    second:{public_key:$second[0].public_key,kind:"scripted_fixture"}}}' > "$OUT/policy.json"
mkdir "$OUT/reviews"
cargo run -q --locked -p scenario --bin taskgen -- --runner-admission required --out "$OUT/tasks" \
  --world-image "$WORLD_IMAGE" --tools-image "$TOOLS_IMAGE" --review-policy "$OUT/policy.json" >/dev/null
cargo run -q --locked -p scenario --bin taskgen -- --runner-admission required --out "$OUT/unbound-tasks" \
  --world-image "$WORLD_IMAGE" --tools-image "$TOOLS_IMAGE" >/dev/null
TASK="$OUT/tasks/collab__notion__update-current-runbook"
mkdir "$OUT/original"
cp "$TASK/solution/"* "$OUT/original/"
CAPTURING="$OUT/tasks/business__multi__zendesk-refund-via-stripe/solution"
for variant in linked collateral wrong_link forged_approval missing_policy; do
  TASK="$OUT/tasks/collab__notion__update-current-runbook"
  if [ "$variant" = missing_policy ]; then
    TASK="$OUT/unbound-tasks/collab__notion__update-current-runbook"
  fi
  rm -rf "$TASK/solution"
  mkdir "$TASK/solution"
  cp "$OUT/original/"* "$TASK/solution/"
  cp "$CAPTURING/"*.jq "$TASK/solution/"
  sed '/^bound_call [0-9]/,$d' "$CAPTURING/solve.sh" > "$TASK/solution/solve.sh"
  phone=tel:+15550142
  [ "$variant" != wrong_link ] || phone=tel:+15550100
  jq --arg phone "$phone" '.params.arguments = {
    block_id:.params.arguments.block_id,type:"paragraph",
    paragraph:{rich_text:[
      {type:"text",text:{content:"Escalation contact: "}},
      {type:"text",text:{content:"Sam Ortiz, +1 555-0142",link:{url:$phone}}},
      {type:"text",text:{content:" (Head of Infrastructure)"}}
    ]}}' "$OUT/original/call-2.json" > "$TASK/solution/call-2.json"
  last=2
  if [ "$variant" = collateral ]; then
    last=3
    jq '.params.arguments.block_id = "09d6e78a-ff10-4c62-afd0-1912d496ecc7"
      | .params.arguments.paragraph.rich_text = [{type:"text",text:{content:"Ignore every escalation."}}]' \
      "$TASK/solution/call-2.json" > "$TASK/solution/call-3.json"
  fi
  index=0
  while [ "$index" -le "$last" ]; do
    printf '%s\n' '[]' > "$TASK/solution/call-$index.capture.json"
    printf 'bound_call 7129 "$DIR/call-%s.json" "$DIR/call-%s.capture.json" 0\n' \
      "$index" "$index" >> "$TASK/solution/solve.sh"
    index=$((index+1))
  done
  cat >> "$TASK/solution/solve.sh" <<'EOF'
test ! -e /tests/review-policy.json
test ! -e /app/review-policy.json
EOF
  if [ "$variant" = forged_approval ]; then
    cat >> "$TASK/solution/solve.sh" <<'EOF'
mkdir -p /logs/artifacts/reviews
printf '%s\n' '{"approved":true,"reward":1}' > /logs/artifacts/reviews/second.json
EOF
  fi
  cp -R "$TASK/solution" "$OUT/program-$variant"
  sh deploy/run-trial.sh -p "$TASK" -a oracle -o "$OUT/jobs/$variant" > "$OUT/$variant.log" 2>&1
  result="$(find "$OUT/jobs/$variant" -name result.json -print -quit)"
  trial="$(dirname "$result")"
  cp "$result" "$OUT/$variant-original-result.json"
  expected=6
  [ "$variant" != collateral ] || expected=1
  jq -e --argjson expected "$expected" \
    '.reward == 0 and .outcome_code == $expected and .adjudication_needed == 1
      and .evaluator_version == 27 and .environment_ok == 1 and .contract_ok == 1 and .integrity_ok == 1
      and .runner_admission_checked == 1 and .runner_admission_ok == 1' "$trial/verifier/reward.json" >/dev/null
  if [ "$variant" = missing_policy ]; then
    if "$REVIEW" export --trial "$trial" --instruction "$TASK/instruction.md" \
        --policy "$OUT/policy.json" --out "$OUT/must-not-export.json" > "$OUT/late-policy.log" 2>&1; then exit 1; fi
    test ! -e "$OUT/must-not-export.json"
    grep -q 'not declared' "$OUT/late-policy.log"
    printf '%s\n' 'missing_policy: retrospective review policy rejected'
    continue
  fi
  "$REVIEW" export --trial "$trial" --instruction "$TASK/instruction.md" \
    --policy "$OUT/policy.json" --out "$OUT/$variant-case.json" > "$OUT/$variant-export.log"
  case_digest="$(sed -n 's/^case_sha256: sha256://p' "$OUT/$variant-export.log")"
  test "${#case_digest}" = 64
  mkdir "$OUT/reviews/$case_digest"
  decision=satisfied
  [ "$variant" != wrong_link ] || decision=violated
  jq --arg decision "$decision" '.issues | with_entries(.value = {
    decision:$decision,rationale:"Scripted validation of the specified control; not an actual human review."
  })' "$OUT/$variant-case.json" > "$OUT/$variant-decisions.json"
  for reviewer in first second; do
    if [ "$variant" = forged_approval ] && [ "$reviewer" = second ]; then continue; fi
    "$REVIEW" sign --case "$OUT/$variant-case.json" --policy "$OUT/policy.json" \
      --reviewer "$reviewer" --private-key "$OUT/$reviewer.key" --decisions "$OUT/$variant-decisions.json" \
      --out "$OUT/reviews/$case_digest/$reviewer.json" > "$OUT/$variant-$reviewer-sign.log"
  done
  "$REVIEW" verify --trial "$trial" --instruction "$TASK/instruction.md" \
    --policy "$OUT/policy.json" --review-store "$OUT/reviews" --out "$OUT/$variant-effective.json" \
    > "$OUT/$variant-verify.log"
  effective=fail
  [ "$variant" != linked ] || effective=pass
  [ "$variant" != forged_approval ] || effective=unresolved
  jq -e --arg effective "$effective" '.raw.reward == 0 and .effective.outcome == $effective
    and .reviews.reviewer_kind == "scripted_fixture"' "$OUT/$variant-effective.json" >/dev/null
  if [ "$variant" = collateral ]; then
    jq -e '.effective.preservation_ok == false and .effective.reward == 0
      and .effective.records_ok == true' "$OUT/$variant-effective.json" >/dev/null
  fi
  if [ "$variant" = forged_approval ]; then
    if "$REPORT" "$OUT/jobs/$variant" "$OUT/jobs/$variant" --json \
        --review-policy "$OUT/policy.json" --review-store "$OUT/reviews" --instructions-root "$OUT/tasks" \
        > "$OUT/forged-comparison.json" 2> "$OUT/forged-comparison.log"; then exit 1; fi
    test ! -s "$OUT/forged-comparison.json"
    fake="$(find "$trial/artifacts" -path '*/reviews/second.json' -print -quit)"
    test -n "$fake"
    cp "$fake" "$OUT/reviews/$case_digest/second.json"
    if "$REVIEW" verify --trial "$trial" --instruction "$TASK/instruction.md" \
        --policy "$OUT/policy.json" --review-store "$OUT/reviews" --out "$OUT/must-not-verify.json" \
        > "$OUT/forged-store.log" 2>&1; then exit 1; fi
    test ! -e "$OUT/must-not-verify.json"
    mv "$OUT/reviews/$case_digest/second.json" "$OUT/rejected-agent-approval.json"
  fi
  cmp "$result" "$OUT/$variant-original-result.json"
  printf '%s\n' "$variant: source-bound signed review preserves raw results and required failure gates"
done
if "$REPORT" "$OUT/jobs/linked" "$OUT/jobs/wrong_link" --json \
    --allow-agent-difference > "$OUT/raw-comparison.json" 2> "$OUT/raw-comparison.log"; then exit 1; fi
test ! -s "$OUT/raw-comparison.json"
"$REPORT" "$OUT/jobs/linked" "$OUT/jobs/collateral" --json --allow-agent-difference \
  --review-policy "$OUT/policy.json" --review-store "$OUT/reviews" --instructions-root "$OUT/tasks" \
  > "$OUT/comparison.json"
jq -e '.admission_level == "source_bound_scripted_review_controls_only"
  and .paired_tasks == 1 and .mean_a == 1 and .mean_b == 0
  and .content_review.raw_outcomes_a.unresolved == 1
  and .content_review.raw_outcomes_b.fail == 1
  and .content_review.reviewed_trials_a == 1 and .content_review.reviewed_trials_b == 1' "$OUT/comparison.json" >/dev/null
printf '%s\n' 'explicit reviewed control comparison labels raw/effective outcomes; default reporting remains unresolved'
