# Running tool-bench in production

tool-bench measures how well an agent completes ordinary business tasks when it is
handed a very large tool set: 32 MCP endpoints, 1,595 tool definitions, about 727,000
tokens of tool schemas ([measured](research/tool-payload.md)). It is built to compare ways of
serving that tool set (send everything, client-side tool search, router-side deferral)
on the same 150 tasks.

This page is the path from "merged to `main`" to "a comparison you can read". The
grading rules are in [contracts.md](contracts.md) and
[answer-fact-contracts.md](answer-fact-contracts.md); building and changing the
benchmark is in [runbook.md](runbook.md).

## How a trial runs

The production worker (`services/gcp-harbor-trial` in openrouter-web) clones this
repository at `main` and runs each directory under `tasks/` with stock Harbor 0.23.0.
Nothing in this repository has to be installed on the worker.

| Step | What happens |
|---|---|
| Start | Compose starts three services: `main` (the agent), `tools` (every MCP server) and `world` (the mock APIs). The task's compose file passes the worker's per-trial `HARBOR_TRIAL_ADMIN_TOKEN` only to `world`. |
| Agent | The harness registers the task's MCP servers and gets `instruction.md`. Tasks that expect a reply ask for it in `/app/answer.md`. |
| Collect | Harbor copies the answer file out of `main` and stops `main`. It then runs the task's collect hook inside `world`, which closes the world to new requests and exports its sealed evidence, and collects that file. |
| Verify | Harbor builds the separate verifier from `tests/` and runs `tests/test.sh`: `bench-verify --allow-unattested-runner --withhold-invalid-reward` on the contract, the evidence and the answer. |

