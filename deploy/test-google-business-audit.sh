#!/bin/sh
set -eu
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
OUT="${GOOGLE_AUDIT_OUT:-$(mktemp -d /tmp/tool-bench-google-direct-audit.XXXXXXXX)}"
test -d "$OUT"
test -z "$(find "$OUT" -mindepth 1 -maxdepth 1 -print -quit)"
printf 'direct source-fixture artifacts: %s\n' "$OUT"
cargo run --locked -p world --example google-audit-fixtures -- "$OUT"
if PYTHONDONTWRITEBYTECODE=1 "${GOOGLE_AUDIT_PYTHON:-python3}" deploy/audit-google-business.py "$OUT" > "$OUT/audit.log" 2>&1; then
  cat "$OUT/audit.log"
else
  cat "$OUT/audit.log"
  exit 1
fi
test "$(find "$OUT" -name initial-state.json | wc -l | tr -d ' ')" -eq 17
PYTHONDONTWRITEBYTECODE=1 "${GOOGLE_AUDIT_PYTHON:-python3}" deploy/check-google-replica-results.py "$OUT"
printf '%s\n' 'All 17 direct Rust source controls independently agree. No native-client, runner, frozen-runtime or model admission is implied.'
