# tool-bench

A deterministic benchmark for tool calling with very large tool sets. Each trial attaches 30
services through 32 MCP endpoints, 1,595 tools and about 727k tokens of tool definitions, then
asks the agent to do one realistic task: file an issue, scale a deployment, refund an order
across Zendesk and Stripe.

The MCP servers are the real, pinned upstream servers (GitHub, Kubernetes, AWS, Stripe, …).
They talk to mock APIs written in Rust that answer like the real services from a curated
dataset per scenario. Nothing leaves the trial except model traffic, and every run of a scenario
starts from the same world.

## What it measures

Every scenario is graded on the world's final state and on the calls the agent made through MCP:

| Check | Passes when |
|---|---|
| `state_ok` | The world ends in the expected state (the issue exists, the replicas changed, …) |
| `required_calls_ok` | The agent made each required API call through an MCP server |
| `forbidden_calls_ok` | The agent made none of the forbidden calls (wrong refund, deleted repo, …) |
| `answer_ok` | The written answer, when the scenario asks for one, contains what it must |

`reward` is 1 when all four pass, otherwise 0. `outcome_code` is 0 pass, 1 fail, or 2 context
overflow: the model rejected the prompt before making any tool call, which is what happens
when the full tool list doesn't fit its context window. `api_calls` counts the agent's own calls
(the servers' startup requests are subtracted) and `mcp_bypass` counts calls that skipped MCP.

## Determinism

The world runs on a logical clock with seeded IDs and records every request in a journal.
`world-replay` re-issues a trial's journal against a fresh world, and the result must match byte
for byte. Tool lists are captured offline and are hash-identical across runs.

## Scenarios

150 scenarios in six groups (search, code, ops, cloud, business, collab): 120 use one service
and 30 cross services. Each one is generated into a Harbor task in `tasks/`. Every task scores
reward 1 with Harbor's oracle agent (which replays the solution over MCP) and 0 with the nop
agent.

## Results so far

- **Smoke run:** Claude Code with Claude Haiku 5.5 on OpenRouter passed 3 tasks with tool
  search on and off. With every tool sent upfront, a single turn is about 820k Claude tokens
  (`docs/research/smoke-2026-10-31.md`).
- **Deferred-tools arms:** router-side tool search vs Claude Code's own ToolSearch vs all tools
  upfront, 324 runs over Claude Haiku 5.5 and Kimi K2.7 Code
  (`docs/research/arms-2026-10-31.md`).
- **Tool payload:** tools, bytes and tokens per endpoint in `docs/research/tool-payload.md`.

## Layout

| Path | What |
|---|---|
| `catalog/services.toml` | Source of truth for the 30 services and 32 MCP endpoints: pins, launch commands, env, ports, TLS hostnames |
| `crates/mock-core` | Shared mock library: logical clock, deterministic IDs, document store, datasets, request journal, grading, admin API |
| `crates/services/*` | One mock API per service |
| `crates/world` | Binary hosting every mock on its port, plus HTTPS on 443 (local CA) for clients with fixed hostnames |
| `crates/mcp-tools` | Stdio↔streamable-HTTP bridge, replica backend, MCP clients, `toolmeter` (capture, report, import) |
| `crates/catalog` | Catalog parser and CLI, plus `tools-supervisor` (the tools container entrypoint) |
| `crates/scenario` | `taskgen`: turns `scenarios/` into `tasks/` |
| `crates/report` | `bench-report`: compares two runs |
| `scenarios/` | Scenario sources (`scenario.toml`, `instruction.md`, `dataset/`) |
| `tasks/` | Generated Harbor tasks; never edit by hand |
| `servers/` | npm and Python lockfiles, per-server config, capture scripts, tool-list snapshots |
| `images/` | `world` and `tools` Dockerfiles |
| `deploy/` | Local compose stack, capture, oracle and smoke-run scripts |
| `docs/` | Design, authoring guide, runbook, research notes |

## Development

Needs Rust 1.99 and Docker.

```bash
cargo test --workspace                                    # all tests
cargo run -p catalog --bin catalog -- validate            # check the catalog
cargo run -p scenario --bin taskgen                       # validate scenarios, regenerate tasks/

docker build -f images/world/Dockerfile -t tool-bench-world:dev .
docker build -f images/tools/Dockerfile -t tool-bench-tools:dev .

docker compose -f deploy/compose.dev.yaml up              # world + tools; MCP on localhost:7101-7132
./deploy/capture-stack.sh                                 # offline capture of every endpoint -> servers/snapshots + crates/world/startup-calls.json
cargo run -p mcp-tools --bin toolmeter -- report          # table of tools, bytes and tokens per endpoint
deploy/oracle.sh code/github/file-login-bug               # oracle must score 1, nop 0 (needs Harbor)
```

The world on its own:

```bash
SCENARIO_DIR=scenarios/example/github/basic cargo run -p world
curl localhost:8101/api/v3/repos/octo-org/app/issues
curl localhost:9000/_bench/journal
```

Images, running on Harbor and comparing runs: `docs/runbook.md`. Architecture:
`docs/superpowers/specs/2026-10-30-tool-bench-design.md`.

## Contributing

Adding a service or a scenario: `docs/authoring.md`. See `CONTRIBUTING.md` for the checks a pull
request must pass, and `SECURITY.md` to report a vulnerability.

## Licence

MIT (`LICENSE`). Vendored upstream data keeps its own licence: see `docs/provenance.md` and
`THIRD_PARTY_NOTICES.md`.
