# Grafana WRITE operation contract — 2026-11-07

## Scope, evidence labels, and limits

Research-only sidecar for `ops/grafana/ack-checkout-page-oncall`, `ops/grafana/annotate-checkout-deploy`, and `ops/grafana/silence-replica-maintenance`, with `code/multi/terraform-failure-grafana-annotate` as cross-service context. Only this file is authored. No implementation, tests, installations, containers, account/infrastructure operations, model inference, auxiliary models, or frozen Terraform34 capsule/image changes were performed.

Evidence labels used below:

- **[D] Documented:** first-party published API/product documentation, with product/version distinctions.
- **[P] Pinned source:** inspected first-party implementation at an exact revision. This establishes code behavior, not a live deployment's configuration.
- **[F] Authored fixture:** local instructions, datasets, snapshot, Rust simulator, or evaluator. These are benchmark facts, not vendor guarantees.
- **[I] Inference:** a proposed contract or static consequence of cited code; counterexamples were not executed.
- **[U] Uncertainty:** a live/runtime, authorization, asynchronous, or rendering fact not established by this read-only investigation.

The three single-service scenarios freeze `now = 2026-11-12T15:30:00Z`; the Terraform/Grafana scenario freezes `2026-10-28T15:30:00Z`. Their “today,” “tonight,” and “this afternoon” refer to those scenario clocks, not this research date, November 7, 2026. The absolute requested dates remain unchanged. The native silence tool also consults the real process clock: this is a distinct reproducibility concern, not permission to rewrite the prompts. **[F/P/I]**[^f-ack][^f-deploy][^f-silence][^f-multi][^p-silence]

## Principal verified caveats

1. **Do not import the archived OnCall service-account prohibition into current Cloud IRM.** The live Cloud documentation now explicitly permits service-account alert-group actions with the write permission and records the service account as actor. Archived OSS documentation/source rejects those actions. The local simulator instead attributes acknowledgment to its first admin; that is an authored shortcut, not either product's identity contract. A documentation-index result returned the older prohibition during this research; direct retrieval of the current first-party page contradicts it. **[D/P/F]**[^d-cloud-auth][^d-cloud-groups][^p-oss-groups][^d-oss-groups][^l-irm]
2. **Native results are not uniformly raw REST bodies.** Annotation create/read returns a JSON text block containing a capitalized `Payload` envelope; search returns `{dashboards,total,hasMore}`; acknowledgment returns `{alertGroupId,action,updated,state? ,stateWarning?}`; silence creation returns `{silenceID}`. `grafana_api_request` is genuinely reachable and reports HTTP errors inside a successful tool result's `status`, unlike dedicated handlers. **[P/F]**[^p-tools][^p-annotation][^p-openapi][^p-search][^p-oncall][^p-silence][^p-api][^l-snapshot]
3. **A stored annotation is not proof of visible latency/DB markers.** Dashboard association, panel scope, point versus region, query enablement, displayed time range, and supported panel type matter. The captured examples omit `panelId`, producing dashboard-wide scope; adding `panelId=1` on Checkout would put the marker on Request rate, not p99 latency panel 3, yet current assertions do not reject it. **[D/P/F/I]**[^d-annotation-ui][^p-ui-filter][^p-ui-datasource][^f-deploy-data][^f-deploy][^l-api]
4. **Silence matching must be evaluated as a predicate, not two present array elements.** The current assertions allow a contradictory additional matcher, a second broad silence, and edits to unchecked parts of existing silences. The native tool is fixed to the Grafana-managed Alertmanager, not the `am-prod` datasource. At scenario time the requested maintenance silence must be pending, not already suppressing notifications. **[D/P/F/I]**[^d-silences][^d-matchers][^p-silence][^f-silence][^f-silence-data][^l-alerting]
5. **Fixed generated IDs, raw timestamp spellings, and origin-shaped strings are not business success.** Current expectations hard-code annotation IDs 42/53 and a seed-derived silence UUID. Equivalent paths through the reachable generic API, semantic matcher alternatives, transport retries, and different real allocation histories need separate treatment. Native OnCall requests can have a different ordinary user-agent from Grafana requests. **[P/F/I]**[^f-deploy][^f-multi][^f-silence][^p-api][^p-transport][^p-amixr][^l-grade]

## Exact source and capture pins

| Layer | Inspected pin | Interpretation |
| --- | --- | --- |
| Native Grafana MCP | `github.com/grafana/mcp-grafana/v2/cmd/mcp-grafana@v2.1.1`; commit `244d1adde04194ecad15125cb17159a1db50626b`; module timestamp `2026-10-20T17:21:57Z` | Catalog and Go build recipe agree; no binary was launched or image inspected. **[P/F]** |
| Module checksum | `h1:6wEZslhyIe9pYmz7xKulst56PtHl+wMrLQmb8aC1Rx8=`; `go.mod` checksum `h1:0Ipmvq7KAmYpFt50JL99KdYcUnobxE2RxiStCq451uc=` | Public Go checksum entry; not a checksum of the compiled tools image. **[P]** |
| Native dependencies | `amixr-api-go-client v0.0.32` → `390dbc3886c934099336963a5af1107afba1e729`; `grafana-openapi-client-go v0.0.0-20260330113218-ee77c4f6f90e` → `ee77c4f6f90e56e7cb0a71586f275754c143d8a4`; Alertmanager `v0.34.1` → `73c6bfe7393929211294c1954f30d8ed78e4d0ad` | Exact versions selected by the MCP module. Alertmanager is a client/model dependency here, not evidence of the simulated or hosted server's actual binary. **[P]** |
| Core Grafana comparison | `v11.6.4` → commit `0567ef29beee0e82ea8d4da6cd4e3511d277324f` | Matches the simulator's advertised Grafana version; does not make the Rust implementation a full Grafana instance. **[P/F]** |
| Archived OnCall comparison | `grafana/oncall` inspected revision `af0fbd40558c9a63bcf438589894c440fc434a54` | Archived OSS source, not the current Cloud IRM server's implementation. Its README says maintenance began March 11, 2025 and archival March 24, 2026. **[P]** |
| Terraform context | `terraform-mcp-server v1.4.1` → `3781509a674482ca7e2821fab8159a25e3b29bdf`; `go-tfe v1.111.2` → `76ed5bb13de750863b0a99c3a2a8961fa5edfb46` | Only relevant read handlers/representations examined; no frozen capsule edits or runs. **[P/F]** |
| Captured Grafana surface | `server_info={"name":"mcp-grafana","version":"v2.1.1"}`, protocol `2025-06-18`, 79 tools | Static snapshot, not a fresh runtime `tools/list`. **[F]** |
| Snapshot declared inventory hash | `d4379b55347e9b2cc47a8f2444ae3a2d490ccbd7a353962e7f46b4488bb4d21d` | The snapshot's own `sha256` field; do not confuse it with the file-byte SHA-256 below. **[F]** |

Pin sources: catalog/build recipe, module metadata/checksum/manifests, tag resolution, and local snapshot. **[P/F]**[^l-catalog][^l-image][^p-module][^p-go-mod][^p-downstream][^p-core-pin][^p-oss-readme][^p-tfe][^l-snapshot]

Local worktree HEAD observed: `4ef643dac101fba8e67701f8c0ac7f9dd6e3e5fd`. The worktree already contains unrelated edits; HEAD alone does not identify the inspected working files. These SHA-256 values bind the actual local inputs read for this note. **[F]**

