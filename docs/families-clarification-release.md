# Difficulty families, interaction outcomes and frozen release admission

Source implementation for improvement-plan items 12, 13 (scripted-user
criterion), 15, 18 and 19. It is consumer-path tested, not a native-model
campaign, a published image, a production runner qualification, or a held-out
release. No score is produced or implied by anything here.

## Reviewed family metadata (`[scenario.family]`)

Every scenario outside `example/` carries a reviewed table directly after its
`[scenario]` keys:

```toml
[scenario.family]
schema_version = 1
primary = "entity_ambiguity"
secondary = ["stale_state"]
dependency_depth = 2
rationale = "Order #1077 has two 48000 charges 2 min apart plus look-alike #1080 ..."
```

- `primary` is one of `entity_ambiguity`, `tool_ambiguity`, `pagination`,
  `stale_state`, `reconciliation`, `cross_system_identity`,
  `permission_refusal`, `retry_idempotency`, `untrusted_retrieval`,
  `dependency_depth`, `baseline`, and the diagnostic-only `no_action` and
  `clarification`. It names what the task's *data and contract* exercise. It is
  never derived from a service name, group or tag.
- `secondary` lists other axes the data also exercises. It is diagnostic and
  never a scoring split.
- `dependency_depth` is the longest chain of information/operation
  dependencies (0 = the request supplies every identifier). It is not a
  reference-call count: parallel lookups add nothing and a batched query keeps
  the dependencies it resolves.
- `rationale` names the concrete data or contract fact behind the primary
  family, so an assignment can be reviewed against the dataset.

`Scenario::load` rejects a missing table, an unknown family or field, a depth
above six, a `dependency_depth` primary with fewer than three steps, baseline
with secondary axes, and a scored `no_action`/`clarification` primary. The
coverage inventory exports each row's `family` and summarizes
`scored_families`, `scored_secondary_families` and `scored_dependency_depths`.
The family table is part of `scenario.toml`, so the existing scenario source
hash covers it; it is not written to `instruction.md` or any agent-visible
generated file.

The 150 scored tasks were assigned by reading each instruction, dataset and
contract (four model sub-agents over disjoint groups, then a consolidation review that
spot-checked cited facts against the datasets and normalized three
assignments). Current primary distribution: entity_ambiguity 96, stale_state
19, baseline 18, cross_system_identity 8, tool_ambiguity 4, dependency_depth 3,
reconciliation 2. Depths: 0×5, 1×56, 2×63, 3×22, 4×4. The suite has no scored
pagination, permission/refusal, retry/idempotency or untrusted-retrieval task;
that is a measured gap, not a hidden one. Family assignment is a reviewed
judgment and has not had independent human adjudication.

## Unscored diagnostics that run for real

`scenarios/diagnostic/stripe/no-action-already-refunded` and
`scenarios/diagnostic/stripe/clarify-ambiguous-refund` are genuine scenarios:
materialized from `scenarios/_organizations/synth-ops/base.json` plus a task
overlay by `Dataset::load`, served by the world, sealed and graded by
`WorldEvidence::assess`. The `diagnostic/` group is never scored: `taskgen`
omits it unless `--include-examples` is given, the inventory counts it under
`unscored_diagnostics`, and the release manifest cannot contain it.

### `[expect.interaction]`

```toml
[expect.interaction]
kind = "no_action"

[expect.interaction]
kind = "clarify_then_act"
required_facts = ["order"]
```

Both need an audited version-2 contract and are judged from authoritative
operation events.

- `no_action` fails when any business change is committed
  (`interaction/invalid_action`). It requires an answer contract; the
  diagnostic binds the reply to the existing refund's identifier and reviewed
  "already refunded" wording. That wording check is a pattern, not semantic
  understanding.
- `clarify_then_act` fails when a change is committed before the scripted user
  revealed every required fact (`interaction/acted_without_clarification`),
  including a correct guess and asking only afterwards, and reports
  `interaction/clarification_missing` when nothing was asked.

`grading::classify_interaction` separates a correct outcome, an invalid action
(committed change or attempted prohibited operation), acting without
clarification, incomplete execution, and unscorable evidence.

### Scripted user

```toml
[scripted_user]
schema_version = 1
max_turns = 4
default_reply = "I can only tell you about the double charge I called about."
forbidden_in_replies = ["ch_", "pi_"]

[[scripted_user.facts]]
id = "order"
content = "#D-201"

[[scripted_user.questions]]
id = "which_order"
patterns = ["which order", "order number"]
reply = "It was order #D-201."
reveals = ["order"]
```

