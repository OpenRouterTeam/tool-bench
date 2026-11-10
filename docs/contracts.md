# Evaluation contracts

The current source evaluator revision is **44**. Frozen runtime checkpoints are
recorded separately in `implementation-progress.md`. Every numeric result records both
`evaluator_version` and `contract_version`; retain those fields when exporting results.
Revision 44 integrates the remaining version-2 source contracts, projected
information delivery, alternative read shapes, exact timestamp conversion,
effect-bound actions, complete-event causal checks and typed source-bound research
answers. Source declarations,
passing local tests and current candidate images are not runtime or release
admission. Remaining answer and qualification limits are tracked in
`overhaul-checklist.md`.

## Source-Bound Research Answers

`expect.answer.source_facts` reads the declared columns of an authored baseline
Markdown table. `rows = "all"` selects every row; `rows = { largest = "share" }`
derives the requested leader from the source values. Missing source records or
unparseable source tables are environment faults; a tied ranking needs adjudication.

The reply is read as it is written. Prose, bullets, a Markdown table (also
transposed), JSON and fenced blocks are all turned into statements, and each stated
value is attributed to the source row named with it: the row named in the same
table row or JSON object, the row named before the value in a sentence, or the row
the reply was last talking about. There is no list of accepted sentences, headers or
aliases; values and row relationships decide the verdict.

- **Pass**: every requested row is named, each required fact is stated for its own
  row, and nothing denies it. Extra columns, extra rows from the same source, other
  true context, approximation marks (`~340`, `≈340`, "about 340"), grouping and
  magnitude notations (`1,150`, `1.15k`, `EUR 6,900`, `€6.9k`, `6.900 €`) and place
  qualifiers ("Pittsburgh, PA (USA)") do not change the verdict. With
  `rows = { largest = ... }` the reply must lead with the requested row. Other rows
  may follow it as context; they may precede it only when they are set aside
  ("compared with", "runner-up", "not X") or carry their own value of the ranking
  column.
- **Fail**: a requested row or required fact is missing, denied or wrong; another
  row's value is stated for a requested row before its own; a currency differs; or
  another source row is named first.
- **Unresolved**: a fact is both stated and disputed; the requested row is named only
  under a label that ranks it lower or negates it ("Runner-up: X", a
  `notTheMarketLeader` key); a JSON object gives one key two values; or a figure is in
  a notation that cannot be read exactly (a range, number words, scientific notation,
  `4.200` for 4.20).

Known limits: a reply that pairs lists with "respectively" is not attributed, a
wrong region after the right city ("Pittsburgh, Ohio") is not detected, and a reply
that states the requested row's facts first and then names another row as the leader
passes the lead rule. The authored table is authoritative; companion structured
representations are not independently cross-checked.

## On-Call Answers

`expect.answer.oncall` binds the answer to the selected schedule's `on_call_now`
identities and the directory collection:

```toml
[expect.answer.oncall]
requirement = "Identify exactly the current assignees of Checkout Primary."
schedule = { collection = "/services/grafana/oncall_schedules", matching = { name = "Checkout Primary" } }
directory = "/services/grafana/oncall_users"
allow_unambiguous_surnames = true
```

A person is identified by username, email, full name or (when allowed) a surname
no other directory person shares. A structured user list (`[{...}]` or
`{"users": [...]}`) must identify exactly the current assignees and may carry
fields the directory does not hold. Any other reply is read as prose:

- **Pass**: every current assignee is named and not denied. Other directory people
  may appear after the answer, in a denial ("not Mei Chen"), or with the other
  schedule they are really on call for.
- **Fail**: an assignee is missing or denied ("Sam Brennan is not on call"), or
  another directory person is named first.
- **Unresolved**: only a shared surname or a first name is given, the assignee
  appears only in a negative remark ("I would not page X"), the duty is both stated
  and denied, or another person is joined to the assignee ("X and Y are on call")
  with no separate plain statement.

When nobody is on call, the reply passes when it says so and names no one.

## Historical Revisions

Revision 40 migrates Google Sheets and Calendar and refines the Drive access
contract. Exact business dates, exact instants, semantic guest identities and
verified modeled Meet resources replace fixed cell spellings, predicted event
IDs and conferencing labels. Organizer-side completion does not imply guest
email delivery; notification requests are separate observations. Revision 39's
Drive source slice and revision 38's admitted scripted-client condition remain
distinct. See the [revision-40 source record](checkpoints/evaluator-40-google.md).
The subsequent [40B result condition](checkpoints/google-40b-results.md) retains
business evaluator 40 while adding an explicitly selected-replica text adapter.
Its corrected bounded scripted MCP campaign is now qualified separately; this
does not admit hosted semantics, the upstream Python/SDK or native-model execution.
Program-condition/source fingerprints remain distinct from evaluator versions.
Revision 34 migrates the three Terraform writes. Exact approval/run text,
typed immutable input snapshots, actual NAT/EIP/route relationships and
recovery-before-verification ordering replace substring and summary-only grading.
Numeric preservation predicates accept exact value-equivalent spellings but reject
wrong intermediate values even when restored. Raw v4 state, public JSON state
and public JSON plans are distinct representations of the bounded model.
Source migration is not runtime admission or proof of provider/hosted execution.
Revisions 28–33 migrate MongoDB, Supabase, Sentry, Vercel, Argo CD and Okta.
Their separate source, transport, scripted-runtime and qualification limits remain
recorded in `operation-support.md` and `implementation-progress.md`.
Revision 27 migrates the three Kubernetes writes. Exact resource quantities and
container identities replace spelling/index assumptions; desired replicas replace
a fixed request shape. Request-cause-scoped permissions admit reviewed automatic
effects without authorizing direct child/status writes. Helm release storage is
decoded and audited even when the real client writes compressed Secrets.
Revision 26 migrates AWS access, CPU alarms and retained-disk shutdown. Its later
reference-result/S3 condition has separate source, world and program identities.
Revision 25 migrates the three Cloudflare writes. Complete encoded JSON checks,
field-scoped decoded preservation, distinct aggregate value-set alternatives and
explicit URL-host case equivalence replace cached-value and predicted-ID grading.
Leaf removal and exact initial-value restoration permissions admit documented
ancillary API effects without granting arbitrary metadata rewrites. User
instructions remain unchanged; the frozen-client checkpoint is recorded separately.
Revision 24 migrates three cross-system writes. Exact decimal record and causal
predicates replace request-body amount substrings; actual note/discount identities,
customer/source relationships, conserved refund accounting and committed history
replace fixed generated identities and reference-transport requirements.
These are bounded local contracts, not proof of information acquisition or general
semantic correctness.
Revision 23 migrates the Notion writes, reads complete structured rich-text values,
adds exact array cardinality and explicit reviewed incorrect text examples, and
distinguishes unchanged wording from newly committed claims after passive reformatting.
Revision 22 migrates GitLab writes, adds exact record-key relationships and
nonempty-text/array-membership causal predicates, and repairs note resolution and
actual squash-commit materialization.
Revision 21 migrates GitHub writes, binds approval/merge outcomes to actual
reviews/commits, and adds explicitly optional narration without making supplied
claims or required descriptions optional.
Revision 20 migrates the Linear/Slack writes and adds bounded identity-scoped
array append permissions without granting updates to preexisting rows.
Revision 19 adds identity-backed whole-text examples and closes the cross-service
refund's identifier-mention false positive. Conservation and bijections admit
legitimate split refunds without prescribing a single API call.
Revision 18 extends declared content checks to authoritative committed history,
so restoring a positive final record cannot erase earlier unreviewed claims.
Revision 17 migrates the three Jira writes, adds complete reviewed multi-field
content projections, and repairs atomic validation of modeled Jira writes.
Revision 16 migrates the three Zendesk write workflows, adds declared
reviewed HTML representations and explicit permission for coincident atomic
outcomes. Strict prior-order checks retain their original semantics.
Revision 15 binds evidence schema 5 to world-loaded contract/data sources and adds
protected local scripted execution manifests and source-compatible comparison
admission. Native/model runtime comparison remains unsupported; see
[execution provenance](execution-provenance.md).
Revision 14 adds genuine immediate Shopify publication on the composed client
path, identity-based nested array checks, bounded array-element permissions and
unique returned-row captures. Activation alone remains insufficient.
Revision 13 adds exact scoped numeric preservation, additive string-set guards,
plain literal content and tag identity checks, nullable choices, and declared
environment faults. It removes false Shopify publication signals; that workflow
is not admitted until genuine publication support exists.
Revision 12 migrates the Salesforce writes and adds conservative reviewed-text
examples, nullable/optional record fields and complete field alternatives.
Revision 11 adds controller-gated post-stop main snapshots and exact legacy-answer
byte binding under `tool-bench-runner-v2`; it does not authenticate native replies.
Revision 10 adds bounded rich-text equivalence and explicit unresolved assessments.
Revision 9 adds many-to-one foreign-key checks and reviewed HubSpot outcome contracts.
Revision 8 adds required runner admission to generated offline verification.
Revision 7 added bounded record counts, exact integer totals, and identity-based
bijections with joined-field checks. Revisions 4–6 introduced offline finalization,
request receipts/replay context, and fixed business time, described below.
Revision 3 added record relationships and audited mutation histories, including
integrity evidence for requests that never finish. Revision 2 introduced
interface-integrity and required-response semantics.
Reproducing historical scores requires their original pinned evaluator and images,
not silently applying the current evaluator.