| Local input | SHA-256 |
| --- | --- |
| `servers/snapshots/grafana.json` | `3ec44088f847dbf85007401f3ae97d1028b4acba2b38210ec6e54916a94ce9b2` |
| `catalog/services.toml` | `b65aec21f983a384afd1fb5e67c2b4416537c6bb4951581595b0a92afdec4c5f` |
| `crates/services/grafana/src/lib.rs` | `1aace98f09f0623451e17802a9178b61aeadf00e2e920bb544ff024c65a3bb17` |
| `crates/services/grafana/src/api.rs` | `d16d7c3a384ba7391c1ba4b35e2a00ac340b27cf9b57b0c95799ab207f819818` |
| `crates/services/grafana/src/alerting.rs` | `40d0f41e5e9567c6876287ef017cb9d38fe9b10032153f0eaa09342d16a57a6e` |
| `crates/services/grafana/src/irm.rs` | `3844ec9418c8d938ab6cabf9c0d4f70ed6e9ac0fe7afcd2a7ef77407b680b38d` |
| Ack `instruction.md` / `scenario.toml` | `cd445f47f01d8e90fc1d3b3b09d8d2aa9a4cae2e76874987957e794120b7a384` / `b2f0b1465f0528d5291bdd6abb3eabfd67a2777e961c3bd700c9971de1586d9d` |
| Deploy `instruction.md` / `scenario.toml` | `a7c1583c262829af19113f7ee5d57e29cdf98270337ddb3582cff774488ce84e` / `5cc442ce501afa1234b26a23d9240936f437a7a7bd989af0288d02cb38171a97` |
| Silence `instruction.md` / `scenario.toml` | `890022e809e52fbcd969a7da227eaad9e8f5af75819a32edaaa4c9e89bcfdd36` / `d8cafdd0c976ddb73c35ca39716bfa2963bd66687adcab9f4fcac9eed8c7e12a` |
| Multi `instruction.md` / `scenario.toml` | `0c405b2b555ab05efd04647e4ccc10580acf2a79333ae5ea5f6a681be2f8fc4d` / `ec1a46279bac09f9dfeaf371d846c84ecfa72466d8e9f7e455858260587198c6` |

The three single-service datasets have byte-identical copies of the relevant `dashboards`, `annotations`, `datasources`, `orgs`, `user`, `alert_rules`, `silences`, and five `oncall_*` collections. References below use the scenario most relevant to each claim rather than treating copies as independent corroboration. **[F]**[^f-ack-data][^f-deploy-data][^f-silence-data]

## Native reachable surface and result contract

### Selection and scope

The catalog starts stdio with `--usage-stats=disabled`, sets `GRAFANA_URL={world}` and `GRAFANA_SERVICE_ACCOUNT_TOKEN=glsa_toolbench`, and supplies no `GRAFANA_ORG_ID`, OBO tokens, write-disable flag, or dynamic multi-org setting. The captured schema includes the dedicated writes and **`grafana_api_request`** with GET/POST/PUT/PATCH/DELETE. Upstream registration gates writes, but this selected capture has them enabled. Do not invent an `acknowledge_alert_group`, a separate create-silence tool name, or a dedicated current-schedule endpoint. **[P/F]**[^l-catalog][^l-snapshot][^p-register][^p-api]

Native OnCall selects the IRM plugin proxy only when both access and ID tokens exist. Under the selected API-key configuration it discovers `onCallApiUrl` from `GET /api/plugins/grafana-irm-app/settings`, then uses the public OnCall API beneath `/oncall/api/v1`. The local settings advertise `http://<request-host>/oncall`; only this public route is implemented for OnCall in the inspected Rust router. Cloud OBO proxy paths are upstream capability, not interchangeable local reachable routes under these credentials. **[P/F]**[^p-oncall][^p-proxy][^l-irm][^l-lib]

All local fixtures use organization 1, “Octo Corp”; the current Grafana user is service account ID 7, login `sa-1-mcp-assistant`. The Rust handler validates one synthetic token and does not partition collections by `X-Grafana-Org-Id` or enforce the advertised per-tool RBAC permissions. Production versus staging is therefore a dashboard/folder, label, workspace, and schedule identity distinction inside the fixture organization—not an organization switch. Neither a successful local call nor `_meta.grafana.com/requiredPermissions` attests to hosted authorization. **[F/P/I]**[^f-deploy-data][^l-lib][^l-api][^l-snapshot][^p-transport]

### Inputs, wire operations, and successful values

Successful values in this table are the JSON encoded into the native MCP text content, **not** a required `structuredContent` shape. The pinned converter marshals typed return values into one text block; ordinary handler errors become `isError:true` text. The snapshot supplies input schemas, not output schemas. **[P/F]**[^p-tools][^l-snapshot]

| Native tool | Relevant input / actual HTTP | Returned successful value and caveat |
| --- | --- | --- |
| `search_dashboards` | `query`, `folderUid`, `tag` array, `starred`, `limit`, `page`; GET `/api/search` with `type=dash-db` | `{dashboards:[{uid,title,url,folderUid?,folderTitle?,tags?,...}],total,hasMore}`. `total` is this page's count; `hasMore` means page size equals limit, not a proven global count. **[P]** |
| `list_alert_groups` | Optional `state`, `name`, `teamId`, `labels:["env:prod"]`, IDs, range, `page`; GET `/oncall/api/v1/alert_groups/` | Array of normalized groups. Public-path normalization retains ID/title/state/integration/count/times/permalinks, but drops fixture `team_id`, labels, route, actor IDs, and `last_alert` from list output. Filtering can still use labels/team server-side. **[P/F]** |
| `get_alert_group` | Required `alertGroupId`; GET group detail | Normalized group plus `last_alert` and arbitrary payload if available. Public-path output still does not expose every raw fixture field. **[P]** |
| `update_alert_group` | Required `alertGroupId`, `state`; allowed transitions `acknowledged`, `unacknowledged`, `resolved`, `unresolved` | POST distinct action (`acknowledge`, `unacknowledge`, `resolve`, `unresolve`), then GET detail. Result `{alertGroupId,action,updated:true,state? ,stateWarning?}`. “Unacknowledged”/“unresolved” are input transition names, not returned states. **[P]** |
| `list_oncall_teams` | Optional `page`; GET teams | Array of `{id,name,email?,avatar_url?}`; not the unrelated Grafana teams directory. **[P]** |
| `list_oncall_schedules` | `teamId` or `scheduleId`, optional `page`; GET schedules/list or detail | Array of `{id,name,teamId,timezone,shifts}`. It deliberately omits `on_call_now`. **[P]** |
| `get_current_oncall_users` | Required `scheduleId` | GET schedule detail, then GET each `on_call_now` user's detail. Result `{scheduleId,scheduleName,users:[{id,username,email?,role?}]}`. Public-path conversion does not populate the fixture user's full `name`. Individual failed user lookups are silently skipped. **[P]** |
| `create_annotation` | `dashboardUid`, optional `panelId`, integer `time`/`timeEnd` in milliseconds, `text`, `tags`, optional `data` | Standard POST `/api/annotations` maps **tool `dashboardUid` → REST `dashboardUID`**. Result `{"Payload":{"id":N,"message":"Annotation added"}}`, not top-level `id`. Text is checked by the handler despite no schema-level required list for this tool. **[P]** |
| `get_annotations` | `dashboardUid`, `from`/`to` milliseconds, `panelId`, tags, `matchAny`, etc. | GET `/api/annotations`; result `{"Payload":[...annotation models...]}`. This is not the raw array, and tags default to AND, not OR. **[P/D]** |
| `update_annotation` | Required `id`; optional text/time/timeEnd/tags/data | PATCH existing annotation; no dedicated dashboard/panel reassignment input. Updating saved `dashboard.annotations.list` through `update_dashboard` is a different operation. **[P]** |
| `alerting_silences_read` | `operation=list|get`, optional `silence_id`, list-only matcher/rule filters | GET `/api/alertmanager/grafana/api/v2/silences` or `/silence/<id>`; returns array or individual silence, including `status.state`. **[P]** |
| `alerting_silences_write` | `operation=create|update|delete` (also advertises/accepts list/get); `matchers`, `starts_at`, `ends_at`, `comment`, optional `created_by` | Create/update POST `/api/alertmanager/grafana/api/v2/silences`; result `{"silenceID":"..."}`. Delete uses singular `/silence/<id>` and returns `{status:"deleted",silence_id}`. No datasource selector. **[P]** |
| `grafana_api_request` | Required `endpoint` relative to Grafana base; optional `method`, JSON **string** `body`, headers, `jq` | Returns `{status,headers:{"Content-Type":...},data}`. HTTP 4xx/5xx does **not** itself set MCP `isError`; network/read/jq failures can. A JSON parse failure yields response text in `data`. **[P]** |

Sources for table: inspected factories/handlers, normalized OnCall types, generated OpenAPI response structs and field tags, captured schemas, and annotation API documentation. **[P/F/D]**[^p-search][^p-oncall][^p-oncall-types][^p-annotation][^p-openapi][^p-silence][^p-api][^l-snapshot][^d-annotations]

