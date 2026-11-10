# Source-bound answer facts (`record_facts`, statements mode)

Lookup tasks are graded on what the agent tells the user. A keyword predicate
(`contains`, `regex`) cannot tell "it is Northwind" from "it is not Northwind",
and it accepts the right number attached to the wrong company. Statements mode
replaces those predicates with facts **derived from the authored baseline
data** and a conservative reading of natural prose.

Code: `crates/mock-core/src/grading/answers/record_facts.rs` and its
submodules `derive.rs` (where facts come from), `statements.rs` (how an answer
is read) and `numbers.rs` (exact quantities and dates). Tests:
`crates/mock-core/tests/statement_answers_*.rs`.

## Verdicts

| Outcome | When |
|---|---|
| Pass | Every declared fact is *affirmed*, nothing competes with it, and any declared affirmative grammar covers the reply. |
| Fail | A fact is denied (`not X`, `rather than X`), another authored record's value is asserted in its place, a number with the reviewed unit or currency differs, a `rejected` form matches, or the fact is absent. |
| Unresolved | The reader cannot classify the answer: a hedge or retraction, an unreviewed notation (number words, `11/12/2026`, a bare number where a unit is required), a partial name, a fact both asserted and disputed, or a correct fact next to another record's value. |
| Environment error | The source record, field, join or capture is missing from the initial state. A tie in a ranking is unresolved. |

The reader never invents a score: anything it does not understand is
Unresolved, and a missing source is an environment fault.

## Where facts come from

Facts are read from the **initial business state** only. A contract never
restates the expected answer: changing the authored data changes the graded
fact (each test file proves this with `outcome_after`).

```toml
[expect.answer.record_facts]
requirement = "Human-readable criterion shown to the adjudicator."
mode = "statements"
selection = { largest = { field = "/properties/amount" } }   # only | largest | smallest | group

[expect.answer.record_facts.selector]            # ordinary RecordSelector
collection = "/services/hubspot/deals"
matching = { archived = false }
# field_patterns = { "/CloseDate" = "^2026-0[456]-" }

[expect.answer.record_facts.filter]              # optional, applied after the selector
excluding = [{ "/properties/dealstage" = "closedwon" }]
# matching = { "/status" = "paid" }
# patterns = { "/title" = "^Pricing" }           # regex on a text field
# present = ["/Amount"]                          # field must be non-null
# absent = ["/cancelledAt"]                      # field must be null or missing
# ranges = { "/status_code" = { min = "500", max = "599" } }
```

### Bounded joins: `related`

A related record is reached from the primary record (or from another related
record named by `from`) by field equality. At most eight; no cycles.

```toml
[expect.answer.record_facts.related.link]
collection = "/services/hubspot/associations"
filter = { matching = { "/fromObjectType" = "companies", "/toObjectType" = "deals" } }
on = { "/toObjectId" = "/id" }                   # related field = field of the `from` record

[expect.answer.record_facts.related.company]
from = "link"
collection = "/services/hubspot/companies"
on = { "/id" = "/fromObjectId" }
# pick = { largest = "/created_at_epoch" }       # default "only": exactly one match
# pick = "all"                                   # keep every match (1-16): a fact read from
#                                                # it lists one value per record, and the
#                                                # answer must state each of them
```

If the `from` field is an array, the join matches any element. Values compare
as scalars (`7102` joins `"7102"`; `{"$oid": "..."}` unwraps). Three further
forms cover links that live in text or in the record key:

```toml
[expect.answer.record_facts.related.pipeline]
from = "commit"
collection = "/services/gitlab/pipelines"
on = { "/id" = "/message" }
capture_from = { "/message" = 'pipeline #(\d+)' }   # compare the captured part of the `from` field
# capture = { "/email" = '^([^@]+)@' }              # compare the captured part of the related field

[expect.answer.record_facts.related.ticket]
from = "comment"
collection = "/services/zendesk/tickets"
on = { "/id" = "/_key" }                            # `/_key` is the record's collection key
capture_from = { "/_key" = '^(\d+)/' }

[expect.answer.record_facts.related.pricing]       # no `on`: a second source chosen by its
collection = "/services/firecrawl/pages"            # own reviewed filter
filter = { patterns = { "/title" = '^Pricing \| Norvik' } }
```

### Bounded computed facts: `selection.group`

```toml
[expect.answer.record_facts.selection.group]
# unnest = "/items"            # each array element becomes a row {parent, item}
by = "/OwnerId"                # omit for one total over every row
measure = { sum = "/Amount" }  # "count" | { sum } | { average } | { sum_product = ["/item/price", "/item/qty"] }
pick = "largest"               # only | largest | smallest
# rows = { matching = { "/item/category" = "outdoor" } }   # RecordFilter on rows
```

The primary record is then `{ key, value, count }`: `/key` is the group key,
`/value` the exact measure. `/member/<field>` reads a field every row of the
chosen group shares (an environment error if they disagree); `/members/<field>`
lists that field of every row, and the answer must state each value. Arithmetic is
exact rational arithmetic; there is no floating point.

