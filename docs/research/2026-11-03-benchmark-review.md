# Tool-bench review: validity, realism, and experimental purity

Reviewed November 3, 2026, at `4dd037d62a4358535b68b5db2b96289d962fe637`.

## Verdict

**Keep the deterministic infrastructure. Tighten the evaluator before expanding the catalog. Separate router mechanics, tool discovery, and realistic workflow completion rather than asking one score to represent all three.**

The approved design explicitly measures **model + harness + router**, not an isolated model. Its unusually large frozen MCP catalog is a useful stress condition. It is not, by itself, evidence of representative everyday usage or comprehensive agent competence. These are different objectives, not an argument to abandon the current one.

This is a whole-repository methodological review, not a PR-diff review. Benchmark implementation was not changed. Evidence includes source inspection, an inventory of all 150 scored scenarios, 50 existing focused tests, and temporary grader-level counterexample probes. External comparisons are documented separately in [benchmark comparisons](2026-11-03-benchmark-comparisons.md).

## 1. What is already strong

- Pinned vendor/community MCP servers where practical, captured schemas for replicas, and executable Rust backends. Protocol fidelity and semantic backend fidelity can be discussed separately.
- Isolated per-scenario worlds, seeded identifiers, a logical clock, journal recording, and replay tooling.
- Programmatic state and call checks instead of an opaque LLM judge for the core score.
- Both an oracle and a no-op baseline. These establish satisfiability and reject doing nothing; they do **not** establish evaluator soundness.
- A generated task format, checked-in sources, source-content image tags, and CI freshness checks.
- Real disambiguation in task data: similar names, staging versus production, obsolete records, and cross-service identities.
- An existing paired task-level comparison with bootstrap intervals. Build on it rather than replacing it with independent run averages.

Evidence: `README.md:3`, `crates/scenario/src/taskgen.rs:31`, `crates/scenario/src/images.rs:38`, `crates/mock-core/src/grading.rs:146`, `crates/report/src/lib.rs:132`, `.github/workflows/tool-bench.yml:39`.

## 2. Measured inventory

Counts below were computed from the current scored `scenario.toml` files, excluding `scenarios/example`.

| Dimension | Current checkout | Interpretation |
|---|---:|---|
| Services / endpoints / snapshot tools | 30 / 32 / 1,595 | A catalog-size measure, not a semantic-coverage measure |
| MCP service modes | 14 real, 8 real-TLS, 8 replica | All still use simulated vendor backends |
| Single-service / cross-service tasks | 120 / 30 | 80% / 20%; cross-service does not dominate |
| Services per cross-service task | 26 use two; 4 use three | No scored task currently names four services |
| Lookup / write tags | 50 / 100 | A useful initial mix |
| Tasks with answer checks | 55 | All 55 ask for `/app/answer.md` |
| Tasks with state / required-call / forbidden-call checks | 100 / 120 / 131 | Guard count does not establish guard completeness |
| Oracle calls: median / mean / maximum | 2 / 2.87 / 8 | Reference trace lengths, **not** proven minimum plan lengths |
| Tasks with at most two oracle calls | 78 / 150 | 52%; catalog breadth is much larger than workflow depth |
| Distinct endpoint/tool pairs used by oracles | 217 | About 13.6% of snapshot tools; not total implemented or reachable coverage |
| Dataset documents per collection: median / maximum | 4 / 80 | A collection is not an entire tenant; nevertheless, test larger cluttered worlds |

The goal is not to inflate any count. Add difficulty dimensions deliberately: entity ambiguity, joins, pagination, policy constraints, horizon, error recovery, irrelevant context, and tool ambiguity.

## 3. Correctness issues to fix first

### P0 — Read-only success can coexist with destructive SQL

`cloud/supabase/top-customer-march-revenue` has an answer check but no state-preservation check. Its forbidden POST paths cover pause/restore/migrations/deployment/branches, not the SQL query endpoint. A query that mutates data is not caught by these HTTP-path guards.

I passed the **actual scenario expectations** to the grader with a constructed changed state, a journaled SQL `UPDATE` request, and the expected answer. It returned `reward=1` and `forbidden_calls_ok=true`.

