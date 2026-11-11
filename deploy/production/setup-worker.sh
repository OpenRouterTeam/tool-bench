#!/bin/sh
# Extract services/gcp-harbor-trial at WORKER_REF and install it, with the versions its uv.lock
# pins, into an isolated venv. Mirrors the worker Dockerfile's `uv sync --frozen --no-dev
# --no-editable`. Read-only towards openrouter-web (git rev-parse + git archive only).
set -eu
. "$(dirname "$0")/env.sh"
mkdir -p "$WORK"
if [ ! -d "$WORKER_PKG" ]; then
  mkdir -p "$WORKER_SRC"
  git -C "$OPENROUTER_WEB" archive "$WORKER_SHA" services/gcp-harbor-trial | tar -x -C "$WORKER_SRC"
fi
UV_PROJECT_ENVIRONMENT="$VENV" uv sync --frozen --no-dev --no-editable \
  --project "$WORKER_PKG" --python "$PYTHON"
"$VENV/bin/python" - <<'PY'
import importlib.metadata as m, gcp_harbor_trial, sys
print("python", sys.version.split()[0])
for name in ("gcp-harbor-trial", "harbor", "pyarrow", "google-cloud-storage"):
    print(name, m.version(name))
print("worker module", gcp_harbor_trial.__file__)
PY
echo "worker ref $WORKER_REF = $WORKER_SHA"
