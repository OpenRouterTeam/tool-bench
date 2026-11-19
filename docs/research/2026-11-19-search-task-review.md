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

## One task changed in place: the Firecrawl crawl

`search/firecrawl/crawl-api-reference` asks for a Firecrawl crawl of
`https://docs.lumenpay.dev` that collects only the pages under `/api/`. In the round it
failed in all seven cells with the whole tool list sent, and passed 6 of 27 scored
trials of the arms that hide the tool servers behind a search tool. Six of the seven
whole-list failures were on records, history and preservation together; the other was
on records alone.

What the sources showed, by running each natural set of crawl arguments against the
in-process world:

- The fixture had no sitemap and no page at `/api/`. The site root with an `/api/`
  filter collects nothing, because the mock, like the vendor, tests the start URL
  against `includePaths`. The section path collects nothing either, because it is not a
  page. Those are the two crawls the request describes.
- One seed passed: `/api/overview`, the only page that links to the other four, with
  `crawlEntireDomain` set. The request names no such page. The tool schema describes
  none of the crawl arguments, and that one's name says the opposite of "only the API
  section".
- The contract allows three crawls that collected nothing. It does not allow a second
  crawl that collected a page, and no tool removes a crawl. The step between an empty
  crawl and the right one, the `/api/overview` seed without `crawlEntireDomain`,
  collects one page. The right crawl after it fails on records, history and
  preservation: the signature of six of the seven failures.

The fix is in the dataset: the docs site publishes a sitemap
(`dataset/firecrawl/sitemap.json`), as a documentation site does, and the mock already
reads one. The site root with an `/api/` filter, the site root with the rest excluded,
and the section path now collect the five pages. The instruction, the contract and the
mock are unchanged.

What still fails: a crawl with no filter, one that keeps the home page, one whose page
limit truncates the section, a seed on one page without sibling traversal, a seed on
the marketing site, and a second crawl that collected any page.

One strictness is kept and is a judgment: a crawl that collected a page is final,
because the agent has no tool to remove it, so one wrongly scoped attempt fails the
trial even when the next crawl is right.

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
- `crates/world/tests/firecrawl_contracts.rs` holds the crawl task: the crawls the
  request describes pass, part of the section fails, and a whole-domain crawl from an
  API page fails. `deploy/test-firecrawl-business-audit.sh`, an independent reading of
  the same fixtures, agrees on all 18.
- `taskgen` validates the scenarios and their reference calls against the locked tool
  snapshots, and `release-manifest --check` passes for release 2.
