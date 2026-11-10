#!/bin/sh
set -eu
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
OUT="$(mktemp -d /tmp/tool-bench-aws-review.XXXXXX)"
chmod 700 "$OUT"
trap 'rm -f "$OUT/first.key" "$OUT/second.key"' EXIT
printf 'review artifacts: %s\n' "$OUT"
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
REVIEW_POLICY="$OUT/policy.json" sh deploy/test-aws-contracts.sh "$@" > "$OUT/aws.log" 2>&1
cat "$OUT/aws.log"
AWS="$(sed -n 's/^artifacts: //p' "$OUT/aws.log")"
test -d "$AWS"
printf '%s\n' "$AWS" > "$OUT/aws-path"
mkdir "$OUT/reviews"
for variant in alarm_math alarm_output_default alarm_math_collateral; do
  result="$(find "$AWS/$variant" -name result.json -print -quit)"
  test -n "$result"
  trial="$(dirname "$result")"
  instruction="$AWS/tasks/cloud__aws__orders-api-cpu-alarm/instruction.md"
  cp "$result" "$OUT/$variant-original-result.json"
  "$REVIEW" export --trial "$trial" --instruction "$instruction" \
    --policy "$OUT/policy.json" --out "$OUT/$variant-case.json" > "$OUT/$variant-export.log"
  jq -e 'all(.issues[]; .scope.kind=="record_semantics"
    and .scope.phase=="final_semantics"
    and .scope.coverage=="all_applicable_content_and_metric_requirements"
    and .scope.declarations.metric_alarm.metric=="CPUUtilization"
    and (.scope.declarations.final_target_identities|length)==1)' \
    "$OUT/$variant-case.json" >/dev/null
  case_digest="$(sed -n 's/^case_sha256: sha256://p' "$OUT/$variant-export.log")"
  test "${#case_digest}" = 64
  mkdir "$OUT/reviews/$case_digest"
  decision=satisfied
  [ "$variant" != alarm_output_default ] || decision=violated
  jq --arg decision "$decision" '.issues | with_entries(.value = {
    decision:$decision,
    rationale:"Scripted protocol control only: this chosen vote does not establish production metric-math equivalence or human judgment."
  })' "$OUT/$variant-case.json" > "$OUT/$variant-decisions.json"
  for reviewer in first second; do
    "$REVIEW" sign --case "$OUT/$variant-case.json" --policy "$OUT/policy.json" \
      --reviewer "$reviewer" --private-key "$OUT/$reviewer.key" --decisions "$OUT/$variant-decisions.json" \
      --out "$OUT/reviews/$case_digest/$reviewer.json" > "$OUT/$variant-$reviewer-sign.log"
    if [ "$variant" = alarm_math ] && [ "$reviewer" = first ]; then
      "$REVIEW" verify --trial "$trial" --instruction "$instruction" --policy "$OUT/policy.json" \
        --review-store "$OUT/reviews" --out "$OUT/incomplete-effective.json" > "$OUT/incomplete-verify.log"
      jq -e '.effective.outcome=="unresolved" and .effective.adjudication_needed==true' \
        "$OUT/incomplete-effective.json" >/dev/null
    fi
  done
  "$REVIEW" verify --trial "$trial" --instruction "$instruction" --policy "$OUT/policy.json" \
    --review-store "$OUT/reviews" --out "$OUT/$variant-effective.json" > "$OUT/$variant-verify.log"
  expected=fail
  [ "$variant" != alarm_math ] || expected=pass
  jq -e --arg expected "$expected" '.raw.reward==0 and .effective.outcome==$expected
    and .reviews.reviewer_kind=="scripted_fixture" and .effective.adjudication_needed==false' \
    "$OUT/$variant-effective.json" >/dev/null
  if [ "$variant" = alarm_math_collateral ]; then
    jq -e '.raw.outcome=="fail" and .effective.preservation_ok==false and .effective.records_ok==true' \
      "$OUT/$variant-effective.json" >/dev/null
  fi
  if [ "$variant" = alarm_math_collateral ]; then
    "$REPORT" "$trial" "$trial" --json > "$OUT/$variant-raw-comparison.json"
    jq -e '.mean_a==0 and .mean_b==0' "$OUT/$variant-raw-comparison.json" >/dev/null
  else
    if "$REPORT" "$trial" "$trial" --json > "$OUT/$variant-raw-comparison.json" \
        2> "$OUT/$variant-raw-comparison.log"; then exit 1; fi
    test ! -s "$OUT/$variant-raw-comparison.json"
    grep -q 'require adjudication' "$OUT/$variant-raw-comparison.log"
  fi
  cmp "$result" "$OUT/$variant-original-result.json"
  printf '%s\n' "$variant: complete declared semantic review scope, immutable raw evidence and nonoverridable known failures passed"
done
