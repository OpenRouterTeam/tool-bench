# tool-bench

A benchmark for agents that are handed a very large tool set. The agent gets an
ordinary business task and 32 MCP endpoints covering 30 services: 1,595 tools, about
727,000 tokens of schemas ([measured](docs/research/tool-payload.md)). The tools talk
to deterministic mock services, and the benchmark grades the state the agent leaves.

## What it measures

Whether the agent finds the right tools among 1,595 and finishes the work without
touching anything else. The tool set is larger than most context windows on purpose:
the benchmark exists to compare ways of serving it (send every definition, let the
harness search, let the router defer) on identical tasks.

Each endpoint is the pinned vendor or community MCP server, or for 8 services a
replica that serves the captured tool list. Behind them are Rust models of each API,
loaded from a fictional dataset per task. The models are deterministic: a logical
clock, seeded IDs, a journal of every request. They are models, not
conformance-tested copies of the hosted APIs.

## Tasks

150 tasks in six groups. Each group has 20 single-service tasks, four per service,
and four to six tasks that span services. 100 tasks change something; 50 are lookups
that ask for a written reply.

| Group | Services | Tasks |
|---|---|---|
| `business` | HubSpot, Salesforce, Shopify, Stripe, Zendesk | 25 |
| `cloud` | AWS, Cloudflare, MongoDB, Okta, Supabase | 25 |
| `code` | Argo CD, GitHub, GitLab, Terraform, Vercel | 24 |
| `collab` | Google Workspace, Jira, Linear, Notion, Slack | 25 |
| `ops` | Datadog, Grafana, Kubernetes, PagerDuty, Sentry | 26 |
| `search` | Brave Search, Elasticsearch, Exa, Firecrawl, Tavily | 25 |

Examples: file an issue, scale a deployment, refund an order in Stripe and tell the
customer in Zendesk. Instructions read as a user would ask and name no tools. Each task
has a scripted reference solution that must score 1, and doing nothing must score 0.

## Grading

When the agent stops, the world closes and exports sealed evidence: its initial and
final state, every request and every committed change. An offline verifier checks that
evidence against the task's contract (`tests/expect.json`). To pass, the requested
records and relationships must exist with the right values, nothing outside the
contract may have changed, and every call must have gone through the MCP servers.
A lookup reply is read as ordinary prose and must state the facts that the task's
data determines.

`reward` is 1 only when every check passes. `outcome_code` says what happened:

| Code | Meaning |
|---|---|
| 0 | Pass |
| 1 | Fail: a required outcome is missing or wrong, or something else was changed |
| 2 | Context overflow: the request was rejected for length before any tool ran |
| 3 | Integrity violation: the world was reached around the MCP servers |
| 4 | Invalid contract |
| 5 | Environment error: the evidence is missing or does not match the task |
| 6 | Unresolved: the grader could not decide either way; reward 0 |

Codes 4 and 5 mean the trial could not be assessed, which is a benchmark fault: the
verifier writes `reward.invalid.json` instead of `reward.json`, and Harbor records an
error instead of a zero. Under stock Harbor a context overflow appears as Harbor's
`ContextWindowExceededError`, not as code 2. The rules are in
[docs/contracts.md](docs/contracts.md) and [docs/answer-fact-contracts.md](docs/answer-fact-contracts.md).

## Running it

`tasks/` holds ordinary Harbor tasks; stock Harbor 0.23.0 runs and scores them.
[docs/production.md](docs/production.md) covers a production run end to end: what a
trial does, how to read one, how to compare arms with `bench-report`, and how to
change the size of the tool set.

## Developing

Needs Rust 1.99 and Docker. Local trials also need Harbor 0.23.0 and `jq`.

```bash
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
# Regenerate tasks/ and the inventory after changing scenarios, crates, servers or the catalog
cargo run --locked -p scenario --bin taskgen -- --inventory docs/coverage/scenario-inventory.json
cargo run --locked -p scenario --bin taskgen -- --check --inventory docs/coverage/scenario-inventory.json
# Build the tools image, then run the reference and do-nothing agents through stock Harbor
docker build -f images/tools/Dockerfile -t tool-bench-tools:dev .
deploy/stock-campaign.sh --only 'code__github__*'
```

[docs/runbook.md](docs/runbook.md) has the rest: images, local checks, the strict
controller, replay and tool-list capture. To add a service or a task, start with
[docs/authoring.md](docs/authoring.md) and [CONTRIBUTING.md](CONTRIBUTING.md). All
documentation is indexed in [docs/README.md](docs/README.md).

## Layout

| Path | What |
|---|---|
| `catalog/services.toml` | The 30 services and 32 MCP endpoints: pins, launch commands, env, ports, TLS hostnames |
| `catalog/operation-support/`, `catalog/validation-evidence/` | Which operations and tasks have reviewed tests behind them |
| `crates/mock-core` | Clock, IDs, datasets, evidence, grading; the `bench-verify` and `bench-answer-probe` binaries |
| `crates/services/*` | One mock API per service |
| `crates/world` | Hosts every mock on its port and on HTTPS for clients with fixed hostnames; `world-replay` |
| `crates/mcp-tools` | Stdio-to-HTTP bridge, replica backend, `toolmeter` |
| `crates/catalog` | Catalog parser and CLI; `tools-supervisor`, the tools container entry point |
| `crates/scenario` | Scenario validation, the inventory, `taskgen`, `release-manifest` |
| `crates/report` | `bench-report` (arm comparison) and `bench-review` |
| `servers/` | Lockfiles, per-server config and extensions, tool-list snapshots |
| `images/` | `world` and `tools` Dockerfiles |
| `scenarios/` | Task sources: `scenario.toml`, `instruction.md`, `dataset/` |
| `tasks/` | Generated Harbor tasks; never edit by hand |
| `releases/` | The frozen task list and source hashes, written by `release-manifest` |
| `deploy/` | `stock-campaign.sh`, the strict controller (`run-trial.sh`, `runner/`), regression scripts |
| `docs/` | Documentation; `docs/history/` holds superseded working notes |

## Licence

MIT (`LICENSE`). Vendored upstream data keeps its own licence: [docs/provenance.md](docs/provenance.md)
and `THIRD_PARTY_NOTICES.md`. Security reports: `SECURITY.md`.
