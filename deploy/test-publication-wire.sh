#!/bin/sh
set -eu
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
WORLD_IMAGE="${WORLD_IMAGE:?provide the current world image}"
TOOLS_IMAGE="${TOOLS_IMAGE:?provide the composed tools image}"
EXPECTED_EVALUATOR_VERSION="$(sed -n 's/^pub const EVALUATOR_VERSION: u32 = \([0-9][0-9]*\);/\1/p' crates/mock-core/src/grading.rs)"
EXPECTED_EVIDENCE_SCHEMA_VERSION="$(sed -n 's/^pub const EVIDENCE_SCHEMA_VERSION: u32 = \([0-9][0-9]*\);/\1/p' crates/mock-core/src/evidence.rs)"
test -n "$EXPECTED_EVALUATOR_VERSION"
test -n "$EXPECTED_EVIDENCE_SCHEMA_VERSION"
OUT="$(mktemp -d /tmp/tool-bench-publication-wire.XXXXXX)"
PROJECT="tb-publication-$$"
export HARBOR_TRIAL_ADMIN_TOKEN
HARBOR_TRIAL_ADMIN_TOKEN="$(openssl rand -hex 32)"
printf 'artifacts: %s\n' "$OUT"
cargo run -q --locked -p scenario --bin taskgen -- --runner-admission required --out "$OUT/tasks" \
  --world-image "$WORLD_IMAGE" --tools-image "$TOOLS_IMAGE" >/dev/null
TASK="$OUT/tasks/business__shopify__relaunch-tote"
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
  compose down -v --remove-orphans >/dev/null 2>&1 || true
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
compose up -d --build --wait > "$OUT/compose.log" 2>&1
compose cp "$TASK/solution" main:/solution
compose exec -T main sh /solution/solve.sh > "$OUT/mcp-results.log" 2>&1
compose stop --timeout 10 main >/dev/null
compose exec -T world sh -eu -c \
  'curl -fsS -X POST -H "authorization: Bearer $BENCH_ADMIN_TOKEN" http://localhost:9001/_bench/finalize' \
  > "$OUT/evidence.json"
jq -e --argjson evaluator "$EXPECTED_EVALUATOR_VERSION" --argjson schema "$EXPECTED_EVIDENCE_SCHEMA_VERSION" '
  .schema_version == $schema and .evaluator_version == $evaluator
  and .finalization.phase == "sealed" and .finalization.failure == null
  and .state.services.shopify.products["8201"].status == "ACTIVE"
  and .state.services.shopify.products["8201"].onlineStoreVisible == true
  and (.state.services.shopify.products["8201"].variants[] | select(.title == "Navy") | .price == "42.00")
  and any(.state.services.shopify.resource_publications[]; .productId == 8201 and .publicationId == 3001 and .isPublished == true)
  and any(.journal[]; .service == "shopify" and (.request_body.query // "" | contains("publishablePublish")))
' "$OUT/evidence.json" >/dev/null
printf '%s\n' 'native MCP discovery, returned-ID binding, publication mutation and online visibility passed'
printf '%s\n' 'this is diagnostic wire evidence, not runner-admitted scoring'
