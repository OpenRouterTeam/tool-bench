#!/bin/sh
set -eu
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
WORLD_IMAGE="${WORLD_IMAGE:?provide the current world image}"
TOOLS_IMAGE="${TOOLS_IMAGE:?provide the tools image}"
OUT="$(mktemp -d /tmp/tool-bench-admission.XXXXXX)"
printf 'artifacts: %s\n' "$OUT"
export PYTHONPATH="$ROOT${PYTHONPATH:+:$PYTHONPATH}"
export PYTHONDONTWRITEBYTECODE=1
cargo run -q --locked -p scenario --bin taskgen -- --runner-admission required --out "$OUT/tasks" \
  --world-image "$WORLD_IMAGE" --tools-image "$TOOLS_IMAGE" >/dev/null
TASK="$OUT/tasks/business__stripe__customer-mrr"
mkdir -p "$OUT/bin"
export REAL_DOCKER
REAL_DOCKER="$(command -v docker)"
cat > "$OUT/bin/docker" <<'EOF'
#!/bin/sh
set -eu
stop=0
main=0
for argument in "$@"; do
  [ "$argument" != stop ] || stop=1
  [ "$argument" != main ] || main=1
done
if [ "$stop" = 1 ] && [ "$main" = 1 ]; then
  if [ "${STOP_BEHAVIOR:-}" = false_success ]; then
    exit 0
  fi
  printf '%s\n' 'injected main shutdown failure' >&2
  exit 77
fi
exec "$REAL_DOCKER" "$@"
EOF
chmod +x "$OUT/bin/docker"
for mode in strict stop_failure false_success unsupported missing_controller; do
  jobs="$OUT/$mode"
  adapter=deploy.runner.strict_docker:StrictDockerEnvironment
  search_path="$PATH"
  [ "$mode" != unsupported ] || adapter=docker
  case "$mode" in stop_failure|false_success) search_path="$OUT/bin:$PATH" ;; esac
  case "$mode" in
    unsupported|missing_controller) set -- harbor run --debug -q --env "$adapter" ;;
    *) set -- sh deploy/run-trial.sh ;;
  esac
  if PATH="$search_path" STOP_BEHAVIOR="$mode" HARBOR_TRIAL_ADMIN_TOKEN="$(openssl rand -hex 32)" \
      "$@" -p "$TASK" -a oracle \
      -o "$jobs" > "$OUT/$mode.log" 2>&1; then
    run_status=0
  else
    run_status=$?
  fi
  reward="$(find "$jobs" -path '*/verifier/reward.json' -print -quit 2>/dev/null || true)"
  case "$mode" in
    stop_failure|false_success)
      test "$run_status" != 0
      test -z "$reward"
      receipt="$(find "$jobs" -name runner-admission.json -print -quit)"
      test -n "$receipt"
      jq -e '.phase == "failed" and .failure != null and .evidence_sha256 == null
        and .main_artifacts_sha256 == null' "$receipt" >/dev/null
      if find "$jobs" -path '*/artifacts/world/evidence.json' -print -quit | grep -q .; then
        printf '%s\n' 'world evidence was collected after failed main shutdown' >&2
        exit 1
      fi
      printf '%s\n' "$mode: failed lifecycle hook cannot produce a scored assessment"
      continue
      ;;
  esac
  if [ -z "$reward" ]; then
    printf '%s\n' "$mode: no verifier assessment (runner exit $run_status)" >&2
    tail -60 "$OUT/$mode.log" >&2
    exit 1
  fi
  case "$mode" in
    strict)
      test "$run_status" = 0
      jq -e '.reward == 1 and .runner_admission_checked == 1
        and .runner_admission_ok == 1 and .environment_ok == 1' "$reward" >/dev/null
      receipt="$(find "$jobs" -path '*/artifacts/world/runner-admission.json' -print -quit)"
      test -n "$receipt"
      jq -e '.phase == "stopped" and .failure == null and .main.running == false
        and .collection_checks > 0 and (.evidence_sha256 | startswith("sha256:"))
        and (.main_artifacts_sha256 | startswith("sha256:"))
        and (.answer_sha256 | startswith("sha256:"))' "$receipt" >/dev/null
      ;;
    missing_controller|unsupported)
      jq -e '.reward == 0 and .runner_admission_checked == 1
        and .runner_admission_ok == 0 and .environment_ok == 0 and .outcome_code == 5' "$reward" >/dev/null
      if [ "$mode" = missing_controller ]; then
        receipt="$(find "$jobs" -mindepth 3 -maxdepth 3 -name runner-admission.json -print -quit)"
        test -n "$receipt"
        jq -e '.phase == "failed" and .failure != null and .evidence_sha256 == null' "$receipt" >/dev/null
        if find "$jobs" -path '*/artifacts/world/evidence.json' -print -quit | grep -q .; then
          printf '%s\n' 'world evidence was collected after failed main shutdown' >&2
          exit 1
        fi
      fi
      ;;
  esac
  printf '%s\n' "$mode: runner admission behaved as required"
done
