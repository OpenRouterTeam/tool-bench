# Running tool-bench in production

tool-bench measures how well an agent completes ordinary business tasks when it is
handed a very large tool set: 32 MCP endpoints, 1,595 tool definitions, about 727,000
tokens of tool schemas (`docs/research/tool-payload.md`). It is built to compare ways of
serving that tool set (send everything, client-side tool search, router-side deferral)
on the same 150 tasks.

This page is the path from "merged to `main`" to "a comparison you can read". The
grading rules are in [contracts.md](contracts.md) and
[answer-fact-contracts.md](answer-fact-contracts.md).

## How a trial runs

The production worker (`services/gcp-harbor-trial` in openrouter-web) clones this
repository at `main` and runs each directory under `tasks/` with stock Harbor 0.23.0.
Nothing in this repository has to be installed on the worker.

| Step | What happens |
|---|---|
| Start | Compose starts three services: `main` (the agent), `tools` (every MCP server) and `world` (the mock APIs). The worker's per-trial `HARBOR_TRIAL_ADMIN_TOKEN` reaches only `world`. |
| Agent | The harness registers the task's MCP servers and gets `instruction.md`. Lookup tasks ask for the reply in `/app/answer.md`. |
| Collect | Harbor runs the task's collect hook inside `world`, which closes the world to new requests and exports its sealed evidence. Harbor then collects that file and the answer file. |
| Verify | Harbor stops the agent, builds the separate verifier from `tests/`, and runs `bench-verify` offline on the contract, the evidence and the answer. |

The verifier has no network access, no credentials and no agent process beside it.

## Reading one trial

`verifier/reward.json` is a flat map of numbers. `reward` is 1 only when every check
passed.

| `outcome_code` | Meaning | Counts against the model |
|---|---|---|
| 0 | Pass | |
| 1 | Fail: a required outcome is missing or wrong, or something else was changed | Yes |
| 2 | Context overflow | Yes, it is what the arm under test produced |
| 3 | Integrity violation: the world was reached other than through the MCP servers | Yes |
| 6 | Unresolved: the reply could not be read either way | Reward 0; see below |

Two cases are deliberately **not** scored:

- **The trial could not be assessed** (outcome 4 or 5: an invalid contract, missing or
  mismatched evidence). The verifier writes `reward.invalid.json` instead of
  `reward.json` and exits with status 3. Harbor records `RewardFileNotFoundError`, and
  the production pipeline counts the trial as skipped, not as incorrect.
- **The harness or environment failed before grading.** Harbor records the exception and
  there is no reward.

