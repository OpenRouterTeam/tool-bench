#!/bin/sh
set -eu
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
WORLD_IMAGE="${WORLD_IMAGE:?provide the frozen world image}"
TOOLS_IMAGE="${TOOLS_IMAGE:?provide the frozen tools image}"
OUT="$(mktemp -d /tmp/tool-bench-profiles.XXXXXX)"
printf 'artifacts: %s\n' "$OUT"
for profile in support engineering; do
  case "$profile" in
    support) task_name=business__stripe__customer-mrr; identity="Customer Operations" ;;
    engineering) task_name=code__github__ci-failure-root-cause; identity="Platform Engineering" ;;
  esac
  cargo run -q --locked -p scenario --bin taskgen -- --out "$OUT/$profile/tasks" \
    --workspace-profile "$profile" --world-image "$WORLD_IMAGE" --tools-image "$TOOLS_IMAGE" >/dev/null
  TASK="$OUT/$profile/tasks/$task_name"
  IMAGE="tool-bench-profile-test:$$-$profile"
  docker build -q -t "$IMAGE" "$TASK/environment" > "$OUT/$profile/build.log" 2>&1
  docker run --rm --network none "$IMAGE" sh -eu -c '
    test -f /app/AGENTS.md
    test "$(cat /app/CLAUDE.md)" = "@AGENTS.md"
    test -f /app/README.md
    test "$(git config --global --get user.name)" = "$1"
    test -z "$(git config --global --get credential.helper || true)"
    test ! -e /tests/expect.json
    test ! -e /opt/tool-bench/scenarios
    test ! -e /solution/solve.sh
    test ! -e /root/.config/gh/hosts.yml
    jq -e ".timezone == \"UTC\" and (.data_as_of | type == \"string\")" /app/.config/workspace/session.json >/dev/null
    jq -e ".connections | length == 32" /app/.config/workspace/connections.json >/dev/null
    if grep -riE "benchmark|evaluator|oracle|scenario|_bench|reward|BENCH_ADMIN_TOKEN" \
      /app/AGENTS.md /app/CLAUDE.md /app/README.md /app/.config/workspace /root/.gitconfig; then exit 1; fi
    if env | grep -qE "^(BENCH_ADMIN_TOKEN|HARBOR_TRIAL_ADMIN_TOKEN)="; then exit 1; fi
  ' probe "$identity"
  printf '%s\n' "$profile: rendered image surface passed"
  for agent in oracle nop; do
    JOBS="$OUT/$profile/$agent"
    HARBOR_TRIAL_ADMIN_TOKEN="$(openssl rand -hex 32)" \
      sh deploy/run-trial.sh -p "$TASK" -a "$agent" \
        -o "$JOBS" > "$OUT/$profile/$agent.log" 2>&1
    ASSESSMENT="$(find "$JOBS" -path '*/verifier/assessment.json' -print -quit)"
    EXPECTED=0
    [ "$agent" != oracle ] || EXPECTED=1
    jq -e --argjson expected "$EXPECTED" \
      '.reward == $expected and .environment_ok == true and .contract_ok == true and .integrity_ok == true' \
      "$ASSESSMENT" >/dev/null
    TRIAL_LOG="$(find "$JOBS" -name trial.log -print -quit)"
    grep -q 'Main service stopped' "$TRIAL_LOG"
    printf '%s\n' "$profile: $agent assessment passed"
  done
done
