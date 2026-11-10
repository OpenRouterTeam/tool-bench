# Full improvement plan: integration status

The goal remains active. This is an integration ledger, not a completion
certificate. The acceptance criteria in
`research/2026-11-03-benchmark-improvement-plan.md` remain authoritative.
An exported type, standalone command, passing schema or scripted fixture does
not establish that a production consumer uses it.

## Source baseline

The protected local baseline is `8ec14bbb`; the draft PR head reported by the
existing pipeline is `67ad344c`. Their production code is identical, but branch
synchronization requires preserving both histories and the pipeline's custody.
Implementation continues in isolated worktrees rather than bypassing that
gate. New source changes here are uncommitted and not published.

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
| Native inference and final answers (3–5, 11, 15–16) | A separate source slice now observes the real Harbor `Computer1` lifecycle and wires protected receipts, verifier/report consumers and explicit answer channels. Integrate it, qualify the actual runtime/provider, complete failure export and SDK/settings/timing coverage, and deliberately migrate natural-response tasks. This computer-use capture is not the native-MCP exposure arm. |
| Answer semantics (11, 17) | Close the baseline's 50 legacy-answer-semantic gaps. Proposed source-bound substring checks still cannot establish negation or relationships and are not accepted as semantic completion. |
| Resolved leakage and awareness (6–8) | Audit actual resolved runtime surfaces privately; run the prespecified matched awareness experiment. Standalone source scans and condition manifests are preparation only. |
| Controlled exposure (14) | Connect conditions to actual runtime registration/loading, retain authentic pinned definitions, account for the complete budget, and observe the relevant catalog stages. A byte-length proxy is not provider context admission. |
| Organization materialization (6, 18) | Wire base/overlay data into the actual dataset loader and generated task/world source bindings; test shared-base mutation and separate mutable trial state through those consumers. A loader helper not called by `Dataset::load` does not meet this criterion. |
| Families and clarification (12–13, 17–19) | Freeze meaningful family/dependency metadata and integrate bounded, fact-safe clarification/no-action outcomes. Deriving a family from a service name or counting diagnostic fixtures as scored tasks is insufficient. |
| Release admission (15–19) | Use frozen release/split identities in the actual report comparison path; declare budgets, repeats, replacements and denominators before observing results. Release types without a report call site are not admission. |
| Final integration | Resolve the protected branch custody question, integrate accepted changes, bump the evaluator deliberately, regenerate inventory/tasks, and repeat focused and broad validation on the integrated source. |

## External gates

The PR remains draft/do-not-merge. Completion additionally needs authorized
native-model runs, production consumer qualification, final published-image
qualification, private resolved-surface custody, human adjudication where
required, and genuinely novel held-out-family governance.

No paid benchmark inference or live-account changes are authorized by this
ledger. Historical results retain their original evaluator and evidence
labels. Temporary local evidence is not a substitute for durable release
custody.
