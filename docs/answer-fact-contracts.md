# Source-bound answer facts (`record_facts`)

Lookup tasks are graded on the reply the agent writes to the user. A
`record_facts` contract names facts **derived from the authored baseline data**,
and the reader checks that the reply states each required fact, does not deny it,
and gives the source value as its answer. The reply is free prose: there is no
list of accepted sentences.

The reader is a deterministic heuristic over sentences, clauses and cue words. It
does not parse sentences. It is tuned to accept correct replies written in
ordinary prose and is measured against corpora of such replies; see
[Known limits](#known-limits) for what it gets wrong.

Code: `crates/mock-core/src/grading/answers/record_facts.rs` and its submodules
`derive.rs` (where facts come from), `statements.rs` (how a reply is read) and
`numbers.rs` (exact quantities and dates).

## Verdicts

Only a pass earns reward.

| Outcome | When |
|---|---|
| Pass | Every required fact is affirmed, none is denied or disputed, the reply's answer for each fact is the source value (see [The reply's answer](#the-replys-answer)), and facts named in `order` are stated in that order. |
| Fail | The reply is empty; a `rejected` form matches; or a required fact is never affirmed and is denied, replaced by another authored record's value, stated with a different value in the reviewed unit or currency, or simply absent. |
| Unresolved | A required fact is affirmed in one place and denied or disputed in another; it appears only in disputed statements; the reply's answer for it is another authored record, a list of records, or never given (the source value appears only where the reply sets it aside); `order` is violated; a missing fact may be present in a notation the reader cannot type; or the reply is longer than 65,536 bytes or holds invisible control characters. |
| Environment error | A source record, field, join or capture is missing from the initial state. |

A fail on any required fact outranks an unresolved finding on another. A tie in
a ranking is unresolved.

In one paragraph: a reply passes when every required fact is affirmed and not
denied, and for each fact the reply's answer is the source value. The answer is
read as an ordered sequence of claims. The first record the reply commits to
takes it. Another authored record takes it over only by strong wording ("the
most recent is 5511", "the real peak was"). The source value takes it back only
by concluding wording ("so the app is X", "Owner: X"). The source value listed
with another record by a bare conjunction is not an answer. A record that is
denied or set aside in its own clause or sentence is not a claim.

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
   clause is disputed.
3. The reply is split into sentences (lines, table cells, `;` and sentence
   punctuation), and every statement of each fact's source value is located.
4. Each statement is classified:
   - **denied**: a negator reaches it through function words and label nouns
     only (`not X`, `isn't the order X`, `no longer X`, `not owned by X`,
     `rather than X`);
   - **disputed**: a negator reaches it across one other word (`did not pick
     X`); a denial follows it (`X did not delete …`, `X is not the …`, `X was
     ruled out`, `X's deal is not the largest`); the sentence holds a retraction
     (`at first it looked like …`); its clause holds a `contradictions` cue; or
     the next sentence takes it back (`That was not the final pick.`);
   - **affirmed**: anything else. A hedged statement (`it appears to be X`,
     `probably X`) counts as affirmed.

   A short closed list of negative predicates leaves its subject or object
   correctly identified and is not a dispute (`column X does not exist`, `PR X
   has not been merged`, `could not pull X`); anything else needs a `predicates`
   entry.

   A statement negated only **in passing** does not count once the reply has
   concluded on the value elsewhere (`Round: Series B`, then `the profile has
   not been updated for the Series B`). In passing means the negator reaches
   the value across a verb (`not owned by X`, `did not pick X`) or follows it
   as a negated verb (`X did not delete …`), and nothing denies or retracts
   the value itself. Without a concluding statement it counts as before.
5. For each fact that is affirmed and nowhere denied or disputed, the reply's
   answer is read from its claims, in order (see
   [The reply's answer](#the-replys-answer)).
6. The verdict follows the table above, fact by fact. A fact with several source
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

### The reply's answer

The alternatives to a fact are the same fact read from the other records of the
collection (other groups for an aggregate). A number counts as an alternative
only when it carries the reviewed unit or currency. A value that equals a
required fact is not an alternative.

If the source value is never affirmed, stating an alternative fails. Otherwise
the reader collects **claims** for the fact and reads them in the order they
appear. The answer starts open.

| Claim | When | Effect on the answer |
|---|---|---|
| Source | The source value is affirmed and not set aside. | Takes an open answer and keeps its own. Takes the answer back from another record or from a list only when *concluding*. |
| Other | Another authored record's value is stated and is not denied, disputed or set aside. | *Strong*: takes the answer wherever it appears. *Opening*: takes an open answer. Otherwise it is named in passing and changes nothing. |
| Listed | The source value and another record's value are joined by nothing but a comma, a slash or a conjunction (`X and Y`, `X, Y`, `X or Y`). | Makes an open or source answer a list. |

The reply's answer must end on the source value. An answer that ends on another
record, on a list, or still open is unresolved.

What makes a claim, with the cue lists in `statements.rs`:

- **Not a claim: denied.** A denied or disputed alternative (`not Globex`,
  `Globex was ruled out`) is ignored.
- **Not a claim: set aside.** An alternative is set aside by a word anywhere in
  its sentence (`SET_ASIDE`: `excluding`, `compared with`, `the runner-up`,
  `previous`, `also`, `other`, `for context`, `staging`, `failed`, `if`,
  `should`, `normally`), or by
  a `contradictions` cue in the sentence. The source value is set aside by
  wording in its own clause, before it (`DEMOTED_BY`: `ruled out`, `rather
  than`, `ahead of`, `behind`, `over`) or after it (`DEMOTED_AS`: `is a red
  herring`, `is outdated`, `was considered`, `for comparison`). A reply that
  states the source value only where it is set aside has not answered.
- **Answer cues** come in two kinds. `ASSERTED` wording is about its whole
  clause (`actually`, `therefore`, `the answer`, `the real`, `in short`).
  `SINGLED_OUT` wording picks one thing out of several (`the one`, `final`,
  `chosen`, `largest`, `latest`) and presents the thing it describes, which
  need not be every value in the clause.
- **Concluding** (source): the clause holds an answer cue of either kind or a
  conclusion cue (`CONCLUSION_CUE`: `so`, `only`, `matches`, `remaining`,
  `current`, `→`), or the value completes an identification (`IDENTIFIES`: `the
  app is X`) or answers a label. A label is the lead-in before a colon or
  equals sign with at most two words between it and the value (`Owner: X`,
  `That leaves one: order X`); a lead-in that itself sets the value aside
  (`Previous owner: X`) does not conclude.
- **Strong** (other record): the clause holds an answer cue, or the value
  directly completes an identification (`the vendor is Y`). For a measured
  fact only `ASSERTED` wording counts as a cue: a date or amount that happens
  to equal another record's turns up beside a `SINGLED_OUT` word in passing
  (`the PO matches the one we sent on 20 October`). An identification without
  a cue does not take the answer from a source value the reply has already
  concluded on (`Winning category: outdoor`, then `the only difference is
  apparel`).
- **Opening** (other record): the value answers a label or opens its line.

A clause runs to the nearest comma, semicolon, colon or clause word (`but`,
`so`, `while`, `because`, `however`); a parenthetical is a clause of its own.

Measured facts (numbers, amounts, dates, clock times) are restated, compared and
negated in passing far more than names are, so they are read more leniently.
Only a negator that governs the value directly, an explicit denial after it, a
`contradictions` cue or a retraction counts against one. The source value is
never set aside or listed, and another record's value competes only by strong
wording, never by opening. A fact named in `order` is not set aside either: its
place in the order is described in those words (`then`, `next`, `the backup`).

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

The reader does not parse sentences, so some correct replies are left
unresolved and some wrong replies pass. Both are measured on three corpora in
`crates/mock-core/tests/fixtures/`, and every reply the reader gets wrong is
marked in its fixture entry with `"known"`, the verdict the reader returns
today.

| Corpus | Replies | Reader agrees | Known limit |
|---|---|---|---|
| `natural_answers/`, `natural_answers_heldout/`: correct replies | 500 | 499 pass | 1 unresolved |
| `natural_answers/`, `natural_answers_heldout/`: wrong replies | 300 | 300 rejected | none |
| `wrong_answers/`: wrong replies written to defeat the reader | 80 | 48 rejected | 32 pass |

`from_unit_tests.json` in `natural_answers/` and in `wrong_answers/` holds
replies that unit tests used to assert on and the reader misread: 7 correct
replies, 5 of them still left unresolved, and 29 wrong replies that pass. They
were picked because the reader got them wrong, so they are not part of the
rates above. `natural_answers/pilot_agent.json` holds the five replies that
pilot agents wrote and an earlier reader left unresolved; all pass now.

**Correct replies left unresolved** state a bare answer and then name another
authored record in wording the reader takes for a claim ("the September invoice
was PO-47790", "The 12:05 highlights cut is https://…"), or state the answer in
a clause that holds a dismissal word ("went over the function size limit").

The first 440 correct replies and their wrong counterparts were used to tune
the cue lists, and the held-out half has since informed two more rules, so
these rates describe the corpora, not unseen replies. On first contact the
held-out half scored 195 of 210 correct replies passing and 123 of 126 wrong
replies rejected. Fresh evidence comes from `deploy/pilot/`, where an agent
attempts the task without seeing the contract: [production.md](production.md#what-has-been-checked)
has the latest counts.

**Wrong replies that pass** hold every right value, attached to the wrong thing
or dismissed. The kinds, each a `style` in the fixtures:

- Role swaps with every name present: "The author is ines-d, the approver is
  riley-p and the merger is mateoalv." Roles among declared facts are caught
  only by `order` or a `rejected` form.
- A dismissal the cue lists do not know: "checkout-7b9d6c5f4-xk2lp is fine.
  Cordon checkout-7b9d6c5f4-m8vqt."
- A plain statement of a wrong record followed by the right one in concluding
  words: "Production serves 7d41c9e (…). Before that it served c3a9f12 (…)."
  A record stated without strong or opening wording is only named in passing.
- A wrong number, amount or date first, with the right one after it or in a
  parenthesis: "December 1, 2026; verify Ferrowave-Signature-V2. (The signature
  guide mentions January 15, 2027.)", "The MRR is $439.00 or $339.00." Another
  record's measured value competes only by strong wording and is never a list.
- A rival in a sentence that holds a set-aside word. `failed` and `errored` are
  set-aside words and also the vocabulary of the CI tasks, so "The most recent
  failed pipeline on main is 5511" is not read as a claim.
- A negated verb before a number, amount or date: "Production did not reach
  1843 ms at 14:35 UTC." Only a negator that governs a measured value directly
  ("was not 1843 ms") denies it.
- The opposite predicate about the right values, outside the contract's
  `contradictions` ("eu-west-1 wins on speed"), in a label or table header
  (`"fastest_region"`), or as a symbol (`!=`). The reader binds values and
  polarity, not arbitrary predicates.
- A wrong detail the contract does not require: a fabricated or stale optional
  fact, other seconds after the right minute of a clock fact (`16:42:00` for
  `16:42:57`), or an extra item after a complete list.

Limits of notation:

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

- `natural_answers.rs` grades the three corpora under `tests/fixtures/` for the
  `record_facts` scenarios and prints one summary line per directory. An entry
  is `{"style", "expect", "answer"}`; `expect` is `pass` or `reject` (fail or
  unresolved). An entry whose verdict disagrees with `expect` carries
  `"known": "<verdict>"`, and the test requires exactly that verdict: a change
  to the reader that fixes or breaks a known limit fails until the fixture is
  updated. `"edit": {"<json pointer>": <value>}` replaces authored source values
  before the reply is graded, for a reply that is wrong only once the source has
  changed.
- `answer_phrasing_*.rs` and `statement_answers_*.rs` hold per-scenario cases:
  alternative phrasings that pass; denied, wrong-record, role-swapped and stale
  replies that earn no reward; a missing source as an environment error; and
  data edits showing the fact follows the source. A reply the reader misreads
  belongs in a corpus with `known`, not in these files.
- `bench-answer-probe SCENARIOS_ROOT CORPUS_JSON` grades a corpus file from the
  command line. It prints one JSON line per reply (`verdict`, `agrees` with
  `expect`, `recorded` when the verdict is what the entry records) and a summary
  on stderr, and exits non-zero when a verdict is not as recorded.
