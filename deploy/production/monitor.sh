#!/bin/sh
# Observe Docker while trials run: read-only (docker events + docker stats), never touches
# containers. usage: monitor.sh start <dir> | monitor.sh stop <dir>
#   <dir>/events.jsonl  container/image/network events (all projects; filter by project later)
#   <dir>/stats.tsv     epoch <TAB> container name <TAB> memory usage, sampled about every 2s
set -eu
cmd="$1"; dir="$2"
case "$cmd" in
  start)
    mkdir -p "$dir"
    docker events --format '{{json .}}' \
      --filter type=container --filter type=image --filter type=network \
      >> "$dir/events.jsonl" 2>/dev/null &
    echo $! > "$dir/events.pid"
    (
      while :; do
        now=$(date +%s)
        docker stats --no-stream --format '{{.Name}}	{{.MemUsage}}' 2>/dev/null \
          | sed "s/^/$now	/" >> "$dir/stats.tsv" || true
        sleep 1
      done
    ) &
    echo $! > "$dir/stats.pid"
    ;;
  stop)
    for name in events stats; do
      if [ -f "$dir/$name.pid" ]; then
        kill "$(cat "$dir/$name.pid")" 2>/dev/null || true
        rm -f "$dir/$name.pid"
      fi
    done
    ;;
  *) echo "usage: $0 start|stop <dir>" >&2; exit 2 ;;
esac
