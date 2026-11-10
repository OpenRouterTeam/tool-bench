# Runbook: building, running and comparing tool-bench

## 1. Images

Tasks reference `us-docker.pkg.dev/openrouter-ci/harbor/tool-bench-{world,tools}:<tag>`, where each
tag is a hash of the files that image is built from (`crates/scenario/src/images.rs`). After changing
scenarios, crates, servers, the catalog or an image Dockerfile, regenerate and commit `tasks/`:

```bash
CARGO_INCREMENTAL=0 cargo run --locked -p scenario --bin taskgen -- --inventory docs/coverage/scenario-inventory.json
```

CI (`.github/workflows/tool-bench.yml`) checks both generated tasks and the source
inventory. Tool/argument admission, capture deferrals and inventory claim limits
are documented in [authoring admission](authoring-admission.md).
On merge to `main`, `.github/workflows/tool-bench-images.yml` builds and pushes any tag that isn't
published yet, so the tasks on `main` always point at images built from the same sources.
`taskgen --print-images` prints the two references. The trial pulls them with its Workload
Identity; compose `image:` lines are not rewritten by the trial's mirror logic.

For local runs, pass local tags: `--world-image tool-bench-world:dev --tools-image tool-bench-tools:dev`
(`deploy/oracle.sh` does this).

### Retained-source replay

Build an extracted source archive with a fresh, per-extracted-root
`CARGO_TARGET_DIR`, not a previously populated workspace target. A local
revision-38 replay reused newer compiled libraries despite different extracted
source bytes and reported evaluator 39. Source hashes alone did not detect the
mixed executable. That attempt is retained but excluded from validation.

Assert the executable's evaluator identity before interpreting replay results;
also check the changed behavior, since matching revision numbers are not a
complete build attestation. Retain mixed-artifact diagnostics separately rather
than relabeling them as historical controls. See the
[Google source checkpoint](checkpoints/evaluator-39-google.md).

## 2. Register the benchmark

`tool-bench` is registered in the Harbor runner (`services/gcp-harbor-trial/src/gcp_harbor_trial/benchmarks.py`
in openrouter-web) as this repository's `main`, tasks path `tasks`.

## 3. Run

Submit through the Argo `benchmark-request` template, like any Harbor benchmark. The fields
that matter here:

| Field | Use |
|---|---|
| `benchmarkId` | `tool-bench` |
| `agent.name` / `agent.modelName` | `claude-code`, `codex`, or `acp`; any OpenRouter slug |
| `agent.kwargs.version` | harness pin, e.g. Claude Code `2.1.235` |
| `attempts` | repeats per task (≥ 3 recommended for paired comparisons) |
| `tasks.include` / `tasks.exclude` | globs over `<group>__<service>__<slug>`, e.g. `["ops__*"]`, `["*__multi__*"]` |
| `experimentIds` | router arm (sent as `x-openrouter-experiment-ids`) |
| `requestOverlay.extra_body` | router tool-handling fields, e.g. `{"deferred_tools": {...}}` |

Run each arm as its own run, with everything else identical.

### Required verifier capabilities

Generated tasks now require Harbor's separate verifier, sidecar collect hooks, and
service-scoped artifact collection/rematerialization. They declare
`metadata.verification_protocol = "offline-world-evidence-v5"`. The runner must
admit this protocol explicitly; silently ignoring new task fields is not supported.
Since revision 15, tasks declare `metadata.runner_protocol = "tool-bench-runner-v3"`
and require a runner-authored receipt bound to exact evidence and legacy-answer
bytes, with post-stop main snapshot and protected execution/source proof. The supported scripted local path is
`sh deploy/run-trial.sh -p <task-directory> -a oracle -o <output-directory>`.
Stock `harbor run --env deploy.runner.strict_docker:StrictDockerEnvironment`
alone lacks the required agent-end hook and cannot produce admitted results.
See [runner-admission.md](runner-admission.md).
The external Argo runner's equivalent admission and artifact behavior still need
independent validation before publishing this revision.

The strict adapter generates a fresh `HARBOR_TRIAL_ADMIN_TOKEN` for each trial. Compose
passes it only to `world`, never `main`, `tools`, or the separate verifier.
`deploy/oracle.sh` selects that controller and adapter for local validation.
An unset credential now prevents startup; generated tasks do not enable insecure
local access.

`main` and `tools` share the frontend network. Only `tools` and `world` join the
internal backend network. No world ports are published. For MCP-only tasks, the
world also enforces the actual tools peer on both HTTP and TLS requests and on
nonlocal admin requests. Forwarded headers cannot override the peer. This is
necessary because some Docker runtimes permit known-IP traffic across bridges.
At agent end, the strict controller stops `main` before main artifact collection.
Harbor subsequently runs the world collect hook, which
calls authenticated `POST /_bench/finalize`. This closes new world requests and
drains admitted requests plus registered background work for at most the declared
20-second window before atomically exporting `/exports/evidence.json`. The evidence
contains the finalization state, execution proof, and separate drain timing.
Only the sidecar artifact
is authoritative; an agent-created file at the same path is not.

