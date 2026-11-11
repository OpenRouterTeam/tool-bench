# Datadog WRITE operation contracts — 2026-11-07

## Scope and evidence discipline

Research-only sidecar for:

- `ops/datadog/cancel-called-off-maintenance`
- `ops/datadog/mute-host-from-error-logs`
- `ops/datadog/schedule-replica-downtime`

Only this note is authored. Instructions, scenarios, datasets, implementation, existing documentation, source snapshots, and qualification evidence are not changed. No builds, tests, installations, Docker commands, model trials, Datadog MCP calls, live Datadog accounts, infrastructure operations, or subagents were used. Public documentation and package/source downloads were read; archives were decompressed and hashed in memory, not installed or executed. The research skill's primary-source workflow was applied directly, respecting the explicit no-subagents instruction. PagerDuty37 qualification was not inspected or operated.

Evidence labels:

- **[D] Vendor documentation:** Datadog-owned documentation, including documentation embedded in its generated SDK.
- **[P] Pinned implementation:** the selected native MCP publisher's implementation, Datadog SDK, or MCP SDK at the exact package/source pins below. The selected `@winor30` MCP is third-party software, not Datadog's own hosted MCP.
- **[R] Replica/fixture convention:** local authored instructions, data, Rust service, captured tool definitions, or evaluator. Not a vendor guarantee.
- **[I] Inference/proposal:** static consequences, evaluator objections, or recommended business contracts. No counterexample program was executed.
- **[U] Unestablished:** behavior requiring runtime, account, authorization, notification, migration, or independent qualification evidence.

The research date is November 7, 2026. All three scenarios instead reference **October 30, 2026**, at 10:00, 14:30, and 16:00 UTC respectively. “Today,” “tonight,” and “this Saturday” must use those scenario references, not the researcher's wall clock. The requested October 31 maintenance remains October 31 even though it is already past on the research date. **[R]**[^r-cancel][^r-mute][^r-schedule]

## Findings that should change evaluator design

1. **DELETE means cancel, not erase immediately.** The pinned native tool issues v1 DELETE, expects a void success, and returns its own cancellation sentence. The replica retains the downtime, sets `canceled`, `disabled`, `active`, and `updater_id`, and returns empty HTTP 204. Requiring absence or a JSON response body would be wrong for this condition. Current vendor **v2** documentation describes roughly two days of canceled-record retention, not indefinite retention; the v1 retention horizon and repeated-cancel idempotence are not established here. **[D/P/R/I/U]**[^d-api-cancel][^p-mcp-downtime][^p-sdk-downtime][^r-downtime]
2. **Host mute has a vendor-side effect the replica omits.** Datadog's pinned SDK and current operation-specific API docs explicitly document that the v1 host-mute endpoint creates a **Downtime V2**. The local handler instead updates a host and an authored `host_mutes` collection, without creating any downtime. Do not promote “host mute must never create a downtime” into a vendor preservation rule; equally, do not invent a v2 child ID in this replica's evidence. **[D/P/R/I]**[^d-api-host][^p-sdk-hosts][^r-hosts]
3. **The MCP does not expose raw REST responses.** Downtime models are SDK-deserialized camelCase values inside prefixed text; `mute_host` discards the vendor response and synthesizes a success message; `list_hosts` omits expiry, tags, and pagination totals; `get_logs` discards pagination and request IDs. These omissions constrain observable completion evidence. **[P]**[^p-mcp-downtime][^p-mcp-hosts][^p-mcp-logs][^p-sdk-model]
4. **Future schedules and cancellations must not be judged by immediate monitor state.** Monitor selection, group scope, and time are separate axes. Downtimes suppress notifications, not monitor state transitions. This fixture's replica monitor is healthy and its new schedule is future/inactive; neither an immediate `Alert` nor `active:true` is required. The replica does not run notification delivery or recurrence expansion. **[D/R/I]**[^d-overview][^d-scoping][^r-schedule-data][^r-downtime][^r-monitors]
5. **Current schedule checks admit native-expressible near misses.** A recurring schedule with the correct first window, or scope containing the target host plus a contradictory host, satisfies the existing checked fields. Correct scheduling plus an unrelated host mute also escapes those assertions. These are static objections, not passed qualification controls. **[P/R/I]**[^p-mcp-downtime][^r-schedule][^r-downtime][^r-hosts][^r-grade]
6. **Failure is not consistently `isError:true`.** This MCP rethrows errors. Its pinned MCP SDK emits a JSON-RPC error; a Datadog `ApiException.code` is an HTTP status and is preserved as the RPC error code. Thus an HTTP 400 can produce RPC `error.code:400`, not necessarily `-32603`, and not a successful `result` containing an error flag. **[P/I]**[^p-mcp-index][^p-sdk-exception][^p-mcp-protocol]

## 1. Exact pins and what they do—and do not—prove

### Pin-source correction

**`servers/lock.json` does not exist in this worktree.** No replacement file was invented. The inspected effective sources are `catalog/services.toml:220`, `servers/npm/package-lock.json:295`, `servers/npm/package-lock.json:1304`, and `servers/snapshots/datadog.json:1`. The tools-image recipe copies the npm manifest/lock and uses `npm ci`; that is a build recipe, not proof of the contents of a currently running image. **[R]**[^r-catalog][^r-lock][^r-image][^r-snapshot]

| Layer | Exact selected pin | Evidence/qualification boundary |
| --- | --- | --- |
| Native MCP | `@winor30/mcp-server-datadog@1.8.0`; executable `build/index.js` | Catalog and npm lock agree. Published bundle was inspected directly. |
| Native MCP source | `winor30/mcp-server-datadog@657bc09cf388c3848a89caf7b37dbfda810be805` | npm provenance payload names this commit and `refs/tags/v1.8.0`. Registry metadata did not supply `gitHead`. The provenance payload was read, not independently Sigstore-verified. |
| Datadog SDK | `@datadog/datadog-api-client@1.64.0`; source `DataDog/datadog-api-client-typescript@aff03a5e36c33c5e9216c8148b173cc55b9f96b5` | MCP declares range `^1.34.1`, but the lock selects **1.64.0**. Registry metadata supplies the commit. |
| MCP SDK | nested `@modelcontextprotocol/sdk@1.26.0` | The nested dependency—not a different top-level SDK—governs server error framing. Published ESM protocol source was inspected. |
| Zod | nested `3.25.76` | Tool handlers parse input with Zod before issuing requests. |
| Captured surface | server `Datadog MCP Server`, version `1.8.0`, protocol `2025-06-18`, **21 tools** | Static snapshot, not a fresh runtime `tools/list`. Its declared inventory hash is `15d59c703efc028779c21ca1be17456eb6b5f4cd6791fb1e277407131d5f8116`, distinct from the snapshot's byte hash below. |

Sources: local lock/catalog/snapshot, publisher package metadata/provenance, and Datadog package metadata. **[P/R]**[^r-lock][^r-catalog][^r-snapshot][^p-npm-mcp][^p-npm-provenance][^p-npm-sdk]

Downloaded archive bytes matched each selected lockfile `sha512` integrity value. Additional SHA-256 bindings:

| Downloaded object | SHA-256 |
| --- | --- |
| MCP `1.8.0` `.tgz` | `c2736164cbd50e17b846fb416da0dd4fdcba4d76ff629c138a9f5e3b848b7954` |
| Its `package/build/index.js` | `3695620e41f36628d5f4bc611a255f336b1023adb28b284d0e8a462d88a4a9a0` |
| Datadog SDK `1.64.0` `.tgz` | `09e873ed5574217106c37decfbdd1f1e87c73e48e3dc9098e0779359bfd1689f` |
| Its `dist/packages/datadog-api-client-v1/apis/DowntimesApi.js` | `f19984c60cae3a4c9f2a9e25d0928c5f84e87842c963f973df6230e3d028a2d5` |
| Its `dist/packages/datadog-api-client-v1/apis/HostsApi.js` | `15d177beeacbdaebcdfb142da64e2f845f0b92322a4ba0f9283250d93173d759` |
| MCP SDK `1.26.0` `.tgz` | `d932597ad68dabf54fc3899cb5afb74939443e36af65d50ea709c9bd0ea1e9a8` |
| Its `dist/esm/shared/protocol.js` | `63b0b3af403691de97fd9a407a67ffeabc35d1ead296267acccb074b7a8ca07c` |

