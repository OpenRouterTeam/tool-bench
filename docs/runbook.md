# Developer runbook

For building, changing and checking the benchmark. Running it in production and
reading the results is in [production.md](production.md).

Needs Rust 1.99, Docker, `jq`, and Harbor 0.23.0 for anything that runs a trial.

## Images

A trial uses two images. `world` holds every mock API, the scenario datasets and
`bench-verify`. `tools` holds the 32 MCP servers.

```bash
docker build -f images/world/Dockerfile -t tool-bench-world:dev .
docker build -f images/tools/Dockerfile -t tool-bench-tools:dev .
```

Committed tasks do not use local tags. They reference
`us-docker.pkg.dev/openrouter-ci/harbor/tool-bench-{world,tools}:<tag>`, where the tag
is a hash of the files the image is built from (`crates/scenario/src/images.rs`): the
workspace manifests, `crates/`, `catalog/services.toml` and the image's own directory,
plus `scenarios/` and `deploy/runner/` for `world` and `servers/` for `tools`. Any
edit under those paths, a README included, changes the tag. `taskgen --print-images`
prints the two references. On a push to `main`,
`.github/workflows/tool-bench-images.yml` builds and pushes any tag that is not
published yet.

To run tasks against local images, generate them elsewhere with the tags overridden.
`deploy/stock-campaign.sh` and `deploy/oracle.sh` do this for you.

```bash
cargo run --locked -p scenario --bin taskgen -- --out /tmp/tasks \
  --world-image tool-bench-world:dev --tools-image tool-bench-tools:dev
```

## Regenerating tasks and the inventory

`tasks/` and `docs/coverage/scenario-inventory.json` are generated. Regenerate both
after any change to an image input, and commit them with the change.

```bash
cargo run --locked -p scenario --bin taskgen -- --inventory docs/coverage/scenario-inventory.json
cargo run --locked -p scenario --bin taskgen -- --check --inventory docs/coverage/scenario-inventory.json
cargo run --locked -p scenario --bin release-manifest -- --manifest releases/tool-bench-public-dev-1.json --freeze
```

`taskgen` validates every scenario before it writes anything
([authoring-admission.md](authoring-admission.md)). `--check` changes nothing and
fails if `tasks/` or the inventory is stale; CI runs it. `release-manifest --freeze`
rewrites the frozen task list and hashes, which `cargo test -p scenario` compares
with the sources.

