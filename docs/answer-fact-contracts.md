# Source-bound answer facts (`record_facts`)

Lookup tasks are graded on the reply the agent writes to the user. A
`record_facts` contract names facts **derived from the authored baseline data**,
and the reader checks that the reply states each required fact, does not deny it,
and makes clear which record is its answer. The reply is free prose: there is no
list of accepted sentences.

Code: `crates/mock-core/src/grading/answers/record_facts.rs` and its submodules
`derive.rs` (where facts come from), `statements.rs` (how a reply is read) and
`numbers.rs` (exact quantities and dates).

## Verdicts

Only a pass earns reward.

| Outcome | When |
|---|---|
| Pass | Every required fact is affirmed, none is denied or disputed, no other authored record competes with it as the reply's answer, and facts named in `order` are stated in that order. |
| Fail | The reply is empty; a `rejected` form matches; or a required fact is never affirmed and is denied, replaced by another authored record's value, stated with a different value in the reviewed unit or currency, or simply absent. |
| Unresolved | A required fact is affirmed in one place and denied or disputed in another; it appears only in disputed statements; the reply leaves its answer unclear between authored records; `order` is violated; a missing fact may be present in a notation the reader cannot type; or the reply is longer than 65,536 bytes or holds invisible control characters. |
| Environment error | A source record, field, join or capture is missing from the initial state. |

A fail on any required fact outranks an unresolved finding on another. A tie in
a ranking is unresolved.

## Where facts come from

Facts are read from the **initial business state** only. A contract never
restates the expected answer: changing the authored data changes the graded
fact.

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
#                                                # reply must state each of them
# allow_absence = true                           # no match is allowed; every fact read from
#                                                # it must be optional
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
lists that field of every row, and the reply must state each value. Arithmetic is
exact rational arithmetic; there is no floating point.

### Facts

```toml
[expect.answer.record_facts.facts.rep]
record = "rep"                 # a related record; omit for the primary record
field = "/Name"
kind = "text"                  # text | identifier | number | money | date | utc_date | phone | time_of_day
# optional = true              # context only; see "Optional facts"
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

Wordings of a coded source value are bound to that value, so they stop applying
when the authored record changes:

```toml
[expect.answer.record_facts.facts.status.wordings]
UNFULFILLED = ["has not shipped", "hasn't shipped", "not yet shipped"]
FULFILLED = ["has shipped", "was shipped"]
```

A `date` fact reads an ISO date or timestamp, or a field that holds exactly one
full prose date ("January 15, 2027"). A `utc_date` fact derives a calendar day
from integer Unix seconds or an RFC 3339 timestamp after resolving its offset to
UTC. Both validate the actual calendar; an impossible source date is an
environment error. A text fact with `clock = true` renders the same validated
timestamp as `HH:MM` in UTC, and also binds the exact seconds and the full UTC
RFC 3339 form.

## How a reply is read

1. A `rejected` form (a template with `${fact}` slots, such as
   `${blocker} is blocked by ${blocked}`) fails the reply outright.
2. `context` forms are set aside before reading. `predicates` are negative
   phrases that are true of the facts (`does not match the expected scopes`) and
   are not read as denials. `contradictions` are words that say the opposite of
   what the task asks (`fastest` for a slowest-region task): a fact in the same
   sentence is disputed.
3. The reply is split into sentences (lines, table cells, `;` and sentence
   punctuation), and every statement of each fact's source value is located.
4. Each statement is classified:
   - **denied**: a negator reaches it through function words and label nouns
     only (`not X`, `isn't the order X`, `no longer X`, `not owned by X`,
     `rather than X`);
   - **disputed**: a negator reaches it across one other word (`did not pick
     X`); a denial follows it (`X did not delete …`, `X is not the …`, `X was
     ruled out`, `X's deal is not the largest`); the sentence holds a retraction
     (`at first it looked like …`) or a `contradictions` cue; or the next
     sentence takes it back (`That was not the final pick.`);
   - **affirmed**: anything else. A hedged statement (`it appears to be X`,
     `probably X`) counts as affirmed.

   A short closed list of negative predicates leaves its subject or object
   correctly identified and is not a dispute (`column X does not exist`, `PR X
   has not been merged`, `could not pull X`); anything else needs a `predicates`
   entry.
5. The verdict follows the table above, fact by fact. A fact with several source
   values (a `pick = "all"` join, `/members/<field>`) needs each value affirmed.

### Notations accepted

| Fact | Read as equal |
|---|---|
| Any | Case, typographic dashes and quotes, and Markdown emphasis or backticks do not matter. |
| Text, identifier | Whole names only: `checkout-web` is not found inside `checkout-web-7f8b9c6d5`. A link also matches without its scheme or trailing slash. `also` fields, `wordings` and a `min_prefix` abbreviation count as the value. |
| Number, money | Standalone tokens (not digits inside an identifier, version, date, time or address), compared exactly: `1,234.50`, `1234.5` and `$1.2345k` are equal, `1,234.49` is not. Counts in words from zero to ninety-nine are read (`six attempts`). `k`, `thousand`, `million`, `mn`, `billion`, `bn` scale any number; `m` and `b` scale money. ms/s/min/h convert exactly; `%`, `percent` and `pct` are one unit. With `rounding`, a correct rounding is equal. Another currency or an inconvertible unit never matches. |
| Unit | Attached to the number, the word after or before it, or the label it answers (`Failed sign-ins on 13 March: 6`), within the same sentence. `$` is the unit of a money fact. |
| Date | ISO, `Nov 12, 2026`, `November 12th`, `12 November 2026`; the year may be left out. `11/12/2026` is not read. |
| Clock time, duration | `9 a.m.` and `2:30 pm` are read as `09:00` and `14:30`; `47 min 12 s` as `47:12`. |
| Phone, time of day | Common display variants of the source value (dashes, dots, parentheses, country prefix; 12-hour and 24-hour). |