**Equivalent API access is real, not a hypothetical extra tool.** The generic tool can perform the same authorized HTTP operation and inspect raw fields omitted by dedicated normalizers. It can also choose a trailing slash or a numeric dashboard ID. These are native-expressible plans, although some expose local fidelity/evaluator gaps described below. Tools not present in this snapshot and direct privileged world/admin access are not alternative plans. **[P/F/I]**[^p-api][^l-snapshot][^l-lib][^l-grade]

## 1. Acknowledge production checkout and identify current primary on-call

### Identity and required effects

| Object | Fixture identity / intended treatment |
| --- | --- |
| Current production page | `IHQ4K2M7`, `[prod] HighCheckoutLatency checkout p99 above 1.5s`, integration `CIGRAFAN`, created `2026-11-12T15:09:00Z`, initially `new`; acknowledge only this group. |
| Staging page | `IST7B3N1`, same latency subject but staging, initially `new`; preserve. |
| Historical production page | `IOLD9X1Z`, same production title, already `resolved`, created November 8; preserve. |
| Unrelated DB page | `IPG2L8R4`, Postgres replica lag, already `acknowledged`; preserve. |
| Team and primary schedule | Checkout `TCK4M2P8` → `SCK1PRIM` (“Checkout Primary”, UTC). `SCK2SECN` is secondary; `SSR1PRIM` belongs to SRE. |
| Current assignee | Primary `on_call_now=["UZ9D6K3L"]` → `sbrennan`, `s.brennan@octo-corp.com`; fixture full name Sam Brennan. Secondary is `mchen`, SRE primary `pokafor`. |

These are real identifiers in the authored dataset, not verified customer resources. The required answer destination is the unchanged `/app/answer.md`. Current expected-answer regex only searches for `brennan`. **[F]**[^f-ack][^f-ack-data]

**Operation contract [I]:** correlate production/fresh-page identity, perform acknowledgment, preserve unresolved/resolution fields appropriately, preserve staging/history/unrelated groups, and answer the person presently associated with Checkout Primary. Do not treat acknowledgment as resolution, an Alertmanager silence, an alert-rule pause, or an instruction to contact/page the person. Those are different effects and not requested. **[I/D/P/F]**[^f-ack][^p-oncall][^d-cloud-groups][^d-silences]

The local action changes `state`, `acknowledged_at`, and `acknowledged_by`, returning HTTP 200 JSON null. Its timestamp comes from the frozen world clock; its actor comes from the first admin in `oncall_users`, not the authenticated Grafana service account or the current on-call user. It rejects already acknowledged/resolved groups with 400. The native tool ignores the action body and reads detail before returning its result. **[F/P]**[^l-irm][^p-oncall]

### Product semantics and authentication distinction

- **Current Cloud [D]:** the live Cloud IRM infra-as-code page says service-account tokens can acknowledge, resolve, and silence groups with `grafana-irm-app.alert-groups:write`; IRM records the service account as actor. The alert-group API page documents Bearer authorization and `X-Grafana-URL` stack context. Do not claim user-only authentication is a universal current Cloud requirement. **[D]**[^d-cloud-auth][^d-cloud-groups]
- **Archived OSS [D/P]:** the older public API docs specify user authentication for actions. The archived handler rejects service accounts, already acknowledged/resolved groups, attached groups, and maintenance groups for acknowledgment. Its model records acknowledgment, stops escalation, creates log records, schedules asynchronous signals/reminders, and can propagate to dependent groups. That is useful comparative semantics, not proof current Cloud has the identical implementation or restrictions. **[D/P/U]**[^d-oss-groups][^p-oss-groups][^p-oss-model]
- **Local qualification gap [F/U]:** no real Cloud/OSS authentication was exercised. The catalog's API-key fallback sends the library's raw token header, whereas current Cloud documentation illustrates Bearer auth. The mock accepts both forms and invents an admin actor. It therefore proves neither hosted token compatibility nor faithful actor attribution. It also does not implement escalation queues, reminders, source backsync, or dependent-group propagation. **[P/F/U]**[^p-amixr][^p-oncall][^l-lib][^l-irm][^d-cloud-groups]

### Valid plans, partial success, and observability

Listing new groups, filtering by production label/name/team, or inspecting a known candidate by `get_alert_group` are all native discovery options. Teams → primary schedule → current users is the example, not a compulsory sequence of all those tools. Generic GET of the schedule and user details also supplies the relationship. Rotation configuration alone is weaker current-duty evidence because schedule overrides can apply, and this simulator returns a stored `on_call_now` array without calculating a rotation at the current clock. **[P/D/F/I]**[^p-oncall][^p-api][^d-schedules][^f-ack-data][^l-irm]

An acknowledgment write can succeed while its following GET fails. The pinned tool then returns `updated:true`, the action, and **`stateWarning` instead of a tool error**; `state` is absent. This prevents inviting a blind repeat of a committed write. Conversely, `updated:true` means the action request succeeded, not that the requested state is immutable or that all downstream notifications have stopped. If detail is unavailable, retain the committed-write/unknown-readback distinction. **[P/I/U]**[^p-oncall][^p-oss-model]

OnCall public list calls consume one requested page and normalize away pagination metadata; the caller cannot assume an empty first page is exhaustive in larger fixtures. Failed per-user detail lookups in `get_current_oncall_users` are skipped, so `users:[]` is not proof nobody is on duty when the schedule contains IDs. The public normalizer also omits the user's full name: requiring exactly “Sam Brennan,” rather than the observed username/email identity, would be an unnecessary false failure. **[P/F/I]**[^p-oncall][^p-oncall-types][^f-ack-data]

## 2. Annotate checkout deployment

### Exact target and event relationships

The production dashboard is `checkout-prod`, numeric ID **101**, title “Checkout Service”, folder `f-prod` (“Production”), tags `checkout,prod,slo`. Staging is `checkout-staging` / 102 / `f-staging`; legacy is `checkout-legacy` / 87 / `f-archive`. The desired point event is:

```json
{
  "dashboardUID": "checkout-prod",
  "time": 1794492300000,
  "text": "Deploy checkout v4.19.0",
  "tags": ["deploy", "checkout"]
}
```

`1794492300000` is **November 12, 2026, 14:05:00 UTC**, in milliseconds. Omitting `timeEnd` yields a point whose stored end equals its start. Dashboard-wide scope omits `panelId`/uses 0; p99 latency is panel **3**, whereas panel 1 is Request rate, panel 2 error ratio, and panel 4 memory. A panel-specific plan is not automatically equivalent to a dashboard-wide event. **[F/D/P]**[^f-deploy][^f-deploy-data][^d-annotations][^p-core-store][^p-ui-filter]

Existing annotations 38–41 must be preserved: production's earlier `v4.18.2` deployment (38), staging's `v4.19.0` event (39), an organization-wide incident region (40), and an Orders DB panel event (41). The Rust allocator chooses one more than the largest annotation `id`, making the first newly created event **42 in this fixture**. That is not a vendor allocation promise or an identifier agents should supply. **[F/P]**[^f-deploy-data][^l-lib][^l-api][^p-core-store]

**Operation contract [I]:** exactly one newly added matching event, on the production dashboard, with the exact requested text, instant, and required tags, no region beyond the instant, and no saved-dashboard changes or collateral annotation changes. Tag order is not identity. The instruction does not require setting a particular creator, using a particular generated ID, or modifying an annotation query. **[I/F/D]**[^f-deploy][^d-annotations]

### Visibility is a separate relationship

The fixture production dashboard already has an enabled built-in annotation query using `-- Grafana --`, `type:"dashboard"`, with `hide:true`. Core Grafana's datasource queries the current dashboard UID and removes tag filters for a dashboard-type query. Core panel filtering restricts a nonzero `panelId` to that panel; an event with panel 0 is not so restricted. The graphs query `prom-prod`, but annotation events live in Grafana's annotation store, not in Prometheus. Matching event tags do not substitute for attaching the correct dashboard. **[F/P/D]**[^f-deploy-data][^p-ui-datasource][^p-ui-filter][^d-annotations]

An enabled query, supported timeseries panel, and in-range event make the example a plausible visible marker without editing the dashboard. `hide:true` must not be simplistically interpreted as `enable:false`; changing it is not authorized. This investigation did not render the UI, so it does not prove pixels, browser refresh timing, panel annotation toggles, or hosted display permissions. The fixture's `now-6h` range includes this event at scenario time. **[F/P/I/U]**[^f-deploy-data][^d-annotation-ui][^p-ui-datasource][^p-ui-filter]

