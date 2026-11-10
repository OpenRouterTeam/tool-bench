#!/bin/sh
set -eu
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
WORLD_IMAGE="${WORLD_IMAGE:?provide the current world image}"
TOOLS_IMAGE="${TOOLS_IMAGE:?provide the tools image}"
OUT="$(mktemp -d /tmp/tool-bench-oracle-completion.XXXXXX)"
printf 'artifacts: %s\n' "$OUT"
cargo run -q --locked -p scenario --bin taskgen -- --runner-admission required --out "$OUT/tasks" \
  --world-image "$WORLD_IMAGE" --tools-image "$TOOLS_IMAGE" >/dev/null
TASK="$OUT/tasks/code__gitlab__file-circuit-breaker-issue"
cp "$TASK/solution/solve.sh" "$OUT/original-solve.sh"
for mode in nonzero forged_success forged_failure; do
  cp "$OUT/original-solve.sh" "$TASK/solution/solve.sh"
  case "$mode" in
    nonzero) printf '%s\n' 'exit 17' >> "$TASK/solution/solve.sh" ;;
    forged_success)
      printf '%s\n' 'printf "0\n" > /logs/agent/exit-code.txt' \
        'printf "reference completed successfully\n"' 'exit 17' >> "$TASK/solution/solve.sh"
      ;;
    forged_failure)
      printf '%s\n' 'printf "17\n" > /logs/agent/exit-code.txt' \
        'printf "reference failed\n"' 'exit 0' >> "$TASK/solution/solve.sh"
      ;;
  esac
  cp "$TASK/solution/solve.sh" "$OUT/program-$mode.sh"
  if sh deploy/run-trial.sh -p "$TASK" -a oracle -o "$OUT/$mode" > "$OUT/$mode.log" 2>&1; then
    run_status=0
  else
    run_status=$?
  fi
  receipt="$(find "$OUT/$mode" -name runner-admission.json -print -quit)"
  result="$(find "$OUT/$mode" -name result.json -print -quit)"
  reward="$(find "$OUT/$mode" -path '*/verifier/reward.json' -print -quit)"
  test -n "$receipt"
  test -n "$result"
  if [ "$mode" = forged_failure ]; then
    test "$run_status" = 0
    test -n "$reward"
    jq -e '.exception_info == null' "$result" >/dev/null
    jq -e '.reward == 1 and .environment_ok == 1 and .contract_ok == 1
      and .integrity_ok == 1 and .runner_admission_ok == 1' "$reward" >/dev/null
    jq -e '.phase == "stopped" and .failure == null
      and (.evidence_sha256 | startswith("sha256:"))' "$receipt" >/dev/null
    test "$(cat "$(dirname "$result")/agent/exit-code.txt")" = 17
  else
    test "$run_status" != 0
    test -z "$reward"
    jq -e '.exception_info != null' "$result" >/dev/null
    jq -e '.phase == "failed" and .failure != null and .evidence_sha256 == null
      and .main_artifacts_sha256 == null' "$receipt" >/dev/null
    if find "$OUT/$mode" -path '*/artifacts/world/evidence.json' -print -quit | grep -q .; then
      printf '%s\n' 'failed reference program produced authoritative world evidence' >&2
      exit 1
    fi
    if [ "$mode" = forged_success ]; then
      test "$(cat "$(dirname "$result")/agent/exit-code.txt")" = 0
      grep -q 'reference completed successfully' "$(dirname "$result")/agent/oracle.txt"
    fi
  fi
  printf '%s\n' "$mode: host-observed reference completion controls admission"
done
