# Shared parameters for the production-worker rehearsal. Source it; override any value from the
# environment. Nothing here writes into the tool-bench checkout, openrouter-web, or TASKS_SRC.
: "${OPENROUTER_WEB:?set OPENROUTER_WEB to a checkout of openrouter-web}"   # read-only: git rev-parse + git archive
: "${WORKER_REF:=origin/main}"                             # ref whose services/gcp-harbor-trial runs
: "${TASKS_SRC:=/tmp/tb-stock/tasks}"                      # generated tasks (copied, never edited)
: "${WORLD_IMAGE:=tool-bench-world:dev}"               # images the tasks must reference
: "${TOOLS_IMAGE:=tool-bench-tools:dev}"
: "${WORK:=/tmp/tb-prod-worker}"                           # everything this rehearsal creates
: "${PYTHON:=$(command -v python3.13 || command -v python3.12 || command -v python3.11 || command -v python3)}"

WORKER_SHA=$(git -C "$OPENROUTER_WEB" rev-parse "$WORKER_REF")
WORKER_SRC="$WORK/src-$(printf %s "$WORKER_SHA" | cut -c1-8)"
WORKER_PKG="$WORKER_SRC/services/gcp-harbor-trial"
VENV="$WORK/venv"
RUNS="$WORK/runs"   # <run-id>/trials/<trial>/<retry>/<attempt-id>/; plans are in $WORK/plans/<run-id>/
export OPENROUTER_WEB WORKER_REF TASKS_SRC WORLD_IMAGE TOOLS_IMAGE WORK PYTHON
export WORKER_SHA WORKER_SRC WORKER_PKG VENV RUNS
# Scripts here are imported from the checkout (probe agent); never leave __pycache__ in it.
export PYTHONDONTWRITEBYTECODE=1
