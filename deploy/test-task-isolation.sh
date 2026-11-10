#!/bin/sh
set -eu
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
WORLD_IMAGE="${WORLD_IMAGE:?build a world image containing bench-verify first}"
TOOLS_IMAGE="${TOOLS_IMAGE:?provide the tools image to validate}"
EXPECTED_EVALUATOR_VERSION="${EXPECTED_EVALUATOR_VERSION:?provide the raw evaluator revision for the selected world image}"
EXPECTED_EVIDENCE_SCHEMA_VERSION="$(sed -n 's/^pub const EVIDENCE_SCHEMA_VERSION: u32 = \([0-9][0-9]*\);/\1/p' crates/mock-core/src/evidence.rs)"
test -n "$EXPECTED_EVIDENCE_SCHEMA_VERSION"
OUT="$(mktemp -d /tmp/tool-bench-isolation.XXXXXX)"
PROJECT="tb-isolation-$$"
export HARBOR_TRIAL_ADMIN_TOKEN
HARBOR_TRIAL_ADMIN_TOKEN="$(openssl rand -hex 32)"
printf 'artifacts: %s\n' "$OUT"
cargo run -q --locked -p scenario --bin taskgen -- --out "$OUT/tasks" \
  --world-image "$WORLD_IMAGE" --tools-image "$TOOLS_IMAGE" >/dev/null
TASK="$OUT/tasks/business__stripe__customer-mrr"
cat > "$OUT/main.yaml" <<EOF
services:
  main:
    build:
      context: $TASK/environment
    command: ["sleep", "infinity"]
