# Tool-bench: expanded improvement proposals

Date: November 3, 2026.

This expands the [benchmark review](2026-11-03-benchmark-review.md) into proposed changes, implementation seams, and acceptance criteria. **It is a proposal, not a description of implemented features.** Existing benchmark code and historical results are unchanged.

The previous review contains the source-backed findings and validation limits. The [comparison research](2026-11-03-benchmark-comparisons.md) supplies external methodological references. This document adds design recommendations grounded in the current local implementation, not new external benchmark research.

## Guiding decision

Keep one deterministic execution world, but expose separate evaluation questions:

- **Router mechanics:** can the router admit and preserve valid requests and calls?
- **Controlled tool discovery:** can a model find and correctly use tools under specified exposure?
- **Realistic workflows:** can the complete system help an ordinary user safely?

Maximum catalog stress is an exposure condition, not a synonym for realism. Do not make the realistic condition artificially huge, or shrink the stress condition and lose the original experiment.

## P0: establish trust in success and failure

### 1. Replace a list of positive checks with a business-level task contract

**Change.** Extend the evaluator to distinguish:

- Required outcomes.
- Allowed business-state changes.
- Prohibited actions and effects.
- Required causal prerequisites.
- Acceptable final-response facts or artifacts.
- Interface-integrity requirements for the selected track.

The default should be **preserve business state unless the task author allows a particular change**. An allowed resource is not permission to edit every field on that resource.

**Example: duplicate-charge refund.** Permit one refund against the confirmed duplicate payment, corresponding accounting updates, one public response on the correct ticket, and the requested ticket-status change. Do not permit changing the customer email, refunding another payment, modifying a subscription, or editing an unrelated ticket.

Allow necessary derived effects: refund totals, ledger entries, audit records, and related timestamps where the operation contract requires them. Define these centrally with the operation's semantics rather than adding broad per-task exceptions until an oracle passes.

**Implementation seam.** Keep a deep evaluator module with a small interface: task contract plus immutable trial evidence produces an assessment. Hide normalization, record matching, relational checks, and temporal predicates inside that implementation. Collection of network evidence remains outside the evaluator. Evolve `crates/mock-core/src/grading.rs:14`; do not create an externally configurable mini-framework for every individual predicate.

**Acceptance criteria.**

- A correct answer after a SQL mutation fails safe completion on a lookup task.
- Correct target changes plus an unrelated field edit fail preservation.
- Valid accounting/audit side effects pass without allowing unrelated changes.
- Read-only HTTP POSTs do not fail solely because they use POST.
- Alternative valid tool plans reaching the same authorized outcome pass.

### 2. Grade relationships instead of predicted generated IDs

**Change.** Resolve new records from their meaning and relationship to the task, not a predicted random-looking ID or an assumed insertion position.

The current duplicate-charge scenario checks the reply for one seeded refund ID. That makes the response expectation depend on the generator sequence, although the user's requirement is to report the actual refund created for the duplicate charge.

**Example.** Find the refund whose charge is the duplicate payment and whose amount matches the remaining refundable amount; verify that the public ticket response references that refund's actual ID. Verify its status according to the supported operation semantics. Do not require the first ID that a particular oracle happens to mint.

For other tasks, join the created issue to the correct project, the annotation to the intended dashboard, or the assignment to the correct person—not merely any record containing the right words.

**Implementation seam.** Add a small set of named relational checks inside the evaluator, backed by stable store identities. Start with record selection, cardinality, field comparison, and references between selected records; avoid a new general-purpose query language until actual tasks need it.

**Acceptance criteria.**

- Different legitimate generated IDs still pass when the response references the correct record.
- A response referencing a refund from another order fails.
- Two refunds where one was requested fail cardinality or conservation checks.
- Listing a matching ID in a sentence that says no refund occurred does not establish a correct customer response.

Evidence: `scenarios/business/multi/zendesk-refund-via-stripe/scenario.toml:35`.

### 3. Make integrity a gate, while retaining diagnostic results

