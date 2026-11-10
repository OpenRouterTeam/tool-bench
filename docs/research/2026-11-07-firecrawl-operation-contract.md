# Firecrawl WRITE operation contracts — 2026-11-07

## Scope and evidence discipline

Research-only sidecar for `search/firecrawl/{crawl-api-reference,monitor-competitor-pricing,pause-broken-pricing-monitor}`. Only this note is authored. Instructions, datasets, scenarios, generated tasks, implementations, staged/unstaged work, and frozen capsules/tags are left unchanged. No builds, tests, installations, Docker, live accounts, infrastructure actions, evaluated-tool calls, paid/model inference, commits, branches, or delegation were used. Public documentation, schemas, package archives, and source were read; downloaded archives were decompressed and hashed in memory, not installed or executed.

Evidence labels:

- **[D] Documented vendor fact:** Firecrawl-owned documentation/OpenAPI, retrieved for this note. Mutable documentation is not a hosted-server revision pin.
- **[P] Pinned publisher implementation:** exact published MCP/SDK bytes, or source at the npm provenance revision. The monorepo's API source at that revision is separately identified; it is **not** a hosted-deployment pin.
- **[R] Replica/fixture convention:** local authored instructions, data, catalog, snapshot, Rust service, generated oracle, or evaluator. Not a vendor guarantee.
- **[I] Inference/proposal:** static counterexample, causal interpretation, or recommended contract. No counterexample was executed.
- **[U] Unestablished:** requires runtime, account, delivery, deployment, or independent qualification evidence.

The note uses the requested November 7, 2026 research date. Business clocks remain the authored clocks:

| Scenario | Business reference, UTC | Requested outcome |
| --- | --- | --- |
| `crawl-api-reference` | October 29, 2026, 10:00 | Start a crawl collecting only LumenPay's API-reference pages; no summary requested. |
| `monitor-competitor-pricing` | October 28, 2026, 16:30 | Create a new daily Fabrikam pricing monitor, with price/plan-limit email alerts; preserve existing monitors. |
| `pause-broken-pricing-monitor` | October 29, 2026, 15:00 | Identify and pause the repeatedly failing old Tailspin pricing monitor, then initiate a check of the new US plans monitor; preserve everything else. |

Thus “today” in the Tailspin task means **October 29**, not the research date. **[R]**[^r-crawl][^r-create][^r-pause]

**Research is not qualification.** The existing ledger records 129 v2 / **21 remaining legacy write tasks**, and all three researched Firecrawl tasks are still version 1. This note closes none of the full **19-item overhaul**, operation-support, native-runtime, human-review, awareness, or release gates. **[R/I]**[^r-ledger][^r-generated]

## Findings that should change contract design

1. **Crawl admission, wrapper return, and coverage are different facts.** REST POST returns a job identity and polling URL; the selected ordinary MCP tool then polls to a terminal status. `failed` and `cancelled` are terminal too. Neither a local `completed` record nor equal counters establishes hosted coverage. **[D/P/R/I]**[^d-crawl][^p-server][^r-service]
2. **The authored crawl plan is locally successful by construction, not universally documented discovery.** The replica filters the entire fixture page collection, independently of links and sitemaps. Current official docs warn that excluding the starting URL with `includePaths` can yield zero pages. A matching API-page seed plus bounded sibling traversal is another reachable plan; do not require the reference root seed or exact regex spelling. **[D/P/R/I/U]**[^d-crawl][^d-schema][^p-server][^r-service][^r-crawl-data]
3. **`intervalMinutes:1440` is local, not the vendor schedule contract.** Vendor responses normalize schedules to cron/timezone. The local parser even labels some twice-daily and weekday-only cron expressions as 1440 minutes. Validate actual daily recurrence, not this scalar or an exact next-run timestamp. **[D/P/R/I]**[^d-monitor][^d-schema][^p-cron][^r-text]
4. **Configuring an email recipient is not enabling delivery to that person.** Vendor documentation requires external recipients to confirm; the pinned SDK exposes subscription status. The local service merely stores notification JSON and implements neither consent nor delivery. Do not claim an email arrived, or require consent outside the available surface as if the agent could provide it. **[D/P/R/I/U]**[^d-monitor][^p-types][^p-monitor-controller][^r-service]
5. **Pause and run have distinct identities and lifecycle.** Pause modifies a monitor; run returns a new **check**, not a new monitor or fresh data. The replica returns `queued` while already storing a terminal check synchronously. Publisher API source rejects manual runs of paused monitors and overlapping checks with 409; the replica has neither guard. **[D/P/R/I]**[^d-schema][^p-monitor-controller][^r-service]
6. **The current checks admit wrong business outcomes and reject some equivalent ones.** Concrete native-expressible examples include mixed API/guide crawls, disabled-email monitors, twice-daily schedules, target-monitor collateral changes, extra runs, and case-equivalent URL spellings rejected by substring checks. These are static objections, **not passed or failed controls**. **[P/R/I]**[^r-crawl][^r-create][^r-pause][^r-grade][^p-monitor]

## 1. Exact pins, provenance, and native boundary

### Selected condition

| Layer | Selection | What it establishes |
| --- | --- | --- |
| Catalog | Firecrawl `mode="real"`, world port `8102`, MCP port `7102`, npm command `firecrawl-mcp` | Real publisher client is intended to call the local HTTP service, not Firecrawl's hosted API. |
| MCP package | `firecrawl-mcp@3.36.0`, `dist/index.js` | Published implementation inspected directly; package integrity matches the selected npm lock. |
| MCP source association | `firecrawl/firecrawl-mcp-server@965456eb17bd28ad200bcbaac94159b000bd2b52` | npm SLSA provenance payload identifies this revision on `refs/heads/main`; not a capture-time assertion or independently verified Sigstore attestation. |
| Firecrawl SDK | `firecrawl@4.50.0` | Exact dependency declared by MCP and selected by lock; its transport is used for crawl, **not** monitor requests. |
| SDK source association | `firecrawl/firecrawl@fc74e903469990c1ab72f33abdff4352c7e9d8e8` | npm provenance revision. Published `httpClient.ts` and `types.ts` hashes match the corresponding monorepo files. API source inspected at this revision is publisher-source evidence, not production execution evidence. |
| Supporting dependencies | SDK-local Axios `1.20.0`; MCP-local `@modelcontextprotocol/{server,core}@2.3.1` | Lock-selected dependencies; not an assumption that another root-level SDK supplies the protocol behavior. |
| Captured surface | `firecrawl-fastmcp` `3.36.0`, protocol `2025-06-18`, **28 tools** | Static declarations. Advertised inventory digest `5cf8310b4c0b369e6e27b2b75429fe51e7e3430225c226da9708dcd3f4a81761`; not the snapshot file's byte hash. |

Sources: catalog, lock, snapshot, registry metadata/provenance, and publisher source. **[P/R]**[^r-catalog][^r-lock][^r-snapshot][^p-npm-mcp][^p-attest-mcp][^p-npm-sdk][^p-attest-sdk]