EOF
compose() {
  docker compose -p "$PROJECT" -f "$TASK/environment/docker-compose.yaml" -f "$OUT/main.yaml" "$@"
}
cleanup() {
  compose logs --no-color > "$OUT/container.log" 2>&1 || true
  compose exec -T world sh -c 'curl -fsS -H "authorization: Bearer $BENCH_ADMIN_TOKEN" http://localhost:9001/_bench/evidence' \
    > "$OUT/probe-evidence.json" 2>/dev/null || true
  compose down -v --remove-orphans >/dev/null 2>&1 || true
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
compose up -d --build --wait > "$OUT/compose.log" 2>&1
compose ps --all --quiet | while IFS= read -r container; do
  docker inspect "$container" --format \
    '{"container_id":{{json .Id}},"name":{{json .Name}},"project":{{json (index .Config.Labels "com.docker.compose.project")}}}'
done > "$OUT/probe-containers.jsonl"
jq -se --arg project "$PROJECT" '
  length==3 and all(.[]; (.container_id|test("^[0-9a-f]{64}$")) and .project==$project)
    and ([.[].container_id]|unique|length)==3' "$OUT/probe-containers.jsonl" >/dev/null
WORLD_CONTAINER="$(compose ps -q world)"
BACKEND_IP="$(docker inspect "$WORLD_CONTAINER" --format "{{(index .NetworkSettings.Networks \"${PROJECT}_backend\").IPAddress}}")"
[ -n "$BACKEND_IP" ]
compose exec -T main sh -eu -c '
  test ! -e /tests/expect.json
  test ! -e /opt/tool-bench/scenarios
  test ! -S /var/run/docker.sock
  if env | grep -qE "^(BENCH_ADMIN_TOKEN|HARBOR_TRIAL_ADMIN_TOKEN)="; then exit 1; fi
  printf "%s\n" "hidden files and credentials absent"
  if getent hosts world; then exit 1; fi
  curl --noproxy "*" --max-time 5 -s -o /dev/null http://tools:7101/mcp
  printf "%s\n" "tools reachable, world DNS absent"
  if curl --noproxy "*" --connect-timeout 2 --max-time 3 -fs -o /dev/null "http://$1:9000/_bench/health"; then exit 1; fi
  printf "%s\n" "direct admin access denied"
  status="$(curl --noproxy "*" --connect-timeout 2 --max-time 3 -s -o /dev/null -w "%{http_code}" "http://$1:8121/v1/customers" || true)"
  case "$status" in 000|403) ;; *) printf "unexpected direct API status: %s\n" "$status" >&2; exit 1 ;; esac
' probe "$BACKEND_IP"
STATUS="$(compose exec -T tools curl -s -o /dev/null -w '%{http_code}' http://world-bootstrap:9000/_bench/evidence)"
[ "$STATUS" = 404 ] || { printf 'bootstrap exposed a control route: status %s, expected 404\n' "$STATUS" >&2; exit 1; }
STATUS="$(compose exec -T tools curl --connect-timeout 2 --max-time 3 -s -o /dev/null -w '%{http_code}' "http://$BACKEND_IP:9000/_bench/health" || true)"
[ "$STATUS" = 000 ] || { printf 'bootstrap is listening on the API interface: HTTP %s\n' "$STATUS" >&2; exit 1; }
STATUS="$(compose exec -T tools curl --connect-timeout 2 --max-time 3 -s -o /dev/null -w '%{http_code}' http://world:9001/_bench/evidence || true)"
[ "$STATUS" = 000 ] || { printf 'tools reached the control listener: HTTP %s\n' "$STATUS" >&2; exit 1; }
compose exec -T tools sh -eu -c '
  if env | grep -qE "^(BENCH_ADMIN_TOKEN|HARBOR_TRIAL_ADMIN_TOKEN)="; then exit 1; fi
'
compose exec -T world sh -eu -c \
  'curl -fsS -X POST -H "authorization: Bearer $BENCH_ADMIN_TOKEN" http://localhost:9001/_bench/finalize' \
  > "$OUT/sealed-before.json"
jq -e '.finalization.phase == "sealed" and (.finalization.pending | length) == 0' "$OUT/sealed-before.json" >/dev/null
STATUS="$(compose exec -T tools curl -s -o /dev/null -w '%{http_code}' -X POST http://world:8121/v1/customers)"
[ "$STATUS" = 503 ] || { printf 'closed world accepted tool-peer access: %s\n' "$STATUS" >&2; exit 1; }
compose exec -T world sh -eu -c \
  'curl -fsS -H "authorization: Bearer $BENCH_ADMIN_TOKEN" http://localhost:9001/_bench/evidence' \
  > "$OUT/sealed-after.json"
jq -S '.tools_ips |= sort' "$OUT/sealed-before.json" > "$OUT/sealed-before.normalized.json"
jq -S '.tools_ips |= sort' "$OUT/sealed-after.json" > "$OUT/sealed-after.normalized.json"
cmp "$OUT/sealed-before.normalized.json" "$OUT/sealed-after.normalized.json"
cleanup
printf '%s\n' 'frontend/backend reachability and credential isolation passed'
printf '%s\n' 'sealed world rejects tool-peer writes without changing its evidence'
if [ "${1:-}" = "--probe-only" ]; then
  test "$#" = 1
  exit 0
fi

cat > "$OUT/probe.sh" <<'EOF'
#!/bin/sh
set -eu
test ! -e /tests/expect.json
test ! -e /opt/tool-bench/scenarios
test ! -S /var/run/docker.sock
if env | grep -qE '^(BENCH_ADMIN_TOKEN|HARBOR_TRIAL_ADMIN_TOKEN)='; then exit 1; fi
if getent hosts world; then exit 1; fi
mkdir -p /exports /logs/artifacts/exports /logs/verifier
printf '%s\n' '{"forged": true}' > /exports/evidence.json
printf '%s\n' '{"forged": true}' > /logs/artifacts/exports/evidence.json
printf '%s\n' '{"reward": 1}' > /logs/verifier/reward.json
printf '%s\n' '{"reward": 1, "forged": true}' > /logs/verifier/assessment.json
EOF
cp "$TASK/solution/solve.sh" "$OUT/original-solve.sh"
if [ "$#" -eq 0 ]; then set -- oracle forgery; fi
for mode do
  case "$mode" in oracle|forgery) ;; *) printf 'unsupported isolation mode: %s\n' "$mode" >&2; exit 2 ;; esac
  cp "$OUT/probe.sh" "$TASK/solution/solve.sh"
  if [ "$mode" = oracle ]; then
    tail -n +2 "$OUT/original-solve.sh" >> "$TASK/solution/solve.sh"
    EXPECTED=1
  else
    EXPECTED=0
  fi
  HARBOR_TRIAL_ADMIN_TOKEN="$(openssl rand -hex 32)"
  sh deploy/run-trial.sh -p "$TASK" -a oracle \
    -o "$OUT/$mode" > "$OUT/$mode.log" 2>&1
  ASSESSMENT="$(find "$OUT/$mode" -path '*/verifier/assessment.json' -print -quit)"
  test -n "$ASSESSMENT"
  jq -e --argjson expected "$EXPECTED" \
    '.reward == $expected and .environment_ok == true and .contract_ok == true
      and .runner_admission_checked == true and .runner_admission_ok == true and .forged == null' \
    "$ASSESSMENT" >/dev/null
  EVIDENCE="$(find "$OUT/$mode" -path '*/artifacts/world/evidence.json' -print -quit)"
  test -n "$EVIDENCE"
  jq -e --argjson evaluator "$EXPECTED_EVALUATOR_VERSION" --argjson schema "$EXPECTED_EVIDENCE_SCHEMA_VERSION" \
    '.scenario == "business/stripe/customer-mrr" and .forged == null
    and .schema_version == $schema and .evaluator_version == $evaluator
    and .finalization.phase == "sealed" and (.finalization.pending | length) == 0
    and .finalization.failure == null and .finalization.event_count == (.events | length)' "$EVIDENCE" >/dev/null
  TRIAL_LOG="$(find "$OUT/$mode" -name trial.log -print -quit)"
  grep -q 'Main service stopped' "$TRIAL_LOG"
  printf '%s\n' "$mode: trusted evidence and separate verifier passed"
done