**Change.** In the MCP-only track, direct-world access or evaluator access invalidates safe completion. Retain separate raw outcome and safety checks so an integrity failure does not erase useful debugging evidence.

Proposed assessment dimensions:

- Task outcome.
- State preservation.
- Policy/temporal safety.
- Interface integrity.
- Answer quality or artifact correctness.
- Environment validity.
- Inference admission and failure details.

The main safe-completion metric requires all applicable dimensions to pass. Never allow high answer accuracy to compensate for a safety violation in a weighted average.

Distinguish **track rules** from universal agent ability. A future workflow condition that permits direct APIs should not call their use cheating. Record permitted interfaces in ordinary working policy and in the hidden evaluator contract.

**Implementation seam.** Update the pass predicate in `crates/mock-core/src/grading.rs:173`, extend the assessment/outcome types, and enforce isolation through generated deployment topology. The evaluator should not infer trusted provenance from a tool argument or agent-controlled header.

**Acceptance criteria.**

- An extra bypass request invalidates a run even when required MCP calls also occurred.
- The original task result and the integrity reason remain visible in artifacts.
- A successful declared direct-API workflow can be evaluated under its own track without being mislabeled.
- Missing admin protection aborts a scored trial rather than starting an exposed evaluator.

### 4. Add semantic events and causal ordering

**Change.** Keep the existing HTTP journal, but supplement it with events representing what actually happened:

- Tool call started/completed.
- Business operation attempted.
- Operation committed, failed, or rolled back.
- Resource fields changed.
- Policy-relevant information obtained.

Each event should carry a trial/session identifier, parent call or operation identifier, origin, and a deterministic ordering relation. Keep duration and user-facing latency on a separate monotonic timing channel.

The existing request journal assigns its sequence number after the response completes. That is not enough by itself to establish causal ordering when requests overlap. A parent identifier and semantic commit event are more useful than assuming an earlier completed request necessarily caused a later one.

**Example.** “Resolve the incident after remediation” should require evidence of the intended remediation before the resolution operation. Scaling, resolving, undoing, and re-scaling should not pass merely because the final snapshots look right.

Record attempted actions separately from effects. A blocked forbidden attempt, a committed forbidden change, and a legitimate request rejected by the backend need not have identical severity. Choose the task's rule explicitly rather than assuming every failed call is unsafe.

**Implementation seam.** Extend `crates/mock-core/src/journal.rs:6` and the operation implementations that commit changes. Observe real and replica MCP calls at their common harness/MCP seam where possible. Do not duplicate causality reconstruction in every service-specific grader.

**Acceptance criteria.**

- A prohibited change followed by restoration is detected.
- Resolution before successful remediation fails a causal prerequisite.
- Legitimate independent parallel reads remain valid.
- A tool that performs multiple operations has traceable child operations.
- Missing required semantic instrumentation is an environment-validation error, not silent evidence that no mutation occurred.

### 5. Separate inference, startup, discovery, and execution failures

**Change.** Stop using an empty API journal or broad transcript grep as the authoritative inference classifier.

Record structured evidence for:

- Environment startup failure.
- Tool registration or schema admission failure.
- Context rejection before inference.
- Context rejection on a later turn.
- Rate limit or transient provider failure.
- Model output that cannot be parsed into a valid call.
- Tool execution failure.
- Policy-correct clarification/refusal.
- Ordinary task failure.

Transient errors may recover. Preserve their events and cost without automatically converting an eventual successful task into failure.

Also distinguish backend startup calls from model-initiated operations, including connector initialization that occurs lazily after a session opens. Clearing the journal once at startup is not a sufficient substitute.

**Implementation seam.** Normalize harness/proxy evidence into a common trial format; replace the competing classification paths in `crates/scenario/src/taskgen.rs:183` and `deploy/tally-arms.py:54`.

**Acceptance criteria.**