`FIRECRAWL_API_URL={world}` substitutes to **`http://world:8102`**; the catalog supplies a synthetic bearer credential and replay authorization-header template. The supervisor chooses a stdio publisher backend for npm endpoints, not the snapshot-backed replica MCP backend. The tools-image recipe uses `npm ci` from the selected manifest/lock. These are **configuration/build-source facts**, not evidence that a particular image, session, or native request was executed here. The Rust HTTP service accepts any nonempty `Bearer ` token; this establishes no real Firecrawl account validity, RBAC, credit balance, team isolation, or recipient membership. **[P/R/U]**[^r-catalog][^r-supervisor][^r-catalog-code][^r-image][^r-service]

### Read-only byte bindings

The two archive SHA-512 values matched the lockfile. Additional SHA-256 bindings:

| Object | SHA-256 |
| --- | --- |
| MCP `3.36.0` archive | `300d46120ee53f550899e2ecab194ed706c9996af0a71d1ad5260a6241d771a5` |
| Its `package/dist/index.js` | `689dc50fa9104f235828fa0c7bd3bd014c14c337c37aa15d44ad11dece2237a7` |
| SDK `4.50.0` archive | `1aafbcd39f490e422902ba9dc611047c9de13823f90b71a36de15ee074ae3cb7` |
| Its `src/v2/utils/httpClient.ts` | `c18114232646c155b2edda6b99b8696a3a291a91fd0feeb3badb159f84a74e45` |
| Its `src/v2/types.ts` | `debaf6f3b309ddd19b0a6af3106d459308d3cf3738b41f8dc57405531cca10af` |
| Local Firecrawl snapshot file | `d9bd274fb97d35993c5ef4c1128ebc50d1a020e0130f023e8127d1b2b8af57a5` |
| Local Rust `firecrawl/src/lib.rs` | `0ac549dbe53f71163a333f502a0f8d694780935ac8fc67f652b6ba997f973a6d` |
| Local Rust `firecrawl/src/text.rs` | `07e9927ab7c640924e86a1b667265053cc931055285d2c4b92ea015a17004c26` |
| Retrieved official `v2-openapi.json` | `9629a3c8681d140c3927e84276875be5728a66e08255f04acc0ab35f8461902f` |
| Retrieved official `features/crawl.mdx` | `77a60386302fbbc1bc46f16883396040c0c26337c1694223636a75f2da056cad` |
| Retrieved official `features/monitoring.mdx` | `75df340c2dae275a1ea1e9853c98fcfe99d85bc913d917ccbac04c08cb19cc77` |

These bind inspected bytes, not runtime identity, hosted conformance, or independent business completion. **[P/R/U]**[^p-npm-mcp][^p-npm-sdk][^r-snapshot][^r-service][^r-text][^d-schema][^d-crawl][^d-monitor]

## 2. Reachable operations and result identities

All tools below exist in the captured surface and publisher implementation. Static reachability does not establish an admitted native run. **[P/R/U]**[^r-snapshot][^p-server][^p-monitor]

| Native tool | Outbound request | Agent-visible result / identity |
| --- | --- | --- |
| `firecrawl_map({url,...})` | POST `/v2/map` | URL discovery; not a crawl job or proof all URLs are discoverable by a later crawl. |
| `firecrawl_crawl({url,includePaths?,excludePaths?,crawlEntireDomain?,...})` | POST `/v2/crawl`, then ordinary foreground GET polling `/v2/crawl/{id}` and result cursors | JSON text plus structured object containing the **crawl ID**, status/counters/data. Normally returns after `completed`, `failed`, or `cancelled`. |
| `firecrawl_check_crawl_status({id})` | GET `/v2/crawl/{id}` and result cursors | Read-only state/results for that crawl ID; no new crawl. |
| `firecrawl_monitor_create({page?,pages?,goal?,scheduleText?,email?,body?,...})` | POST `/v2/monitor` | Full REST envelope, normally `success:true,data:{id,...}`. **`data.id` is the monitor ID**. |
| `firecrawl_monitor_list({limit?,offset?})` | GET `/v2/monitor?...` | One response page; MCP does not auto-paginate. Replica adds pagination metadata; do not require it universally. |
| `firecrawl_monitor_get({id})` | GET `/v2/monitor/{id}` | `data` is the monitor/configuration, not a run. |
| `firecrawl_monitor_update({id,body})` | PATCH `/v2/monitor/{id}` | Updated monitor envelope. Canonical pause body: `{"status":"paused"}`. |
| `firecrawl_monitor_run({id})` | POST `/v2/monitor/{id}/run`, **no request body** | Queued-check envelope. Vendor schema/publisher source include top-level check `id` and `data.id`; replica only supplies `data.id`. **`data.monitorId` identifies its parent**. |
| `firecrawl_monitor_checks({id,limit?,offset?,status?})` | GET `/v2/monitor/{id}/checks?...` | Check summaries; use history to establish a failure streak. |
| `firecrawl_monitor_check({id,checkId,limit?,skip?,pageStatus?})` | GET `/v2/monitor/{id}/checks/{checkId}?limit=...&skip=...&status=...` | One page of check detail and results, optionally with `next`. Native `pageStatus` becomes HTTP query **`status`**. |

Request/result details above follow the selected handlers, not just SDK method names. **[D/P/R]**[^d-schema][^p-server][^p-monitor][^r-service]

Important boundaries:

- **Monitor tools bypass SDK monitor methods.** They use a separate raw `fetch` helper, preserve the REST envelope, URL-encode identities, and do not invoke SDK auto-pagination. SDK `createMonitor()` instead unwraps `data`; SDK `runMonitor()` sends `{}` and can stamp an origin body field. Requiring that SDK body from the MCP would be a false failure. **[P/I]**[^p-monitor][^p-sdk-monitor][^p-http]
- **Crawl also does not simply call SDK `crawl()`.** The MCP posts through `client.http`, then uses its own origin-aware polling/collection helpers. SDK `crawl()`'s polling-error recovery and abort/cancel behavior must not be attributed to this tool. **[P]**[^p-server][^p-sdk-crawl]
- Foreground crawl collection omits upstream `success`, `error`, `warning`, start/finish times, and duration from its projected result. `success:true` is therefore **not required in a legitimate terminal MCP result**. The original POST `url` is a **polling URL**, despite the MCP output schema's description suggesting a starting URL; it is not the target site or a second identity. **[D/P/I]**[^d-schema][^p-server]
- An MCP-task path can return admission before polling, but the generated oracle initializes protocol `2025-06-18` with empty capabilities and ordinary tool calls. Do not silently replace this condition with the newer task-extension path or assume an advertised `startOnly` flag. **[P/R/I]**[^p-server][^r-generated]
- The captured surface has **no generic Firecrawl HTTP tool**, standalone `startCrawl`, crawl-errors tool, active-crawls tool, or ordinary crawl-cancel tool. Rust REST routes and SDK methods do not add those native capabilities. Monitor detail/history tools *are* reachable. Direct simulator/admin access is not an equivalent plan under the default MCP-only policy. **[P/R/I]**[^r-snapshot][^r-service][^r-grade]
- The SDK/current API supports check status `skipped_no_credits`, but the pinned MCP `status` input enum omits it. Unfiltered history remains reachable and can return such records. A contract requiring the unavailable filter would be wrong. **[D/P/I]**[^d-schema][^p-types][^p-monitor]

## 3. Crawl admission, scope, and completion

### Business scope and existing data