### Native alternatives and non-equivalents

- `create_annotation` standard format is the direct native path. `grafana_api_request` POSTing the equivalent JSON body is also available and does not become a bypass merely because a dedicated tool exists. Searching by folder/tags or inspecting a dashboard by UID can replace the example's exact search query/order. **[P/F/I]**[^p-annotation][^p-api][^p-search][^l-snapshot]
- A raw API body with `dashboardId:101` is a documented Grafana association alternative. Core GET resolves its dashboard UID from the stored numeric ID. **The Rust create handler instead leaves `dashboardUID` empty unless provided**, so this otherwise valid product plan fails its local UID expectation/read filtering. It is both a representation-fidelity issue and a potential false failure, not evidence that the native generic tool lacks this input. **[D/P/F/I]**[^d-annotations][^p-core-api][^p-api][^l-api][^f-deploy]
- `format:"graphite"` is not equivalent here. The pinned handler divides its millisecond `when` input by 1000 for Graphite seconds and sends only Graphite text/tags/data; it does **not** forward dashboard or panel association. The result is an organization event, not the required dashboard event. Appending to saved `annotations.list` is a dashboard-definition change, not an event write. **[P/F]**[^p-annotation][^p-openapi][^l-snapshot][^f-deploy]
- A create acknowledgment contains ID/message, not a round-trip echo of all fields or a uniqueness guarantee. Read back by dashboard/time/tags and compare ID, text, point scope, and neighbors if completion evidence is required. This is a verification recommendation, not a new mandatory tool order. **[P/D/I]**[^p-annotation][^p-openapi][^d-annotations]

## 3. Silence replica maintenance

### Scope, time, and matcher meaning

The requested business predicate is:

```text
alertname == "PostgresReplicationLag"
AND instance == "pg-replica-2"
```

The requested UTC window is **November 13, 2026, 02:00:00–04:00:00**, duration 7,200 seconds; epoch milliseconds are `1794535200000` and `1794542400000`. Comment must identify **CHG-7781**. At the frozen November 12, 15:30 world time the silence is **pending**. Do not require immediate suppression as proof of a future silence. **[F/D/P/I]**[^f-silence][^p-silence][^l-alerting]

The native payload uses `starts_at`/`ends_at` strings, converted to REST `startsAt`/`endsAt`; matcher `isRegex` defaults false and omitted `isEqual` means equality. The two ordinary matchers are conjunctive, not an OR between replicas. Alertmanager regex matchers are full-string anchored; an exact regex such as `^pg-replica-2$` can denote the same replica, while `pg-replica-.*` is broader. Negated matchers and missing labels require actual predicate evaluation, not counting label names. The dedicated schema exposes one flat matcher list, not newer Alertmanager matcher-set extensions. **[P/D]**[^p-silence][^p-am-labels][^d-matchers]

The relevant Grafana rule is `ae3pglag`, title `PostgresReplicationLag`, folder `f-platform`, group `databases`, using `prom-prod`. Its replica-2 instance is `Alerting`; its replica-1 instance is **Normal**, not firing. “pg-replica-1 must keep alerting” therefore means preserve its ability to notify if it fires, not require this fixture's healthy replica to become firing now. A negative probe must use its label set or an admitted counterfactual, because the local Alertmanager alert list omits Normal instances. No live alert evaluation was performed. **[F/I]**[^f-silence-data][^l-alerting]

Silences suppress **notifications**, not evaluation or the underlying condition. The local derived read effect is `status.silencedBy` plus `status.state:"suppressed"` for matching firing instances **only while active**; the rule and its instance's `Alerting` condition remain intact. The future maintenance request does not authorize pausing/deleting the rule, changing labels/routes/contact points, acknowledging the unrelated OnCall lag group, or clearing the lag condition. **[D/F/I]**[^d-silences][^l-alerting][^f-silence]

### Organization / Alertmanager / datasource are different axes

The native silence implementation hard-codes `/api/alertmanager/grafana/api/v2`, the built-in Grafana Alertmanager within the selected Grafana context. `am-prod` is a distinct, read-only external Alertmanager datasource whose fixture configuration has `handleGrafanaManagedAlerts:false`; a rule's `prom-prod` query datasource is not its silence destination. The existing rule is Grafana-managed. This evidence supports the built-in destination for this fixture, not the claim that all production alerts everywhere share one Alertmanager. **[D/P/F/I]**[^p-silence][^d-silences][^f-silence-data][^l-alerting]

`rule_uid` in silence-tool input is only a **list filter**. On create/update, the pinned handler rejects it; it does not silently add a rule matcher. Rule-scoped silences use an explicit `__alert_rule_uid__` matcher. A further matcher can narrow the original requested predicate, so adding rule/team/folder labels should be accepted only if that narrower notification scope is actually intended/equivalent—not merely because today's fixture has one matching rule. **[P/D/I]**[^p-silence][^d-silences][^f-silence]

The sample discovery call uses `alerting_rules_read` with `search_rule_name`. The captured schema and pinned client mark this server-side filter as requiring Grafana **12.4+**, although the Rust simulator advertises **11.6.4** and implements it. This is an authored compatibility extension, not evidence that an ordinary 11.6.4 server supports the filter. Unfiltered rule discovery followed by identity inspection is a legitimate alternative; a successful silence does not require the example's exact filtered read. Hosted behavior with unsupported query parameters was not exercised. **[P/F/I/U]**[^l-snapshot][^p-silence][^l-lib][^l-alerting][^f-silence]

### Existing objects and derived effects

| Existing silence | Preserve the entire object, not just the presently checked field |
| --- | --- |
| `3c1f2b7e-9a44-4c1e-8d0b-6f1a2e3b4c5d` | NodeDiskFillingUp on `ip-10-0-3-52`, expired November 10; comment `Volume resize CHG-7702`. |
| `8d2e4f6a-1b3c-4d5e-9f0a-7b8c9d0e1f2a` | Staging checkout error-rate silence, active from November 12, 11:30 through November 13, 11:30. |

The expected new UUID `eb498c8c-12e2-8b09-a878-b3d26d162c7f` is only this scenario seed's first generated silence ID. The current TOML checks two matcher-subset entries, exact time strings, comment substring, and one field on each old silence; it does not assert the entire predicate, every existing field, or the absence of additional silences. **[F]**[^f-silence][^f-silence-data][^l-alerting]

**Operation contract [I]:** add the requested new silence with a returned identity, a correctly normalized two-hour interval and comment, ensure its effective label predicate includes the target and excludes replica 1, preserve preexisting silence/rule/routing state, and create no additional unauthorized muting. During a qualifying active-window observation, suppression membership should derive from that same silence ID. No evidence here supports a universal “exactly one HTTP POST attempt” rule. **[I/D/P/F]**[^f-silence][^p-silence][^d-silences][^l-alerting]

### Time, updates, validation, and retries

- **Native clock [P/F/U]:** `validateWritePayload` calls `time.Now()` and rejects an already closed interval before HTTP. The Rust backend compares against the frozen world clock. No process-clock override was found in the inspected image/server/deploy/catalog setup. Consequently an unchanged valid scenario can fail natively after November 13, 2026, 04:00 UTC even when its world is still November 12. A `grafana_api_request` POST avoids that helper's clock check but is not itself a remedy for the inconsistent fixture/native clock contract. No such request was executed. **[P/F/I/U]**[^p-silence][^p-api][^l-clock-setup][^l-alerting]
- **Validation [P/F]:** the dedicated helper requires nonempty matchers/names, RFC3339 timestamps, end after start, nonexpired end, and comment. `created_by` defaults to `grafana-assistant`; `sre` is only the example's choice, not a requested authenticated actor. Create rejects a supplied `silence_id` to avoid accidental update. The backend is weaker: it does not validate regex syntax or require nonempty comment/creator; Alertmanager source rejects invalid regex and all-empty-matching matcher sets. Generic API reachability makes this difference relevant. **[P/F]**[^p-silence][^l-alerting][^p-am-silence]
- **Clamping and representation [P/F/I]:** the local backend clamps a start in the past to its current time and normalizes RFC3339 output, so UTC-offset and fractional-second spellings need instant-based comparison. The native client serializes parsed times rather than preserving argument bytes. Here the requested start is future, so clamping is not a license to accept a shortened maintenance window. **[P/F/I]**[^p-silence][^l-alerting]
- **Updates are not universally same-ID [P/F]:** upstream Alertmanager can expire a prior silence and allocate a new ID when updating changes historical meaning; its `canUpdate` rules distinguish active/pending/expired and matcher sets. The Rust backend overwrites a supplied existing ID unconditionally after basic checks and clamps its start. Reusing the two existing silences is forbidden by the prompt regardless of ID stability. **[P/F]**[^p-am-silence][^l-alerting][^f-silence]
- **Expiration is not erasure [D/P/F]:** the delete/unsilence operation expires a silence, with ended records retained in the product; local DELETE adjusts times and keeps the record. It cannot recover already missed notifications, and the task forbids DELETE in any case. The imported Alertmanager version uses a strict `After(endsAt)` state boundary while the simulator uses `end <= now` for expired; qualify exact endpoint-boundary probes instead of assuming bit-identical server timing. **[D/P/F/I]**[^d-silences][^p-am-silence][^l-alerting][^f-silence]

