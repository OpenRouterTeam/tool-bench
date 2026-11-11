#!/bin/sh
# Builds mcp-grafana from its pinned module plus the tool-bench silence
# validation-clock adaptation (see README.md in this directory).
#
#   build.sh source <dir>   write the patched module source tree to <dir>
#   build.sh binary <dir>   build <dir>/mcp-grafana from that tree
#
# The module is fetched through the Go module proxy/checksum database and must
# also match the pinned h1 sum below; the patch must apply exactly.
set -eu

MODULE=github.com/grafana/mcp-grafana/v2
VERSION=v2.1.1
# go.sum h1 hash of the module zip; origin tag v2.1.1 = commit
# 244d1adde04194ecad15125cb17159a1db50626b.
MODULE_SUM=h1:6wEZslhyIe9pYmz7xKulst56PtHl+wMrLQmb8aC1Rx8=
HERE=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)

usage() { echo "usage: $0 source|binary <dir>" >&2; exit 2; }
[ $# -eq 2 ] || usage
mode=$1
target=$2
case "$mode" in source|binary) ;; *) usage ;; esac

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT INT TERM

# Resolve the pinned module outside any Go module so no local go.mod applies.
info=$(cd "$work" && GOFLAGS=-modcacherw go mod download -json "$MODULE@$VERSION")
dir=$(printf '%s\n' "$info" | sed -n 's/^[[:space:]]*"Dir": "\(.*\)",$/\1/p')
sum=$(printf '%s\n' "$info" | sed -n 's/^[[:space:]]*"Sum": "\(.*\)",$/\1/p')
[ "$sum" = "$MODULE_SUM" ] || { echo "grafana adapter: $MODULE@$VERSION sum $sum != $MODULE_SUM" >&2; exit 1; }
[ -n "$dir" ] && [ -f "$dir/tools/alerting_silences.go" ] || { echo "grafana adapter: module source missing" >&2; exit 1; }

src="$work/src"
cp -R "$dir" "$src"
chmod -R u+w "$src"
[ ! -e "$src/tools/toolbench_validation_clock.go" ] || { echo "grafana adapter: seam file already present upstream" >&2; exit 1; }
# git apply works outside a repository; --check first so nothing is half-applied.
(cd "$src" && git apply --check "$HERE/validation-clock.patch" && git apply "$HERE/validation-clock.patch")
cp "$HERE/toolbench_validation_clock.go" "$src/tools/toolbench_validation_clock.go"

case "$mode" in
  source)
    mkdir -p "$target"
    cp -R "$src/." "$target/"
    ;;
  binary)
    mkdir -p "$target"
    out=$(CDPATH= cd -- "$target" && pwd)
    # Same main package and dependency graph (the module's own go.mod/go.sum,
    # read-only) as `go install $MODULE/cmd/mcp-grafana@$VERSION`. The version
    # variable is set to the pinned tag, which `go install @version` would
    # otherwise report from build info, so serverInfo and User-Agent stay
    # mcp-grafana/v2.1.1; the adaptation is declared in the catalog source.
    (cd "$src" && go build -mod=readonly -trimpath \
      -ldflags "-X $MODULE.version=$VERSION" \
      -o "$out/mcp-grafana" ./cmd/mcp-grafana)
    ;;
esac