`verifier/assessment.json` lists each failed check with its location and reason.
`runner_admission_checked` is 0 for every production trial; see
[Assurance](#assurance).

### Context overflow

The offline verifier cannot see the model's requests. A provider rejection for prompt
length is detected by Harbor's agent wrapper, which raises
`ContextWindowExceededError`; the production pipeline already treats that as a scored
trial, and `bench-report` counts it as a context overflow.

### Unresolved

A reply is unresolved when it states the right facts but the reader cannot tell which
record it presents as the answer, for example a bare list of every candidate. It
earns no reward. On 500 correct replies written in ordinary prose, 493 pass; the
measured limits are in [answer-fact-contracts.md](answer-fact-contracts.md).

## Comparing arms

Run each arm as its own run with everything else identical: same model, same harness
version, same `datasetRef`, at least three attempts per task.

| Field of the benchmark request | Use |
|---|---|
| `benchmarkId` | `tool-bench` |
| `agent.name`, `agent.modelName`, `agent.kwargs.version` | Harness, model, harness pin |
| `attempts` | Repeats per task |
| `tasks.include` / `tasks.exclude` | Globs over `<group>__<service>__<slug>`, such as `ops__*` or `*__multi__*` |
| `experimentIds`, `requestOverlay.extra_body` | The router treatment for this arm |
| `datasetRef` | A branch or tag of this repository, to run a different tool exposure (below) |

Download each run's artifacts and compare them:

```bash
cargo run -p report --bin bench-report -- runs/arm-a runs/arm-b          # Markdown
cargo run -p report --bin bench-report -- runs/arm-a runs/arm-b --json
```

For each arm the report gives the pass rate over assessed trials with an interval, the
outcome breakdown, pass rates by task group and for single- versus multi-service
tasks, API calls per trial, and the checks that failed most. With two or more arms it
adds a paired comparison on the tasks both attempted and warns when the arms differ in
evaluator version, task set or contracts. Trials without an assessment are listed and
left out of the pass rate.

Per trial, the report re-runs the assessment from the collected evidence, contract and
answer and requires it to match what the verifier saved. Token and cost figures are
whatever Harbor recorded for the harness.

## Changing the size of the tool set

The committed `tasks/` register the full catalog. `taskgen --exposure` generates the
same tasks with a different registered set, from the same pinned tool definitions:

| Variant | Registers |
|---|---|
| `relevant_only` | Only the tools the task's reference solution uses |
| `context_feasible` | The relevant tools plus others up to `--exposure-token-budget` |
| `organization_pack` | The endpoints of the task's organization |
| `full_catalog` | Everything (the default) |
| `randomized_order` | Everything, in a seeded order |
| `confusing_distractors` | The relevant endpoints plus look-alike ones |

To run a variant in production, generate it on a branch and pass that branch as
`datasetRef`:

```bash
git switch -c exposure/relevant-only
cargo run --locked -p scenario --bin taskgen -- --exposure relevant_only
git commit -am "tasks: relevant-only exposure" && git push -u origin exposure/relevant-only
```

The images are the same; only the registration differs. The variant, seed and realized
tool list are recorded in each task's `tests/exposure.json`.

## Releasing a change

Tasks reference images by a hash of their sources, so a change to scenarios, crates,
servers or the catalog needs regenerated tasks and new images.

1. Regenerate and commit:
   ```bash
   cargo run --locked -p scenario --bin taskgen -- --inventory docs/coverage/scenario-inventory.json
   cargo run --locked -p scenario --bin release-manifest -- --manifest releases/tool-bench-public-dev-1.json --freeze
   ```
2. Check locally:
   ```bash
   cargo test --workspace --locked
   cargo clippy --workspace --all-targets --locked -- -D warnings
   deploy/stock-campaign.sh        # oracle scores 1 and no-op scores 0 on all 150 tasks, through stock Harbor
   ```
3. Merge to `main`. `.github/workflows/tool-bench-images.yml` builds and pushes any image
   tag that is not published yet. Production picks up the new tasks on its next clone.

Bump `EVALUATOR_VERSION` (`crates/mock-core/src/grading.rs`) whenever grading changes,
and do not pool results across evaluator versions.

## Assurance

Production trials are **unattested**: the verifier trusts that Harbor collected the
evidence and the answer from this trial's containers. What it checks itself is that the
evidence is sealed by the world, belongs to this scenario, and matches the task's
contract and dataset hashes.

`deploy/run-trial.sh` runs a trial under a stricter local controller that stops the
agent container before collection and publishes a receipt binding the evidence, the
answer and the realized configuration. Tasks generated with
`taskgen --runner-admission required` accept only such trials. That mode is for
auditing the benchmark itself (`deploy/test-*.sh`); it is not needed to run it.

## Checking the benchmark without a model

| Tool | What it shows |
|---|---|
| `deploy/stock-campaign.sh` | Every task end to end through stock Harbor with the scripted oracle and no-op agents |
| `deploy/production/` | The production worker's own code run locally against generated tasks |
| `deploy/pilot/` | A person or an AI subagent acting as the agent on one task, then graded |
| `cargo run -p mock-core --bin bench-answer-probe -- scenarios <replies.json>` | How the reader grades candidate replies for any lookup task |