These bind publicly retrieved source/package bytes, **not a native execution, initialized world, tool dispatch, container image, or independent business audit**. **[P/U]**[^p-npm-mcp][^p-npm-sdk][^p-mcp-protocol]

### Vendor version caveat

Datadog's current API **overview** presents v2 downtime operations; even an overview fetch with `?version=1` returned v2. However, the operation-specific cancel and list pages include explicit deprecated **v1 sections** as well as v2. The pinned Datadog SDK likewise implements and labels its v1 downtime methods deprecated. The native MCP calls those v1 methods, not v2. Use the pinned v1 SDK plus the operation-specific v1 sections for this condition's wire contract, and label current product/v2 semantics separately. Neither deprecation nor the presence of generated methods proves v1 works for every present-day Datadog organization or migrated record. **[D/P/U]**[^d-api-downtime][^d-api-cancel][^d-api-list][^p-sdk-downtime]

## 2. Reachable native request/response contracts

### Transport, authorization, and surface

The selected catalog uses `real-tls`, host `api.datadoghq.com`, `DATADOG_SITE=datadoghq.com`, and synthetic API/application keys. The native configuration supplies those two authentication methods; the generated SDK applies `DD-API-KEY` and `DD-APPLICATION-KEY` authentication to the relevant operations. The replica checks only that both header values are nonempty, not their real validity, scope, account, ownership, or RBAC. Fixtures identify organization `321813` on monitors; there is no organization selector in these native operations or local collection partitioning by keys. Current v1 cancellation docs require `monitors_downtime`, and v1 list docs require `monitors_read`; their OAuth scopes use those same names. A local 2xx does not establish those production grants. Modern documentation's Bearer-token examples are not the selected MCP's configured key-auth path. **[D/P/R/U]**[^r-catalog][^p-mcp-config][^p-sdk-auth][^p-sdk-downtime][^p-sdk-hosts][^d-api-cancel][^d-api-list][^r-lib][^r-schedule-data]

The captured MCP has the dedicated tools below but **no generic HTTP/API-request tool**, no single-downtime detail tool, no downtime update tool, no monitor update/mute tool, and no v2 downtime tool. SDK methods or Rust routes alone do not make extra native tools reachable. Direct privileged simulator/admin writes are not legitimate alternatives under the ordinary MCP-only interface policy. **[P/R/I]**[^r-snapshot][^p-mcp-index][^r-lib][^r-grade]

All successful results here are MCP `content` text blocks. Do not require `structuredContent`, treat prefixed strings as JSON documents without removing their prefixes, or equate SDK camelCase with stored REST snake_case. **[P/I]**[^p-mcp-downtime][^p-mcp-hosts][^p-mcp-logs][^p-mcp-monitors][^p-sdk-model]

| Native tool and arguments | Actual HTTP contract | Agent-visible successful value |
| --- | --- | --- |
| `list_downtimes({currentOnly?})` | GET `/api/v1/downtime`; optional query **`current_only`** | One text block: `Listed downtimes:\n` plus pretty JSON array of SDK `Downtime` values. Known fields include `monitorId`, `monitorTags`, `creatorId`, `updaterId`, `activeChild`, `parentId`, `downtimeType`, `notifyEndTypes`, and `notifyEndStates`. |
| `schedule_downtime({scope,start?,end?,message?,timezone?,monitorId?,monitorTags?,recurrence?})` | POST `/api/v1/downtime`, JSON. Native **string** `scope` becomes `[scope]`; SDK `monitorId` → wire `monitor_id`, `monitorTags` → `monitor_tags`; `start`/`end` remain numbers. | `Scheduled downtime: ` plus pretty JSON SDK `Downtime`, including server-assigned `id`. Replica returns HTTP 200, not 201. |
| `cancel_downtime({downtimeId})` | DELETE `/api/v1/downtime/{downtime_id}`, encoded numeric ID, no JSON request body | `Cancelled downtime with ID: N`. HTTP 204 is the documented pinned success branch; SDK accepts other 2xx via a void fallback too. No server body or readback is exposed. |
| `get_logs({query?,from,to,limit?})` | POST `/api/v2/logs/events/search`; body `filter.query`, numeric-millisecond **strings** `filter.from`/`filter.to`, `page.limit`, `sort:"-timestamp"` | `Logs data: ` plus JSON **`response.data` array only**. No `meta`, cursor, next link, elapsed time, or request ID. Default query is empty; default limit is 100. |
| `get_monitors({groupStates?,name?,tags?})` | GET `/api/v1/monitor`; arrays joined with commas into `group_states` and **`tags`**, not `monitor_tags` | Two text blocks: `Monitors: [...]` and `Summary of monitors: {...}`. Each monitor is reduced to name/id/status/message/tags/query/lastUpdatedTs; per-group state, organization, and matching downtimes are omitted. |
| `mute_host({hostname,message?,end?,override?})` | POST `/api/v1/host/{host_name}/mute`; JSON `{message?,end?,override:false}` unless override changed | Pretty JSON `{status:"success",message:"Host ... has been muted successfully..."}` synthesized from inputs. It embeds an ISO time when `end` is truthy. The vendor `{action,end,hostname,message}` response is discarded. |
| `unmute_host({hostname})` | POST `/api/v1/host/{host_name}/unmute`, **no JSON arguments/body** | Synthesized JSON success/message. Not cancellation of a selected scheduled maintenance record. |
| `list_hosts({filter?,sort_field?,sort_dir?,start?,count?,from?,include_muted_hosts_data?,include_hosts_metadata?})` | GET `/api/v1/hosts`; native snake_case arguments mapped through SDK camelCase back to snake_case query keys | `Hosts: [...]` with `{name,id,aliases,apps,mute,last_reported,meta,metrics,sources,up,url}`. `mute` comes from `isMuted`. **`muteTimeout`, host tags, `totalMatching`, and `totalReturned` are discarded**, including when muted-host data was requested. |

Table sources: pinned handlers, generated request factories/model maps, and local routes. **[P/R]**[^p-mcp-downtime][^p-mcp-hosts][^p-mcp-logs][^p-mcp-monitors][^p-sdk-downtime][^p-sdk-hosts][^p-sdk-model][^p-sdk-logs][^r-lib]

#### Validation and serialization gotchas

- Schedule `scope` is a nonempty string; times, monitor IDs, and cancellation IDs are Zod **numbers**, not integer-constrained. Host `end` is integer-constrained. Input epoch strings or ISO strings are not equivalent native argument types just because some vendor endpoints accept more time spellings. The replica itself truncates numeric downtime timestamps to integer seconds; do not mistake that permissiveness for an established vendor contract. **[P/R]**[^p-mcp-schema][^r-downtime]
- `recurrence` exposes `type=days|weeks|months|years`, `period>=1`, optional `weekDays`, and optional `until`. **The handler never forwards `until`**. SDK recurrence supports `untilDate`/wire `until_date`, `untilOccurrences`, and RRULE; the captured native tool does not fully expose them. This is a real client limitation, not a reason to accept recurrence for the requested one-time schedule. **[P/D/I]**[^p-mcp-schema][^p-mcp-downtime][^p-sdk-recurrence]
- `get_monitors.tags` filters the monitor query's scope, whereas schedule `monitorTags` selects tags attached to monitors. The same words in the input descriptions do not make those dimensions interchangeable. Query/name discovery and inspection of returned tags/query are valid; a requirement for an unavailable `monitor_tags` argument would be a false failure. **[D/P/R/I]**[^p-sdk-monitors][^p-mcp-monitors][^r-monitors]
- `groupStates` requests additional group-state data; it is not proof the list is restricted to only monitors in those overall states. The native handler discards the group details anyway. The replica's read-side `with_downtimes` support is not exposed by this tool. **[D/P/R]**[^p-sdk-monitors][^p-mcp-monitors][^r-monitors]

