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

Use Python 3.12+ for host tests. Runner regressions additionally need the pinned
Harbor environment in `deploy/runner/requirements.txt`; CI runs those separately.

- Commit generated `tasks/` and `docs/coverage/scenario-inventory.json` with the
  source change. Never edit generated tasks by hand.
- Regenerate after the last source edit and check from a clean checkout of the
  submitted commit. Image tags include file bytes and executable modes; an
  uncommitted image input cannot be assumed present in CI.
- Include alternative valid plans, unsafe collateral changes, restored changes,
  ordering violations and missing-evidence controls for changed contracts.
- Use `deploy/oracle.sh <scenario-id>` for local oracle/no-op controls. It invokes
  `deploy/run-trial.sh` and requires Docker plus Harbor 0.23.0. Raw stock Harbor
  does not supply the required runner receipt.
- A business failure, environment fault, invalid contract and unresolved answer
  are different outcomes. Unknown semantics require adjudication, not keyword
  success or an invented model failure.
- Source tests, scripted client trials, published-image qualification and model
  performance are separate claims. Record exact conditions and never pool
  incompatible checkpoints.
- Do not merge the overhaul into a production-consumed task path before the
  corresponding runner integration and published-image checks are validated.

## Data

Datasets are invented. Never add real customer, account or personal data, or real
credentials. Preserve authored instructions and business dates when repairing a
grader; a deliberate task revision must be identified separately.