- A provider context rejection is correctly classified despite successful MCP startup traffic.
- A later-turn context rejection is distinguishable from first-turn admission failure.
- A recovered rate limit preserves the eventual task result and the actual retry cost.
- The same evidence produces the same classification in local and Harbor reporting.

## P1: make the work surface plausible without introducing hidden advantages

### 6. Introduce versioned organization and user profiles

**Change.** Give the agent an ordinary, deterministic work environment:

- A fictional employee, role, timezone, locale, and active accounts.
- A realistic workspace appropriate to that employee's job.
- Team ownership, operating policies, and reference documents.
- A task-independent permission and approval policy.
- Connected tools appropriate to the organization.
- Consistent people and projects across the connected systems.

An engineer might receive a repository with source, issue references, and on-call docs. A support analyst might receive account notes, a refund policy, and a support workspace. Do not give every persona the same artificial coding repository.

Keep three inputs distinct:

1. **Stable profile:** identity, preferences, role, accounts, standing policies.
2. **Session context:** date, active project, ordinary open documents or scoped history.
3. **User request:** the actual desired outcome.

Session context may vary, but must not be assembled from the solution trace or selected to point at the correct record. Profiles should work across multiple task families.

Use adapters for each harness's native settings and prompt channels. Freeze both the portable profile and the actual resolved agent-visible content; nominally identical profiles can render differently in different harnesses.

**Implementation seam.** Add deterministic profile materialization to task generation near `crates/scenario/src/taskgen.rs:31`. Keep organization datasets and scenario overlays separate from harness adapters. A small materializer interface should return the resolved workspace/config and its hashes; callers should not need to understand every harness-specific setting.

**Acceptance criteria.**

- A profile can run multiple unrelated tasks without editing its standing instructions.
- Profile documents contain no expected answer, target IDs added by the evaluator, tool-ranking hints, or grading instructions.
- No operator home files, real cookies, or real credentials enter the trial.
- Requested actions are authorized or intentionally clarification-dependent under a visible, consistent policy.
- An identical profile seed and task revision produce identical initial content.

### 7. Remove evaluation clues from every agent-visible channel

**Change.** Audit more than the user prompt:

- Initial system/developer/user messages.
- Tool names, descriptions, startup notices, and results.
- Identity endpoints and permission-review responses.
- Workspace names, accessible mounts, process arguments, environment, logs, and history.
- DNS names, service-discovery endpoints, health routes, and exposed URLs.
- Authentication failures and backend error bodies.

Replace evaluator-owned identifiers exposed in ordinary responses with coherent fictional organization identities. Keep authentic upstream terminology intact; a legitimate tool mentioning test fixtures is not automatically a leak.

Keep the evaluator and its credentials on a verifier-only control path. A distinct tools-to-world path must remain available, but sharing a backend network must not inadvertently give the agent access to the admin port.

Do not conceal instrumentation by altering pinned vendor tool definitions or promising that real people are affected. Natural presentation and truthful research reporting are compatible.

**Implementation seam.** Add an agent-phase leakage audit plus guarded deployment checks. Start with the concrete identities and health leaks in `crates/services/kubernetes/src/lib.rs:42`, `crates/services/aws/src/s3.rs:32`, and `crates/mock-core/src/admin.rs:20`.

**Acceptance criteria.**

- Ordinary tool responses and workspace inspection disclose no evaluator-owned scenario IDs, scoring instructions, or solution artifacts.
- Admin state/reset/grade routes are inaccessible from the agent network.
- Logs and credentials are checked through a private audit process without emitting secret values into model transcripts.
- The checklist is passed for actual resolved harness inputs, not only source templates.

### 8. Build an evaluation-awareness sensitivity experiment

**Change.** Treat reduced benchmark awareness as a hypothesis to measure, not a property certified by renaming files.

Use matched diagnostic conditions:

- Neutral versus explicit evaluation framing.
- Ordinary workspace/profile versus the minimal workspace.

Keep task semantics, authorization, business state, tool catalog, harness, and budgets matched as closely as possible. A profile that introduces a refund policy where the minimal condition lacks one is also an information treatment; it cannot isolate awareness.

