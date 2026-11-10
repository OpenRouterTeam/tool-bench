# tool-bench

A deterministic benchmark for tool calling with very large tool sets. The full
catalog spans 30 services and 32 MCP endpoints, with roughly 1,600 tool definitions.
Tasks ask for ordinary work: file an issue, scale a deployment, or refund an order
across Zendesk and Stripe. Catalog stress and realistic workspace profiles are
separate evaluation conditions.

Pinned MCP clients and explicitly declared replicas call deterministic Rust service
models populated from fictional per-scenario datasets. Captured schemas and passing
local workflows do not establish hosted-service conformance. Google, for example,
uses the Rust MCP replica, not the published Python Workspace server or Google SDK.

**Release status:** this overhaul is not a drop-in replacement for stock Harbor.
Scored tasks require the strict runner's per-trial credential, stop-before-collect
lifecycle and source-bound receipt. Production-runner integration, native-model
completion provenance and final published-image qualification remain release gates.
See `docs/runner-admission.md` and `docs/overhaul-checklist.md` before running or
deploying this revision.

## What it measures

Version-2 contracts require the requested business outcome while preserving
unrelated state. Created identities, relationships, exact quantities and committed
history replace reference-shaped positive checks where those contracts apply.

| Dimension | Evidence |
|---|---|
| Business outcome | Required records, relationships, values and artifacts |
| Preservation | Only authorized fields and records change, including intermediate history |
| Causal safety | Required remediation or information precedes the action |
| Interface integrity | Calls use the permitted interface and authenticated trial evidence |
| Answer quality | Declared source-bound facts or reviewed content; unknown semantics require adjudication |
| Environment and admission | Complete evidence, valid contracts and trusted runner completion |

`reward = 1` requires every applicable gate to pass. A zero reward alone is not a
model failure:

| `outcome_code` | Meaning |
|---|---|
| 0 | Pass |
| 1 | Proven task failure |
| 2 | Context overflow |
| 3 | Integrity violation |
| 4 | Invalid contract |
| 5 | Environment error |
| 6 | Unresolved; adjudication required |

Do not average invalid or unresolved trials into ordinary model accuracy. Retain
`evaluator_version`, `contract_version`, failure details and the exact execution
condition. `docs/contracts.md` defines the predicates and their limits;
`docs/authoring-admission.md` explains static tool/schema admission.
Tasks flagged `legacy_answer_semantics` in the inventory still use their declared
substring/regex checks. Contract version 2 does not certify complete answer
semantics or runtime support.

## Determinism

The world uses a logical business clock, seeded IDs, a request journal and committed
record events. `world-replay` checks replay against a fresh world. Tool definitions
are captured offline and versioned. Third-party client behavior, process clocks
and hosted APIs require separate qualification; deterministic fixtures do not make
those guarantees automatically.

## Scenarios And Evidence

The suite contains 150 scored task definitions: 120 single-service and 30
cross-service workflows. Two examples are validated but not scored. The generated
inventory in `docs/coverage/scenario-inventory.json` reports source declarations,
not proof that every operation or task has runtime qualification.

Historical model smoke runs and the 324-run tool-discovery experiment are documented
in `docs/research/smoke-2026-10-31.md` and `docs/research/arms-2026-10-31.md`.
They predate this overhaul and are not performance results for the current source.
Firecrawl42B remains the latest admitted bounded scripted native-client condition,
not a native-model or full-suite release. Checkpoint evidence currently retained
only on the development machine is not independently downloadable by reviewers.
Current acceptance gaps and retention boundaries are in `docs/overhaul-checklist.md`.

## Layout

| Path | What |
|---|---|
| `catalog/services.toml` | Source of truth for the 30 services and 32 MCP endpoints: pins, launch commands, env, ports, TLS hostnames |
| `crates/mock-core` | Logical clock, deterministic IDs, datasets, evidence, grading and admin API |
| `crates/services/*` | One mock API per service |
| `crates/world` | Binary hosting every mock on its port, plus HTTPS on 443 (local CA) for clients with fixed hostnames |
| `crates/mcp-tools` | Stdio↔streamable-HTTP bridge, replica backend, MCP clients, `toolmeter` (capture, report, import) |
| `crates/catalog` | Catalog parser and CLI, plus `tools-supervisor` (the tools container entrypoint) |
| `crates/scenario` | Source validation, inventory and Harbor task generation |
| `crates/report` | Condition-aware result comparison |
| `servers/` | npm and Python lockfiles, per-server config, capture scripts, tool-list snapshots |
| `images/` | `world` and `tools` Dockerfiles |
| `deploy/` | Strict runner, local stack, capture, reference controls and evidence auditors |
| `scenarios/` | Scenario sources (`scenario.toml`, `instruction.md`, `dataset/`) |
| `tasks/` | Generated Harbor tasks; never edit by hand |
| `docs/` | Contracts, runbook, authoring, research and qualification boundaries |

## Development

Needs Rust 1.99 and Docker. Runner checks additionally require Python 3.12+ and the
pinned Harbor environment in `deploy/runner/requirements.txt`.

```bash
cargo test --workspace --locked
cargo run --locked -p catalog --bin catalog -- validate
cargo run --locked -p scenario --bin taskgen -- --inventory docs/coverage/scenario-inventory.json
cargo run --locked -p scenario --bin taskgen -- --check --inventory docs/coverage/scenario-inventory.json

docker build -f images/world/Dockerfile -t tool-bench-world:dev .
docker build -f images/tools/Dockerfile -t tool-bench-tools:dev .

cargo run --locked -p mcp-tools --bin toolmeter -- report
```

For local scripted oracle/no-op controls, build the tools image and use
`deploy/oracle.sh <scenario-id>`, which invokes `deploy/run-trial.sh`. Follow
`docs/runner-admission.md`; a raw `harbor run` does not establish admission.
Native-model benchmark inference is not enabled by this diagnostic path.

An explicitly insecure, unscored world-only development session:

```bash
BENCH_ALLOW_INSECURE_LOCAL=1 SCENARIO_DIR=scenarios/example/github/basic cargo run --locked -p world
curl localhost:8101/api/v3/repos/octo-org/app/issues
curl localhost:9000/_bench/journal
```

Running, capturing and comparing declared conditions: `docs/runbook.md`. Architecture:
`docs/superpowers/specs/2026-10-30-tool-bench-design.md`.

## Contributing

Start with `docs/authoring.md`. Keep generated tasks and the inventory synchronized,
preserve fictional data and record the limits of every qualification claim. See
`CONTRIBUTING.md` for validation and `SECURITY.md` for vulnerability reporting.

## Licence

MIT (`LICENSE`). Vendored upstream data keeps its own licence: see `docs/provenance.md` and
`THIRD_PARTY_NOTICES.md`.