## Contract versions

Existing expectations without `version` are version 1 contracts. They retain their
declared state, call, and answer predicates, but **do not establish preservation of
unrelated business state** unless a preservation policy is supplied.

Version 2 contracts preserve all business state by default:

```toml
[expect]
version = 2

[expect.answer]
contains = ["Northwind Logistics"]
regex = '412,?000'
```

All 50 lookup-tagged scenarios currently use version 2 with `audit_history = true`.
Write scenarios are being
migrated individually: they must declare authorized field and record changes instead
of receiving a broad exemption.

The verifier supplies the world's authoritative initial snapshot. A preservation
check without that snapshot produces `environment_error`, not a passing check.
Snapshot comparison excludes the world clock and scenario metadata and disregards
empty lazily allocated collections. It does not globally remove resource timestamps,
business fields, or empty fields within records.

### Service setup and client sessions

Service initialization finishes before listeners become available. Its normalized
starting state is captured once and restored on reset; initialization cannot rewrite
the baseline after requests have begun.

Client-local state lives in a separate `sessions` store, not in business `services`.
For MongoDB, connection handles, initialization bookkeeping, and export handles are
client state. Querying or connecting must not be mistaken for a database mutation.
MongoDB namespace/index definitions and every `db.<database>.<collection>` remain
business data, including database collections named `connections` or `_meta`.
Firecrawl search history and browser scrape handles likewise use session state;
its persistent crawls, monitors, monitor checks, and account records remain business data.

This separation is defined in service code, not a scenario-specific ignore list.
Full snapshots retain session state for diagnostics and explicit state checks.

## Authorized changes

Existing records may change only specified fields:

```toml
[expect]
version = 2

[[expect.state]]
pointer = "/services/github/issues/octo-org~1payments-api#12/state"
equals = "closed"

[[expect.preservation.allow]]
pointer = "/services/github/issues/octo-org~1payments-api#12/state"
kind = "update"
```

Pointers use RFC 6901 escaping: `/` inside a record key becomes `~1`, and `~`
becomes `~0`. Rules apply to the named field and its descendants, not sibling fields.
An update rule cannot authorize an entire existing record.

New records and deletions require an explicit kind. For generated IDs, use a
record-key wildcard with a nonempty matcher and a cardinality limit:

```toml
[[expect.preservation.allow]]
pointer = "/services/github/issue_comments/*"
kind = "create"
matching = { issue_number = 12 }
max_count = 1
```

This example describes the contract shape, not a guarantee that every service stores
comments under those exact fields. Select the actual collection and record fields
used by the mock being authored.

An allowed change can require exact numeric record fields without prescribing
their textual spelling: `numeric_fields = { "/value" = 4 }` admits `"4.000"`
and `"0.4e1"`, not `99`, Booleans, approximate near-misses or unrelated metadata.
These predicates apply at each audited commit, not only to the final record.
They do not themselves authorize creation, deletion or a competing string
pattern on the same field.

Only record keys may be wildcarded; blanket service/collection wildcards are rejected.
`max_count` defaults to one and counts distinct affected records, not changed leaves.
A create rule cannot authorize modifications to a matching preexisting record.
Matchers inspect the final record for creates/updates and the original record for deletions.
“Preexisting” means present at trial start: a permitted new record may be refined
in later calls while its identity/invariants continue to match. Deleting it still
requires a deletion rule. This must not force one particular create/update sequence.
Required outcomes remain separate: permission to create a record does not establish
that its required content is correct.

Record selectors and preservation rules share `field_patterns`: a map of literal
record-field JSON Pointers to Rust regular expressions. Every declared pattern
and ordinary subset matcher must hold. A missing, null, array or nontext field
does not match. Invalid patterns, wildcard field paths and patterns matching the
empty string are rejected. These are evaluator predicates, not MongoDB/PCRE
query syntax or email-validation rules.

For a request to delete **every** member of an actual set, do not encode the
fixture's current record count as the permission limit:

```toml
[[expect.records]]
selector = { collection = "/services/mongodb/db.crm.customers", field_patterns = { "/email" = '(?i)@qa[.]octoshop[.]test\z' } }
equals = 0

[[expect.preservation.allow]]
pointer = "/services/mongodb/db.crm.customers/*"
kind = "delete"
field_patterns = { "/email" = '(?i)@qa[.]octoshop[.]test\z' }
max_count = "all_matching"
```

`all_matching` permits explicitly matched whole-record deletions, or field updates
with an explicit trial-start scope. It requires a record-key wildcard and trial
count scope. Deletion matchers continue to inspect the authoritative
**trial-start** record; an agent cannot reclassify another record into that set.
New records, survivor field edits and unrelated collections remain unauthorized.
Numeric limits keep their existing distinct-record and per-commit behavior.

### Related Records and Baseline Update Sets

A selector's `references` filters rows by actual selected target identities,
using the same typed string/integer identity semantics as field relationships.
All references, subset fields and string patterns must hold. For example:

```toml
[[expect.records]]
selector = { collection = "/services/sales/customers", matching = { email = "priya.shah@fastmail.com" } }
equals = 1

[[expect.records]]
selector = { collection = "/services/sales/orders", matching = { status = "pending" }, references = [{ source_field = "/customer_id", target = { collection = "/services/sales/customers", matching = { email = "priya.shah@fastmail.com" } }, target_field = "/id" }] }
equals = 0

[[expect.preservation.allow]]
pointer = "/services/sales/orders/*/status"
matching = { status = "cancelled" }
initial_matching = { status = "pending" }
initial_references = [{ source_field = "/customer_id", target = { collection = "/services/sales/customers", matching = { email = "priya.shah@fastmail.com" } }, target_field = "/id" }]
max_count = "all_matching"
```

The permission uses frozen initial records and initial lookup targets, while
`matching` checks the resulting record. Renumbering the actual customer and its
foreign keys does not change business intent. Editing the lookup later cannot
authorize a different baseline set. Existing paid rows, other customers, money
and all other fields remain protected. Repeating an already-satisfied assignment
does not require inventing a new mutation.

Reference filtering is not an existence/cardinality assertion: an empty target
set selects no sources. Declare the required target count separately, as above,
to prevent vacuous success. Duplicate selected target identities fail rather than
choosing the first. Field pointers are literal; duplicate references and nesting
beyond sixteen related-selection hops are invalid contracts.

`initial_matching`, when provided, must be a nonempty object; explicit null is
not silently ignored. Initial references cannot depend on newly created targets.
Initial scopes apply only to changes of existing records, not creation or array
append permissions. `all_matching` updates authorize fields, never an entire
existing record. Existing bounded and baseline-deletion policies are unchanged.

Final-state preservation alone cannot detect a modification subsequently undone.
`audit_history = true` requires an authoritative mutation history and checks every
committed transition, including changes later restored. A missing, inconsistent,
unattributed, or incomplete history is an environment fault, not evidence of success.

### Appending Records Within Existing Arrays

Some interfaces store list items inside an existing business record. Authorize
only new array identities rather than the entire mutable `records` array:

```toml
[[expect.preservation.allow]]
pointer = "/services/slack/lists/known-list/records"
kind = "append"
identity_field = "/id"
matching = { created_by = "current-user" }
max_count = 1
```

The pointer must be a literal existing array within one known business record,
with a nonempty new-row matcher and explicit field identity. IDs are typed
nonempty strings or integers, not positions; duplicate identities cannot be
resolved by taking the first row. No wildcard parent/array path, additive-set
permission, or per-commit cardinality escape is allowed.

Every committed transition retains the entire existing row prefix and its
identities. Preexisting rows remain byte-value equal, not merely similar on a
few positive fields. New rows may be refined in later calls while retaining
their identity, position and matcher invariants. Deletion, replacement, reordering
and changes outside the array remain unauthorized. `max_count` counts actual
new row identities across the trial, not the one outer list record.

Append contracts require authoritative mutation history. An old row changed and
restored cannot pass because the final array looks correct. Outcomes still
separately require the actual created row and its requested fields; permission
to append is not proof of a correct checklist item. Missing/non-array initial
structures are unsupported evidence, not silently interpreted as an empty list.
The raw state/evidence is unchanged; this is a comparison projection only.

### Derived side effects

Some legitimate operations create bookkeeping records, such as one Zendesk audit
per ticket update. Their number can vary with a valid plan or a harmless retry.
Do not hard-code the oracle's number of API calls as a business requirement.

For reviewed created side-effect records, `count_scope = "per_commit"` enforces
`max_count` within each atomic state commit rather than across the trial:

```toml
[[expect.preservation.allow]]
pointer = "/services/zendesk/audits/*"
kind = "create"
matching = { ticket_id = 5102 }
count_scope = "per_commit"
```

This requires audited history. Ordinary business cardinality, such as exactly one
new discount, remains trial-scoped. Per-commit counting is not allowed for update/delete
rules. Match the intended record precisely; a bookkeeping rule must not authorize
effects on unrelated tickets.

### Value and string-set preservation

JSON representation is not always business value. `equivalent_numbers` explicitly
identifies numeric fields whose exact decimal value is preserved across number
and decimal-string representations:

```toml
[expect.preservation]
equivalent_numbers = ["/services/salesforce/Opportunity/known-id/Amount"]
```

