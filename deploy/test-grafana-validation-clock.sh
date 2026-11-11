#!/bin/sh
# Behavioral checks for the mcp-grafana silence validation-clock adaptation
# (servers/extensions/grafana). Needs Go, python3, git and the Go module proxy
# (or a warm module cache). Contacts no Grafana service or model.
set -eu
ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
ADAPTER="$ROOT/servers/extensions/grafana"
FIXTURES="$ROOT/deploy/fixtures/grafana-validation-clock"
MODULE=github.com/grafana/mcp-grafana/v2
VERSION=v2.1.1
CLOCK_ENV=TOOL_BENCH_GRAFANA_SILENCE_VALIDATION_NOW
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT INT TERM
fail() { echo "FAIL $*" >&2; exit 1; }
gotest() { (cd "$1" && shift && env -u "$CLOCK_ENV" go test -count=1 "$@"); }

echo "1. patched tree = pinned module + one declared seam"
"$ADAPTER/build.sh" source "$work/src"
pristine=$(cd "$work" && go mod download -json "$MODULE@$VERSION" | sed -n 's/^[[:space:]]*"Dir": "\(.*\)",$/\1/p')
[ -d "$pristine" ] || fail "pinned module directory not found"
expected="Files $pristine/tools/alerting_silences.go and $work/src/tools/alerting_silences.go differ
Only in $work/src/tools: toolbench_validation_clock.go"
[ "$(diff -rq "$pristine" "$work/src" || true)" = "$expected" ] || fail "unexpected source changes: $(diff -rq "$pristine" "$work/src" || true)"
removed=$(diff "$pristine/tools/alerting_silences.go" "$work/src/tools/alerting_silences.go" | grep '^<' || true)
[ "$removed" = '< 	if !endsAt.After(time.Now()) {' ] || fail "patch removed more than the silence check: $removed"
echo "ok   only tools/alerting_silences.go:154 changed; seam file added"

echo "2. behavioral tests in the pinned tools package"
cp "$FIXTURES/toolbench_validation_clock_test.go" "$work/src/tools/"
gotest "$work/src" ./tools -run '^TestToolBench'
echo "ok   seam, validation, handler, TLS and auth tests"

echo "3. upstream silence unit tests unchanged with the clock unset"
gotest "$work/src" ./tools -run '^(TestManageSilencesParams_Validate|TestManageSilencesReadParams_Validate|TestMatcherToFilterString|TestBuildSilenceFilters|TestToPostableSilence)$'
echo "ok   upstream tests"

echo "4. mutation: the same tests fail without the one-line patch"
cp -R "$pristine" "$work/unpatched"
chmod -R u+w "$work/unpatched"
cp "$ADAPTER/toolbench_validation_clock.go" "$FIXTURES/toolbench_validation_clock_test.go" "$work/unpatched/tools/"
if gotest "$work/unpatched" ./tools -run '^TestToolBench(AuthoredWindowAgainstDeclaredInstant|HandlerRejectsBeforeHTTP)$' >"$work/mutation.log" 2>&1; then
  fail "clock tests pass without the patch"
fi
grep -q -- '--- FAIL: TestToolBench' "$work/mutation.log" || { cat "$work/mutation.log" >&2; fail "mutation did not fail in the clock tests"; }
echo "ok   unpatched source is detected"

echo "5. built binary: dependency graph and stdio surface"
"$ADAPTER/build.sh" binary "$work/bin"
(cd "$work" && GOBIN="$work/ref" GOFLAGS=-modcacherw go install "$MODULE/cmd/mcp-grafana@$VERSION")
go version -m "$work/bin/mcp-grafana" | grep -E '^[[:space:]]+(dep|=>)' >"$work/adapted.deps"
go version -m "$work/ref/mcp-grafana" | grep -E '^[[:space:]]+(dep|=>)' >"$work/reference.deps"
cmp -s "$work/adapted.deps" "$work/reference.deps" || { diff "$work/reference.deps" "$work/adapted.deps" >&2; fail "dependency graph differs from go install @$VERSION"; }
echo "ok   $(wc -l <"$work/adapted.deps" | tr -d ' ') dependencies equal go install $MODULE@$VERSION"
python3 -I "$FIXTURES/stdio_probe.py" "$work/bin/mcp-grafana" "$ROOT/servers/snapshots/grafana.json"
echo "PASS grafana validation clock"
