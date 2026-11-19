# Search task review and release 2 (2026-11-19)

A review of the 25 `search/*` tasks after the round of 19 November 2026, in which an arm
that hides the tool servers behind a search tool lost on these tasks and nowhere else.
Almost all of that loss is on the prompts that name no search vendor. This note records
what the review found in the task sources and what release `tool-bench-public-dev-2`
changes because of it.

## What the sources showed

- Twelve instructions name no search vendor ("check the news", "look it up on the
  web", "according to Quillstream's own current documentation"). Four web-search servers
  are connected (Brave, Exa, Firecrawl, Tavily), and so is the agent's own web search.
- Each of those tasks kept its pages in one vendor's dataset; the other three connected
  search servers held no pages for it. `search/multi/competitor-funding-and-pricing`
  split them: the news on Brave alone and the pricing page on Firecrawl alone.
- Nine of the twelve declared a `required_calls` entry for that vendor, and two an
  `information` witness that only that vendor's response could satisfy.
- The pages describe invented companies and exist on no public site, so the agent's own
  web search cannot return them. Nothing in the instruction says so.
- `interfaces = "mcp_only"` is not what fails a built-in web search: the agent container
  has no route to the world, so such a search goes to the public web and finds nothing.
- With every tool sent, passing trials got past the placement by trying the search
  servers in turn: 23 of 28 such trials with a recorded tool sequence used two or more
  search vendors.

So these tasks measured two things at once: whether the agent looks for a connected
search server the prompt does not name, and whether it lands on the one vendor that was
given the data. The first is worth measuring. The second followed from the directory a
task was filed under.

One of the twelve is kept as it is: `search/brave/caterer-saturday-hours` asks for
"current local business listing data", which one connected server offers.

## What release 2 changes

Eleven tasks are replaced; the mapping is in
`releases/archive/tool-bench-public-dev-1/README.md`, with the originals' sources. In
each replacement:

- the instruction is the original, word for word;
- the original vendor's dataset is kept as it was, so the answer contract reads the same
  records with the same patterns;
- the same pages, look-alikes and superseded pages included, are written in the dataset
  shape of the other three search services. A video result is served elsewhere as its
  watch page, with the channel and duration that page shows;
- the single-vendor `required_calls` entry is removed (eight tasks), and the two
  `information` witnesses gain `alternatives`, so the page read through any of the four
  services is the evidence the write needs.

The second half of the three cross-service tasks is unchanged. The Zendesk reply and
the GitHub issue keep their state, record, relation, text and preservation checks and
the rule that the write follows a response that carried the facts. The funding brief
keeps its answer contract: the price still has to come from the company's own pricing
page.

What still fails: no reply, a reply with a look-alike or superseded page's values, a
write with no read that carried the page, and any answer taken from outside the tool
servers, because the facts exist nowhere else.

### Two limits

- The eight lookups whose required call was removed no longer state that a search call
  was made. `required_calls` takes one service per entry and every entry must hold, so
  "any one of four services" cannot be written. The answer check carries the route, as
  it did in release 1 for the Northwind pricing task. A form that accepts any one of
  several matchers would restore the explicit check; it needs a grader change and a new
  evaluator version.
- On the two tasks with a witness, a Brave web search filtered to exclude web results,
  or a Firecrawl search that asks only for news or images, returns no web result list.
  Made before the write, the grader reads such a response as an environment fault,
  not as a pass or a fail (`a_search_without_a_web_result_list_before_the_write_is_not_assessed`).
  Tavily and Exa responses always carry the list, so release 1 had no such case.

## Route label

Every search task carries one tag, by what its instruction says about the route:

- `route:named` (13 tasks): the instruction names the vendor or the system ("in
  Elasticsearch", "a Firecrawl monitor").
- `route:kind` (1): the instruction names a kind of data one connected server offers.
- `route:unnamed` (11): the instruction names neither; several connected servers, and
  the agent's own tools, fit its wording.

A report can group on the tag to read "finds a tool the prompt does not name" apart
from the rest. The tag appears in each task's `task.toml` keywords.

## Validation

- `crates/world/tests/any_search_vendor_contracts.rs`, against the in-process world:
  for each of the nine lookups, each of the four services returns the page and the
  passage that carries the fact, the reference reply passes after a search on any of
  them, and a look-alike page's values fail on every route. For the two tasks that
  write, the page read through Tavily, Exa, Brave (two ways) or Firecrawl authorizes
  the write; a read that returned only the look-alike's page does not; a superseded
  figure fails after a valid read.
- The contract tests of the two write tasks
  (`remaining_cross_service_research_contracts.rs`) and the reply corpora run unchanged
  against the replacements.
- `catalog/validation-evidence/search.toml` declares a valid alternative and a near
  miss for each of the eleven.
- `taskgen` validates the scenarios and their reference calls against the locked tool
  snapshots, and `release-manifest --check` passes for release 2.