The user is a deterministic state machine, not a model. It answers only a
message containing an authored pattern, with authored text; anything else gets
the fact-free default reply, and after `max_turns` (at most eight) it stops
answering. Loading fails if a reply contains a fact it does not declare, omits
one it declares, contains forbidden text, or if a fact is unreachable.

A turn is `POST /_bench/user/exchange` with `{"message": "..."}` on the
world's admin port. It is guarded like the other admin routes and is not
served on any agent-facing service port. Each turn is an ordinary recorded
operation plus a `user_exchanged` event, and the script is part of the initial
state snapshot. The grader replays the frozen script over the captured
requests; a conversation that does not replay is an environment fault. The
channel is excluded from agent API-call and MCP-bypass accounting.

Limits: no runner or harness forwards a model's question to this route yet, so
conversational trials are not qualified for native-model runs and remain a
separately labeled, unscored track. The oracle shell has no conversation
driver, so the clarification scenario has no `[[solution.call]]` list; its
reference run is `crates/world/tests/diagnostic_outcomes.rs`. The
`user_exchanged` is carried by evidence schema 7, paired with unpublished
evaluator 45, and appears only
for scripted-user scenarios; the schema number was not changed here.

## Frozen release admission

`releases/tool-bench-public-dev-1.json` freezes, before any result:

- every scored task's split, family, family-assignment hash, scenario-source
  hash, dataset hash and contract hash;
- the evaluator revision and family schema;
- the comparison design: counted attempts per task per arm, budgets, order
  policy, replacement policy, declared treatments, denominators, and no
  sequential stopping.

All 150 tasks are `public_development`. The repository contains no held-out
task set. The single `external_bundle_seals` entry is flagged
`synthetic_fixture`; it exercises the sealed-input path and supports no
held-out claim. A manifest with held-out tasks but only a synthetic seal does
not validate, and the report refuses a held-out comparison against it.

`bench-report --require-attested <a> <b> --release <manifest>` admits both arms
before pairing. `--release` belongs to the attested comparison only; the default
report on production runs ([production.md](production.md#comparing-arms)) does not
take it.
It rejects a trial whose task is outside the release, whose contract, scenario
source (which carries the family assignment), dataset or evaluator differs
from the frozen value, or which lacks verified provenance; counted attempts
that differ from the prespecified repeats (selective repeats, omissions,
missing tasks); unscorable attempts under a no-replacement design or beyond
the declared per-task limit; invalid-contract and unresolved trials; an agent
difference the release does not declare; a mixed-split comparison; and an
agent time budget above the declared maximum. A declared inference-token
budget is rejected as unverifiable rather than assumed.

Replacement admission checks the failed attempt's verified source, evaluator,
execution conditions and budget too. Only reproduced environment-error
assessments are replaceable. A resultless attempt or self-labelled failure
without admitted provenance is unqualified, not evidence that the environment
caused a failure.

An admitted report states its denominators (tasks, families, counted and
replaced attempts per arm, attempt-level pass rates, retry semantics), a
per-family table, every replaced attempt, and three separately labeled 95%
intervals for the difference: repeated attempts on the fixed suite, task
resampling, and whole-family clusters. For the public split it says these do
not describe unseen held-out families. The declared order policy is reported
as declared and unverified, because trial evidence does not record run order.

Without `--release` the output is labeled
`unfrozen_legacy_not_a_release_result`.

`cargo run -p scenario --bin release-manifest -- --manifest <file> --check`
fails, naming each task, when the manifest is not what the current sources
freeze to. `--freeze` rewrites only the derived task list and revisions and
keeps the authored header. After an intended scenario, contract or evaluator
change the manifest must be refrozen; once results exist against a release, a
change needs a new release identity rather than an edit. Historical results
keep their original evaluator and release labels.

## Validation

- `cargo test -p world --test diagnostic_outcomes`: both diagnostics over real
  HTTP, valid and invalid outcomes, replay and tamper evidence, channel access,
  shared-base invalidation and trial isolation.
- `cargo test -p report --test release_admission`: admission, rejections,
  denominators and the CLI against a manifest file.
- `cargo test -p scenario --test release_manifest --test coverage`: manifest
  freshness, named differences, family load validation and inventory export.
- `python3 -m unittest deploy/tests/test_materialization_loader.py`: the real
  loader on committed and edited sources against an independent reference.
