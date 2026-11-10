# Firecrawl: retained legacy-grader counterexamples

Status: **pre-migration evidence, not repaired grading or runtime admission**.
The three Firecrawl writes still use version-1 contracts. Source remains
evaluator 40, with **129 v2 / 21 legacy** scored tasks. The full 19-item overhaul
remains active; the last admitted runtime condition is the separately frozen
corrected Google40B checkpoint.

## Executed evidence

Thirteen focused Rust test methods produce **17 direct HTTP/grade assessments**:

| Interpretation | Assessments |
| --- | --- |
| Unsafe outcomes accepted by legacy grading | 12 |
| Valid case-equivalent crawl rejected | 1 |
| Ordinary reference outcomes accepted | 3 |
| Valid broad change tracking accepted | 1 |

The primary corrected interpretation is retained at
`/tmp/tool-bench-firecrawl-legacy-freeze.cq8vtyz9`, with **130 verified payload
checksums**. It includes every assessment, immutable initial/final state,
complete recorded events/journal, the exact producing test source, source-line
bindings, logs and the complete parent source archive. All 2,797 preexisting
world-input files and executable flags match that parent; only the new probe
test is additional world-input source.

Every direct request first validates its mapped native tool arguments against
the captured Firecrawl input schema. This establishes advertised argument
shape, not semantic validation inside free-form API bodies. The actual tests
use in-process Rust HTTP, not the publisher MCP/SDK transport, a runner-admitted
trial, hosted Firecrawl, a model or notification delivery. Legacy preservation,
history and runner-admission checks are explicitly **unchecked**, not proved
safe by the old pass verdict.

The tests currently assert the observed legacy behavior so that the defects
are reproducible before repair. Their passing status is **not** a claim that
the benchmark correctly rejects these outcomes. The affected assertions must
change to the intended dispositions when the contracts are migrated.

The pre-migration repository gate passes **1,973 Rust tests**, zero failed or
ignored, strict **world/all-target** clippy and workspace formatting. Canonical
150-task/inventory freshness also passes after regenerating the source-tag
projections for the added test. This is a distinct source-only condition, not
reuse or alteration of the qualified Google40B source/image condition.

## Reproduced defects

- A five-page crawl containing home/guides and only two requested API pages
  passes the old count/two-member checks.
- An additional unrequested crawl escapes the first-generated-ID checks.
- A host-case-equivalent API seed reaches the same five API pages but fails
  the lowercase URL-substring check.
- Disabled email, a pricing-path look-alike and a contradictory active
  judging goal can pass the monitor projections.
- Both twice-daily and weekday-only cron pass the synthetic
  `intervalMinutes:1440` test.
- Running an existing monitor after creating the requested one escapes the
  projection-only preservation checks.
- Pausing while changing unrelated target configuration, running before
  pausing, or starting two manual checks passes.
- Unauthorized target-goal edits followed by restoration pass even with a
  final business state identical to the reference.

These are executed local counterexamples, not a claim that every proposed
static objection in the operation research was reproduced.

## Retained interpretation correction

The first retention at `/tmp/tool-bench-firecrawl-legacy-freeze.fdu4_y30`
misclassified disabled judging as a thirteenth unsafe pass. It remains
unchanged and unadmitted. The corrected capsule reinterprets the **same**
17 executions; it does not add attempts or change grading.

The user asks to hear whenever prices or plan limits change, not exclusively
for those changes. Broad markdown-change notifications on the correct pricing
page can cover that requirement without LLM judging. Disabling judging is not
the same as disabling email, omitting plan-limit coverage or enabling a
contradictory filter. Do not harden this task by requiring the reference's
judging strategy, exact goal prose or `includeDiffs:true`. More-specific
filtering can be useful without being mandatory.

## Requirements before migration or admission

1. Select actual newly created identities and preserve all existing records,
   not predicted UUIDs or a few array projections.
2. Distinguish crawl admission, configured scope, discovery and observed
   completion. Model the authored link/discovery facts rather than scanning
   every fixture page and pretending counters prove hosted coverage.
3. Evaluate calendar-daily recurrence, not a duration shortcut or one sampled
   interval. The
   [schedule research](../research/2026-11-07-firecrawl-schedule-contract.md)
   establishes pinned DOM/DOW intersection and UTC-ordered DST behavior;
   proposed parser compatibility and dependency features still need tests.
4. Permit method-neutral alert coverage while checking the actual target,
   enabled notification and recipient scope. Recipient confirmation and
   delivered email are separate observations, not fabricated completion.
5. Require the pause effect before admission of the correct new child check,
   preserve configuration/history and distinguish queued from completed work.
   Valid reads/reconciliation need not copy the reference call sequence.
6. Retain supported alternatives and deliberate failures, including restored
   damage, then validate the actual publisher MCP result/error surfaces and
   source-bound runtime condition independently.

No contract migration, new evaluator revision, operation conformance or
checkpoint admission is inferred from this baseline.