**Fix:** enforce unchanged normalized business state for read-only tasks. For write tasks, declare an allowed change set and protect everything outside it. Supplement this with semantic mutation events for attempted or reverted harmful changes. HTTP verbs alone cannot distinguish a read-only SQL/GraphQL POST from a mutation.

Evidence: `scenarios/cloud/supabase/top-customer-march-revenue/scenario.toml:14`, `crates/mock-core/src/grading.rs:152`, `crates/mock-core/src/grading.rs:232`.

### P0 — MCP bypass is counted but does not invalidate success

The pass predicate includes four component checks, but not `mcp_bypass == 0`. A constructed answer-only task with one direct-world request and a matching answer returned `reward=1, mcp_bypass=1`.

Some tasks' required-call checks prevent an exclusively bypassed solution from passing. They do not reject additional bypasses alongside legitimate calls, and tasks without required calls lack even that protection.

**Fix:** block agent-to-world access at the network layer and reject bypassed trials in the MCP-only track. Preserve a distinct integrity outcome rather than quietly treating bypass as an ordinary task failure. If direct API access is intentionally allowed in a future workflow track, label and grade that track differently.

Evidence: `crates/mock-core/src/grading.rs:146`, `crates/mock-core/src/grading.rs:173`, `crates/scenario/src/taskgen.rs:105`.

### P1 — Keyword answers are not semantic answer checks

The answer evaluator only combines case-insensitive substring checks and an optional regex. A contradictory answer that names the expected person and amount as rejected alternatives can still satisfy it. A matching required call also need not have succeeded: `call_matches` does not inspect response status or application-level errors.

My probes accepted a contradictory answer and, separately, a required call with status 500. The latter used a minimal constructed expectation; it is a limitation of the matcher, not evidence that every existing task can pass on failed calls.

**Fix:** use typed facts, numeric units/tolerances, entity relationships, and state-derived answers. Score the normal final assistant message rather than requiring a benchmark answer file. For free-form synthesis, use a small reviewed rubric or clearly separate any judge-assisted extension. Require successful semantic evidence where task completion depends on a call; do not add irrelevant mandatory calls to otherwise outcome-based tasks.

Evidence: `crates/mock-core/src/grading.rs:74`, `crates/mock-core/src/grading.rs:232`, `crates/mock-core/src/grading.rs:286`.

### P1 — Startup traffic prevents overflow classification

Overflow is recognized only when the complete tools-origin API journal is empty. MCP startup requests populate that journal before the model does useful work. A probe with a startup GET and `context_overflow=true` returned `Fail`, not `ContextOverflow`.

The existing smoke documentation already acknowledges this and the Python tally works around it. The core verifier/reporting path should not disagree with the smoke tally.

**Fix:** record model requests, startup activity, discovery, and agent-initiated calls as different phases. Classify provider rejection from structured inference events, including overflow after earlier valid turns. Record recoveries rather than equating any transient rejection with a failed task.

Evidence: `crates/mock-core/src/grading.rs:174`, `crates/scenario/src/taskgen.rs:183`, `docs/research/arms-2026-10-31.md:36`, `deploy/tally-arms.py:54`.

### P1 — Control-plane protection is optional, and health reveals the scenario

The admin token is a good existing safeguard when the external runner injects it. However, the world starts unguarded when it is missing, and generated compose defaults it to an empty value. The public health endpoint returns the scenario ID.

**Fix:** make scored runs fail closed without an admin credential, explicitly opt in to insecure local development, and move admin traffic onto a verifier-only network. Return only readiness from any agent-reachable health route. Verify runner injection and agent-phase mount visibility in an actual Harbor run; the runner implementation is outside this repository and was not inspected here.

Evidence: `crates/world/src/main.rs:31`, `crates/world/src/lib.rs:97`, `crates/mock-core/src/admin.rs:20`, `crates/scenario/src/taskgen.rs:117`.

### P1 — An answering endpoint is not a faithful implementation

AWS's generic fallback returns empty successful responses for unmodeled services. Some S3 configuration writes are accepted without storing their effect. These can make an invalid action look harmless or a nonexistent capability look functional.

The repository has substantive service tests; nevertheless, “every tool answers” tests such as the Linear dispatch test primarily establish routing, not real-world semantic equivalence.