The fixture contains 12 pages: five API pages, the docs home page, three guides, the changelog, and two marketing pages. Its exact desired API set is:

```text
https://docs.lumenpay.dev/api/overview
https://docs.lumenpay.dev/api/payments
https://docs.lumenpay.dev/api/refunds
https://docs.lumenpay.dev/api/customers
https://docs.lumenpay.dev/api/webhooks
```

The docs home links to all five; the overview links to the other four. The fixture provides **no sitemap file** for this scenario. An older, unrelated completed crawl `6a0d8a3e-2b1c-4f5e-8d7c-9b0a1c2d3e01` targets Contoso Pay and should remain untouched. The expected `763a0804-9f8a-89f5-a2e0-550cf370d6ae` is explicitly the fixture's **first generated crawl identity**, not a vendor ID guarantee. **[R]**[^r-crawl][^r-crawl-data]

### Admission is sufficient to initiate work, not certify collected coverage

**[D]** POST `/v2/crawl` returns HTTP 200 with `success`, `id`, and a job polling URL. Current docs distinguish this from SDK crawl-and-wait. The task asks to **start** a job and explicitly says the job—not a page summary—is needed. An evaluator should decide explicitly whether it requires (a) accepted, correctly scoped work or (b) completed collection of the five authored pages. The current local `total=5` shortcut mixes those layers. **[D/R/I]**[^d-crawl][^d-schema][^r-crawl]

**[P/R]** The ordinary pinned MCP itself waits for a terminal response, but its terminal list includes **failed/cancelled**, and its ordinary return path does not set `isError` just because the projected job status is `failed`. The replica, by contrast, stores `status:"completed"`, all page identities, equal counters, synthetic scrape IDs, and an expiry **during the POST**, before any GET. There is no asynchronous crawl worker or hosted fetch in this handler. Polling that local record cannot turn it into evidence of hosted completion. **[P/R/I/U]**[^p-server][^r-service]

**[D/P]** Current vendor guidance explicitly says `completed == total` does not prove no pages failed: failed pages are excluded from those totals. The provenance-pinned API controller agrees: `total` sums completed, active, queued, and backlog, not failed. A scraped origin HTTP 404 can still appear in `data` with `metadata.statusCode:404`; execution failures/robots blocks belong to the errors endpoint. Even that endpoint is documented as not a complete enumeration of every internal failure class. The OpenAPI `total` description still says “attempted”; prefer the specific accounting guidance and pinned arithmetic, while recording the schema/documentation disagreement. **[D/P/U]**[^d-crawl][^d-schema][^p-crawl-status]

**Decision [I]:** for a start-only contract, bind admission and scope to the actual returned job and report its observed lifecycle honestly. If completed coverage is required, independently evaluate the exact returned/authoritative URL set, origin status/error evidence, and residual failures; do not promote counters into completeness. Full native error enumeration is unavailable on this captured tool surface. A hosted failure or missing evidence is not success, but neither is it proof the scope request was malformed. **[D/P/R/U]**[^d-crawl][^r-snapshot][^r-crawl]

### Scope alternatives and limitations

Vendor scope is the conjunction of seed/child traversal, domain controls, regex filters, depth, sitemap discovery, and limit. It is not a fixed request template. **[D]**[^d-crawl][^d-schema]

| Plan | Contract assessment |
| --- | --- |
| Authored root seed + `includePaths:["^/api/.*"]`, `limit:50` | Accepted locally: service filters all fixture pages even though the home seed does not match. Current docs warn a nonmatching seed can produce zero pages. Do not certify hosted coverage from the local result. |
| Known seed `/api/overview` + `crawlEntireDomain:true` + API include filter | Native-expressible alternative: seed matches, sibling API pages can be traversed, and filters retain API-only scope. Exact discovery remains contingent on upstream execution. |
| Seed under `/api` + child-path scope and/or include filter | Potentially valid when that start resource/discovery works; this fixture has no actual `/api` page, so its local success from collection scanning is not proof that a hosted `/api` seed exists. |
| Root seed + exclusions covering home, guides, changelog; external/subdomain traversal disabled | Another expressible scope strategy. Exclude-based discovery behavior must be checked rather than inferred from collection scanning. |
| `sitemap:"only"` | Valid vendor option, but comprehensive API coverage depends on an actual sitemap; none is supplied here. It is not an independently established plan for these authored pages. |
| Five standalone scrapes or a recurring crawl monitor | Can read pages or schedule crawls, but does not supply the requested standalone crawl job. Not a semantic replacement. |

The table separates documented capabilities, local behavior, and proposed plans. **[D/P/R/I/U]**[^d-crawl][^d-schema][^p-server][^r-service][^r-crawl-data]

Additional constraints:

- Default regex matching is against **pathname**, not hostname/query/full URL. Vendor API/SDK support `regexOnFullURL`; the captured crawl input does **not** expose it. A full URL expression such as `^https://docs.../api/` is not an interchangeable pathname filter. Rust/RE2-style regex excludes look-around/backreferences; invalid patterns are documented as HTTP 400. The replica also compiles Rust regex, but additionally tries the path with its leading slash removed. **[D/P/R]**[^d-crawl][^d-schema][^p-types][^r-snapshot][^r-service]
- `crawlEntireDomain` widens sibling/parent paths; it does **not** mean “all unrelated domains.” Subdomain/external controls are separate. Marketing `lumenpay.dev` is a different host from `docs.lumenpay.dev`. Local crawl scope never follows external hosts, even when `allowExternalLinks:true` is supplied, so that switch cannot be qualified from this fixture. **[D/R/I/U]**[^d-crawl][^r-service][^r-crawl-data]
- `maxDiscoveryDepth` is link-discovery depth, not slash count. The replica treats every fixture page as one hop and only depth zero narrows it. It ignores real sitemap traversal, robots, link ordering, delay/concurrency, and prompt-based option generation in its selection. `limit` is a cap, not proof of complete discovery. **[D/R/I]**[^d-crawl][^r-service]
- “Do not include home/guides/marketing” is a collection-scope requirement, not an explicit ban on fetching a discovery page or reading a map. Do not forbid the authored map or a legitimate discovery read solely because it mentions an excluded URL. **[R/I]**[^r-crawl]

### Credits, cursors, and unresolved version drift

**[D/P/U] Credit admission:** current guide says insufficient balance to cover `limit` produces 402. The pinned publisher middleware additionally permits some explicitly limited low-credit crawls to proceed with a smaller limit after a second balance check; an allowed/overage path avoids clamping. The controller clamps to the resulting balance. The replica implements none of these gates and simply records calculated credits without spending a team balance. Do not promise either unconditional 402 or unconditional acceptance, or equate requested limit with admitted capacity. **[D/P/R/I]**[^d-crawl][^p-routes][^p-shared][^p-crawl-controller][^r-service]

**[D/P/U] Cursor drift:** current guide/OpenAPI say non-completed jobs can retain `next`, and the guide recommends stopping result paging on no new documents. At the inspected API-source revision, terminal cursor generation is more conditional and can stop for failed/cancelled jobs. The pinned MCP collector follows `next` until falsy and has **no empty-page or repeated-cursor guard**; it also breaks on a later page's false `success` and then returns `next:null`, potentially hiding partial collection. Do not interpret that null as an independent exhaustion proof or claim the documented empty-page safeguard exists in this wrapper. These are static compatibility risks, not reproduced incidents. **[D/P/I/U]**[^d-crawl][^d-schema][^p-crawl-status][^p-server]