The verifier container has no agent process beside it, and `bench-verify` makes no
network request and uses no credentials. The task does not ask Harbor to cut the
verifier's network: that declaration made the worker pull and build two extra images
in every trial, after the agent had finished. `test.sh` has those two flags because
`taskgen` generates tasks with `--runner-admission optional` unless told otherwise; see
[Assurance](#assurance).

## Reading one trial

`verifier/reward.json` is a flat map of numbers. `reward` is 1 only when every check
passed.

| `outcome_code` | Meaning | Counts against the model |
|---|---|---|
| 0 | Pass | |
| 1 | Fail: a required outcome is missing or wrong, or something else was changed | Yes |
| 2 | Context overflow. Production trials never carry this code; see below | Yes, it is what the arm under test produced |
| 3 | Integrity violation: the world was reached other than through the MCP servers | Yes |
| 6 | Unresolved: the reply could not be read either way | Reward 0; see below |

Two cases are deliberately **not** scored:

- **The trial could not be assessed** (outcome 4 or 5: an invalid contract, missing or
  mismatched evidence, or an operation the agent asked for that the mock does not
  model). The verifier writes `reward.invalid.json` instead of `reward.json` and exits
  with status 3. Harbor records `RewardFileNotFoundError`, and the production pipeline
  counts the trial as skipped, not as incorrect. Such a trial is void whether or not
  the agent would have passed, so watch the skipped count per arm: an arm with many
  more skips than another is not being compared on the same tasks.
- **The harness or environment failed before grading.** Harbor records the exception and
  there is no reward.

`verifier/assessment.json` lists each failed check with its location and reason.
`runner_admission_checked` is 0 for every production trial; see
[Assurance](#assurance).

### Context overflow

The offline verifier cannot see the model's requests, so it never writes
`outcome_code` 2 for a production trial. A provider rejection for prompt length is
detected by Harbor's agent wrapper, which raises `ContextWindowExceededError`. The
production pipeline treats that as a scored trial, not a skipped one, and
`bench-report` counts it as a context overflow unless the trial passed anyway.

### Unresolved

A reply is unresolved when it states the right facts but the reader cannot tell which
record it presents as the answer, for example a bare list of every candidate. It
earns no reward. The reader's measured limits are in
[answer-fact-contracts.md](answer-fact-contracts.md), and
[What has been checked](#what-has-been-checked) has the rate on replies written by
agents.

The two consumers treat it differently. The production pipeline sees reward 0 and
counts the trial as incorrect. `bench-report` lists unresolved trials beside the pass
rate and leaves them out of it.

## Comparing arms

Run each arm as its own run with everything else identical: same model, same harness
version, same `datasetRef`, at least three attempts per task.

| Field of the benchmark request | Use |
|---|---|
| `benchmarkId` | `tool-bench` |
| `agent.name`, `agent.modelName`, `agent.kwargs.version` | Harness, model, harness pin |
| `attempts` | Repeats per task |
| `tasks.include` / `tasks.exclude` | Globs over `<group>__<service>__<slug>`, such as `ops__*` or `*__multi__*` |
| `clientToolSearch` | `on` or `off`: the harness's own tool search (Claude Code, Codex, pi) |
| `deferredTools` | Router-side deferral; sent as `deferred_tools` on every request |
| `experimentIds`, `requestOverlay.extra_body` | Any other router treatment for this arm |
| `arm` | A label for the results; it changes no request |
| `datasetRef` | A branch or tag of this repository, to run a different tool exposure (below) |

Download each run's whole trial directories, one directory per arm, and compare them:

```bash
cargo run -p report --bin bench-report -- runs/arm-a runs/arm-b          # Markdown
cargo run -p report --bin bench-report -- runs/arm-a runs/arm-b --json
cargo run -p report --bin bench-report -- runs/arm-a runs/arm-b --explain <task>
```

For each arm the report gives the pass rate over assessed trials with an interval, the
outcome breakdown, pass rates by task group and for single- versus multi-service
tasks, API calls per trial, and the checks that failed most. With two or more arms it
adds a paired comparison on the tasks both attempted and warns when the arms differ in
evaluator version, task set or contracts. Unresolved trials and trials without an
assessment are listed and left out of the pass rate. `--explain` prints the failed
checks of each of one task's trials.

Per trial, the report re-runs the assessment from the collected evidence, contract and
answer and requires it to match what the verifier saved. Build it from the revision
that graded the run: a trial graded by another evaluator version, or downloaded
without its artifacts, is reported as recorded only, with the verifier's numbers
unchecked. Token and cost figures are whatever Harbor recorded for the harness.

## Changing the size of the tool set

The committed `tasks/` register the full catalog. `taskgen --exposure` generates the
same tasks with a different registered set, from the same pinned tool definitions:

| Variant | Registers |
|---|---|
| `relevant_only` | Only the tools the task's reference solution uses |
| `context_feasible` | The organization pack plus further endpoints while they fit in `--exposure-token-budget` (required for this variant) |
| `organization_pack` | Every endpoint of the services the task's scenario lists in `services`, which stand for the applications its organization has connected |
| `full_catalog` | Everything (the default) |
| `randomized_order` | Everything, in an order seeded by `--exposure-seed` |
| `confusing_distractors` | The organization pack plus endpoints of other applications whose tool names overlap it |

To run a variant in production, generate it on a branch and pass that branch as
`datasetRef`:

```bash
git switch -c exposure/relevant-only
cargo run --locked -p scenario --bin taskgen -- --exposure relevant_only
git add tasks && git commit -m "tasks: relevant-only exposure" && git push -u origin exposure/relevant-only
```

The images are the same; only the registration differs. The variant, seed and realized
tool list are recorded in each task's `tests/exposure.json`, a new file that
`git commit -a` would leave out. The contract in `tests/expect.json` is identical
under every variant.

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
   The campaign builds the world image itself and needs a `tool-bench-tools:dev`
   image; see [runbook.md](runbook.md#images).
3. Merge to `main`. CI (`.github/workflows/tool-bench.yml`) fails if `tasks/` or the
   inventory is not what `taskgen` generates. `.github/workflows/tool-bench-images.yml`
   builds and pushes any image tag that is not published yet. Production picks up the
   new tasks on its next clone.

Bump `EVALUATOR_VERSION` (`crates/mock-core/src/grading.rs`) whenever grading changes,
and do not pool results across evaluator versions.

## Assurance

Production trials are **unattested**: the verifier trusts that Harbor collected the
evidence and the answer from this trial's containers. What it checks itself is that the
evidence is sealed by the world, belongs to this scenario, and matches the task's
contract and dataset hashes.

`deploy/run-trial.sh` runs a trial under a stricter local controller. It stops the
agent container before anything is collected, checks that it stopped, and publishes a
receipt binding the evidence, the answer and the realized configuration. Tasks
generated with `taskgen --runner-admission required` accept only such trials. That
mode is for auditing the benchmark itself (`deploy/test-*.sh`); it is not needed to
run it. See [runner-admission.md](runner-admission.md).

## What has been checked

No model has been run against this revision through OpenRouter. These checks stand in
for one, and each can be repeated from this repository:

| Check | Result |
|---|---|
| Reference solution and no-op agent through stock Harbor (`deploy/stock-campaign.sh`) | The reference solution scores 1 on 150 of 150 tasks and the no-op agent scores 0 on 150 of 150 |
| The production worker's own planner, validation and trial code (`deploy/production/`) | All 150 task directories load, and 26 scripted attempts end as expected |
| AI agents attempting tasks through the MCP servers without seeing the contracts (`deploy/pilot/`) | 357 attempts in four waves. The last wave covered all 150 tasks on the final contracts: 147 passed |
| Reply corpora (`crates/mock-core/tests/fixtures/`) | 499 of 500 correct replies pass and 300 of 300 wrong replies are rejected |

What the agent attempts found, and what to expect from a model run:

- **Lookup replies.** The reader that grades written replies was the weak point. In
  successive waves it left 5 of 56, 3 of 55 and 2 of 55 correct replies unscored, each
  time because of a note written after a complete answer. Every case was fixed and all
  158 replies are now regression fixtures, but those rates are the honest guide: expect
  a few percent of correct, wordy replies to come back unresolved. `bench-report` lists
  unresolved trials apart from failures. Read them before trusting a small difference
  between two arms.
- **Write tasks.** All 95 passed in the last wave. A probe of the alternatives the
  agents had doubted changed 30 tasks where an ordinary way of doing the job was
  rejected or the data contradicted itself, and fixed 11 mock behaviours that blocked
  or misled correct work.
- **One model.** Every pilot agent was the same model. A different model will take
  paths these did not, so treat a task that one model fails every time while its trace
  looks right as a contract to inspect, with `bench-report --explain <task>`.

Before the first production run:

1. Merge to `main` and wait for `.github/workflows/tool-bench-images.yml` to publish the
   two images. The committed tasks name image tags that exist only after that.
2. Run the `nop` agent over all 150 tasks. It costs nothing. Expect 150 incorrect and 0
   skipped; a skipped trial means the environment or the verifier failed.
3. Run a small paid sample (for example 20 tasks, one attempt, one harness) and read
   every skipped and unresolved trial before scaling up.

The worker-side follow-ups from the rehearsal are in
[`deploy/production/FINDINGS.md`](../deploy/production/FINDINGS.md).

## Checking the benchmark without a model

| Tool | What it shows |
|---|---|
| `deploy/stock-campaign.sh` | Every task end to end through stock Harbor with the scripted oracle and no-op agents |
| `deploy/production/` | The production worker's own code run locally against generated tasks |
| `deploy/pilot/` | A person or an AI subagent acting as the agent on one task, then graded |
| `cargo run -p mock-core --bin bench-answer-probe -- scenarios <replies.json>` | How the reader grades candidate replies for any lookup task |