A value that is never affirmed but may be present in another notation (a bare
number where a unit is required, part of a name, other number words, a numeric
date) is unresolved rather than failed.

### Other records

The alternatives to a fact are the same fact read from the other records of the
collection (other groups for an aggregate). A number counts as an alternative
only when it carries the reviewed unit or currency. A denied or disputed
alternative (`not Globex`) and a value that equals a required fact are ignored.

- If the source value is never affirmed, stating an alternative fails.
- If the source value is affirmed, an alternative is context when it appears in
  a sentence that also affirms a required fact, in a sentence that sets it aside
  (`excluding`, `compared with`, `the runner-up`, `the previous`, `also`, `for
  context`, `staging`), or between the first and the last statement of the
  source value.
- An alternative stated on its own **after** the last statement of the source
  value is unresolved when that sentence presents it as the answer (`actually`,
  `in fact`, `instead`, `the answer`, `final`, `largest`).
- An alternative stated on its own **before** the first statement of the source
  value requires the reply to conclude on the source value: it is unresolved
  when the last statement of the source value is a list item, or is worded as a
  lesser item (`second`, `next`, `also`, `another`, `previous`), and carries no
  concluding cue (`so`, `therefore`, `the answer`, `in summary`, `actually`,
  `largest`, `→`).
- Alternatives stated on their own both before and after are unresolved.

### Statement order: `order`

```toml
[expect.answer.record_facts]
order = ["oncall1", "oncall2"]
```

`order` names two or more required facts of the same kind whose roles a reply
could swap, such as the level-1 and level-2 on-call. The reply must first state
them in this order; a later-listed fact stated first is unresolved. Where a swap
has a fixed wording, a `rejected` form fails it outright.

### Optional facts

A fact with `optional = true` is context. Stating it, omitting it or misstating
it never decides the verdict. Its source must still exist, unless its related
record sets `allow_absence = true`. Its value can fill a slot of a `context` or
`rejected` form. At least one fact must be required, a required fact cannot read
a record that allows absence, and `order` cannot name an optional fact.

## Known limits

- The reader binds values and polarity, not arbitrary predicates. A wrong
  predicate about a correctly named value ("eu-west-1 wins on speed") is caught
  only by a `contradictions` cue or a `rejected` form.
- Nothing reads which role a value plays. A swap among the declared facts is
  caught by `order`, by a `rejected` form, or by a number's required unit
  (`1,318 events` where the source holds `2,172 events`).
- A wrong optional detail next to correct required facts passes.
- A list fact needs each source value. An extra value listed in the same
  sentence is context and is not counted against the reply.
- A bare number without declared `units` matches any equal standalone number;
  declare `units` (and `require_unit`) for counts.
- Retraction cues are sentence-level and broad; they cost adjudication.
- A paraphrase of a captured free-text fact that shares no word with it reads
  as absent (Fail). Shared words make it Unresolved.
- A unit is read from the number's own sentence: a table whose unit sits in the
  header row leaves the cell number unlabeled.

## What still cannot be bound

The derivations cover bounded per-row joins, second-level aggregates,
interval-normalized arithmetic and the reviewed on-call rotation and override
fixtures. They do not establish arbitrary query, scheduling or free-text
semantics.

Leave a scenario on the legacy predicate, and say why, when its answer needs
unsupported derivation, judgment over unfamiliar free text, or a value that is
not in the authored data at all. `legacy_answer_semantics` in the coverage
inventory stays open for those. Today that is one unscored scenario, enforced in
`crates/mock-core/tests/statement_answers_inventory.rs`:

| Scenario | Why |
|---|---|
| `example/aws/missing-bucket-policy` | The expected API error code is produced by the service; it is not in the authored data. |

## Tests

`crates/mock-core/tests/statement_support/mod.rs` loads a scenario's real
contract and dataset and grades through `mock_core::grading::grade`.

- `natural_answers.rs` grades the corpus in `tests/fixtures/natural_answers/`:
  replies as agents write them, each marked `pass` or `reject` (fail or
  unresolved), for the `record_facts` scenarios.
- `answer_phrasing_*.rs` and `statement_answers_*.rs` hold per-scenario cases:
  alternative phrasings that pass; denied, wrong-record, role-swapped and stale
  replies that earn no reward; a missing source as an environment error; and
  data edits showing the fact follows the source.
- `bench-answer-probe SCENARIOS_ROOT CORPUS_JSON` grades a corpus file from the
  command line and prints one JSON line per reply.
