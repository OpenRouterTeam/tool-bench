# Archived tasks of release `tool-bench-public-dev-1`

The eleven search tasks below were scored in release 1 and are not scored in release 2
(`releases/tool-bench-public-dev-2.json`, frozen 2026-11-19). Their sources are kept here,
unchanged, so results of a round run on release 1 can still be read against the
instruction, the contract and the dataset they were graded with.

Nothing loads this directory. It is outside `scenarios/`, so `taskgen` writes no task
for it, the world image does not carry it, and no test reads it except the one that
checks every task release 1 lost has its source here.

## Why they were replaced

Each instruction asks for a kind of lookup and names no search vendor ("check the
news", "look it up on the web"). Four equivalent web-search servers are connected
(Brave, Exa, Firecrawl, Tavily). Release 1 kept each task's pages on one vendor, so
the other three returned nothing, and ten of the eleven accepted that vendor only
(a `required_calls` entry or an `information` witness). The tasks therefore measured
two things at once: whether the agent looks for a connected search server the
prompt does not name, and whether it lands on the vendor that was given the data.
Release 2 keeps the first and removes the second.

| Archived (release 1) | Replaced by (release 2) |
|---|---|
| `search/brave/competitor-funding-news` | `search/brave/funding-news-any-search` |
| `search/brave/hosting-sla-lookup` | `search/brave/hosting-sla-any-search` |
| `search/brave/keynote-recording` | `search/brave/keynote-any-search` |
| `search/exa/find-vp-engineering-profile` | `search/exa/vp-profile-any-search` |
| `search/exa/sdk-timeout-default` | `search/exa/sdk-timeout-any-search` |
| `search/firecrawl/northwind-team-pricing` | `search/firecrawl/team-pricing-any-search` |
| `search/tavily/vendor-rate-limit` | `search/tavily/vendor-rate-limit-any-search` |
| `search/tavily/webhook-signature-sunset` | `search/tavily/webhook-sunset-any-search` |
| `search/multi/competitor-funding-and-pricing` | `search/multi/funding-and-pricing-any-search` |
| `search/multi/competitor-research-github-issue` | `search/multi/github-issue-any-search` |
| `search/multi/vendor-rate-limit-zendesk-reply` | `search/multi/zendesk-reply-any-search` |

A replacement keeps the instruction word for word and the original vendor's dataset
as it was; it adds the same pages to the other three search services and accepts a
read through any of them. Each replacement's `scenario.toml` opens with a note.

## Running or grading release 1

Release 1 as a whole, with these eleven as generated Harbor tasks under `tasks/`, is
the repository at commit `5b2cf1a6a296597aa2ce1d8b17fe46fe70d6d3ca`. Its tasks name
the world image built from that commit. A harness pins it by passing a branch or tag
at that commit as `datasetRef` ([production.md](../../../docs/production.md#comparing-arms)).

## What can still be compared across the two releases

- The 125 tasks outside the search group are frozen identically in both.
- The 14 search tasks kept in release 2 have the same instruction, dataset and
  contract; only a `route:*` tag was added, which moves their scenario-source hash.
- The eleven replaced tasks are different tasks under different names. Do not pool
  their results with their originals'.
