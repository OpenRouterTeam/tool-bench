#!/bin/sh
# Verifies scenarios end to end with Harbor on local Docker:
#   oracle agent (replays solution/ over MCP) must score reward 1,
#   nop agent (does nothing) must score reward 0.
#
#   deploy/oracle.sh <scenario-id> [<scenario-id> ...]     e.g. code/github/file-login-bug
#   deploy/oracle.sh --all
#
# Builds the world image from this checkout as tool-bench-world:<branch> (so parallel
# worktrees don't collide) and uses the shared tool-bench-tools:dev image.
set -eu
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
command -v jq >/dev/null || { echo 'jq is required to validate assessment metrics' >&2; exit 2; }
TAG="$(git rev-parse --abbrev-ref HEAD | tr '/' '-')"
# Set WORLD_IMAGE to reuse a prebuilt world image (e.g. several oracle.sh runs in parallel).
PREBUILT_WORLD="${WORLD_IMAGE:-}"
WORLD_IMAGE="${WORLD_IMAGE:-tool-bench-world:$TAG}"
TOOLS_IMAGE="${TOOLS_IMAGE:-tool-bench-tools:dev}"
OUT="$(mktemp -d /tmp/tool-bench-oracle.XXXXXX)"
mkdir -p "$OUT/jobs"
printf 'artifacts: %s\n' "$OUT"

[ -n "$PREBUILT_WORLD" ] || docker build -q -f images/world/Dockerfile -t "$WORLD_IMAGE" . >/dev/null
printf 'world_image: %s %s\n' "$WORLD_IMAGE" "$(docker image inspect "$WORLD_IMAGE" --format '{{.Id}}')"
printf 'tools_image: %s %s\n' "$TOOLS_IMAGE" "$(docker image inspect "$TOOLS_IMAGE" --format '{{.Id}}')"
if [ -n "${TASKGEN_BINARY:-}" ]; then
  "$TASKGEN_BINARY" --out "$OUT/tasks" --world-image "$WORLD_IMAGE" --tools-image "$TOOLS_IMAGE" >/dev/null
else
  cargo run -q --locked -p scenario --bin taskgen -- --out "$OUT/tasks" --world-image "$WORLD_IMAGE" --tools-image "$TOOLS_IMAGE" >/dev/null
fi

if [ "${1:-}" = "--all" ]; then
  set -- $(ls "$OUT/tasks" | sed 's/__/\//g')
fi
status=0
for id in "$@"; do
  task="$OUT/tasks/$(echo "$id" | sed 's/\//__/g')"
  [ -d "$task" ] || { echo "$id: no such task"; status=1; continue; }
  for agent in oracle nop; do
    jobs="$OUT/jobs/$(basename "$task")-$agent"
    if sh deploy/run-trial.sh -p "$task" -a "$agent" -o "$jobs" >"$jobs.log" 2>&1; then
      run_status=0
    else
      run_status=$?
    fi
    reward_file=$(find "$jobs" -path '*verifier/reward.json' -print -quit 2>/dev/null || true)
    reward="invalid_assessment"
    if [ "$run_status" = 0 ] && [ -n "$reward_file" ]; then
      reward=$(jq -er '
        if .contract_ok == 1 and .environment_ok == 1 and .integrity_ok == 1 and .grader_error != 1
          and .runner_admission_checked == 1 and .runner_admission_ok == 1
          and .adjudication_needed == 0
          and (.reward == 0 or .reward == 1)
        then .reward else error("invalid oracle assessment") end
      ' "$reward_file" 2>/dev/null || printf 'invalid_assessment')
    fi
    want=$([ "$agent" = oracle ] && echo 1 || echo 0)
    if [ "${reward%.*}" = "$want" ]; then
      echo "$id: $agent reward=$reward ok"
    else
      echo "$id: $agent reward=${reward:-none} EXPECTED $want (logs: $jobs, $jobs.log)"
      status=1
    fi
  done
done
exit $status