Vendor API pagination is approximately 10MB per response; local status pagination is **10 documents**, and local expiry is `now + 24h` at admission without an expiry-enforcement check in the status handler. Vendor guidance instead describes API availability for 24 hours after completion. None of the local fixture timing/paging rules establishes hosted retention or page ordering. **[D/R/U]**[^d-crawl][^r-service]

## 4. Create the Fabrikam monitor

### Required configuration, not one serialization

The two existing monitors are `...a01` **Contoso blog**, six-hourly, and `...a02` **Fabrikam careers**, daily at 9am America/New_York. Preserve their full configurations and histories, not just the two checked projections. The requested new name is exactly **Fabrikam pricing**, target exactly **`https://www.fabrikam.io/pricing`**, daily, with alerts about **prices or plan limits** to **`pricing-intel@contoso.example`**. Fixture expectation `adf6b65a-6476-82e0-8fc2-15f9b1e90267` again names the first deterministic local creation, not a production identity. **[R/I]**[^r-create][^r-create-data]

The authored native shorthand builds:

```json
{
  "name": "Fabrikam pricing",
  "schedule": {"text": "daily", "timezone": "UTC"},
  "goal": "Alert when Fabrikam changes any plan price or plan limit.",
  "targets": [{"type": "scrape", "urls": ["https://www.fabrikam.io/pricing"]}],
  "notification": {
    "email": {
      "enabled": true,
      "recipients": ["pricing-intel@contoso.example"],
      "includeDiffs": true
    }
  }
}
```

That is publisher-wrapper behavior and a sound expressible configuration, not a required exact JSON/string template. The advanced **`body` wins entirely** over shorthand fields; if `body` is supplied, putting `email` or `goal` alongside it does not patch them into the body. Nonempty shorthand `queries` take precedence over `page/pages` and create a search target instead. Those are actionable request-construction hazards. **[P/R/I]**[^p-monitor][^r-create]

### Schedule semantics

**[D/P]** Monitor create schema requires name, schedule, and targets. Schedule accepts **either** natural-language `text` or five-field `cron`; publisher validation rejects both together. Timezone is IANA, default UTC; minimum interval is five minutes. Vendor responses normalize to `schedule.cron`/`timezone`, and documentation describes spreading/jitter. The replica instead retains supplied text/cron plus synthetic `intervalMinutes`, and computes next run as **current scenario time + interval**, not a timezone-aware calendar occurrence. **[D/P/R]**[^d-schema][^d-monitor][^p-monitor-types][^p-cron][^r-service]

Accept native advanced-body cron such as `0 9 * * *` in an appropriate timezone, or supported equivalent daily text. The user did not mandate UTC, midnight, exact text `"daily"`, a 24-hour offset from creation, or a particular minute. Calendar-daily runs in DST zones need not be exactly 86,400 elapsed seconds apart. Do not impose those additional requirements. Conversely, weekday-only, twice-daily, and weekly schedules are not daily just because the local parser maps them poorly. **[D/P/R/I]**[^d-monitor][^p-cron][^r-text][^r-create]

### Change selection, judging, and notification evidence

**[D/P]** Page monitors diff markdown by default. A nonempty goal enables judging when `judgeEnabled` is omitted; explicit `judgeEnabled:false` can store a goal without judging. JSON-mode change tracking can instead focus on structured fields, but a schema tracking only prices would miss requested plan-limit changes. Price/limit-specific markdown-plus-goal and a complete structured-field design are alternatives, not equivalent to a search monitor looking for new results. Exact goal prose, an explicit `judgeEnabled:true`, JSON extraction, and `includeDiffs:true` are not all mandatory user requirements. **[D/P/I]**[^d-monitor][^d-schema][^p-types][^p-monitor-types][^r-create]

**[D]** Email is disabled by default unless enabled. Explicit recipients are optional in the general API, but omitting them selects eligible team alert recipients rather than proving delivery to this task's address. Up to 25 explicit recipients are documented. Summaries are sent for changed/new/removed/error pages, not every successful unchanged run. With judging enabled, a check containing only changed pages judged as noise can suppress email. Webhook events and email are separate notification mechanisms; a webhook alone is not the requested email configuration. **[D/I]**[^d-monitor][^d-schema][^r-create]

**[D/P/U] Consent and partial effects:** external recipients must confirm subscription; team members are auto-confirmed. SDK/source expose per-recipient `pending|confirmed|unsubscribed`, `source`, and confirmation-email information, though the retrieved OpenAPI monitor model omits that field. Publisher create persists the monitor **before** syncing recipients, catches sync failure, and can return success with an empty subscription list. Thus a created active monitor with a configured address is not proof confirmation was sent, accepted, or that alerts are deliverable. The `.example` fixture address and absence of recipient/team data do not establish mailbox reachability or membership. **[D/P/R/I/U]**[^d-monitor][^p-types][^p-monitor-controller][^d-schema][^r-create-data]

**[R]** Local create does not run a baseline check, validate delivery, normalize recipient subscriptions, enable judging from the goal, or deliver mail/webhooks. It copies largely arbitrary optional JSON, defaults active/30-day retention, stores null last-run/check fields, and sets next run by interval. It has no background recurring-monitor engine or real change comparison. A local manual run labels every found good URL `same` regardless of changed markdown. A `goal`, `notification`, or `active` string in this store is configuration evidence only. **[R/I/U]**[^r-service]

**Decision [I]:** require a new request-bound monitor with the intended URL, real daily schedule, enabled email to the intended recipient, and price/limit alert intent; preserve baseline monitors and histories. Treat recipient confirmation/delivery as a separate observable capability or explicit unresolved prerequisite. Do not fabricate successful delivery or require the agent to confirm somebody else's mailbox. **[D/P/R/U]**[^d-monitor][^p-monitor][^r-create]

## 5. Identify, pause, then run Tailspin

### Identity and diagnostic evidence

| Monitor suffix | Identity and evidence | Required treatment |
| --- | --- | --- |
| `...a11` | **Tailspin pricing**, `https://www.tailspintoys.com/pricing`; one successful check October 26, then **six consecutive failed checks** October 27 00:00 through October 29 12:00, every 12 hours, each with one 404 page error | Pause; retain old URL, configuration, and history for later repointing. |
| `...a12` | **Tailspin plans**, `https://www.tailspintoys.com/plans`; six successful checks over that same failure window, including the earlier Family-price-change diff | Initiate a fresh manual check after pausing `...a11`; keep its configuration active. |
| `...a13` | **Tailspin pricing (EU)**, `https://www.tailspintoys.eu/pricing`, Europe/Berlin | Preserve; similar name is not the US plans target. |
| `...a14` | **Wingtip careers**, `https://careers.wingtiptoys.com` | Preserve. |

Full monitor IDs share prefix `0c7e4b21-9d3a-4e6f-a1b2-5c8d7e6f9a`; the row suffixes are `11`–`14`. This is fixture identification, not inference from name alone. The old URL is absent from the current page collection while `/plans` exists. **[R]**[^r-pause][^r-pause-data]