### Facts

```toml
[expect.answer.record_facts.facts.rep]
record = "rep"                 # a related record; omit for the primary record
field = "/Name"
kind = "text"                  # text | identifier | number | money | date
# append = ["/lastName"]       # joined with one space
# also = ["/login"]            # other fields of the same record that name it
# capture = '--- FAIL: (\S+)'  # one capture group, exactly one distinct value, for logs/pages
# clock = true                 # render epoch seconds / ISO UTC timestamp as HH:MM
# units = ["ms", "milliseconds"]       # first entry is the source unit; may be several
#                                      # words ("to add"); "$"/"usd" on a money fact
# require_unit = true          # a bare number does not count
# rounding = true              # accept a correct rounding (averages, rates)
# min_prefix = 7               # accept a short commit hash
```

Reviewed wordings of a coded source value are bound to that value, so they
stop applying when the authored record changes:

```toml
[expect.answer.record_facts.facts.status.wordings]
UNFULFILLED = ["has not shipped", "hasn't shipped", "not yet shipped"]
FULFILLED = ["has shipped", "was shipped"]
```

A `date` fact reads an ISO date or timestamp, or a field that holds exactly
one full prose date ("January 15, 2027"). Expected fact values belong in the
authored data, not literal prose in a checker.

A `utc_date` fact derives a calendar day from integer Unix seconds or an RFC
3339 timestamp after resolving its offset to UTC. It does not guess milliseconds,
fractional epoch units or ambiguous local timestamps. Both date kinds validate
the actual calendar, including leap days; impossible source dates are environment
errors.

Text facts with `clock = true` use that same validated timestamp interpretation
before rendering `HH:MM` in UTC. An equivalent RFC 3339 offset is accepted;
an impossible date or unsupported epoch is not reduced to an apparently valid
clock time. Exact seconds and full UTC RFC 3339 forms also bind the actual
timestamp. They are not accepted by stripping a seeded date or a fixed `:00`
suffix.

## How an answer is read

1. `rejected` forms (templates with `${fact}` slots) fail the answer outright;
   use them for role swaps such as `${blocker} is blocked by ${blocked}`.
2. `context` forms are reviewed neutral remarks; they are set aside before
   reading. `predicates` are reviewed negative predicates that are true of the
   facts (`does not match the expected scopes`) and are not read as denials.
   `contradictions` are reviewed words that say the opposite of what the task
   asks (`fastest` for a slowest-region task): a fact in the same sentence is
   disputed.
3. The answer is split into sentences. Each fact is located with typed
   matching:
   - text and identifiers match whole names only (`checkout-web` is not found
     inside `checkout-web-7f8b9c6d5`);
   - numbers are standalone tokens (not digits inside an identifier, version,
     date, time or address), compared exactly; `1,234.50`, `1234.5` and
     `$1.2345k` are equal, `1,234.49` is not; ms/s/min/h convert exactly;
     a foreign currency or an inconvertible unit never matches;
   - dates accept ISO, `Nov 12, 2026`, `12 November 2026` and a year-less
     `Nov 12`.
4. Each mention is *denied* (a negator reaches it through function words and
   label nouns only: `not X`, `isn't the order X`, `no longer X`, `not owned
   by X`, `rather than X`), *disputed* (a negator reaches it across one other
   word, `did not pick X`; a negated verb follows it, `X did not delete …`,
   `X is not the …`, `X was ruled out`; a retraction, `at first it looked
   like …`; or a reviewed contradiction cue), *hedged* (`probably`, `I
   think`, `may be`) or *affirmed*. A short closed list of negative predicates
   that leave their subject or object correctly identified is not disputed
   (`column X does not exist`, `PR X has not been merged`, `could not pull
   X`); anything else needs a reviewed `predicates` entry.
5. Alternatives are the same fact read from the other records of the same
   collection (other groups for aggregates). Asserting one instead of the
   source fact fails; asserting one next to it is unresolved, unless the
   sentence is a reviewed `context` form or denies the alternative.
6. With `assertions`, the complete reply must be covered by reviewed affirmative
   clauses and neutral context. Typed slots bind the actual source values,
   including supported conversions and notations. Unknown extra predicates,
   cross-sentence commentary and unreviewed structural headers are unresolved,
   even when all required values appear.

### Reviewed affirmative clauses

```toml
[expect.answer.record_facts]
assertions = [
  "${region} is the slowest, averaging ${average}",
  "${region} averages ${average}",
  "Slowest region: ${region}",
  "Average duration: ${average}",
]
```

These are typed clause forms, not frozen answer strings. The same form binds a
new winning region or a changed average, and supported ms/seconds conversions
remain valid. Negations, contradictions, missing facts and incompatible units
are still checked before clause coverage. Every extra clause must be reviewed;
an asserted fact in one sentence does not license an unsupported claim in the
next. Literal numeric and entity claims must not be smuggled into the forms:
they need their own source bindings.