## 4. Terraform failure → Payments DB annotation context

The read-only source chain is:

```text
acme-platform / Payments project
  → payments-api-prod / ws-vJd7W10B1cG75Uxp
  → run-Gy4tBHGggKx49nbH / "Upgrade payments DB to Postgres 16" / errored
  → apply-OttY2qKt1DNXCd1P / errored / actual failure log
  → errored-at 2026-10-28T13:12:09Z + AWS InvalidParameterCombination
  → Grafana payments-db-prod / id 120 / Production
  → one point event at 1793193129000 ms, tags terraform + payments
```

The log describes `aws_db_instance.payments` / `payments-prod-db`: RDS `ModifyDBInstance` rejected the 14.12 → 16.4 major upgrade because `AllowMajorVersionUpgrade` was not true. That is **fixture log content**, not a new diagnosis from a real AWS API call. Older production `run-nAAGvVVuIe2rfmNM` is an unrelated failed plan; staging `run-YF4vEQXaSoBg49H3` failed with `InvalidDBParameterGroupState`. Staging dashboard is `payments-db-staging` / 121; legacy is `payments-db-legacy` / 122. Preserve them and all Terraform state. **[F]**[^f-multi][^f-multi-data]

The pinned Terraform `get_run_details` returns JSON:API text with status timestamps and relationships; its SDK has `ErroredAt`. `get_apply_logs` reads apply detail first, rejects an unreachable apply, reports nonterminal status without logs, and fetches logs for terminal states. Public HCP Terraform exposes applies with a `log-read-url`. The native tools are therefore sufficient to derive the exact failure instant and error code without a Terraform write. Do not infer terminal failure time solely from the first log line, a plan timestamp, creation time, or when the annotation is written. **[P/D/I]**[^p-tfe][^d-tfe-applies][^l-tfe]

**Fixture weakness [F/I]:** the selected run's `created-at`, planned time, and `errored-at` are all `2026-10-28T13:12:09Z`; the apply's queued/started/errored times also coincide. A solver using the wrong timestamp field can consequently produce the right answer. The existing event text is checked only for run-ID/error-code substrings, not a relationship to the read source. Strengthen future evidence binding/fixtures rather than claiming the present example distinguishes those derivations. **[F/I]**[^f-multi-data][^f-multi]

Production Payments DB contains a single **Replication lag** timeseries panel 1, not a seeded connection-error panel. Its enabled built-in dashboard annotation query and six-hour time range support placing the event, but the request's desired correlation with connection errors is not proof those errors are observable in this dataset. No additional panel, metric query, or Terraform correction is required or authorized. **[F/I/U]**[^f-multi-data][^f-multi][^p-ui-datasource]

The first new local annotation is 53 only because existing IDs are 50–52. Apply the same point-scope, dashboard-association, result-envelope, exactly-one-new-event, collateral preservation, and readback caveats as Checkout. The example sentence is not compulsory: text must include the correct run ID and AWS code, with `terraform` and `payments` tags. Terraform POST/PATCH/DELETE are forbidden; a future contract should also express complete read-only preservation rather than rely only on that verb list and the selected run's final `errored` state. **[F/I]**[^f-multi][^f-multi-data][^p-annotation][^l-grade]

## Error propagation, no-ops, and origin/visibility evidence

| Situation | What is established | Evaluator implication |
| --- | --- | --- |
| Dedicated handler receives HTTP rejection | Annotation/OpenAPI and OnCall errors are wrapped; silence helper rejects non-2xx including body/status. Converter emits `isError:true`. **[P]** | A tools/call transport success is not business success. Preserve the actual tool result, HTTP operation, and state. |
| Generic API receives HTTP 400/401/403/500 | `grafana_api_request` returns a regular `{status,data}` result without automatically setting `isError`. **[P]** | Parsing `isError` alone creates false success; inspect `status` and returned semantics. |
| Acknowledge succeeds; detail read fails | `updated:true` with `stateWarning`, no `state`. **[P]** | Distinguish committed mutation from uncertain resulting observation; do not demand a second acknowledgment. |
| Mutation commits; response is lost, unreadable, oversized, or decoding fails | A transport/tool error may follow a committed mutation. Silence helper even accepts an empty 2xx body without proving nonempty `silenceID`. **[P/I]** | Mark ambiguity and seek read evidence; neither assume rollback nor blind retry. |
| Repeated acknowledgment | Local/archived OSS reject already acknowledged groups; native tool is marked nonidempotent. **[F/P]** | It is not an idempotent “ensure state” API. Already-correct state in a recovery observation and a second failed request need attempt/result correlation. |
| Repeated annotation/silence create | No idempotency key is supplied by these dedicated tools; each successful local create allocates another record. **[P/F]** | Count logical creations/effects, not merely at-least-one accepted POST. Do not promise safe retries. |
| Asynchronous external effects | Archived OnCall schedules signals/reminders; real notification delivery/display is not simulated. Silence acceptance schedules a future mute, not a retroactive fix. **[P/F/U]** | Do not require or assert synchronous Slack delivery, stopped phone calls, database recovery, or UI pixels from these responses. |
| Destructive undo | Annotation delete is permanent in the native tool description; silence expire is retained history; OnCall inverse actions create further transitions. **[P/D]** | Reverting final fields does not erase unauthorized intermediate action or external consequences. |

Sources: converter, dedicated handlers, generic handler, OpenAPI response models, local creation/action code, and product silence behavior. **[P/F/D]**[^p-tools][^p-annotation][^p-openapi][^p-oncall][^p-api][^p-silence][^p-amixr][^p-oss-model][^l-api][^l-alerting][^l-irm][^d-silences]

### Ordinary-origin/client markers

Grafana's pinned `BuildTransport` normally adds `User-Agent: mcp-grafana/<version>`, optional organization context, and configured auth. The OnCall fallback is different: pinned amixr has a **private** retryable HTTP client, sets `User-Agent: amixr-api-go-client`, raw `Authorization`, and `X-Grafana-URL`. The MCP reflection hook searches exported `HTTPClient`/`HttpClient`/`Client` fields; none exists on this pinned client. With no configured SOCKS proxy it continues without replacing that transport. Consequently a universal requirement for the Grafana user-agent or bearer spelling would reject ordinary first-party OnCall traffic. **[P/F/I]**[^p-transport][^p-oncall][^p-amixr][^l-catalog]

Amixr's private client uses retryablehttp (its source describes up to four retries on recoverable 429/5xx/network failures), including requests created for mutations. One logical native call can thus produce multiple HTTP attempts. The surrounding Grafana transport has no universal retry policy establishing exactly-once behavior for these operations. A successful final read does not reconstruct which uncertain attempt committed. **[P/I]**[^p-amixr][^p-oncall][^p-transport]

The generic API tool permits request headers, and the transport leaves an already-set user-agent intact. Client markers are useful diagnostics, **not authenticated invocation provenance**. The local grader partitions requests by trusted tools IPs, not user-agent; it correlates required operations with accepted statuses and filters startup traffic. This proves a bounded HTTP origin classification, not that a given dedicated native tool was used, nor that its result was understood. The catalog's synthetic replay authorization header is replay configuration, not independent evidence of a live caller. **[P/F/I]**[^p-api][^p-transport][^l-grade][^l-catalog]