The authored list + latest-five-checks plan is enough to see five recent failures, but the list response strips page details locally: summaries establish failures; `firecrawl_monitor_check` is needed for those checks' page-level 404 reasons. Claiming the author solution inspected the exact error text from summaries would overstate its evidence. Alternative discovery/detail reads are valid; hard-coding a failure counter is not. Vendor guidance explicitly says no aggregate failure/streak count is maintained and checks are newest first. **[D/P/R/I]**[^d-monitor][^p-monitor][^r-service][^r-pause]

### Pause is not deletion, repair, or retrospective cancellation

Canonical PATCH `{"status":"paused"}` is reachable. Pausing stops future scheduling while retaining configuration/history; it does not authorize retargeting the retired page, changing recipients/goal/retention, deleting historical checks, or pausing look-alikes. Do not infer automatic pausing after three ordinary HTTP failures: current docs describe automatic pause specifically for three partner-revocation credit skips, not the six fixture 404 failures. **[D/P/R/I]**[^d-monitor][^d-schema][^p-monitor][^r-pause]

**[P/R/U] Derived-field caveat:** local pause recomputes `nextRunAt:null`. At the provenance-pinned publisher source, general `updateMonitor()` sets status and `updated_at` but changes `next_run_at` only when a schedule is supplied; its separate automatic-pause helper explicitly clears next run. Therefore, requiring exact `nextRunAt:null` for every acknowledged manual pause is not established by that source. Whether a hosted DB trigger/later reconciliation changes it is unresolved here. Require the scheduling effect and preserved configuration, and version any stricter response-field assertion. Neither this PATCH path nor the docs establish cancellation of an already queued/running check. **[P/R/I/U]**[^p-monitor-controller][^p-store][^r-service]

### Manual run is admission of a new child check

Vendor schema describes HTTP 200 as **check queued**, with 409 for overlap. The pinned publisher controller additionally rejects a paused monitor with 409, returns its current `checkId` on overlap, inserts a new check row, then enqueues work and returns the new identity. These are ordered steps, not evidence of a distributed atomic transaction or finished scrape. A queue-enqueue failure can follow check-row persistence; static source establishes that partial-effect opportunity, not that it occurred. **[D/P/I/U]**[^d-schema][^p-monitor-controller][^p-store][^p-scheduler]

Local run instead:

1. Accepts any existing monitor, even paused or already running.
2. Checks listed URL existence/status synchronously, without HTTP or retained-snapshot comparison.
3. Stores `completed`, `failed`, or `partial` with created/started/completed times all equal to the scenario clock.
4. Updates parent `lastRunAt`/`lastCheckId`.
5. Returns a **different lifecycle projection**, `data.status:"queued"`, with the same new check ID.

It does not reserve/settle credits, enforce overlap, update parent next-run timing on run, or execute real notification delivery. A local terminal snapshot cannot prove that hosted queue work completed, that new web data was obtained, or that a notification was sent. **[R/I/U]**[^r-service]

**Decision [I]:** the instruction says “kick off,” so a newly accepted **manual check of `...a12`**, causally after the pause, is the minimum requested operation. The parent/check relationship and request-relative creation distinguish it from the old October 29 noon check. If “fresh data today” is interpreted as completed refresh, make that extra completion requirement explicit and inspect that returned check's terminal status and `/plans` result; a queued acknowledgement is insufficient. Conversely, do not reject an honestly reported queued run merely because the local model would already have stamped completion. **[D/P/R/U]**[^r-pause][^d-schema][^p-monitor-controller][^r-service]

Allow only justified derived updates for the run target: new check/results and corresponding run/check/summary/billing metadata when modeled. Preserve its target, schedule, goal, recipients, and active status. Preserve all other monitors/check history; allow the paused monitor's status/update metadata and modeled scheduling consequence, not an unrestricted patch. **[I]**[^r-pause][^p-types][^p-store]

## 6. Errors, retries, and partial-effect recovery

| Boundary | Established behavior / objection |
| --- | --- |
| Replica HTTP | Nonempty bearer required; missing bearer → 401 `{"success":false,"error":"Unauthorized: Invalid token"}`. Invalid body/schedule/target/status yields local 400 forms; unknown monitor/check/job yields 404. These exact errors do not establish hosted authorization or every framework error shape. |
| Monitor transport | Raw `fetch`; no automatic HTTP retry or polling. Non-2xx or `payload.success===false` throws `CoreHttpError(message,status)`. Malformed JSON is converted to `{}`; on HTTP 2xx this can look like an empty successful tool result, not a confirmed operation. |
| Crawl transport | SDK `HttpClient` defaults to **three total attempts**, retrying HTTP **502** with 0.5/1-second exponential waits. This applies to POST/PATCH as well as GET; it is not a blanket network/429/5xx retry policy. |
| MCP foreground crawl polling | Separate helper waits normally two seconds; status-poll 429 honors a bounded retry delay. Other thrown errors escape. Internal timeout/poll settings are not advertised native crawl arguments. It does not cancel the already admitted crawl on return failure. |
| MCP API errors | Recognized `CoreHttpError`/`SdkError` with HTTP 4xx/5xx become `isError:true`, with declared structured `success:false,error:{code,status,message,...}` when schema-valid. Generic execution failures become text plus `isError:true`; do not require every failure to be a JSON-RPC error or every error to have structured details. |
| Lost monitor diagnostics | `monitorRequest` retains message/status but discards other vendor error fields such as overlap `checkId`; get/list/history may be needed to reconcile. The full REST error body is not guaranteed in the tool result. |
| Ordinary crawl terminal failure | Terminal `failed`/`cancelled` can be returned as structured job state without `isError:true`; projected details can omit upstream error/warning text. Universal envelope success is not business success. |
| Existing evaluator | Required-call default means an observed **HTTP 2xx**, not just an attempt. For Firecrawl it does not independently reject every HTTP-200 semantic `success:false`. Forbidden calls without status restrictions reject attempts, including observed request-start evidence. |

Sources: local service/grader and the exact MCP/SDK implementations. **[P/R/I]**[^r-service][^r-grade][^p-monitor][^p-server][^p-errors][^p-http]

**No exposed exactly-once guarantee [P/I/U].** The crawl API source has `x-idempotency-key` middleware, returning 409 for a reused key; this is not a documented replay of the original result. The captured crawl tool exposes no idempotency argument or general outbound-header facility; `webhookHeaders` are destination webhook headers, not crawl-request headers. The wrapper's crawl POST does not pass an idempotency key. Monitor create/run likewise expose none, and publisher routes do not attach that middleware to them. Do not claim either “Firecrawl has no idempotency facility anywhere” or “this captured operation is safely deduplicated.” **[P/R/I]**[^p-routes][^p-shared][^p-server][^p-monitor][^r-snapshot]

Recommended recovery is operation-specific:

- **Crawl:** retain the returned crawl ID and poll that job after a polling/output failure; do not start a replacement merely because collected results were not delivered. A 502 after a committed POST can trigger another admission in the pinned transport. If the ID was lost, this surface lacks active-crawl listing, so acknowledge reconciliation limits rather than inventing a known identity. **[P/I/U]**[^p-server][^p-http][^r-snapshot]
- **Monitor create:** reconcile name plus exact target/configuration and operation context using list/get; names are not resource IDs and a concurrent same-name monitor is not automatically this request's creation. Read subscription state separately where returned. A lost response or recipient-sync issue need not mean creation failed. **[P/I/U]**[^p-monitor][^p-monitor-controller][^p-store]
- **Pause:** read the selected monitor after uncertainty; repeating the same canonical status patch is configuration-idempotent, but local/update timestamps and other derived metadata can still change. The already committed pause must not be rolled back because the following manual run failed. **[P/R/I]**[^p-monitor][^p-store][^r-service]
- **Run:** use the returned **check ID** and parent ID to inspect progress. On response loss/409, inspect current state/history before initiating another run. Multiple newly queued checks are not a justified substitute for discovering whether the first ran. The raw monitor helper does not itself retry. **[D/P/I/U]**[^d-schema][^p-monitor][^p-monitor-controller]

There is no cross-operation transaction around “pause broken monitor, then run healthy monitor.” A failed second operation can leave the requested pause applied. Evaluate/report the two effects independently; never infer rollback from a failed final tool call or empty/malformed result. **[P/R/I/U]**[^p-monitor-controller][^r-service]

## 7. Static evaluator counterexamples

These are **source-derived objections**, not executed controls or qualification evidence. All three generated expectations have `version:1`, `audit_history:false`, no ordering rules, and `preservation:null`; default preservation is therefore not checked. Their port-7102 result profile is empty and falls back to universal MCP-envelope validation, not Firecrawl-specific identity/lifecycle validation. No answer predicate requires a truthful completion claim. **[R/I]**[^r-generated][^r-grade]

### False-positive candidates in the present condition

| Counterexample, using reachable native tools | Why the existing predicates do not distinguish it |
| --- | --- |
| Crawl root with `includePaths:["^/$","^/api/(overview|webhooks)$","^/guides/(quickstart|testing)$"]`, `limit:50` | Local selection produces five pages, including both required API members, but also home and two guides and omits three API pages. Existing checks assert only total five, two members, and a hostname substring. |
| Correct first crawl, then start an unrelated/broad second crawl | The fixed first-ID projections remain correct; no complete crawl-set preservation or creation-cardinality rule rejects extra work. |
| Advanced create body with correct name/URL/daily schedule/recipient but `email.enabled:false`, absent/wrong goal, or `judgeEnabled:false` | Recipient membership and interval/status projections do not establish enabled price/limit alerts. API-valid “store without judging” is not automatically business-equivalent. |
| Advanced create with `schedule:{"cron":"0 0,12 * * *","timezone":"UTC"}` | It runs twice daily in real cron. Local parser's comma-hour branch returns 1440, satisfying the current daily assertion. `0 9 * * 1-5` likewise becomes 1440 despite skipping weekends. |
| Target URL `https://www.fabrikam.io/pricing-backup`, or correct first target followed by an unrelated target/recipient/webhook | Local substring check accepts the look-alike path; no exact target-set/recipient-scope assertion excludes the additional scope. Target existence is not checked on create. |
| Correct new Fabrikam monitor, then manually run either existing monitor | No DELETE/PATCH occurs; checked career URL/blog active state remain, while existing run metadata/history change. “Leave those as they are” is not fully enforced. |
| Pause `...a11` while also changing its goal/notification/retention/name | Canonical status and old target stay correct; current predicates do not preserve the other fields. Such collateral fields are expressible through the native update `body`. |
| Pause `...a11`, run `...a12`, also run EU/Wingtip/broken monitors or run `...a12` repeatedly | Only deletion and PATCH of look-alikes are forbidden; extra runs mutate unchecked histories/metadata. The local service also permits running the just-paused monitor. |
| Run healthy monitor before pausing broken monitor | Final predicates and required POST are unchanged; no causal “then” ordering is enforced. |
| Correct final projections after intervening target-monitor damage/restoration | With no audited preservation policy, final equality does not establish that unauthorized intermediate effects never occurred. PATCHes of protected look-alikes remain explicitly forbidden and are **not** counterexamples of this kind. |

The current URI predicates, first-generated-ID references, call matchers, service selector/parser, and native parameter schemas establish these static blind spots. **[P/R/I]**[^r-crawl][^r-create][^r-pause][^r-service][^r-text][^r-grade][^p-monitor][^r-snapshot]

Do not claim that a completely failed HTTP run satisfies the required POST: its default matcher requires 2xx. The lifecycle objection is that **accepted work is not completed fresh data**, and the current local run shortcut hides that distinction. **[R/I]**[^r-grade][^r-pause][^r-service]

### False negatives and over-tightening risks

| Valid/equivalent evidence or plan | Rejection risk |
| --- | --- |
| Crawl from `https://DOCS.LUMENPAY.DEV/api/overview`, API-only include filter, `crawlEntireDomain:true` | Hostname is case-equivalent and local normalized selection returns the same five authored URLs, but stored raw seed lacks the exact lowercase `docs.lumenpay.dev` substring. This is a concrete static current-check false-negative candidate. |
| Monitor the same HTTPS page with `www.FABRIKAM.io` hostname casing | Hostname identity is unchanged, but the stored raw target fails lowercase substring matching. Match parsed URL semantics, not this textual accident. |
| A correctly bound creation/run after a justified response-loss recovery | Fixed first-generated UUIDs can select the wrong object; select actual returned/request-caused identities and explicitly reconcile duplicate side effects rather than treating an ID stream as vendor identity. This is a recovery-policy objection, not blanket permission for extra resources. |
| Daily cron instead of shorthand text, different supported daily time/timezone, concise equivalent goal, or default markdown tracking | Do not freeze the author solution's UTC/text/goal/options as the only plan. Validate recurrence and alert scope. |
| Legitimate completed crawl result without `success:true`; monitor result with `data.id` rather than a mandatory top-level ID; no list-pagination metadata | Those are genuine wrapper/local/publisher shape differences. Require the identity-bearing value appropriate to each operation. |
| Canonical manual pause retaining a non-null next-run field in the pinned publisher source | Requiring the replica's exact derived-field normalization would overstate source guarantees; qualify actual scheduling behavior separately. |
| Run acknowledged as queued, or later failed/partial after correct admission | Start-only and successful-refresh contracts must be distinct. Report failure/uncertainty honestly instead of misclassifying admission as success or universally demanding immediate terminal state. |
| Missing external-recipient confirmation or an already running check | May be consent/concurrency constraints outside the requested tool's control; do not fabricate a bypass, treat a 409 as successful new admission, or count a missing confirmation as proven delivered mail. |

Sources for URI normalization and current checks are local; alternative schedule, result, consent, and lifecycle facts are vendor/pinned-source evidence. **[D/P/R/I/U]**[^r-text][^r-service][^r-grade][^r-crawl][^r-create][^p-monitor][^p-server][^p-store][^d-monitor][^d-schema]

## 8. Actionable objections before implementation or admission