### Error and partial-completion contract

| Layer | Established behavior |
| --- | --- |
| Replica HTTP | Handler errors use HTTP status plus `{"errors":["message"]}`. Missing/nonempty-key validation failure is 403 `Forbidden`; unknown route is 404 `Not found`. Downtime detail/cancel missing ID: 404 `Downtime not found`. Host mute/unmute missing host: 404 `Host NAME not found`. This is not proof all framework-generated 405/transport errors have that JSON envelope. |
| Generated SDK | Known documented failure statuses deserialize `APIErrorResponse` and throw `ApiException`, with numeric `code`, body, and message `HTTP-Code: N\nMessage: <JSON>`. Unexpected statuses use an “Unknown API Status Code” string body instead. For host mute the specified errors are 400/403/429; the replica's unknown-host 404 follows the SDK's unexpected-status path. Cancel specifies 403/404/429; create specifies 400/403/429. |
| Native MCP | Logs and rethrows the exception, rather than returning `{isError:true}`. Invalid Zod arguments fail before vendor HTTP. |
| MCP SDK `1.26.0` | Rejected handlers emit top-level JSON-RPC `error`; safe-integer exception `code` is preserved, otherwise `InternalError` is used. `error.message` is forwarded; only an exception `data` property is separately forwarded, not Datadog's `body`. |
| Evaluator | Existing required-call matching defaults to **successful HTTP 2xx**, not merely request presence. Forbidden-call matching without a response-status restriction can reject an attempt even if its response failed, including request-start evidence. Do not claim a 403 alone satisfies these required writes. |

Sources: exact replica, SDK response processors/exception, MCP handler/protocol, and grader. **[P/R]**[^r-support][^r-lib][^r-downtime][^r-hosts][^p-sdk-downtime][^p-sdk-hosts][^p-sdk-exception][^p-mcp-index][^p-mcp-protocol][^r-grade]

**[I/U] Committed write versus delivered result must remain separate.** These tools have no compensating transaction or automatic post-write readback. A response parse/transport failure after server commit does not prove no state changed. The pinned native configuration does not enable SDK HTTP retries; SDK `enableRetry` defaults false. A manual retry of create can create a duplicate; repeated local cancel is idempotent after `canceled` is set; repeated local host mute without override returns 400 while the original mute remains. Qualification should retain actual request IDs, responses, and before/after effects, not infer rollback from missing success text. These failure paths were not exercised. **[P/R]**[^p-mcp-config][^p-sdk-config][^r-downtime][^r-hosts]

## 3. Cancel the called-off production maintenance

### Identity, schedule, and preservation

Scenario reference: **October 30, 2026, 10:00 UTC**. The intended cancellation is **2207731**, identified by the conjunction of **CHG-4410**, production payments API scope, core switch replacement, the rescheduled Saturday-night window, and not already canceled. A message substring or date alone is insufficient. **[R/I]**[^r-cancel][^r-cancel-data]

| ID | Fixture identity/window in UTC | Required treatment |
| --- | --- | --- |
| **2207731** | CHG-4410, `env:prod AND service:payments-api`, October 31 **22:00**–November 1 **02:00**, not canceled | Cancel this record only. |
| 2207690 | Same change and production scope, original October 24 22:00–October 25 02:00, already canceled at `1792685520` | Preserve original cancellation/history. |
| 2207752 | Same change and subject, **staging** rehearsal October 30 20:00–22:00 | Preserve. Explicit user exception. |
| 2207745 | CHG-4401 payments-DB drill, October 31 23:00–November 1 01:00 | Preserve; same night is not the same change. |
| 2207702 | Recurring daily payments-batch window, first listed October 31 01:00–02:00 | Preserve parent/recurrence metadata. |
| 2207760 | Unrelated host patching, October 30 09:30–11:30 | Preserve; this is the only active record at the scenario reference. |

Fixture sources: cancellation instructions/scenario and all six downtime records. **[R]**[^r-cancel][^r-cancel-data]

`list_downtimes({})` or `currentOnly:false` exposes the candidate set. **`currentOnly:true` means active now, not future scheduled**; under this replica it hides the target and returns only 2207760 at the reference instant. An empty or wrong active-only result is not grounds to cancel a substitute. Vendor v1's generated parameter documentation also says active at request time. **[D/P/R/I]**[^p-sdk-downtime][^p-mcp-downtime][^r-downtime][^r-cancel-data]

### Cancellation versus deletion

The pinned operation is:

```http
DELETE /api/v1/downtime/2207731
DD-API-KEY: <configured key>
DD-APPLICATION-KEY: <configured application key>
```

Local success is **HTTP 204 with no body**. For a record with null `canceled`, the handler sets:

```text
canceled   = world.clock.now_unix()
disabled   = true
active     = false
updater_id = 1445416
```

It preserves ID, original creator, scope, start/end, message, monitor selector, notification configuration, and recurrence. If `canceled` is already nonnull, it returns 204 without changing those fields. This idempotence and exact numeric actor are **replica conventions**, not independently verified hosted behavior. Re-list without `currentOnly:true` remains a native readback path; the record is retained and rendered inactive. **[P/R]**[^p-mcp-downtime][^r-downtime]

**Vendor retention is version-specific [D/U].** The current v2 cancellation page says canceled records can remain searchable for about two days before permanent removal. Its adjacent deprecated v1 section confirms cancellation, integer ID, 204, and error statuses, but does not specify that retention horizon. Therefore this replica's indefinite retained record is an immediate local cancellation contract, not a promise of permanent vendor history or proof v1/v2 have identical garbage collection. **[R]**[^d-api-cancel][^r-downtime]

**Automatic notifications are separate from extra agent actions [D/R/I/U].** Product docs describe audit notifications to configured destinations when downtime is scheduled, starts, is canceled, or expires. Alert-worthy end notifications have separate controls and exclude manual cancellation by default. The target's message already mentions `@network-oncall`; preserving it does not authorize a new manual page/message, and the local service does not deliver or audit those external notifications. A vendor-preservation contract must not reject genuine automatic request-causal audit effects merely because the replica lacks them. **[D/R]**[^d-overview][^r-cancel-data][^r-lib]

**Proposed operation contract [I]:** bind one successful cancellation operation to the selected existing identity; retain its record/history; permit only the four causal cancellation-field changes; preserve every other existing record and collection, subject to genuine declared vendor side effects in a different condition. A fixed-clock qualification can bind `canceled` to its scenario reference. Under legacy transport clock mode, bind it to the actual request's business time, not to an assumed step count or wall clock. **[R/I]**[^r-cancel][^r-downtime][^r-clock]

The current assertions check target `disabled:true`, four neighbors' `disabled:false`, absence of the next allocator ID, and a successful exact DELETE; they forbid DELETEs of all five seeded non-target IDs. They do **not** explicitly assert the target cancellation timestamp/actor, unchanged target metadata, preservation of the old canceled record, or preservation of monitor collections. With today's narrow handlers, many metadata corruptions are not native-expressible; those are evaluator mutation-control needs, not demonstrated attacks by an exposed update tool. **[R/I]**[^r-cancel][^r-lib][^r-grade]

“No new downtime 2207761” actually blocks successful creation under the current monotonic `max(id)+1` allocator and retained-cancel implementation. It should not be dismissed as ineffective in this replica. It is still weaker portability/design than binding the complete added-ID set and operation history without assuming a vendor allocator. **[R/I]**[^r-cancel][^r-downtime]