**Fix:** publish a tool/operation conformance matrix: supported semantics, partial behavior, realistic error, or out of scope. Fail unsupported operations explicitly rather than inventing success. Add frozen upstream contract fixtures, positive and negative argument cases, pagination, permissions, and mutation/read-back checks. Preserve raw captured schemas separately from any compatibility adapters.

Evidence: `crates/services/aws/src/lib.rs:274`, `crates/services/aws/src/s3.rs:742`, `crates/services/linear/tests/linear.rs:25`, `docs/research/smoke-2026-10-31.md:28`.

## 4. Separate three experimental questions

| Track | Hold constant | Vary | Report |
|---|---|---|---|
| **Protocol / router mechanics** | Recorded requests, tool catalog, deterministic executor | Router implementation or serialization strategy | Admission, schema/tool-call preservation, request size, added latency, failure category |
| **Tool discovery / calling** | Minimal standardized harness, task, world, model/provider, budgets | Exposure/retrieval method, catalog size/order, semantic distractors | Relevant-tool recall, argument validity, discovery cost, downstream completion |
| **Workflow completion** | Versioned user/organization/environment profile within each comparison | Complete model+harness+router systems | Safe outcome success, policy compliance, reliability, user-facing latency, total cost |

The first track can isolate router mechanics without model sampling. The second measures model and discovery interaction under a controlled harness. The third intentionally measures a product-like system; it is not a pure model leaderboard.

**Keep the existing all-tools maximum-stress condition.** Add a context-feasible control and realistic tenant-specific tool packs, selected from the target deployment population rather than an arbitrary guess at a “normal” tool count. Do not mix their results into one headline percentage.

For discovery, include an oracle-relevant-subset diagnostic, fixed-size catalog sweeps, shuffled catalog order, hard same-purpose distractors, and “no suitable tool” cases. An oracle subset diagnoses the effect of removing distractors and retrieval overhead; it is an easier exposure condition, not a guaranteed performance upper bound or a fair production baseline. Accept multiple valid tool sets rather than treating the supplied oracle trace as the only solution.

Native ToolSearch and universal discovery expose different information in the existing study: the notes say native advertises deferred names while universal does not. This is a legitimate **product-level** comparison if it matches deployment. It does not isolate the retrieval algorithm. Add a matched-information arm for that claim.

Evidence: `docs/superpowers/specs/2026-10-30-tool-bench-design.md:7`, `docs/research/arms-2026-10-31.md:3`, `docs/research/arms-2026-10-31.md:30`.

## 5. Make the agent-facing environment ordinary

### What currently gives it away

These are observable cues or plausible inference channels, **not proof that any tested model recognized the evaluation**.

1. **Explicit product-independent identities in responses.** Kubernetes exposes the user `toolbench` and a `toolbench-ops` role binding. S3 emits `toolbench-owner` and `toolbench/…` host IDs. Hiding the README would not remove these.
2. **Mandatory answer artifact.** All 55 answer-checked tasks request the same `/app/answer.md` file. Scenario validation enforces that wording. This trains the prompt format around the evaluator instead of a user's request.
3. **Bare workspace.** The agent image is a minimal Debian environment with `/app` as its working directory. This repository does not provide a plausible working project, local business context, or user profile.
4. **Task-shaped worlds.** Only selected service datasets are loaded; other service stores can be created empty. A giant catalog attached to a mostly sparse tenant can reveal the exercise structure through ordinary exploration.
5. **Control-plane metadata.** The reachable health route exposes the scenario slug. Other test/rubric/solution paths must be checked in the actual agent-phase container rather than assumed hidden or visible.
6. **Time discontinuity.** Scenario clocks span different dates, but the main image does not coordinate the agent's clock/profile with `scenario.now`. Relative requests such as “last hour” can conflict with a harness date or shell clock.

Evidence: `crates/services/kubernetes/src/lib.rs:42`, `crates/services/kubernetes/src/lib.rs:1619`, `crates/services/aws/src/s3.rs:32`, `crates/services/aws/src/s3.rs:81`, `crates/scenario/src/lib.rs:126`, `crates/scenario/src/taskgen.rs:166`, `crates/mock-core/src/world.rs:71`.

### Proposed user-like profile

Use a reproducible **fictional user and organization**, not a copy of the operator's actual home directory or credentials.

```text
/home/maya/
  workspace/payments-api/
    README.md
    docs/on-call.md
    docs/customer-refunds.md
    ordinary source files and believable git history
  organization/
    team-directory.md
    service-ownership.md
    working-agreements.md
```