This is not permission to change the amount. The comparison uses decimal digits
and an exponent, not floating-point conversion or an epsilon; large integers and
tiny real differences remain distinct. It never applies globally to identities,
booleans or undeclared fields. Missing/null values are not zero. Raw state and
mutation evidence are retained unchanged; only the preservation comparison uses
the declared equivalence. Audited history still rejects real changes even if
subsequently restored.

For an actual string-set addition, authorize only the requested values:

```toml
[[expect.preservation.allow]]
pointer = "/services/shopify/customers/known-id/tags"
add_set_values = ["VIP"]
ascii_case_insensitive = true
```

This permits set reordering, duplicate collapse and declared ASCII case
equivalence, but cannot remove existing members or add anything else. It is
checked on every committed transition, not just the final state. Initial missing
or null sets represent no members; other non-array/non-string shapes are not
silently parsed. Matchers, record cardinality and ordinary field scope still apply.
An explicit empty list means reorder-only, not unrestricted permission. These
guards require update rules; they cannot authorize record creation/deletion.
Undeclared arrays retain their ordinary ordered JSON semantics.

## Record outcomes and relationships

Record checks select by stored business identity rather than a predicted generated
ID. They require an explicit count predicate and may assert exact typed field values:

```toml
[[expect.records]]
selector = { collection = "/services/stripe/refunds", matching = { charge = "ch_duplicate" }, created = true }
equals = 1
fields = { "/amount" = 48000, "/reason" = "duplicate", "/status" = "succeeded" }
```

`created = true` requires initial-state evidence and excludes records already present
at trial start. `key_prefix` supports stores whose parent identity is encoded in the
key, such as Zendesk's `"<ticket>/<comment-position>"` comments.

For a nested array of business records, use its literal path and an explicit
`identity_field`:

```toml
[[expect.records]]
selector = { collection = "/services/shopify/products/known-product/variants", identity_field = "/id", matching = { selectedOptions = [{ name = "Color", value = "Navy" }] } }
equals = 1
fields = { "/price" = "42.00" }
```

Array positions are not identities. Selection, created-item exclusion and causal
transitions use typed element identities and reject duplicate identities. A
preservation rule can authorize a specific nested field through one array
wildcard only with a nonempty `element_matching` constraint:

```toml
[[expect.preservation.allow]]
pointer = "/services/shopify/products/known-product/variants/*/price"
element_matching = { selectedOptions = [{ name = "Color", value = "Navy" }] }
```

Both the before and after element must match, and the location must actually be
an array. This does not authorize other fields, element replacement, insertion,
deletion or array reordering. `max_count` still counts affected outer business
records; use identity/count predicates to establish unique intended elements.

`element_field_patterns` can identify array elements by literal field pointers,
including array-row cells such as `/0` and `/1`. Both versions must satisfy all
patterns; changing an identity cannot move an unrelated row into the allowed set.
An append rule's `field_patterns` applies to each new row, not the containing
record. `value_equals` restricts the exact post-value of a permitted leaf update,
including intermediate commits; it is not a wildcard exemption for the row.

Use either `equals` or a range with `min_count` and/or `max_count`, never both.
An unbounded minimum of zero and reversed bounds are invalid contracts.
`optional_fields` permits a missing/null field or its exact declared value; it
does not prove presence. `null_fields` instead requires explicit JSON null.
`optional_choices` permits missing/null or one of a nonempty, distinct list of
explicit values. An empty string is not an implicit alias: include it deliberately
when it is a valid unassigned representation. A separate positive predicate must
still establish required classification or presence.
`any_fields` requires at least one complete, nonempty field map to match, not
independent words or a mixture of different alternatives. These supplement
`fields`; they do not replace required record counts or audited preservation.

`array_lengths` requires an actual array of exactly the declared length at each
record-relative pointer. Missing/null values, objects and strings do not count as
arrays. Combine cardinality with identity checks: a people field containing the
right owner and an extra owner must not satisfy a request for one owner.

### Google dates and organizer-side meetings

`sheet_date_fields` maps a record-relative cell pointer to a canonical ISO date.
Reviewed equivalent representations include typed exact numeric Sheets serials
with epoch **1899-12-30**, ISO text and the explicitly supported unambiguous
slash-date forms. Numeric-looking RAW strings and unevaluated formula text do
not become dates merely by resembling the expected serial or expression.
Unreviewed spellings require adjudication. Audited history permits blank staging
but rejects a committed wrong date even when the final value is restored.

`calendar_meeting` requires the complete created-event collection, exact title,
calendar and nanosecond instants, a confirmed non-recurring event, the complete
case-equivalent guest set and one usable Google Meet URI backed by a successful
modeled provider resource. A known URI in `location` is a valid representation;
redundant convenience fields are not mandatory. A provider type label or guessed
URL is not provisioning evidence. Optional unreviewed description wording
requires adjudication rather than substring-based success.

Calendar goals cannot hide other new events through filters, `latest_by` or an
alternative identity field, or shadow owned fields through reviewed projections
and raw predicates. Date and meeting goals require audited version-2 contracts.
Final required outcomes remain separate from permissions for safe staged writes.

The authored planning task requests an organizer-side event and attendees, not
email delivery. `all`, `externalOnly` and `none` notification modes do not by
themselves change its completion predicate. A create-then-silent-Meet-update plan
is permitted; the ready URI need not appear in an earlier notification request.
Google's synchronization warnings and guest-side visibility remain unqualified.
`calendar_notification_requests` records the selected mode and organizer's
attendee snapshot, not recipient enrollment, successful sending, receipt or
acceptance. `externalOnly` must not be expanded into recipients by email domain.
The [notification research](research/2026-11-07-google-invitation-notification-contract.md)
records the documented distinction and remaining uncertainty.

The local Calendar model uses IANA zones and distinguishes submitted native
offset/zone conversion from stored display zones. Only the former reproduces
the pinned Python helper's microsecond truncation. Unknown civil-time ambiguity
or unsupported precision is a pre-mutation capability fault, not guessed UTC.
This bounded Rust replica is not execution of the upstream Python server/SDK or
proof of hosted precision. Source checks alone do not admit runtime; the
separately retained corrected 40B campaign qualifies only its declared scripted
selected-replica condition.

### Aggregate and structured predicates

`totals` sums integer fields across all selected records; `integer_bounds` checks
each record independently. Missing fields, strings, and floating-point JSON numbers
fail integer checks. Totals use checked wide-integer arithmetic rather than floating
point. For example, a full refund can consist of one or several positive partial
refunds without requiring the oracle's call count:

```toml
[[expect.records]]
selector = { collection = "/services/stripe/refunds", matching = { charge = "ch_duplicate" }, created = true }
min_count = 1
max_count = 48000
fields = { "/currency" = "usd", "/reason" = "duplicate", "/status" = "succeeded" }
totals = { "/amount" = 48000 }
integer_bounds = { "/amount" = { min = 1, max = 48000 } }
```

The maximum here follows from a 48,000-cent total and a one-cent minimum, not a
preferred tool sequence. Preservation rules must independently authorize the
matching created records and their derived effects.

Relationships of kind `equals` or `contains_reference` select exactly one source
and one target. `equals` compares typed
field values; `contains_reference` checks a complete identifier in a text field.
Missing or ambiguous selections fail instead of picking the first matching row.
For example, bind a ticket's public reply to the actual newly created refund ID,
and bind the refund to its actual balance transaction in both directions.

`numeric_equals` compares exact decimal values and intentionally accepts numeric
text where a service represents quantities that way. It does **not** establish
that a field is a number. Use `bson_numeric_equals` when BSON numeric type is part
of the requirement. This mode accepts JSON numbers and single-key canonical
`$numberInt`, `$numberLong`, `$numberDouble` and finite representable
`$numberDecimal` wrappers, rejecting plain strings and ordinary documents.
Integer wrappers enforce their signed width; doubles are decoded as finite
binary64 values before exact decimal comparison; Decimal128 inputs enforce
precision and range without routing through a float. This is a bounded
numeric-value predicate, not a general BSON engine or guarantee of cross-type
MongoDB expression/comparison parity. Both selectors must identify exactly one
record. Preserve the original total independently when relating a refund amount
to that total; equality alone must not authorize rewriting the total.

`bijection` instead joins complete selected sets by their identity fields. Both
sets must be nonempty, have unique identities, and have exactly matching identity
sets. Identities must be nonempty strings or integers; a string and an integer
with the same printed value remain different identities. Store keys and ordering
do not determine the join.

Optional `fields` on a bijection check each joined pair with typed `equals` or
`negated_integer` predicates:

```toml
[[expect.relations]]
source = { collection = "/services/stripe/refunds", matching = { charge = "ch_duplicate" }, created = true }
source_field = "/balance_transaction"
target = { collection = "/services/stripe/balance_transactions", matching = { type = "refund" }, created = true }
target_field = "/id"
kind = "bijection"
fields = [
  { source_field = "/amount", target_field = "/amount", kind = "negated_integer" },
  { source_field = "/currency", target_field = "/currency", kind = "equals" },
]
```

This rejects ledger amounts attached to the wrong refund even when aggregate
totals balance. Joined-field predicates require a bijection; empty pointers and
duplicate predicates are invalid.

`foreign_key` requires every selected source field to reference an actual selected
target identity. Multiple sources may reference the same target; selected target
identities must be unique and the target selection must be nonempty. String and
integer identities remain distinct. Missing, empty or dangling source identities fail.

