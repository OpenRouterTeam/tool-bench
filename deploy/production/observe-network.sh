#!/bin/sh
# Record, while one trial runs, what network the agent container and the separate verifier
# container really have. Start it first, then run the trial; it exits after the verifier check.
#   usage: observe-network.sh <attempt-id> <out-file> [timeout-sec]
# It only inspects containers of that attempt and execs read-only probes (curl, getent,
# `network-policy show|rules`) inside them; it changes nothing and removes nothing.
set -u
attempt="$1"; out="$2"; limit="${3:-600}"
env_main="${attempt}__env-main-1"
ver_main="${attempt}__verifier__trial-main-1"
ver_side="${attempt}__verifier__trial-harbor-docker-egress-control-sidecar-1"
gateway=$(docker network inspect bridge --format '{{(index .IPAM.Config 0).Gateway}}')
probe='for u in https://openrouter.ai/ https://example.com/ http://'"$gateway"':80/; do
  printf "%s -> " "$u"; curl -sS -m 3 -o /dev/null -w "http %{http_code}\n" "$u" 2>&1 | tail -n 1; done
printf "dns openrouter.ai -> "; getent hosts openrouter.ai | head -n 1 || echo "no answer (exit $?)"'
running() { [ "$(docker inspect -f '{{.State.Running}}' "$1" 2>/dev/null)" = true ]; }
wait_for() { # container name
  while ! running "$1"; do
    [ "$(($(date +%s) - start))" -ge "$limit" ] && { echo "timeout waiting for $1" >> "$out"; exit 1; }
    sleep 0.2
  done
}
start=$(date +%s); : > "$out"

wait_for "$env_main"
{
  echo "## agent container $env_main"
  echo "network mode: $(docker inspect -f '{{.HostConfig.NetworkMode}}' "$env_main")"
  echo "networks: $(docker inspect -f '{{range $k, $v := .NetworkSettings.Networks}}{{$k}} {{end}}' "$env_main")"
  echo "egress sidecar in this project: $(docker ps --format '{{.Names}}' | grep -c "^${attempt}__env-harbor-docker-egress")"
  docker exec "$env_main" bash -c "$probe" 2>&1
  # The trial admin token must stay out of the agent container, and the world's admin port
  # must not be reachable from it (world is only on the internal backend/bootstrap networks).
  echo "admin-token variables in the agent container's own env: $(docker inspect -f '{{range .Config.Env}}{{println .}}{{end}}' "$env_main" | grep -c ADMIN_TOKEN)"
  docker exec "$env_main" bash -c 'printf "world admin port from the agent container -> "; curl -sS -m 3 -o /dev/null -w "http %{http_code}\n" http://world:9001/_bench/health 2>&1 | tail -n 1'
} >> "$out"

wait_for "$ver_side"
wait_for "$ver_main"
{
  echo "## verifier sidecar $ver_side"
  docker inspect -f '{{range .Config.Env}}{{println .}}{{end}}' "$ver_side" | grep '^EGRESS_CONTROL' || true
  echo "image: $(docker inspect -f '{{.Config.Image}}' "$ver_side")"
  docker exec "$ver_side" network-policy show 2>&1
  docker exec "$ver_side" network-policy rules 2>&1
  echo "## verifier container $ver_main"
  echo "network mode: $(docker inspect -f '{{.HostConfig.NetworkMode}}' "$ver_main") (sidecar id $(docker inspect -f '{{.Id}}' "$ver_side"))"
  echo "limits: cpus=$(docker inspect -f '{{.HostConfig.NanoCpus}}' "$ver_main") memory=$(docker inspect -f '{{.HostConfig.Memory}}' "$ver_main")"
  docker exec "$ver_main" bash -c "$probe" 2>&1
  echo "apt sources.list present: $(docker exec "$ver_main" sh -c '[ -f /etc/apt/sources.list ] && echo yes || echo no' 2>&1)"
} >> "$out"