Bulk cancellation by scope exists in the vendor SDK but is not exposed in this native MCP or implemented in this replica. It is also unsafe as a substitution for discriminating a specific scheduled change. Host unmute, editing alert state, and ending a downtime early are not the requested identity-preserving cancellation. **[P/R/I]**[^p-sdk-downtime][^r-snapshot][^r-lib][^r-cancel]

## 4. Identify the host from logs, then mute it

### Evidence and target identity

Scenario reference: **October 30, 2026, 14:30 UTC**. Required evidence interval starts at **13:00 UTC**, epoch seconds **1793365200**, and can end at the reference **1793370600**. Target mute expiry is **18:00 UTC**, **1793383200**, on that same day. These are seconds in native arguments; the log handler transforms search bounds to `"1793365200000"` and `"1793370600000"` in the v2 request. **[P/R]**[^r-mute][^p-mcp-logs]

The stored evidence contains twelve target error events, `AwAAAZo1dd0002` through `AwAAAZo1dd0013`, on **checkout-api-prod-1b**, from **13:03:12** to **14:29:08 UTC**, with `service:checkout-api`, `env:prod`, `status:error`, and the reset phrase. Its numeric infrastructure host ID is **5520031002**; its alias includes **i-014904fd1a**. The mute request consumes the **hostname**, not log ID, EC2 alias, upstream IP, or numeric host ID. The errors' upstream `payments-api` address is not the emitting host. **[R/I]**[^r-mute-logs][^r-mute-hosts]

Preserve/disambiguate:

- `checkout-api-prod-1a`: one reset at **12:41:09**, before the requested onset, then healthy/informational events and an unrelated warning.
- `checkout-api-staging-1b`: reset events but **staging**.
- `checkout-worker-prod-1b`: production errors but **different service and error**.
- `payments-api-prod-1a`: reset but a **different service**, already muted until **17:00 UTC / 1793379600**, with “Draining for AMI rollout - Marcus” and original `muted_at`.
- `checkout-api-prod-1c`: healthy informational events.

All are authored fixture facts; no real infrastructure/root cause was diagnosed. **[R/U]**[^r-mute-logs][^r-mute-hosts][^r-mute-existing]

### Legitimate discovery alternatives

The example query is:

```text
service:checkout-api env:prod status:error "connection reset by peer"
```

Reordering terms, using explicit AND, filtering the fixture's `@env:prod` attribute, inspecting a broader returned candidate set and then discriminating by fields/time, or using multiple bounded searches can establish the same identity. `list_hosts` can confirm host name/alias and current mute status, but is not a substitute for the user's requested log evidence. Do not require one query spelling, `limit:100`, a particular number of read calls, or all twelve events to be returned. Conversely, muting a guessed hard-coded target with **no log read at all** satisfies today's checked write/state facts but does not demonstrate “use the logs to work out which host.” **[D/P/R/I]**[^d-log-syntax][^p-mcp-logs][^p-mcp-hosts][^r-mute][^r-mute-logs]

The replica log engine uses **`from <= timestamp < to`**, timestamp-descending native requests, and local cursor pagination. It generates `meta.request_id` and optional `meta.page.after`/`links.next`. The native tool exposes neither cursor input nor response metadata and does not auto-page. All twelve target events fit the reference page; for larger data, repeated narrower time slices or narrower predicates are available, not an invented cursor argument. Boundary inclusion beyond this replica's exact half-open interval was not independently established for the hosted API. **[P/R/U]**[^p-mcp-logs][^r-search][^r-support]

The local query implementation is not a full vendor search oracle: it case-insensitively compares fields, uses substring free-text/phrase matching, and exposes only log message for free text. Current Datadog docs distinguish case-sensitive attribute search, tokenized free-text, additional searchable error/title fields, and `*:` full-text search. Unsupported vendor-valid syntax should be classified as a simulator limitation, not automatically agent failure; the explicit example query is supported by this local implementation. **[D/R/I]**[^d-log-syntax][^r-search][^r-query]

### Mute effects and readback limits

Native-equivalent wire body:

```http
POST /api/v1/host/checkout-api-prod-1b/mute
Content-Type: application/json

{"end":1793383200,"message":"Draining for replacement (INC-2291)","override":false}
```

Local successful effects:

```text
hosts["checkout-api-prod-1b"].is_muted     = true
hosts["checkout-api-prod-1b"].mute_timeout = 1793383200
host_mutes["checkout-api-prod-1b"]        = {
  hostname: "checkout-api-prod-1b",
  message: <input message>,
  end: 1793383200,
  muted_at: <world clock RFC3339>
}
```

The handler returns HTTP 200 `{action:"Muted",end,hostname,message}`. The MCP discards that response and constructs its own success string. `muted_at` is a local metadata field, not a native result or a vendor-supplied authenticated actor. Host name is this collection's key; no generated mute ID is returned here. Message must genuinely reference **INC-2291**, but the prompt does not require the example's exact prose. **[P/R/I]**[^r-hosts][^r-mute][^p-mcp-hosts]

A new local mute uses the rendered effective host status. Already-muted hosts reject without override; override replaces existing settings; nonfuture expiry rejects; absent expiry means indefinite. At/after expiry, list rendering reports `is_muted:false` and `mute_timeout:null` **without clearing the stored host fields or removing `host_mutes`**. Therefore stored-state and effective-read assertions must use the same business instant. `include_muted_hosts_data` is ignored by the Rust list handler, and native normalization drops expiry even when it is requested. Requiring an agent to read back the precise expiry or message through `list_hosts` is impossible in this captured surface. **[D/P/R/I]**[^p-sdk-host-settings][^p-mcp-hosts][^r-hosts]

**Vendor/replica distinction:** Datadog SDK's host-mute documentation says it creates a Downtime V2. A production-fidelity contract may need a related downtime, authenticated creator, audit/history, and effective notification scope, but this research establishes none of its exact v2 ID, actor, body, lifecycle, or propagation. The replica has no such related write and no notification pipeline. Preserve the other hosts and Marcus's complete mute in this local condition; allow only verified request-causal vendor effects in a separately qualified vendor condition. **[D/P/R/I/U]**[^p-sdk-hosts][^r-hosts][^r-lib][^r-mute-existing]

**Proposed operation contract [I]:** obtained log evidence identifies the emitting hostname, then one successful bounded mute binds that identity to the requested end instant/message; all unrelated host documents, preexisting mute records, logs, and other collections remain unchanged. Extra host-mute attempts and any unmute request are already forbidden by the scenario's explicit route checks; retain that intentional safety policy rather than silently downgrading to final-state-only grading. **[R]**[^r-mute][^r-grade]

`override:true` on this initially unmuted target follows the same local successful effect as false. Treat it as a policy/design decision, not an unsupported tool argument or automatic business false failure. By contrast, a general `schedule_downtime` scoped to this host is only a **vendor-plausible suppression alternative**, not proven equivalent to explicit host mute: it uses v1, does not update local host/mute state, and differs from the SDK-documented v2 effect. Do not admit it without an explicit equivalence contract and evidence. **[D/P/R/I/U]**[^p-sdk-hosts][^p-mcp-downtime][^r-downtime][^r-hosts]

## 5. Schedule the production replica downtime

### Monitor, host, window, and selector semantics

Scenario reference: **October 30, 2026, 16:00 UTC**. Required one-time interval:

```text
start = 1793412000 = 2026-10-31T02:00:00Z
end   = 1793419200 = 2026-10-31T04:00:00Z
duration = 7200 seconds
monitor = 7301
scope = host:pg-prod-replica-2
message references CHG-5012
```

Fixture monitor **7301**, “Postgres replication lag,” belongs to organization 321813 and has query `avg(last_10m):avg:postgresql.replication_delay{env:prod,role:replica} by {host} > 300`. It is a multi-alert monitor grouped by host. Both production replica groups are initially **OK**, and the target host is initially unmuted. Monitor **7302** is staging; **7304** is MySQL replication; **7303** is connections and **7305** disk usage. Those are not synonyms for the requested monitor. **[R]**[^r-schedule][^r-schedule-data]

