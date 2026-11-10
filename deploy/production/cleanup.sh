#!/bin/sh
# Remove what this rehearsal created in Docker and nothing else. Never prunes.
#  - containers/networks whose compose project is one of this rehearsal's attempt ids (only
#    present if a trial was interrupted; finished trials remove their own)
#  - the worker-patched egress sidecar image (harbor-prebuilt:...-sidecar-read-timeout--<hash>),
#    which only the production worker's environment class builds
# Pass --files to also delete $WORK/plans and $WORK/runs (the venv and worker source stay).
set -eu
HERE=$(cd "$(dirname "$0")" && pwd)
. "$HERE/env.sh"
attempts=$(find "$WORK/plans" -path '*/plan/trial-configs/*.json' 2>/dev/null | sed 's|.*/||; s|\.json$||' | sort -u)
for attempt in $attempts; do
  for prefix in "${attempt}__" "mcp-${attempt}__"; do
    # Harbor's projects are <attempt>__env and <attempt>__verifier__trial (hashed when long).
    docker ps -a --format '{{.ID}} {{.Label "com.docker.compose.project"}}' \
      | awk -v p="$prefix" 'index($2, p) == 1 { print $1 }' \
      | while read -r id; do echo "removing container $id ($prefix*)"; docker rm -f "$id" >/dev/null; done
    docker network ls --format '{{.ID}} {{.Label "com.docker.compose.project"}}' \
      | awk -v p="$prefix" 'index($2, p) == 1 { print $1 }' \
      | while read -r id; do echo "removing network $id ($prefix*)"; docker network rm "$id" >/dev/null; done
  done
done
docker images --format '{{.Repository}}:{{.Tag}}' \
  | grep '^harbor-prebuilt:harbor-docker-egress-control-sidecar-read-timeout--' \
  | while read -r image; do echo "removing image $image"; docker rmi "$image" >/dev/null; done
if [ "${1:-}" = "--files" ]; then rm -rf "$WORK/plans" "$WORK/runs"; fi
echo "cleanup done"