1. **Choose completion level per operation.** Crawl acceptance, terminal collection, monitor configuration, recipient readiness, queued check, fresh page data, and delivered alert are separate assertions. Bind each to a real identity and evidence; do not derive hosted facts from local status fields.
2. **Replace fixed-ID/projection grading with causal resource selection.** Match actual newly created crawl/monitor/check records and parent relationships; preserve preexisting collections and historical rows. Explicitly decide safe retry/cardinality policy.
3. **Check exact business scope.** Crawl exact API set when completion is required; exact intended monitor target and notification scope; no extra marketing/guide collection, monitor targets, recipients, webhooks, or unrelated runs.
4. **Model daily scheduling semantically.** Accept supported text/cron alternatives; reject comma-hour/weekday masquerades. Do not expose `intervalMinutes` as if it were Firecrawl's schema, or require `now+1440m` across timezone/DST.
5. **Preserve all unauthorized state and history.** Pause only the identified old monitor, then run only the healthy plans monitor. Grant narrow allowances for actual status/run/check consequences, not a subtree-wide write exemption.
6. **Do not silently repair permissive replica behavior into vendor truth.** Review ignored crawl controls, lack of real discovery, arbitrary monitor JSON, missing paused/overlap guards, consent/delivery omissions, synthetic diffs, and synchronous terminal records explicitly.
7. **Add operation-aware result validation without a new false negative.** Distinguish failed terminal crawl from envelope success; retain available identities; support the actual nested monitor envelopes. Detect empty/malformed 2xx monitor results and incomplete pagination instead of treating them as completion.
8. **Version or adjudicate source/documentation conflicts.** Credit clamping versus blanket 402, terminal cursors, manual-pause derived fields, recipient-subscription schema omission, and the missing `skipped_no_credits` filter require explicit decisions, not inferred hosted behavior.
9. **Keep qualification separate.** These findings propose future controls; they do not migrate any of the 21 legacy tasks, execute a native trace, qualify notification delivery, change a frozen condition, or close any of the 19 overhaul items.

These are recommendations **[I]**, supported by the preceding primary-source and local evidence, not a claim of implementation or validation.

## Primary sources and local evidence

External source links are Firecrawl-owned docs, repositories, or publisher npm distribution/provenance. Immutable code links use the provenance revisions; current documentation links remain mutable and are byte-bound above.