Observation normalization is also material: OnCall list output omits raw labels/team/actor fields; current-duty output can silently omit failed user lookups; local annotation reads hard-code empty `data` and do not implement every advertised filter; local silence-list filtering is not a complete Alertmanager regex/negation implementation. Grade by observations the admitted interface actually returns, and keep backend-only preservation witnesses distinct from invented native output fields. **[P/F/I]**[^p-oncall][^p-oncall-types][^l-api][^l-alerting][^l-snapshot]

## Actionable evaluator objections and counterexamples

The following are **static, source-supported objections [I]**, not executed benchmark passes/failures. “Current checks” means the inspected ordinary scenario TOML and current matching implementation, not a fresh claim about an unknown deployed/frozen image. Required calls currently need an accepted response, so a rejected POST alone is **not** sufficient to pass. **[F/I]**[^l-grade][^f-ack][^f-deploy][^f-silence][^f-multi]

| Counterexample / objection | Why the current ordinary checks are insufficient or too narrow | Bounded follow-on decision |
| --- | --- | --- |
| **Ack: canned identity answer without schedule evidence.** Acknowledge correct group; write any sentence containing `brennan`, even with the wrong schedule or a negated claim. | Answer regex is unbound; no required schedule/user relationship is checked. | Bind answer identity to current primary schedule/user observations, accepting username/email where full name is not returned. |
| **Ack: collateral Grafana mutation.** Correct acknowledgment plus unrelated annotation or rule edit. | The ack TOML protects selected group POST paths and resolve/silence actions, not a general allowed mutation scope. | Permit only the authorized target action; preserve unrelated logical state and attempted safety boundaries. |
| **Ack: resolve then unresolve using trailing-slash paths.** After the valid acknowledgment, generic POST target `/resolve/`, then `/unresolve/`. | Router trims trailing slashes, but the forbidden regex ends with `/(resolve|silence)$`; the target's final state can again be acknowledged and the required original POST still exists. | Normalize semantic action paths for authorization while preserving original wire evidence; catch intermediate resolution, not just final state. |
| **Ack: correct write with failed readback.** | A new evaluator demanding `state:"acknowledged"` in every successful tool result would reject the pinned `stateWarning` partial-success result. | Record committed action and qualify the unavailable readback separately; do not claim unobserved state or retry automatically. |
| **Deploy: wrong text / region / wrong panel.** Create at correct time with text merely containing `4.19.0`, a later `timeEnd`, or `panelId=1`/a nonexistent panel. | TOML uses text substring, omits end-time/panel checks; the Rust handler does not validate panel existence. These can satisfy event pointers without the requested marker. | Require exact deploy text, a point interval, and the intended dashboard-wide/latency-visible scope. |
| **Annotations: edit existing staging/history events.** Add correct event and PATCH event 39's time/tags (keeping its checked text), or another old event. | No complete preservation check; forbidden DELETE/dashboard-save does not forbid annotation PATCH. | Preserve all prior events and saved dashboards, including intermediate unauthorized writes. |
| **Annotations: numeric dashboard-ID native alternative.** Generic POST uses correct `dashboardId`, not UID. | Official core accepts it; local create stores empty UID and the expected UID/body matcher rejects it. | Repair/qualify association normalization rather than label a product-valid API plan wrong. |
| **Annotations: generated-ID dependence.** Correct single event receives an ID other than 42/53 under different allocation history. | Exact pointers and “next ID absent” bind one local allocation, not event identity. | Resolve created ID from correlated response/state delta; count all new logical events. Keep isolated deterministic-ID checks labeled fixture-specific. |
| **Silence: contradictory third matcher.** Add both required equality matchers plus `instance="pg-replica-1"`. | Object/array subset checks still find the two required entries, but the conjunction matches no replica. | Evaluate the whole predicate, demonstrate target inclusion, and detect contradictory/narrower unintended conditions. |
| **Silence: additional broad silence.** Create the checked narrow silence first, then a second `alertname="PostgresReplicationLag"` silence. | No no-extra-silences assertion; replica 1 would be muted if it fired. | Inspect complete creation delta and every effective new predicate, not the seeded first UUID alone. |
| **Silence: modify existing staging silence.** Keep its checked `endsAt`, change matcher/comment/creator/start via update, plus create the correct new silence. | Only its end and the expired silence's comment are preserved. Local update keeps ID even where real Alertmanager may allocate replacement history. | Preserve both existing objects and histories, not selected fields or presumed ID stability. |
| **Silence: valid equivalent regex/order/time/creator.** Use exact anchored regex predicate, reordered matchers, equivalent UTC offset, default creator. | Literal matcher values/time strings or an invented `created_by="sre"` requirement can reject an equivalent operation. Current tag/matcher subset checks already tolerate some order changes. | Compare semantic instants and predicates; do not make the sample author string mandatory. |
| **Silence: expect immediate suppression / replica-1 firing.** | The requested silence is pending; replica 1 is Normal and absent from local firing list. | Separate scheduled effectiveness from active-window suppression; test non-target eligibility without inventing a current firing alert. |
| **Silence: host date advances beyond fixed interval.** | Dedicated helper rejects against `time.Now()` before reaching frozen world. | Declare/qualify native clock admission; classify clock incompatibility as setup/contract failure, not reasoning failure. Do not alter unchanged dates to hide it. |
| **Generic API HTTP failure with no MCP error flag.** | A 400 can be returned as normal `{status:400,data:...}` text; current generic oracle framing only rejects MCP error flags before service-specific interpretation. | Decode native result semantics as well as the HTTP evidence; do not confuse an accepted tool invocation with an accepted operation. |
| **Multi: use creation time or copy a stale error.** | Selected run times coincide; only annotation substrings/time and one run status are checked, not the source derivation. | Bind workspace → run → errored timestamp → apply log/error code → event; do not require the example sentence or unused discovery calls. |
| **Retry/temporary undo histories.** | At-least-one POST and selected final fields cannot prove one business effect, no unauthorized intermediate action, or an unambiguous result. | Correlate attempts/results/created identities and preserve history. Do not demand no repeated HTTP attempts when the selected client itself retries. |

Counterexample sources: unchanged scenario assertions, fixtures, native generic/dedicated implementations, local route normalization/action handlers, and evaluator subset/path/status logic. **[F/P/I]**[^f-ack][^f-deploy][^f-silence][^f-multi][^f-ack-data][^f-deploy-data][^f-silence-data][^f-multi-data][^l-grade][^l-lib][^l-api][^l-alerting][^l-irm][^p-api][^p-oncall][^p-silence][^p-core-api]

### Minimal contract decisions for follow-on implementation

1. **Acknowledgment:** one authorized production-group acknowledgment, no resolution or collateral writes, meaningful current-primary identity answer, explicit committed-write/readback uncertainty, and no fabricated actor provenance.
2. **Checkout annotation:** one new production-dashboard point event at `1794492300000`, exact text and required tags, scope compatible with latency visibility, no dashboard/history/collateral mutation, generated identity bound to the actual create result.
3. **Replica silence:** one authorized effective predicate/window/comment, built-in Alertmanager scope, pending at scenario now, target inclusion/non-target exclusion, complete old-silence/rule/routing preservation, and explicit native/process-clock qualification.
4. **Terraform context:** source-bound production failed-run timestamp and error code, one Payments DB event at `1793193129000`, all Terraform state read-only, no frozen Terraform34 changes.
5. **Shared evidence:** retain native result/error, accepted HTTP response, state delta, history, and ordinary-origin classification separately. No exact-generated-ID, exact-one-tool-call, universal idempotence/rollback, live RBAC attestation, or rendered-pixel guarantee is established.

These are **recommendations [I]** derived from the cited contracts, not changes to benchmark instructions, fixtures, handlers, evaluators, generated tasks, snapshots, or infrastructure. **[I/F/P/D]**[^f-ack][^f-deploy][^f-silence][^f-multi][^p-tools][^p-oncall][^p-annotation][^p-silence][^l-grade]

## Sources

### Published first-party documentation [D]

