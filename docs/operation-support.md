# Reviewed operation support

The sections below are dated review records. Their counts and revision numbers are
those of their date; the current inventory is `docs/coverage/scenario-inventory.json`.
Where a section says wording "requires adjudication", the trial is recorded as
unresolved with reward 0 and nothing waits on a reviewer. Current grading rules are in
[contracts.md](contracts.md) and [answer-fact-contracts.md](answer-fact-contracts.md).

This is a **partial inventory**, not certification of every operation accepted by
the mock schemas. Task success requires the real business outcome, not merely an
HTTP success, a matching mutation name, or an oracle that appears to pass.
Expand this inventory before suite admission; do not infer unsupported fields work
because their surrounding mutation accepts them.

## Machine-readable index

`catalog/operation-support/<service>.toml` indexes this document for the authoring
gate. `taskgen` loads it, verifies each entry against the catalog, the locked tool
snapshot and the section headings below, and reports every operation that a scored
reference solution exercises without a declaration as the
`unreviewed_reference_operation` inventory gap
([admission rules](authoring-admission.md#reviewed-operation-admission)).

| Status | Meaning |
|---|---|
| `reviewed` | Executable semantics reviewed for the operation as a whole. No entry currently claims this. |
| `partial` | Reviewed for the workflows recorded in the named section, with stated limitations. |
| absent | Not reviewed. A dispatching handler or a passing oracle is not a review. |

The index was seeded conservatively and every current entry is `partial`:

- Tools carrying a reviewed native result contract in
  `crates/scenario/src/oracle_results.rs` (Terraform, Grafana, PagerDuty, Datadog,
  Google Workspace, Firecrawl), under their checkpoint section.
- Operations exercised by the reference solutions of a service's single-service
  write scenarios, where a section below records the review of those write
  contracts (the six services above plus Argo CD, AWS, Cloudflare, GitHub, GitLab,
  Jira, Kubernetes, Linear, MongoDB, Notion, Okta, Sentry, Shopify, Slack,
  Supabase, Vercel, Zendesk). Read-only steps of those write workflows are included
  only as exercised there.

Stripe, HubSpot, Salesforce, Elasticsearch and the search providers have no
dedicated section here, so their operations are undeclared, as are read-only
operations used only by lookup or cross-service scenarios. Add an entry only with
the review that supports it; do not edit a handler or a declaration to clear the gap.
Supported-but-empty data and unsupported environment capability are not yet
separate statuses in the index.

## Cross-system remediation source — revision 43

Current inventory is **134 v2 / 16 legacy**. The
[source checkpoint](history/checkpoints/evaluator-43-ops-remediation.md) migrates checkout
scaling/resolution and OOM-limit/Sentry resolution. Readiness is checked against
actual owned modeled pods; memory uses exact quantity equivalence before plain
resolution. Committed history protects unrelated records and restored collateral
changes. Unreviewed note meaning requires adjudication.

The independent remediation auditor is explicitly direct-fixture-only, not a
runtime admission checker. The Datadog/Argo rollback remains legacy: missing
authored rendered content is an environment fault, not successful deployment or
an ordinary model failure. No new native-model or hosted claim follows.
Firecrawl42B remains the latest admitted bounded scripted native-client condition.

## Firecrawl scripted native-client checkpoint — condition 42B

That condition retains 132 v2 / 18 legacy. The
[revision-42 record](history/checkpoints/evaluator-42-firecrawl.md) reproduces two valid
staged-clock plans rejected by revision 41. The correction grants only the new
monitor's requested schedule and derived timing updates; preexisting monitors
and active non-daily history remain protected.

The new independent auditor source-binds normalized initial data, complete
history, real created identities, authorized scope and transport. Three old
undeclared initial overlays reject rather than becoming apparent model failures.
Seventeen fresh on-disk-source fixtures independently agree, including their
properly declared variants. These source fixtures alone do not establish native
execution, hosted notification delivery or model receipt.

The separately frozen replacement B runtime now qualifies: all 31 distinct
native controls independently agree, 65 actual rendered inputs pass locked
schemas and all 42 primary assessments reproduce. The ordered 28-definition
surface, HTTPS authority, native errors, three reference/no-op pairs and
source/lifecycle/isolation guards pass. One unresolved filter blocks its raw
comparison; four invalid cases and eight failed programs are not ordinary model
failures. The source gate passes 2,031 Rust tests, 327 host checks and 61 runner
checks. Complete retained attempts and scoped cleanup verify. A remains
unadmitted after its real default-port URI fixture-lookup bug; only B is qualified.
This is still not hosted delivery, native-model, human/awareness or full-release
admission. The [42B record](history/checkpoints/evaluator-42-firecrawl.md) states exact limits.

## Prior Firecrawl source checkpoint — revision 41

That source checkpoint has 132 v2 / 18 legacy. The
[Firecrawl source record](history/checkpoints/evaluator-41-firecrawl.md) separates exact
API-section scope, daily monitor configuration and pause-before-run child
admission from completion, notification delivery and hosted operation guarantees.
Created identities, committed-history preservation and valid alternative plans
replace fixed-ID/projection checks. Broad unjudged tracking remains valid.
Unknown active filtering needs adjudication.

The bounded scheduler has a retained pinned-reference comparison; unsupported
syntax is not silently approximated. The native result guard distinguishes a
failed foreground crawl from a legitimate failed-job readback. Ten tools have
explicit result profiles, without certifying the remaining advertised tools.
Network-isolated installed-client codec controls are not task qualification.
The new HTTPS catalog origin needs a rebuilt and recaptured world/tools condition.
The source gate passes 2,027 Rust tests, strict clippy, formatting/freshness,
278 host Python checks and 61 runner checks. All 1,042 source-evidence payloads
verify. Thirty-nine direct source assessments use the exact final workspace
executable; independent replay proves trace consistency, not business
authorization or correctness. At that source checkpoint, independent runtime
business qualification remained open and corrected Google40B was the latest
admitted scripted task condition. The separate 42B condition above supersedes it.

## Google scripted selected-replica MCP checkpoint — condition 40B

That frozen condition retains business evaluator 40 and 129 v2 / 21 legacy.
The [40B condition record](history/checkpoints/google-40b-results.md) adds an explicitly
selected-replica text-result profile for five reviewed tools, without pretending
that human confirmations are Google SDK JSON. Four reference calls declare it.
The separate jq module rejects conflicting envelopes, unbound acknowledgments,
partial batches and permission-read error strings before later program effects.
Exact persisted state and complete history remain independently audited.

All 54 generated-helper fixtures pass. Seventeen direct Rust controls agree
independently and 26 actual direct HTTP results replay unchanged. The serial-date
control now uses the advertised JSON-string route, and all direct Rust control calls receive
captured-schema validation. These are not native-client or model receipt proofs;
the full current source gate passes 1,960 Rust tests, 257 independent/consumer
Python tests, 61 runner tests, strict clippy, formatting and freshness.
The distinct source/build condition was retained before actual MCP campaigning.
Its corrected bounded runtime condition is now admitted: all 43 business controls
independently agree, all 55 primary assessments reproduce, and the locked
119-definition surface matches. Actual-byte schema checks cover 116 operations
in 47 trials; 278 current host tests pass. Four invalid assessments stay excluded
and fifteen failed programs stay unscored, including a wrongly authored
duplicate-grant control correctly rejected by the existing capability guard.
The separate bootstrap-auditor correction and that original control mistake stay
retained. Scoped ownership finds zero survivors among 282 owned IDs / 149 project
namespaces. This is selected Rust replica MCP qualification, not upstream
Python/SDK or model execution, hosted conformance or universal argument/runtime
support. Original 40 and 38 capsules remain unchanged and distinct.

## Original Google source candidate — revision 40

All three single-service Google writes now have audited v2 source contracts:
**129 v2 / 21 legacy** at that source checkpoint. At this original source checkpoint,
revision 38 remained the last admitted scripted native-client condition. The [original source record](history/checkpoints/evaluator-40-google.md)
extends the [revision-39 Drive slice](history/checkpoints/evaluator-39-google.md).

Sheets supports exact serial/ISO/reviewed locale dates, bounded constant
`DATE`/`DATEVALUE` expressions, staged target-cell updates and identity-selected
rows. Null inputs skip cells. Unknown expressions decline before mutation;
RAW formula strings are not computed results. Neighbors, archived data and
older trackers remain protected, including metadata changes on unnecessary
writes of identical old values.

Calendar supports IANA-zone conversion, exact supported instants and actual
new-event identities, complete guest sets, staged create/update and modeled
usable Meet resources. Ambiguous civil times and unqualified precision decline
before mutation. A stored display zone does not invent native input conversion.
Invalid interval updates cannot leave orphan conference records.
Raw `conference_data.createRequest` provisioning, asynchronous polling and request
identity/retry semantics remain unqualified and decline before mutation. The
reviewed `add_google_meet` model is a distinct bounded path, not proof that every
raw conferencing option works.

Notification modes are not an all-only completion gate. `externalOnly` does not
mean guests outside the email domain, and notification requests do not establish
delivery, receipt, acceptance or guest-side visibility. A silent update may add
Meet after creation without resending the final URI. See the
[independent primary-source review](history/research/2026-11-07-google-invitation-notification-contract.md).
The rejected stricter candidate is retained separately.

Drive supports scoped permission append/direct-role correction, existing safe
access, ASCII email case variants and identity-preserving collaborators.
Transient writer/public access, collateral resources and restored changes fail.

Expiration, custom notification delivery and repeated-principal grants are
explicit pre-mutation capability limitations, not silently ignored successful
operations. Inherited/effective access, hosted authorization, notification and
duplicate-create behavior remain unqualified. This is the Rust MCP replica,
not execution of the published upstream Python server or Google SDK.
The separate Python checker passes 50 unit tests, and all seventeen real
in-process Rust controls independently agree: eight passes and nine deliberate
failures, zero audit rejections. Its requirements come from reviewed source
instructions/email, not expectation predicates or cached grades. CI runs this
source-fixture gate separately. Shared generic trace utilities do not decide
Google business semantics. This is not a native-client or runtime qualification.

Those source controls alone claimed no Google runtime qualification; the
separate corrected 40B campaign above now qualifies its declared bounded condition.
The original revision-40 source gate passes 1,942 Rust tests, strict clippy,
formatting and freshness, plus 250 independent/consumer Python tests and 61
runner tests. Its checksummed source/evidence retention is recorded in the
checkpoint, not borrowed from revision 39's 1,883-test gate. Historical
diagnostics and mixed-artifact replays remain separate and excluded from admission.

## Datadog scripted native-client checkpoint — revision 38

The three Datadog writes now declare audited v2 preservation/history contracts:
**126 v2 / 24 legacy** in this condition. Cancellation preserves complete retained
records; host mute requires
source-bound prior HTTP log evidence and exact host/deadline relationships;
scheduling uses actual created identity, one-time coverage and complete
conjunctive scope semantics instead of predicted IDs or substring matches.

The [checkpoint record](history/checkpoints/evaluator-38-datadog.md) documents the thirteen
reproduced legacy defects, the separately repaired ticket-link false failure,
typed native result profiles, source/capture limitations and distinct immutable
build inputs. Candidate A remains unadmitted; qualification admits only B.
Twenty-five workflow controls, eighteen core interface controls, thirty-five
result-contract tests and the ninety-test independent checker provide bounded
source evidence, not hosted conformance or a native-model score.

All 42 native business controls independently agree: 18 passes and 24 deliberate
failures, with zero audit rejections. Three reference/no-op pairs and the full
21-definition surface capture pass. All 54 primary assessments reproduce;
four invalid cases stay excluded and twelve failed programs stay unscored.
The revised source gate passes 1,855 Rust tests, strict clippy, formatting,
canonical freshness, 61 runner tests and 200 independent/consumer tests.
The qualification capsule independently verifies 6,470 file checksums and
retains 70 distinct trials and 2,518 byte-verified attempt payloads. Scoped
ownership finds zero owned survivors. These are scripted local checks, not
native-model performance, hosted semantics or full-release admission.

The host-mute-created vendor V2 downtime remains unmodeled. The local actor is a
fixture convention; genuine external side effects must not be guessed into or
forbidden by this local contract. HTTP prerequisite checks do not prove receipt
by a model. Combined explicit monitor selectors fail as an authenticated
capability limitation before mutation. Unknown semantics remain explicit.
**Revision 38 was the last admitted scripted native-client checkpoint at that
stage; the separate corrected Google40B condition is now the latest.**

## PagerDuty scripted native-client checkpoint — revision 37

Revision 36 remains frozen and **unadmitted**. Qualification exposed a real
interface mismatch: the installed native `Override` model truncates input
fractions to six digits, while the simulator rejected all nonzero fractions.
The [pinned normalization research](history/research/2026-11-07-pagerduty-time-normalization.md)
distinguishes observed model behavior, published source and unverified hosted
semantics. The repair uses a distinct source/image condition.

- Classic creation, storage, layer rotation, overlap selection, rendered coverage
  and on-call queries preserve exact supported nanosecond instants. Valid
  microsecond splits are no longer classified as capability faults. Genuine
  fractional gaps/extensions are business failures instead of rounded successes.
- Incident/history time filters and incident sorting retain fractional precision.
  Integer on-call duration metrics quantize after interval union, not by rounding
  each endpoint. These changes do not certify arbitrary v3 recurrence, every
  analytics time field or live vendor precision.
- Reference creation results bind to the pinned model's converted request
  instants. Only the expected input is truncated to microseconds; returned values
  remain exact. A raw nanosecond boundary that the client converts to the requested
  whole second may succeed, but a surviving microsecond wrong-window must fail.
  Intervals collapsing to zero during native conversion cannot be certified.
- Naive timestamps retain the explicitly authored local UTC compatibility policy;
  no hosted interpretation is claimed. Raw REST precision outside the exact
  parser's domain remains distinct from the native model's input conversion.
- A separate business/history checker resolves source identities and current
  primary duty, replays complete committed records, correlates HTTP phases and
  checks actor/parent/response relationships. It does not invoke Rust grading or
  use expected predicates/cached verdicts to decide. Its 24 unit tests and the
  combined 106-test independent/consumer suite pass after the host-driver
  regression additions.

The bounded scripted native-client checkpoint is now independently audited and
retained. All **39 unique controls agree: nineteen passes and twenty deliberate
failures**, with zero audit rejections. The original fifteen controls and
twenty-four-control continuation remain separate attempts, with both host-driver
revisions retained. Their guard-only correction does not change the evaluator or
weaken source/evidence admission.

All three reference/no-op pairs pass independent business/history review. Both
distinct-account surface workflows independently pass; each capture matches all
101 complete locked definitions. Four invalid source/admission cases remain
excluded, and twelve failed native/completion/shutdown programs remain unscored.
Every one of **52 primary assessments reproduces**, with source/byte/receipt
checks and verified report loading. These diagnostic self-comparisons are not
independent experimental repeats or model results.

The source passes 1,800 Rust tests, strict clippy, formatting and freshness;
61 pinned-runner Python checks and 106 independent/consumer checks pass.
Qualification retention byte-verifies 4,171 files and 2,551 archived attempt
payloads. Ownership covers 201 trial/probe IDs, three supplemental probe/copy IDs
and 137 explicit project namespaces, with zero owned survivors. The
[progress ledger](history/implementation-progress.md#verified-pagerduty-native-client-checkpoint)
records the exact source and qualification capsules. Revision 36 remains frozen
and unadmitted; its six historical preflights are not pooled into revision 37.
Source inventory remains **123 v2 / 27 legacy**. No hosted, native-model, human,
awareness or full-release admission is implied.

## PagerDuty source candidate — revision 36

The three single-service PagerDuty writes now declare audited v2 contracts:
**123 v2 / 27 legacy** source contracts. The
[operation research](history/research/2026-11-07-pagerduty-operation-contract.md) separates
published documentation, exact pinned packages, fixture conventions and
uncertainties. This section is **source support**, not an admitted runtime
checkpoint. At that historical preflight, the last retained scripted native-client
condition was Grafana35. The separately repaired and qualified revision 37 above
does not retroactively admit this source.

- Coverage means exactly Jordan Lee on classic Platform Primary for the requested
  October 31, 2026, 09:00–17:00 UTC window. New override identities are selected
  relative to the baseline and bound to their actual parent. Adjacent, reordered,
  overlapping or repeated same-user segments are accepted when their union is
  exact. No gap, extra coverage, other replacement/schedule, rotation edit or
  baseline-override rewrite is permitted. Since every new winning candidate has
  the same requested user, exact union is sufficient within this declared fixture;
  this is not arbitrary conflicting-override precedence certification.
- Incident handoff requires the complete singleton Dana assignment and the whole
  quoted DBRE handoff note. Original service, team and escalation-policy records
  remain unchanged; direct user assignment is not policy migration. No
  acknowledgement/resolution or collateral incident edit is authorized, including
  mutations later restored. Array-length permissions constrain every committed
  assignment change, not only the final array.
- Checkout must finish resolved, with both related production alerts resolved and
  a complete rollback-resolution note. Target-only acknowledgement before
  resolution is admitted; note/status order is free. Only scoped lifecycle,
  acknowledgement, child-alert, note and timeline effects are allowed. Staging,
  other incidents, prior notes/logs, status-page content and webhook configuration
  stay protected. Final resolution does not imply irreversible hosted lifecycle.
- Notes follow actual parent/allocation and account actor relationships; resolve's
  actor is Lena, while handoff's is Alex. Repeated complete copies of the requested
  note are explicitly admitted, without claiming lost-response lineage or
  exactly-once delivery. Every created note is checked as whole content, including
  committed history. Reviewed paraphrases are finite; unknown prose requires
  adjudication, not token-based success or invented failure.
- Cause-scoped, matched record creation can use an explicit `all_matching` limit;
  it never authorizes updating/deleting a baseline record. New interval goals
  compare exact nanosecond RFC3339 instants and interval unions; partial coverage
  need not be complete before the last legitimate split write. Each committed
  segment must independently stay within the authorized window.
- The existing schedule renderer is second-resolution. Nonzero subseconds,
  excessive precision and lossy-but-parseable time forms now produce authenticated
  capability faults instead of being silently rounded into success. This also
  applies to time-query parameters. Classic override precision is preflighted
  before any member commits; ordinary admitted-precision invalid batches retain
  per-item partial-commit behavior. A fractional valid alternative therefore needs
  environment admission, not an ordinary model-failure label. This does not claim
  vendor subsecond rounding, full v3 precision or classic cancellation fidelity.
- Native configuration now names ordinary TLS `api.pagerduty.com`, including
  returned resource references, with no emitted benchmark-labelled user key.
  TLS/account-origin tests do not establish real RBAC, human identity, a resolved
  native model prompt or model unawareness.
- `pagerduty_api_v1` covers three write tools and nine declared reference calls.
  It checks typed note/incident responses, actual request-bound incident effects,
  every classic-override item status/error/ID/user/instant and text/structured
  agreement. Equivalent response ordering is not a hidden requirement. A correct
  MCP/HTTP envelope around failed override items or an actionless empty management
  response cannot complete a reference program. Pinned `assignement` is the
  supported input spelling. Urgency and other normalized-away fields cannot be
  certified from the native response; authoritative outcome/history checks remain
  separate. Other PagerDuty tools retain the generic envelope profile.

All 1,788 Rust tests, strict clippy, formatting and source freshness pass.
Twenty-six HTTP/grade cases, five result cases and twenty-four generated-helper
controls cover the declared source repairs. The distinct native candidates pass
three references and three no-op controls; all six raw assessments reproduce.
Eighteen observed container IDs and six project/trial namespaces have zero owned
survivors. Source/preflight evidence is retained in the
[progress ledger](history/implementation-progress.md#verified-source-gate-and-initial-pagerduty-native-preflight).
This remains a preflight, not the complete native family campaign.

Native valid-alternative/near-miss, partial-error, source/completion/admission/
isolation and complete-surface campaigns, independent business/history auditing,
reassessment, ownership and immutable runtime retention were still required at
this preflight. The precision objection was repaired and qualified under revision
37 instead of admitting revision 36. No hosted notification/delivery, concurrent responder,
native-model, human, awareness or full-release qualification is claimed.

## Grafana scripted native-client checkpoint — revision 35

The three single-service Grafana writes now use v2 contracts. Current source
inventory at revision 35 is **120 v2 / 30 legacy**, with **454 reference calls**, 438 static
argument checks and sixteen deferred captured-argument checks. The bounded
[scripted native-client checkpoint](history/implementation-progress.md#verified-grafana-native-client-checkpoint)
is retained separately from its initial source/preflight capsule. This is not
hosted conformance, native-model admission or full-overhaul completion.
Governing primary-source notes are the
[operation contract](history/research/2026-11-07-grafana-operation-contract.md) and the
[independent regex review](history/research/2026-11-07-grafana-label-regex-compatibility.md).

- Acknowledgment targets the actual production group/team relationship and
  preserves unrelated groups, datasource configuration and the unresolved state.
  Resolve/silence attempts remain forbidden, including trailing-slash routes.
  Current-primary answers resolve the actual schedule's complete duty set against
  its directory, not one seeded surname or the first returned user.
- Truthful structured identities and bounded reviewed whole-answer presentations
  are supported. Unknown wording/metadata requires adjudication; unknown metadata
  does not conceal known schedule/person/cardinality failures. An empty native
  user list does not establish empty duty because the pinned helper skips failed
  individual user lookups. Review packets include the authoritative sources and
  explicitly require reviewers to establish factual correctness; they do not say
  the raw on-call answer's binding already passed.
- Annotation success means one new event, correct dashboard identity, requested
  text/time/required tags and point scope on the whole dashboard or actual latency
  panel. Numeric-dashboard association, tag order/additions and actual allocated
  IDs are supported. Existing annotations/dashboard definitions remain protected.
  Typed input rejection is atomic; supplied JSON data survives readback. Graphite
  events do not acquire unsupported dashboard/panel associations. These are stored
  relationship checks, not rendered-pixel certification.
- Silence success means exactly one new silence for the complete requested label
  predicate and exact maintenance instants, with the complete ticket identifier.
  Matcher order, reviewed regex/negation/redundancy alternatives, equivalent UTC
  offsets and a different nonempty creator need not copy the reference spelling.
  Old silences, alert rules and routing remain protected. Changed committed
  predicates/windows are checked even when later restored.
- Predicate comparison uses a bounded automata-product proof over all valid
  UTF-8 label strings, not fixture samples. The shared server/compiler admits a
  deliberately limited Go-style grammar. Known literal-brace, leading-bracket,
  shorthand/range, case-fold, quoting, punctuation and group-wrapper hazards have
  permanent regressions. Case-folding flags, counted repetitions/literal brace
  fallback, ambiguous classes, unscoped flags, Unicode properties, octal and other
  unqualified forms are not silently interpreted as Rust regexes. Capability or
  proof limits remain environment faults/adjudication, not invented inequality.
  This is not full Go/RE2, arbitrary byte-string or hosted-version conformance.
- Silence-list filtering compares matcher name, source value and operator/type;
  it is distinct from evaluating a predicate against alert label values.
  Timestamp support preserves nanoseconds for the explicitly admitted strict
  RFC3339 subset; leap seconds and more than nine fractional digits are not
  claimed. Source-only Go witnesses were run with local Go 1.27.1, separately from
  the research note's advertised-server Go 1.24.4 source comparison.
- The connection now uses ordinary TLS `grafana.octo-corp.com`; OnCall discovery
  and pagination preserve that origin and filters. Explicit emitted benchmark
  credential/UID markers are removed. Unmapped service-account actions no longer
  name the first admin as actor; the actor remains unobserved/null. No Cloud/OSS
  wire identity or hosted authentication guarantee is invented. The pinned
  fallback transport remains a compatibility simulation requiring qualification.
- `grafana_api_v1` declares thirteen reviewed tool names, covering twelve actual
  reference calls. It checks generic HTTP status, capitalized annotation
  `Payload`, actual silence IDs and request-bound schedule/action results.
  `updated:true,stateWarning` remains a committed write with failed readback,
  not a failed write to retry blindly. A successful MCP envelope around HTTP 400
  is not operation success. Rule/query/datasource tools outside this reviewed
  set retain their explicit generic-envelope limitation.

All **1,748 workspace Rust tests** pass with zero ignored, together with strict
clippy, formatting and generated task/inventory freshness. This includes
thirty-five HTTP/grade cases, twelve label-language tests, eight answer tests,
isolated committed-semantic checks, review tests and nineteen result-contract
tests. Forty generated-helper fixtures and 61 runner Python checks also pass;
helper fixtures are not actual MCP/runtime campaigns.

The unchanged, distinct v35 candidate images now pass the complete declared
matrix: **27 native controls, thirteen passes and fourteen deliberate failures**,
all agreeing with a separately implemented business/history checker. Its
43 unit tests and the combined 76-test independent-auditor suite pass.
Actual initialization and all **79 locked tool definitions** match; separate
native user/settings/HTTP-error captures confirm the ordinary origin.
Dashboard UID precedence over a conflicting numeric ID has its own actual
native positive control and independent regression.

Five native failed-program controls and source/completion/admission/isolation
negatives pass. Four invalid source/admission cases stay excluded; nine failed
programs have no authoritative evidence or numeric assessment. All **39 primary
assessments reproduce: 21 passes and eighteen deliberate failures**. The retained
52-trial inventory contains 53 terminal files, including one tampered copy.
Scoped ownership checks find zero survivors among 153 captured trial/probe IDs,
106 project namespaces and two supplementary binary-copy IDs. Source retention,
explicit post-source changes, 2,105 byte-verified attempt payloads, runtime
binaries and final checksums are recorded in the progress ledger. These are
scripted controls, not model-performance comparisons or independent trial repeats.

**Current source fixes the silence-validation deadline:** the original client
would reject the fixed maintenance window after November 13, 2026, 04:00 UTC.
The [declared adapter](../servers/extensions/grafana/README.md) changes only
that business-payload check and receives each scenario's own business instant.
Pinned-client tests exercise before/at/after boundaries and retain real TLS,
authentication and process clocks. Other relative-query defaults are unchanged.
This is a new source/program condition, not a retrospective change to the
bounded campaign above. Full rebuilt-image/native runtime qualification,
production trust-path validation and the broader release gates remain open.

## Terraform workflow and representation checkpoint — revision 34

The three single-service Terraform writes now use v2 contracts, bringing the
source inventory at that checkpoint to **117 v2 / 33 legacy**. The bounded condition has a
[retained, audited scripted native-client checkpoint](history/implementation-progress.md#verified-terraform-native-client-checkpoint).
This is not hosted/provider certification, native-model admission or completion
of the full overhaul. The governing research is
[operation contracts](history/research/2026-11-07-terraform-operation-contract.md) and
[state representations](history/research/2026-11-07-terraform-state-contract.md).

- NAT approval requires the exact requested comment and terminal modeled apply,
  a new state snapshot bound to the actual run/apply/workspace, resolved EIP
  allocation/NAT/route relationships and unchanged staging/unrelated records.
  A successful action response or increased count alone is insufficient.
- Checkout requires the existing Terraform-category input changed to exact
  numeric four, immutable category/identity/value snapshots, a real input-bound
  normal reviewable plan and the complete requested run message. Production,
  other inputs, input flags and committed history remain protected.
- Orders recovery admits force-unlock followed by speculative verification or
  cancellation of the crashed target run followed by verification. Force-unlock
  does not claim to cancel that run. Recovery must precede plan creation.
  The Legacy workspace, state snapshots and unrelated runs remain protected.
- The fixture declares a closed managed-instance inventory. Startup materializes
  and normalizes it before baseline capture: modeled prod NAT count is **13→15**,
  not the old incoherent display count 58→60. This is not a universal HCP count
  formula or proof of a complete, provider-valid AWS state.
- `execution_outcomes` supplies explicit synthetic computed facts for the two
  declared NAT plans. Creation or required unknown resolution without supplied
  facts fails as an authenticated capability/environment fault, never fabricated
  success. Known planned values cannot be overridden by those facts.
- Public plan JSON has structured unknown masks and separate resource/module/
  numeric-instance metadata. Authored unknown sentinels are normalized before
  runtime; an explicitly known literal with the same text is not a wildcard.
  Independent display-plan overrides and unsupported address/provider bindings
  fail closed. Raw v4 and public JSON state downloads have distinct shapes.
- Saved plans bind their original state ID, serial and lineage. Disjoint serial
  or lineage changes reject before mutation; older modeled snapshots remain
  immutable. Deterministic initial lineage and finite synchronous execution are
  declared fixture conventions, not hosted concurrency or completion guarantees.
- `managed_resources`, `plan_changes`, `run_inputs`, base-state metadata and
  execution facts are private model/evaluator records, not invented HCP fields.
  Sensitive values are redacted in captured inputs and accompanied by private
  value hashes; these are not a new native secret-read capability.
- The catalog now uses the ordinary TLS account origin `app.terraform.io`.
  Pinned native MCP v1.4.1/go-tfe behavior, lossy workspace-detail relationships
  and ignored Boolean variable arguments remain explicitly bounded. No invisible
  owner-observation requirement or new upstream capability is invented.
- `terraform_api_v1` covers nine reviewed native tools: typed list/read JSON and
  exact request-bound action/unlock/variable-update text. An optimistic apply
  message is not completion proof. Reference solutions observe run details and
  bind new-run follow-ups to actually returned identities. Other tools retain
  their declared generic MCP-envelope contract, not inferred result support.

All 1,675 workspace Rust tests, strict clippy, formatting and task/inventory
freshness pass. The 24 distinct native-client controls produce nine passes and
fifteen deliberate failures; every verdict agrees with an independent resource,
input, ownership and committed-history checker. All 39 primary assessments
reproduce. Four invalid source/admission cases are excluded and nine failed
programs remain unscored. Actual initialization, all 61 locked tool definitions,
HTTPS authorities, state URLs, lossy SDK ownership visibility and indexed plan
unknowns are checked separately. Ownership audits find no owned survivors.

Data/deposed/replacement state lifecycles, string/sparse address reconstruction,
provider schemas/private state, intermediate persistence/import histories,
arbitrary configuration or per-run override planning, computed outputs,
real infrastructure outcomes, hosted races, native owner visibility, and
variable-flag toggling are not qualified. Source and runtime evidence, including
the post-freeze test-expectation-only delta, are recorded separately in the ledger.

## Okta source and native-result checkpoint

Revision 33 migrates the three single-service Okta write source contracts,
bringing the source inventory to 114 v2 contracts and 36 legacy writes. This is
a [retained, audited scripted native-client checkpoint](history/implementation-progress.md#verified-okta-native-client-checkpoint),
not hosted Okta qualification or native-model admission. The
[operation research](history/research/2026-11-07-okta-operation-contract.md) separates
official documentation, pinned package/SDK source, fixture facts and uncertainties.

- The exact contractor account remains present and terminally deprovisioned;
  its intended Salesforce membership alone is removed. Other accounts, profile
  values, group memberships and assignments remain protected.
- Maria's two requested profile fields and two membership edges are independent
  authorized changes. Full preservation-safe profile POSTs, supported login
  aliases, repeated requested updates and different safe orderings are valid.
  Group membership and actual administrator-role grants are not equivalent.
- Password-policy denial preserves policy/rule activation, applicability,
  priorities, provider/group targeting, other actions, requirements, complexity,
  timed unlock and all employee/default configuration. A higher-priority shadow
  allow policy and transient changes restored later do not pass.
- Native SDK cleanup omits empty optional rule exclusions. Only reviewed empty
  `people.users.exclude` containers are normalized before the authoritative
  baseline and on replacement; nonempty exclusions and other requirements remain
  protected. This is logical normalization, not a universal vendor JSON default.
- Default brand/theme records are materialized before baseline. Read-only
  discovery must not cause a false preservation failure by lazily creating them.
  Known legacy fixture default-user-type IDs are normalized consistently; actual
  custom type identities are not indiscriminately rewritten.
- Encoded login segments are decoded once, with malformed encodings, invalid
  UTF-8, control characters and path delimiters rejected before mutation.
  Invalid password-rule types/access enums/shapes no longer silently succeed.
- The finite local synchronous deactivation path returns HTTP 200 without a JSON
  body, rejects an already terminal user and records the supported lifecycle
  effect. No hosted timing, universal retry safety, response-loss reconciliation,
  session revocation or downstream application/data deprovisioning is certified.
- Unimplemented operation routes return a neutral connection-unavailable response
  and an authenticated private capability-fault event. They must invalidate a
  comparison rather than masquerade as an ordinary missing record/model failure.
  A missing user on a modeled route remains a normal 404 and can be recovered
  without poisoning the environment.
- OAuth/client/log/type/schema/default-brand identities no longer explicitly
  announce the benchmark. Issued client/scopes agree with declared catalog
  configuration; header-presence authentication remains a simulation, not
  demonstrated vendor OAuth/RBAC enforcement or coherent native-user provenance.
- The pinned `okta-mcp-server` 1.1.7 and SDK 3.4.6 remain unchanged.
  `okta_api_v1` checks actual JSON text-block/structured-output consistency,
  typed native error/exception/serialization envelopes and the exact
  request-bound deactivation completion message. This is internal reference
  transport validation, not keyword scoring of arbitrary user answers or proof
  of lifecycle completion. Authoritative final state remains necessary.
- The native `get_user_profile_attributes` helper samples the first returned
  user's SDK profile variables; it is not a complete org-schema or permission
  introspection operation. Its extra-read path is tested without interpreting
  those sampled names/values as proof of an actual schema or access grant.

The reference fetches the actual rule and changes only the two access decisions
through the pinned replacement tool. Supported nested conditions, requirements
and literal `${...}` values survive the binding/render path. All 152 scenarios
validate and 150 tasks generate; 449 reference calls comprise 436 static schemas
and thirteen deferred bindings. All 1,637 Rust tests and strict clippy pass.
The three native-client positive preflights pass; actual HTTP initialization,
all 113 definitions, HTTPS authority, account/type/brand and error surfaces pass.
The immutable 25-control main campaign plus a separately retained valid
profile-attribute-read alternative produce **26 distinct controls**: twelve
passes and fourteen deliberate failures. Every result agrees with an independent
identity/preservation/history checker, including finite group/provider/priority/
rule-applicability evaluation rather than trusting denial strings alone.
All **41 primary assessments** reproduce: twenty-three passes and eighteen
deliberate failures, including three explicit repeated positive preflights.
Four invalid source/admission cases are excluded; eight failed programs remain
unscored. Sixty-one runner checks, twenty-three auditor checks and seven actual
installed-package codec checks pass. Exact source/binaries and 2,198 attempt
payload files are retained; all recorded owned containers are gone. Three old
evaluator-32 legacy native diagnostics are explicitly separate. None of this
certifies a complete native session, hosted enforcement, downstream application/
data deprovisioning, human adjudication, model performance or model unawareness.

## Argo CD source, controller and connector checkpoint

Revision 32 migrates the three single-service Argo write contracts. Its frozen
source inventory has 111 v2 contracts and 39 legacy writes. Its bounded, scripted
native-client condition is [retained and audited](history/implementation-progress.md#verified-argo-native-client-checkpoint).
This is not native-model admission or general Argo CD/Kubernetes conformance.

The [primary-source operation review](history/research/2026-11-07-argocd-operation-contract.md)
separates native `argocd-mcp` 0.9.0 from the Rust backend and the deliberately
selected Argo CD 3.2.5 semantic comparison baseline.

- Git content is represented by immutable repository/path/concrete-revision
  manifests. Before baseline capture, existing fixture `targetManifest` and
  explicitly Synced resource projections materialize the comparison revision.
  The billing task additionally authors full 1.42.0 and 1.41.0 release manifests.
  No old workload image is guessed from a Git tag or a commit-health label.
- Runtime image facts are separate inputs. Existing Pod container observations
  seed known per-Application startup outcomes; new required release outcomes
  are explicitly authored. These are **simulated runtime facts**, not execution
  of real image payloads, registry validation or external credential testing.
- Sync acceptance records a pending operation. Selected-Application observations
  advance Running and then apply/prune/rollout, with failure or pending-finalizer
  state when appropriate. Other Application reads cannot advance the operation.
  Revision comparison, successful operation and live workload readiness are not
  interchangeable.
- Successful modeled Deployment rollout preserves Deployment identity, creates
  or reuses a correctly owned ReplicaSet, replaces Pods, retains inactive
  ReplicaSet history, and updates generation/readiness consistently. Known
  startup failures do not become healthy merely because Git metadata says so.
  Node placement uses observed workload nodes; this is not a general scheduler,
  garbage collector or capacity model.
- Canonical sync history remains immutable even when the public history
  retention limit is zero. Dry-run operations do not append successful deployment
  history or remove resources. Finalizer-blocked deletion remains pending.
- Goals join actual Application, destination cluster, resource identity, current
  template and child ownership. Ready-Pod counts derive from desired replica
  quantities rather than a fixed UID or insertion position. Payments requires
  actual legacy ConfigMap absence by cluster/GK/namespace/name, including orphan
  or same-name replacement cases—not merely disappearance of one old UID.
- Restart preserves the unreviewed application/configuration drift and Git
  deployment history. Only worker/descendant effects are permitted. Its actual
  action request uses decoded scalar parameter identity, not a name substring.
- Billing requires the persistent requested Git tag and effective automation on.
  Explicit `enabled:false` is off; null/omitted is on. Equal destination
  server/name representations normalize without reconfiguring the cluster.
  Same-target manual sync, preflight dry-run, retries and history retention
  variants are exercised without requiring a preferred initiator or call count.

The composed connector is **`0.9.0-operations.2`**, not unchanged upstream:
all sixteen original definitions are retained and an actual spec-only API tool
is added. Sync encodes operation options as `{items:[...]}` and propagates HTTP
failure. A native fetch adapter also raises non-2xx results and makes the legacy
subset update preserve metadata and unrepresentable spec fields using read-
modify-write. It does not promise atomic concurrency control or exactly-once
execution. Thirteen checks, including real stdio composition with the installed
package, pass with external networking disabled. The seventeen-tool snapshot
is captured from that running composition, never appended offline.

The candidate uses ordinary HTTPS origin `https://argocd.lumen-retail.dev/`,
environment-held credentials and declared runtime-origin binding. The tools
image is a development overlay on the explicitly retained Sentry-origin image;
it is not evidence of a fresh complete tools-image rebuild.

Unsupported rendering/startup/strategy/creation paths return a neutral public
connection error and an authenticated private capability-fault event. They must
not turn into ordinary comparable model failure. Advanced source transformations,
generic new managed-resource creation, arbitrary rollout strategies, complete
retry/backoff/fault scheduling, external service authentication, all namespace/
RBAC configurations and general Kubernetes behavior remain unqualified.

The focused evidence includes twenty HTTP/grade workflow cases, twenty-one
backend HTTP cases, shared Rust/jq argument-replacement parity and thirteen
composed client checks. Both pre- and post-lint-fold source sweeps pass 1,609 Rust
tests; post-fold source passes strict clippy and formatting. Sixty-one runner
checks and ten independent-auditor checks pass.

All twenty-one distinct native-client controls agree with the separately
implemented workload/ownership/history checker: nine valid alternatives and
twelve deliberate failures. Thirty-six primary assessments reproduce on the
declared image pair: twenty passes and sixteen deliberate failures, including
three explicitly repeated positive preflights. Four source/admission faults
remain outside comparisons; eight failed programs have no authoritative evidence
or numeric assessments. Actual initialization, seventeen definitions, HTTPS
authority, metadata, tree and error captures pass. Owned-container audits find
zero survivors.

The scored world remains the pre-fold image. Its exact binaries, pre-fold model
source and syntax-only post-build patch are retained. The diagnostic rebuilt
world differs bytewise and is not scored or pooled; only its verifier is
byte-identical. Source retention is not hermetic build attestation, production
qualification, model performance or model-unawareness evidence.

## Vercel environment scope and production recovery

Revision 31 migrates three Vercel write contracts. This is the local **replica
connector**, with the captured fifteen-tool community surface, not official
remote Vercel MCP or production Vercel certification. The
[operation review](history/research/2026-11-07-vercel-operation-contract.md) distinguishes
documented behavior, examined community code and unresolved vendor guarantees.

- Environment identity is project plus record ID. Authoritative
  `environment_variables` records are materialized before the baseline; nested
  project `env`, `env_by_target` and `environment_bindings` are derived views.
  Their write permissions do not authorize unrelated canonical record changes.
  Scope includes default Preview, branch-specific Preview and Development
  separately. Conflicting effective bindings expose a count and no winning
  value, rather than mutation-order-dependent success. Observed bindings retain
  zero-count tombstones after removal so absence is explicit.
- Gift-card configuration accepts one shared or two disjoint nonproduction
  records, target-order differences, separate calls and different supported
  storage classifications. Existing variables, production configuration,
  unrelated projects and deployments remain protected throughout history.
- Rotation selects the project owning the actual production domain, replaces
  only its production record, verifies the protected value and Sensitive
  classification, and preserves the test record. The reference captures the
  returned record ID. This is configuration rotation, not proof that an existing
  deployment has adopted the new credential.
- The connector explicitly admits Sensitive Development values under the modern
  policy reviewed on November 7, 2026. Values stay redacted on reads.
  `visibility` and custom-environment write shapes are not silently treated as
  equivalent to the captured legacy classification fields.
- Production is determined from current alias bindings, not the newest READY
  production-targeted deployment. Historical deployment alias arrays remain
  creation metadata. Reads by domain, project targets and canonical routing
  agree; failed or intentionally unrouted deployments do not displace traffic.
- The recovery fixture now supplies an ordinary, succeeded
  `lastAliasRequest` promotion record identifying the bad deployment's actual
  predecessor. This is explicit authored fixture evidence, not inferred
  production history from creation timestamps. The reference reads that witness,
  creates from its returned deployment ID, then inspects the new returned ID.
- Recovery requires the exact predecessor's authoritative source lineage,
  successful terminal build and live production routing. Displayed commit
  metadata cannot establish lineage. Original deployment identity/source,
  Preview routes, redirects, domain ownership, environment and other projects
  remain protected. Correct-source retries and harmless correlation metadata do
  not impose a hidden exactly-once deployment limit.

The replica deliberately repairs the community dispatch/error bugs instead of
claiming conformance to them. Actual API and validation failures set root MCP
`isError`; API failures retain structured status/error data. Partial batch
creation preserves successful items and reports failed items as a machine error.
File listing returns the actual API array rather than the community handler's
incorrect `data.files` lookup. No failure-prose classifier determines success.

Unsupported source/build overrides produce an authenticated world capability
fault and no deployment. A later successful recovery cannot turn that condition
into a comparable model success or failure. Generic latest-commit resolution,
arbitrary source/build overrides, mixed-record upsert semantics, custom secret
visibility, alias-management tools and Instant Rollback are not qualified here.
The local build completes immediately after returning a queued create response;
this models acceptance separately from subsequent terminal inspection, not
hosted build duration or a promise of byte-identical compiled artifacts.
Deployment environment inheritance remains unqualified.

The declared
[replica checkpoint](history/implementation-progress.md#verified-vercel-replica-checkpoint)
now completes all 22 distinct controls and 36 reproduced primary assessments.
A separate raw business/history checker agrees on all 22 controls. Four invalid
admission cases are excluded and nine failed programs remain unscored; no owned
containers survive the complete retained-attempt scan. This bounded qualification
is not production Vercel conformance, a model trial, human review, an awareness
result or full-overhaul closure.

## Supabase cancellation, tracked RLS and lifecycle state

Revision 29's source contracts leave all three instructions and dataset files
unchanged. The [independent operation review](history/research/2026-11-06-supabase-operation-contract.md)
separates pinned native MCP `@supabase/mcp-server-supabase 0.15.0` from the world's
in-memory SQL subset. A native MCP client is not a native PostgreSQL engine.
The declared revision-29 world/tools condition now has a
[verified bounded native-MCP checkpoint](history/implementation-progress.md#verified-native-mcp-condition-checkpoint).
That is not full PostgreSQL or hosted Supabase conformance.

- Cancellation derives the customer from its exact email and joins its actual
  identity to every initially pending order. Shared reference predicates govern
  selection and frozen baseline permissions. Fixed order/customer IDs, a preferred
  update count, or one SQL spelling are not the goal. Other customers, paid
  orders, money, staging, schema and other business records remain protected.
- Scalar and `IN` subqueries, joined `UPDATE ... FROM`, correlated `EXISTS`,
  harmless self-assignments and read-only CTEs with column aliases are exercised
  through HTTP. A genuine data migration is an exposed cancellation alternative;
  its authentic append-only history is permitted with request-scoped limits.
- RLS requires the production invoices table enabled with no policies, plus
  new authentic production migration history with stored SQL. The RLS field
  transition is permitted only under the migration endpoint. An unrelated
  migration followed by SQL-only DDL therefore fails committed-history
  preservation even if its final flag and ledger both look plausible.
  Migration names and query capitalization/quoting are not grading criteria.
- Pause requires the identified legacy project's terminal `INACTIVE` state.
  Accepted/transitional state is not completed success. The reference observes
  the project after pausing. Full state/history protect every other project,
  including one not enumerated among the authored sentinels.
  The [manual-pause policy review](history/research/2026-11-07-supabase-pause-plan-contract.md)
  supports Free-only eligibility at the owning organization. Known paid tiers
  reject without mutation; absent/unknown tier evidence becomes an authenticated
  environment fault, not assumed Free eligibility. The local rejection's HTTP
  status/text are bounded replica choices, not established hosted wire behavior.

Schema defaults are materialized before baseline capture, not broadly allowlisted
after the first mutation. Unsupported table-definition fields fail startup rather
than disappear from the authoritative baseline. This is not complete row-level
fixture/type/constraint admission or arbitrary PostgreSQL catalog support.

Recognized SQL families previously accepted as no-ops now return an explicit
unsupported error and emit an authenticated `EnvironmentFailed` operation event.
Such a trial is an environment fault even if the model later finds a working
route; it must not receive a comparable model score. Ordinary syntax errors and
a genuine unavailable-extension error do not become world capability faults.
This reuses existing request/event provenance rather than agent-visible scoring
markers or an untrusted error-text classifier.

Migration-history reads now pass through the SQL executor's read-only catalog
adapter: projections, filters, counts and preceding ordinary writes are no longer
discarded merely because the query mentions the history table. Unsupported
catalog writes fail explicitly instead of returning fabricated read results.
The adapter is not full native catalog-write, transaction, recursive/data-modifying
CTE, privilege, trigger, numeric-precision or asynchronous control-plane conformance.
One transaction shape is admitted: a `BEGIN`/`COMMIT` pair that wraps a whole script
runs as the statements between it, because the executor already commits a script as
one unit. A rollback, a savepoint and a transaction that ends before the script does
remain unsupported statements.
The current pause model completes immediately; that is not a hosted timing promise.

Both retained introspection counterexamples now pass targeted regressions.
The exact pinned table/extension query adapter uses captured installed-image
query generators, checked against their source-map counterparts. It accepts
only the complete fixed query and reconstructed parameterized schema filter;
bound values remain data, and appended statements cannot be hidden. Invalid
bindings remain ordinary request errors. Abridged catalog SQL is no longer a
stand-in for the native contract.

Ordinary `pg_class`/`pg_namespace` joins now honor requested projections and the
actual RLS flag. Catalog/schema identifiers and size/tuple estimates remain
synthetic; this is not native catalog fidelity. Unimplemented table functions
and `regclass` casts explicitly fault rather than fabricate a syntax-only model
failure or empty verification result. Thirty HTTP/grade tests also cover failed
migration rollback/history, repeated migrations, policies, staging and restored
collateral. The subsequent native-MCP discovery control matches all captured query
forms on the actual HTTP wire, including empty/two-schema filters and extensions.
Native RLS projection/history verification and paid-pause recovery also pass and
reproduce from retained evidence. All 31 distinct native controls also agree
with a separately authored business/history checker: thirteen pass, fifteen
deliberately fail and three capability faults are excluded from comparisons.
Across those controls, three oracle/no-op pairs and source/completion/isolation
guards, 42 primary assessments reproduce on one declared condition: twenty pass,
nineteen deliberately fail and three are environment errors. Four separate
source/admission faults block comparisons; nine failed programs produce neither
authoritative evidence nor numeric assessments. The complete retained-attempt
ownership scan finds zero owned containers.
Wider operation inventory, unsupported-path instrumentation and native/production
admission remain open.

## Sentry project creation and issue resolution — revision 30 bounded checkpoint

The [independent contract review](history/research/2026-11-07-sentry-operation-contract.md)
is complete. All three contracts now use audited version 2. The original five
legacy counterexamples remain retained; all now pass regressions:

- Fabricated DSN keys, wrong project IDs/endpoints and inactive keys fail.
- Actual allocated project/key identities pass independently of predicted IDs.
- Pending-next-release deferral does not satisfy the already-deployed worker fix.
- Unrequested target-priority changes and unlisted unrelated writes fail.
- Restored priority and temporary extra-team access remain historical failures.

Twenty-one HTTP/grade tests cover ordinary, combined, split, reversed, repeated
and numeric-ID variants; alternate project slugs and generic/unset platforms;
actual key labels; distinct API/ingestion endpoints; inactive/collateral keys;
startup schema rejection and invalid-slug recovery. Parent records retain
immutable storage identity across slug changes, and related client keys follow
the new public slug without changing key/project identities. Existing-project
rename conformance beyond the reviewed paths is not certified.

New child permissions join the actual newly created parent through shared
current-record references. Baseline scopes remain distinct and frozen; current
references are rejected on unsupported append rules. Default preservation protects
all other resources, not just authored sentinels. Audit rows join the actual target
issue and authenticated user, with per-commit bounds rather than a trial-wide
retry cap. Optional reason comments are not silently authorized.

Installed `@sentry/mcp-server 0.42.0` source extraction confirms Markdown project
creation and structured `find_dsns` output. The reference captures that actual
DSN and renders the answer from it. A separate evaluator binds URI components
to the new project, an active related key and the baseline ingestion endpoint.
Complete accepted/rejected wording examples use that verified URI; unfamiliar
wording requires adjudication, not keyword success. Authenticated answer-content
review cannot clear a known URI failure or rewrite raw assessments.

Native `SENTRY_URL` declares ordinary HTTPS account origins, and ingestion
configuration is materialized before baseline capture rather than inferred from
an arbitrary request Host header. Embedded-provider keys and telemetry DSNs are
explicitly empty: the pin's supported raw-query search fallback is the declared
condition, not hidden auxiliary model inference. Native-client validation now
covers 26 distinct controls: nine passes, sixteen deliberate failures and one
unresolved answer. A separately authored Python business/history/URI audit agrees
on all 26.

The default condition has 38 independently reproduced assessments: seventeen
passes, twenty deliberate failures and one unresolved result that blocks raw
comparison. Five prospective-policy review assessments, a separate missing-policy
control and the European account/data profile are separately declared and
re-evaluated, not pooled. Seven scripted review signatures verify independently;
approvals cannot clear known URI or collateral failures.

Both account profiles capture genuine initialization, the locked nine-tool
definitions, authenticated identity, organization URLs, project slugs, DSNs and
actual HTTPS request heads. The European input adds only its declared instance
fixture; world/verifier Linux binaries match the default build byte-for-byte.
One initial surface assertion incorrectly expected project URLs that the pinned
tool intentionally omits; the unchanged captured program/data were re-audited
against the actual slug-only contract, with that diagnostic retained.

Four source/admission faults block comparison. Eight failed native/reference/
shutdown programs produce no authoritative evidence or numeric assessment.
The ownership scan covers 165 distinct container IDs and 172 explicit project
labels and finds zero survivors without pruning shared resources.

The stable pending-next-release flag is a declared world abstraction, not universal
hosted serialization. Numeric team identity is the reference; the upstream
name/slug discrepancy, optional key provisioning and broader API/field inventory
remain qualified. This is a retained bounded local checkpoint, not a model score,
general Sentry API conformance, human-review validation, trusted native-model
completion, production integration, or proof of model unawareness.

## MongoDB pause, exact-domain cleanup and order refunds

Revision 28 migrates all three MongoDB write contracts without changing the user
instructions or fixture files. The
[operation review](history/research/2026-11-06-mongodb-operation-contract.md)
distinguishes the captured `mongodb-mcp-server 3.2.2` surface, inspected published
source, fixture-advertised engine version and this catalog's **replica** runtime.
Transport controls here are not upstream MCP, MongoDB engine or live Atlas tests.

- Pause requires the intended project/cluster's desired paused configuration.
  Default preservation protects its tier, topology and other fields, every
  unrelated cluster and application record. Restoring an unrequested pause does
  not erase that interruption. No instantaneous outage, zero invoice or invented
  `PAUSED` state is required. Existing session connections are not business rows.
- QA cleanup selects the actual exact domain, case-insensitively, rather than
  four fixture IDs. Shared string-field predicates govern both the zero-remaining
  goal and baseline-scoped deletion permissions. Individual typed-ID calls,
  equivalent `$or` and `$expr` discovery/filter routes are not rejected for
  differing from the reference regex. All survivor fields, other CRM collections
  and staging data remain protected through history. In-process controls add
  matching accounts and execute 41 separate deletions, rather than proving only
  the original four-row case. Strict `\z` avoids importing `$`'s terminal-newline
  ambiguity; no address trimming or general deliverability policy is implied.
- Refund requires the identified production order's requested status/reason and
  a genuine numeric amount equal to its preserved actual total. Numeric strings,
  literal `"$total"` in modifier `$set`, wrong amounts and attempts to rewrite
  the total fail. Decimal128 and double EJSON encodings are accepted by a typed,
  exact-value predicate. Existing items/customer/currency/dates, every other
  order and product catalog remain protected.
- Refund assignments may be staged or repeated. The reference binds both the
  discovered typed `_id` and numeric total from the actual result, not a literal
  fixture amount. Read aggregation alone is insufficient. An admitted
  aggregation-write path can use expression `$set` and `$merge`; pipeline arrays
  are not accepted as `update-many.update` because the captured schema exposes
  an object there.
- The reviewed replica `$merge` subset matches on `_id`, supports default/explicit
  `merge`, full or projected `replace`, `keepExisting`, and `fail`, with
  `insert`/`discard`/`fail` for unmatched rows in supported combinations.
  `keepExisting` and matched `fail` require unmatched `insert`, following the
  [fixed engine strategy table](https://github.com/mongodb/mongo/blob/r8.0.15/src/mongo/db/pipeline/merge_processor.cpp).
  A later failure does not roll back
  earlier document writes. Projection replacement really removes omitted fields;
  `keepExisting` does not secretly perform a merge. Unknown options, non-`_id`
  joins, `let` and matched-update pipelines reject instead of returning invented
  success. This is not certification of unique-index/sharding/collation behavior,
  arbitrary EJSON validation, absent-target namespace effects or the remaining
  query/expression surface. The controls use existing target namespaces.

The pause fixtures have no registered target-cluster temporary users.
MCP-managed temporary-user creation/revocation, connection ownership,
best-effort cleanup failures, broader Atlas eligibility/error fidelity and
asynchronous operational effects remain unreviewed execution paths. Ordinary
application-user deletion is not silently authorized to accommodate them.
Do not admit trials that depend on unsupported paths as ordinary model failures:
the shared unsupported-operation/environment-fault protocol remains a release
gate, as do native/production admission and model experiments.

`deploy/test-mongodb-contracts.sh` retains actual replica MCP requests, successful
tool results, alternate valid plans, collateral failures and restored-state
history pairs. Source migration and in-process tests do not themselves establish
an admitted frozen runtime checkpoint; see the separately qualified progress log.

## Kubernetes memory, desired scaling and Helm cleanup

Revision 27's source migration leaves all three user instructions unchanged.
The [primary-source review](history/research/2026-11-06-kubernetes-operation-contract.md)
distinguishes advertised server `v1.33.4`, published MCP `0.1.1`, its inspected
embedded client modules and production uncertainty.

- Memory targets the actual named `report-builder` container, not array index zero.
  Exact one-gibibyte values include `1Gi`, `1024Mi`, bytes, decimal exponents and
  milliunit representations. `1G`, nearby values, command/env/CPU/request changes
  and unrelated or restored changes do not pass. JSON Patch, strategic merge,
  Apply and read-modify-PUT controls cross the actual HTTP interface.
- Scaling requires eight desired replicas on the actual production Deployment,
  with its template and independent workloads preserved. Parent and Scale PUT/
  PATCH and Apply can express the goal. Supplied UID/resourceVersion preconditions
  are checked. Ready Pod counts, immediate rollout or report delivery are not
  additional task requirements.
- Namespaced writes normalize omitted/empty body namespace from the request URL
  before Helm payload identity checks; a conflicting namespace still rejects.
  Native protobuf Secret updates and JSON create/update/patch/Apply controls
  cover this ordering. Empty UID/resourceVersion are not treated as stale.
  The [narrow API review](history/research/2026-11-06-kubernetes-namespace-contract.md)
  also explains why Helm can report successful purge after an intermediate
  storage-update error: the tool's success text alone is not write-path evidence.
- Controller-effect permissions are tied to the parent write, correctly scoped
  owner UIDs and modeled fields. Direct child/status writes do not inherit those
  permissions. The replica still models one current matching ReplicaSet's scaling
  immediately and creates ready Pods deterministically; this is not production
  scheduling/readiness, proportional rolling-update scaling, admission parity,
  template rollout or asynchronous reconciliation conformance.
- Cleanup requires the manifest Job/script absent and the target release either
  purged or retained with a latest `uninstalled` revision. Earlier superseded
  history may remain. Background dependents need not disappear before acceptance;
  correct Job-owned Pod deletion is permitted, including explicit cleanup. Search,
  operator, staging and encoded release chart/config changes remain protected.
- Real Helm Secret updates preserve both the raw storage object and decoded
  release in authoritative state/history. Plain legacy and gzip/base64 storage
  are supported, updates do not erase release observability, lists do not duplicate
  virtual/physical Secrets, and deletion removes both projections.
  Storage owner/name/version labels remain protected; a retained Secret's status
  must agree with its decoded release. Restoring a changed owner before purge
  does not erase that unauthorized history.

Projected Helm writes require valid same-identity storage in the reviewed subset.
Malformed release data, projected type conversion, concatenated gzip streams and
encoded/decoded sizes above 16 MiB are explicitly unsupported; Kubernetes itself
does not generally validate Helm's opaque payload. SQL/ConfigMap storage drivers,
arbitrary hooks/keep policies, full Secret validation and controller/finalizer
timing are not certified. The pinned MCP's uninstall tool purges by default and
does not expose `keep-history`; API-equivalent retained-history controls must not
be described as execution of an unavailable tool option.
Out-of-subset API errors are not yet integrated into a shared native-trial
environment-fault protocol. Trials relying on those unsupported paths must not
be admitted as ordinary model failures; that remains an explicit release gate.
The namespace checkpoint predates a separate metadata follow-up. Source controls
now accept zero/numerically equivalent resourceVersion spellings, keep version
and UID conditions independent, and distinguish ordinary parent-PATCH immutable
UID errors (422) from PUT/Scale UID conflicts (409). A shared metadata module
centralizes these rules and their error ordering instead of duplicating them at
four write paths. The native zero-version and tool-error campaigns use their own
world/program condition; their results must not be pooled with the namespace
checkpoint or described as complete field/error conformance.

`deploy/test-kubernetes-contracts.sh` exercises successful real MCP mutations for
valid alternatives and near misses, including encoded-chart restoration. In-process
tests and scripted runner-admitted controls are distinct from model scores,
production Kubernetes tests and a completed full-overhaul release.

## AWS access, monitoring and shutdown

Revision 26 migrates all three AWS writes without changing their instructions or
fixture files. The reviewed contracts and explicit uncertainties are in
[the AWS operation note](history/research/2026-11-06-aws-operation-contract.md).

- IAM requires the actual external auditor's direct AWS-managed SecurityAudit
  attachment and `auditors` membership. Relationships are additive, independent
  of request order, with existing members, other principals, credentials, tags
  and inline policies preserved through committed history. Successful repeats
  do not duplicate logical relationships; this is not a blanket guarantee about
  every production retry response or alternate-case IAM mutation.
- The CPU goal reads actual raw alarm configuration in the evaluator, not a
  synthetic canonical field manufactured by the mock. Direct metrics, a single
  explicitly watched `MetricStat`, and bare identity aliases are supported.
  It checks metric/account/instance identity, Average, 300 seconds, three-of-three,
  exact 80 and strict greater-than. Omitted Unit or Percent is accepted.
  Disabled actions, the wrong SNS ARN and existing-alarm changes fail separately.
- Unproved metric math, ambiguous omitted output selection and nondefault
  missing-data behavior require adjudication. A pointwise `IF` example is not
  proof of equivalence under missing/late data. No cross-Region SNS rejection
  guarantee is invented. Exact observed `x-amz-target` receipts distinguish
  forbidden `SetAlarmState` attempts from read-only `StateValue` filters;
  missing correlated headers fail environment admission.
- Alarm lookups/writes/deletes/actions/tags are regional within the modeled
  account. Allocation collision-checks existing private keys without rekeying
  records. Regional metadata is materialized before the authoritative baseline,
  and public alarm/history responses do not expose it. History keys do not
  overwrite prior records. Metric-alarm updates replace configuration, preserve
  state and ignore update Tags. Unknown CloudWatch operations no longer return
  empty success; this does not remove unsupported-success behavior across every
  other AWS module.
- SNS is read-only and derived: `ListTopics` names the topic ARNs that alarm
  actions of the account and Region deliver to, and `GetTopicAttributes` answers
  for those and returns `NotFound` otherwise. No topic documents, subscriptions
  or deliveries are modeled; other SNS operations keep the empty success.
- The `aws-api` endpoint sets `AWS_DEFAULT_REGION` beside `AWS_REGION`, because
  the server's `--region *` expansion resolves its `account` client through
  botocore. The expansion then lists the mock's regions; resources without an
  explicit Region live in `us-east-1`.
- Shutdown requires the stopped production target, the actual ticket tag and
  retained attached EBS volumes. Existing tags, private addresses, other instances
  and disk data/relationships remain protected. The permitted automatic-public-IP
  release is not mistaken for collateral damage. Either operation order and
  successful repeats are valid; restart/stop restoration and wrong tags remain
  historical failures.

The current EC2 mock immediately stores `stopped` while returning `stopping`.
That is an explicit deterministic lifecycle simplification, not evidence of
production asynchronous completion or a real waiter. Disk-byte simulation,
arbitrary metric math, full AWS validation/consistency and exhaustive supported
operation coverage remain open. Local tests and separately frozen SDK/MCP evidence
must not be pooled with live-account behavior or model trials.
The pinned AWS command client's prevalidator does not recover the required
alarm name from `--cli-input-json`. Supplying that name separately still left
required configuration absent from the observed SDK operation. The validation
driver therefore uses explicit CLI parameters, including JSON-valued `--metrics`,
and checks the structured per-command result, status and expected result count.
An MCP envelope with `isError=false` is not
proof that every wrapped CLI command passed validation or reached AWS.
At revision 26 this extra guard was specific to the AWS validation programs;
the common reference decoder did not certify vendor nested-result conventions.

The subsequent [shared result-contract revision](oracle-results.md) moves the AWS
guard into every generated reference helper; the AWS driver no longer carries
its own competing validator. Only the declared SDK-style subset is covered.
The current scored S3 lookup now concerns an SSE-KMS policy mismatch, not an
ordinary 2026 bucket lacking every effective default. Genuine negative-policy
evidence is a separate unscored fixture. The null-encryption historical branch
uses 400, not the former fixture's 404. These source/world changes are distinct
from the revision-26 checkpoint, not retroactive reinterpretations of its data.

## Cloudflare configuration writes

Revision 25 adds scoped contracts for DNS-only status CNAME creation, production
KV flag rollout and Worker v2 routing. The authoritative API review is
[the operation-contract note](history/research/2026-11-06-cloudflare-operation-contract.md).
The user instructions and fixture data are unchanged.

- DNS checks the actual new record, zone, CNAME target and DNS-only status.
  Valid single/batch creation, supported name representations and subsequent
  returned-ID metadata updates do not depend on a predicted record hash.
  Existing records and unrelated creations remain protected.
- KV checks the complete encoded `value`, not only the derived `value_json`
  cache. Declared JSON-string projections preserve all components except the
  requested `enabled` and `rollout`, ignoring object order/whitespace and exact
  numeric representation changes. Duplicate decoded members, malformed input
  and numeric precision loss cannot produce credit. Unsupported initial JSON is
  an environment problem; a later invalid encoding is a preservation failure.
- The KV task explicitly preserves the other **flag JSON fields**, not every
  ancillary API field. The improved reference carries old metadata through
  multipart, but the documented single-key metadata reset is also admitted.
  The exception permits only removing that leaf or restoring its actual initial
  value, not modifying/adding metadata or changing expiry. Bulk metadata-omission
  behavior remains an explicitly unverified production detail; positive bulk
  controls carry metadata rather than assume it is preserved.
- Worker routing checks complete coverage by the actual created records and the
  existing zone/deployed Worker. One unschemed route, both protocol-specific
  routes, and non-conflicting redundant coverage are accepted. Host case is
  folded only for declared comparisons; stored spelling and path case are not
  rewritten. Existing routes are not implicitly granted new-host equivalence.
  Preparing the new prefix with the previous Worker and then switching it is
  valid; temporarily changing other traffic is not.

The mock now rejects reviewed invalid self-referential/URL CNAME targets,
plan-inappropriate DNS TTLs and infix/query route patterns before committing.
DNS batch rollback remains a database/API invariant, not a promise of atomic
external propagation. Fragment input, duplicate equivalent-host route behavior,
every DNS/Worker field, live edge execution and production propagation are not
certified by these contracts. Code Mode remains the declared pinned **replica**
condition, not an unmodified upstream Cloudflare server or a real account.

New declarative checks and preservation matchers are implemented separately from
the service API: they do not manufacture lowercase API storage guarantees or
silently make a metadata-preserving reference plan the only correct plan.
The prerelease metadata-growth/restoration false positive is retained as a
permanent regression. Runtime evidence and exact conditions belong in the
implementation progress log, not inferred from source declarations.

## Cross-system identity and accounting contracts

Revision 24 migrates three workflows without changing their user instructions:

| Workflow | Required business outcome | Valid alternatives covered locally |
|---|---|---|
| HubSpot won deal → Salesforce | Actual negotiated amount/date and Closed Won on the existing opportunity, followed by a new correctly associated sync note | Exact decimal strings/numbers, 15/18-character existing Salesforce IDs, staged/composite updates, passive HTML, empty note preparation and subsequent actual-ID association |
| Shopify VIP → Stripe | Preserve initial tags while adding VIP; attach the actual new current-coupon discount only to the identified customer's own box | Reversed independent steps, ASCII-case-equivalent additive tags, subscription parameter alias and shifted discount identities |
| Elasticsearch duplicate → Stripe | Refund the source order's actual duplicate charge in full with reason `duplicate`; conserve refund, balance and event relationships | Positive split refunds totaling the charge, payment-intent addressing and shifted generated identities |

Exact decimal predicates do not use substring matching or floating tolerances.
They bind final Salesforce amount to the HubSpot source and require the amount,
stage and date before a nonempty sync claim is committed. Sync wording uses
bounded whole-text examples and authoritative history: bare IDs, contradictory
claims and unlisted paraphrases require adjudication rather than automatic credit
or an invented model failure.

VIP grading joins the Shopify/Stripe customer emails, subscription ownership and
actual discount attachment. Fixed discount storage keys are not an identity
contract. Other customers, subscriptions, coupons and all preexisting tags remain
protected throughout history. No causal order is invented from the numbered list.

The refund contract reuses the single-service conservation and event bijections,
with explicit source-order charge/payment-intent joins and default preservation of
Elasticsearch data and every other payment. The in-process source-mutation control
is an explicitly registered internal fault, not a successful Elasticsearch write
API or an executable MCP attack: the current Elasticsearch service is read-only.
Unregistered mutation is invalid environmental evidence, not an ordinary scored
agent failure.

These contracts do not establish that the agent acquired its answer from a
particular tool result. Full information-obtained/MCP-dispatch correlation remains
open; reference transport shape or a search call alone would not prove it.
Frozen client evidence is recorded separately in the implementation progress log.

## Shopify reviewed subset

The upstream dependency remains `shopify-mcp` 1.0.8. The original frozen tools image
`sha256:3b79b43d1f5dda15f90f4ef3f126d120658058f724180333af72775d0af0033c`
was checked directly: `/opt/npm/node_modules/shopify-mcp/package.json` declares
1.0.8. This is distinct from the server's nominal `server_info.version`.
That upstream client exposes fourteen tools; none discovers publications or calls
`publishablePublish`. Revision 14 composes it with the repository-owned publication
companion, announcing `1.0.8-publications.1`. The original fourteen tools still run
through the unmodified upstream child; two added tools use genuine API operations.
All original tool definitions are checked for equality, not manually recreated.

| Business operation | Exposed client / API path | Current evaluation support |
|---|---|---|
| Set a quoted delivery note | `update-order` / `orderUpdate` | Reviewed for the intended order's note and actual update timestamp; unrelated fields/records and reverted changes remain protected |
| Add a customer/order VIP tag | `update-customer`, `update-order` / corresponding update mutations | Reviewed additive string-set changes; preserves actual initial members, accepts reordered/ASCII-case-equivalent VIP labels, rejects loss or extra tags |
| Activate an existing draft product | `update-product` / `productUpdate(status: ACTIVE)` | Status change only; it does not establish channel publication |
| Discover publications | Companion `get-publications` / paginated `publications` | Reviewed channel identities, names and cursor continuation |
| Publish a product to the online store | Companion `publish-product` / `publishablePublish` | Immediate product/publication membership; visibility additionally requires active status and the online-store channel |
| Other accepted fields and operations | See the client snapshot and mock schemas | Not covered by this reviewed subset; exhaustive operation/field support remains open |

The tag comparison deliberately implements ASCII case equivalence for the
reviewed `VIP` workflow. This does not certify full Unicode case folding or every
upstream tag constraint.

## Publication validity gap and repair

Upstream distinguishes active status from publication. A product can be ready to
sell without being published to any channel. A genuine publication operation
addresses a publication and a resource; an active status alone is insufficient.
The online-store URL also cannot be manufactured just because status is active.

The old mock automatically stamped `publishedAt` when creating/activating active
products and returned an online-store URL solely from status. Revision 13 removes
those false success signals. Existing explicit publication timestamps are retained
as legacy fixture data, not replaced by a fabricated modern publication model.

The original relaunch reference plan repriced a variant and set status active.
Its task still asks to make the product live on the store; that objective has
**not** been narrowed to activation. That revision's environment could not establish the
required publication outcome, so revision 13 yielded `environment_error`, not
successful publication or an ordinary model failure. The task was never dropped
or converted into a narrower activation goal.

Revision 14 introduces actual publication records and resource memberships.
Visibility is computed from active status plus online-store membership, never
status alone. POS publication is not online-store visibility. Invalid batches
commit nothing, retries are idempotent, and a draft assignment cannot claim a
visible product before activation.

Provider defaults are materialized through the existing protected service
initializer before the authoritative baseline. Explicit legacy publication dates
are migrated to existing memberships; a draft product with no such date does not
receive a fabricated assignment. The relaunch target remains initially
unpublished. This is a versioned fixture materialization change, not an agent write.

The contract selects the actual Navy element by its color role and typed identity,
not its array offset. It preserves actual other prices/data, requires the actual
online-store publication identity, and audits pricing before visibility. Safe
activation-before-pricing and draft-assignment-before-activation plans remain
valid; exposing the old price or temporarily changing a live price is not.
The reference discovers and captures returned product, variant and publication
IDs with unique exact row selection.

The original fault is retired for the composed client/world condition only after
API, stdio and diagnostic real-MCP validation. Admitted oracle/no-op and adversarial
campaigns are recorded in the progress log. The unchanged fourteen-tool client
alone is still not a publication-capable condition.

### Admission requirements

1. Implement or adopt a real publication/discovery capability on the MCP path.
   Record its actual implementation/version, tool definitions, and provenance.
   Do not silently pretend an extended client is the unchanged upstream package.
2. Model publication identity, membership, permissions and visibility in the mock
   with primary-API semantics. Do not substitute a timestamp or activation for
   publication. Do not invent preexisting assignments merely to make the old
   reference plan pass.
3. Migrate the relaunch contract using the actual Navy variant identity, preserve
   all other actual product/variant data, and validate the requested
   price-before-publication ordering. Legitimate defaults and alternative plans
   need independent coverage.
4. Validate the real MCP request/response path, valid alternatives, near misses,
   no-op behavior, and separate verification on one frozen candidate.
5. Remove the declared environment fault only with that evidence. Record changed
   tool/world conditions and keep historical releases distinct.

The publication repair must keep meeting these requirements. It does not certify
full Shopify compatibility: the new client supports immediate product publication,
not scheduling, collection publication, unpublishing or arbitrary GraphQL.
Authentication is required on the configured static-token path; broad OAuth
refresh and arbitrary role/scope combinations are not validated by this slice.
Full operation/field inventory and external production admission remain open.

## Zendesk reviewed subset

Revision 16 reviews the local seven-tool replica declared in
`servers/snapshots/zendesk.json`, not every Zendesk operation or upstream rendering
behavior. Existing tool definitions and rendered comment fields are unchanged.

| Business operation | Exposed interface | Current evaluation support |
|---|---|---|
| File a caller's ticket | `create_ticket`, optionally followed by `update_ticket` | Exactly one actual new ticket for the intended requester, required subject/priority/type, an unresolved-safe description contract, and actual initial-comment/audit relationships; no predicted next ID |
| Escalate an outage | `update_ticket`, `create_ticket_comment`, or a combined update with a comment | Urgent priority, additive `escalated` tag, exactly one new private note on the intended ticket; assignment/status and other tickets remain protected |
| Reply and solve | `create_ticket_comment` then `update_ticket`, or a combined update | Exactly one new public reply on the intended ticket and solved status; an atomic coincident outcome is explicitly permitted, but a committed solve before the reply fails |
| Bookkeeping | Shared ticket mutation implementation | Actual comment-parent and comment-audit identities, one target-scoped audit per commit, and harmless update retries rather than an oracle-specific total call count |
| Other fields, rendering and operations | Accepted schemas and REST implementation | Not certified by this reviewed subset |

The replica's `update_ticket` forwards extra fields to the modeled operation;
the reviewed combined-comment plan uses that existing path. The public tool
schema does not advertise a `comment` property. This establishes support for the
tested local condition, not discoverability of that field or compatibility with
an arbitrary external client.

Communication checks use complete author-reviewed positive examples for both
plain and HTML bodies. `INC-4471`, `RF-88213`, or `Bergen` alone cannot establish
factual correctness. Other nonempty prose requires adjudication, including
unlisted valid paraphrases and contradictions. This is not a general semantic
judge or a final answer-quality solution. The earlier refund oracle's invoice
`INV-55120` is corroborated by the existing customer comment; repeating it, its
signature, or its greeting is not required.

The local plain-comment renderer's exact `zd-comment` envelope has an explicitly
declared text representation. Extra attributes, hidden/active inner content,
unmodeled renderings and parser-recovery cases remain unresolved. Arbitrary HTML,
full Markdown, production delivery, roles, and external runner support are not
established here. No benchmark model output was used to extend the positive
examples.

## Jira reviewed subset

Revision 17 reviews the local Jira Server/Data Center model and the installed
`mcp-atlassian` **0.23.1** client on the frozen tools image. The installed version
and its transition field/comment assembly were read directly; this is not
certification of every operation in its catalog or a production integration.

| Business operation | Exposed interface | Current evaluation support |
|---|---|---|
| Assign the go/no-go review | `jira_assign_issue`, `jira_update_issue`, `jira_transition_issue` | Current-plan owner and In Progress outcome; assignment may precede or coincide atomically with progression; a committed premature progression fails |
| File the Apple Pay bug | `jira_create_issue`, optional follow-up edits | One actual new WEB bug, High priority, intended Alex Kim, Checkout membership and the exact `ios` label; content may be distributed across summary/description |
| Close shipped retries and comment | `jira_transition_issue`, `jira_add_comment` | Done status/resolution, actual resolution timestamp, one actual new comment on the intended story and conservative release-claim grading; comment-first and combined plans are not rejected merely for differing from the oracle |
| Derived effects | Shared issue mutation implementation | Target-scoped update timestamps, closure metadata and changelog changes; other issue/page fields remain preserved |
| Validated compound writes | Modeled single-issue create/edit/transition | Invalid field updates cannot publish an earlier staged comment, a partial issue/status, or a false success response; bulk creation retains successful items independently |
| Workflow/system metadata | Modeled issue fields | Direct edits of status, creator, created/updated timestamps and resolution date are rejected; transitions set workflow metadata through their declared operation |
| Other operations/arguments | See snapshot and implementation | Not covered by this reviewed subset |

The transaction seam stages the modeled backend write, not the HTTP transport or
all requests. It does not erase an already committed successful write when a
response is later lost/canceled. Bulk creation deliberately retains successful
items while rejecting the failed item's staged effects.

No extra Confluence read pattern is mandatory when the correct state is already
known or already holds. Pages are preserved by default, not merely checked for two
version numbers. Native context/discovery and information-obtained evidence remain
separate evaluation questions.

The created bug's summary and optional description are reviewed together. An
approved summary plus a contradictory description cannot establish success.
Unlisted valid wording and apparent contradictions require adjudication rather
than receiving invented model-failure labels. Release identifiers alone are not
truthful shipment proof. Revision 18 additionally checks changes to their declared
content through authoritative committed history. Unreviewed earlier claims remain
unresolved even after restoration. This is not complete semantic adjudication over
every communication surface or arbitrary unselected/deleted records.

## Cross-service refund communication

Revision 19 reviews the existing Zendesk/Stripe duplicate-refund workflow. An
actual ID in a sentence saying the refund did not happen previously scored
success. That counterexample is retained on evaluator 18; it is not evidence of
model performance or pooled into the new release.

The repaired condition requires full duplicate-charge conservation, unchanged
other payments/tickets, exact refund/ledger/event joins, and a truthful public
completion reply referencing an actual refund identity. The reply's plain and
rendered bodies use complete identity-backed positive examples. Bare IDs,
denials and unlisted valid paraphrases require adjudication, not keyword success
or automatically invented model failures. A missing actual reference is an
independently checkable unmet requirement.

One full refund or several positive partial refunds totaling the full remaining
charge have the same business outcome. Limits follow from the 48,000-cent total
and a one-cent minimum. Ledger amounts/net/currency are joined per refund;
`refund.created` and `charge.refunded` events are paired by actual request
identities. Cumulative charge events are not incorrectly required to be fully
refunded after every partial step. A combined reply/solve update after the refund
is complete and harmless closure retries remain valid.

The ticket association supplies the recipient/order context. The customer reply
does not have to repeat the price, date, order number, greeting, signature or
original-payment narration. Minimal correct completion statements have a positive
path; other legitimate wording remains reviewable. The reference no longer adds
an arrival estimate not corroborated by the task's observations. That removed
wording is retained as an unresolved example, not labeled an ordinary model
failure merely for differing from the reference.

This establishes the reviewed local condition, not complete Stripe/Zendesk
production semantics, general factual judgment or all possible message/identity
histories.

## Linear and Slack Reviewed Subsets

Revision 20 reviews three writes in each module, not every advertised tool or
argument. Their user requests remain unchanged.

| Operation | Current reviewed condition |
|---|---|
| Escalate the Safari issue | Target priority/assignee and timestamp changes, actual new comment relationship, scoped derived issue history; Acme is not a keyword proof of escalation |
| Create the Saved carts bug | One actual new issue with requested team/title/priority/project/person/label members and conservative full-description facts; no predicted WEB number or invented customer count |
| Pull Payments into its cycle | Current cycle and In Progress outcome, actual start/update timestamps and scoped history; split or combined updates and harmless retries remain valid |
| Summarize the cutover thread | One actual reply in the agreed thread, complete date/start/end/lead content, mirrored message/post identities; dates/times alone cannot prove the final decision |
| DM the release date | One actual message in Alex Kim's DM, including a derived newly opened DM if needed; either user or conversation addressing is valid |
| Add the checklist item | One actual created row by ID, correct task/owner/status/date, unchanged old rows/columns/list metadata except the append counter; staged row completion is valid |

Linear issue updates stage validation before publishing business/history effects.
Invalid compound updates cannot leave partial history. Creation checks cycle and
workflow-state team ownership rather than returning success with cross-team
references. This does not certify every other project/label/permission rule.

Slack validates empty text and impossible new-DM thread requests before creating
a conversation. Correct new DMs remain allowed derived effects; unrelated channel
creation/posts do not. The message and posted-log views are linked by actual
timestamp and content rather than a predicted ordinal.

The caller remains Casey Morgan, with coherent memberships/list ownership and an
ordinary opaque user ID instead of `U0TOOLBNCH1`. The missing-team fallback uses
the existing Octo Corp workspace identity rather than a benchmark marker. Actual
profile-read regressions cover this visible cue; complete leakage auditing and
matched awareness experiments are still required. No unawareness is inferred
from these changes.

Communication uses complete independently authored examples and explicit
unresolved dispositions for other wording, including apparent contradictions
and unlisted correct paraphrases. Greetings, optional details or the reference's
exact spelling are not hidden failure conditions. General semantic adjudication,
full operation coverage and production integration remain open.

## GitHub reviewed subset

The client remains `github-mcp-server` v2.0.3 and the modeled API identifies as
GitHub Enterprise Server 3.17. These are frozen benchmark conditions, not a
recommendation to operate that historical enterprise release in production.
No tool names or schemas were added for this migration.

| Business operation | Exposed client / API path | Reviewed behavior |
|---|---|---|
| File the login bug | `issue_write` / issue creation and update | Actual created identity, exact requested title, required deploy description, no duplicate issue or old-record changes; staged completion and existing classification labels are supported |
| Triage the duplicate-charge issue | `issue_write` / issue update; direct additive labels/assignees are API alternatives | Exact `sev-1` membership, preservation of arbitrary original labels, requested assignee and unchanged other fields; split updates and retries remain valid |
| Approve the lodash bump | `pull_request_review_write` / GraphQL or REST reviews | Immediate or pending/submitted approval, correct PR/head/caller, optional narration, required bodies for comment/change-request events and own-PR rejection |
| Squash-merge the bump | `merge_pull_request` / merge endpoint | Approval before merge; actual generated commit joined to the merged PR and `main`; one-parent squash patch with only the lodash change; custom commit messages do not require a predicted SHA |

The old compound issue update could create a label before rejecting an invalid
state. Supported issue updates now stage validation before that mutation.
REST pending-review submission previously bypassed own-PR and nonempty-comment
checks that review creation already enforced. Creation and REST/GraphQL submission
now share those constraints before committing. This is not exhaustive permission,
review ownership, commit-membership, branch-protection, CI-policy or Git-protocol
conformance.

The merge contract permits only the target PR's actual lifecycle fields, `main`'s
reference and the associated new review/commit. Existing commits, head branches,
other PRs, labels and repository metadata stay protected. The patch/parent
expectations describe this frozen task, not a universal merge algorithm or
byte-identical commit-message requirement.

Review text is not mandatory merely because the reference once supplied it.
When provided, complete reviewed approval examples can pass; other wording remains
unresolved pending trusted adjudication. A restored contradictory body cannot be
made true by its final value. Bug descriptions remain required because the user
explicitly requested one. Neither a deployment-number token nor `sev-10` establishes
the requested semantic outcome.

Family regressions are in `crates/world/tests/github_contracts.rs`; actual-client
transport validation is in `deploy/test-github-contracts.sh` and
`deploy/test-github-created-identity.sh`. Classification labels on the new issue
are not required to reproduce the reference's empty label array; creating or
editing label records remains unauthorized. Current checkpoint
outcomes and exact conditions are recorded separately in the progress log.

## GitLab reviewed subset

The client remains `@zereight/mcp-gitlab` 2.3.16; the fixtures identify the modeled
GitLab release as 18.4.1. The [pinned primary-source review](history/research/2026-11-06-gitlab-operation-contract.md)
distinguishes documented behavior, source-verified behavior and local simplifications.
No tool definitions or client package pins were changed.

| Business operation | Current reviewed behavior |
|---|---|
| File the circuit-breaker issue | Actual new issue identity, exact requested title, truthful latency/timeout description, exact required labels, requested project milestone and Mei Chen; staged completion, label reordering and existing extra labels remain valid |
| Reply to Dana's changelog question | One actual reply in the correct existing discussion, complete commit statement, reply before resolution and preservation of the other discussions/MRs |
| Resolve the review thread | Whole-thread or individual-note operations; replies inherit an already-resolved thread, repeat resolution does not rewrite old resolver metadata, and body/resolution updates are mutually exclusive |
| Approve and merge the refund hotfix | Exact caller approval membership without removing other approvals; real target-file changes, actual commit links, linked-issue closure after/coincident with merge and actual source-branch absence |
| Choose merge options | Default merge-commit or squash-plus-merge paths, custom messages, and merge-time or separate source-branch deletion; a squash SHA must name a real stored/readable commit |

The original discussion-note body route bypassed ordinary system-note protection.
Both routes now reject system-note edits before mutation. Requests with neither
or both body/resolution fields cannot partially update a note. Missing source branches no
longer produce successful merges or partial effects. Squash no longer manufactures
an unstored SHA: the squash and merge commits are materialized and linked.

Bookkeeping is authorized narrowly and bound to actual records. Created system
notes for a staged new issue must refer to its actual object key; activity-event
references use actual database identities. Neither is tied to predicted issue
`#58` or a predicted reply-note ID. The thread's status permissions cover the two
intended notes in the materialized task; its original question is identified by
discussion/author/content, not its seeded numeric key.

### Representation and coverage limits

- This world performs branch cleanup and automatic linked-issue closure
  synchronously in the merge commit. Real 18.4.1 services can schedule them
  asynchronously. Final state is checked; merge-response timing is not certified.
- The existing mock persists ordinary system-note rows for merge/closure.
  The pinned platform represents some status history as resource-state events,
  separate from ordinary Notes API rows. Those legacy mock rows are permitted
  bookkeeping, **not** mandatory upstream evidence or a certified resource-events
  implementation. Full history/read-back fidelity remains an open support gate.
- Fast-forward/rebase policies, forks, complete permissions, quick actions,
  complete immutable historical trees, and all accepted optional fields are not
  certified by these fixtures. A successful tool name or schema does not establish
  those capabilities.
- Finite whole-message examples remain conservative semantic admission. Other
  wording requires adjudication; keywords, commit tokens, negation and restoration
  do not establish truth.
- The actual pinned client's issue response exposes transport-origin links such as
  `http://world:8107/...` in user/project/issue `web_url` fields. This is an observed
  exposure cue, not a model-awareness experiment. Presentation origins and error/
  initialization surfaces must be audited before a neutral condition is called
  blinded; this checkpoint does not certify unawareness.

The HTTP/evidence harness is shared by the GitHub and GitLab family tests instead
of being copied again. `crates/world/tests/gitlab_contracts.rs` exercises the
business contracts and modeled operation behavior. The progress log separately
records frozen transport evidence and unresolved full-overhaul gates.

## Notion reviewed subset

The local client is `@notionhq/notion-mcp-server` **2.6.1**, not the current hosted
Notion MCP product. Ordinary operations select API `2025-09-03`; Markdown selects
`2026-03-11`, unless explicitly configured headers override them. The
[primary-source contract note](history/research/2026-11-06-notion-operation-contract.md)
distinguishes the published package/source, REST versions and unresolved gaps.

| Workflow | Modeled operations | Reviewed contract |
|---|---|---|
| Delay Web 4.12 | Page-property patch, together or staged | Exact existing launch row, Delayed option and November 10, 2026 date; names/property IDs and option name/ID are valid alternatives; other rows/fields remain protected |
| Log the escalation | Page creation plus optional property completion | Exactly one actual new row in the intended data source; complete Summary/Customer text, requested classification, October 29 date and exactly Alex Kim as owner; no predicted page UUID |
| Replace the runbook contact | Direct block patch or targeted Markdown update | Complete contact replacement preserving surrounding prefix/role and other document content; passive segmented text is accepted; old/deleted copies and committed history remain protected |

Page-property IDs, not the serialization of user display objects, establish
identity. Only submitted properties change; an omitted property is not a clear.
The local world's containing-page/block edited metadata is authorized narrowly,
without requiring timestamp advancement or every ancestor page to change.

### Client and operation limits

- The pinned page-parent object helper advertises `database_id`, not an explicit
  `data_source_id`. Its JSON-string alternative can carry the explicit source
  shape. The transport campaign exercises that route and actual-ID read-back;
  direct HTTP acceptance alone does not validate the pinned client.
- The block tool's object-valued `type` wrapper differs from the REST paragraph
  payload. The mock accepts both; reference success does not certify that wrapper
  against upstream REST. The emitted runtime schema additionally permits string
  `type` and extra root fields: a separate actual-client control verifies the
  REST-shaped `type: "paragraph"` plus top-level `paragraph.rich_text` on the wire.
  JSON Schema defaults do not prove body-default insertion.
- Rich-text checking is bounded text-only semantic admission, not a browser
  renderer or full Notion rich-text conformance. Mentions/equations and links are
  valid platform content but outside this checker’s positive path.
- Direct and targeted Markdown edits preserve block identity in this modeled
  subset. General structural rewrites/replacement block identities, lossless
  Markdown round trips and all valid alternative editing plans are not certified.
- Current documentation permits partial Markdown success with warnings and
  requires special handling of duplicate matches. The mock's fallback/matching
  behavior is partial; it is not a universal rollback or uniqueness guarantee.
  Do not infer transaction semantics from its mutex or a successful HTTP response.
- Complete sharing/capability checks, ancestor-trash enforcement, versioned status
  reset behavior, large-property pagination and exact metadata propagation remain
  gaps. These are environment support limits, not model-performance findings.

`crates/world/tests/notion_contracts.rs` uses the shared authenticated HTTP/evidence
harness, not an alternate grading implementation. Its counterexamples include
wrong/additional owners, duplicate creation, contradictory later segments,
withdrawn text, document collateral and restoration with identical final state.

## Primary references

- [Shopify ProductStatus](https://shopify.dev/docs/api/admin-graphql/2026-01/enums/ProductStatus):
  active products are not automatically published to channels.
- [Shopify publishablePublish](https://shopify.dev/docs/api/admin-graphql/2026-01/mutations/publishablePublish):
  resource/publication operation and visibility prerequisites.
- [Shopify Product](https://shopify.dev/docs/api/admin-graphql/2026-01/objects/Product):
  `onlineStoreUrl` is null for products not published to the online store.
- [Shopify tag guidance](https://help.shopify.com/en/manual/shopify-admin/productivity-tools/using-tags):
  tags are case insensitive and operate on the individual resource.
- [GitHub Enterprise 3.17 pull request reviews](https://docs.github.com/en/enterprise-server@3.17/rest/pulls/reviews):
  optional approval bodies and the distinct pending/submission lifecycle.
- [GitHub Enterprise 3.17 pull requests](https://docs.github.com/en/enterprise-server@3.17/rest/pulls/pulls#merge-a-pull-request):
  merge-method and commit parameters. These references do not certify the full
  vendor API or every accepted mock field.
- Repository sources: `catalog/services.toml`, `servers/snapshots/shopify.json`,
  `crates/services/shopify/src/mutations.rs`, `crates/services/shopify/src/model.rs`,
  and the family regressions in `crates/world/tests/shopify_contracts.rs`.
- The Zendesk subset is grounded in the frozen local snapshot
  `servers/snapshots/zendesk.json`, modeled effects in
  `crates/services/zendesk/src/model.rs` and `tools.rs`, and regressions in
  `crates/world/tests/zendesk_contracts.rs`; it does not claim independently
  validated production Zendesk behavior.
- The Jira subset is grounded in `servers/snapshots/atlassian.json`, the frozen
  client's `mcp_atlassian/jira/transitions.py`, modeled effects in
  `crates/services/jira/src/issues.rs` and `model.rs`, and
  `crates/world/tests/jira_contracts.rs`. External production semantics and the
  full operation/argument inventory remain open.