An extra explanatory fact may declare `optional = true`. Its source must still
exist, its value is checked whenever stated, and any clause containing it must
be reviewed. Omission alone is allowed. At least one fact must remain required;
optional facts are invalid without affirmative clauses. This supports, for
example, an optional source-derived event count without requiring every reply
to include it or freezing that count inside a literal sentence.

A related selector may explicitly set `allow_absence = true` when an optional
explanation has no matching record in a valid authored collection. All facts
read from it must be optional, as must descendants joined from it. Absence
supplies no matchable value: a stale explanation still cannot pass clause
coverage. This does not excuse missing collections, invalid fields on a
matched record, ambiguous matches or missing required facts.

Context and rejected-form slots use the same typed matching as affirmative
clauses. They do not synthesize a second, weaker set of number or date spellings;
source-bound currency, unit conversions and identifier prefixes remain valid
in those slots too. Unreviewed compound-rate denominators and unsupported
microsecond/nanosecond conversions require adjudication, rather than being
treated as a confidently different value.

Optional values do not establish that a required value was stated. Conversely,
a mention of some other record does not by itself assert an unasked optional
field. Extra text must still pass reviewed clause coverage. A negative optional
context cannot be stripped when its value also denotes a required fact; it must
not hide a denial after source values converge.

`bench-answer-forms SCENARIO_DIRECTORY CANDIDATE_ANSWERS_JSON` proposes slot
bindings for author review. It does **not** certify the prose's meaning and
never changes a scenario or approves validation evidence. Inspect every
proposal against the request and data, including any remaining literal
quantities. A proposal can bind the right values inside a false claim; such a
claim must not become an approved assertion.

Adding this interface does not migrate contracts automatically. Contracts
without `assertions` retain the predicate/role limitations below and are not
thereby semantically complete. The coverage inventory reports
`answer_assertion_grammar_missing` for such a contract rather than labeling its
relationships reviewed merely because its values are source-bound.

Known limits, stated so nobody over-reads a pass:

- Without affirmative clause coverage, the reader binds values and polarity, not arbitrary predicates. A wrong
  predicate about a correctly named value ("eu-west-1 is the fastest") is only
  caught by a reviewed `contradictions` cue or `rejected` form.
- Without affirmative clause coverage, role swaps among the declared facts
  themselves are only caught by `rejected` forms.
- A bare number without declared `units` matches any equal standalone number;
  declare `units` (and `require_unit`) for counts.
- Hedge and retraction cues are sentence-level and deliberately broad; they
  cost adjudication, never a wrong score.
- A paraphrase of a captured free-text fact that shares no word with it reads
  as absent (Fail), as it did under the keyword predicate. Shared words make
  it Unresolved.
- Units next to a number are read from the same sentence only: a table whose
  unit sits in the header row leaves the cell number unlabeled (Unresolved).

## What still cannot be bound

The current derivations include bounded per-row joins, second-level aggregates,
interval-normalized arithmetic and the reviewed on-call rotation/override
fixtures. Those capabilities do not establish arbitrary query, scheduling or
free-text semantics.

Leave a scenario on the legacy predicate, and say why, when its answer needs
unsupported derivation, judgment over unfamiliar free text, or a value that is
not in the authored data at all (an API-generated error code).
`legacy_answer_semantics` in the coverage inventory stays open for those.

Scenarios left on keyword predicates today (the same ledger is enforced in
`crates/mock-core/tests/statement_answers_inventory.rs`):

| Scenario | What binding it would need |
|---|---|
| `business/stripe/customer-mrr` | A per-item join to prices inside the aggregate, plus cents-to-dollars and yearly-to-monthly normalization. |
| `cloud/multi/supabase-slack-referral-signups` | Two independent aggregates (a customer count and the sum of those customers' orders) and a negative text filter. |
| `search/elasticsearch/checkout-errors-last-hour` | A second-level aggregate: the most frequent message within the winning service. |
| `collab/multi/slack-pagerduty-dbre-oncall`, `ops/pagerduty/payments-oncall-now`, `ops/multi/crashloop-oncall-lookup` | The PagerDuty rotation (and override) rendered against the business clock. |
| `search/brave/caterer-saturday-hours` | Phone-number and opening-hours notations (time ranges). |
| `example/aws/missing-bucket-policy` (unscored) | The expected API error code is produced by the service; it is not in the authored data. |

## Tests

`crates/mock-core/tests/statement_support/mod.rs` loads the scenario's real
contract and dataset and grades through `mock_core::grading::grade`. Every
migrated scenario has: the reference answer passing, an independently worded
valid answer, negated / rejected-alternative / wrong-entity / near-miss
failures, unreviewed wording left unresolved, a missing source as an
environment error, and a data edit showing the fact follows the source.