```toml
[[expect.relations]]
source = { collection = "/services/hubspot/associations", matching = { toObjectType = "notes" }, created = true }
source_field = "/toObjectId"
target = { collection = "/services/hubspot/notes", created = true }
target_field = "/id"
kind = "foreign_key"
```

This can bind a deal link and optional related contact/company links to one actual
new note, without predicting its ID or forcing those optional links to exist.
An empty source selection is permitted; required business cardinality remains an
explicit record outcome. The HubSpot task separately requires exactly one new note
and exactly one link from the requested deal. Joined-field predicates remain
restricted to bijections rather than being silently ignored.

Revision 22 adds `key_reference` for a field that contains an actual object-record
key rather than a separate database ID:

```toml
[[expect.relations]]
source = { collection = "/services/gitlab/notes", matching = { system = true }, created = true }
source_field = "/noteable"
target = { collection = "/services/gitlab/issues", key_prefix = "northwind/payments/checkout-api#", created = true }
kind = "key_reference"
```

Every source must contain a nonempty string exactly equal to a selected target
key. It does not predict an issue number, confuse a database ID with an IID, decode
JSON-pointer spelling, or accept a prefix/substring/mention. Multiple sources may
reference one target. Sources may be empty, but selected target keys must be
known and nonempty. This mode requires a top-level object-record collection;
array identities, `target_field`, and joined-field predicates are rejected rather
than silently ignored. Existing field-based relationships still require their
explicit nonempty `target_field`.

New Zendesk comments carry an internal `ticket_id`, derived by the write operation,
in addition to their existing parent-encoded store key. This field is not added to
the rendered comment interface. Contracts can bind the comment to an actual newly
created ticket and its audit, or constrain permissions to a known ticket, rather
than permitting any public/private comment anywhere. Existing fixture comments
are not rewritten during a request.

### Exact content in rich-text records

`fields` compares serialized values exactly. For a user-provided note whose content
may be stored as ordinary HTML formatting, use `html_text` explicitly:

```toml
[[expect.records]]
selector = { collection = "/services/hubspot/notes", created = true }
equals = 1
fields = { "/archived" = false }
html_text = { "/properties/hs_note_body" = "Review passed; send the SOW by Nov 14." }
```

The checker tokenizes the original source with locked `html5ever` code; it does not
strip tags with a regex or trust a repaired DOM. Supported fragments are plain text
and balanced, attribute-free `p`, `div`, `span`, `b`, `strong`, `i`, `em`, `u`, and
`br` formatting. Paragraph/division/break boundaries and decoded whitespace,
including nonbreaking spaces, are canonically folded to ordinary spaces. This
equivalence is a declared content contract, not browser-exact whitespace or CSS
rendering. Inline adjacency is preserved; comments do not supply requested text.

Attributes, active/hidden/head/template/foreign/unknown markup, unsupported error
recovery, directional/invisible controls, and resource-limit cases require
adjudication. They cannot establish a pass merely because extracted words match.
Limits are 1 MiB of source, 100,000 tokens, and 128 open formatting elements.
Supported but different text, wrong types, wrong quantities, and wrong relationships
remain ordinary proven failures. Known failures are not hidden by another field's
unsupported rendering.

### Reviewed whole-text examples

Free-form factual requirements are not literal-copy requests. A finite collection
of author-reviewed positive examples can establish only those complete texts,
with declared Unicode-whitespace folding:

```toml
[[expect.records]]
selector = { collection = "/services/salesforce/Task", created = true }
equals = 1
reviewed_text = { "/Description" = { requirement = "Dana requested a three-year renewal quote by May 22, 2026.", accepted = ["Dana requested a three-year renewal quote by May 22, 2026.", "Dana wants a 3-year renewal quote by May 22."] } }
```

`requirement` is the adjudication criterion, not a keyword matcher. Examples must
be nonempty, distinct after whitespace folding, and independently justified from
the user request and task context before model experiments. A candidate must match
an entire example; fragments, a quoted example inside a contradictory narrative,
or the presence of words such as `quote` cannot establish success. This checker
does not strip HTML or infer facts from arbitrary natural language.

Missing, non-string or empty required text is a proven failure. Other nonempty
wording—including valid unlisted paraphrases and apparent contradictions—requires
adjudication, not an invented model-failure verdict. This finite positive path is
**not** a general semantic judge, a trusted human-review receipt, or permission
to prefer the oracle's wording in final scores. Until the prespecified trusted
adjudication workflow is implemented, affected comparisons remain unresolved.
Do not add newly observed model outputs to the examples to repair a measured run.

Revision 21 adds explicit `optional = true` for narration the user did not require,
such as the body accompanying an approval review. Missing, null or whitespace-only
text then needs no positive example. Supplied nonempty text still requires the
same complete-example or adjudication path, and committed-history admission still
checks earlier claims even if the body is later emptied or restored. Nontext values
remain a known representation failure; `optional` is not an unrestricted-content
permission. Nonempty criteria/examples remain mandatory when authoring the check.
The default remains required, and the default false flag is omitted from serialized
contracts so existing default contract identities do not change merely from adding
this option. Required descriptions must not be made optional to repair an oracle.

`format` defaults to `plain`; markup is not implicitly stripped. Explicit
`format = "html"` compares passive rich-text content against the same complete
plain-text examples, using the bounded formatting grammar above. Unsupported
rendering or unlisted nonempty content requires adjudication; empty supported
content is a proven failure. This does not make factual prose a literal-copy task.

`format = "zendesk_html"` additionally recognizes exactly one outer
`<div class="zd-comment" dir="auto">...</div>` envelope emitted by the local
Zendesk plain-comment renderer. Only that exact envelope is removed before
the inner source passes the ordinary passive grammar. Added attributes, hidden
inner content, malformed envelopes, and arbitrary classes remain unresolved.
Ordinary `html` checks do not acquire this exception. Zendesk communication checks
validate both the stored plain body and the rendered body; the mock's simple
HTML-to-plain conversion alone cannot establish visible factual correctness.

For explicitly quoted plain content, a state check can use `text_equals`. It folds
Unicode whitespace and otherwise requires the complete requested text; it does
not strip markup, accept a substring, or turn negated instructions into success.
`member_ascii_ci` instead checks a complete string member of an array using ASCII
case equivalence, not substring search. These are distinct from general factual
adjudication.

### Identity-backed whole-text examples

An actual identifier in prose proves reference, not truthful communication.
For a required generated reference, `bound` replaces static `accepted` examples:

```toml
[expect.records.reviewed_text."/body"]
requirement = "Truthfully say the duplicate payment was refunded and include an actual Stripe refund reference."

[expect.records.reviewed_text."/body".bound]
examples = ["Your duplicate payment was refunded. Stripe refund references: ${refund_ids}."]

[expect.records.reviewed_text."/body".bound.identities.refund_ids]
selector = { collection = "/services/stripe/refunds", matching = { charge = "ch_duplicate" }, created = true }
field = "/id"
separator = ", "
```

The source selector and field read authoritative business state, not an oracle
capture, agent log or answer file. `created = true` requires initial-state evidence.
Sources contain nonempty whitespace-free strings or JSON integers; floats,
booleans and objects are not coerced into identities. Repeated observations of
the same typed identity collapse to one member; different typed identities with
the same printed form require adjudication. Directional/invisible source controls
cannot establish visible truth.

Without `separator`, a source must resolve to one unique identity; ambiguity
requires review, not picking the first row. With `separator`, the positive path
represents the complete identity set and accepts member reordering. Delimiters
must preserve identity boundaries; an identity containing its delimiter requires
review. No factorial permutation enumeration or arbitrary expression evaluation
is used.

Each example must be a complete author-reviewed positive statement using exactly
the declared `${name}` sources with complete identity boundaries. Unknown,
missing or unused names, malformed slots, duplicate/empty examples and mixing
static/bound modes are invalid contracts. Values remain opaque literal data:
an identity containing `${...}` is never evaluated recursively. Candidate HTML
is normalized only through the explicitly declared passive representation; source
values do not become markup, queries or code.

Every required identity group must contribute at least one actual complete
reference. A wrong/old/prefix-only identifier cannot meet that obligation.
Merely mentioning an actual ID, denying the action, quoting a positive message
inside contradictory text, or using another unlisted narrative cannot establish
success. Such nonempty wording with a valid reference requires adjudication,
not an invented model-failure verdict. A valid split-refund response mentioning
only one of several IDs likewise remains reviewable rather than being failed
solely for not using the positive path's complete list.

Final content uses final business state. Committed content resolves identities
against that commit's state, so a later source change cannot retroactively approve
an earlier claim. Empty intermediate construction still does not require the
reference source to exist before any claim is written. Physical outcome, allowed
effects, accounting and causal checks remain independent required gates.

These are bounded positive examples, not general semantic adjudication. Greetings,
signatures, repeated prices/order numbers and ancillary narration must not become
hidden requirements when the user did not request them.

### Reviewed multi-field content

User facts can be distributed across an issue's summary and description without
requiring the oracle's exact title. `reviewed_fields` declares complete positive
projections over a fixed set of plain-text fields:

```toml
[expect.records.reviewed_fields]
requirement = "Report that Apple Pay is missing at checkout on iOS 18 Safari, without contradictory or invented claims."
accepted = [
  { "/fields/summary" = "Apple Pay missing on iOS 18 Safari checkout", "/fields/description" = "" },
  { "/fields/summary" = "Apple Pay missing", "/fields/description" = "The button is absent at checkout on iOS 18 Safari." },
]
```

Every example must cover the same nonempty set of valid field pointers, contain
some nonempty text, and be distinct after Unicode-whitespace folding. Each
candidate must match a whole projection, not independent pieces of different
examples. An accepted summary does not excuse an unreviewed or contradictory
description. Other predicates may not also constrain these fields.

Within this explicitly declared content projection, missing/null/whitespace-only
fields mean absent text and match an author's empty-string example. This is not
global JSON equivalence or permission to omit structural requirements. Non-text
values and an entirely empty projection are proven failures. Other nonempty
projections require adjudication; unknown paraphrases are not automatically
model failures. Markup is not stripped. This remains a finite positive path,
not a general factual judge or permission to extend examples after measurement.

### Structured Notion text and reviewed incorrect examples

Revision 23 adds `format = "notion_rich_text"`. The field is an array, not a
string or segment-zero shortcut. The bounded projection concatenates every
supported text segment's `text.content` in order, without invented separators,
then folds whitespace. A supplied `plain_text` must agree with its content.
Passive bold/italic/underline/code and recognized Notion colors are supported.

Links, strikethrough, mentions, equations, invisible/directional controls, unknown
metadata and inconsistent shapes require adjudication. These are checker limits,
not claims that Notion forbids such content. In particular, neither a link label
nor withdrawn characters alone establish the requested factual content.
The underlying structured value remains subject to ordinary preservation.

For static reviewed text, `rejected` may list exact, whole-text incorrect
examples. They must be nonempty, normalized-distinct and disjoint from `accepted`.
They cannot be combined with source-bound templates. A known incorrect example
is a proven failure, including when committed and later restored; other unknown
wording remains unresolved. This is not keyword rejection or a general negation
classifier. For example, the unchanged old escalation contact is an explicitly
incorrect final result for the frozen replacement task.

### Committed content admission

When mutation history is required (`audit_history` or ordering checks), revision 18
also reviews changes to declared `reviewed_text`, `html_text`, `reviewed_fields`
and literal business-state `text_equals` content. Restoring an accepted final
message cannot erase an earlier unreviewed committed claim.

Record targets come from the final selector's actual identities. Their earlier
content is checked even before intermediate metadata matches the final selector.
Nested array records retain their explicit typed identities. Final cardinality,
structural fields, totals and relationships are not reapplied to every incomplete
construction step. Empty/missing content and supported empty HTML do not force
one create call; final presence requirements remain separate.

Nonempty earlier wording or an unsupported representation that the declared
positive path cannot establish requires adjudication, not an invented ordinary
model failure. This also applies to earlier nonmatching literal text: a final
literal mismatch can be proven wrong, but an intermediate draft's acceptability
cannot be guessed merely from different serialized content. Harmless approved
formatting and metadata-only refinements remain valid. Proven final structural
violations retain their ordinary failure disposition.
For reviewed text, an unchanged supported text projection is not a new claim
merely because passive serialization/formatting changed. This does not erase a
real intervening contradiction or bypass structural preservation.

The checker observes protected lock-release commits during authoritative replay,
not agent-written logs. Historical content failures retain their causal commit
sequence in `event_evidence`. If history is incomplete or inconsistent, provisional
content observations are discarded and the assessment is an environment fault.
`records_ok` and `state_ok` include their declared historical content checks;
`history_ok` separately describes replay/preservation/ordering, not unrestricted
semantic understanding.

This covers declared content fields of final-selected records and explicit
business-field literal checks, not every unselected/deleted record, arbitrary
wire text, or trusted native final replies. Contracts without required history
remain final-only. A finite positive path is still not general factual
adjudication, authenticated human review or a prespecified replacement policy.

### Salesforce outcome scope

The three reviewed single-service writes preserve all unrelated records and fields
and audit restored changes. Case closing permits the requested status/reason and
the mock's actual read-only closure/system fields. Opportunity closing preserves
the actual amount and other business fields, authorizing standard close-won
probability/forecast changes without forcing those defaults onto an already-won
record that has unchanged custom forecasting. Arbitrary new overrides are not
authorized. Split updates, equivalent IDs, harmless retries and already-satisfied
outcomes do not require an oracle-shaped call sequence.

Call logging checks one actual created Task rather than a predicted next ID.
It accepts a Call subtype or a Task with Type Call, with no contradictory type,
and permits an optional coherent account link. Unspecified owner and priority
choices are not hidden literal requirements. Call duration/direction/disposition
cannot be invented from an instruction that provides none; unrelated links and
edits to existing tasks are not requested. Contact/account
activity-date effects are permitted only on the intended identities. Already later
activity dates are not forced backwards.