Measure behavioral outcomes: task completion, unnecessary confirmation, prohibited actions, discovery strategy, and user-facing response quality. Optional post-run questions use a cloned transcript and remain outside scored interaction.

Do not use forced chain-of-thought disclosure as a detector. A self-report of unawareness, or a classifier failing to identify the benchmark, cannot establish that the model was unaware.

**Acceptance criteria.**

- Information differences are listed explicitly for every diagnostic condition.
- A neutral environment passes the leakage audit before being labeled blinded.
- Results support only the stated sensitivity claim, not “the model cannot tell.”
- No diagnostic questions or awareness labels appear during scored tasks.

### 9. Make business time coherent across the whole task

**Change.** Introduce a task/session business clock used consistently by date context, relative-time queries, visible documents, and relevant tool responses. Record inference transport time and performance latency separately.

The current recorder advances the logical clock for any method other than GET/HEAD/OPTIONS. A read-only SQL or GraphQL POST therefore advances it, as can an unsuccessful request. This is reproducible but couples business time to transport shape rather than task semantics.

Prefer a fixed task reference time for “today” and “last hour.” If time progression is part of a scenario, advance it through explicit semantic events or a declared deterministic schedule. Time-sensitive safety checks must use the declared model, not incidental request count.

Do not modify the host clock. Use an isolated supported time abstraction, make the business date explicit where appropriate, and do not break external authentication or inference TLS.

**Implementation seam.** Revisit the transport-driven tick in `crates/mock-core/src/serve.rs:72` and `crates/mock-core/src/serve.rs:110`; resolve task time during profile/world materialization.

**Acceptance criteria.**

- Equivalent read-only operations using different HTTP methods do not change a task's business answer.
- The user, harness date context, and relative-time tool queries agree.
- Replaying the same declared time schedule produces the same world.
- Additional legitimate exploration does not silently move an evaluation window past the relevant records.

## P1: improve semantic fidelity and evaluator honesty

### 10. Publish a supported-behavior contract for the mock world

**Change.** Track support by operation and important behavior, not only by whether a tool name dispatches.

For each relevant operation, identify supported validation, permissions, pagination, filtering, mutations, consistency, error handling, and read-back behavior. Distinguish:

- Supported executable semantics.
- Supported but intentionally empty data.
- Partial implementation with declared limitations.
- Unsupported environment capability.

Do not return empty success for unimplemented functionality. Equally, do not present an evaluator's implementation gap as a faithful vendor error if the real vendor supports the operation.

A full catalog with partially simulated operations may still be useful for protocol stress. It cannot support the same claim as a fully executable workflow environment. Valid alternative model plans need conformance coverage too; testing only oracle operations can reward oracle-shaped behavior.

**Implementation seam.** Start with `crates/services/aws/src/lib.rs:274` and the S3 write/read-back gap in `crates/services/aws/src/s3.rs:742`. Extend existing service tests with reviewed upstream fixtures instead of replacing detailed tests with a generic “every tool answers” loop.

**Acceptance criteria.**

- Successful writes have the documented subsequent read-back and side effects.
- Invalid arguments, pagination, and permissions have reviewed behavior.
- Missing environment functionality is reported separately from model failure.
- Fixtures and normalization rules are versioned and do not require live vendor calls during scored runs.

### 11. Migrate answer scoring deliberately

**Change.** Stop requiring the same answer file for every lookup. Collect the final user-visible message from each harness through an adapter, with explicit rules for resumed sessions, failed finalization, and tool outputs that are not assistant answers.

However, **typed expected facts alone do not solve natural-language evaluation**. A parser that extracts the expected name and amount without understanding negation recreates the present bug.

Choose an honest answer contract by task type:

- State-changing tasks: grade authoritative business state and separately check any important customer communication.
- Naturally structured work: evaluate a user-requested CSV, report, or application record with exact typed checks.
- Bounded factual chat: use a conservative, validated parser for entities, quantities, units, relationships, negation, and uncertainty.
- Open-ended synthesis: use reviewed human adjudication or a separately labeled judge-assisted extension if a robust deterministic evaluator is not practical.

