#!/bin/sh
set -eu
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
mkdir -p "$WORK/bin"
cat > "$WORK/bin/docker" <<'EOF'
#!/bin/sh
printf '%s\n' sha256:test-image
EOF
cat > "$WORK/bin/cargo" <<'EOF'
#!/bin/sh
set -eu
while [ "$#" -gt 0 ]; do
  if [ "$1" = --out ]; then
    mkdir -p "$2/code__github__file-login-bug"
    exit 0
  fi
  shift
done
exit 1
EOF
cat > "$WORK/bin/runner-python" <<'EOF'
#!/bin/sh
set -eu
while [ "$#" -gt 0 ]; do
  case "$1" in
    -a) agent="$2"; shift ;;
    -o) jobs="$2"; shift ;;
  esac
  shift
done
mkdir -p "$jobs/trial/verifier"
reward=1
environment=1
contract=1
integrity=1
admission_checked=1
admission_ok=1
adjudication=0
if [ "$agent" = nop ]; then
  reward=0
  case "$MODE" in
    valid) ;;
    environment_error) environment=0 ;;
    contract_error) contract=0 ;;
    integrity_error) integrity=0 ;;
    admission_unchecked) admission_checked=0 ;;
    admission_error) admission_ok=0 ;;
    unresolved) adjudication=1 ;;
    missing) exit 0 ;;
    missing_metadata) printf '%s\n' '{"reward":0}' > "$jobs/trial/verifier/reward.json"; exit 0 ;;
    grader_error) printf '%s\n' '{"reward":0,"environment_ok":1,"contract_ok":1,"integrity_ok":1,"grader_error":1}' > "$jobs/trial/verifier/reward.json"; exit 0 ;;
    malformed) printf '%s\n' '{' > "$jobs/trial/verifier/reward.json"; exit 0 ;;
    not_binary) reward=0.5 ;;
    crash) printf '%s\n' '{"reward":0,"environment_ok":1,"contract_ok":1,"integrity_ok":1}' > "$jobs/trial/verifier/reward.json"; exit 1 ;;
    *) exit 2 ;;
  esac
fi
printf '{"reward":%s,"environment_ok":%s,"contract_ok":%s,"integrity_ok":%s,"runner_admission_checked":%s,"runner_admission_ok":%s,"adjudication_needed":%s}\n' \
  "$reward" "$environment" "$contract" "$integrity" "$admission_checked" "$admission_ok" "$adjudication" > "$jobs/trial/verifier/reward.json"
EOF
chmod +x "$WORK/bin/docker" "$WORK/bin/cargo" "$WORK/bin/runner-python"
for mode in valid environment_error contract_error integrity_error admission_unchecked admission_error unresolved missing missing_metadata grader_error malformed not_binary crash; do
  if PATH="$WORK/bin:$PATH" HARBOR_PYTHON="$WORK/bin/runner-python" MODE="$mode" WORLD_IMAGE=world:test TOOLS_IMAGE=tools:test \
      "$ROOT/deploy/oracle.sh" code/github/file-login-bug > "$WORK/$mode.log" 2>&1; then
    status=0
  else
    status=$?
  fi
  case "$mode:$status" in
    valid:0) ;;
    valid:*|*:0) cat "$WORK/$mode.log" >&2; exit 1 ;;
  esac
  artifacts="$(sed -n 's/^artifacts: //p' "$WORK/$mode.log")"
  case "$artifacts" in /tmp/tool-bench-oracle.*) rm -rf "$artifacts" ;; esac
done
printf '%s\n' 'oracle runner rejects invalid no-op assessments and runner failures'
