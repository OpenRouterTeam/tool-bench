# Search task review and refined variants (2026-11-19)

A review of the 25 `search/*` tasks after the round of 19 November 2026, in which an arm
that hides the tool servers behind a search tool lost on these tasks and nowhere else.
Almost all of that loss is on the prompts that name no tool server. This note records
what the review found in the task sources and what the five refined variants change. The
150 scored tasks are unchanged.

## What the sources show

- Twelve instructions name no search vendor ("check the news", "look it up on the
  web", "according to Quillstream's own current documentation"). Four web-search servers
  are connected (Brave, Exa, Firecrawl, Tavily), and so is the agent's own web search.
- Each of those tasks keeps its pages in one vendor's dataset; the other three connected
  search servers hold no pages for it. `search/multi/competitor-funding-and-pricing`
  splits them: the news is on Brave alone and the pricing page on Firecrawl alone.
- Nine of the twelve declare a `required_calls` entry for that vendor, and two an
  `information` witness that only that vendor's response can satisfy
  (`search/multi/vendor-rate-limit-zendesk-reply`, Tavily;
  `search/multi/competitor-research-github-issue`, Exa). The entry is redundant while
  only that vendor holds the pages, and it would reject a correct answer from another
  connected server if the pages were shared.
- The pages describe invented companies and exist on no public site, so the agent's own
  web search cannot return them. Nothing in the instruction says so.
- `interfaces = "mcp_only"` is not what fails a built-in web search: the agent container
  has no route to the world, so such a search goes to the public web and finds nothing.

So these tasks measure two things at once: whether the agent looks for a connected
search server the prompt does not name, and whether it lands on the one vendor that was
given the data.

## Refined variants

`scenarios/diagnostic/multi/*-any-search`, generated from the originals. In each one the
instruction is the original word for word; the original vendor's dataset is copied byte
for byte, so the answer contract reads the same record with the same patterns; the same
pages, look-alikes included, are written in the dataset shape of the other three
web-search services; and the single-vendor required call is removed.

| Refined task | Original | Original vendor |
|---|---|---|
| `diagnostic/multi/hosting-sla-any-search` | `search/brave/hosting-sla-lookup` | Brave |
| `diagnostic/multi/funding-news-any-search` | `search/brave/competitor-funding-news` | Brave |
| `diagnostic/multi/vendor-rate-limit-any-search` | `search/tavily/vendor-rate-limit` | Tavily |
| `diagnostic/multi/sdk-timeout-any-search` | `search/exa/sdk-timeout-default` | Exa |
| `diagnostic/multi/team-pricing-any-search` | `search/firecrawl/northwind-team-pricing` | Firecrawl |

What still fails: no reply, a reply with a superseded or look-alike page's values, and
any answer taken from outside the tool servers, because the facts exist nowhere else.

What the contract no longer states: that a search call was made. `required_calls` takes
one service per entry and every entry must hold, so "any one of four services" cannot be
written. The original `search/firecrawl/northwind-team-pricing` already relies on the
answer check alone. A `required_calls` form that accepts any one of several matchers
would restore the explicit check; it needs a grader change and a new evaluator version.

They sit in the unscored `diagnostic/` group because the scored suite is frozen at 150
tasks by the release manifest. `taskgen --include-examples --out <dir>` writes them as
Harbor tasks.

## Proposed route label

So a report can read "finds a tool the prompt does not name" apart from the rest, each
search task gets one of three values, by what its instruction says about the route:

- `route:named`: the instruction names the vendor or the system ("in Elasticsearch",
  "a Firecrawl monitor").
- `route:kind`: the instruction names a kind of data that one connected server offers
  ("current local business listing data").
- `route:unnamed`: the instruction names neither; several connected servers, and the
  agent's own tools, fit its wording.

The refined variants carry the value as a scenario tag. The scored tasks are not
retagged here, because a scenario's source hash is frozen in the release manifest;
`docs/coverage/search-route-labels.json` holds the value for all 30 tasks.

## Validation

`crates/world/tests/refined_search_contracts.rs` runs each refined task against the
in-process world: each of the four services returns the page and the passage that
carries the fact; the reference reply passes after a search on any of them; a
look-alike page's values fail on every route; no reply fails. The natural-answer corpus
holds the originals' 40 replies under the new IDs (25 that must pass, 15 that must be
rejected), and each earns the verdict it earns on the original. `taskgen` validates the
five scenarios and their reference calls against the locked tool snapshots. No Docker
oracle run and no model trial was made.
