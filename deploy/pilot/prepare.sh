#!/usr/bin/env bash
# prepare.sh [--world-image TAG] [--tools-image TAG] [--out DIR]
#
# Builds everything the pilot harness needs from the current tree:
#   - the world image (default tag tool-bench-world:pilot),
#   - target/debug/bench-verify, the grader grade.sh runs on the host,
#   - the generated tasks in DIR (default /tmp/tb-pilot/tasks), which must be outside the repo.
# The tools image (default tool-bench-tools:e45) is reused as it is and must already exist.
# Safe to re-run: the image is retagged and the tasks (generated beside DIR) are swapped in
# only after every build step has succeeded, and running slots keep the image and task
# inputs they started with.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
world_image=tool-bench-world:pilot
tools_image=tool-bench-tools:e45
out=/tmp/tb-pilot/tasks
die() { printf 'prepare.sh: %s\n' "$*" >&2; exit 2; }

while [ $# -gt 0 ]; do
  case "$1" in
    --world-image|--tools-image|--out)
      [ $# -ge 2 ] && [ -n "$2" ] || die "$1 needs a value"
      case "$1" in
        --world-image) world_image="$2" ;;
        --tools-image) tools_image="$2" ;;
        --out) out="$2" ;;
      esac
      shift 2 ;;
    *) die "usage: prepare.sh [--world-image TAG] [--tools-image TAG] [--out DIR]" ;;
  esac
done

case "$out" in /*) ;; *) out="$PWD/$out" ;; esac
out="${out%/}"
mkdir -p "$(dirname "$out")"
out="$(cd "$(dirname "$out")" && pwd -P)/$(basename "$out")"
case "$out/" in "$(cd "$ROOT" && pwd -P)"/*) die "--out must be outside the repository ($ROOT)" ;; esac
# Only ever replace a directory that holds nothing but generated tasks.
if [ -d "$out" ]; then
  for entry in "$out"/* "$out"/.[!.]*; do
    [ -e "$entry" ] || continue
    [ -f "$entry/task.toml" ] || die "$out holds $entry, which is not a generated task; refusing to replace it"
  done
fi
docker image inspect "$tools_image" > /dev/null 2>&1 \
  || die "tools image $tools_image does not exist locally; build it or pass --tools-image"

cd "$ROOT"
# Order matters: everything that can fail on a tree that does not build runs before anything
# is replaced, and the image is tagged and the tasks swapped in only at the end.
printf 'building target/debug/bench-verify\n'
cargo build -q -p mock-core --bin bench-verify \
  || die "bench-verify did not build; the image and tasks are unchanged"

printf 'generating tasks\n'
staging="$out.staging.$$"
retired="$out.retired.$$"
trap 'rm -rf "$staging"' EXIT
cargo run -q -p scenario --bin taskgen -- --out "$staging" \
  --world-image "$world_image" --tools-image "$tools_image" > /dev/null \
  || die "taskgen failed; the image and tasks are unchanged"

printf 'building %s from %s\n' "$world_image" "$ROOT"
docker build -q -f images/world/Dockerfile --label tool-bench.pilot=world -t "$world_image" . > /dev/null \
  || die "the world image did not build; the image and tasks are unchanged"
# Earlier pilot builds lose their tag here. Remove those that no container uses; the label
# keeps this away from every image this script did not build.
docker image prune -f --filter label=tool-bench.pilot=world > /dev/null

if [ -d "$out" ]; then
  mv "$out" "$retired"
fi
mv "$staging" "$out"
rm -rf "$retired"

printf 'world image: %s %s\n' "$world_image" "$(docker image inspect "$world_image" --format '{{.Id}}')"
printf 'tools image: %s %s\n' "$tools_image" "$(docker image inspect "$tools_image" --format '{{.Id}}')"
mkdir -p "${PILOT_HOME:-/tmp/tb-pilot}/bin"
cp "$ROOT/target/debug/bench-verify" "${PILOT_HOME:-/tmp/tb-pilot}/bin/bench-verify"
printf 'bench-verify: %s (pinned copy of target/debug/bench-verify)\n' "${PILOT_HOME:-/tmp/tb-pilot}/bin/bench-verify"
printf 'tasks: %s in %s\n' "$(find "$out" -mindepth 1 -maxdepth 1 -type d | wc -l | tr -d ' ')" "$out"
