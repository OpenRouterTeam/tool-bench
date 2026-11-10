#!/bin/sh
set -eu
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
WORLD_IMAGE="${WORLD_IMAGE:?provide the connector-capable world image}"
TOOLS_IMAGE="${TOOLS_IMAGE:?provide the connector-capable tools image}"
OUT="$(mktemp -d /tmp/tool-bench-connector-observation.XXXXXX)"
printf 'artifacts: %s\n' "$OUT"
cargo run -q --locked -p scenario --bin taskgen -- --out "$OUT/tasks" \
  --world-image "$WORLD_IMAGE" --tools-image "$TOOLS_IMAGE" >/dev/null
TASK="$OUT/tasks/business__stripe__customer-mrr"
cp "$TASK/tests/task-context.json" "$OUT/original-context.json"
jq '.connector_env_sha256.GITLAB_API_URL = ("sha256:" + ("0" * 64))' \
  "$OUT/original-context.json" > "$TASK/tests/task-context.json"
cp "$TASK/tests/task-context.json" "$OUT/incorrect-context.json"
if sh deploy/run-trial.sh -p "$TASK" -a oracle -o "$OUT/mismatch" > "$OUT/mismatch.log" 2>&1; then
  printf '%s\n' 'incorrect actual-connector observation was admitted' >&2
  exit 1
fi
result="$(find "$OUT/mismatch" -name result.json -print -quit)"
test -n "$result"
jq -e '.exception_info.exception_type == "AdmissionError"
  and .exception_info.exception_message == "actual tools-container connection settings differ from the declared profile"
  and (.exception_info.exception_traceback | contains("_observe_image"))
  and .verifier_result == null' "$result" >/dev/null
receipt="$(find "$OUT/mismatch" -name runner-admission.json -print -quit)"
test -n "$receipt"
jq -e '.phase == "failed" and .failure != null and .evidence_sha256 == null
  and .main_artifacts_sha256 == null' "$receipt" >/dev/null
if find "$OUT/mismatch" -path '*/verifier/reward.json' -o -path '*/artifacts/world/evidence.json' | grep -q .; then
  printf '%s\n' 'mismatched connector configuration produced scored or authoritative evidence' >&2
  exit 1
fi
printf '%s\n' 'mismatched actual connector settings fail before authoritative collection or scored assessment'