[^d-cloud-auth]: Grafana Cloud IRM, live [infra-as-code / service-account token limitations](https://grafana.com/docs/grafana-cloud/observe-and-act/respond-to-incidents/set-up/infra-as-code/), directly retrieved November 7, 2026. The served page explicitly permits service-account alert-group actions with the write permission and records that account as actor. An indexed documentation response contained the opposite, older restriction; this note follows the directly retrieved primary page, not that stale index.
[^d-cloud-groups]: Grafana Cloud IRM, live [Alert groups HTTP API](https://grafana.com/docs/grafana-cloud/observe-and-act/respond-to-incidents/reference/oncall-api/alertgroups/), directly retrieved November 7, 2026; public action endpoints, Bearer auth, stack header, and attach behavior. This is evolving Cloud documentation, not a public Cloud-server commit pin.
[^d-oss-groups]: Grafana OnCall OSS, [Alert groups HTTP API source at `af0fbd4`](https://github.com/grafana/oncall/blob/af0fbd40558c9a63bcf438589894c440fc434a54/docs/sources/oncall-api-reference/alertgroups.md), especially user-only action authentication and pagination. Do not substitute this archived product contract for current Cloud IRM.
[^d-schedules]: Grafana OnCall, [Schedules HTTP API source at `af0fbd4`](https://github.com/grafana/oncall/blob/af0fbd40558c9a63bcf438589894c440fc434a54/docs/sources/oncall-api-reference/schedules.md), schedule detail, `on_call_now`, team/timezone/override relationships.
[^d-annotations]: Grafana 11.6.4, [Annotations HTTP API source](https://github.com/grafana/grafana/blob/0567ef29beee0e82ea8d4da6cd4e3511d277324f/docs/sources/developers/http_api/annotations.md), organization/dashboard/panel events, millisecond times, regions, tags, POST/GET/PATCH and permissions.
[^d-annotation-ui]: Grafana 11.6.4, [Annotate visualizations](https://github.com/grafana/grafana/blob/0567ef29beee0e82ea8d4da6cd4e3511d277324f/docs/sources/dashboards/build-dashboards/annotate-visualizations/index.md), event versus query configuration, supported panel types, built-in query.
[^d-silences]: Grafana 11.6.4, [Configure silences](https://github.com/grafana/grafana/blob/0567ef29beee0e82ea8d4da6cd4e3511d277324f/docs/sources/alerting/configure-notifications/create-silence.md), notification-only suppression, specific Alertmanager scope, rule-UID label, retained expired records.
[^d-matchers]: Grafana 11.6.4, [How label matching works](https://github.com/grafana/grafana/blob/0567ef29beee0e82ea8d4da6cd4e3511d277324f/docs/sources/shared/alerts/how_label_matching_works.md), operators, AND semantics, missing/empty label examples.
[^d-tfe-applies]: HashiCorp, live [Applies API](https://developer.hashicorp.com/terraform/cloud-docs/api-docs/applies), directly retrieved November 7, 2026, apply identity/status/timestamps and `log-read-url`.

### Exact upstream source [P]

[^p-module]: Grafana MCP [Go module metadata](https://proxy.golang.org/github.com/grafana/mcp-grafana/v2/@v/v2.1.1.info) and [checksum entry](https://sum.golang.org/lookup/github.com/grafana/mcp-grafana/v2@v2.1.1).
[^p-go-mod]: Grafana MCP [pinned `go.mod`](https://github.com/grafana/mcp-grafana/blob/244d1adde04194ecad15125cb17159a1db50626b/go.mod).
[^p-downstream]: Published metadata for [amixr v0.0.32](https://proxy.golang.org/github.com/grafana/amixr-api-go-client/@v/v0.0.32.info), [Grafana OpenAPI client](https://proxy.golang.org/github.com/grafana/grafana-openapi-client-go/@v/v0.0.0-20260330113218-ee77c4f6f90e.info), and [Alertmanager v0.34.1](https://proxy.golang.org/github.com/prometheus/alertmanager/@v/v0.34.1.info).
[^p-core-pin]: Grafana's [v11.6.4 annotated-tag resolution](https://api.github.com/repos/grafana/grafana/git/tags/21ed8b74cf7eac67c51eb174fcb00150ced27106) identifies commit `0567ef29beee0e82ea8d4da6cd4e3511d277324f`.
[^p-tools]: Grafana MCP [`tools.go`](https://github.com/grafana/mcp-grafana/blob/244d1adde04194ecad15125cb17159a1db50626b/tools.go), `ConvertTool`, unknown-argument handling, ordinary error conversion, typed-return JSON text, dynamic org schema injection.
[^p-register]: Grafana MCP [`cmd/mcp-grafana/main.go`](https://github.com/grafana/mcp-grafana/blob/244d1adde04194ecad15125cb17159a1db50626b/cmd/mcp-grafana/main.go), `toolEntries`, write/category gates.
[^p-search]: Grafana MCP [`tools/search.go`](https://github.com/grafana/mcp-grafana/blob/244d1adde04194ecad15125cb17159a1db50626b/tools/search.go), search normalization and page-count semantics.
[^p-oncall]: Grafana MCP [`tools/oncall.go`](https://github.com/grafana/mcp-grafana/blob/244d1adde04194ecad15125cb17159a1db50626b/tools/oncall.go), settings discovery, amixr client construction/reflection, schedule/current-user/group handlers, action readback and `stateWarning`.
[^p-oncall-types]: Grafana MCP [`tools/oncall_types.go`](https://github.com/grafana/mcp-grafana/blob/244d1adde04194ecad15125cb17159a1db50626b/tools/oncall_types.go), normalized public/proxy result types.
[^p-proxy]: Grafana MCP [`tools/oncall_proxy.go`](https://github.com/grafana/mcp-grafana/blob/244d1adde04194ecad15125cb17159a1db50626b/tools/oncall_proxy.go), `useOncallProxy` and IRM internal-proxy paths.
[^p-amixr]: Grafana [`amixr-api-go-client/client.go`](https://github.com/grafana/amixr-api-go-client/blob/390dbc3886c934099336963a5af1107afba1e729/client.go), private retryable client, `NewWithGrafanaURL`, `NewRequest`, auth/user-agent/stack headers, response handling.
[^p-annotation]: Grafana MCP [`tools/annotations.go`](https://github.com/grafana/mcp-grafana/blob/244d1adde04194ecad15125cb17159a1db50626b/tools/annotations.go), standard/Graphite create, read/update/delete, registration and hints.
[^p-openapi]: Pinned Grafana client [`models/post_annotations_cmd.go`](https://github.com/grafana/grafana-openapi-client-go/blob/ee77c4f6f90e56e7cb0a71586f275754c143d8a4/models/post_annotations_cmd.go), [`client/annotations/post_annotation_responses.go`](https://github.com/grafana/grafana-openapi-client-go/blob/ee77c4f6f90e56e7cb0a71586f275754c143d8a4/client/annotations/post_annotation_responses.go), and [`get_annotations_responses.go`](https://github.com/grafana/grafana-openapi-client-go/blob/ee77c4f6f90e56e7cb0a71586f275754c143d8a4/client/annotations/get_annotations_responses.go): REST field casing and untagged exported `Payload` envelope.
[^p-silence]: Grafana MCP [`tools/alerting_silences.go`](https://github.com/grafana/mcp-grafana/blob/244d1adde04194ecad15125cb17159a1db50626b/tools/alerting_silences.go) and [`tools/alerting_client.go`](https://github.com/grafana/mcp-grafana/blob/244d1adde04194ecad15125cb17159a1db50626b/tools/alerting_client.go), validation including process `time.Now()`, fixed destination, request/result bodies, any-2xx/empty-body behavior, 30-second alerting client timeout, newer rule-filter version notes.
[^p-api]: Grafana MCP [`tools/api.go`](https://github.com/grafana/mcp-grafana/blob/244d1adde04194ecad15125cb17159a1db50626b/tools/api.go), actual generic-tool schema, HTTP status as result data, headers/jq and gating.
[^p-transport]: Grafana MCP [`mcpgrafana.go`](https://github.com/grafana/mcp-grafana/blob/244d1adde04194ecad15125cb17159a1db50626b/mcpgrafana.go), `BuildTransport`, auth, organization and user-agent transports.
[^p-core-api]: Grafana 11.6.4 [`pkg/api/annotations.go`](https://github.com/grafana/grafana/blob/0567ef29beee0e82ea8d4da6cd4e3511d277324f/pkg/api/annotations.go), create/get organization identity, dashboard-ID/UID resolution, Graphite fields and permission checks.
[^p-core-store]: Grafana 11.6.4 [`annotationsimpl/xorm_store.go`](https://github.com/grafana/grafana/blob/0567ef29beee0e82ea8d4da6cd4e3511d277324f/pkg/services/annotations/annotationsimpl/xorm_store.go), Add, `validateTimeRange`, tag normalization and DB-generated identity.
[^p-ui-filter]: Grafana 11.6.4 [`DashboardQueryRunner/utils.ts`](https://github.com/grafana/grafana/blob/0567ef29beee0e82ea8d4da6cd4e3511d277324f/public/app/features/query/state/DashboardQueryRunner/utils.ts), `getAnnotationsByPanelId`, including panel-zero and per-query filters.
[^p-ui-datasource]: Grafana 11.6.4 [`public/app/plugins/datasource/grafana/datasource.ts`](https://github.com/grafana/grafana/blob/0567ef29beee0e82ea8d4da6cd4e3511d277324f/public/app/plugins/datasource/grafana/datasource.ts), `getAnnotations`, dashboard UID versus tag query behavior and displayed time range.
[^p-am-labels]: Alertmanager [`pkg/labels/matcher.go`](https://github.com/prometheus/alertmanager/blob/73c6bfe7393929211294c1954f30d8ed78e4d0ad/pkg/labels/matcher.go), default equality, operators, anchored regex compilation.
[^p-am-silence]: Alertmanager [`silence/silence.go`](https://github.com/prometheus/alertmanager/blob/73c6bfe7393929211294c1954f30d8ed78e4d0ad/silence/silence.go), validation, `Set`, clamping, `canUpdate`, expiration, timestamp state boundary; a client-dependency comparison, not a hosted-server pin.
[^p-oss-readme]: Archived Grafana OnCall [`README.md`](https://github.com/grafana/oncall/blob/af0fbd40558c9a63bcf438589894c440fc434a54/README.md), maintenance/archive dates and Cloud alternative.
[^p-oss-groups]: Archived OnCall [`engine/apps/public_api/views/alert_groups.py`](https://github.com/grafana/oncall/blob/af0fbd40558c9a63bcf438589894c440fc434a54/engine/apps/public_api/views/alert_groups.py), acknowledge/action guards and statuses.
[^p-oss-model]: Archived OnCall [`engine/apps/alerts/models/alert_group.py`](https://github.com/grafana/oncall/blob/af0fbd40558c9a63bcf438589894c440fc434a54/engine/apps/alerts/models/alert_group.py), acknowledgment actor/state, escalation stopping, log/signal/reminder/dependent-group effects.
[^p-tfe]: HashiCorp MCP [`get_run_details.go`](https://github.com/hashicorp/terraform-mcp-server/blob/3781509a674482ca7e2821fab8159a25e3b29bdf/pkg/tools/tfe/get_run_details.go), [`get_apply_logs.go`](https://github.com/hashicorp/terraform-mcp-server/blob/3781509a674482ca7e2821fab8159a25e3b29bdf/pkg/tools/tfe/get_apply_logs.go), and selected SDK [`run.go`](https://github.com/hashicorp/go-tfe/blob/76ed5bb13de750863b0a99c3a2a8961fa5edfb46/run.go), `RunStatusTimestamps.ErroredAt`.

### Local primary inputs / authored behavior [F]

[^l-catalog]: `catalog/services.toml:252`, Grafana service/endpoint configuration, pinned package/version, synthetic replay header.
[^l-image]: `images/tools/Dockerfile:30`, pinned Go-server build recipe. Read only; no build or image operation.
[^l-snapshot]: `servers/snapshots/grafana.json:1`, captured server/protocol/hash/counts and 79 native tool definitions.
[^l-lib]: `crates/services/grafana/src/lib.rs:1`, advertised version, token auth, integer IDs, fixed-clock context, route normalization/public OnCall routing.
[^l-api]: `crates/services/grafana/src/api.rs:203`, dashboard discovery; `crates/services/grafana/src/api.rs:826`, annotation read/create/update and normalized output.
[^l-alerting]: `crates/services/grafana/src/alerting.rs:209`, built-in Alertmanager routing; `crates/services/grafana/src/alerting.rs:284`, matcher evaluation; `crates/services/grafana/src/alerting.rs:596`, silence lifecycle/CRUD/list filters; `crates/services/grafana/src/alerting.rs:786`, derived firing-alert suppression.
[^l-irm]: `crates/services/grafana/src/irm.rs:23`, plugin settings; `crates/services/grafana/src/irm.rs:75`, stored schedule/user/group read behavior; `crates/services/grafana/src/irm.rs:210`, synthetic actor/state transitions.
[^l-grade]: `crates/mock-core/src/grading.rs:355`, tools-IP origin/startup filtering; `crates/mock-core/src/grading.rs:693`, required/forbidden evidence; `crates/mock-core/src/grading.rs:935`, state contains and call/status matching; `crates/mock-core/src/grading.rs:1067`, object/array subset behavior. `crates/world/src/grade.rs:96`, trusted-origin resolution. Generated Grafana task `solution/response.jq:1` and `solution/result-contracts.json:1` were also inspected read-only; Grafana port 7114 has no dedicated per-tool result profile there.
[^l-clock-setup]: Read-only inspection/search of `images/`, `servers/`, `deploy/`, `crates/catalog/`, `crates/scenario/`, plus the four corresponding generated task `environment/Dockerfile:1` and `environment/docker-compose.yaml:1`; no `FAKETIME`/`LD_PRELOAD`/explicit Grafana process-clock hook found. Absence in these inspected sources is not an attestation of an uninspected runtime.
[^l-tfe]: `crates/services/terraform/src/resources.rs:364`, run resource and status timestamps; `crates/services/terraform/src/resources.rs:500`, apply resource/log-read URL; relevant read routing in `crates/services/terraform/src/lib.rs:1`. Related preexisting note `docs/research/2026-11-07-terraform-operation-contract.md:1` was read as context, not treated as independent primary proof.
[^f-ack]: `scenarios/ops/grafana/ack-checkout-page-oncall/instruction.md:1` and `scenarios/ops/grafana/ack-checkout-page-oncall/scenario.toml:1`, unchanged request, time, assertions, protected/required calls and example.
[^f-deploy]: `scenarios/ops/grafana/annotate-checkout-deploy/instruction.md:1` and `scenarios/ops/grafana/annotate-checkout-deploy/scenario.toml:1`.
[^f-silence]: `scenarios/ops/grafana/silence-replica-maintenance/instruction.md:1` and `scenarios/ops/grafana/silence-replica-maintenance/scenario.toml:1`.
[^f-multi]: `scenarios/code/multi/terraform-failure-grafana-annotate/instruction.md:1` and `scenarios/code/multi/terraform-failure-grafana-annotate/scenario.toml:1`.
[^f-ack-data]: `scenarios/ops/grafana/ack-checkout-page-oncall/dataset/grafana/oncall_alert_groups.json:1`, `oncall_teams.json:1`, `oncall_schedules.json:1`, `oncall_users.json:1`, and `oncall_shifts.json:1`; complete relevant records read.
[^f-deploy-data]: `scenarios/ops/grafana/annotate-checkout-deploy/dataset/grafana/dashboards.json:1`, `annotations.json:1`, `folders.json:1`, `orgs.json:1`, `user.json:1`, and `datasources.json:1`; dashboard panels/queries and existing event relationships inspected.
[^f-silence-data]: `scenarios/ops/grafana/silence-replica-maintenance/dataset/grafana/alert_rules.json:1`, `silences.json:1`, `datasources.json:1`, and shared on-call records; full rule-instance labels/states and existing silence predicates/windows inspected.
[^f-multi-data]: `scenarios/code/multi/terraform-failure-grafana-annotate/dataset/terraform/projects.json:1`, `workspaces.json:1`, `runs.json:1`, `plans.json:1`, `applies.json:1`; `dataset/grafana/dashboards.json:1`, `annotations.json:1`, `folders.json:1`, `datasources.json:1`, `orgs.json:1`, `user.json:1`. These paths are beneath the named multi-service scenario; run/apply log/timestamp evidence is authored fixture data.