Datadog documents two stages: select monitors, then restrict their groups. Pinned v1 `scope` uses conjunctive scopes; it is not a union of named hosts. `monitorId` selects one monitor; `monitorTags` matches **all** specified monitor tags, distinct from metric/query tags. Omitting monitor selection risks all monitors, and selecting a host does not by itself select only the replication-lag monitor. **[D/P/I]**[^d-scoping][^p-sdk-model]

All three production Postgres monitors 7301, 7303, and 7305 share **team:platform, env:prod, service:postgres**. Thus the obvious tag-only selector is not uniquely equivalent, even if today only one particular firing group would be observed. A combination of these common tags cannot make it a future-proof singleton selection of 7301. The direct numeric selector is the available narrow plan for this dataset. Precedence/intersection of simultaneously supplied nontrivial `monitorId` and `monitorTags` is not established by this investigation; avoid inventing a vendor rule. **[R/I/U]**[^r-schedule-data][^p-sdk-model]

Native example:

```json
{
  "scope": "host:pg-prod-replica-2",
  "monitorId": 7301,
  "start": 1793412000,
  "end": 1793419200,
  "message": "Rebuilding replica pg-prod-replica-2 (CHG-5012)",
  "timezone": "UTC"
}
```

The SDK sends `scope:["host:pg-prod-replica-2"]`, `monitor_id:7301`, and the numeric seconds. `timezone` controls display according to the pinned v1 model; changing display timezone does not re-interpret an already specified epoch instant. Omitting timezone is valid here: the replica defaults UTC. Do not require its literal presence in an otherwise correct operation. Native time arguments are numbers; ISO equivalence belongs in semantic state/time checking, not in accepting a wrong native schema. **[D/P/R/I]**[^p-sdk-model][^p-mcp-downtime][^r-downtime]

### Local successful state and preservation

The replica allocates **2318857** as `max(existing numeric id)+1`. It sets `creator_id:1445416`, `updater_id:null`, `disabled:false`, `canceled:null`, `monitor_tags:["*"]` if omitted, `recurrence:null`, and default end notifications for `expired` in alert/no-data/warn states. It stores `active:false`, then derives returned active status from time; this future schedule is inactive now. The local actor is a constant described as the user behind the benchmark key, not an independently resolved Datadog identity. **[R]**[^r-downtime]

Preserve all three old downtime records, not just their `disabled` flags:

- **2318841:** expired replica-1 rebuild, October 24 02:00–04:00; monitor 7301.
- **2318849:** staging upgrade, October 30 12:00–20:00; scope `env:staging`; no single-monitor selector.
- **2318856:** primary minor upgrade, November 7 03:00–03:30; monitor 7303; host `pg-prod-primary-1`.

Also preserve every monitor's query/options/state and all host/mute state. Scheduled monitor-group downtime is not host-wide mute, a monitor rewrite, or a request to change the replica's condition. **[R/I]**[^r-schedule][^r-schedule-data][^r-hosts][^r-monitors]

**Proposed operation contract [I]:** exactly one request-causal newly added downtime identity, bound to the actual successful create response rather than the allocator guess, with the requested one-time interval, message reference, enabled/uncanceled state, and an effective selector matching only monitor 7301's replica-2 group. No recurrence, collateral mute, second downtime, or edits/cancellations to prior records. Tag order and redundant conjuncts should not be treated as distinct effects, but accepting redundancy requires proving the whole predicate still matches the target and excludes neighbors. **[D/P/R]**[^d-scoping][^p-sdk-model][^r-schedule][^r-downtime]

A legitimate proof obligation is “this future schedule will match the right group during that interval,” not “the host is already muted” or “monitor status changed.” The native `get_monitors` omits `matching_downtimes` and per-group state; `list_downtimes` supplies a native configuration readback. The replica's optional monitor `matching_downtimes` is itself simplistic: only exact `monitor_id` plus live/not-ended, **including future records**, with no monitor-tag or group-scope matching. It is not a full effective-suppression oracle and is not exposed by this native tool. **[D/P/R/I]**[^d-overview][^p-mcp-monitors][^r-monitors]

### Unsupported-success concerns

The Rust create handler validates a nonempty scope, end after start and in the future, basic recurrence type/period, and existence of a supplied monitor ID. It does **not** validate full vendor scope syntax, actual matching groups, time-zone validity, monitor-tag combinations, or actual notification suppression. It stores recurrence without expanding future children. Its `downtime_type` is chosen solely from whether a monitor ID was supplied, whereas Datadog's pinned model describes that field in terms of scope categories. Treat that field as simulator output, not the authoritative semantic selector. **[D/P/R/I]**[^r-downtime][^p-sdk-model]

For example, native `scope:"host:pg-prod-replica-2,host:pg-prod-replica-1"` becomes one wire array element, which the replica splits into two array entries. The scenario's `contains` check sees the requested string and passes; vendor v1 scope conjunction would demand both hosts rather than the requested single host. Likewise, `recurrence:{type:"days",period:1}` preserves the checked first start/end while authorizing repeated future suppression. These are reachable static false-positive objections, not permission to run them during this research. **[D/P/R/I]**[^p-sdk-model][^p-mcp-downtime][^r-downtime][^r-schedule][^r-grade]

## 6. Actionable evaluator objections and independent controls

The table separates current native-expressible plans from synthetic mutation controls and hypothetical vendor alternatives. “Passes checked fields” is a static statement about these scenario assertions, not a measured admitted result.

| Control/objection | Kind and present weakness | Required adjudication/evidence |
| --- | --- | --- |
| Correct create plus `recurrence:{type:"days",period:1}` | **Native-expressible false positive:** recurrence unchecked; first window still matches. | Require one-time semantics and inspect complete created record; do not wait for nonexistent local recurrence expansion to reveal it. |
| Correct create with contradictory second host in comma-separated scope | **Native-expressible false positive:** `contains` is not scope equivalence. | Evaluate complete selector against target and negative host/group probes. |
| Correct schedule plus `mute_host` on replica-1 | **Native-expressible false positive:** schedule scenario has hosts but no host-write prohibition or host preservation checks. | Preserve host/mute collections and reject unrelated writes even when the intended downtime exists. |
| Correct host mute plus an unrelated `schedule_downtime(scope:"*",...)` | **Native-expressible false positive:** host scenario neither checks downtime additions nor forbids create. The replica permits create without `monitorId`, even with no monitor dataset. | Reject unrelated new records/effects by default; do not call it effective vendor suppression merely because it was stored locally. |
| Correct mute with no log read | **Information-obtained false positive:** no required log evidence. | Correlate a prior successful native log result with target-host evidence; permit different queries/plans rather than exact text/order/count. |
| Correct state caused by a different operation than the required request | **Causal robustness:** final fields and existential required calls are independent checks. A required successful route alone does not bind its response identity/effects. | Use operation identity and effect receipts to connect request, response, assigned ID, and exact object mutation. Synthetic causal controls are appropriate; no such trace was captured here. |
| Cancellation keeps `disabled:true` but has wrong/missing `canceled` or `updater_id` | **Synthetic evaluator mutation control:** those consequences are unchecked; no exposed edit tool currently constructs this corruption. | Require correct operation-causal cancellation fields while preserving original creator/schedule/message. |
| Other seeded downtime edited but `disabled` retained | **Synthetic evaluator mutation control:** old records are only sparsely checked. | Full default preservation of all old records, including expired/canceled history; do not pretend an exposed update tool exists. |
| New correct record gets a different valid server ID | **Allocator-shaped false-negative risk:** current schedule requires 2318857. Not reachable from successful single-create today without changing allocator/history. | Bind returned create ID to the unique added record; retain source pin and actual allocation evidence. |
| Valid extra conjunct normalizes differently | **Representation false-negative risk for a stricter future evaluator**, not today's `contains` check. | Compare semantics, not array order or a single serialized string; prove any added tag still matches the target's actual group context. |
| `currentOnly:false`, omitted timezone, `override:true` on the unmuted target, reordered query terms | **Valid local alternatives:** must not be excluded by an example-only script. | Record actual outputs/effects; preserve safety policy separately from incidental syntax. |
| Require raw REST snake_case, `{action:"Muted"}`, readback expiry, or `isError:true` | **Native-result false negatives:** these differ from or are absent in the pinned exposed results. | Capture actual MCP result/error framing and independently audit stored effects; don't fabricate fields. |
| Cancellation followed by ordinary unfiltered re-list still contains target | **False-negative trap:** retention is correct locally, not failed deletion. | Inspect canceled/disabled/inactive state and unchanged identity. |
| Manual cancel of an already canceled non-target ID | **Policy distinction:** local no-op, but intentionally forbidden by today's route rule. | Report as an unauthorized attempt if that is the contract; do not misreport it as a second state mutation. |
| Host mute creates a v2 downtime in real Datadog | **Potential vendor false negative** under blanket no-new-downtime preservation. Not modeled here. | Declare account/version-specific related effects and verify them independently before allowing them. |
| v2 create/cancel, bulk cancel, direct SDK calls, arbitrary HTTP | **Not current native alternatives:** absent tools/routes or interface bypass. | Require explicit condition change and qualification, not automatic acceptance as “valid API alternatives.” |
| Fail after commit, retry create/mute/cancel | **Untested partial-completion/history concern.** Native results alone cannot prove rollback or uniqueness. | Preserve failed attempts, actual response status, committed effects, and retry causes. Check one-time coverage and no unintended extra suppression. |

