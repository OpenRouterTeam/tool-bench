#!/bin/sh
set -eu
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
mkdir -p "$WORK/bin"
for command in docker ori bun cargo; do
  cat > "$WORK/bin/$command" <<'COMMAND'
#!/bin/sh
printf 'called\n' > "$HOST_GUARD_MARKER"
exit 99
COMMAND
  chmod +x "$WORK/bin/$command"
done
for script in smoke-ori.sh smoke-arms.sh; do
  status=0
  ALLOW_UNSAFE_HOST_AGENT=0 HOST_GUARD_MARKER="$WORK/called" PATH="$WORK/bin:$PATH" \
    "$ROOT/deploy/$script" code/github/file-login-bug > "$WORK/output" 2>&1 || status=$?
  test "$status" = 2
  test ! -e "$WORK/called"
  grep -q 'does not isolate file-reading tools' "$WORK/output"
done
. "$ROOT/deploy/host-agent-guard.sh"
ALLOW_UNSAFE_HOST_AGENT=1 guard_host_agent
printf 'host-agent guard: both entrypoints stop before agent or infrastructure commands\n'
