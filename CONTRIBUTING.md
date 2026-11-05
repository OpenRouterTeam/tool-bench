# Contributing

Start with `docs/authoring.md`. It covers what a mock service must implement, how to write a
scenario (instruction, dataset, expectations, oracle solution) and the realism and determinism
rules both must follow.

## Before opening a pull request

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo run -q -p scenario --bin taskgen          # validates every scenario, regenerates tasks/
```

- Commit the regenerated `tasks/` together with the change that caused it. CI runs
  `taskgen --check` and fails when `tasks/` is stale. Image tags are content hashes, so any
  change under `crates/`, `scenarios/`, `servers/`, the catalog or the image Dockerfiles changes
  them.
- For a new or changed scenario, run `deploy/oracle.sh <scenario-id>` (needs Harbor and Docker):
  the oracle agent must score 1 and the nop agent 0.
- Don't edit files under `tasks/` by hand.

## Data

Datasets are invented. Never add real customer, account or personal data, and never add a real
credential: tokens in the catalog and datasets are placeholders the mocks accept.