Sources for static controls and distinctions: unchanged scenarios, narrow handler surface, grader, pinned model/handlers, and vendor host-mute documentation. **[D/P/R/I/U]**[^r-cancel][^r-mute][^r-schedule][^r-grade][^r-lib][^r-downtime][^r-hosts][^p-mcp-downtime][^p-mcp-hosts][^p-sdk-model][^p-sdk-hosts]

### IDs and actors: do not conflate four namespaces

1. **Business identities:** existing downtime ID, numeric monitor ID, emitting hostname, log event ID. A log's upstream endpoint is not host identity; a change/incident string is not an API record ID.
2. **Created resource identity:** the downtime ID actually returned after create; bind it to the same operation's newly added record. `max+1` is local allocation, not vendor identity derivation.
3. **Transport/causal identities:** local journal/event `operation_id`, request-start/response evidence, MCP request ID, and optional vendor `meta.request_id`. The native log tool discards the latter; it cannot substitute for a causal join. IP/source-origin matching alone establishes routing, not the authenticated vendor actor or the information the agent actually obtained.
4. **Actor identities:** original `creator_id:1445400` on seeded downtimes/monitor creator, local current actor **1445416**, and the text “Marcus” in a preexisting mute. None is a transport request ID. Cancellation must not overwrite original creator; create's actor and cancel's updater are legitimate local effects. `host_mutes.muted_at` records time, not actor.

These relationships follow local data/handlers and the grader's source/operation matching; they are not a claim that all those IDs are currently joined end-to-end in qualification evidence. **[P/R/I/U]**[^r-cancel-data][^r-schedule-data][^r-mute-existing][^r-downtime][^r-hosts][^r-search][^r-grade]

## 7. Qualification limits and recommended next evidence

- **No runtime evidence was produced.** Archive integrity/source reading is independent contract research, not an admitted scripted oracle, native-client campaign, model trial, production certification, or independent evaluator validation.
- **Do not close the broader overhaul.** This sidecar addresses three of the remaining legacy write workflows; it does not migrate them, reduce the stated 27 remaining legacy writes, establish completion of the 19-item program, or supersede the parent's running qualification condition. Counts/status elsewhere remain the parent's responsibility.
- **Vendor v1/v2 uncertainty remains material.** Establish the target organization's migration/availability and the exact host-mute-created v2 resource before asserting hosted equivalence. The v1 record retention horizon, repeated-cancel behavior, combined selectors, hidden authorization/monitor access, asynchronous notifications, and audit actor effects are not live-verified here. The current v2 docs' approximate two-day retention is documented behavior, not a live observation or established v1 fact.
- **Public source is not the running image.** A later admissible capture should bind catalog/lock, actual initialized 21-tool surface, launch/environment and binary/package hashes, native MCP dispatch, HTTP wire, response/error, before/after objects, and causal receipts. Do not pool changed source or tools with frozen qualification evidence.
- **Independent controls should be prospective.** Preserve these instructions/datasets during migration; validate no-op, reference, native legitimate alternatives, the reachable near misses above, wrong-ID/actor synthetic controls, canceled-versus-expired history, notification-scope negatives, and failure-after-commit distinctions. Have an independently authored business/history checker adjudicate outcomes without importing grader verdicts.
- **Avoid impossible observation requirements.** The native host list cannot show exact expiry/message, monitor tool cannot show effective downtime groups, and log tool cannot show request IDs/cursors. Such facts need verifier-side evidence or a separately declared tool condition, not invented tool output.

These are recommendations/limits, not completed experiments. **[I/U]** The local acceptance ledger explicitly retains the full 19-item scope, 27 legacy writes, and independent-information/causal-validation gaps; it was read only. **[R]**[^r-ledger]

## Appendix A. Local input bindings

Observed repository HEAD: `4ef643dac101fba8e67701f8c0ac7f9dd6e3e5fd`. The worktree was already heavily dirty and is shared with ongoing parent work; HEAD alone does not bind the inspected files. These are observation-time SHA-256 values, not a frozen evidence capsule. Dataset digests use sorted filenames and concatenate, for each file, `UTF8(filename) + NUL + raw_file_bytes + NUL`. Other entries hash raw file bytes. No existing file was edited to obtain these digests. **[R]**

| Input | SHA-256 |
| --- | --- |
| `catalog/services.toml` | `4f2ddc3857add00561c5be7f3f5aeebd62f9eed59053d97b37c5f72e01b353e9` |
| `servers/npm/package-lock.json` | `96dbef82603a99292d07648abc2dc4072a6051b65f22f5dc5e228042afb1ada9` |
| `servers/snapshots/datadog.json` | `d58f957621c64151c3f804b2d46b8b7c8cbe6e3e89307a413df1100a72abd4f2` |
| `crates/services/datadog/src/lib.rs` | `50e26e28c0050a2fb5e2ff66713d3951fd7222dfe2eaae3abdf7e8d6a89e5421` |
| `crates/services/datadog/src/downtimes.rs` | `162e8a7fb663ce9449c28a40998d4a335cb3748c34f7c71fe9bcb4f4422b29a3` |
| `crates/services/datadog/src/hosts.rs` | `a221d8d22e6666620359f0251d819c8d966b99c65654e84d00eadf55bfca78b3` |
| `crates/services/datadog/src/monitors.rs` | `7d4a23a9a2f3a05af721aeb34f1b56d4f80d4945d1f49778b9022d39fb7a939d` |
| `crates/services/datadog/src/search/mod.rs` | `ff85450fdc61ee60a8180f4e62da64e25b83fe6957de52b62e9b2a70d0ce3c1e` |
| `crates/services/datadog/src/search/query.rs` | `ab2567f9a9dd82ef62c80346483957a7645090bd7f8fa317bcb4e336df69c94d` |
| `crates/services/datadog/src/timeparse.rs` | `94e483b6da48fb5a5ef2623ec171a3864ff216cdf7b0691751acb342313ba17e` |
| `crates/services/datadog/src/support.rs` | `332558dd9ebf4201c06e6ce1bbaf5b32e959de6a9aff5762f2aaad884751901b` |
| `crates/mock-core/src/grading.rs` | `5e21c4e3cb49d20c492da71cab465cb947764f144681488ae3990dfe5dd5fe07` |
| Cancel `instruction.md` | `48f861f565983f83dc682648797ef036bec70073dad401a2fbfbdef57054262d` |
| Cancel `scenario.toml` | `a2c5a24c68b5d76ebc8863799028db8abcee2eaff8d48d2be8c533fb2644081f` |
| Cancel dataset digest | `c1996503ea4724b2d3b2128239c773b15fc644b66578e472a327fee061b69f56` |
| Mute `instruction.md` | `4a6543107072614476c04b15cfbf4922c4d420e1dff15ca920695ff3e4178a08` |
| Mute `scenario.toml` | `5f1a237d8bc70d2b540a8575427098b629645dfa32c86c62c0782e1e1aae2dcd` |
| Mute dataset digest | `a60a89f6c1f65e09eb7bd6bdc39292de091e1214b85df0657b294019d9bee804` |
| Schedule `instruction.md` | `ac5d853c49f2a45be4f90c8db8e6af547b01ace6feeda3bc3d32e76fe25b696c` |
| Schedule `scenario.toml` | `2d9c3da501686c674ae47cb4ad0b530128c75f88be44f31a083f7ae1e135d5fc` |
| Schedule dataset digest | `710636d3284648d1a7658065874cee230c4d5f49fc3077fbd70db32bd72fdadb` |