The verifier builds from `tests/`, with no agent processes, no MCP connections,
no admin credential, and no network access. It runs `bench-verify` on the trusted
contract, expected scenario identity, exported evidence, and the collected legacy
answer file when present. Missing, unsealed, failed-finalization, or incompatible
evidence fails closed. A failed drain cannot later be retried into a passing attempt;
retain that failure and start a fresh trial under the declared replacement policy.

The installed stock Harbor treats a failure to stop `main` as a warning. The strict
adapter permanently refuses sidecar collection after such a failure, independently
inspects stopped container state, and publishes the required receipt only on valid
collection. An unsupported runner cannot score merely by ignoring the metadata:
the offline verifier rejects missing admission proof. This is not evidence that
the external production runner already implements the protocol.

## 4. Read results

The generated inventory's `summary.scored_source_gaps` counts each declared gap
among scored tasks only, including `legacy_answer_semantics` and
`legacy_answer_file_collection`. The per-scenario `source_gaps` remain
authoritative for selecting a reviewed release. A version-2 declaration or
`scored_declared_unscorable: 0` does not certify complete answer semantics or
runtime admission. Do not derive a release denominator from migration counts
alone, or average provisional rewards across environment/unresolved outcomes.

- Per-trial metrics in `reward.json`:
  - `reward`
  - `outcome_code`: 0 pass, 1 fail, 2 context overflow, 3 integrity violation,
    4 invalid contract, 5 environment error, 6 unresolved/adjudication required
  - Individual state, record, relation, history, preservation, interface, answer,
    and call-check metrics, plus evaluator and contract versions
  - `api_calls` (world requests through MCP servers, minus the servers' own startup requests in `crates/world/startup-calls.json`), `mcp_bypass`
- The verifier keeps `assessment.json` with typed failed checks, `evidence.json`
  with the complete atomic bundle, and `verification.json` with input hashes.
  `journal.json`, `events.json`, `state.json`, and `initial-state.json` are derived
  from that same bundle, not separate live snapshots.
  The input evidence hash refers to the original collected
  `artifacts/world/evidence.json`, not the verifier's reserialized diagnostic copy.
- Evaluator revision 15 requires evidence schema 5, including world-loaded contract/data
  fingerprints, initialized-state provenance and finalization ownership
  proof, pre-dispatch request receipts, and declared clock policy. Historical results retain their original
  evaluator and image labels.
- `runner_admission_checked` and `runner_admission_ok` distinguish a validated
  runner from world-only diagnostics; both must be 1 for the attested tier. Stock
  Harbor publishes no runner receipt, so production trials have
  `runner_admission_checked: 0` and `"runner_attested": false` in
  `verification.json`. That is the unattested tier `bench-report` reads by default.
- `business/shopify/relaunch-tote` requires the composed publication-capable client.
  `ACTIVE` is not channel publication. Revision 14 checks actual online-store
  membership and price-before-visibility history; the upstream-only client is
  not an equivalent condition. See [operation support](operation-support.md).
- `adjudication_needed` records undecided checks. An unresolved-only result has
  outcome code 6 and a provisional compatibility reward of zero, not a proven
  model failure. `bench-report` counts unresolved trials beside the pass rate, never
  inside it; with `--require-attested` it rejects a comparison that contains one.
  Production consumers must implement the same distinction before admitting this
  revision.
- The optional [source-bound content-review path](content-review.md) requires an
  exact policy declared before execution, every required reviewer's signed
  decisions, an external store and explicit report options. Raw/effective results
  and reviewer kinds stay separate; missing/conflicting reviews block final
  comparison. Agent-written approvals and cached summaries are not authority.
  Local scripted controls do not establish human semantic accuracy or admit
  native model runs.
- A trial that could not be assessed (invalid contract or environment error) has
  `verifier/reward.invalid.json` instead of `reward.json`, and Harbor records a
  `RewardFileNotFoundError` in the trial's `result.json`. That is an infrastructure
  error, not a model failure. `bench-report` counts withheld rewards, verifier
  errors, agent or environment exceptions and unfinished trials separately from
  the pass rate; `--require-attested` comparisons reject them, and reject
  inconsistent binary metrics. Retain those attempts and apply only the
  prespecified repair/replacement policy.
- With a trial credential, the world's bootstrap port (`world-bootstrap:9000`)
  serves only health and CA data on a separate tools/world network, not the API
  backend. `BENCH_BOOTSTRAP_BIND_HOST` resolves the world's own private network
  alias before any listener starts, without waiting for tools to start. State,
  grading, reset, evidence and scripted-user
  routes bind to loopback port 9001 inside the world container and still require
  its admin token. Collection uses world-container execution, not an agent network
  route. Readiness checks loopback port 9001. `main` never joins the bootstrap
  network, even on a direct-API track, and does not share the backend network for
  scored MCP-only tasks. Protected startup fails if the bootstrap interface
  is missing or ambiguous.
  A manually started development world can explicitly enable
  `BENCH_ALLOW_INSECURE_LOCAL=1`; this is not a scored-task configuration.
- Legacy answer-file collection remains while trustworthy harness-final-response
  adapters are implemented. The supported controller collects stopped-container
  files and binds their bytes; this is not native completion authentication.
  The offline Harbor path does not classify context
  overflow by grepping agent-writable text. Structured inference rejection evidence
  is still required before restoring that diagnostic there.
- To report on production runs, download each run's artifacts (the run's artifact
  bucket), one directory per arm, and run:

  ```bash
  cargo run -p report --bin bench-report -- runs/arm-a                          # one arm, markdown
  cargo run -p report --bin bench-report -- runs/arm-a runs/arm-b               # arms and paired comparison
  cargo run -p report --bin bench-report -- runs/arm-a runs/arm-b --json
  cargo run -p report --bin bench-report -- runs/arm-a runs/arm-b --explain <task>
  ```

  Trials are found at any depth as a `result.json` beside a `verifier/` directory.
  Per arm the report gives trials and distinct tasks, the pass rate with a 95%
  Wilson interval, the outcome breakdown, pass rates by task group and for
  single-service and `__multi__` tasks, mean and median `api_calls`, and the checks
  that failed most often. The pass rate is passes over assessed trials (pass, fail,
  context overflow, integrity violation); unresolved trials and trials without an
  assessment are counted beside it and listed with their reasons. With two or more
  directories it pairs the arms on the tasks both attempted (per-task pass rates,
  wins, losses, ties and the tasks that flip) and warns when the arms differ in
  evaluator version, task set or contract hashes, because then the numbers are not
  comparable. `--explain <task>` prints the typed failed checks of each of that
  task's trials.

  Download whole trial directories, not just `result.json`. For each trial the
  tool checks that the contract, world evidence, answer, task source and task
  context still match the hashes in `verification.json`, re-runs the assessment
  from them, and requires the saved `assessment.json` and the reward in
  `result.json` to equal the re-run. Such a trial is `reproduced`. A trial graded
  by another evaluator revision than the checkout the tool is built from, or
  downloaded without its artifacts, is `recorded only`: its numbers are the
  verifier's, unchecked. Build the tool from the revision that graded the run. A
  result that contradicts its artifacts is excluded from every rate and listed.

  This is the unattested tier, and the report says so. Stock Harbor publishes no
  runner receipt or realized manifest, so nothing shows that the evidence and
  answer files came from that trial's world and agent, which agent, model and
  images ran, or what the model was shown. Token, cost and timing figures are
  whatever the harness wrote to `result.json`; the verifier does not see them.

  `--require-attested <run-a> <run-b>` keeps the strict paired comparison for
  trials from the strict local controller (`deploy/run-trial.sh`). It refuses
  anything without a verified receipt, labels admitted comparisons
  `source_bound_scripted_only`, and is the only mode that takes
  `--allow-agent-difference`, `--release`, `--information-treatment` and the review
  options. See [execution provenance](execution-provenance.md).

