#!/bin/sh
set -eu
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
WORLD_IMAGE="${WORLD_IMAGE:?provide the current world image}"
TOOLS_IMAGE="${TOOLS_IMAGE:?provide the tools image}"
OUT="$(mktemp -d /tmp/tool-bench-main-snapshot.XXXXXX)"
printf 'artifacts: %s\n' "$OUT"
cargo run -q --locked -p scenario --bin taskgen -- --runner-admission required --out "$OUT/tasks" \
  --world-image "$WORLD_IMAGE" --tools-image "$TOOLS_IMAGE" >/dev/null
TASK="$OUT/tasks/business__stripe__customer-mrr"
cp "$TASK/solution/solve.sh" "$OUT/original-solve.sh"
mkdir -p "$OUT/bin"
export REAL_DOCKER COLLECTION_TRACE SNAPSHOT_CASE
REAL_DOCKER="$(command -v docker)"
cat > "$OUT/bin/docker" <<'EOF'
#!/bin/sh
set -eu
for argument in "$@"; do
  case "$argument" in
    *:/app/answer.md)
      container="${argument%:/app/answer.md}"
      if [ "$SNAPSHOT_CASE" = lifecycle_change ]; then
        "$REAL_DOCKER" start "$container" >/dev/null
        "$REAL_DOCKER" stop --time 0 "$container" >/dev/null
      fi
      state="$("$REAL_DOCKER" inspect --format '{{json .State}}' "$container")"
      printf '%s\n' "$state" >> "$COLLECTION_TRACE"
      printf '%s' "$state" | jq -e \
        '.Running == false and .Paused == false and .Restarting == false
          and (.Status == "exited" or .Status == "dead")' >/dev/null
      if [ "$SNAPSHOT_CASE" = copy_failure ]; then
        printf '%s\n' 'injected archive transport failure' >&2
        exit 70
      fi
      ;;
  esac
done
exec "$REAL_DOCKER" "$@"
EOF
chmod +x "$OUT/bin/docker"
for SNAPSHOT_CASE in background missing directory copy_failure unsafe_link lifecycle_change; do
  cp "$OUT/original-solve.sh" "$TASK/solution/solve.sh"
  agent=oracle
  case "$SNAPSHOT_CASE" in
    background)
      cat >> "$TASK/solution/solve.sh" <<'EOF'
mkdir -p /logs/artifacts/empty
(
  trap '' HUP
  while :; do
    printf '%s\n' 'background writer' >> /app/answer.md
    printf '%s\n' 'published while running' >> /logs/artifacts/updates.txt
    sleep 0.02
  done
) >/dev/null 2>&1 &
sleep 0.1
EOF
      ;;
    missing) agent=nop ;;
    directory)
      cat >> "$TASK/solution/solve.sh" <<'EOF'
mv /app/answer.md /tmp/original-answer
mkdir /app/answer.md
mv /tmp/original-answer /app/answer.md/text
EOF
      ;;
    unsafe_link)
      printf '%s\n' 'ln -s /etc/passwd /logs/artifacts/unsafe-link' >> "$TASK/solution/solve.sh"
      ;;
  esac
  COLLECTION_TRACE="$OUT/$SNAPSHOT_CASE.trace.jsonl"
  if PATH="$OUT/bin:$PATH" sh deploy/run-trial.sh -p "$TASK" -a "$agent" \
      -o "$OUT/$SNAPSHOT_CASE" > "$OUT/$SNAPSHOT_CASE.log" 2>&1; then
    run_status=0
  else
    run_status=$?
  fi
  receipt="$(find "$OUT/$SNAPSHOT_CASE" -name runner-admission.json -print -quit)"
  test -n "$receipt"
  reward="$(find "$OUT/$SNAPSHOT_CASE" -path '*/verifier/reward.json' -print -quit)"
  case "$SNAPSHOT_CASE" in
    background|missing|directory)
      test "$run_status" = 0
      test -n "$reward"
      test -s "$COLLECTION_TRACE"
      jq -e '.phase == "stopped" and .failure == null
        and (.main_artifacts_sha256 | startswith("sha256:"))' "$receipt" >/dev/null
      jq -e '.runner_admission_ok == 1 and .environment_ok == 1' "$reward" >/dev/null
      trial="$(dirname "$(find "$OUT/$SNAPSHOT_CASE" -name result.json -print -quit)")"
      if [ "$SNAPSHOT_CASE" = background ]; then
        cmp "$trial/main-snapshot/answer" "$trial/artifacts/agent/answer.md"
        cmp "$trial/main-snapshot/publish/updates.txt" "$trial/artifacts/logs/artifacts/updates.txt"
        test -d "$trial/main-snapshot/publish/empty"
        grep -q 'background writer' "$trial/main-snapshot/answer"
        grep -q 'published while running' "$trial/main-snapshot/publish/updates.txt"
        expected="sha256:$(shasum -a 256 "$trial/artifacts/agent/answer.md" | cut -d ' ' -f 1)"
        jq -e --arg expected "$expected" '.answer_sha256 == $expected' "$receipt" >/dev/null
        jq -e '.reward == 1' "$reward" >/dev/null
      elif [ "$SNAPSHOT_CASE" = directory ]; then
        test -d "$trial/artifacts/agent/answer.md"
        jq -e 'any(.[]; .source == "/app/answer.md" and .type == "directory" and .status == "ok")' \
          "$trial/artifacts/manifest.json" >/dev/null
        jq -e '.answer_sha256 == null' "$receipt" >/dev/null
        jq -e '.reward == 0' "$reward" >/dev/null
      else
        jq -e '.answer_sha256 == null' "$receipt" >/dev/null
        jq -e '.reward == 0' "$reward" >/dev/null
      fi
      ;;
    copy_failure|unsafe_link|lifecycle_change)
      jq -e '.phase == "failed" and .failure != null and .evidence_sha256 == null' "$receipt" >/dev/null
      if [ -n "$reward" ]; then
        jq -e '.reward == 0 and .environment_ok == 0 and .runner_admission_ok == 0' "$reward" >/dev/null
      else
        test "$run_status" != 0
      fi
      if find "$OUT/$SNAPSHOT_CASE" -path '*/artifacts/world/evidence.json' -print -quit | grep -q .; then
        printf '%s\n' 'invalid main snapshot permitted world evidence collection' >&2
        exit 1
      fi
      ;;
  esac
  printf '%s\n' "$SNAPSHOT_CASE: post-stop snapshot and admission assertions passed"
done
