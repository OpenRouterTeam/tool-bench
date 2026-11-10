#!/bin/sh
set -eu
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
WORLD_IMAGE="${WORLD_IMAGE:?provide the source-bound world image}"
TOOLS_IMAGE="${TOOLS_IMAGE:?provide the tools image}"
OUT="$(mktemp -d /tmp/tool-bench-source-provenance.XXXXXX)"
printf 'artifacts: %s\n' "$OUT"
cargo run -q --locked -p scenario --bin taskgen -- --out "$OUT/tasks" \
  --world-image "$WORLD_IMAGE" --tools-image "$TOOLS_IMAGE" >/dev/null
TASK="$OUT/tasks/business__stripe__customer-mrr"
cp "$TASK/tests/expect.json" "$OUT/contract.json"
cp "$TASK/tests/task-source.json" "$OUT/task-source.json"
cp "$TASK/solution/solve.sh" "$OUT/solve.sh"
for variant in valid weakened_contract mismatched_source; do
  cp "$OUT/contract.json" "$TASK/tests/expect.json"
  cp "$OUT/task-source.json" "$TASK/tests/task-source.json"
  cp "$OUT/solve.sh" "$TASK/solution/solve.sh"
  if [ "$variant" = weakened_contract ]; then
    printf '%s\n' '{"version":2,"audit_history":true,"answer":{"contains":["forged"]}}' > "$TASK/tests/expect.json"
    printf '%s\n' '#!/bin/sh' "printf '%s' forged > /app/answer.md" > "$TASK/solution/solve.sh"
  elif [ "$variant" = mismatched_source ]; then
    jq '.dataset_sha256 = ("sha256:" + ("0" * 64))' "$OUT/task-source.json" > "$TASK/tests/task-source.json"
  fi
  sh deploy/run-trial.sh -p "$TASK" -a oracle -o "$OUT/$variant" > "$OUT/$variant.log" 2>&1
  result="$(find "$OUT/$variant" -path '*/verifier/reward.json' -print -quit)"
  test -n "$result"
  if [ "$variant" = valid ]; then
    jq -e '.reward == 1 and .environment_ok == 1 and .runner_admission_ok == 1' "$result" >/dev/null
  else
    jq -e '.reward == 0 and .outcome_code == 5 and .environment_ok == 0
      and .contract_ok == 1 and .answer_ok == 1 and .runner_admission_ok == 1' "$result" >/dev/null
  fi
  printf '%s\n' "$variant: source binding and execution proof classified as required"
done
for variant in weakened_contract mismatched_source; do
  if cargo run -q --locked -p report --bin bench-report -- "$OUT/$variant" "$OUT/valid" --json \
      > "$OUT/$variant-comparison.json" 2> "$OUT/$variant-comparison.log"; then
    printf '%s\n' 'source-incompatible assessment produced a final score' >&2
    exit 1
  fi
  test ! -s "$OUT/$variant-comparison.json"
done
cp -R "$OUT/valid" "$OUT/tampered"
manifest="$(find "$OUT/tampered" -path '*/verifier/realized-manifest.json' -print -quit)"
printf '%s\n' '{}' > "$manifest"
if cargo run -q --locked -p report --bin bench-report -- "$OUT/tampered" "$OUT/valid" --json \
    > "$OUT/tampered-comparison.json" 2> "$OUT/tampered-comparison.log"; then
  printf '%s\n' 'tampered execution metadata produced a final score' >&2
  exit 1
fi
test ! -s "$OUT/tampered-comparison.json"
printf '%s\n' 'weakened contracts, changed source descriptors and tampered manifests cannot publish scores'