## 5. Verify determinism of any trial

```bash
cargo run -p world --bin world-replay -- scenarios/<scenario-id> <trial>/verifier/journal.json
```

The stricter player first validates complete request context for the whole journal.
It restores only declared synthetic header references, sends the exact captured body,
and does not automatically follow redirects. Missing/redacted context, unsupported
services, and unsupported nonserial ordering are distinguished from response mismatches;
no requests run after a preflight failure. It is not general concurrent-scheduler replay.

The prior Firecrawl authorization failure is fixed for newly captured journals:
fixture credentials are referenced safely instead of being omitted or invented.
Legacy journals still lack this information and are explicitly non-replayable under
the new player, rather than being mislabeled nondeterministic backends. Both world
and tools content tags now include the catalog because its synthetic replay-header
declarations affect runtime behavior.

The tool lists themselves are covered by `deploy/capture-stack.sh`: run it twice and
compare the `sha256` of every snapshot. Actual model-visible schema/exposure preflight
remains a separate requirement.

## 6. Local checks before publishing

```bash
cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings
cargo run --locked -p scenario --bin taskgen -- --check --inventory docs/coverage/scenario-inventory.json
deploy/oracle.sh --all                               # oracle = 1 and nop = 0 for every task
```

After building local images, also run:

```bash
EXPECTED_EVALUATOR_VERSION=29 WORLD_IMAGE=tool-bench-world:dev TOOLS_IMAGE=tool-bench-tools:dev deploy/test-task-isolation.sh
```

Set `EXPECTED_EVALUATOR_VERSION` to the raw evaluator revision of the selected
world image; `29` is an example, not an inferred or permanent default. The script
requires that declaration before starting infrastructure.

This checks actual frontend/backend routing, credential and hidden-file separation,
unauthenticated sidecar access, successful main-container shutdown, and attempts to
forge world evidence and rewards from the agent container. It uses only scripted
oracle executions, not paid model inference. It is a targeted isolation regression,
not a complete evaluation-awareness or information-leakage audit.