[^d-crawl]: Firecrawl, [Crawl guide](https://docs.firecrawl.dev/features/crawl), including Execution and result accounting, scope, credits, and configuration; [retrieved documentation source](https://github.com/firecrawl/firecrawl-docs/blob/main/features/crawl.mdx).
[^d-monitor]: Firecrawl, [Monitoring guide](https://docs.firecrawl.dev/features/monitoring), especially Schedules, Goals and judging, Email/Recipient confirmation, Check results, and Skipped checks and paused monitors; [retrieved documentation source](https://github.com/firecrawl/firecrawl-docs/blob/main/features/monitoring.mdx).
[^d-schema]: Firecrawl, [official v2 OpenAPI source](https://github.com/firecrawl/firecrawl-docs/blob/main/api-reference/v2-openapi.json): POST/GET crawl, monitor create/update/run/history, `MonitorSchedule`, `MonitorNotification`, `Monitor`, `MonitorCheck`, `MonitorRunResponse`, and crawl response schemas.
[^p-npm-mcp]: npm publisher metadata for [`firecrawl-mcp@3.36.0`](https://registry.npmjs.org/firecrawl-mcp/3.36.0) and its [published archive](https://registry.npmjs.org/firecrawl-mcp/-/firecrawl-mcp-3.36.0.tgz); inspected `package/dist/index.js`.
[^p-attest-mcp]: npm [MCP package attestations](https://registry.npmjs.org/-/npm/v1/attestations/firecrawl-mcp@3.36.0), decoded SLSA `resolvedDependencies.gitCommit`; payload read, signatures not independently verified.
[^p-npm-sdk]: npm publisher metadata for [`firecrawl@4.50.0`](https://registry.npmjs.org/firecrawl/4.50.0) and its [published archive](https://registry.npmjs.org/firecrawl/-/firecrawl-4.50.0.tgz); inspected v2 transport/types/crawl/monitor source and compiled bundle.
[^p-attest-sdk]: npm [SDK package attestations](https://registry.npmjs.org/-/npm/v1/attestations/firecrawl@4.50.0), decoded SLSA `resolvedDependencies.gitCommit`; payload read, signatures not independently verified.
[^p-server]: Firecrawl MCP source at `965456eb17bd28ad200bcbaac94159b000bd2b52`, [`src/server.ts`](https://github.com/firecrawl/firecrawl-mcp-server/blob/965456eb17bd28ad200bcbaac94159b000bd2b52/src/server.ts): `prepareTool`, `createClient`, origin-aware crawl helpers, crawl/check-status tools, task hooks; [`src/tool-output.ts`](https://github.com/firecrawl/firecrawl-mcp-server/blob/965456eb17bd28ad200bcbaac94159b000bd2b52/src/tool-output.ts). Actual published bundle inspected in parallel; framework catch/result behavior is also present in that bundle.
[^p-monitor]: Same MCP revision, [`src/monitor.ts`](https://github.com/firecrawl/firecrawl-mcp-server/blob/965456eb17bd28ad200bcbaac94159b000bd2b52/src/monitor.ts): raw fetch helper, body precedence, shorthand conversion, all eight monitor tools, check-status enum, encoding, query mapping.
[^p-errors]: Same MCP revision, [`src/core-http-error.ts`](https://github.com/firecrawl/firecrawl-mcp-server/blob/965456eb17bd28ad200bcbaac94159b000bd2b52/src/core-http-error.ts), plus `prepareTool` and bundled framework error handling.
[^p-http]: Firecrawl monorepo at `fc74e903469990c1ab72f33abdff4352c7e9d8e8`, [`apps/js-sdk/firecrawl/src/v2/utils/httpClient.ts`](https://github.com/firecrawl/firecrawl/blob/fc74e903469990c1ab72f33abdff4352c7e9d8e8/apps/js-sdk/firecrawl/src/v2/utils/httpClient.ts), identical to the inspected published file; request attempts, 502 retry, authorization/origin, request timeout, idempotency-header helper.
[^p-types]: Same monorepo revision, [`apps/js-sdk/firecrawl/src/v2/types.ts`](https://github.com/firecrawl/firecrawl/blob/fc74e903469990c1ab72f33abdff4352c7e9d8e8/apps/js-sdk/firecrawl/src/v2/types.ts), identical to the inspected published file: crawl types and monitor schedule/target/request/subscription/check/diff types.
[^p-sdk-crawl]: Same revision, [`SDK v2 methods/crawl.ts`](https://github.com/firecrawl/firecrawl/blob/fc74e903469990c1ab72f33abdff4352c7e9d8e8/apps/js-sdk/firecrawl/src/v2/methods/crawl.ts); corresponding published archive source inspected.
[^p-sdk-monitor]: Same revision, [`SDK v2 methods/monitor.ts`](https://github.com/firecrawl/firecrawl/blob/fc74e903469990c1ab72f33abdff4352c7e9d8e8/apps/js-sdk/firecrawl/src/v2/methods/monitor.ts); corresponding published archive source inspected.
[^p-monitor-controller]: Same revision, [`apps/api/src/controllers/v2/monitor.ts`](https://github.com/firecrawl/firecrawl/blob/fc74e903469990c1ab72f33abdff4352c7e9d8e8/apps/api/src/controllers/v2/monitor.ts): serialize/create/update/run/history paths and recipient sync. API-source evidence only, not hosted deployment provenance.
[^p-monitor-types]: Same revision, [`apps/api/src/services/monitoring/types.ts`](https://github.com/firecrawl/firecrawl/blob/fc74e903469990c1ab72f33abdff4352c7e9d8e8/apps/api/src/services/monitoring/types.ts): strict schedule/target/config validation, defaults, canonical status patch.
[^p-store]: Same revision, [`monitoring/store.ts`](https://github.com/firecrawl/firecrawl/blob/fc74e903469990c1ab72f33abdff4352c7e9d8e8/apps/api/src/services/monitoring/store.ts): create/update monitor, create check, next-run/last-check updates, automatic pause.
[^p-scheduler]: Same revision, [`monitoring/scheduler.ts`](https://github.com/firecrawl/firecrawl/blob/fc74e903469990c1ab72f33abdff4352c7e9d8e8/apps/api/src/services/monitoring/scheduler.ts): queue handoff, scheduled overlap handling, jitter.
[^p-cron]: Same revision, [`monitoring/cron.ts`](https://github.com/firecrawl/firecrawl/blob/fc74e903469990c1ab72f33abdff4352c7e9d8e8/apps/api/src/services/monitoring/cron.ts): supported text conversion, cron parsing, timezone-aware next occurrence, minimum interval.
[^p-crawl-controller]: Same revision, [`apps/api/src/controllers/v2/crawl.ts`](https://github.com/firecrawl/firecrawl/blob/fc74e903469990c1ab72f33abdff4352c7e9d8e8/apps/api/src/controllers/v2/crawl.ts): admission validation, credit limit clamping, group/save/mark/enqueue order, polling URL response.
[^p-crawl-status]: Same revision, [`apps/api/src/controllers/v2/crawl-status.ts`](https://github.com/firecrawl/firecrawl/blob/fc74e903469990c1ab72f33abdff4352c7e9d8e8/apps/api/src/controllers/v2/crawl-status.ts): `outputBulkA` counters, kickoff-failure response, conditional terminal cursors.
[^p-routes]: Same revision, [`apps/api/src/routes/v2.ts`](https://github.com/firecrawl/firecrawl/blob/fc74e903469990c1ab72f33abdff4352c7e9d8e8/apps/api/src/routes/v2.ts): crawl/monitor authorization, credit checks, and idempotency middleware selection.
[^p-shared]: Same revision, [`apps/api/src/routes/shared.ts`](https://github.com/firecrawl/firecrawl/blob/fc74e903469990c1ab72f33abdff4352c7e9d8e8/apps/api/src/routes/shared.ts): credit admission/clamped retry and `x-idempotency-key` reused-key 409 behavior.
[^r-catalog]: [`catalog/services.toml:29`](../../catalog/services.toml#L29), Firecrawl selection, environment, ports, and synthetic replay header.
[^r-lock]: [`servers/npm/package.json:16`](../../servers/npm/package.json#L16), [`servers/npm/package-lock.json:2954`](../../servers/npm/package-lock.json#L2954), exact MCP/SDK/transport dependencies and integrity.
[^r-snapshot]: [`servers/snapshots/firecrawl.json:1`](../../servers/snapshots/firecrawl.json#L1), full captured inventory and relevant input/output schemas.
[^r-supervisor]: [`crates/catalog/src/bin/tools-supervisor.rs:40`](../../crates/catalog/src/bin/tools-supervisor.rs#L40), npm stdio versus replica dispatch.
[^r-catalog-code]: [`crates/catalog/src/lib.rs:404`](../../crates/catalog/src/lib.rs#L404), `{world}` substitution.
[^r-image]: [`images/tools/Dockerfile:41`](../../images/tools/Dockerfile#L41), locked npm image recipe; not an observed build.
[^r-service]: [`crates/services/firecrawl/src/lib.rs:54`](../../crates/services/firecrawl/src/lib.rs#L54) routes; helpers/auth at 129–190; crawl implementation at 1257; monitor implementation at 1907. All references are to the unmodified inspected worktree.
[^r-text]: [`crates/services/firecrawl/src/text.rs:10`](../../crates/services/firecrawl/src/text.rs#L10) URL normalization; schedule/cron parsers at 271 and 326.
[^r-grade]: [`crates/mock-core/src/grading.rs:27`](../../crates/mock-core/src/grading.rs#L27), version/default interfaces; preservation at 436; required/forbidden matching at 700; state contains at 947; required 2xx/error handling at 1051; subset/loose matching at 1078.
[^r-crawl]: [`crawl-api-reference/instruction.md`](../../scenarios/search/firecrawl/crawl-api-reference/instruction.md), [`scenario.toml`](../../scenarios/search/firecrawl/crawl-api-reference/scenario.toml), authored expectation and reference plan.
[^r-crawl-data]: [`crawl pages`](../../scenarios/search/firecrawl/crawl-api-reference/dataset/firecrawl/pages.json), [`existing crawls`](../../scenarios/search/firecrawl/crawl-api-reference/dataset/firecrawl/crawls.json); full scenario data inventory inspected.
[^r-create]: [`monitor-competitor-pricing/instruction.md`](../../scenarios/search/firecrawl/monitor-competitor-pricing/instruction.md), [`scenario.toml`](../../scenarios/search/firecrawl/monitor-competitor-pricing/scenario.toml).
[^r-create-data]: [`Fabrikam scenario pages`](../../scenarios/search/firecrawl/monitor-competitor-pricing/dataset/firecrawl/pages.json), [`monitors`](../../scenarios/search/firecrawl/monitor-competitor-pricing/dataset/firecrawl/monitors.json), [`checks`](../../scenarios/search/firecrawl/monitor-competitor-pricing/dataset/firecrawl/monitor_checks.json).
[^r-pause]: [`pause-broken-pricing-monitor/instruction.md`](../../scenarios/search/firecrawl/pause-broken-pricing-monitor/instruction.md), [`scenario.toml`](../../scenarios/search/firecrawl/pause-broken-pricing-monitor/scenario.toml).
[^r-pause-data]: [`Tailspin scenario pages`](../../scenarios/search/firecrawl/pause-broken-pricing-monitor/dataset/firecrawl/pages.json), [`monitors`](../../scenarios/search/firecrawl/pause-broken-pricing-monitor/dataset/firecrawl/monitors.json), [`17 historical checks`](../../scenarios/search/firecrawl/pause-broken-pricing-monitor/dataset/firecrawl/monitor_checks.json).
[^r-generated]: Generated copies inspected without regeneration: [`crawl expect`](../../tasks/search__firecrawl__crawl-api-reference/tests/expect.json), [`create expect`](../../tasks/search__firecrawl__monitor-competitor-pricing/tests/expect.json), [`pause expect`](../../tasks/search__firecrawl__pause-broken-pricing-monitor/tests/expect.json); all three `tests/task-source.json`, `solution/solve.sh`, `solution/response.jq`, and `solution/result-contracts.json`; [`result-profile fallback`](../../tasks/search__firecrawl__pause-broken-pricing-monitor/solution/tool-result.jq#L426). Generated instructions match authored instructions by read-only byte comparison.
[^r-ledger]: [`docs/overhaul-checklist.md:9`](../overhaul-checklist.md#L9), current inventory/open gates; [`docs/operation-support.md:1`](../operation-support.md#L1), partial support is not certification.