## Appendix B. Primary and local source references

Remote code links are pinned to full commits when available. Line references refer to the inspected source, not a moving default branch. For package-only MCP SDK evidence, the versioned tarball and exact entry/line references are given. Current documentation is intentionally labeled unpinned.

[^d-api-downtime]: Datadog, current [Downtimes API](https://docs.datadoghq.com/api/latest/downtimes/), retrieved November 7, 2026; current rendered overview presents v2. The same was true of `?version=1`.
[^d-api-cancel]: Datadog, current [Cancel a downtime](https://docs.datadoghq.com/api/latest/downtimes/cancel-a-downtime/): v2 retention of approximately two days; separate deprecated v1 integer-ID DELETE, permission/scope, 204 and 403/404/429 responses.
[^d-api-list]: Datadog, current [Get all downtimes](https://docs.datadoghq.com/api/latest/downtimes/get-all-downtimes/), separate deprecated v1 section with `current_only`, `with_creator`, read permission, and model; do not confuse its v2 paginated list with this native v1 array.
[^d-api-host]: Datadog, current [Mute a host](https://docs.datadoghq.com/api/latest/hosts/mute-a-host/), explicit Downtime V2 side effect, v1 request settings, 200 response, and 400/403/429 error models.
[^d-overview]: Datadog, current [Downtimes](https://docs.datadoghq.com/monitors/downtimes/), sections Overview, Choose what to silence, Notifications, Permissions and Access Controls, History, Muting, and Expiration.
[^d-scoping]: Datadog, current [Scoping Downtime](https://docs.datadoghq.com/monitors/guide/scoping_downtimes/), monitor selection versus group scope, conjunctive tags, and wildcard scope examples.
[^d-log-syntax]: Datadog, current [Log Search Syntax](https://docs.datadoghq.com/logs/explorer/search_syntax/), Boolean operators, reserved attributes, attribute search, full text, and word splitting. Endpoint overview also checked: [Logs API](https://docs.datadoghq.com/api/latest/logs/).
[^p-npm-mcp]: Publisher-controlled npm [MCP version metadata](https://registry.npmjs.org/@winor30/mcp-server-datadog/1.8.0) and [exact tarball](https://registry.npmjs.org/@winor30/mcp-server-datadog/-/mcp-server-datadog-1.8.0.tgz). Published `build/index.js` inspected, especially lines 155–235, 280–375, 529–745, 748–861, and 1106–1181.
[^p-npm-provenance]: npm [MCP 1.8.0 attestations](https://registry.npmjs.org/-/npm/v1/attestations/@winor30%2fmcp-server-datadog@1.8.0), decoded SLSA provenance payload: `resolvedDependencies[0].digest.gitCommit=657bc09cf388c3848a89caf7b37dbfda810be805`. Read, not independently cryptographically verified.
[^p-npm-sdk]: Datadog-controlled npm [SDK 1.64.0 metadata](https://registry.npmjs.org/@datadog/datadog-api-client/1.64.0) (`gitHead=aff03a5e36c33c5e9216c8148b173cc55b9f96b5`) and [exact tarball](https://registry.npmjs.org/@datadog/datadog-api-client/-/datadog-api-client-1.64.0.tgz).
[^p-mcp-downtime]: Selected MCP publisher, pinned [downtime handler](https://github.com/winor30/mcp-server-datadog/blob/657bc09cf388c3848a89caf7b37dbfda810be805/src/tools/downtimes/tool.ts#L36), lines 36–115; especially scope wrapper 63–70, dropped recurrence `until` 74–79, and cancellation 96–112.
[^p-mcp-schema]: Selected MCP publisher, pinned [downtime schema](https://github.com/winor30/mcp-server-datadog/blob/657bc09cf388c3848a89caf7b37dbfda810be805/src/tools/downtimes/schema.ts); corroborated against published `build/index.js:748` and snapshot input definitions. Host schema corroborated against published `build/index.js:529`.
[^p-mcp-hosts]: Selected MCP publisher, pinned [host handlers](https://github.com/winor30/mcp-server-datadog/blob/657bc09cf388c3848a89caf7b37dbfda810be805/src/tools/hosts/tool.ts#L55), mute 63–91, unmute 98–120, totals 126–150, host normalization 157–207.
[^p-mcp-logs]: Selected MCP publisher, pinned [log handlers](https://github.com/winor30/mcp-server-datadog/blob/657bc09cf388c3848a89caf7b37dbfda810be805/src/tools/logs/tool.ts#L49), lines 49–97, seconds-to-millisecond strings and returned data only.
[^p-mcp-monitors]: Selected MCP publisher, pinned [monitor handlers](https://github.com/winor30/mcp-server-datadog/blob/657bc09cf388c3848a89caf7b37dbfda810be805/src/tools/monitors/tool.ts#L21), lines 21–111.
[^p-mcp-index]: Selected MCP publisher, pinned [entrypoint](https://github.com/winor30/mcp-server-datadog/blob/657bc09cf388c3848a89caf7b37dbfda810be805/src/index.ts#L53), registration 53–90 and exception rethrow 96–115.
[^p-mcp-config]: Selected MCP publisher, pinned [Datadog configuration](https://github.com/winor30/mcp-server-datadog/blob/657bc09cf388c3848a89caf7b37dbfda810be805/src/utils/datadog.ts); corroborated against published `build/index.js:719`, lines 719–745, and entrypoint environment mapping.
[^p-sdk-downtime]: Datadog, pinned [v1 DowntimesApi](https://github.com/DataDog/datadog-api-client-typescript/blob/aff03a5e36c33c5e9216c8148b173cc55b9f96b5/packages/datadog-api-client-v1/apis/DowntimesApi.ts#L24), request factories 24–245; cancellation response 343–391, create 463–514, list 583 onward; `currentOnly` documentation 790–795; v1 deprecations 841–954.
[^p-sdk-model]: Datadog, pinned [v1 Downtime model](https://github.com/DataDog/datadog-api-client-typescript/blob/aff03a5e36c33c5e9216c8148b173cc55b9f96b5/packages/datadog-api-client-v1/models/Downtime.ts#L19), field semantics 19–114, SDK/wire name mapping 131 onward.
[^p-sdk-recurrence]: Datadog, pinned [v1 DowntimeRecurrence](https://github.com/DataDog/datadog-api-client-typescript/blob/aff03a5e36c33c5e9216c8148b173cc55b9f96b5/packages/datadog-api-client-v1/models/DowntimeRecurrence.ts#L12), supported fields 12–45 and maps 62–89.
[^p-sdk-hosts]: Datadog, pinned [v1 HostsApi](https://github.com/DataDog/datadog-api-client-typescript/blob/aff03a5e36c33c5e9216c8148b173cc55b9f96b5/packages/datadog-api-client-v1/apis/HostsApi.ts#L66), list request 66–159, mute request 162–215, mute errors 386–436, list parameters 510–550, and **host mute creates Downtime V2** 646–650.
[^p-sdk-host-settings]: Datadog, pinned [HostMuteSettings](https://github.com/DataDog/datadog-api-client-typescript/blob/aff03a5e36c33c5e9216c8148b173cc55b9f96b5/packages/datadog-api-client-v1/models/HostMuteSettings.ts#L12), lines 12–24; [HostMuteResponse](https://github.com/DataDog/datadog-api-client-typescript/blob/aff03a5e36c33c5e9216c8148b173cc55b9f96b5/packages/datadog-api-client-v1/models/HostMuteResponse.ts#L12), lines 12–28; [Host](https://github.com/DataDog/datadog-api-client-typescript/blob/aff03a5e36c33c5e9216c8148b173cc55b9f96b5/packages/datadog-api-client-v1/models/Host.ts#L34), mute/expiry/tags and maps.
[^p-sdk-monitors]: Datadog, pinned [v1 MonitorsApi](https://github.com/DataDog/datadog-api-client-typescript/blob/aff03a5e36c33c5e9216c8148b173cc55b9f96b5/packages/datadog-api-client-v1/apis/MonitorsApi.ts#L1290), lines 1290–1317 distinguish group state, scope tags, monitor tags, and with-downtimes.
[^p-sdk-logs]: Datadog, pinned [v2 LogsApi](https://github.com/DataDog/datadog-api-client-typescript/blob/aff03a5e36c33c5e9216c8148b173cc55b9f96b5/packages/datadog-api-client-v2/apis/LogsApi.ts#L80), list POST and response processor; [LogsQueryFilter](https://github.com/DataDog/datadog-api-client-typescript/blob/aff03a5e36c33c5e9216c8148b173cc55b9f96b5/packages/datadog-api-client-v2/models/LogsQueryFilter.ts#L13), time/string/storage mappings.
[^p-sdk-exception]: Datadog, pinned [ApiException](https://github.com/DataDog/datadog-api-client-typescript/blob/aff03a5e36c33c5e9216c8148b173cc55b9f96b5/packages/datadog-api-client-common/exception.ts#L10), lines 10–18.
[^p-sdk-config]: Datadog, pinned [configuration](https://github.com/DataDog/datadog-api-client-typescript/blob/aff03a5e36c33c5e9216c8148b173cc55b9f96b5/packages/datadog-api-client-common/configuration.ts#L220), `enableRetry` default false at 228 and HTTP library configuration 857–864.
[^p-sdk-auth]: Datadog, pinned [authentication](https://github.com/DataDog/datadog-api-client-typescript/blob/aff03a5e36c33c5e9216c8148b173cc55b9f96b5/packages/datadog-api-client-common/auth.ts#L59), API-key header at 60 and application-key header at 80.
[^p-mcp-protocol]: MCP SDK, exact [1.26.0 package](https://registry.npmjs.org/@modelcontextprotocol/sdk/-/sdk-1.26.0.tgz), `package/dist/esm/shared/protocol.js:354`, especially rejected-handler framing 385–409 and safe-integer error code selection 394. Archive and entry SHA-256 values above.
[^r-catalog]: `catalog/services.toml:220`, Datadog catalog entry through line 234.
[^r-lock]: `servers/npm/package-lock.json:295`, SDK 1.64.0; `servers/npm/package-lock.json:1304`, MCP; `servers/npm/package-lock.json:1334`, nested MCP SDK; `servers/npm/package-lock.json:1374`, Zod.
[^r-image]: `images/tools/Dockerfile:37`, npm lockfile copy/install recipe, and line 68 runtime copy.
[^r-snapshot]: `servers/snapshots/datadog.json:1`, selected server info, tools/input schemas; get_logs line 73, monitors 135, host tools 253–350, downtime tools 355–463, declared inventory hash line 592.
[^r-lib]: `crates/services/datadog/src/lib.rs:64`, exact routes, and line 105 key-presence-only authentication.
[^r-downtime]: `crates/services/datadog/src/downtimes.rs:18`, constant actor/live/active/list; line 63 timestamp parsing; line 118 creation through 208; line 210 cancellation through 229.
[^r-hosts]: `crates/services/datadog/src/hosts.rs:84`, effective expiry rendering; line 98 host list; line 180 mute; line 250 unmute.
[^r-monitors]: `crates/services/datadog/src/monitors.rs:20`, query-scope extraction; line 63 rendering/with-downtimes; line 124 list filtering.
[^r-search]: `crates/services/datadog/src/search/mod.rs:69`, log field/text extraction; line 138 search/time/page/meta; line 250 log request parsing.
[^r-query]: `crates/services/datadog/src/search/query.rs:280`, parser; line 293 case-insensitive glob; line 321 field/free-text matching.
[^r-support]: `crates/services/datadog/src/support.rs:11`, errors; line 74 cursor encoding; `crates/services/datadog/src/timeparse.rs:11`, local search time parser.
[^r-clock]: `crates/mock-core/src/clock.rs:6`, fixed versus legacy clock mode; line 102 logical clock; line 130 current time and transport stepping.
[^r-grade]: `crates/mock-core/src/grading.rs:357`, source/interface partition; line 695 required/forbidden matching; line 938 array/string contains; line 1033 body subset; line 1042 successful-response matching.
[^r-cancel]: `scenarios/ops/datadog/cancel-called-off-maintenance/instruction.md:1`; `scenarios/ops/datadog/cancel-called-off-maintenance/scenario.toml:1`, clock, state checks, required and forbidden routes, and example calls.
[^r-cancel-data]: `scenarios/ops/datadog/cancel-called-off-maintenance/dataset/datadog/downtimes.json:1`, six records; target begins line 74; `scenarios/ops/datadog/cancel-called-off-maintenance/dataset/datadog/monitors.json:1`, both payments monitors.
[^r-mute]: `scenarios/ops/datadog/mute-host-from-error-logs/instruction.md:1`; `scenarios/ops/datadog/mute-host-from-error-logs/scenario.toml:1`, state checks 14–48, required/forbidden host writes 50–63, example logs/time bounds 65–73.
[^r-mute-logs]: `scenarios/ops/datadog/mute-host-from-error-logs/dataset/datadog/logs.json:1`, all 29 events; first target line 33, service/staging decoys lines 132/194/222, last target line 901.
[^r-mute-hosts]: `scenarios/ops/datadog/mute-host-from-error-logs/dataset/datadog/hosts.json:1`, six hosts; target line 52, existing payments mute line 251.
[^r-mute-existing]: `scenarios/ops/datadog/mute-host-from-error-logs/dataset/datadog/host_mutes.json:1`, preexisting Marcus mute.
[^r-schedule]: `scenarios/ops/datadog/schedule-replica-downtime/instruction.md:1`; `scenarios/ops/datadog/schedule-replica-downtime/scenario.toml:1`, fixed-ID expectations and required/forbidden routes.
[^r-schedule-data]: `scenarios/ops/datadog/schedule-replica-downtime/dataset/datadog/monitors.json:1`, target, staging, connections, MySQL, and disk monitors; `scenarios/ops/datadog/schedule-replica-downtime/dataset/datadog/downtimes.json:1`, three previous schedules; `scenarios/ops/datadog/schedule-replica-downtime/dataset/datadog/hosts.json:1`, four Postgres hosts.
[^r-ledger]: `docs/overhaul-checklist.md:1`, governing full scope and current 27 legacy writes; line 178 acceptance table, especially causal/information evidence and independent evaluator validation. Read-only context, not qualification evidence produced by this note.