These options generate a different condition. Write the result to another `--out`, or
to a dataset branch as [production.md](production.md#changing-the-size-of-the-tool-set)
describes, never over `tasks/` on `main`.

| Option | Effect |
|---|---|
| `--exposure <variant>` with `--exposure-seed`, `--exposure-token-budget` | Registers a different tool set |
| `--workspace-profile support\|engineering`, `--framing neutral\|evaluation`, `--awareness-condition <cell>` | Adds workspace files or an evaluation notice ([workspace-profiles.md](workspace-profiles.md)) |
| `--runner-admission required` | Tasks that only the strict controller can score (below). The default is `optional` |
| `--include-examples` | Also generates the unscored `example/` and `diagnostic/` scenarios |
| `--review-policy <file>` | Binds a signed-review policy ([content-review.md](content-review.md)) |

## Local checks

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
python3 -m unittest discover -s deploy/tests -p 'test_*.py'
deploy/stock-campaign.sh
```

`deploy/stock-campaign.sh` runs every task through stock Harbor the way production
does, once with the scripted oracle agent and once with the no-op agent. It fails
unless the oracle scores 1 and the no-op scores 0 everywhere. It builds
`tool-bench-world:campaign` from the checkout and expects `tool-bench-tools:dev`; set
`WORLD_IMAGE` or `TOOLS_IMAGE` to use others.

```bash
deploy/stock-campaign.sh --only 'code__github__*' --agents oracle --parallel 2 --out /tmp/campaign
```

Each trial lands under `<out>/jobs/<task>-<agent>/`. Its `verifier/` directory is
read the same way as a production trial's
([production.md](production.md#reading-one-trial)).

To see how the grader reads candidate replies to a lookup task, without running a
world:

```bash
cargo run -p mock-core --bin bench-answer-probe -- scenarios replies.json
```

`replies.json` maps a scenario id to `[{"answer": "...", "expect": "pass" | "reject"}]`.
The command exits non-zero when a verdict disagrees with `expect`.

A world on its own, without Harbor or credentials, for poking at one mock:

```bash
BENCH_ALLOW_INSECURE_LOCAL=1 SCENARIO_DIR=scenarios/example/github/basic cargo run --locked -p world
curl localhost:8101/api/v3/repos/octo-org/app/issues
curl localhost:9000/_bench/journal
```

The same scenario with the `tools` container in front of the world, also unprotected.
The MCP endpoints are published on `127.0.0.1:7101` to `7132` and the world's admin
API on `127.0.0.1:9000`:

```bash
docker compose -f deploy/compose.dev.yaml up        # WORLD_IMAGE and TOOLS_IMAGE choose other images
cargo run --locked -p catalog --bin catalog -- port github    # 7101: http://127.0.0.1:7101/mcp
curl localhost:9000/_bench/journal
```

## The strict controller

`deploy/run-trial.sh` runs one trial under a local controller instead of plain
`harbor run`. The controller stops the agent container before anything is collected
and checks that it stopped, takes the answer file from the stopped container, and
publishes a receipt that binds the evidence, the answer and the realized images and
configuration. The verifier checks that receipt and sets
`runner_admission_checked: 1`.

```bash
sh deploy/run-trial.sh -p <task-directory> -a oracle -o <output-directory>
deploy/oracle.sh code/github/file-login-bug        # oracle must score 1 and nop 0, with a valid receipt
deploy/oracle.sh --all
```

`deploy/oracle.sh` generates tasks with `--runner-admission required`, so a trial
without a valid receipt is an environment error. Use the controller when you are
auditing the benchmark itself: most `deploy/test-*.sh` regressions run through it,
and `bench-report --require-attested` accepts only its trials. You do not need it to
run the benchmark or to compare arms. Do not give required-mode tasks to a stock
Harbor runner: every trial would be an environment error with reward 0.

It runs on the Python that the `harbor` launcher uses, which must have Harbor 0.23.0
(`deploy/runner/requirements.txt`); set `HARBOR_PYTHON` to choose another. `-a` also
takes `computer-1` and `claude-code`; see
[native-inference-capture.md](native-inference-capture.md). The protocol is in
[runner-admission.md](runner-admission.md) and
[execution-provenance.md](execution-provenance.md).

## Replaying a trial

```bash
cargo run -p world --bin world-replay -- scenarios/<scenario-id> <trial>/verifier/journal.json
```

This starts a fresh world from the scenario, sends the recorded requests again in
order and reports every response that differs. It exits non-zero on any difference.
It first checks that the journal can be replayed at all: one that lacks recorded
request context, or whose requests overlapped, is reported as not replayable and
nothing is sent.

## Capturing tool lists

`servers/snapshots/<endpoint>.json` is the pinned tool list of each endpoint. Task
generation validates reference solutions against these files, and
[research/tool-payload.md](research/tool-payload.md) reports their size.

```bash
deploy/capture-stack.sh [out-dir] [endpoint-id ...]     # default: servers/snapshots, every endpoint
cargo run --locked -p mcp-tools --bin toolmeter -- report
```

`capture-stack.sh` starts `world` and `tools` on an internal Docker network and lists
each endpoint's tools through `toolmeter`, exactly as an agent reaches them. The world
runs protected, as in a generated task, with an admin token made for the run. The
images are `tool-bench-world:dev` and `tool-bench-tools:dev` unless `WORLD_IMAGE` and
`TOOLS_IMAGE` name others. `toolmeter report` prints the per-endpoint tool, byte and
token counts.

A capture of every endpoint also rewrites `crates/world/startup-calls.json`, the
requests the servers make on their own, which grading leaves out of `api_calls`. It
does so wherever the snapshots are written. A capture of named endpoints leaves that
file alone.

A replica endpoint serves the snapshot its `tools` image was built with, so the script
lists it (its startup requests still count) and leaves its file alone. Replica tool
lists come from their source servers through `servers/capture/replicas.sh` and
`servers/capture/cloudflare/run.sh`.
