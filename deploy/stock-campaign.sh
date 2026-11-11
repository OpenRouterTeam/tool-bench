#!/bin/sh
# Runs every task through stock Harbor, the way the production worker does: no strict
# controller, a fresh admin token per trial, the offline verifier in its unattested tier.
# The scripted oracle agent must score 1 and the no-op agent 0 on every task.
#
#   deploy/stock-campaign.sh [--agents "oracle nop"] [--parallel 4] [--only GLOB] [--out DIR]
#
# Images default to tool-bench-world:campaign (built from this checkout) and
# tool-bench-tools:dev; set WORLD_IMAGE or TOOLS_IMAGE to reuse prebuilt ones.
# A trial that never reached the verifier (a Docker or host fault) is retried once and
# reported separately; it is not a benchmark failure.
set -eu
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
AGENTS="oracle nop"
PARALLEL=4
ONLY='*'
OUT=""
while [ $# -gt 0 ]; do
  case "$1" in
    --agents) AGENTS="$2"; shift 2 ;;
    --parallel) PARALLEL="$2"; shift 2 ;;
    --only) ONLY="$2"; shift 2 ;;
    --out) OUT="$2"; shift 2 ;;
    *) echo "unexpected argument $1" >&2; exit 2 ;;
  esac
done
command -v harbor >/dev/null || { echo 'harbor 0.23.0 is required' >&2; exit 2; }
command -v jq >/dev/null || { echo 'jq is required' >&2; exit 2; }
[ -n "$OUT" ] || OUT="$(mktemp -d /tmp/tool-bench-stock.XXXXXX)"
mkdir -p "$OUT/jobs"
PREBUILT_WORLD="${WORLD_IMAGE:-}"
WORLD_IMAGE="${WORLD_IMAGE:-tool-bench-world:campaign}"
TOOLS_IMAGE="${TOOLS_IMAGE:-tool-bench-tools:dev}"
[ -n "$PREBUILT_WORLD" ] || docker build -q -f images/world/Dockerfile -t "$WORLD_IMAGE" . >/dev/null
CARGO_INCREMENTAL=0 cargo run -q --locked -p scenario --bin taskgen -- \
  --out "$OUT/tasks" --world-image "$WORLD_IMAGE" --tools-image "$TOOLS_IMAGE" >/dev/null
printf 'artifacts: %s\nworld_image: %s\ntools_image: %s\n' "$OUT" "$WORLD_IMAGE" "$TOOLS_IMAGE"

cat > "$OUT/trial.sh" <<'EOF'
#!/bin/sh
# trial.sh <out> <task> <agent>: one stock Harbor trial, one summary line.
OUT="$1"; TASK="$2"; AGENT="$3"
JOB="$OUT/jobs/$TASK-$AGENT"
rm -rf "$JOB"
HARBOR_TRIAL_ADMIN_TOKEN="$(openssl rand -hex 16)" \
  harbor run -q -p "$OUT/tasks/$TASK" -a "$AGENT" -e docker -o "$JOB" >"$JOB.log" 2>&1
reward="$(find "$JOB" -path '*verifier/reward.json' -print -quit 2>/dev/null)"
withheld="$(find "$JOB" -path '*verifier/reward.invalid.json' -print -quit 2>/dev/null)"
if [ -n "$reward" ]; then
  jq -r --arg task "$TASK" --arg agent "$AGENT" \
    '"\($task) \($agent) reward=\(.reward | floor) outcome=\(.outcome_code | floor) attested=\(.runner_admission_checked | floor)"' "$reward"
elif [ -n "$withheld" ]; then
  jq -r --arg task "$TASK" --arg agent "$AGENT" \
    '"\($task) \($agent) withheld outcome=\(.outcome_code | floor)"' "$withheld"
else
  printf '%s %s no-verifier-output\n' "$TASK" "$AGENT"
fi
EOF
chmod +x "$OUT/trial.sh"

find "$OUT/tasks" -mindepth 1 -maxdepth 1 -type d -name "$ONLY" -exec basename {} \; | sort >"$OUT/tasks.txt"
status=0
for agent in $AGENTS; do
  results="$OUT/$agent.txt"
  xargs -P "$PARALLEL" -I{} "$OUT/trial.sh" "$OUT" {} "$agent" <"$OUT/tasks.txt" >"$results"
  # Host faults (no verifier output) get one sequential retry.
  grep ' no-verifier-output$' "$results" | cut -d' ' -f1 >"$OUT/$agent.retry" || true
  if [ -s "$OUT/$agent.retry" ]; then
    grep -v ' no-verifier-output$' "$results" >"$results.kept" || true
    while read -r task; do "$OUT/trial.sh" "$OUT" "$task" "$agent"; done <"$OUT/$agent.retry" >>"$results.kept"
    mv "$results.kept" "$results"
  fi
  want=1; [ "$agent" = oracle ] || want=0
  total="$(wc -l <"$results" | tr -d ' ')"
  good="$(grep -c " reward=$want " "$results" || true)"
  printf '%s: %s of %s scored %s\n' "$agent" "$good" "$total" "$want"
  if [ "$good" != "$total" ]; then
    grep -v " reward=$want " "$results" | sed 's/^/  /'
    status=1
  fi
done
exit "$status"
