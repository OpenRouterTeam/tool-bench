# Fairness: what a scored task must meet

tool-bench compares ways of serving one large tool set on identical tasks. A task is
fair when its score depends on what the agent did for the user and on nothing about
how the tools reached the agent. This page lists the rules a scored task must meet,
the check that enforces each, and the rules that have no check yet. The grading rules
themselves are in [contracts.md](contracts.md) and
[answer-fact-contracts.md](answer-fact-contracts.md).

## What fair means

1. **Solvable as written.** An agent that does what the instruction asks, by any
   ordinary route through the connected servers, scores 1.
2. **Not passable otherwise.** An agent that does nothing, guesses, answers from a
   look-alike record or changes the wrong thing scores 0.
3. **Neutral between arms.** The data is as reachable, and costs as many calls to
   reach, whether every definition is sent, the client searches or the router defers.

A task that fails rule 1 measures the author's route. A task that fails rule 3
measures where the data was filed.

## Rules

| # | Rule | Check | Status |
|---|---|---|---|
| F1 | The reference solution scores 1 and the do-nothing agent scores 0 | `deploy/stock-campaign.sh` | Run by hand against the images; not in CI |
| F2 | The instruction names no tool, API path or data location a real user would not name | None | Authoring rule (`CONTRIBUTING.md`) |
| F3 | Every task has a route label, and the label is what its instruction says | `taskgen` | Enforced |
| F4 | Where interchangeable services are connected, every one leads to a pass, or the instruction names one | `taskgen` | Enforced for new and changed tasks; 9 tasks of release 2 are listed as known |
| F5 | The contract takes no single vendor the instruction does not name | `taskgen`, with F4 | Enforced, as F4 |
| F6 | Each accepted route reaches a pass | `crates/world/tests/fairness_routes.rs` | Enforced in process; the Docker oracle still runs one route |
| F7 | The same facts are on every accepted route | `crates/world/tests/fairness_routes.rs` | Enforced for the facts; look-alikes are not compared |
| F8 | No accepted answer value appears in the instruction | None | Not built |
| F9 | A reply with a look-alike's values scores 0, and so does a correct reply with no call where the facts are invented | Reply corpora | Corpora enforced; no scripted no-call agent |
| F10 | An independent valid alternative passes and a near miss fails | `catalog/validation-evidence/` | Declared for 35 of 150; the tests are found, not run |
| F11 | Exact counts, history and preservation checks follow from the instruction | Review | Review only |
| F12 | The reference solution finishes inside the time budget with slack | None | Not built |
| F13 | The facts are invented, so no source outside the connected servers holds them | Review | Authoring rule (`CONTRIBUTING.md`, "Datasets are invented") |

Rules F2, F8 and F12 have no check. F9's no-call agent and F6 through the real MCP
servers are not built. F11 and F13 rest on review.

### F3. Route labels

Every scenario has one of three labels, by what its instruction says about the route
to the services it needs:

| Label | The instruction | Example |
|---|---|---|
| `route:named` | Names each product | "in our GitHub Enterprise" |
| `route:kind` | Names a kind of thing only one connected service has | "current local business listing data" |
| `route:unnamed` | Names neither | "look it up on the web" |

The label is derived, not declared. `catalog/equivalence.toml` holds, for each of the
30 services, the product names an instruction would write (`names`, matched with
their capitals) and the words for a kind of thing no other connected service has
(`only`, matched in any case). `taskgen` reads the instruction for each service the
scenario lists; the task's label is the weakest of its services'. A task that names
PagerDuty and "the `payments` namespace" is `route:kind`.

`taskgen --inventory` writes the label of every scenario to
[coverage/route-labels.json](coverage/route-labels.json). A scenario may carry its
label as a `route:` tag; a tag that disagrees with the instruction fails `taskgen`.
The label is never shown to the agent.

### F4 and F5. Interchangeable services

`catalog/equivalence.toml` also lists classes: connected services that can do the
same kind of job, such as the four web-search servers, the two CRMs, or the services
that hold a support ticket. Each class has the words an instruction uses for the job
when it names no product (`kind_words`).

For each service of a scenario that the instruction neither names nor points at, the
check takes the classes that service belongs to whose kind words the instruction
uses, or all of its classes when it uses none. The scenario passes a class when all
of these hold:

- it serves records on every member (`services` lists each and `dataset/<member>/`
  is not empty);
- no `required_calls` entry names a member, since an entry takes one service and
  every entry must hold;
- every `information` read that names a member accepts, through `alternatives`, a
  read of each;
- no state, record or relation check reads one member's records, since the same
  change on another member would then fail.

A scored scenario that fails a class fails `taskgen`. The message names the service,
the class and what is missing. The two repairs are to name the product in the
instruction, or to serve the records on each member and accept each.

A task filed under `scenarios/<group>/<service>/` does not inherit that service as its
only route. The directory is where the source lives.

A write cannot accept a class today: `required_calls` has no form that takes a list
of services, and a record check reads one service's records. Until it can, a task
that changes something names its product.

### Known tasks

