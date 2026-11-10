# Revision 38: Datadog scripted native-client checkpoint

Status: **bounded scripted local native-client checkpoint admitted**. This
condition has 126 v2 contracts and 24 legacy writes. Revision 38 is the last
admitted scripted native-client checkpoint. The full 19-item overhaul remains
active; this is not native-model or hosted admission.

The governing [operation research](../research/2026-11-07-datadog-operation-contract.md)
separates vendor documentation, pinned native code and local fixture conventions.
Instructions, datasets and business dates remain unchanged.

## Contract repairs

Sixteen initial workflow tests reproduce thirteen legacy defects: twelve false
passes and one generated-ID false failure. Diagnostics are
`/tmp/tool-bench-e38-datadog-red-complete.log`. The repaired family now has
25 targeted workflow controls.

- Cancellation retains the selected existing downtime. Only its request-causal
  cancellation, disabled/active and updater fields may change. Its creator,
  message, window, selectors and every neighboring record remain protected.
- Host mute requires the actual emitting production host, exact deadline,
  complete requested incident reference and related host/mute records. A
  successful log response must contain a complete qualifying source record
  before any target mute request is admitted. Empty, unrelated and late reads
  do not establish that prerequisite.
- Scheduling selects the actual new record instead of predicting `max+1`.
  Exactly one uncanceled one-time downtime must have the requested numeric
  window, production monitor, host scope and change reference. Baseline
  downtimes, monitors, hosts and unrelated mute state remain protected.
- Scope predicates are positive conjunctions, not host unions. Redundant tags,
  commas, ordering and value globs may describe the same outcome. Target tags
  come from the initial host; singleton identity is proved over the matcher
  language, not just the seeded neighbors. Unqualified syntax requires review.

### Do not overconstrain identifier references

Review found an additional false failure in the first revision-38 candidate:
valid ticket links with descriptive slugs were rejected by an unnecessarily
strict hyphen boundary. The actual one-test failure is retained in
`/tmp/tool-bench-e38-identifier-link-red-observed.log`. The earlier partial-name
selection ran zero tests and is a diagnostic, not regression evidence.

The repair accepts ASCII-case variants and identifiers inside ordinary ticket
links while rejecting longer alphanumeric identifiers. Both the Rust contract
and independent checker have controls for this distinction. This checks the
requested reference, **not arbitrary prose truth or external-link validity**.
Candidate A is retained but unadmitted; it is not silently relabeled as B.

## Shared implementation

`InformationCheck` binds a successful HTTP response's complete source record,
optional exact time window and response-completion event to subsequent action
admission. Startup reads are excluded; query spelling, page size and the complete
reference read sequence are not requirements. Missing or inconsistent operation
evidence is an environment error. A complete trace proving no qualifying read is
an ordinary business/order failure, not invalid evidence.

This is **HTTP-side prerequisite evidence**. It does not establish delivery to a
native model, inference-phase attribution or full MCP-dispatch correlation.
Those broader causal gates remain open.

`TagScopeGoal` binds initial tag context and reuses the bounded exact matcher
proof for identity constraints. Both final state and committed creation history
are checked. Raw scope spelling predicates cannot compete with the semantic
goal. Missing context and unqualified expressions do not become guessed model
failures.

The revised full source gate passes **1,855 Rust tests, zero ignored**, strict
workspace clippy, formatting and canonical task/inventory freshness. The original
eighteen information/scope interface controls pass. Four
`programcheck` controls validate retained static request files against the same
locked schema interface as canonical task generation. Its report explicitly
does not certify shell dispatch, runtime captures, operation support or business
semantics.

## Native and simulator contracts

- `datadog_api_v1` covers the three write tools and three declared reference
  calls. It validates pinned text prefixes, SDK camelCase, actual request fields,
  returned IDs and completion messages. Ordinary business words such as “error”
  are not failure heuristics.
- Host expiry formatting is checked against actual JavaScript `Date`, including
  its clipping bounds and extended years. The wrapper's dropped recurrence
  `until` is not certified as a completed constraint.
- Numeric downtime timestamps are retained rather than truncated to seconds.
  This follows the native JavaScript-number representation; it is not a claim
  of arbitrary decimal precision in the hosted API.
- Display timezones are validated through pinned `chrono-tz 0.10.4`, including
  IANA aliases, rather than a reference-only whitelist. Existing lockfile
  package versions, sources and checksums remain unchanged; four packages are
  added for this validation.
- Combined explicit monitor-ID/tag precedence is unqualified. It produces an
  authenticated capability failure before mutation, not a guessed success or
  ordinary model failure. Empty scope conjuncts do not silently disappear.
- Native configuration uses ordinary-shaped synthetic API/application keys.
  This removes the explicit credential marker for this endpoint; it does not
  certify every model-visible input or model unawareness.