Feed each harness its normal supported configuration for:

- User identity, team, timezone, locale, and a scenario-consistent current date.
- Installed tools and connected accounts, including a clear active tenant and least-privilege identity.
- Ordinary authorization rules: which actions are preauthorized and which require approval.
- Stable user preferences about concise replies, communication channels, and workspace use.
- Non-task-specific background documents and synthetic history shared across a coherent organization.

Keep profile settings **task-independent**. A user's preferences must not encode the intended service, target entity, correct plan, or reward requirements. Keep exact native configuration syntax in per-harness adapters; a portable profile manifest is not itself a native configuration file.

Seed plausible background activity across connected services, with consistent people, projects, IDs, timestamps, and links. Incorrect service choices should encounter believable irrelevant information, not a uniquely empty tenant. Vary writing styles and request completeness based on reviewed user-like requests, not only template paraphrases.

**Example ordinary policy context:** “You are helping Maya on the customer operations team at Northwind Bakery. Routine duplicate-payment refunds may be issued after both successful charges are confirmed to belong to the same order. Reply on the original support ticket. Ask before changing an unrelated subscription or payment.”

**Example ordinary request:** “Hannah says we charged her twice for wholesale order #1077. Can you check and sort it out, then reply on her ticket?”

The current more explicit version remains useful as a separate fully specified task. The shorter version measures policy retrieval and missing-context handling too; do not pretend these are equally difficult or silently switch the main track.

The host smoke script already warns that enabling Bash exposes the operator's real login and dotfiles. Do not solve realism by relaxing that boundary. Use the same isolated container path for smoke and scored runs.

Evidence: `deploy/smoke-arms.sh:8`, `scenarios/business/multi/zendesk-refund-via-stripe/instruction.md:1`.

### Blinding, not guaranteed invisibility

Aim for **benchmark-blinded agent inputs**, not a claim that the model cannot know. Keep the evaluation transparent to researchers; remove labels and privileged artifacts from the agent's ordinary work surface. Do not add “this is not a benchmark” instructions or falsely tell it that real customers are at risk.

Research reports both evaluation-awareness effects and the use of environmental cues, but that does not demonstrate awareness in this repository. LURE is a preprint describing filesystem, environment, and prompt leakage channels; Apollo reports evaluation/deployment behavior differences in studied settings.[^lure][^apollo]

Add an observer-side leakage test that inventories the initial system/user messages, serialized tool definitions, tool results, working directory, environment variable **names**, accessible mounts, DNS/network surfaces, and identity responses. Scan evaluator-owned markers such as scenario IDs, rubric paths, solution filenames, and fake principal names. Do not remove legitimate upstream uses of “mock,” such as Terraform Sentinel mock exports.

Run a separate paired sensitivity study: same task/world/config with neutral presentation versus explicit evaluation framing, plus realistic versus bare profiles. Measure behavioral/performance differences; an ability to distinguish the settings is not the same as a demonstrated impact on results. Optional post-run awareness questions belong on a cloned diagnostic transcript, never in the scored interaction. Self-reports or a classifier failing to detect cues cannot prove unawareness.

Do not modify the host clock to align dates. Coordinate the harness's date context and a safe isolated time abstraction, or make the business date explicit in the user context. Keep the experiment clock separate from external inference transport timestamps.

## 6. Improve grading without making it an exact-trace contest

- **Outcome:** did the requested business change occur on the right entity?
- **Preservation:** did unrelated business state stay unchanged? Normalize volatile timestamps and documented bookkeeping, not arbitrary differences.
- **Safety:** did any prohibited semantic action occur, even if later undone?
- **Ordering:** did required prerequisites precede irreversible changes? The remediation task should not pass simply because both “scaled” and “resolved” eventually appeared.
- **Answer:** does the final response assert the correct typed facts, not merely contain their tokens?
- **Evidence:** was required information actually obtainable through permitted interfaces? A guessed answer should not establish discovery competence.

The current journal has sequence numbers, so temporal predicates can build on existing infrastructure. Introduce them only for genuine causal or safety requirements, not to force a preferred plan.

