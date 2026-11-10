# mcp-grafana silence validation-clock adaptation

**This is not the unchanged upstream `mcp-grafana`.** The tools image builds
`github.com/grafana/mcp-grafana/v2` **v2.1.1** (module sum
`h1:6wEZslhyIe9pYmz7xKulst56PtHl+wMrLQmb8aC1Rx8=`, tag commit
`244d1adde04194ecad15125cb17159a1db50626b`) with one declared source change.
The catalog `source` field and the Dockerfile name it.

## Why

`alerting_silences_write` create/update runs `validateWritePayload`
(`tools/alerting_silences.go:154` at the pin). It rejects a window whose
`ends_at` is not after `time.Now()` before sending any HTTP request.
`ops/grafana/silence-replica-maintenance` has a fixed business clock
(`2026-11-12T15:30:00Z`) and an authored window of `2026-11-13T02:00Z`–`04:00Z`.
After 2026-11-13 04:00 UTC real time, the unchanged client would reject the
correct operation, and so would every model and the reference solution, while
the world still says November 12. That is an environment fault, not a
reasoning failure. The authored instructions, dates and business clock are
unchanged.

## What changes

- `validation-clock.patch` replaces that single `time.Now()` with
  `silenceValidationNow()`. No other line changes.
- `toolbench_validation_clock.go` (a new file in package `tools`) defines the
  seam. It reads `TOOL_BENCH_GRAFANA_SILENCE_VALIDATION_NOW`:
  - **unset**: `time.Now()`, the upstream behavior;
  - **set**: exactly that RFC3339 instant;
  - **empty or malformed**: a neutral error ("silence validation clock is
    misconfigured"). It never falls back to the real clock and never fakes
    success.
- The catalog declares the variable in `runtime_clock_env`. Generated tasks
  supply `TOOL_BENCH_SCENARIO_NOW` from their own authored business instant;
  the supervisor validates it before launching servers and copies it only into
  declared validation-clock variables. Standalone catalog launches retain the
  `2026-11-12T15:30:00Z` diagnostic default.

## What stays the same

- **Real clocks.** No faketime, `LD_PRELOAD` or global clock override.
  `time.Now` is unchanged. TLS verification (`tls.Config.Time` is nil, so Go
  uses `time.Now`), authentication, HTTP timeouts, logging and every other
  mcp-grafana time default keep the real process clock. Those defaults are the
  relative query windows in Loki, Prometheus, SQL, Tempo and others, and they
  are **not** adapted.
- **Wire and schemas.** Build-info dependencies are identical to
  `go install …/cmd/mcp-grafana@v2.1.1`. The version is stamped as `v2.1.1` via
  `-ldflags -X`, so `serverInfo` and the `User-Agent` are unchanged. The
  stdio `initialize` and `tools/list` results equal
  `servers/snapshots/grafana.json` as parsed JSON (79 tools, in order). No
  snapshot was edited.
- **Validation.** Read, list, get and delete never consult the seam. Every
  other create/update check and its order are unchanged.

## Build

`build.sh binary <dir>` downloads the pinned module and checks its sum
(through the Go checksum database and against the constant). It applies the
patch with `git apply --check` first, adds the seam file and builds
`<dir>/mcp-grafana`. `build.sh source <dir>` writes only the patched tree.

## Tests

Run `deploy/test-grafana-validation-clock.sh`. It needs no model and no
Grafana account. It checks:

1. The patched tree differs from the pinned module in exactly two files, and
   only the silence `time.Now()` line was removed.
2. Behavioral Go tests (`deploy/fixtures/grafana-validation-clock/`) run inside
   the pinned `tools` package:
   - the authored window before, at and after its end;
   - update vs create messages;
   - invalid payloads;
   - a misconfigured clock fails closed;
   - the real alerting client POSTs exactly once over TLS with the unchanged
     bearer token;
   - expired or misconfigured calls make no HTTP request;
   - a certificate valid only now is accepted with a 2100 business instant;
   - a certificate valid only at the business instant is rejected by the real
     clock.
3. The upstream silence unit tests pass with the variable unset.
4. Mutation: the clock tests fail on unpatched source.
5. The built binary's dependency graph equals `go install @v2.1.1`, and a
   stdio probe checks the snapshot equality and the per-launch clock behavior.

## Not claimed / open

- This is a source fix, **not runtime qualification.** The full tools image,
  the world/oracle trial and a native campaign have not been rerun with it.
- Each generated task supplies its own business instant, including
  `2026-10-28T15:30Z` for the Terraform/Grafana workflow. This changes the
  declared silence-validation seam only, not other client query-window defaults.
- These scenarios use the default `Fixed` world clock mode. Under
  `legacy_transport` the world clock would step past the client's fixed
  instant. The world still applies its own expiry check either way.
- This is not a vendor conformance or hosted-Grafana claim.