The local actor remains the explicitly authored application-key convention
1445416, not a hosted identity/RBAC proof. Host mute's vendor-created V2 downtime
is not modeled here; no V2 child ID, notification delivery or hosted side effect
is fabricated. This local condition must not prohibit genuine verified vendor
side effects in a separately qualified condition.

## Independent validation

The separately authored Datadog business checker derives identities and
requirements from instructions and source data, not evaluator predicates. It
replays complete committed records and correlates raw/parsed requests, responses,
parent/actor relationships and log evidence.

Parent review corrected a classification error: a fully captured workflow that
never reads the required logs is a business negative, not missing evidence.
Malformed, partial, spoofed or incomplete captures remain audit rejections.
Scope whitespace is not guessed into a conjunction, and explicit source-origin
inconsistency remains invalid evidence.

The independent checker has 90 tests; four consumer tests execute the actual
native-matrix admission predicate. All 94 pass. Positive controls still require
safe history; deliberate negatives may have unsafe history only when source,
environment, runner, integrity and complete history remain valid.

## Frozen build inputs

Candidate B is built from a private per-file-verified copy:
`/tmp/tool-bench-e38-build-input.09xr4160`.

- World: `tool-bench-world:datadog-contracts-v38-candidate-b`,
  `sha256:123bdbbd7019b94252e19bc1574b55d10be7c0119acfae752fea52114313db96`.
- Tools: `tool-bench-tools:datadog-contracts-v38-candidate-b`,
  `sha256:872fdca0798b178eed1fa3a45f0d6eda6fd0456ef4d28c559472cdf59378e855`.

The compiler reference is digest-pinned. Runtime bases preserve the frozen
revision-37 image layers; new local executables, catalog and scenarios are
overlaid without reinstalling native clients or mutable OS packages. Exact
recipes, image inspections and input hashes are retained. Network downloads and
compiler/cache execution are not independently attested: this is not a hermetic
build claim.

The pre-repair A images and their inputs remain separate:
`/tmp/tool-bench-e38-build-input.jceldy65`. No runtime result from that condition
is pooled with B.

Exact source retention is `/tmp/tool-bench-e38-source-freeze.320fqe1t`:
7,655 source entries independently reread, with archive hash
`sha256:49858ac25ecb06b9de13b80a3e89eac48a37334704b0430da336f080a0b42f2c`.
Every selected frozen B input matches its source archive entry. This is source
retention, not a qualification capsule.

All three B reference/no-op pairs ran at
`/tmp/tool-bench-oracle.5zZymS`; independent business/history review agrees on all
six outcomes. The isolation campaign also finishes with hidden-file/credential,
network, sealed-world, oracle and forgery controls passing. The forgery receives
a missing-answer failure, not an invented integrity failure.

## Verified qualification

The complete native business campaign contains **42 unique programs**:
18 passes and 24 deliberate failures. Every outcome agrees with the independent
source-bound business/history auditor, with zero audit rejections. The three
matrix roots are `/tmp/tool-bench-datadog-controls.RH8lmr`,
`/tmp/tool-bench-datadog-controls.uhRQ49` and
`/tmp/tool-bench-datadog-controls.eYk0FA`.

The full 21-definition surface capture verifies initialization, HTTPS, the
explicit local actor convention and a handled RPC400. Eight unhandled-error
programs stop before later calls and have no authoritative exports or numeric
assessments. Across all framework campaigns, four invalid cases remain excluded
and twelve failed programs remain unscored; a tampered manifest is refused.
The failed Date-format program does not independently establish persisted
after-commit state because authoritative collection never occurred.

All **54 unique primary assessments reproduce** from their retained evidence:
26 passes and 28 failures. This is diagnostic self-comparison, not independent
experimental repetition or model performance. The revised source gates include
1,855 Rust tests with zero failures/ignored, strict clippy, formatting, canonical
freshness, 61 pinned-runner Python tests and 200 independent/consumer Python
tests.

The qualification capsule is `/tmp/tool-bench-e38-qualification-freeze.oz95b0ym`.
Its **6,470 file checksums independently verify**, including complete attempt
retention: 70 distinct trials, 71 terminal result files including the tampered
copy, and 2,518 byte-verified attempt payloads. The attempt archive hash is
`sha256:17b96c0574024bf240990d1aa1c85a9529fd2cdf2a7775e42805114b031501ce`.
Both frozen build-input trees are retained and labeled separately.

Scoped ownership checking finds zero survivors among 207 captured trial/probe
IDs, two binary-copy IDs and 141 project namespaces. No shared Docker pruning
is used. Documentation closeout is written after the frozen qualification and
retained separately; the frozen source and observations are not rewritten.

No native-model, hosted, human-review, awareness, broader authoring/difficulty/
fault or full-release gate is closed by this bounded checkpoint.