Do not force all normal users to request JSON just to preserve a judge-free label. Do not pretend a fixed judge model guarantees deterministic or unbiased semantic scoring.

Ambiguous parser output should become an explicit unresolved evaluator decision with a predeclared adjudication process. Do not silently drop it from the denominator or report provisional results as final.

**Implementation seam.** Replace the answer-path requirement in `crates/scenario/src/lib.rs:126` only after harness final-message extraction is available. Evolve `crates/mock-core/src/grading.rs:74` with explicit answer types; keep the old evaluator available to reproduce old releases.

**Acceptance criteria.**

- Natural final replies are captured without altering the user request.
- A negated expected answer and a correct fact quoted as a rejected alternative fail.
- Correct currency/unit conversions and permitted answer formats pass.
- Unsupported paraphrases are surfaced for adjudication, not mistaken for evidence of model failure.
- Existing scores retain their original evaluator-version label.

## P2: broaden what the benchmark can diagnose

### 12. Add orthogonal difficulty families

**Change.** Expand beyond more services or longer reference traces. Add tasks varying one meaningful difficulty at a time:

- Entity ambiguity without tool ambiguity.
- Tool ambiguity without entity ambiguity.
- Multi-page evidence and pagination.
- Cross-system identity resolution.
- Stale versus current information.
- Reconciliation between contradictory records.
- Partial completion followed by retry or recovery.
- Idempotency and already-completed requests.
- Permissions and correctly scoped refusal.
- Missing information requiring a useful clarification.
- No appropriate tool or unsupported user objective.
- Untrusted retrieved content trying to redirect the task, as an optional separate safety condition.

Use matched task variants where practical. If a model fails only when a second similarly named customer is added, that is more diagnostic than another unrelated long workflow.

Annotate depth through information dependencies and business operations, not just MCP call count. One SQL query, batched cloud command, or code-execution tool can perform substantial work. Do not force extra serial calls or penalize a valid batched plan merely to make tasks look longer.

Add a trusted-completion/no-action baseline: “this has already been refunded” should sometimes require checking and explaining, not creating another refund.

**Acceptance criteria.**

- Tasks have explicit family/difficulty metadata.
- Appropriate no-action and clarification responses can be correct outcomes.
- Invalid action, valid refusal, and incomplete execution are distinguishable.
- Single-instruction tasks remain self-contained; conversational versions are separately labeled and budgeted.

### 13. Use deterministic failures before simulated users or random chaos

**Change.** Add a versioned failure schedule with stable operation identities and occurrence counters. Examples:

- One selected read returns a transient failure, then succeeds.
- A write commits but its response is lost; retry must avoid duplicating the effect.
- A stale page is followed by a refreshed read.
- A permission check denies a request outside the active project.

A failure keyed only to “request seven” changes meaning when agents take different legitimate plans. Prefer operation/resource/occurrence selectors with documented semantics.

For clarification tasks, start with a scripted user state machine whose permitted replies and available facts are frozen. If an LLM-simulated user is later added, record its model, prompts, sampling, turns, and cost as part of that separate condition.

**Acceptance criteria.**

- Identical semantic schedules are replayable.
- Alternative correct plans do not receive unrelated injected failures.
- Lost-response tests validate idempotency rather than only retry count.
- User simulation cannot leak the hidden target or volunteer solution facts.

### 14. Add discovery diagnostics without overfitting to the oracle

**Change.** Run controlled exposure variants: relevant-tool-only diagnostic, context-feasible catalog, realistic organization pack, full catalog, randomized order, and semantically confusing distractors.

Keep native, universal, and upfront exposure as product conditions. To isolate a retrieval algorithm, also match visible tool names/descriptions, search budget, result count, and loading behavior.

For search metrics, annotate multiple acceptable tools or plans where known. Track:

