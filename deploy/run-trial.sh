#!/bin/sh
set -eu
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
if [ -z "${HARBOR_PYTHON:-}" ]; then
  harbor="$(command -v harbor)"
  interpreter="$(head -n 1 "$harbor")"
  case "$interpreter" in
    '#!/'*) HARBOR_PYTHON="${interpreter#\#!}" ;;
    *) printf '%s\n' 'set HARBOR_PYTHON to the Python interpreter with Harbor 0.23.0 installed' >&2; exit 2 ;;
  esac
fi
export PYTHONPATH="$ROOT${PYTHONPATH:+:$PYTHONPATH}"
export PYTHONDONTWRITEBYTECODE=1
exec "$HARBOR_PYTHON" -m deploy.runner.run "$@"
