#!/usr/bin/env bash
# down.sh <slot>
#
# Removes compose project tbpilot-<slot> (its containers, networks and volumes) and nothing
# else. The slot directory, with any verifier output, is kept until the slot is reused.
set -euo pipefail
PILOT_HOME=/tmp/tb-pilot
die() { printf 'down.sh: %s\n' "$*" >&2; exit 2; }

[ $# -eq 1 ] || die "usage: down.sh <slot>"
slot="$1"
case "$slot" in ''|*[!0-9]*) die "slot must be a number from 0 to 19" ;; esac
slot=$((10#$slot))
[ "$slot" -le 19 ] || die "slot must be a number from 0 to 19"
project="tbpilot-$slot"
slot_dir="$PILOT_HOME/slots/$slot"

# With no compose file, compose finds the project's resources by their labels. The short
# stop timeout is safe: nothing in a slot needs a graceful shutdown.
cd /
if ! docker compose -p "$project" down -v --remove-orphans -t 1 > /dev/null 2>&1; then
  docker compose -p "$project" down -v --remove-orphans -t 1
fi
left="$(docker ps -aq --filter "label=com.docker.compose.project=$project")"
[ -z "$left" ] || die "containers of $project are still present: $left"
rm -rf "$slot_dir/servers.json" "$slot_dir/sessions"
printf 'slot %s down: project %s removed\n' "$slot" "$project"