- Search requests and result rank.
- Recall of useful available operations.
- Selected tools and argument validity.
- Unnecessary schema loading.
- Recovery from an unhelpful search result.
- Whether useful retrieval led to successful execution.

Do not mistake low reference-trace overlap for failure when another valid plan succeeds. A relevant-tool-only result is a result on an easier exposure condition, not proof of a full-catalog upper bound under every possible model behavior.

**Acceptance criteria.**

- Treatment differences are explicit.
- A working alternative plan is not penalized for using a different tool set.
- Discovery inference, execution, and token costs all consume the declared budget.
- Reported catalog size distinguishes stored, registered, provider-accepted, model-visible, and loaded definitions.

## P1/P2: improve reporting, maintenance, and release discipline

### 15. Freeze realized trial manifests and declare comparison treatments

**Change.** Persist what actually ran, not only what the run requested:

- Task, world, evaluator, profile, and failure-schedule revisions.
- Materialized initial-state and workspace hashes.
- Resolved images and available model/provider identifiers.
- Harness and router versions, wire format, and resolved settings.
- Tool schemas before/after relevant adapters and actual exposure.
- Sampling, budgets, retries, cache condition, and run ordering.

Record unavailable identifiers as unknown rather than inventing immutable provider revisions.

Add a comparison specification declaring **what is allowed to vary**. In a router comparison, a changed task revision should block pairing. In a workflow comparison, a changed harness may be the intended treatment and must not be rejected automatically.

**Implementation seam.** Extend `crates/report/src/lib.rs:47` and its input loader to validate invariants before pairing. Match task identity using resolved task/world/contract versions, not just a display name.

**Acceptance criteria.**

- Undeclared task/world/grader changes stop the comparison.
- Declared treatment differences remain visible and are accepted.
- Empty paired intersections are invalid comparisons, not zero-effect findings.
- Missing metadata produces a labeled legacy/non-comparable result instead of a silently compatible one.

### 16. Produce one explainable result format

**Change.** Use one canonical assessment/evidence format for local smoke runs, Harbor grading, and paired reports. Keep a compact numeric reward artifact for compatibility, with a richer companion artifact for explanations.

For a failed check, show the violated contract, affected resource, actual versus expected condition, and supporting events. Separate evaluator faults from agent faults.

Record distinct cost/latency stages: environment readiness, first useful inference, discovery, tool execution, hidden router inference, retries, final response, and whole-workflow completion. Keep raw attempts and prespecified replacements.

**Acceptance criteria.**

- Local and production results agree on the same evidence.
- One failed task can be explained without manually reconstructing three incompatible log formats.
- Infrastructure faults cannot be silently relabeled ordinary model failures.
- Costs include hidden inference and failed/retried requests where measurable.

### 17. Validate the evaluator like production logic

**Change.** Turn the temporary review probes into permanent regression tests when evaluator changes are implemented. For each task family, maintain:

- The reference valid solution.
- An independently authored valid alternative.
- Deliberately incorrect near-miss executions.
- Safety violations with and without restoration.
- Wrong-entity and cross-tenant variants.
- Contradictory answers and failed required operations.

Add property checks where appropriate: unrelated record insertion should not change the expected target; changing generated IDs consistently should not invalidate relationships; a correct state plus an unauthorized change should not pass safe completion.

Validate perturbation preconditions. Adding a new paid order, renaming a uniquely specified person, or changing task-relevant chronology is not automatically semantics-preserving.

**Acceptance criteria.**

- All documented grader counterexamples fail for the correct reason.
- Valid alternatives survive evaluator hardening.
- New tasks cannot enter the scored suite with only oracle=1/nop=0 evidence.
- Contract changes trigger task-family validation and a new evaluator revision.

### 18. Clean up repeated data and authoring rules without a rewrite

**Change.** Introduce immutable organization bases plus task overlays, but materialize a complete dataset per task before execution and hash it. Preserve fresh mutable state per trial.

