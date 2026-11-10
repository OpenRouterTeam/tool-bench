#!/bin/sh
# Offline capture of every MCP endpoint exactly as an agent sees it:
#   world (mock APIs + TLS) <- tools (supervisor: bridges + replicas) <- toolmeter over HTTP.
# The network is --internal, so nothing can reach the internet. The world runs protected, as in
# a generated task: a per-run admin token, API calls accepted from tools only, and the CA served
# under the name world-bootstrap, where the tools image looks for it.
#   deploy/capture-stack.sh [out-dir] [endpoint-id ...]   (default: servers/snapshots, all)
# A full capture also rewrites crates/world/startup-calls.json.
set -eu
OUT="${1:-servers/snapshots}"
case "$OUT" in /*) ;; *) OUT="$(cd "$(dirname "$0")/.." && pwd)/$OUT" ;; esac
[ $# -gt 0 ] && shift
ONLY="$*"
NET=tool-bench-capture
WORLD_IMAGE=${WORLD_IMAGE:-tool-bench-world:dev}
TOOLS_IMAGE=${TOOLS_IMAGE:-tool-bench-tools:dev}
TOKEN="$(openssl rand -hex 16)"
cleanup() { docker rm -f tb-world tb-tools >/dev/null 2>&1 || true; docker network rm $NET >/dev/null 2>&1 || true; }
trap cleanup EXIT
cleanup
docker network create --internal $NET >/dev/null

aliases="--network-alias world --network-alias world-bootstrap"
for host in $(docker run --rm --entrypoint catalog "$TOOLS_IMAGE" --file /etc/tool-bench/services.toml aliases); do
  aliases="$aliases --network-alias $host"
done
docker run -d --name tb-world --network $NET $aliases -e SCENARIO_ID=example/github/basic \
  -e BENCH_ADMIN_TOKEN="$TOKEN" -e BENCH_API_CALLER_HOST=tools \
  -e BENCH_BOOTSTRAP_BIND_HOST=world-bootstrap "$WORLD_IMAGE" >/dev/null
# A world that did not start says why in its log; without it every endpoint would time out.
i=0
until docker exec tb-world curl -fs http://localhost:9001/_bench/health >/dev/null 2>&1; do
  i=$((i + 1))
  if [ $i -ge 60 ] || [ "$(docker inspect -f '{{.State.Running}}' tb-world)" != true ]; then
    docker logs tb-world >&2; echo "world did not become healthy" >&2; exit 1
  fi
  sleep 1
done
docker run -d --name tb-tools --network $NET --network-alias tools "$TOOLS_IMAGE" >/dev/null

mkdir -p "$OUT"
docker run --rm --network $NET -e ONLY="$ONLY" -v "$OUT:/out" --entrypoint sh "$TOOLS_IMAGE" -c '
  set -u
  failures=0
  endpoints=$(catalog --file /etc/tool-bench/services.toml endpoints)
  while IFS="$(printf "\t")" read -r id runtime pkg ver; do
    if [ -n "$ONLY" ] && ! echo " $ONLY " | grep -q " $id "; then continue; fi
    port=$(catalog --file /etc/tool-bench/services.toml port "$id")
    for i in $(seq 1 60); do curl -s -o /dev/null http://tools:$port/mcp && break; sleep 0.5; done
    if ! toolmeter capture --name "$id" --out /out --http "http://tools:$port/mcp"; then
      echo "$id: FAILED"; failures=$((failures + 1))
    fi
  done <<EOF
$endpoints
EOF
  [ "$failures" = 0 ] || { echo "$failures endpoint(s) failed"; exit 1; }'
if [ -z "$ONLY" ]; then
  # What the servers requested on their own while every endpoint was initialized and listed;
  # grading excludes these startup requests from the agent's calls. The admin API is on the
  # world's loopback only; fetched first so a failed request cannot leave the file empty.
  journal=$(docker exec tb-world curl -fs -H "Authorization: Bearer $TOKEN" http://localhost:9001/_bench/journal)
  printf '%s\n' "$journal" \
    | jq '[.entries[] | select(.path | startswith("/_bench") | not) | {service, method, path, query}] | sort' \
    > "$(cd "$(dirname "$0")/.." && pwd)/crates/world/startup-calls.json"
fi
echo "--- tools container log (errors only)"
docker logs tb-tools 2>&1 | grep -iE "error|panic|failed" | head -40 || true
