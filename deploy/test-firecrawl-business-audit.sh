#!/bin/sh
set -eu
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
OUT="${FIRECRAWL_AUDIT_OUT:-$(mktemp -d /tmp/tool-bench-firecrawl-direct-audit.XXXXXXXX)}"
test -d "$OUT"
test -z "$(find "$OUT" -mindepth 1 -maxdepth 1 -print -quit)"
OUT="$(cd "$OUT" && pwd)"
printf 'source-bound direct artifacts: %s\n' "$OUT"
LOG="$(mktemp /tmp/tool-bench-firecrawl-generator.XXXXXXXX)"
if cargo run --locked -p world --example firecrawl-audit-fixtures -- "$OUT" > "$LOG" 2>&1; then
  cp "$LOG" "$OUT/generator.log"
else
  cp "$LOG" "$OUT/generator.log"
  cat "$LOG"
  exit 1
fi
cat "$LOG"
count=0
for verifier in "$OUT"/cases/*/verifier; do
  test -d "$verifier"
  source="$(jq -er '.source_directory' "$verifier/direct-source.json")"
  PYTHONDONTWRITEBYTECODE=1 "${FIRECRAWL_AUDIT_PYTHON:-python3}" - "$OUT" "$source" <<'PY'
import sys
from pathlib import Path

root, source = map(lambda value: Path(value).resolve(), sys.argv[1:])
assert source.is_dir() and source.is_relative_to(root / "sources")
PY
  PYTHONDONTWRITEBYTECODE=1 "${FIRECRAWL_AUDIT_PYTHON:-python3}" \
    deploy/audit-firecrawl-business.py --direct --source-directory "$source" "$verifier" \
    > "$(dirname "$verifier")/audit.log" 2>&1
  cat "$(dirname "$verifier")/audit.log"
  count=$((count + 1))
done
test "$count" = 17
printf '%s\n' 'All 17 source-bound fixtures independently agree, including three explicit on-disk variants.'
printf '%s\n' 'The unresolved filter remains unresolved; this is not a model score, runtime campaign or hosted certification.'