The stored note uses reviewed examples and honest unresolved dispositions rather
than treating `Description contains "quote"` as factual proof. User instructions
are unchanged; the task does not request one of the checker's example spellings.
The field distinction is grounded in the official
[Salesforce Task object reference](https://developer.salesforce.com/docs/atlas.en-us.object_reference.meta/object_reference/sforce_api_objects_task.htm):
Type and TaskSubtype both classify activities, TaskSubtype is create-only,
AccountId is derived from relationships, and IsClosed derives from status.
This is a reviewed mock contract, not a claim of complete Salesforce compatibility.

### Unresolved assessments

An undecidable check has `disposition = "unresolved"` in `failures` and sets
`adjudication_needed = true`. Proven violations have `disposition = "failed"`.
Both prevent reward 1. An assessment containing only undecidable checks has
`outcome = "unresolved"` and code 6; its compatibility reward of zero is
**provisional, not a final model-failure judgment**. Independent proven failures
retain their appropriate failure/integrity/environment/contract outcome even when
another check remains unresolved.

`bench-report` refuses a final paired comparison containing unresolved outcomes;
it does not remove those attempts from the denominator or turn them into measured
model failures. Empty/disjoint runs are invalid comparisons as well. A trusted,
prespecified adjudication workflow and final native-answer semantics remain
separate release work; do not edit reward files or rerun selectively to make
uncertainty disappear. External consumers must support these distinctions before
this evaluator revision is admitted there.

**A reference in text does not prove the text makes a truthful assertion.** These
checks establish identity and referential integrity, not general natural-language
semantics. Communication/answer adjudication remains a separate migration.

Use literal field constraints only when they reflect the user request or a visible
standing policy. Do not turn an optional annotation or a preferred wording into a
hidden requirement.

### Declared unsupported environments

A known setup/capability defect is declared in the trusted contract, not inferred
from agent-written logs:

```toml
[[expect.environment_faults]]
code = "missing_capability"
reason = "The required operation is not supported on the configured execution path."
```

Codes must be distinct and meaningful, with nonempty reasons. A valid declaration
keeps `contract_ok` true but makes `environment_ok` false and the overall result
`environment_error`, even if some legacy business checks happen to pass. It
cannot become reward 1 or an ordinary model-failure label. This is a setup
diagnostic, not a completed task or permission to drop difficult tasks.

Revision 13 used this for the unvalidated Shopify publication workflow. Revision
14 implements a composed publication-capable client and real publication state;
the gate is retired only for that validated configured condition. The stock
fourteen-tool client alone still lacks the capability. See
[operation support](operation-support.md).

Final paired reporting refuses known invalid environments/contracts, missing or
unknown assessments, reported host trial exceptions, inconsistent binary
reward/outcome pairs, and nonfinite rewards, as well as unresolved outcomes.
It does not silently turn those attempts into zeros or remove them from the
denominator. Full realized-manifest and cross-condition compatibility admission,
complete inference classification, and prespecified replacement policy are still
separate remaining work.

## Encoded JSON and complete plan alternatives

`records[].json_fields` checks a selected record's string as a complete decoded
JSON value. It does not trust a separately cached parsed field or a substring.
Object ordering and insignificant whitespace do not matter; arrays retain order
and cardinality. Numeric representations compare by exact decimal value without
coercing strings to numbers. Duplicate decoded object members, trailing input,
malformed values and numbers that would lose precision during parsing cannot
produce credit. This is a bounded evaluator policy, not a claim that a service's
byte-storage API rejects such input.

`preservation.json_strings` declares distinct, nonoverlapping literal business
field pointers whose encoded JSON is projected before preservation comparison.
Authorize only the desired decoded components:

```toml
[expect.preservation]
json_strings = ["/services/configuration/values/target/value"]

[[expect.preservation.allow]]
pointer = "/services/configuration/values/target/value/enabled"

[[expect.preservation.allow]]
pointer = "/services/configuration/values/target/value/rollout"
```

Initial encodings must exist and be unambiguous, supported JSON; otherwise the
environment lacks the required baseline evidence. A later invalid encoding is
a preservation violation, not a fabricated environmental escape. The projection
also runs on each authoritative commit: restoring a positive final object cannot
erase an earlier unauthorized component change.

`records[].value_sets` lists alternative complete, distinct field-value sets
across the selected actual records. Order is irrelevant, but missing/extra
members and duplicate contributions are not silently ignored. It can express
one unschemed route **or** both protocol-specific routes without prescribing
one reference call count. `url_host_fields = ["/pattern"]` explicitly folds only
the ASCII host component for that declared value-set comparison. It does not
fold path case, strip query/fragment data, rewrite stored state or implement a
general URL/Cloudflare routing engine.

Creation permissions can use the same `url_host_fields` declaration for their
explicit record matcher. This is restricted to created records; existing
resource mutations do not gain an automatic normalization exception. The
reviewed Cloudflare contract applies it only to newly created routes, avoiding
an assumed equivalence for historical host-case-sensitive routes.

`kind = "remove_field"` permits only removing the declared leaf from an existing
record, not rewriting it or deleting the record. `kind = "restore_field"` permits
only returning that leaf to its authoritative initial value, not a matching
object subset with additional fields. Both remain identity/field scoped and
participate in committed-history auditing. These permissions let an author admit
a documented ancillary reset without granting arbitrary metadata changes.

## Causal preconditions

Ordering checks require a data precondition to hold when an observed action commits:

```toml
[[expect.ordering]]
prerequisite = { selector = { collection = "/services/stripe/charges", matching = { id = "ch_duplicate" } }, field = "/amount_refunded", equals = 48000 }
action = { selector = { collection = "/services/zendesk/tickets", matching = { id = 5102 } }, field = "/status", equals = "solved" }
```

The prerequisite must still hold at that moment. A later remediation cannot repair
a premature action, and a prerequisite that was temporarily true does not count if
it was undone before the action.

Each effect check requires exactly one predicate: typed `equals`, exact numeric
or quantity equality, exact array membership through `has_value`,
`nonempty_text = true`, or selected `record_count`. Array membership never
matches a string or substring. Nonempty text means a string with non-whitespace
content; it is not evidence that the text is true. GitLab reply ordering combines
that prerequisite with separately reviewed whole-message content and committed
history, rather than forcing one literal reply. Approval ordering uses
`has_value = "sam.okafor"` so other legitimate approvals remain valid. Missing,
conflicting or false nonempty-text predicates fail authoring. Explicit null
equality retains its prior meaning and serialization.

Deletion prerequisites use an explicit count instead of a field on a record that
no longer exists:

```toml
[[expect.ordering]]
prerequisite = { selector = { collection = "/services/linear/team_memberships", matching = { userId = "departing-user" } }, record_count = 0 }
action = { selector = { collection = "/services/slack/posted", created = true }, field = "/text", nonempty_text = true }
```

A count predicate cannot also declare `field` or another predicate. The selected
scope must be empty before every announcement, not merely in the final snapshot.
An unrelated user's membership does not affect the scoped count. A missing or
malformed authoritative collection is missing evidence, not proof of absence.

A whole-record `calendar_meeting` prerequisite reuses the meeting goal rather
than checking only event existence or a convenient status:

```toml
[[expect.ordering]]
prerequisite = { selector = { collection = "/services/google/calendar_events", created = true }, calendar_meeting = { calendar_id = "ops@example.test", summary = "Cutover", start = "2026-11-08T04:00:00Z", end = "2026-11-08T07:00:00Z", attendees = ["sam@example.test", "priya@example.test"], require_meet = false } }
action = { selector = { collection = "/services/slack/posted", created = true }, field = "/text", nonempty_text = true }
```

The exact instants, calendar, title, confirmed/non-recurring status and complete
case-equivalent guest identities must hold when the notice commits. A later
correction cannot repair a premature notice. `require_meet` defaults to true for
existing meeting contracts; false makes conferencing optional, not unverified.
Any supplied video URI still needs a genuine modeled provider resource, and its
creation requires a separately scoped preservation permission.

An already satisfied prerequisite is sufficient unless `require_transition = true`
is explicitly requested. Do not force an unnecessary write or a preferred tool plan.
When the prerequisite and action share one atomic commit and the prerequisite was
not previously satisfied, the observer cannot establish their internal order; this
is insufficient environment evidence, not a fabricated ordering verdict.

An author may explicitly set `allow_same_commit = true` only when a coincident
atomic outcome is an acceptable business alternative. For the reviewed Zendesk
reply-and-solve workflow, a single ticket update that creates the public reply and
solves the ticket is acceptable: no committed solved state precedes that reply.
This proves coincidence, not an unobserved internal order. A previously committed
premature solve still fails even if the ticket is reopened and solved afterward.
`require_transition` still requires an observed new prerequisite, in an earlier
commit or the permitted coincident commit. The default remains strict: do not
enable this option merely to turn missing evidence of genuine prior causality
into a pass.

The world records backend request starts/completions and record changes at actual
lock-release commits. These events preserve mutation order even when concurrent
HTTP responses complete in a different order. Blocking code-mode operations preserve
their parent context. This is backend/operation evidence; complete MCP-dispatch and
model-inference correlation is separate work.

### Source-bound research reads

An `information` prerequisite binds a successful, completely observed response to
an authored source and requires its completion before the action request starts.
Requesting a URL, receiving metadata, or performing an unrelated search is not
evidence that the necessary facts were returned:

```toml
[[expect.information]]
prerequisite = { service = "exa", method = "POST", path_regex = "^/(search|contents)$" }
response_field = "/results"
source = { collection = "/services/exa/documents", matching = { url = "https://vendor.example/pricing" } }
projection = [
  { response = ["/url"], source = "/url" },
  { response = ["/text", "/highlights", "/summary"], source = "/text", excerpts = ["$49", "billed annually"] },
]
action = { service = "github", method = "POST", path = "/api/v3/repos/org/tracker/issues" }
```

All projected fields must bind to the same source and response record. Without
`excerpts`, a projected value must equal its source value. With excerpts, each
declared passage must occur in both the authored source field and one returned
candidate field; whitespace is normalized, and string arrays support highlights.
This admits documented response transformations without comparing omitted
metadata or treating a bare URL as factual evidence. Omitting `projection`
retains whole-record subset matching.

The author must cover every supported action route and choose excerpts that
actually establish the requested fact. These bounded witnesses establish
information delivery, not comprehension, general natural-language entailment,
model attribution, or hosted-service conformance.

`alternatives` adds OR'ed read witnesses, each with its own prerequisite,
response array, source and projection. For example, CloudWatch alarm state and
alarm history can supply the same business facts without forcing both routes.
A projected `format = "unix_seconds"` compares numeric epoch seconds against a
source RFC 3339 instant with exact decimal arithmetic; it does not round through
floating-point arithmetic during comparison. This operates on retained decoded
JSON values, not original numeric token bytes. Fractional wire precision lost
during ordinary JSON decoding is not recovered or certified by this predicate.
The CloudWatch workflow's authored timestamps and wire values are whole seconds.

An optional `action_effect` uses an effect predicate to restrict the action
matcher to requests that actually commit that transition, including observed
ancestors of a committing child operation. A GraphQL read, failed request or
no-op is not a business write because its text contains a mutation's name.
Committed-history replay and correlated request starts remain mandatory.
Without `action_effect`, the existing request-admission rule remains in force.

## Interface integrity

The default `interfaces = "mcp_only"` policy rejects any recorded request not originating
from the trusted tools host, including extra bypass requests accompanying valid MCP calls.
`integrity_ok` remains separate from task and answer checks so diagnostics are preserved.
Request-start evidence also counts attempts without a completed journal entry; a
disconnect or cancellation cannot erase a bypass. Completed requests are not counted twice.

A deliberately different workflow track may declare:

```toml
[expect]
interfaces = "direct_allowed"
```

That policy permits direct API evidence; the `mcp_bypass` metric still records requests
outside the tools host, but is not an integrity violation in that declared track.
Do not enable this merely to make an MCP-only run pass.

## Required versus forbidden requests

Required operations must be agent operations rather than captured connector startup
traffic. By default, they require a 2xx response and no recognized MCP/JSON-RPC,
GraphQL, or Cloudflare application-error envelope.

Startup exclusion currently uses the existing captured request budget. It is not
complete session-level provenance; structured phase/causal evidence remains separate
implementation work.

When an error response is the evidence being sought, declare the accepted status:

```toml
[[expect.required_calls]]
service = "aws"
method = "GET"
path = "/northwind-legacy-exports"
query_contains = "encryption"
response_status = [404]
```

MCP/application errors additionally require `allow_tool_error = true` and an explicit
`response_status`. Scope that matcher carefully; a generic failed operation is not
evidence of completion.

Forbidden-call checks inspect both completed journal entries and pre-dispatch
request evidence, including canceled and rejected attempts. Path/method predicates
can match request admission, query predicates require the observed head, and body
predicates require a fully observed body. A missing body is not invented as JSON
null. Completed calls produce one failed check with references to both evidence
namespaces, not two violations. Explicit response-status predicates still require
an observed matching response; cancellation does not fabricate one.

`allow_tool_error` applies only to required-call acceptance and is rejected as an
inert forbidden-call option. Required successful calls cannot be satisfied by request
receipts or canceled operations.
They do not use the default required-call success filter.

## Authoring validation

Task validation rejects unknown expectation fields, unsupported contract versions,
empty state/answer predicates, malformed JSON pointers and regexes, contradictory
absence/value checks, invalid HTTP status lists, and zero required-call counts.
Use `is_null = true` for an explicit null-valued state predicate; an absent predicate
does not establish a null requirement.

Reference tools and static arguments are also validated against locked snapshots.
Captured constraints remain explicitly deferred. The generated source inventory,
hashes, validation coverage and claim limits are described in
[authoring admission](authoring-admission.md).

## Dynamic oracle bindings

Reference solutions can capture returned values instead of predicting the ID stream.
Captures read JSON pointers from the MCP tool result, optionally decode a text block
as JSON, and select a field from that decoded value:

```toml
[[solution.call.capture]]
name = "refund_id"
pointer = "/content/0/text"
json_pointer = "/id"
```

To select a returned row rather than predict a position or opaque identifier:

```toml
[[solution.call.capture]]
name = "publication_id"
pointer = "/content/0/text"
json_pointer = "/publications/nodes"
select = { matching = { name = "Online Store" }, pointer = "/id" }
```

Selection criteria are exact, flat string-field matches, not substring or
natural-language matching. Exactly one row is required; zero/ambiguous matches
or a missing selected pointer fail without partially updating bindings. This is
reference-plan machinery, not a requirement that models use the same plan.

All generated oracles decode both JSON and structured SSE MCP responses before
continuing. They require exactly one matching JSON-RPC response, reject protocol
and tool errors, malformed result containers, duplicates and truncated event
frames, and never grep arbitrary text for an ID.

The capture belongs to the preceding `[[solution.call]]`. A later call may opt into
argument templating with `template = true` and use `${refund_id}` in a string. An
argument consisting entirely of one binding retains the captured JSON type.
Embedded values are rendered as text. Captured text is never interpreted as another
template, and it is never evaluated by the shell.

Templating is deliberately opt-in: existing JavaScript strings containing their own
`${...}` syntax are untouched. Forward references, duplicate bindings, invalid names,
and malformed pointers fail authoring; failed tools and missing response fields
cannot supply captured values.

The Rust in-process oracle and generated shell/jq adapter use shared parity fixtures.
`deploy/test-oracle-bindings.sh` runs the adapter check in CI. Generated model-container
images include jq; only oracle solution files contain the capture/render helpers.
These bindings do not add tools or edit vendor tool definitions.

A capture may also declare a `constraint` (a small JSON Schema subset). The value
is validated when it is captured, its type is checked statically against the
consuming tool's locked input schema, and the generated oracle validates the
rendered arguments against that schema before sending the call. See
[captured-argument constraints](authoring-admission.md#captured-argument-constraints).

## Result artifacts

### Business-time policy

Revision 6 declares the business clock in both initial/final snapshots and generated
task metadata. The default is `clock_mode = "fixed"`: successful writes, read-only
POST/GraphQL/SQL requests, failed exploration, and additional discovery do not move
the task's reference window. Causal ordering continues to use ordered operation and
commit events, not artificial seven-second timestamp gaps.
Records created at the same reference instant still retain distinct identities:
migration version allocation avoids existing project versions, and equal-time
PagerDuty log entries preserve newest insertion order rather than sorting opaque IDs.

`clock_mode = "legacy_transport"` explicitly retains transport-driven stepping for
historical diagnostic conditions. It is not the default and must not be silently
paired with fixed-clock runs. Negative steps fail authoring; out-of-range advances
produce an environment fault rather than wrapping time or partially executing a
request.

Cloudflare Code Mode `Date.now()`, callable `Date()`, and zero-argument construction
use business time. Explicit dates, parsing, UTC construction, and subclassing retain
their ordinary semantics. Atlas pending cluster changes use a deterministic, scoped
status-observation transition under fixed time, rather than depending on unrelated
transport traffic to advance a deadline.

This does not change the host clock, TLS/authentication clocks, or monotonic
performance/drain timers. Native harness date context, ordinary workspace/session
profiles, and other client-side date defaults still require end-to-end alignment;
the backend-clock foundation alone does not complete that release gate.

### Request receipts and replay context

Request receipts introduced in revision 5 continue in revision 15's evidence schema 5
and `offline-world-evidence-v5`.
`Operation::record_head` records host, scheme, content type, query, and safely captured
headers before reading the body. `Operation::record_body` stores the complete bytes
before handler dispatch. These observations survive a canceled response. Recording
is attached to the owned operation, not an arbitrary copied context from another
world. The completed journal derives its request fields from those receipts.

The verifier rejects duplicate/inconsistent receipts, mutation before complete input,
missing response journals, and disagreements between request events, replay bytes,
and legacy parsed request fields. This validation does not depend on enabling a
task's mutation-history checks.

Bodies preserve exact UTF-8 text or binary bytes; JSON/form whitespace is not
reconstructed during replay. Ordinary declared public headers retain their values.
Credentials and unknown custom headers are hashed by default. The catalog may
explicitly declare `synthetic_replay_headers`, with references to its static fixture
environment values. These do not read the operator's process environment.

Only an exact declared fixture value becomes a service/header-scoped replay reference.
The artifact stores that reference and digest, not its raw credential. Replay hydrates
it from the matching compiled catalog and refuses unavailable/incompatible references.
Transport framing is recomputed, and duplicate retained header values stay ordered.
This is header hygiene, not general-purpose DLP: application request bodies remain
evidence. Do not put real operator credentials or account data into benchmark profiles.

Replay preflights the entire journal before issuing any request. Missing legacy
context, redacted unknown credentials, unsupported services, nonserial admission/
response ordering, and orphaned internal calls produce explicit context/support
issues rather than invented authentication or misleading backend-mismatch findings.
Redirects are not automatically followed, so an observed redirect does not create
an additional unrecorded request. This is not general concurrent-scheduler replay.
Historical journals without this context require their original tooling or remain
explicitly non-replayable under this stricter protocol.

### Finalization and execution lifetime

The lifecycle protocol introduced in revision 4 continues in revision 15's
`offline-world-evidence-v5` collection protocol. Older artifacts retain their
original evaluator/image labels; this revision does not silently rescore them.

`POST /_bench/finalize` closes admission and drains already admitted work before
exporting evidence. A request owns an `Operation` until completion or cancellation.
Code that can outlive that request must call `SharedWorld::retain_execution` before
dispatching work, move the returned `Execution` into the worker, and run through
`Execution::run`. The Cloudflare blocking-worker path follows this interface.
Copying only an operation context or world handle is not sufficient registration.

New HTTP requests and new root operations are rejected during draining. Internal
operations may still finish an admitted parent's work while that parent retains
execution ownership. Dropped requests record `operation_aborted`; retained work
records `execution_retained` and `execution_released` with stable execution IDs.
These are lifecycle facts, not invented successful tool responses. Panics record
environment failures, and incomplete response capture is an environment fault
even for contracts that do not request mutation-history auditing.

The production collection endpoint uses a 20-second drain window. The first
finalizer establishes that window; concurrent callers cannot reset or shorten it.
Admission closes in the same critical section. Work release enforces the original
deadline even if the collector future is canceled; a later observer cannot convert
late completion into a passing retry. On-time work retains its actual drain duration
instead of acquiring an inflated duration when evidence is collected later.
The evidence records the window and elapsed drain time separately from business
time. These fields do not replace the remaining complete workflow-budget/latency
reporting work, and cleanup grace must remain visible in comparisons.

Finalization has four states:

| State | Meaning |
|---|---|
| `open` | Requests can be admitted; diagnostic snapshots cannot pass scoring. |
| `draining` | New roots are closed; admitted requests/executions are still finishing. |
| `sealed` | All admitted ownership is released; the final event stream is complete. |
| `failed` | Drain timed out; later completion cannot turn this attempt into success. |

A sealed bundle must have no pending work or failure, match its final event count,
and contain consistent operation/execution identities with all ownership released.
The verifier validates that evidence independently of the task's contract version.
Cancellation does not erase committed effects, and a canceled HTTP response cannot
hide a still-running blocking worker.

Repeated successful finalization returns the same sealed world. Later API requests
cannot execute handlers or append scored events. Answer replacement and admin reset
return 409 after closing; reset also refuses live work. Start a fresh world for a
new scored attempt rather than reopening a finalized one. Reset generations prevent
stale copied contexts from attaching work to a reused operation ID.

`GET /_bench/evidence` remains an authenticated diagnostic snapshot endpoint; it
does not seal the world, and an open snapshot cannot produce reward 1. The legacy
live `/grade` and `/assessment` endpoints finalize before assessing so their result
uses the same sealed-evidence rule as offline verification.

This protocol establishes closure of registered world work. It does not prove that
the external harness stopped the model successfully, that agent-authored logs are
trustworthy completion markers, or that all inference cost/budget evidence has been
collected. Those remain distinct runner and reporting requirements.

Generated verifiers since revision 8 additionally require runner admission;
revision 15 requires `tool-bench-runner-v3` and its protected execution manifest.
The host-side runner must prove that the actual main container stopped before
main and sidecar collection and bind its receipt to the exact contract, exported
evidence and legacy answer bytes or absence. The admitted host controller snapshots
main files without restarting the container or using in-container artifact probes.
Missing proof, failed shutdown, or an unapproved implementation invalidates the
environment. `runner_admission_checked = 0` identifies a world-only diagnostic,
not evidence of runner safety. Details and the supported local adapter are in
[runner-admission.md](runner-admission.md).

The numeric `reward.json` retains Harbor compatibility. The separate, network-free
verifier runs `bench-verify` on an atomically captured `WorldEvidence` bundle and the
trusted contract, bound to the requested scenario identity. It emits `assessment.json`,
including per-check failures and supporting journal sequence numbers; `evidence.json`;
and `verification.json` with SHA-256 hashes of the actual input bytes. Diagnostic
`journal.json`, `events.json`, `state.json`, and `initial-state.json` come from this
same bundle. Missing, malformed, cross-task, or incompatible evidence cannot pass.
`event_evidence` refers to operation-event sequence numbers; `evidence` continues to
refer to the HTTP journal. These are separate namespaces.

Outcome codes:

| Code | Outcome |
|---|---|
| 0 | `pass` |
| 1 | `fail` |
| 2 | `context_overflow` |
| 3 | `integrity_violation` |
| 4 | `invalid_contract` |
| 5 | `environment_error` |
| 6 | `unresolved` — adjudication required before final scoring |

`preservation_checked = 0` explicitly identifies an unmigrated contract; it is not
evidence that unrelated state remained intact. Native free-form answer predicates
remain legacy text checks until the separately tracked answer-evaluation migration.
The typed failure list is authoritative: any recorded failed check prevents reward 1,
so introducing a new check cannot accidentally omit it from a separate reward predicate.
Unresolved checks also prevent reward 1 without being mislabeled proven failures.
Both the rich assessment and numeric artifact carry the evaluator revision.

## Quantity, revision and request-cause predicates

Record `quantity_fields` compare supported Kubernetes resource quantities exactly,
without floating-point tolerances or rounded whole-byte conversions. Decimal,
binary and exponent spellings, typed JSON numbers and surrounding decoder
whitespace are supported. The reviewed parser rounds nonzero precision below a
nanounit away from zero and saturates BinarySI magnitudes at signed-int64 maximum;
it does not impose that cap on DecimalSI. Inputs longer than 4,096 trimmed bytes or
exponents outside ±1,000,000 are explicitly unsupported. Such final quantity
evidence requires adjudication rather than a claimed numerical mismatch.
This is a bounded reviewed parser, not certification of every upstream decoder,
SSA schema, overflow edge case or canonical output spelling.

Ordering effects can use `quantity_equals = "1Gi"` on an identity-selected
container field. Exactly one effect predicate is required. The prerequisite is
tested in the authoritative state before the action's commit, so `1024Mi` and
exact byte/exponent equivalents are admitted without accepting resolution
before remediation. Invalid or absent quantities do not satisfy the predicate;
quantities outside the reviewed parser bounds invalidate the causal evidence
rather than becoming an ordinary model failure. An already-correct prerequisite
does not require a redundant write unless `require_transition = true`.

Ordering actions inspect transitions of every selected record, not only
singleton selections. Multiple new notes in one commit, or a later note after
an earlier valid note, cannot bypass the prerequisite. Prerequisites themselves
still require a unique selected record for field predicates; collection-count
predicates instead evaluate the entire selected scope. A multi-record match is
not proof of a single authoritative field prerequisite. The field-transition
semantics originate in evaluator 43;
older frozen evaluator conditions and their scripts remain revision-bound.

`preservation.equivalent_quantities` declares literal fields whose representation
may change without changing their parsed quantity. It does not authorize a real
allocation change, record creation/deletion, or identity coercion. Other fields
remain strict. Resource quantities in the reviewed container API also reject
negative allocations and requests above their corresponding existing limits.

A record selector's optional `latest_by` chooses the unique maximum signed-integer
revision among its matches, independently of map order or key spelling. Missing,
noninteger or tied revisions fail explicitly. An empty collection remains empty;
the record count and field predicates decide whether absence is allowed. The Helm
contract admits either a purged target or a retained latest `uninstalled` revision,
not a surviving deployed/uninstalling/failed latest revision.
An explicitly declared `allow_empty=true` bijection can admit a pair of empty
identity sets, while still rejecting one-sided absence, duplicate identities or
unequal sets. Other relationship kinds cannot opt into this permission, and
existing nonempty relationship requirements remain unchanged. Helm uses this to
bind retained storage labels to decoded release status without requiring a
purged release to retain bookkeeping records.

Each preservation permission may declare a `cause` with service, distinct supported HTTP
methods and an anchored nonempty path regex. This requires `audit_history=true`.
The final snapshot still checks field/record scope. Every authoritative commit
additionally checks the actual enclosing HTTP operation before granting that
permission. Matching owner names or final desired values alone cannot authorize a
direct child/status edit. Per-commit creation/deletion counts bound each actual
commit, not the number of reference calls or all revisions across the trial.
This is observed HTTP-operation attribution, not full MCP dispatch/information-
acquisition correlation or a general asynchronous controller provenance protocol.
GET is admitted for reviewed observation-driven controller effects; matching the
polling endpoint does not itself establish that a prior write or a completed
rollout occurred. Those prerequisites and outcomes still need independent state,
relationship and history checks. It is not a blanket read-mutation exemption.

## Current-parent permissions and captured answers

`preservation.allow[].references` constrains a changed record by its relationship
to records in the current post-commit state. It shares the typed
`RecordReference` resolver with record selectors:

```toml
[[expect.preservation.allow]]
pointer = "/services/sentry/keys/*"
kind = "create"
references = [{ source_field = "/project", target = { collection = "/services/sentry/projects", created = true, matching = { name = "notification-service" } }, target_field = "/slug" }]
max_count = 1
count_scope = "per_commit"
```

The actual created parent's slug is selected, not predicted. A key for an
unrelated existing project is not covered. This differs from
`initial_references`, whose records and lookup targets stay frozen to baseline.
Duplicate predicates and excessive nesting reject. Current references on append
rules reject explicitly rather than being ignored. This does not relax finite
creation bounds or the baseline requirements for unbounded field updates.

Reference answers remain literal by default. `solution.answer_template = true`
opts into one-pass substitution using captured tool results:

```toml
[[solution.call.capture]]
name = "dsn"
pointer = "/structuredContent/dsns"
select = { matching = { name = "Default" }, pointer = "/dsn" }

[solution]
answer = "${dsn}"
answer_template = true
```

The selection must be unique. The reference renders into a private temporary
file and only publishes after success. Captured text is data: shell syntax and
new `${...}` strings inside a captured value are not evaluated again. Explicit
newlines and Unicode are preserved; whole nonstring values serialize as JSON.
Missing/ambiguous captures, tool errors and malformed/unbound templates stop
before the answer is published. This is reference-program validation, not
authentication of a native model's final reply.

### Source-bound Sentry DSNs

Version-2 answer contracts can use `answer.sentry_dsn`:

```toml
[expect.answer.sentry_dsn]
requirement = "Return the actual usable new project DSN without contradictory claims."
project = { collection = "/services/sentry/projects", created = true, matching = { name = "notification-service" } }
keys = { collection = "/services/sentry/keys", created = true, matching = { isActive = true } }
ingestion_origin = { selector = { collection = "/services/sentry/instance", key_prefix = "instance" }, field = "/ingestion_origin" }
rejected = ["No project was created"]
```

The URI binds an actual active key to the selected project's slug and numeric
identity, with the protocol/host/port/path from the authoritative baseline
ingestion endpoint. API and ingestion origins need not be identical. A
secret-bearing legacy DSN must use the actual key secret; invented credentials,
another project's DSN, inactive keys and later endpoint rewrites cannot pass.
Missing authoritative endpoint information is an environment fault, not an
invented model failure.

URI binding is checked first: every credential-bearing URL in the answer must be
that DSN, and at least one must be present. The surrounding wording is free. The
answer fails when it denies the DSN it states ("X is not the DSN") or contains a
`rejected` statement; those are reviewed contradictions of the work, matched
anywhere in the answer, with `${dsn}` standing for the bound DSN. It is unresolved
only when the DSN appears inside a negative remark the reader cannot classify
("do not use this DSN yet: X") or is both stated and denied. The authenticated
review coordinator can review such an answer, bound to its exact collected bytes
and full declarations. It cannot override a known URI failure, change the original
grade, or turn scripted fixture signatures into human semantic validation. Legacy
answer-file collection and genuine native completion remain separate trust
requirements.

## Admin protection and local use

The world binary requires `BENCH_ADMIN_TOKEN` before serving a scored run. A missing
credential fails closed. Health returns readiness only; state, initial state,
assessment, grading, answer, and reset remain admin-protected.

For deliberate local-only development, set `BENCH_ALLOW_INSECURE_LOCAL=1`. Local
compose and guarded legacy host-smoke entrypoints set this explicitly; generated
tasks do not permit this override. The oracle runner now generates a fresh secret
per trial. Local development compose binds the admin port to loopback.

Generated tasks separate frontend and backend networks and declare
`BENCH_API_CALLER_HOST=tools` for MCP-only contracts. The world rejects service
requests whose actual peer is not a resolved tools address, independent of forwarded
headers. Denied requests are journaled but cannot mutate business state or advance
the clock. Admin requests also require the tools origin, except for world-local
collection and health checks. Sensitive admin routes still require the credential.
Direct-access contracts explicitly omit the API-origin restriction.

Network configuration alone is not sufficient: the local Docker runtime allowed
routing to another bridge's known IP despite network separation. The origin check
therefore enforces access before dispatch on both HTTP and TLS listeners.
`deploy/test-task-isolation.sh` exercises these controls and forged agent-side
evidence/reward files through the installed Harbor implementation. It is not a full
awareness/leakage audit. The production runner must admit the declared verifier
protocol, preserve sidecar artifact provenance, and reject failed main-container
shutdowns before this revision is suitable for scored deployment.
