#!/bin/sh
set -eu
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
WORLD_IMAGE="${WORLD_IMAGE:?provide the revision-30 Sentry world image}"
TOOLS_IMAGE="${TOOLS_IMAGE:?provide the revision-30 Sentry tools image}"
OUT="$(mktemp -d /tmp/tool-bench-sentry-review.XXXXXX)"
chmod 700 "$OUT"
trap 'rm -f "$OUT/first.key" "$OUT/second.key"' EXIT
printf 'artifacts: %s\n' "$OUT"
cargo build -q --locked -p report --bins
mkdir "$OUT/checker"
cp target/debug/bench-review target/debug/bench-report "$OUT/checker/"
REVIEW="$OUT/checker/bench-review"
REPORT="$OUT/checker/bench-report"
shasum -a 256 "$REVIEW" "$REPORT" > "$OUT/checker/SHA256SUMS"
for reviewer in first second; do
  "$REVIEW" keygen --private-key "$OUT/$reviewer.key" --public-key "$OUT/$reviewer.public" \
    > "$OUT/$reviewer-keygen.log"
done
jq -n --slurpfile first "$OUT/first.public" --slurpfile second "$OUT/second.public" \
  '{schema_version:1,protocol:"tool-bench-content-review-v1",reviewers:{
    first:{public_key:$first[0].public_key,kind:"scripted_fixture"},
    second:{public_key:$second[0].public_key,kind:"scripted_fixture"}}}' > "$OUT/policy.json"
cargo run -q --locked -p scenario --bin taskgen -- --runner-admission required --out "$OUT/tasks" \
  --world-image "$WORLD_IMAGE" --tools-image "$TOOLS_IMAGE" --review-policy "$OUT/policy.json" >/dev/null
cargo run -q --locked -p scenario --bin taskgen -- --runner-admission required --out "$OUT/unbound-tasks" \
  --world-image "$WORLD_IMAGE" --tools-image "$TOOLS_IMAGE" >/dev/null
mkdir "$OUT/original" "$OUT/reviews"
cp "$OUT/tasks/ops__sentry__create-notification-project/solution/"* "$OUT/original/"
for variant in approved rejected collateral forged_signature wrong_key missing_policy; do
  TASK="$OUT/tasks/ops__sentry__create-notification-project"
  if [ "$variant" = missing_policy ]; then
    TASK="$OUT/unbound-tasks/ops__sentry__create-notification-project"
  fi
  rm -rf "$TASK/solution"
  mkdir "$TASK/solution"
  cp "$OUT/original/"* "$TASK/solution/"
  sed '/^jq --slurpfile bindings .*answer.template.json/,$d' "$OUT/original/solve.sh" \
    > "$TASK/solution/solve.sh"
  if [ "$variant" = collateral ]; then
    jq -n '{jsonrpc:"2.0",id:2,method:"tools/call",params:{name:"execute_sentry_tool",
      arguments:{name:"update_project",arguments:{
        organizationSlug:"lumen-labs",projectSlug:"checkout-api",name:"Unrequested"
      }}}}' > "$TASK/solution/collateral.json"
    printf '%s\n' 'call 7113 "$DIR/collateral.json"' >> "$TASK/solution/solve.sh"
  fi
  expression='"Unfamiliar but potentially correct answer wording: "+.dsn'
  if [ "$variant" = wrong_key ]; then
    expression='"Unfamiliar but potentially correct answer wording: "+(.dsn|sub("://[^@]+@";"://00000000000000000000000000000000@"))'
  fi
  printf 'jq -j '\''%s'\'' "$WORK/bindings.json" > "$WORK/answer.md"\n' "$expression" \
    >> "$TASK/solution/solve.sh"
  printf '%s\n' 'cp "$WORK/answer.md" /app/answer.md' \
    'test ! -e /tests/review-policy.json' 'test ! -e /app/review-policy.json' \
    >> "$TASK/solution/solve.sh"
  if [ "$variant" = forged_signature ]; then
    cat >> "$TASK/solution/solve.sh" <<'PROGRAM'