For each task, test at least one independently authored valid solution and invalid variants: wrong tenant, wrong similarly named entity, skipped subgoal, duplicate action, unrelated mutation, mutation then rollback, contradictory answer, direct API bypass, and failed required call. Keep oracle=1 and nop=0 as the beginning of validation, not the end.

Evidence: `crates/mock-core/src/journal.rs:6`, `crates/mock-core/src/grading.rs:14`, `scenarios/ops/multi/checkout-saturation-scale-and-resolve/scenario.toml:47`.

## 7. Strengthen experimental hygiene and cleanup

### One immutable manifest per trial

Record task/world/grader revisions; organization/profile seed; runtime image **digests**; base image and package resolution; harness version; model/provider and router revision; sampling settings; API protocol; native tool-handling settings; actual offered/visible/loaded tool hashes and tokenizer-specific sizes; search limits; budgets; retry policy; and cache condition.

Source-content tags are a good start, but they are not immutable OCI digests. Current Dockerfiles use mutable base tags and apt repositories. Persist the realized image digests rather than assuming a source hash fully identifies a rebuild.

Validate comparability before producing a paired result. The current report pairs by task name and holds only task, reward, and outcome per trial; it cannot reject different datasets, graders, catalogs, or harness settings. An empty paired intersection currently yields zero means/zero-width intervals rather than an invalid comparison.

Evidence: `crates/scenario/src/images.rs:38`, `images/world/Dockerfile:4`, `images/tools/Dockerfile:5`, `crates/report/src/lib.rs:47`, `crates/report/src/lib.rs:142`, `crates/report/src/lib.rs:206`.

### A single results/metrics implementation

Consolidate the Rust report and Python smoke tally into one result schema. Separate startup requests, MCP calls, backend requests, discovery calls, provider steps, and hidden router inference. Include cost and latency for all of them. Track the actual number of tools visible to the model; an advertised tool count can differ from the harness-visible count.

The current replay compares status and parsed body equality, not literal response bytes or headers. Either narrow the documentation to that guarantee or capture and compare the raw information needed for a true byte-level claim. Also test the MCP results the model receives, not only backend responses.

Evidence: `deploy/tally-arms.py:43`, `crates/report/src/lib.rs:47`, `crates/world/src/replay.rs:68`, `crates/mock-core/src/journal.rs:6`, `docs/research/smoke-2026-10-31.md:25`.

### Statistics and failures

- Keep paired task-level aggregation and repetitions. Report uncertainty appropriate to the question: task/family resampling for generalization across tasks; repeated-attempt uncertainty for reliability on a fixed suite.
- Cluster related variants by task family or shared world. Renamed entities and repeated seeds are not independent novel tasks.
- Interleave/randomize arm order and keep provider, sampling, budgets, and cache treatment matched.
- Report pass-on-one-attempt, repeat reliability, and safety separately. Best-of-k success is not repeated-run reliability.
- Show both overall deployment success and task success conditional on an admitted, functioning environment. Context overflow counts against deployability; infrastructure faults need a separate, prespecified treatment.
- Predeclare rerun/exclusion rules. Record replaced attempts instead of retaining only the successful retry.
- Preserve the known all-arm failures as diagnostics until independently checked. Failure by every model is not proof a task is broken; oracle success is not proof its prompt is adequate.

The existing 18-task arm study is useful pilot evidence, not a validated result over all 150 scenarios. Its host execution, disabled Bash, local-versus-production router targets, and manually rerun infrastructure failures should remain explicit.

Evidence: `docs/research/arms-2026-10-31.md:3`, `docs/research/arms-2026-10-31.md:30`, `deploy/smoke-arms.sh:20`.

### Dataset organization and contamination

Define public development tasks, a frozen evaluation suite, and a held-out/rotating challenge set. Split by task family, underlying world, entities, and intent—not random instruction rows. Keep solutions and grading definitions inaccessible during agent execution. Releasing public tasks improves reproducibility but does not guarantee later models have not trained on their answers.

Factor repeated data into versioned coherent organization bases plus scenario overlays, while preserving a fully resolved materialized dataset hash per task. Do not introduce implicit shared mutable state between scored trials.

Keep one machine-generated inventory for counts, tags, schema tokens, and support status. The design's aspirational task mix and the authoring guide's “four per service” rule are not the same specification; document the actual versioned coverage. Distinguish historical captures/run counts from the current catalog rather than silently updating old results.

