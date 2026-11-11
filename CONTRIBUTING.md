# Contributing

Start with `docs/authoring.md`. It covers service models, scenario instructions,
datasets, business contracts and reference programs. Reference programs are
calibration controls, not the only valid agent plans.

## Before Opening A Pull Request

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo run -q --locked -p scenario --bin taskgen -- --inventory docs/coverage/scenario-inventory.json
cargo run -q --locked -p scenario --bin taskgen -- --check --inventory docs/coverage/scenario-inventory.json
PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover -s deploy/tests -p 'test_*.py'
```

Use Python 3.12+ for host tests. Runner regressions (`python -m unittest discover -s
deploy/runner -p 'test_*.py'`) additionally need the pinned Harbor environment in
`deploy/runner/requirements.txt` and [Bun](https://bun.sh) on `PATH`, which runs the
provider proxy; CI runs those separately.

- Commit generated `tasks/` and `docs/coverage/scenario-inventory.json` with the
  source change. Never edit generated tasks by hand.
- Regenerate after the last source edit and check from a clean checkout of the
  submitted commit. Image tags include file bytes and executable modes; an
  uncommitted image input cannot be assumed present in CI.
- Include alternative valid plans, unsafe collateral changes, restored changes,
  ordering violations and missing-evidence controls for changed contracts.
- Use `deploy/stock-campaign.sh --only '<task glob>'` for local oracle/no-op runs.
  It runs them through stock Harbor, the way production does, and needs Docker,
  Harbor 0.23.0 and a `tool-bench-tools:dev` image. `deploy/oracle.sh <scenario-id>`
  runs the same pair under the strict controller; see `docs/runbook.md`.
- A business failure, environment fault, invalid contract and unresolved answer
  are different outcomes. Text the grader cannot decide is unresolved. Do not turn
  it into keyword success or an invented model failure.
- Source tests, scripted trials and model performance are separate claims. Record
  the evaluator version and never pool results across evaluator versions.
- The production worker clones `main`. Before merging a change to scenarios,
  crates, servers or the catalog, follow "Releasing a change" in
  `docs/production.md`.

## Data

Datasets are invented. Never add real customer, account or personal data, or real
credentials. Preserve authored instructions and business dates when repairing a
grader; a deliberate task revision must be identified separately.