A scenario's source hash is frozen in the release manifest, so a task of a published
release that fails the check cannot be repaired in place. Such tasks are listed
under `[known]` in `catalog/equivalence.toml`, with the release they are frozen in.
A listed task is reported and not refused. A task leaves the list when its repaired
version is in `scenarios/`; `crates/scenario/tests/fairness.rs` fails when the list
and the flagged tasks differ, so the list cannot hold a task that passes or miss one
that fails.

[coverage/fairness-admission.json](coverage/fairness-admission.json) is the check's
report: for every scenario its label, the words that decided it, and for each class
held against it the members and what keeps one from being a route to a pass.
`flagged` lists the scored tasks that fail.

Release 1 held eleven search tasks that named no vendor and kept their pages on one;
the check flags each of them where they are archived
(`releases/archive/tool-bench-public-dev-1/`). Release 2 replaced them with tasks that
accept any of the four search services. What is still listed: eight `business` tasks
that say "our CRM", "support ticket", "our helpdesk" or "the store", and
`search/multi/zendesk-reply-any-search`, which accepts any search service and says
"support ticket" without naming Zendesk.

### F6 and F7. Every accepted route

`crates/world/tests/fairness_routes.rs` finds each scenario that accepts a class and
runs it once per member against the in-process world. A class declares how each
member's MCP server asks for its job (`[[class.read.<member>]]`, one HTTP request per
tool that can do it); the test issues those requests with the reference solution's
own query, or with the query listed under `[class.queries]` when the reference
solution does not search. For each member:

- the response holds the page the answer contract selects and every fact a reply
  must state, or, for a task that acts on what it read, the excerpts its
  `information` check asks of that member;
- for a lookup, the reference reply then passes, with history and preservation
  intact;
- a read with no reply fails, and so does doing nothing.

Left to a scenario's own contract test: a fact read from a second page, a fact the
grader derives from prose with no capture pattern, and the pass of a task that also
changes something.

The Docker oracle (`deploy/stock-campaign.sh`) still replays one `[[solution.call]]`
list, so it proves one route through the real MCP servers. A scenario has no form
for one scripted solution per member.

### F9 and F13. What a correct agent cannot do

The companies, people and figures are invented. A model's own knowledge and the
public web cannot supply them, so a correct reply shows the records were read. A
reply that states a look-alike's values is wrong however it was reached.

A task that accepts a class has no required call, so in the in-process world the
reference reply passes with no call at all. That the facts are invented is what
stands in for the missing rule; F9's scripted no-call agent would not change it.

### F11. Exact checks

`equals = 1`, `audit_history = true` and the preservation allow-list each fail a
trial for something beyond the requested change. Each needs a phrase in the
instruction that asks for it ("open a new issue", "don't change any existing
tickets"). A side effect of a correct call, such as a field the service updates on
its own, belongs in the allow-list.

## What is not unfair

- **Scoring 0 for an agent that never looks at the connected servers.** The servers
  hold the answer and the agent holds a way to find them.
- **Scoring 0 for an answer taken from a built-in web search.** The facts are not on
  the public web.
- **A task that is hard.** A vendor quirk the reference solution works around is
  fair if the instruction and the tool's own description are enough to find it.
- **A task with one route**, when the instruction names it or only one service can do
  the job.

## Reports

`bench-report` reads the labels apart. Each arm has a table of pass rate by route
label, and each paired comparison has the mean per-task pass rate of both arms, the
difference and its paired bootstrap interval within each label. So "finds a tool the
instruction does not name" is the `route:unnamed` row, with its own interval. The
report uses the labels as they stood when the tool was built; a task it has no label
for is shown as `unlabelled`.

## Admission

A scenario enters the scored set when F1 to F13 hold. `taskgen` runs F3 to F5 on
every run and refuses a scored scenario that fails. `cargo test -p world --test
fairness_routes` runs F6 and F7. `deploy/stock-campaign.sh` runs F1. F10 is declared
in `catalog/validation-evidence/`. F2, F11 and F13 are reviewed.

A scenario that fails a rule can still run in the unscored `diagnostic/` group, where
the check reports it and does not refuse it.

## When a scored task turns out unfair

1. Record the defect: the rule it breaks and the trials that show it.
2. Repair the task, keeping the instruction and the business dates
   (`CONTRIBUTING.md`), unless the instruction is the defect.
3. Freeze a new release. A repaired task has a new source hash, and results before
   and after are not pooled.
4. Results on the old release keep their label. They are not rescored.

A task that no arm passes in a campaign, or that every arm passes, is read against
these rules before the next release.

## Limits

- The classes in `catalog/equivalence.toml` are a reviewed judgment about which
  services can do the same job. A class that is missing hides the defect F4 exists
  to catch; a member that does not belong makes the check ask for a product name a
  user would give anyway.
- F3 to F5 match whole words of the instruction. They catch a product name or a kind
  word, not a hint in other words, and a word listed under `only` is trusted to
  belong to one service.
- A service the scenario lists is taken as one the task needs. A scenario that
  lists a service only for its bystander records is read as needing it.
- F7 compares the page and the facts on each route. It does not compare the
  look-alike records, so a route could hold fewer distractors.
- These rules make a task fair between arms. They do not make the mock a copy of the
  hosted service; see [operation-support.md](operation-support.md).