Evidence: `docs/authoring.md:70`, `docs/superpowers/specs/2026-10-30-tool-bench-design.md:193`, `docs/research/arms-2026-10-31.md:3`.

## 8. What to borrow from other open-source benchmarks

| Benchmark family | Most useful lesson here | Important boundary |
|---|---|---|
| MCPMark | Reviewed task validity, programmatic verification, versioned reproducible environments | Sharing MCP does not match backend fidelity or catalog exposure |
| ToolSandbox | Dependency-aware milestones and negative “minefields” for unsafe trajectories | Conversational simulated state is a different construct |
| AppWorld | Expected **and allowed** database changes; coherent app/person context | Its API/code execution is not this MCP exposure experiment |
| τ-bench / τ²-bench | Explicit policy context, user interaction, repeated-run reliability | Keep simulated-user variance outside the deterministic core |
| BFCL | Separate invocation/relevance/arguments from stateful completion; missing-tool cases | AST/call agreement is not automatically business success |
| ToolBench / StableToolBench | Retrieval versus completion; instability controls | LLM judging/simulation changes the current judge-free contract |
| ToolRet | A focused query-to-tool retrieval diagnostic | Tool recall is not safe executable task completion |

These are design comparisons, not score comparisons. Exact upstream versions and primary-source evidence are in [the companion comparison research](2026-11-03-benchmark-comparisons.md).

## 9. Recommended sequence

The [expanded improvement proposals](2026-11-03-benchmark-improvement-plan.md) translate these recommendations into 19 concrete work items, implementation seams, acceptance criteria, and staged release gates.

1. **Repair trust in the score:** preservation checks, bypass invalidation, typed answer checks, successful-call semantics, and phase-aware outcome classification. Acceptance: all counterexample probes fail appropriately without rejecting independently valid solutions.
2. **Harden isolation:** mandatory admin protection, verifier-only control plane, agent-phase filesystem/network audit, and source-visible “toolbench” identity cleanup where exposed. Acceptance: no rubric/solution/control-plane access through normal agent capabilities.
3. **Run a realistic-profile pilot:** a small coherent organization pack, ordinary workspace/config, final-message capture, and aligned date/identity/policy context. Acceptance: the neutral presentation contains no evaluator-owned clues and policy-correct refusals/clarifications are distinguished from task errors.
4. **Publish separate scoreboards:** router mechanics, controlled discovery, and workflow completion with everyday-pack versus full-catalog stress conditions. Acceptance: every claim states which variables were held constant.
5. **Expand difficulty and fidelity:** longer workflows, realistic irrelevant data, pagination, permission failures, retries/idempotency, stale information, no-action cases, and held-out families. Use deterministic failure schedules first; keep simulated-user and learned-judge extensions separately labeled.

**Do not start by adding more tools or disguising the prompt.** The best next increment is a better evaluator and a credible, reproducible work environment.

## Validation performed and limits

`cargo test -p mock-core -p scenario -p report`: **50 tests passed**.

`cargo run -q -p scenario --bin taskgen -- --check`: **150 tasks up to date**, after removing the temporary probe source. A check while that source existed correctly reported different content-addressed image tags; no generated tasks were modified.

Temporary probes called the actual `mock_core::grading::grade` implementation:

```text
contradictory_answer_and_bypass: reward=1 bypass=1
failed_required_call: reward=1
overflow_after_startup: outcome=Fail
actual_readonly_scenario_with_sql_mutation: reward=1 forbidden_calls_ok=true
```

These were **constructed grader inputs**, not live model rollouts or full API exploitation. The temporary source was removed; benchmark behavior is unchanged. No paid model matrix, Docker oracle sweep, live vendor contract comparison, or external Harbor runner inspection was performed. Historical run outcomes are reported from repository documents, not independently rerun.

[^lure]: LURE authors, [Evaluation Awareness in Language Models via Leakage in Realistic Environments](https://arxiv.org/abs/2511.02192), preprint, including reported filesystem/environment/prompt leakage channels.
[^apollo]: Apollo Research, [Frontier Models are Capable of In-context Scheming](https://www.apolloresearch.ai/research/scheming-reasoning-evaluations), including studied evaluation/deployment behavioral differences. These findings are not evidence about this benchmark's tested models.