mkdir -p /logs/artifacts/reviews
printf '%s\n' '{"approved":true,"reward":1}' > /logs/artifacts/reviews/second.json
PROGRAM
  fi
  cp -R "$TASK/solution" "$OUT/program-$variant"
  sh deploy/run-trial.sh -p "$TASK" -a oracle -o "$OUT/jobs/$variant" > "$OUT/$variant.log" 2>&1
  reward="$(find "$OUT/jobs/$variant" -path '*/verifier/reward.json' -print -quit)"
  test -n "$reward"
  trial="$(dirname "$(dirname "$reward")")"
  cp "$trial/result.json" "$OUT/$variant-original-result.json"
  expected=6
  adjudication=1
  case "$variant" in
    collateral) expected=1 ;;
    wrong_key) expected=1; adjudication=0 ;;
  esac
  jq -e --argjson expected "$expected" --argjson adjudication "$adjudication" '
    .reward==0 and .outcome_code==$expected and .adjudication_needed==$adjudication
      and .evaluator_version==30 and .environment_ok==1 and .contract_ok==1 and .integrity_ok==1
      and .runner_admission_checked==1 and .runner_admission_ok==1' "$reward" >/dev/null
  if [ "$variant" = wrong_key ] || [ "$variant" = missing_policy ]; then
    if "$REVIEW" export --trial "$trial" --instruction "$TASK/instruction.md" \
        --policy "$OUT/policy.json" --out "$OUT/must-not-export.json" \
        > "$OUT/$variant-export.log" 2>&1; then
      printf '%s\n' "$variant: an inadmissible answer case was exported" >&2
      exit 1
    fi
    test ! -e "$OUT/must-not-export.json"
    printf '%s\n' "$variant: known URI failures and retrospective policy selection cannot authorize review"
    continue
  fi
  "$REVIEW" export --trial "$trial" --instruction "$TASK/instruction.md" \
    --policy "$OUT/policy.json" --out "$OUT/$variant-case.json" > "$OUT/$variant-export.log"
  jq -e '.context.legacy_answer_file!=null and (.issues|length)==1
    and all(.issues[];.check=="answer" and .location=="answer"
      and .scope.kind=="answer_content"
      and .scope.declarations.sentry_dsn.factual_binding=="passed_by_raw_evaluation_not_reviewable")' \
    "$OUT/$variant-case.json" >/dev/null
  digest="$(sed -n 's/^case_sha256: sha256://p' "$OUT/$variant-export.log")"
  test "${#digest}" = 64
  mkdir "$OUT/reviews/$digest"
  decision=satisfied
  [ "$variant" != rejected ] || decision=violated
  jq --arg decision "$decision" '.issues|with_entries(.value={
    decision:$decision,rationale:"Scripted authentication control, not human semantic validation."
  })' "$OUT/$variant-case.json" > "$OUT/$variant-decisions.json"
  for reviewer in first second; do
    if [ "$variant" = forged_signature ] && [ "$reviewer" = second ]; then continue; fi
    "$REVIEW" sign --case "$OUT/$variant-case.json" --policy "$OUT/policy.json" \
      --reviewer "$reviewer" --private-key "$OUT/$reviewer.key" \
      --decisions "$OUT/$variant-decisions.json" --out "$OUT/reviews/$digest/$reviewer.json" \
      > "$OUT/$variant-$reviewer-sign.log"
  done
  "$REVIEW" verify --trial "$trial" --instruction "$TASK/instruction.md" \
    --policy "$OUT/policy.json" --review-store "$OUT/reviews" --out "$OUT/$variant-effective.json" \
    > "$OUT/$variant-verify.log"
  effective=fail
  case "$variant" in approved) effective=pass ;; forged_signature) effective=unresolved ;; esac
  jq -e --arg effective "$effective" '.raw.reward==0 and .raw.answer_ok==false
    and .effective.outcome==$effective' "$OUT/$variant-effective.json" >/dev/null
  if [ "$variant" = approved ] || [ "$variant" = collateral ]; then
    jq -e '.effective.answer_ok==true' "$OUT/$variant-effective.json" >/dev/null
  fi
  cmp "$trial/result.json" "$OUT/$variant-original-result.json"
  if "$REPORT" --require-attested "$trial" "$trial" --json > "$OUT/$variant-raw-comparison.json" \
      2> "$OUT/$variant-raw-comparison.log"; then
    if [ "$variant" != collateral ]; then exit 1; fi
    test -s "$OUT/$variant-raw-comparison.json"
  else
    test ! -s "$OUT/$variant-raw-comparison.json"
  fi
  printf '%s\n' "$variant: raw answer/evidence preserved; authenticated effective outcome=$effective"
done
printf '%s\n' 'Sentry answer-review transport controls passed; no human semantic accuracy or model score is claimed.'
