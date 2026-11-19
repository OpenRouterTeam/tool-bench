# Archived tasks of release `tool-bench-public-dev-1`

The eleven search tasks below were scored in release 1 and are not scored in release 2
(`releases/tool-bench-public-dev-2.json`, frozen 2026-11-19). Their sources are kept here,
unchanged, so results of a round run on release 1 can still be read against the
instruction, the contract and the dataset they were graded with.

Nothing loads this directory. It is outside `scenarios/`, so `taskgen` writes no task
for it, the world image does not carry it, and no test reads it except the one that
compares the two releases (`crates/scenario/tests/release_manifest.rs`).

`instruction-sha256.json` records the instruction hash of each of release 1's 150 tasks,
taken from `docs/coverage/scenario-inventory.json` at release 1's commit. A release
manifest freezes a task's `scenario.toml`, dataset and contract, not its instruction,
so this file is what lets that test see an instruction that changed.

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
read through any of them. One instruction differs by a word:
`search/multi/zendesk-reply-any-search` says "an open Zendesk ticket" where the original
said "an open support ticket", because four connected services can hold a support
ticket and the ticket half accepts Zendesk only. Each replacement's `scenario.toml`
opens with a note.

## Running or grading release 1

Release 1 as a whole, with these eleven as generated Harbor tasks under `tasks/`, is
the repository at commit `5b2cf1a6a296597aa2ce1d8b17fe46fe70d6d3ca`. Its tasks name
the world image built from that commit. A harness pins it by passing a branch or tag
at that commit as `datasetRef` ([production.md](../../../docs/production.md#comparing-arms)).

## Tasks changed in place

A task that keeps its name and differs from release 1 in instruction, dataset or
contract is listed in `CHANGED_IN_PLACE` in `crates/scenario/tests/release_manifest.rs`,
and the test fails when the list and the sources disagree. There are 47. Their
release 1 sources are the repository at the commit above.

| Group | Changed | Of |
|---|---|---|
| business | 11 | 25 |
| cloud | 6 | 25 |
| code | 10 | 24 |
| collab | 4 | 25 |
| ops | 13 | 26 |
| search | 3, beside the eleven replaced | 25 |

What changed, by kind. A task can appear under more than one.

- **A forbidden call names the API it guards, not a whole method (33 tasks, contract).**
  A ban on every `POST` to a service also failed reads that use `POST`, such as
  GitHub's GraphQL. Four of the 33 held such a ban; the other 29 are scoped the same
  way so the rule is uniform.
  business: `hubspot/largest-open-deal`, `multi/hubspot-largest-deal-salesforce-owner`.
  cloud: `aws/find-unencrypted-bucket`, `multi/aws-jira-decommission-worker`,
  `multi/aws-replica-lag-pagerduty-notion`, `okta/failed-signins-report`,
  `supabase/top-customer-march-revenue`.
  code: `argocd/find-degraded-app`, `argocd/restart-indexer-worker`,
  `argocd/roll-back-billing-worker`, `argocd/sync-prod-payments-api`,
  `gitlab/diagnose-failed-pipeline`, `multi/argocd-degraded-gitlab-commit`,
  `multi/sentry-spike-vercel-rollback`, `multi/vercel-deploy-github-pr-lookup`,
  `terraform/payments-db-upgrade-failure`, `terraform/unlock-orders-prod`.
  collab: `jira/blocked-sprint-item`, `multi/jira-sentry-crash-impact`,
  `multi/slack-pagerduty-dbre-oncall`, `notion/launch-date-owner`.
  ops: `datadog/schedule-replica-downtime`, `grafana/annotate-checkout-deploy`,
  `grafana/silence-replica-maintenance`, `grafana/top-5xx-checkout-pod`,
  `kubernetes/crashloop-root-cause`, `multi/crashloop-oncall-lookup`,
  `multi/datadog-latency-argocd-rollback`, `multi/sentry-release-commit-lookup`,
  `pagerduty/payments-oncall-now`, `sentry/top-unresolved-checkout`.
  search: `exa/enrich-lead-list`, `multi/api-key-deletion-audit-github`.
- **The prompt names the system it means (8 tasks, instruction).** "Our CRM", "support
  ticket", "our helpdesk" and "the store" each fit more than one connected service,
  and the data is on one.
  business: `hubspot/largest-open-deal`, `hubspot/log-call-note-initech`,
  `multi/zendesk-order-status-shopify`, `multi/zendesk-refund-via-stripe`,
  `zendesk/create-ticket-for-caller`, `zendesk/escalate-outage-ticket`,
  `zendesk/find-invoice-attachment`, `zendesk/reply-refund-and-solve`.
- **The prompt states the date (8 tasks, instruction).** The agent's clock is the day of
  the run and the world's is the scenario's, so "tonight" and "tomorrow" pointed at a
  window that had passed.
  ops: `datadog/schedule-replica-downtime`, `grafana/silence-replica-maintenance`,
  `multi/crashloop-oncall-lookup`, `multi/datadog-latency-argocd-rollback`,
  `pagerduty/cover-shift-override`, `pagerduty/payments-oncall-now`,
  `pagerduty/reassign-replication-lag`.
  collab: `multi/slack-pagerduty-dbre-oncall`.
- **One task each.**
  `business/hubspot/dana-joins-pied-piper`: the contract allows the contact's primary
  company property when it is the Pied Piper company, and the prompt says her Hooli
  association stays.
  `business/shopify/relaunch-tote`: the prompt states that the product is not yet
  published to the Online Store, which no read tool shows.
  `cloud/mongodb/refund-order`: the contract accepts an optional refund date on the
  scenario's day.
  `ops/multi/pagerduty-sentry-github-issue`: a title that starts with the short ID and
  any separator is the title asked for.
  `search/firecrawl/crawl-api-reference`: the dataset gains the docs site's sitemap.
  Without one, and with no page at `/api/`, the two crawls the request describes
  collected nothing, and the only seed that passed was a page the request does not
  name with `crawlEntireDomain` set.

Two more tasks behave differently with no change to their sources:
`cloud/okta/offboard-contractor` and `cloud/multi/okta-offboard-linear-slack`. The
pinned Okta server asks for confirmation before it deactivates a user when the client
says it can show a prompt, and a trial has nobody to answer. The bridge in front of
each server no longer passes that capability on.

## What can still be compared across the two releases

- A task both releases score that is not listed above has the same instruction,
  dataset and contract in both: 92 tasks, two of which are the Okta tasks above. The
  search tasks kept in release 2 gained a `route:*` tag, which moves their
  scenario-source hash and nothing else.
- A task listed above is a different version under the same name. Do not pool its
  results across the releases.
- The eleven replaced tasks are different tasks under different names. Do not pool
  their results with their originals'.