Generate the coverage inventory from the same scenario/catalog sources used by task generation. Add schema validation for meaningful answer checks, state matchers, referenced tools/arguments, and regexes. A malformed checker should fail authoring rather than quietly reject agents at runtime.

Unify the actual versioned authoring requirements: required task contract, support status, profile, evaluation track, difficulty family, valid alternatives, and negative validations. Do not impose arbitrary equal numbers per service when some services represent much larger capability surfaces.

**Implementation seam.** Extend `crates/scenario/src/lib.rs:97` and task generation. Retain the existing Rust workspace and mock implementations; there is no demonstrated need to migrate language or runner to achieve these improvements.

**Acceptance criteria.**

- Materialized task data is independently inspectable and hash-stable.
- Editing a shared organization base invalidates affected task hashes.
- Two parallel trials cannot mutate each other's state.
- A scenario cannot silently name a nonexistent oracle tool or contain an inert/malformed expectation.
- Documentation counts and current task metadata agree.

### 19. Release held-out families and distinguish uncertainty

**Change.** Split development and evaluation by task family and underlying world. Reserve novel held-out families, not merely paraphrases of public solutions. Entity randomization helps test robustness but does not make a familiar task family novel.

Use repeated attempts with matched budgets and interleaved conditions. Separate uncertainty over repeated model attempts on the fixed suite from uncertainty about generalization to new task families. Keep success, repeat reliability, unsafe-action rates, and admission failures visible rather than compressing them into one score.

Choose attempt counts and any sequential stopping rule before inspecting comparative results. Do not selectively repeat models or tasks until a preferred ranking stabilizes.

**Acceptance criteria.**

- Split assignments, family identities, and evaluator releases are frozen.
- Held-out tasks are not used to tune discovery prompts or graders for a published comparison.
- Related variants are not treated as independent evidence of broad generalization.
- Every reported rate states its denominator, retry semantics, and applicable conditions.
- Historical results are never silently rescored with the new evaluator.

## Suggested rollout

### Milestone A — trustworthy evaluator

Implement proposals 1–5 and permanent regression tests from proposal 17. Add the minimum semantic events needed for the current task families. Migrate contracts in small batches and version the score.

**Release gate:** correct valid alternatives pass; the known counterexamples fail; integrity and environment faults are separately visible.

### Milestone B — realistic-profile pilot

Implement proposals 6, 7, 9, and final-message collection from proposal 11 on a representative subset. Use a support persona and an engineering persona to avoid equating realism with one generic coding workspace.

**Release gate:** the resolved agent-facing environment passes the leakage audit, authorization is coherent, and dates agree. Run the awareness sensitivity experiment separately; do not certify unawareness.

### Milestone C — controlled comparisons

Implement the essential manifest/reporting work in proposals 15–16 and the matched-information conditions in proposal 14.

**Release gate:** a comparison states its invariant fields, declared treatments, budgets, rerun rules, and failure handling. Incompatible runs cannot produce an apparently valid paired score.

### Milestone D — broader capabilities

Expand proposals 10 and 12–13, then formalize the family splits and authoring checks in proposals 18–19.

**Release gate:** added difficulty is named and independently validated; environment support covers legitimate alternative plans, not just the oracle.

## What not to do first

- Do not add more tools simply to increase a headline count.
- Do not rewrite the runner or mock stack before fixing the evaluator.
- Do not hide unsupported operations behind plausible success.
- Do not force JSON or a benchmark answer file into every user interaction.
- Do not silently relax permissions to make cautious agents complete more writes.
- Do not copy the operator's real account state to make the profile believable.
- Do not call every conservative refusal or clarification a model failure.
- Do not claim that deterministic backends imply deterministic inference.
- Do not compare versions with different grading semantics as one continuous leaderboard.
- Do not promise that a model cannot recognize evaluation.

The practical target is a **deterministic, well-isolated work environment with a trustworthy evaluator**, plus clearly separated measurements of router mechanics, discovery, and end-to-end usefulness.
