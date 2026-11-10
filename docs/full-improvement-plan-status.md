# Full improvement plan: integration status

The goal remains active. This is an integration ledger, not a completion
certificate. The acceptance criteria in
`research/2026-11-03-benchmark-improvement-plan.md` remain authoritative.
An exported type, standalone command, passing schema or scripted fixture does
not establish that a production consumer uses it.

## Source baseline

The current local integration base is `e39b0e06`, descended from the published
`67ad344c` head and preserving its pipeline fixes. The five subsequent
workstreams have been combined as uncommitted source changes, not published
or certified as a finished release. No branch history has been replaced by
this integration.

The unpublished integration uses evaluator **45** and evidence schema **7**.
Canonical generation validates 154 scenarios and emits only the 150 scored
tasks. The task/inventory freshness check and refrozen development-manifest
check pass. Earlier passing checks still do not certify the entire current
uncommitted tree or the availability of the newly referenced images.

The baseline frozen source run reports 2,279 passing tests in 203 executables,
with no failures or ignored tests. That is source evidence, not native-model,
published-image or production-runner qualification.

## Current source slice

`semantic-fault-schedules.md` documents the wired deterministic Stripe fault
path. It is used by the real dataset loader, shared REST/MCP dispatch,
transport recorder, sealed evidence and grader. Loopback tests exercise an
actual lost response after commit and verify retry accounting.

Independent review identified a cache-hit occurrence bug. The repair excludes
idempotency replay and key-mismatch rejection from handler-attempt counters.
A regression targets occurrence two, not just occurrence one. Encoded resource
paths and accepted trailing slashes use the same decoding during dispatch and
evidence validation. Runtime schedule replacement returns an error rather
than panicking.

This covers the first three acceptance criteria of proposal 13 at source-test
scope. It does not establish fault-conditioned model performance or complete
the scripted-user criterion.

## Outstanding integration

| Area | What still must happen |
| --- | --- |
| Native inference and final answers (3–5, 11, 15–16) | Both Computer1 host capture and the Claude Code provider-proxy path are combined with verifier/report consumers. Qualify actual CLI behavior, runtime networking/provider operation, SDK/settings/timing coverage and deliberately migrate natural-response tasks. Computer1 remains a computer-use condition, not native-MCP exposure. |
| Answer semantics (11, 17) | All 50 scored lookup fact contracts now use source-derived statements with affirmative clauses; the unscored no-action diagnostic does too. This is an authored-source count, not a claim that all answer regressions or semantic qualification pass. Preserve independently valid replies, reject stale explanatory values and roles, and retain adjudication for unsupported wording. |
| Resolved leakage and awareness (6–8) | Resolved container and MCP capture is wired. Claude Code proxy requests now supply actual public harness-message scans; Computer1 harness inputs and actual runtime process observations remain missing. Required uncaptured surfaces keep actual audits unqualified. Run the prespecified matched awareness experiment only after runtime qualification. |
| Controlled exposure (14) | Connect conditions to actual runtime registration/loading, retain authentic pinned definitions, account for the complete budget, and observe the relevant catalog stages. A byte-length proxy is not provider context admission. |
| Authoring evidence (10, 17) | Captured-argument constraints and declaration loading are wired. Existing file/function references establish that evidence exists, not that its body proves a valid alternative, each near-miss class or the declared operation semantics. Complete the behavioral review rather than treating name-derived declarations as qualification. |
| Organization materialization (6, 18) | The real loader, taskgen, runner, verifier and report replay consume the source-bound private bundle. Shared-base mutation, ordered state, trial isolation and tampering tests exist; final integrated validation and published-image qualification remain. |
| Families and clarification (12–13, 17–19) | Family metadata and unscored interaction diagnostics are combined. The no-action reply binds refund, order and UTC refund date from the authored state. Native question forwarding and independently reviewed family assignments remain; diagnostics do not inflate the scored denominator. |
| Release admission (15–19) | Frozen release admission and controlled information treatments share the comparison path. Replacement admission now requires verified source, evaluator, execution and budget identity. Actual ordering/budget evidence and genuine held-out governance remain incomplete. |
| Final integration | Canonical tasks, inventory and the development manifest are regenerated for unpublished evaluator 45/schema 7. Complete the full integrated gate and regenerate again after any source-input changes before draft delivery. |

## November 10 integration safeguards

Shared-checkout baseline testing temporarily stashed the entire tracked tree.
Those workers were stopped. The two recovered snapshots match, current source
was compared against them, and changed/untracked files were backed up without
a blanket restoration. Baseline comparisons must not stash or reset a shared
checkout. No integration commit or push followed this incident.

The subsequent whole-corpus generation gate also exposed 35 missing family
declarations and seven missing capture constraints. Only those authored fields
were restored from the original D/E workstreams, not whole scenario files.
All 150 scored family tables now match the authored family slice.

Timestamp facts now validate calendar dates and UTC offsets, and exact seconds
or full RFC 3339 replies bind their actual source instant. Neutral-context slots
reuse the typed assertion reader, including supported units, currency and
identifier prefixes. Fixed date prefixes, seeded staging values and `:00`
stripping no longer certify stale Datadog answers.

Optional explanations still require observed collections and valid source
fields. A related selector may explicitly declare `allow_absence = true`;
only optional facts and similarly optional descendants may depend on it.
Zero matches then leave no value that a reply can assert. Missing collections,
missing fields on a matched record and ambiguous matches remain errors.

Host terminal parsing requires an integer exit status, an explicit successful
result envelope and one consistent nonblank init/result session. Completed
captures require the terminal digest to match the provider reply. CLI error
envelopes do not erase a separately observed provider context or budget failure.
The positive completion/recovered-retry fixtures now traverse the pinned
`ClaudeCode.run` with fake execution and a loopback provider; this is not real
CLI, Docker, networking or provider qualification.

The focused gate passes 203 Rust tests, including the report's proxy consumers,
plus all 221 runner Python tests. Workspace all-target compilation, strict
all-target lint without allowances, and canonical freshness checks pass.
The complete Rust workspace test run and external qualification gates remain
separate requirements; passing source checks does not complete the plan.

## External gates

The PR remains draft/do-not-merge. Completion additionally needs authorized
native-model runs, production consumer qualification, final published-image
qualification, private resolved-surface custody, human adjudication where
required, and genuinely novel held-out-family governance.

No paid benchmark inference or live-account changes are authorized by this
ledger. Historical results retain their original evaluator and evidence
labels. Temporary local evidence is not a substitute for durable release
custody.
