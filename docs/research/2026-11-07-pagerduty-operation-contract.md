# PagerDuty WRITE operation contracts — 2026-11-07

## Scope and evidence discipline

Research-only sidecar for:

- `ops/pagerduty/cover-shift-override`
- `ops/pagerduty/reassign-replication-lag`
- `ops/pagerduty/resolve-prod-checkout`

Only this document is authored. The three instructions remain unchanged. No implementation, test/build, dependency installation, Docker, account/infrastructure operation, benchmark inference, evaluated-tool auxiliary-model call, delegation, or qualification was performed. Published distributions were fetched and inspected as ZIP contents in memory, not imported or executed. Neither frozen Terraform34/Grafana35 roots nor active Grafana campaign roots were used.

Evidence labels:

- **[D] Current documentation:** first-party hosted REST/product documentation retrieved November 7, 2026. This establishes what is documented, not observed production behavior.
- **[P] Exact pinned source:** published package contents at the versions/hashes below, or a separately identified first-party source revision. Static inspection is not live vendor conformance.
- **[F] Authored local behavior:** instructions, datasets, catalog, captured schema, generated reference artifacts, Rust simulator, or existing test source. Tests were read, not run.
- **[I] Inference/proposal:** a static consequence, counterexample, or recommended evaluator policy; not an executed control.
- **[U] Uncertainty:** an observation or scope decision not established here.

The word **v2** below means the benchmark's `expect.version = 2`, except where explicitly describing PagerDuty's REST v2 or shift-based REST v3. These are different version axes. All three current write scenarios lack an `expect.version` declaration and remain legacy contracts. Existing v2 preservation/history mechanisms are migration vocabulary, not a claim these writes have been migrated. **[F]**[^f-scenarios][^f-contracts]

## Principal findings

1. **The native write surface is much narrower than REST.** The pin has `manage_incidents`, with the actual input spelling **`assignement`**, not `assignment`. It supports one directly assigned user per call, not multiple assignees or a replacement escalation policy. There is no generic HTTP tool, single-incident update tool, classic override list/delete tool, or incident-note update/delete tool in the captured 101-tool surface. Do not present REST-only alternatives as reachable native plans. **[P/F]**[^p-incidents][^p-models][^f-snapshot]
2. **Successful MCP/HTTP envelopes can contain failed operations.** Classic override creation returns a bare REST array of per-item `status`, optional `errors`, and `override`. FastMCP exposes it under `structuredContent.result` and as individual JSON text blocks. An item with `status:400` does not itself cause `isError:true`. A correctly spelled but ignored `assignment` can instead produce a successful empty `response` and no write. The current PagerDuty result profile checks neither case. **[P/F]**[^p-schedules][^p-sdk][^p-fastmcp][^f-results][^d-create-override]
3. **Actor identities differ across the three fixtures.** Cover and reassign use Alex Kim (`PA1EXKM`, `alex.kim@tidewater.io`); resolve uses Lena Weber (`PLWEBR5`, `lena.weber@tidewater.io`), not Alex or the target incident's initial assignee Theo Brown. The connector resolves `/users/me` at context initialization and caches `From`; the local server chooses that identity from fixture account data rather than authenticating the token against a vendor account. **[P/F]**[^p-context][^f-data][^f-util]
4. **A reassignment is not a policy migration.** The requested singleton new assignee is Dana Nguyen, the production DBRE level-1 responder at scenario time. Native direct assignment replaces the assignment list but locally retains the incident's original Platform policy, service and team. Current product documentation says direct-user reassignment stops using the escalation policy for further escalation. Requiring a DBRE policy field or changing the `orders-db` service configuration would misgrade or exceed the request. **[D/P/F/I]**[^d-reassign][^p-incidents][^f-incidents][^f-data]
5. **Override success means exact effective coverage and preserved rotations/history, not a row at `/1`.** Adjacent overrides for the same Jordan Lee covering exactly the eight-hour window can be business-equivalent to one row; the legacy assertion forbids the second row. Conversely, an extra override on Payments can pass its limited guards. The Rust timestamp parser also discards fractional seconds, and classic deletion erases active/past overrides contrary to current documented cancellation semantics. **[D/F/I]**[^f-scenarios][^f-schedules][^f-util][^d-delete-override]
6. **Resolution must finish the incident, not merely acknowledge it or mention a release number.** The requested note must communicate that rollback of `2026.10.29.3` resolved the production incident. The current substring assertion accepts contradictory prose. Locally, resolution also resolves both associated production alerts, clears assignments/pending actions, and appends a timeline entry; those are necessary scoped automatic effects, not blanket permission to edit all incident/alert fields. **[F/I]**[^f-scenarios][^f-incidents][^f-records][^f-data]
7. **“Resolved is forever terminal” is not a current vendor guarantee.** The currently published first-party OpenAPI permits reopening resolved incidents to `triggered` or `acknowledged`. This pin's management input excludes `triggered`, and the local Rust handler rejects status/assignment changes on resolved incidents. Require final resolution for this request without importing an unsupported universal lifecycle claim. **[D/P/F]**[^p-api-schema][^p-models][^f-incidents]

## Exact pins and reproducibility boundary

### Connector, dependencies, and captured inventory

| Layer | Inspected identity | Meaning |
| --- | --- | --- |
| Catalog endpoint | Service/endpoint `pagerduty`; `mode="real"`; world port `8111`; MCP port `7111`; `runtime="pypi"` | The real connector is routed to the local authored HTTP service, not to a live account. |
| Native package | `pagerduty-mcp==1.1.0`; CLI `pagerduty-mcp --enable-write-tools` | First-party project identified by package metadata as `PagerDuty/pagerduty-mcp-server`; wheel uploaded `2026-07-07T22:14:44.958752Z`. |
| MCP wheel | `pagerduty_mcp-1.1.0-py3-none-any.whl`; SHA-256 `f2e3012e5c7b233ce855ec9bfaa41e160ef18a6af64753d858bf8b5d9e13fa05` | Downloaded bytes independently hashed. Source file/symbol references below refer to this distribution, not mutable GitHub `main`. |
| MCP sdist metadata | `pagerduty_mcp-1.1.0.tar.gz`; published SHA-256 `5fe6cf722b5921deebf89f001fbc3868158fdc30f7544e9ea28bca3399065b30` | PyPI-advertised source-distribution hash; the wheel, not this tarball, was the inspected source. |
| Native first-party REST SDK | `pagerduty==5.3.0`; wheel SHA-256 `bcc0686b273d652b4499aba51af5836b5b2abebdf0b8e24a65c5518e0d502f5b` | Downloaded bytes hashed; first-party `python-pagerduty` tag `v5.3.0` resolves to `af123e1378ec205a89cab96fccf86f97a4d68295`. |
| MCP framework | `mcp==1.30.0`; wheel SHA-256 `666edb5009503e1047c9d60346a756f94b261f05cc2625f23d41c728ffc484d0` | Downloaded bytes hashed; its FastMCP/low-level source determines output normalization and tool-error envelopes. |
| Other selected lock entries | `pydantic==2.14.2`, `pydantic-core==2.50.2`, `requests==2.34.2`, `urllib3==2.8.0`, `typer==0.16.1`, `httpx==0.28.1` | Lockfile declarations, not independently executed or audited packages. MCP metadata allows ranges (`mcp[cli]~=1.8`, `pagerduty~=5.3`, `typer~=0.16.0`); the local lock is the tighter selection. |
| Classic API comparison | `PagerDuty/api-schema` revision `9c84115d71fdeba32cb796ac711ea91e7bd7ad0e`, `reference/REST/openapiv3.json`; file SHA-256 `6471be3950a0023c3e3e4153b2a44c6462c2d622ba94b881260f1d8e905c22c8` | Retrieved and byte-hashed at that revision. This is a separately pinned contemporary published specification, not the implementation behind a live vendor endpoint. |
| Captured inventory | `server_info={"name":"PagerDuty MCP Server","version":"1.30.0"}`; protocol `2025-06-18`; 101 tools | The advertised `1.30.0` is the framework version, **not** evidence that `pagerduty-mcp` is version `1.30.0`. Snapshot is static, not a fresh `tools/list`. |
| Snapshot's declared hash | `fa12e2fccc516c61d29c906e8cf42e9557bf43c250f5d5ee8ccb09992a83acdc` | Inventory's own `sha256` field; distinct from the file-byte hash below. |

All entries above are **[P/F]**, except the specification's documented behavior **[D]**. Sources: catalog/lock, published distribution metadata/bytes, SDK tag resolution, framework source and local snapshot. **[P/F/D]**[^f-catalog][^f-lock][^p-wheel][^p-sdk][^p-fastmcp][^p-api-schema][^f-snapshot]

No MCP Git commit is asserted: the inspected repository tag listing returned no tags, and lookups for `v1.1.0` and `1.1.0` returned 404. The immutable distribution hash is the defensible source pin; deriving a commit from the version string would invent provenance. No tools-image digest, installed-package observation, or native runtime execution was obtained. Existing generated environment references are declarations, not proof of what an image currently contains. **[P/F/U]**[^p-mcp-repository][^f-environment]

### Runtime/authentication path

Catalog environment is `PAGERDUTY_USER_API_KEY="u+toolbench"` and `PAGERDUTY_API_HOST="{world}"`; synthetic replay injects `Authorization: Token token={env.PAGERDUTY_USER_API_KEY}`. That is an authored nonsecret credential and replay template, not a vendor token or proof of an authenticated human. The catalog launches the stdio connector behind an HTTP/SSE bridge, one child per MCP session. The Dockerfile declares Python 3.13 environments installed from the Python lock. **[F]**[^f-catalog][^f-runtime]

Pinned `create_pd_client` rejects a missing/empty key, constructs `RestApiV2Client` using default token auth, and replaces its URL with `PAGERDUTY_API_HOST` if nonempty; absent host defaults to `https://api.pagerduty.com`. SDK headers include `Accept: application/vnd.pagerduty+json;version=2`, JSON content type for writes and a User-Agent prefixed by `pagerduty-mcp/1.1.0`. OAuth/Bearer exists in the SDK but is not selected by this catalog/context construction. Current hosted authentication documentation requires HTTPS and genuine account/user credentials; the local HTTP origin is a test substitution. **[D/P/F]**[^p-context][^p-sdk][^d-auth][^f-catalog]

Context initialization calls `/users/me`, validates a `User`, caches it and writes its email into the SDK's `From` header. Initialization exceptions are caught/logged and leave user context absent; startup does not necessarily abort. `list_incidents(request_scope="assigned"|"teams")` then raises if there is no user context. Incident writes/notes can fail because their required `From` was never established. `get_user_data` independently reads `/users/me` but does **not** refresh the cached context or `From`; calling it is useful evidence, not a required setup ritual or a repair for that cache. **[P]**[^p-context][^p-users][^p-incidents]

Local `Ctx::me()` uses `account/account.user_id`, falls back to the first stored user and then a synthetic default. `require_from()` checks only that the submitted email names a stored user; it does not bind the email to the token, verify roles, or implement account/OAuth permissions. The common service wrapper checks the trusted API caller host when configured, not PagerDuty RBAC. Synthetic replay headers establish replay comparability, not real user authentication. **[F]**[^f-util][^f-serve][^f-catalog]

### Local input binding

Worktree HEAD observed: `4ef643dac101fba8e67701f8c0ac7f9dd6e3e5fd`. This is a shared dirty checkout; HEAD does not identify its current working files. The following hashes identify inspected local inputs at research time. **[F]**

| Input | SHA-256 |
| --- | --- |
| `catalog/services.toml` | `52ff7e8aecb3c4d0aaf7b5e8a2440d22113c59d92cc3be8f9c2822f19f24e34e` |
| `servers/python/pagerduty.txt` | `8010cd0fc51ac1e884fd1b11d2bb56215081514e6bf2520c0fbc4edfb607f652` |
| `servers/snapshots/pagerduty.json` | `51174f68d9bcd23620d957a7c68f5582ad82c99c287d1d84a2c03c2576945fc6` |
| `crates/services/pagerduty/src/lib.rs` | `b040492ab39b8eda57e97e059988ff5302fac01345a739bb1b2914a02c75996d` |
| `crates/services/pagerduty/src/incidents.rs` | `e200d0ce11a7344a13ee5bb95a48da1568f9b80e640d7118966c215194acda90` |
| `crates/services/pagerduty/src/schedules.rs` | `99ef37d84575ba86c5318971fa7b9ecd1b109948ee89241ad2535e9abbf8151d` |
| `crates/services/pagerduty/src/util.rs` | `2ab93a289c000b6b35296a59ebe833f40610078a3c24682d1695a383107bfd18` |
| `crates/services/pagerduty/src/directory.rs` | `793617efb5b145c8ddb01eede079d5328909323e3cb4d90af272ac6dbdbe2767` |
| `crates/services/pagerduty/src/records.rs` | `979ed91fe1cb567c3d1a716eb90d88588b230ad90f4a61b4fd1b54a72e91d87d` |
| Cover instruction / scenario | `1a7b078a204257ccd4b1606d1e12461b622dc62418a7dc3fbeca726bcde5ace0` / `f6b8c158ab6f178e7d6d9a320a7be05b822fab609e362fcb69ea19e0ac3c42ea` |
| Reassign instruction / scenario | `483144b2b644debd0926417befbf3ca37ac4d7f65fbb77180d4d449469802595` / `317f52284030b7a7624b610d15f8b2a0856c233b1d676051cac70f5b2f4b8425` |
| Resolve instruction / scenario | `233d114ad31199979a6bb219453e8fe33400674a2d238cfb28ff52938c3f8127` / `18fb259a8cf975d35a3dc613a1e78ffc457d197e81369811d038e51625eded01` |

Dataset tree pins use SHA-256 over a UTF-8 manifest: for every file under the task's `dataset/`, sorted by relative path, append `relative_path + TAB + lowercase_file_sha256 + LF`. No file contents are rewritten. **[F]**

| Task dataset | Files | Manifest SHA-256 |
| --- | --- | --- |
| Cover | 17 | `5645343da3755da1ac19dd2b3869d9ccd79d37381b3d8eb1c69376348b94feaa` |
| Reassign | 16 | `d2c67c6703d40e7ee88f4a3adf762877f350d3ef263a9068b775ab77afa7aefb` |
| Resolve | 24 | `a46e28987901c76b8f389c778ee6c721797b2fc4ef4274640df2538289bda19e` |

Generated instruction copies equal the corresponding authored instructions byte-for-byte. All three inspected generated result-contract files have SHA-256 `006703a9efff2f3faef759691a9e0d893b1773c3812ac0c82b8301c3c007b900`; their `response.jq` hashes are `e061c9ef4f7a069858317d9b3a5d1212896e1f7a3ca7126cd7db1621f2377d60`; their `tool-result.jq` hashes are `e9d58cc6fd0c69b79ffbd758c8506785a75f0695df8365943eb8f3bab975f545`. These are observations of existing generated files, not regenerated outputs. **[F]**[^f-results]

## Native request/result contract

### Reachable operations, normalization, and lost fields

| Native call | Actual HTTP normalization | Native output / limits |
| --- | --- | --- |
| `manage_incidents({manage_request:{incident_ids:[...],status:"resolved"}})` | `PUT /incidents`, body `{"incidents":[{"type":"incident_reference","id":...,"status":"resolved"}]}` | SDK unwraps `incidents`; typed `ListResponseModel[Incident]` contains `response` and computed `response_summary`. |
| `manage_incidents({manage_request:{incident_ids:[...],assignement:{id:...}}})` | Same bulk path; each incident gets **one-element** `assignments`, `assignee:{type:"user_reference",id:...}`, plus client-generated naive `datetime.now().isoformat()` in `at` | Result is not raw incident JSON. `assigned_via`, escalation policy, team, urgency, acknowledgements, pending actions, actor and body are omitted by the pinned `Incident` model. |
| `add_note_to_incident({incident_id,note})` | `POST /incidents/{incident_id}/notes`, `{"note":{"content":note}}`; SDK unwraps `note` | Typed note includes `id`, `content`, `created_at`, `user`; drops channel and newer `updated_at`. Parent incident ID comes from the request, not a returned note field. |
| `create_schedule_override({schedule_id,override_request:{overrides:[{start,end,user_id}]}}})` | Datetimes become `.isoformat()` strings; `user_id` becomes `user:{id,type:"user_reference"}`; `POST /schedules/{schedule_id}/overrides` | SDK special case wraps request in `overrides` but **does not unwrap response**. Returns raw `dict | list`, normally the per-item REST array. No parent schedule field or creator field is guaranteed. |
| `list_incidents({statuses,request_scope,since,until,urgencies,priorities,limit})` | `statuses[]`; `priorities` becomes `priority_ids[]`; assigned/teams scope derives IDs from cached user | Auto-paginated, normally capped at 1000 records (`limit` changes cap). No native service-ID, incident-key, offset, sort, or `date_range` parameter, despite richer unused query models/REST support. Top-level native spelling really is `statuses`. |
| `list_oncalls({...})` | Supports policy/schedule/user filters and explicit datetimes. `service_ids` is connector-side sugar: GET each service, then use its policy ID. `earliest=true` is sent by default | Auto-paginated; cap is `limit or 1000`. Fields include policy, level, schedule, user, start/end. There is no level filter; select level 1 from returned records. |
| `list_users({query,teams_ids,limit})` | `GET /users`; note plural input spelling `teams_ids` | **Single** `rget`, not `paginate`; default API page size requested is 100. Returned `ListResponseModel` loses REST `more/offset/total`. No native offset argument. |
| `list_schedules({query_model,...})` | Independently queries classic v2 and shift-based v3, optionally enriches v3 names (cap 50) | Merged summaries with `kind`, `sources`, `degraded`; summaries omit teams, users and layers even if underlying classic request included layers. v3 honors only query/limit, not classic user/team filters. |
| `get_schedule({schedule_id,kind?})` | Classic GET by default; v3 redirect fallback unless a kind hint bypasses it | `structuredContent.result` wraps a typed schedule union. Classic model exposes layers/users/teams, but drops `final_schedule`, `overrides_subschedule` and rendered layer entries. No native since/until argument. |
| `list_schedule_users({schedule_id})` | `GET /schedules/{id}/users`, no time window | Membership, **not** a proof of who covers a requested instant/window. Locally includes override users as well as rotation users. |

All rows are **[P/F]**. Relevant declarations and implementations are in the pinned wheel and captured snapshot; local rendering may contain more fields than those survive into MCP. **[P/F]**[^p-incidents][^p-models][^p-schedules][^p-users][^p-oncalls][^p-utils][^p-sdk][^f-snapshot][^f-schedules]

`IncidentManageRequest` does not forbid unknown properties. `assignment`, raw REST `assignments`, `escalation_policy`, or `resolution` are not supported management fields and can be ignored rather than applied. If no recognized truthy action is provided, `manage_incidents` returns `response:[]` without writing. Its docstring advertises `assignment`, but source/schema actually require `assignement`. Native tool success therefore must include nonempty request-bound effects, not just a parsed object. Do not confuse the unused `IncidentQuery` model's `status` spelling with the registered `list_incidents` function's supported `statuses` input. **[P/F/I]**[^p-incidents][^p-models][^f-snapshot]

If several supported management fields are supplied together, they are **separate HTTP writes** in this fixed implementation order: status, urgency, assignment, escalation level. Only the last nonempty response is returned; an exception later can follow earlier committed effects. This source order is a hazard to model, **not** a reference order the evaluator should require an agent to reproduce. Sending one `manage_incidents` call is not an atomic combined action. **[P/I]**[^p-incidents]

### Full native envelopes and nested failure handling

The pinned framework validates tool inputs, calls the function, converts its output, then validates structured output. A typed Pydantic model produces one JSON text block and `structuredContent` containing the dumped model. A function returning `dict | list` is wrapped as `structuredContent:{"result":...}`; the unstructured content recursively flattens lists, producing a JSON text block **per item**, not necessarily one JSON array block. Empty lists produce no text blocks. `get_schedule` is another `result`-wrapped union. Computed summaries/types can appear in model serialization. **[P]**[^p-fastmcp]

Schematic successful outputs (placeholders describe values; they are not prescribed IDs):

```json
{
  "jsonrpc": "2.0",
  "id": 2,
  "result": {
    "isError": false,
    "content": [{"type": "text", "text": "<JSON ListResponseModel[Incident]>"}],
    "structuredContent": {
      "response": ["<normalized incident object>"],
      "response_summary": "<computed summary>"
    }
  }
}
```

```json
{
  "jsonrpc": "2.0",
  "id": 2,
  "result": {
    "isError": false,
    "content": [{"type": "text", "text": "<JSON per-item override result>"}],
    "structuredContent": {
      "result": [{
        "status": 201,
        "errors": [],
        "override": {
          "id": "<server-allocated ID>",
          "start": "2026-10-31T09:00:00Z",
          "end": "2026-10-31T17:00:00Z",
          "user": {"id": "PJ0RLEE", "type": "user_reference"}
        }
      }]
    }
  }
}
```

These examples describe source-derived shapes, not captured runtime successes. Vendor success may omit `errors`; the Rust implementation includes an empty list. IDs, reference URL fields and optional fields need not match illustrative API examples. **[D/P/F/I]**[^p-fastmcp][^p-schedules][^f-schedules][^d-create-override]

Four distinct failure levels must remain distinct:

1. **Transport/session/protocol failure:** non-success bridge HTTP status, malformed/truncated SSE or JSON, wrong/multiple response IDs, JSON-RPC `error`, incomplete content, invalid result types. The bridge can also deliver a normal JSON-RPC result over HTTP 200/SSE while the tool fails. **[F]**[^f-front][^f-results]
2. **Tool exception:** input/Pydantic errors, SDK HTTP errors, decoding errors or output validation errors normally become `result.isError:true` with a text error. Tool-execution exceptions are wrapped as `Error executing tool <name>: <exception>`; low-level validation errors can use `Input validation error: ...` or `Output validation error: ...`. Do not demand one universal error-text prefix, raw REST body or retained HTTP status in the MCP error result. **[P]**[^p-fastmcp][^p-sdk]
3. **Successful tool carrying failed suboperations:** classic override REST can return HTTP 201 containing `{"status":400,"errors":[...],...}`. SDK `successful_response` checks outer status; the connector returns nested objects unchanged; FastMCP's broad result schema accepts them with `isError:false`. A bare top-level error dictionary on a 2xx override response can likewise fit `dict | list`; this is a constructed envelope counterexample, not a claim the vendor emits it. Check complete per-item status/errors, required result identity, request-bound user and times, and text/structured agreement. Current documentation's failure example includes the rejected `override`; the Rust failure element omits it. Neither is success. **[D/P/F/I]**[^d-create-override][^p-sdk][^p-schedules][^p-fastmcp][^f-schedules]
4. **Successful but empty/degraded/irrelevant result:** `manage_incidents` can return no records and make no update; unified schedule listing catches source exceptions and returns `degraded:true`/`sources[].status:"error"` rather than a tool error. A partial list is not proof a resource does not exist. Do not globally fail every degraded read if the independently successful source establishes the needed target; do not mistake degraded completeness for complete success either. **[P/I]**[^p-incidents][^p-schedules]

The existing `response.jq` handles outer protocol errors and `isError`; PagerDuty port `7111` has an empty mapping and no default result profile in the generated `result-contracts.json`, so `tool-result.jq` selects only `mcp_envelope_v1`. It does not inspect override item failures, bind returned incident sets to the request, validate the requested note's whole content, or detect an actionless management result. Migration needs operation-specific contracts rather than copying another service's envelope assumptions. **[F/I]**[^f-results]

## Business identities, clock, and discoverability

### Exact fixture relationships

The common core datasets have 11 users, four teams, eight classic schedules, eight policies, 11 technical services, 12 incidents, seven alerts, 21 log entries, two notes and one existing classic override. Cover additionally includes a v3 Security schedule; resolve additionally includes status-page/webhook fixtures. Common core files are byte-identical across these tasks except `account.json`; cover's extra `schedules_v3.json` is not a migration of Platform Primary to v3. **[F]**[^f-data]

| Purpose | Authoritative fixture relationship |
| --- | --- |
| Cover requester | Account owner Alex Kim `PA1EXKM` → Platform `PTPLATF`; Platform Primary `PSPLTPR`; weekly layer `PL8T1A2`; policy `PEPPLAT` level 1 targets that schedule. |
| Cover replacement | Jordan Lee `PJ0RLEE`, Platform `PTPLATF`, email `jordan.lee@tidewater.io`, user timezone America/Chicago. Jordan Alvarez `PJ0RALV` belongs to Payments `PTPAYMT` and is not the replacement. |
| Schedule look-alikes | Platform Secondary `PSPLTSC` is level 2 of `PEPPLAT`; Data Platform Primary `PSDPLPR` is targeted by `PEPDPLT`; shift-based Security Follow-the-Sun `PSV3SEC` is a different schedule. |
| Reassign target | Incident `Q3K8ORDBLAG2RX`, number 4812, “Replication lag above 300s on orders-db-replica-2”; service `PSVORDB` (`orders-db`, explicitly production); policy `PEPPLAT`; Platform team; initially triggered, high urgency, assigned only Alex. Alert `PALR004` has host `orders-db-replica-2`, lag 412 seconds, disk 100%. |
| DBRE primary | Database Reliability team `PTDBREL` → production policy `PEPDBRE` → level 1 schedule `PSDBRPR` (“DBRE Primary”) → Dana Nguyen `PDNGYN2` at scenario time. Level 2 is Sam Okafor `PS4MOKF`, a direct user target. |
| DBRE distractors | `PEPDBST` (“Database Reliability - Staging”) → `PSDBRST` → Sam; inventory incident `Q1INVDBLAG7QWE` #4813 remains assigned Alex/triggered; old orders replica-1 incident `Q2ORDBOLD4KZPA` #4761 is resolved; staging `Q5DBSTG9LAGT3N` #4806 is triggered/low urgency. |
| Resolve target | `Q7CHKAP15PCT1D` #4810, “Checkout error rate above 5%”; production `checkout-api` service `PSVCHKA` → Commerce `PTCOMRC` / policy `PEPCOMM`; initially triggered/high/P1 `PPRIO01`, assigned Theo Brown `PTBRWN8`. |
| Resolve actor | Account owner Lena Weber `PLWEBR5`, not Theo; email `lena.weber@tidewater.io`. A resolver need not be the initial assignee. |
| Resolve distractors | Identically titled staging `Q8CHKSTG5PCT2F` #4811 on `PSVCHKS` remains triggered/low; production frontend `Q9CHKWEB5XX3VK` #4808 on `PSVCHKW` remains acknowledged by/assigned Lena; old production incidents `QA6CHKAPIOLD0D` #4752 and `QB2CHKAPIOLD9S` #4655 stay resolved and unchanged. |
| Target production alerts | `PALR001` us-east-1 and `PALR002` eu-west-1 belong to `Q7CHKAP15PCT1D`; both carry release `2026.10.29.3`. `PALR003` belongs to staging. Change event `PCE0001` links that release to production service `PSVCHKA`. |
| Existing override to preserve | Storage key `PSPAYPR/1`, vendor-style ID `PQ7OV01`: Priya Raman `PPR1YAR` covers Payments Primary `PSPAYPR`, `2026-10-30T06:00:00Z` through `2026-10-31T06:00:00Z`. |
| Existing notes/history | `Q9CHKWEB5XX3VK/1` / `PNOTE01` is Lena's CDN/ingress investigation; `QA6CHKAPIOLD0D/1` / `PNOTE02` concerns a different rollback/timeout incident. All 21 initial timeline records are history, not disposable grading noise. |

All relationships in this table are **[F]**. Environment is established by service identity/descriptions and alert/change-event context, not a required native `incident.environment` field: that field does not exist in this pinned normalized incident. **[F/P/I]**[^f-data][^p-models]

Dana's `users[].teams` includes DBRE, but the separate `team_members.json` has only five authored rows and does not explicitly list Dana. Do not demand a missing membership row when an authoritative exposed user/team relationship already establishes her membership. Conversely, name similarity, job title, the first policy matching “Database,” or the first on-call record is not enough. **[F/I]**[^f-data]

### Business time versus wall time

All three scenarios declare `now="2026-10-30T14:00:00Z"`. Cover's “tomorrow (Saturday, October 31)” means **October 31, 2026**, 09:00–17:00 UTC, not the day after this research date, November 7, 2026. At research time that requested window is already in the past in the real world but still future in the authored scenario. Do not move it to November 8, silently reinterpret it in Alex's or Jordan's local timezone, or change the instruction to make live execution convenient. **[F/I]**[^f-scenarios]

The benchmark core defaults to a fixed business clock; legacy-transport stepping also exists. Operation timestamps should be bound to the recorded business-clock policy/evidence, not a reference HTTP count. The pinned native reassignment helper independently inserts a naive process-wall-clock `at`. The Rust handler ignores supplied assignment `at` and stores `ctx.now_str()`. Thus client input and committed assignment time can differ even on the reference path. Equality to that client timestamp, process timezone, a generated sample timestamp, or a “first write plus seven seconds” calculation is not a valid universal contract. **[P/F/I]**[^p-incidents][^f-clock][^f-incidents]

At scenario time, DBRE's daily rotation selects Dana: October 30 is 29 daily turns after October 1, alternating Sam then Dana. Platform Primary's weekly layer selects Alex in the requested window: seven elapsed weekly turns after September 7, modulo `[Mei,Alex,Jordan]`. These are fixture-derived calculations, not observations of a live schedule. Prefer reachable `list_oncalls` for effective coverage, because raw layer arithmetic alone ignores overrides and additional layers. **[F/I]**[^f-data][^f-schedules]

### Pagination and disambiguation objections

- SDK classic pagination requests pages (default size 100), advances offset by the actual returned row count, follows `more`, and stops at its iteration boundary. Connector `paginate` additionally truncates to `maximum_records` (normally 1000). Its `ListResponseModel` warns only when the returned count equals 1000; a smaller caller-selected cap can silently truncate without that warning. Do not equate “no warning” with exhaustive discovery. **[P]**[^p-sdk][^p-utils][^p-models]
- `list_users` reads one page and exposes no offset. An augmented fixture with more than 100 matching Jordans needs a sufficiently selective name/team query or another genuinely reachable relationship; it cannot require an impossible native next-page call. Current 11-user data does not exercise this. **[P/F/I]**[^p-users][^f-data][^f-snapshot]
- `list_incidents` cannot forward service filters or `date_range="all"`; it can use statuses, assigned scope and explicit dates. REST defaults search one month; local Rust uses a 30-day default and 183-day maximum window. Supported explicit windows or direct `get_incident` after discovering an identity are valid alternatives. Do not require a filter absent from the function merely because an unused model or official REST page has it. **[D/P/F]**[^d-list-incidents][^p-incidents][^f-incidents]
- `list_schedules` can return v3 results outside a classic team/user filter and can cap v3 enrichment/page coverage. Read `kind`, `sources`, `more`, and `degraded`; resolve the target with `get_schedule` for teams/layers. `list_schedule_users` is not a time-filtered rendered schedule. **[P/F]**[^p-schedules][^f-snapshot]
- `list_oncalls` returns escalation levels together; default `earliest=true` collapses repeated entries for a policy/level/user, not “give me level 1 only.” For multi-segment coverage readback use explicit time bounds and `earliest=false`, retaining all relevant segments. Current REST describes overlap queries and a future horizon; local Rust enforces at most a 90-day range, not the same complete hosted horizon policy. **[D/P/F/I]**[^d-oncalls][^p-oncalls][^f-schedules]

## 1. Cover the Platform primary shift

### Required business outcome

Authorize a temporary substitution on **classic** Platform Primary `PSPLTPR` for Jordan Lee `PJ0RLEE`, exactly

`[2026-10-31T09:00:00Z, 2026-10-31T17:00:00Z)`.

The requested interval is eight hours (28,800 seconds). Require effective final coverage to be Jordan throughout the interval, with no gap or different winning user, and require the preexisting schedule coverage outside it to remain unchanged. Preserve the regular rotation, all other schedules, policies, users and preexisting overrides. The authorization is to change routing for that window, not to create a page, acknowledge incidents, move Jordan between teams, modify notification rules, or replace the rotation with Jordan. **[F/I]**[^f-scenarios][^f-data][^d-edit-schedules]

Equivalent aware RFC 3339 representations should be compared as instants: e.g. `2026-10-31T04:00:00-05:00` equals `09:00:00Z`. `09:00` local time in America/Chicago is **not** `09:00Z`. Exactness also means no unnoticed fractional offset. The local `parse_time` uses `unix_timestamp()` and formats seconds, so `09:00:00.900000Z` silently becomes `09:00:00Z`. A v2 contract must not treat that lossy serialization alone as evidence the agent requested the exact instant. Preserve precision or explicitly validate the request-to-effect normalization against a documented supported precision; hosted rounding behavior was not established. **[F/I/U]**[^f-util][^p-schedules]

### Creation, identity, preservation, and cancellation

Current REST documents latest-created override precedence and still accepts a deprecated single-override request shape. The **native pin** exposes only the `overrides` array shape. The server allocates override IDs; local storage separately allocates `PSPLTPR/<n>` keys. There is no reason to require an illustrative generated ID, a `/1` allocation, reference order or one HTTP request. Bind every new override to its request's schedule and actual persisted ID; compare user/time/effective coverage. New rows need not expose `schedule_id` or `created_by` in MCP. **[D/P/F/I]**[^d-create-override][^p-schedules][^f-schedules][^f-util]

At least two native-expressible representations are business-equivalent in the current fixture:

- One override for Jordan for the whole interval.
- Two adjacent overrides, for example 09:00–13:00 and 13:00–17:00 UTC, on the same schedule for the same Jordan. The union is exactly the requested interval and the final routing is identical.

These are proposals for valid-alternative controls, not executed controls. A coverage-union predicate should admit either while rejecting extra coverage, gaps, a later conflicting override, or a second schedule. Retain row identities/history so semantic coverage does not excuse deleting existing records or appending unrelated overrides. Merely finding one correct row is insufficient. **[I]**[^f-scenarios][^f-schedules][^p-schedules]

The existing Payments override `PQ7OV01` must remain unchanged, as must all eight baseline rotation definitions and the unrelated v3 Security schedule where present. Preserve old records by actual identity/parent association, not only their rendered effect today; changing an old inactive override would still violate scope/history. Rust `v2_segments` derives precedence from enumeration of stored child entries, which is not an explicit persisted creation-order field. A future many-override fixture must verify storage ordering matches documented last-created priority rather than assume lexicographic `/10` versus `/2` ordering is chronological. **[D/F/I/U]**[^f-data][^f-schedules][^d-create-override]

Cancellation is a **product/API fidelity issue, not an alternative currently reachable native plan**:

- Current documented DELETE cannot remove a past override; deleting an in-progress one truncates it at now and returns HTTP 200; deleting a future one returns 204. Product docs say the regular schedule then resumes. **[D]**[^d-delete-override][^d-edit-schedules]
- Local `delete_override` unconditionally removes a found row and returns 204, even if active or past. It does not retain the elapsed portion. **[F]**[^f-schedules]
- The captured connector has no classic delete/list-override tool. Its v3 override CRUD tools address a different resource system. Do not use a privileged raw HTTP call or v3 cancellation as a claimed native alternative; do not broaden the cover request into cancel/recreate behavior on existing overrides. **[P/F/I]**[^f-snapshot][^p-schedules][^d-v3]

### Observable verification and v2 permissions

Native `get_schedule` drops rendered final coverage and override subschedules; a requirement to read those fields through that tool is inaccessible. `list_oncalls(schedule_ids=["PSPLTPR"], since=..., until=..., earliest=false)` is reachable and can inspect winning coverage because this schedule is attached to `PEPPLAT`. Include boundary-adjacent instants/windows to detect extension before 09:00 and at/after 17:00, not merely one midpoint. `get_schedule` can independently inspect unchanged base layers. An offline evaluator can use authoritative initial/final state plus interval relations; it should not require these particular read calls or infer the agent observed them merely from final state. **[P/F/I]**[^p-schedules][^p-oncalls][^f-schedules]

Proposed preservation scope: append only the needed target-parent override records with known replacement user/exact interval union, generated identities and permitted representation fields; no edits/removals of any baseline record; preserve all schedules/layers and all other service state. No creator field is stored by `create_one_override`; the override `user` is the **replacement**, not the actor. If actor provenance is required, use connector/session/request evidence, not an invented `override.created_by` or a requirement that Jordan be the token owner. **[F/I]**[^f-schedules][^f-util][^p-context]

## 2. Reassign the orders replica-2 incident to current primary DBRE

### Required business outcome and exact note

At business time, replace the full assignment set of `Q3K8ORDBLAG2RX` with exactly `{PDNGYN2}`. Array index/order is not the identity contract: there must be no retained Alex, no second Sam, and no extra copied assignee. Keep this target `triggered`, with no acknowledgement or resolution anywhere during the task, and leave every other incident unchanged. Required communication is a newly persisted target-incident note containing the user's **whole quoted message**:

> Handing off to DBRE: replica-2 disk is at 100%, needs a volume resize.

The task authorizes recording the disk/resize handoff, **not performing a volume resize**. The legacy requirement that the first note merely contains `replica-2` does not establish this message. Exact quoted content is the safest initial accepted wording; if paraphrases are later admitted, review the complete message for all four propositions: handoff to DBRE, replica-2, disk at 100%, and required volume resize. Contradictory text or text only in the final assistant reply cannot substitute for the incident note. **[F/I]**[^f-scenarios][^f-data]

Native request:

```json
{
  "manage_request": {
    "incident_ids": ["Q3K8ORDBLAG2RX"],
    "assignement": {"id": "PDNGYN2"}
  }
}
```

This replaces the assignment list. It is not `add_responders`, an escalation-level action, an acknowledgement, a second new incident, or an update to a service/policy. `list_oncalls(escalation_policy_ids=["PEPDBRE"])` returns Dana at level 1 and Sam at level 2. Using `service_ids=["PSVORDB"]` instead resolves the original Platform policy and is therefore the wrong way to discover the requested **Database Reliability** primary in this fixture. DBRE membership alone also does not identify the primary. **[P/F/I]**[^p-incidents][^p-oncalls][^f-data]

### Assignment semantics and legitimate side effects

Official REST accepts an array of assignments and a replacement escalation policy; product docs distinguish direct-user, policy, and escalation-level reassignment. A directly assigned individual is the notified responder and the policy no longer drives subsequent escalation. Reassignment therefore can have notification side effects; “don't acknowledge or resolve” does not mean “do not notify the person I asked to take over.” No documented or modeled guarantee here establishes actual SMS/email delivery, exact notification count, or responder acknowledgement. **[D/P/F/U]**[^d-reassign][^d-notifications][^p-api-schema][^f-incidents]

The pinned native tool accepts only one `UserReference`; extra REST-looking fields do not enable multi-user or policy assignment. `escalation_level:1` operates on the incident's **current** Platform policy; it does not mean “primary of whichever team was mentioned.” Multiple users, replacement-policy semantics, and a single-incident REST PUT are genuine vendor APIs but not additional tools in this pin. **[P/F/I]**[^p-models][^p-incidents][^f-snapshot][^p-api-schema]

Local direct assignment updates `assignments` with server business timestamps, sets `assigned_via="direct_assignment"`, updates `updated_at`, and appends `assign_log_entry` with actual assignees/actor. It preserves service `PSVORDB`, team Platform, and escalation policy `PEPPLAT`. It does not explicitly reset every old escalation field. If an acknowledged incident were reassigned, local `assign_to` retriggers it and clears acknowledgements; the target starts triggered, so that transition is neither needed nor permission to acknowledge it first. Preserve target urgency, priority, content and unrelated lifecycle fields, while admitting the source-supported assignment/timestamp/log effects. **[F/I]**[^f-incidents][^f-records][^f-data]

**Evaluator objection:** requiring `incident.escalation_policy.id == "PEPDBRE"` or `incident.teams == DBRE` would fail the supported reference behavior; accepting Dana anywhere in a multi-user assignment list would admit incomplete handoff. Preserve the old routing/configuration fields under the supported direct-assignment branch and test the singleton assignment **set**. A future policy-reassignment alternative changes future escalation behavior and needs explicit scope review and a reachable native tool, not silent reinterpretation of “reassign to whoever is primary.” **[D/P/F/I]**[^d-reassign][^f-incidents][^f-scenarios]

### Actor, note provenance, and sequence freedom

For this fixture, the note's creator and assignment timeline actor are Alex, derived from the initialized context/account; Dana is the destination assignee. Bind the note to its incident through actual parent/request association, to its allocated ID, to full content and to its stored user. The normalized note does not contain its parent ID; the normalized incident does not expose its assignment-log actor or retained escalation policy. Offline relations may inspect authoritative state/history, but do not require those omitted fields to appear in a native tool answer. **[P/F/I]**[^p-models][^p-context][^f-incidents][^f-data]

Both “reassign, then note” and “note, then reassign” can fulfill this request. There is no requested cross-operation prerequisite or transaction between them. Extra directory/policy/alert reads and readback retries are legitimate. What matters is that both required effects persist on the same correct incident, no acknowledgement/resolution occurs, and no other state is altered. Do not require the reference's four calls, exact discovery ordering, an initial `get_user_data`, or note key `Q3K8ORDBLAG2RX/1`. **[F/I]**[^f-scenarios][^p-incidents]

## 3. Resolve production checkout and record rollback

### Required business outcome and communication

Resolve incident `Q7CHKAP15PCT1D` #4810 on production service `PSVCHKA`, and persist a note conveying that **rollback of release `2026.10.29.3` resolved this incident**. Reference wording is:

> Resolved by rolling back release 2026.10.29.3.

The user's remediation statement is supplied context; the agent is not asked to execute a rollback, prove baseline metrics, reopen an old incident, post to a public status page, change the staging load test, or change service configuration. If admitting alternative note wording, review whole-message meaning, including the rollback direction and resolution claim, not just presence of the release number. A future text adjudication boundary must remain explicit: a finite accepted/rejected wording set is not unrestricted semantic understanding. **[F/I]**[^f-scenarios][^f-data]

Keep staging `Q8CHKSTG5PCT2F` open/triggered, checkout-web `Q9CHKWEB5XX3VK` acknowledged, and every other incident, note and configuration record unchanged. Same title, same Commerce policy, same initial Theo assignee, and a higher/lower error rate do not distinguish production from staging; the service relationship does. The native incident model omits its fixture body, while `list_services`/`get_service` and alert/change-event tools expose useful service/environment/release context. Do not require an inaccessible incident body or a non-existent environment field. **[P/F/I]**[^p-models][^p-services][^p-alerts][^f-data]

### Resolution lifecycle and preservation

Supported `manage_incidents(status="resolved")` performs the bulk REST update and returns normalized incident records. Require an actual persisted final `status="resolved"` on the target, not a successful HTTP request, `acknowledged`, empty `response`, a note saying “resolved,” or a request that named the wrong incident. A direct resolution from the target's triggered state is supported locally; prior acknowledgement is not required. **[P/F/I]**[^p-incidents][^f-incidents]

Local automatic effects:

- Target `resolved_at`, `last_status_change_at`, `updated_at` use the business clock; `last_status_change_by` is Lena.
- Target `assignments` and `pending_actions` become empty.
- Both target alerts `PALR001` and `PALR002` become resolved and acquire `resolved_at`; staging alert `PALR003` and all unrelated alerts stay unchanged.
- One resolution action generates a `resolve_log_entry` linked to this incident/service/team and actor; adding the note generates `annotate_log_entry` and can independently update the target's `updated_at`.

These are **[F]** implementation effects, not evidence that real notification delivery, subscriber status automation, or all production incident field-clearing behavior is reproduced. Preserve old acknowledgements/history unless a separately admitted operation genuinely changes them. Cause-scope automatic child/log effects to the correct target action; do not authorize arbitrary direct edits to alert status, history, or all incident timestamps merely because the final status is right. **[F/I/U]**[^f-incidents][^f-records][^f-data]

The resolve fixture also has a public status page, an existing investigating post, and two webhook subscription configurations. Their presence does not create a new customer-communication obligation: none of the three single-service instructions requests a status-page post, subscriber message, or Slack/customer email. Preserve those configuration/content records; do not fabricate a “customer notified” result or require delivery evidence unavailable in this model. Runtime webhook dispatch, if any, is outside inspected guarantees. **[F/I/U]**[^f-data][^f-scenarios]

Current REST's `resolution` field writes text into the Resolve log entry, not directly into the incident-notes UI; it is not a substitute for the explicitly requested note. This native management tool does not expose `resolution` anyway. Current API docs also allow reopening resolved incidents, whereas the local handler rejects updates to resolved incidents' status/assignment and this native schema does not allow `status:"triggered"`. “Terminal” here means **the requested final resolved outcome**, not an assertion of irreversible vendor lifecycle. **[D/P/F/I]**[^p-api-schema][^p-models][^f-incidents]

### Native alternatives and optional intermediate acknowledgement

Either “add note, then resolve” (the reference) or “resolve, then add note” can succeed: native note creation and the local handler allow a note on a resolved incident. Separate get/list/alert/directory reads and status/note readbacks are legitimate. Do not force the reference order or a fixed note index/ID. **[P/F/I]**[^p-incidents][^f-incidents][^f-scenarios]

Unlike the reassignment instruction, the resolve instruction does **not** explicitly prohibit acknowledging this target. A target-only acknowledge-then-resolve sequence can be a legitimate alternative; rejecting it merely for an extra call silently adds a constraint. If admitted, explicitly permit its bounded acknowledgement/timeline effects and any documented automatic ownership effects, while still requiring final resolution and preserving every other incident. Current product docs say acknowledgement can claim ownership; the local acknowledgement path's exact assignment behavior is not a complete vendor model. Direct resolve remains the simpler supported plan. Do not confuse this possible target-only intermediate step with accepting acknowledgement as completion, or with permission to acknowledge staging. **[D/P/F/I/U]**[^d-notification-rules][^d-incidents][^f-incidents][^f-scenarios]

## Retries, idempotency, partial commits, and honest completion

### What the pinned client actually retries

`pagerduty==5.3.0` initializes an empty per-status retry map, `max_network_attempts=3` and `max_http_attempts=10`. Its loop retries caught Requests/urllib3 connection/HTTP exceptions up to the network retry boundary (potentially an initial attempt plus three retries), and HTTP 429 loops with increasing cooldown under default logic. With an empty retry map there is **no blanket automatic retry for every 5xx response**; 401 raises immediately, and other non-success statuses are eventually rejected by `rget/rpost/rput` success validation. Retry behavior is not limited to idempotent HTTP methods. No note/override idempotency key is supplied by the inspected MCP helpers. **[P]**[^p-sdk]

Consequences and recommended adjudication:

| Situation | Required distinction |
| --- | --- |
| POST committed, response lost, SDK retries | Another note/override may be created. A transport retry count is not a logical operation count; neither exactly-once behavior nor a single allocated ID is guaranteed. |
| PUT resolution committed, caller retries after loss | Local subsequent resolution may report “Incident Already Resolved.” An error on that retry does not erase the first commit; inspect actual state/receipts before claiming failure or issuing more changes. |
| Note or management update commits, then Pydantic/output validation fails | The tool can return `isError:true` **after** mutation. An error does not prove rollback. |
| Several `manage_request` actions, later action fails | Earlier HTTP actions may remain committed; only the last response would have been returned on success. Do not infer transactionality from one MCP call. |
| Classic override batch has one valid and one invalid element | The valid element can persist while the outer response is 201 and one element is 400. Check every requested element; report partial completion accurately. |
| All required writes commit, optional readback fails | Outcome can still be complete. Distinguish persisted business success from “verified by a subsequent successful read”; do not force a redundant write or automatically fail the task solely for that read error. |
| Readback returns empty | `list_incident_notes` returns an empty model on an unexpected non-list response; user listing and capped lists can also be incomplete. Empty is not universal proof no write committed. |

These are source-derived **[P/F/I]** cases, not executed fault injections. **[P/F/I]**[^p-incidents][^p-schedules][^p-sdk][^p-fastmcp][^f-incidents][^f-schedules]

The local bulk incident handler validates/stages the whole body before committing that single HTTP request; classic override creation instead commits valid elements independently. This is authored implementation behavior, not proof of the live vendor's transactional boundary. Separate note and incident writes have no transaction at all. **[F/U]**[^f-incidents][^f-schedules]

Migration should distinguish **required effect success**, **attempted operation/result classification**, and **truthful completion communication**. Do not require every exploratory/retry call to succeed, and do not call the overall required write successful solely because one envelope succeeded. A complete result contract must reject a failed required suboperation as evidence of completion; an offline outcome contract may independently establish completion from committed state after a lost/error response. **[I]**

Duplicates need an explicit policy, not hidden fixed cardinality. One logical requested note/coverage change may have several identical persisted copies after an ambiguous POST retry. Decide whether request-lineage-backed identical duplicates are admitted retry equivalence or a flagged side-effect; do not silently bless arbitrary note spam, or reject every legitimate retry because it allocated another ID. For split coverage, use interval-union semantics rather than a one-row rule regardless of retry policy. This note does not claim an existing evaluator can prove response-loss lineage or exactly-once intent. **[I/U]**

If the agent narrates completion, it must distinguish “submitted,” “persisted,” “verified,” “partially completed,” and “could not confirm” according to evidence. The three instructions do not mandate a special answer file or extra customer message. Required incident-note communication remains required regardless of optional final narration; optional narration must not invent delivery, DB resize, rollback execution, responder acceptance or successful cancellation. **[F/I]**[^f-scenarios]

## Exact evaluator counterexamples and future native controls

All cases here are **static proposed controls [I]**, not run results, qualification or claims of observed pass rates. “Legacy gap” means the inspected declared predicates leave the behavior unchecked; it is not a claim that an arbitrary unexecuted harness path has been demonstrated.

| Case | Exact operation/result or state distinction | Evaluator objection / proposed outcome |
| --- | --- | --- |
| Correct spelling, no actual assignment | `manage_request={"incident_ids":["Q3K8ORDBLAG2RX"],"assignment":{"id":"PDNGYN2"}}` | Pin ignores unknown `assignment`, returns empty `response`; success envelope must not establish handoff. Use supported `assignement`. |
| Transient forbidden acknowledgement masked by reassignment | `manage_request={"incident_ids":["Q3K8ORDBLAG2RX"],"status":"acknowledged","assignement":{"id":"PDNGYN2"}}`, then correct note | Native helper ACKs then reassigns, leaving target triggered/Dana. Legacy target-final predicates and its “no resolved PUT” guard do not reject the acknowledgement. Audited history must reject it under the explicit reassign instruction. |
| Unrequested urgency change | `manage_request={"incident_ids":["Q3K8ORDBLAG2RX"],"urgency":"low","assignement":{"id":"PDNGYN2"}}`, then correct note | Legacy checks assignee but not urgency. v2 preserves high urgency and rejects the unrelated change. |
| Extra incident in a bulk resolution | `manage_request={"incident_ids":["Q7CHKAP15PCT1D","Q0AP1GW5XX9LMZ"],"status":"resolved"}`, plus correct production note | Legacy specifically guards staging/web but does not protect gateway incident #4809. v2 rejects extra incident and associated alert/log effects even though target resolves. |
| Weak handoff note | Persist only `replica-2` on target, with otherwise correct reassignment | Passes legacy substring content predicate; fails complete quoted-message contract. |
| Contradictory rollback note | Persist `Release 2026.10.29.3 was not rolled back; this incident is still broken.` and resolve target | Contains legacy release token but contradicts required communication. Reject whole-message meaning, not just token matches. |
| Dana plus old assignee | Final assignment set `{PDNGYN2,PA1EXKM}` or `{PDNGYN2,PS4MOKF}` | Reject. This is a REST/state-level semantics control, not a plan expressible by the current single-user native assignment field. Label that distinction rather than inventing a native multi-user tool. |
| Exact split shift | Two native overrides for Jordan on `PSPLTPR`: 09:00–13:00 and 13:00–17:00 on October 31 UTC | Business-valid alternative; legacy `/PSPLTPR~12 exists=false` rejects it. v2 interval union should admit it without changing any rotation. |
| Extra Payments override | After correct target override, create another native override on `PSPAYPR` for any different coverage window/user | Existing checks do not guard `PSPAYPR/2`. v2 rejects this unrelated schedule effect and preserves `PQ7OV01`. |
| Fractional near miss | Native target override starts `2026-10-31T09:00:00.900000Z`, ends `2026-10-31T17:00:00.900000Z` | Rust truncates both to expected whole seconds. Exact-window evaluation must detect the lossy near miss or state the verified precision boundary; do not call it a proven vendor-equivalent request. |
| Partial batch behind HTTP success | Submit correct Jordan target override plus another with equal start/end; REST array has one 201 and one 400 while MCP `isError:false` | Native input model does not enforce interval ordering. Reject the failed item as a successful creation; classify partial commit and completion claim separately. No claim that all committed target success must be undone. |
| Correct user at wrong level/policy | Sam from `PEPDBRE` level 2, or Sam from `PEPDBST` level 1 | Reject as requested-primary handoff. Their DBRE membership/name relevance is insufficient. |
| Acknowledgement mistaken for resolution | Native status change to `acknowledged` plus correct-looking rollback note, no resolution | Reject missing final resolved outcome even if the user-facing message says “resolved.” |
| Different legitimate note/status ordering | Resolve then add correct note, or note then resolve; reassign/note in either order | Admit when all required effects and preservation hold; don't encode reference call order. |
| Readback failure after committed success | Correct writes persist; subsequent `get_incident` or schedule/on-call read errors | Adjudicate with authoritative commit evidence. Do not infer no mutation, require a fixed retry, or punish extra read attempts. |
| Active/past classic override deletion | API-level delete must truncate active history / reject past removal, while local source erases row | Fidelity/control backlog; not a reachable native classic-override delete in this pin and not authorized by the cover task. |
| State restored after a forbidden write | Acknowledgement or unrelated mutation occurs and is later compensated where a reachable operation permits it | Initial/final equality alone is insufficient. Audit committed histories and request-caused effects; do not assume a restoration API exists for every field. |

Counterexample dependencies: the three declarations, the pinned native helper implementations and schema, local transition/override code, and current first-party API/product docs. **[F/P/D/I]**[^f-scenarios][^p-incidents][^p-models][^p-schedules][^f-incidents][^f-schedules][^f-util][^d-reassign][^d-delete-override]

## Actionable v2 migration sequence

1. **Retain all three instructions and their identities/clocks.** Bind the inspected sources/datasets/catalog/connector selection to a new reviewed contract revision. Do not transplant resolve's actor into the other tasks or normalize everything to Alex. Keep runtime/image admission separate from source migration. **[F/I]**[^f-scenarios][^f-data][^f-contracts]
2. **Repair/declare modeled fidelity boundaries before permissive grading.** Decide timestamp precision, classic cancellation/history semantics, old escalation-field behavior and notification scope. The native pin does not expose classic cancellation or policy reassignment; mark those limitations rather than adding nonexistent alternative tool paths. Resolve current-doc reopening differences without changing this task's requested final state. **[D/P/F/I]**[^f-util][^f-schedules][^f-incidents][^p-api-schema][^f-snapshot]
3. **Use `version=2`, audited history, immutable initial state and narrow permissions.** Cover appends target overrides only; reassign changes target assignment/assignment metadata and appends its note/logs only; resolve admits target lifecycle plus automatic target-alert/note/log effects. Preserve every old unrelated record and initial history entry, including existing override/note IDs and status-page/webhook records. Scope permissions to actual operation causes/record relationships instead of path-wide write exemptions. **[F/I]**[^f-contracts][^f-incidents][^f-schedules][^f-records][^f-data]
4. **Bind outcomes by business relationships.** Incident→service→environment/team, DBRE policy→level-1 schedule→effective current user, cover account→Platform primary and replacement directory user, note→parent incident→actor, override→parent schedule→time/user. Use singleton **sets** for reassignment and interval-union/effective-winner checks for cover; do not hard-code generated `/1` rows, array order, sample IDs, reference call counts or discovery sequencing. **[F/I]**[^f-data][^f-scenarios][^f-contracts]
5. **Add complete reviewed note and optional narration contracts.** Preserve the exact quoted handoff; require meaningful rollback-resolution wording rather than a release token. Record accepted paraphrases and meaningful negative/negated examples, with unresolved adjudication for unreviewed text rather than pretending substring matching is semantic correctness. Do not infer external customer-communication obligations from unrelated fixtures. **[F/I]**[^f-scenarios][^f-data]
6. **Add request-bound native result contracts.** Handle all JSON-RPC/SSE/error layers and typed versus `result`-wrapped output. Match returned incident sets to requested/actioned incidents; validate notes against request context; validate every override result's status/errors/identity/user/instants and cross-check text/structured representations. Treat completeness/degradation and empty results explicitly. Do not require unavailable model fields or equate a final-state pass with successful connector-result parsing. **[P/F/I]**[^p-fastmcp][^p-incidents][^p-schedules][^f-results]
7. **Later execute both alternatives and near misses through the actual pinned native path.** At minimum: note/status ordering, direct discovered lookup versus direct get, shifted timezone-equivalent timestamps, contiguous split coverage, explicit on-call windows, transient ACK violation, wrong DBRE level/staging, unrelated bulk incident, extra schedule, fractional near miss, nested override errors, ambiguous POST retry and committed-write/readback failure. Tag REST-only cancellation/multi-assignee/policy controls separately until reachable. This step is proposed future work, not performed here. **[I]**

## Mixed PagerDuty tasks: context only

The following full instructions/reference declarations were inspected without changes:

| Mixed task | Relevant lesson; not an imported requirement |
| --- | --- |
| `cloud/multi/aws-replica-lag-pagerduty-notion` | Same incident #4812 requires an actual CloudWatch alarm name **and ALARM time** in the incident note, plus a Notion postmortem. Its current note assertion only searches the alarm-name token. Neither the Notion work nor those time facts are requirements of single-service reassignment. |
| `ops/multi/checkout-saturation-scale-and-resolve` | Same production/staging identities but a different saturation/runbook task. Instruction genuinely requires remediation before note/resolve. Authored commentary correctly places runbook data on the alert because native `Incident` drops body. Do not copy its Kubernetes remediation or dependency order into the rollback-already-done task. |
| `ops/multi/pagerduty-sentry-github-issue` | Requires incident number, Sentry culprit/error, a GitHub issue and target incident note; forbids incident/Sentry status changes. Same familiar incident ID does not make its different instruction/clock interchangeable with resolve. |
| `ops/multi/crashloop-oncall-lookup` | Payments override means Priya, not raw scheduled Riya, is primary; Marcus is next level. Illustrates why effective on-calls and escalation levels matter. It is read-only and already declares v2/history. |
| `collab/multi/slack-pagerduty-dbre-oncall` | Thread corrects staging to production DBRE; level 1 Dana and level 2 Sam are both required in its answer. This does not authorize assigning the single-service incident to both people. It is read-only and already declares v2/history. |

All table entries are **[F/I]**. The mixed tasks reuse fixture identities under different instructions and sometimes different clocks; do not infer a single shared live incident universe or import their response-file obligations. **[F/I]**[^f-mixed]

## Remaining uncertainty and admitted-scope limits

- This investigation establishes **published-source/static-local compatibility hypotheses**, not live PagerDuty behavior, execution success, native-model qualification, production conformance, human admission, campaign/release admission or a new evaluator version. No controls, existing tests or reference solutions were executed.
- Real RBAC, token-specific visibility, OAuth scopes, API-region routing, notification channels/delivery, escalation timers/snapshots, webhook dispatch and concurrent external responders are not reproduced or verified merely because local requests use plausible headers. Current docs are evidence of documentation only.
- MCP release provenance is pinned to exact published bytes, not an unobserved Git commit or tools-image installation. A later native run must identify actual connector/dependency/image/configuration observations; a source pin cannot stand in for them.
- Actor provenance is bounded: fixture `/users/me` and cached `From` can establish the intended authored actor, but the local server does not cryptographically bind them to a real vendor identity. Do not add inaccessible actor fields to override results or reuse “token owner” as universal proof of human execution.
- The local simulator's dropped subsecond precision, unconditional classic override removal, simplified acknowledgement/assignment/escalation metadata and absence of genuine notifications are meaningful fidelity objections. They must be fixed, narrowly admitted or explicitly excluded before broader claims.
- Complete override-history enumeration/cancellation, multiple-user reassignment, replacement-policy reassignment, raw single-incident REST update and arbitrary HTTP requests are not supported native tools in this captured pin. A future alternative control must be expressible through the admitted surface, not administrator backdoors.
- Exactly-once POST semantics, retry lineage, hosted fractional timestamp rounding, vendor bulk atomicity and all old-field retention are not established. Avoid overfitting a result parser or preservation policy to one illustrative sample.
- Finite reviewed note wording and modeled interval/relationship predicates are bounded contracts. They do not prove general natural-language equivalence, that the agent acquired facts through a particular read path, or that pages actually reached Jordan/Dana.

## Sources

### Local authored sources

[^f-scenarios]: Full authored files: `scenarios/ops/pagerduty/cover-shift-override/instruction.md`, `scenarios/ops/pagerduty/cover-shift-override/scenario.toml`; `scenarios/ops/pagerduty/reassign-replication-lag/instruction.md`, `scenarios/ops/pagerduty/reassign-replication-lag/scenario.toml`; `scenarios/ops/pagerduty/resolve-prod-checkout/instruction.md`, `scenarios/ops/pagerduty/resolve-prod-checkout/scenario.toml`. Their generated copies/reference requests are under corresponding `tasks/ops__pagerduty__<slug>/instruction.md` and `solution/call-*.json`. The current declarations use specific `/1` note/override keys and a small set of required/forbidden HTTP shapes, not universal preservation.
[^f-data]: All JSON files under the three `scenarios/ops/pagerduty/<slug>/dataset/pagerduty/` directories; manifest hashes above bind the complete sets. Especially `account.json`, `users.json`, `teams.json`, `team_members.json`, `schedules.json`, `escalation_policies.json`, `services.json`, `incidents.json`, `alerts.json`, `overrides.json`, `notes.json`, `log_entries.json`, `change_events.json`. Cover's `schedules_v3.json` and resolve's `status_page*.json` / `webhook_subscriptions.json` were also inspected. Most common files are identical across all three; resolve's `account.user_id` is Lena rather than Alex.
[^f-catalog]: `catalog/services.toml:202`, PagerDuty service/endpoint through its `synthetic_replay_headers`; `crates/world/src/replay_headers.rs`; `crates/catalog/src/lib.rs` replay-header resolution.
[^f-lock]: `servers/python/pagerduty.txt`, complete requirements lock; package metadata pins/ranges independently obtained from PyPI version JSON.
[^f-snapshot]: Actual `servers/snapshots/pagerduty.json`: metadata, all 101 registered names, selected complete input/output schemas and annotations. There is no `list_schedule_overrides`, `delete_schedule_override`, generic request tool, or dedicated `update_incident` in this inventory.
[^f-environment]: Existing `tasks/ops__pagerduty__cover-shift-override/environment/docker-compose.yaml` and `tests/task-context.json`; these declare environment references only. No image was inspected or changed.
[^f-runtime]: `crates/catalog/src/bin/tools-supervisor.rs:1` and endpoint-launch loop; `images/tools/Dockerfile:48` onward, locked Python installation recipe; `crates/mcp-tools/src/bridge.rs:1`.
[^f-front]: `crates/mcp-tools/src/front.rs:80` onward, session validation/HTTP errors; `crates/mcp-tools/src/front.rs:132` onward, JSON/SSE responses; `crates/mcp-tools/src/bridge.rs:51`, tools/call streaming forwarding.
[^f-results]: Existing `tasks/ops__pagerduty__{cover-shift-override,reassign-replication-lag,resolve-prod-checkout}/solution/{result-contracts.json,response.jq,tool-result.jq,solve.sh}`. Port 7111 selects `mcp_envelope_v1`. `response.jq` checks framing/JSON-RPC ID/result types/isError; service-specific dispatch does not add a PagerDuty nested-result contract.
[^f-lib]: `crates/services/pagerduty/src/lib.rs`, classic/v3/directory/incident routing. HTTP routes are broader than the actual MCP tools.
[^f-incidents]: `crates/services/pagerduty/src/incidents.rs:210` list/default filters; `:304` `assign_to`; `:320` `apply_update`; `:501` `commit`; `:525` bulk staging; `:565` single update; `:718` note rendering/list/create. Existing `crates/services/pagerduty/tests/pagerduty.rs` sources cover From, resolution/alert clearing, reassignment retriggering and notes; they were not run.
[^f-schedules]: `crates/services/pagerduty/src/schedules.rs`: layer/override interval computation (`v2_segments`), schedule summary/rendering/get/list, `create_one_override`, partial `create_overrides`, time-bounded `list_overrides`, unconditional `delete_override`, `list_oncalls`. `schedules_v3.rs` is a separate shift-based implementation routed separately in `lib.rs`; native v3 names were inventoried, not treated as classic operations.
[^f-util]: `crates/services/pagerduty/src/util.rs:149` API errors; `:205` classic pagination; `:326` context construction; `:406` child keys/IDs; `:467` `me`; `:479` From lookup; `:487` `require_from`; `:645` lenient second-precision timestamp parser.
[^f-directory]: `crates/services/pagerduty/src/directory.rs:57` user normalization; `:94` `/users/me`; `:101` user query/team filtering; `:424` service rendering; `:466` service listing.
[^f-records]: `crates/services/pagerduty/src/records.rs:25`, `log_entry`, links actor/service/incident/team, allocates ID and stores business-clock time; subsequent timeline reads/filtering are read-only.
[^f-serve]: `crates/mock-core/src/serve.rs:95` onward, request evidence/caller-host access/clock stepping; `crates/mock-core/src/access.rs`, trusted caller resolution, not vendor authorization.
[^f-clock]: `crates/mock-core/src/clock.rs:4`, fixed default versus legacy transport; `parse_rfc3339_nanos` exists in core but PagerDuty's own `util::parse_time` is second-precision. World clock policy must be read from actual evidence, not inferred from call ordering.
[^f-contracts]: `docs/contracts.md`, contract versions, baseline preservation, audited histories, request/effect permissions and session/business-state distinction. Consult current source schema when implementing; no schema/contract was changed by this research.
[^f-mixed]: Full `instruction.md` and `scenario.toml` under `scenarios/cloud/multi/aws-replica-lag-pagerduty-notion/`, `scenarios/ops/multi/checkout-saturation-scale-and-resolve/`, `scenarios/ops/multi/pagerduty-sentry-github-issue/`, `scenarios/ops/multi/crashloop-oncall-lookup/`, `scenarios/collab/multi/slack-pagerduty-dbre-oncall/`.

### Exact distribution / first-party source pins

[^p-wheel]: PyPI immutable version metadata: https://pypi.org/pypi/pagerduty-mcp/1.1.0/json . Inspected wheel: https://files.pythonhosted.org/packages/20/52/341bdcae8c92feb2642884124087889edd9dc1aa75262392a31ed374873c/pagerduty_mcp-1.1.0-py3-none-any.whl . Byte hash in pin table. Package metadata identifies the first-party repository; it does not supply a release Git commit. Only archive contents were read.
[^p-mcp-repository]: First-party project https://github.com/PagerDuty/pagerduty-mcp-server ; inspected tag API https://api.github.com/repos/PagerDuty/pagerduty-mcp-server/tags?per_page=100 returned an empty list; `git/ref/tags/v1.1.0` and `git/ref/tags/1.1.0` returned 404 during retrieval. This limits Git attribution, not the immutable wheel attribution.
[^p-context]: Within the wheel in [^p-wheel]: `pagerduty_mcp/context/application_context_strategy.py` (`PagerdutyMCPClient`, `create_pd_client`, application-scoped context); `context/mcp_context.py` (`_init_user`, exception handling, cached user and From); `context/context_resolver.py`; `server.py` (write flag, annotations, instructions). Native server instructions advise caution/destructive confirmation; a benchmark reference plan is not a new human confirmation requirement or proof of such confirmation.
[^p-incidents]: Same wheel, `pagerduty_mcp/tools/incidents.py:27` `list_incidents`; `:82` `get_incident`; `:108` request builder; `:127` direct reassignment and wall clock; `:143` status; `:166` multi-action management; `:227` note-list empty fallback; `:248` note creation.
[^p-models]: Same wheel, `pagerduty_mcp/models/incidents.py` (`Incident`, `IncidentManageRequest`, `IncidentNote`, unused query models); `models/references.py` (`ReferenceBase`, computed reference type); `models/base.py` (`ListResponseModel`, `MAX_RESULTS=1000`, computed summary); `models/schedules.py` (Override, Schedule, ScheduleSummary, SourceStatus); `models/oncalls.py`. Default extra-field handling is not `extra="forbid"` on the management request, unlike selected query models.
[^p-schedules]: Same wheel, `pagerduty_mcp/tools/schedules.py:39` classic summaries; `:50` v3 enrichment; `:95` independent-source merged listing; `:141` classic get; `:159` discriminated get/fallback; `:214` classic override transformation; `:236` schedule users. Inspect with the matching `models/schedules.py`; do not substitute GitHub main or v3 schemas.
[^p-users]: Same wheel, `pagerduty_mcp/tools/users.py:8` independent `/users/me` read; `:18` one-page user list with `teams_ids`.
[^p-oncalls]: Same wheel, `pagerduty_mcp/tools/oncalls.py`, actual registered flat inputs, service→policy lookup, `earliest=true` default, explicit windows and cap.
[^p-utils]: Same wheel, `pagerduty_mcp/utils.py` `paginate`; `models/base.py` `ListResponseModel` truncation warning behavior.
[^p-services]: Same wheel, `pagerduty_mcp/tools/services.py`, paginated list/get; `tools/escalation_policies.py`, list/get query alternatives.
[^p-alerts]: Same wheel, `pagerduty_mcp/tools/alerts.py:22`, paginated incident-alert retrieval with exposed offset; `models/alerts.py`, retained alert body/details and extra fields.
[^p-sdk]: PyPI version metadata https://pypi.org/pypi/pagerduty/5.3.0/json ; inspected wheel https://files.pythonhosted.org/packages/0d/81/edf610ffc8a9d4449f76ab4ae5fe8489df745086248d0c8f323998665b2e/pagerduty-5.3.0-py3-none-any.whl . `pagerduty/rest_api_v2_client.py:399` special override wrapping; `:518` token constructor/Accept; `:866` list query normalization. `rest_api_v2_base_client.py:299` unwrap, `:386` wrapped entities, `:437` token auth, `:487` page size, `:713` classic iteration. `api_client.py:64`/`:70`/`:90` retry defaults, `:200` headers, `:256` request/retry. `common.py:261` success check, `:293` JSON errors. First-party tag resolution: https://api.github.com/repos/PagerDuty/python-pagerduty/git/ref/tags/v5.3.0 ; source tree https://github.com/PagerDuty/python-pagerduty/tree/af123e1378ec205a89cab96fccf86f97a4d68295 . Current Context7 docs were consulted for orientation but not used instead of this exact pin.
[^p-fastmcp]: PyPI metadata https://pypi.org/pypi/mcp/1.30.0/json ; inspected wheel https://files.pythonhosted.org/packages/f5/f4/e58bc33317c92a0203664daaf00bf6f41166cc0149e5d6870a03f7cd004a/mcp-1.30.0-py3-none-any.whl . `mcp/server/fastmcp/tools/base.py:93` wraps tool exceptions; `fastmcp/server.py:359` enables result conversion; `utilities/func_metadata.py:117` model/result conversion, `:351` union/result wrapping, `:526` recursive content flattening; `server/lowlevel/server.py:473` error result, `:498` tool validation/normalization. These are source-derived envelopes, not an executed native trace.
[^p-api-schema]: First-party OpenAPI, immutable inspected URL https://raw.githubusercontent.com/PagerDuty/api-schema/9c84115d71fdeba32cb796ac711ea91e7bd7ad0e/reference/REST/openapiv3.json . Inspect `paths["/incidents"].put`, `paths["/incidents/{id}"].put`, `paths["/incidents/{id}/notes"].post`, `paths["/schedules/{id}/overrides"].post`, `paths["/schedules/{id}/overrides/{override_id}"].delete`, `/oncalls`, and component schemas/parameters. Bulk maximum 250, assignment arrays, policy field, reopening statuses, resolution-log note, per-item override failures, and cancellation distinctions are documented here. HTTP success example objects are illustrative and not strong evidence for every lifecycle field invariant.

### Current first-party API/product documentation

[^d-auth]: https://docs.pagerduty.com/developer/authentication and https://docs.pagerduty.com/developer/rest-api-overview — account/user token access, required authorization, HTTPS, JSON/version headers, From actor convention.
[^d-create-override]: https://docs.pagerduty.com/developer/api/reference/rest/schedules/create-schedule-override — latest-created priority, deprecated single override, schedules.write, HTTP 201 array with mixed per-item 201/400 and errors.
[^d-delete-override]: https://docs.pagerduty.com/developer/api/reference/rest/schedules/delete-schedule-override — past deletion disallowed, active truncation HTTP 200, future deletion HTTP 204.
[^d-edit-schedules]: https://docs.pagerduty.com/incident-management/schedules/edit-schedules — temporary override layer, override creation permissions, regular schedule resumption after removal, past-history protection, reverting layers does not revert overrides.
[^d-v3]: https://docs.pagerduty.com/developer/api/reference/rest/schedules_v3/create-overrides and https://docs.pagerduty.com/developer/api-concepts — classic versus shift-based system, v3 rotation/custom-shift/member semantics; v3 automations are not classic v2 substitutes.
[^d-reassign]: https://docs.pagerduty.com/incident-management/incidents/reassign-incidents — distinct user/policy/level reassignment paths; direct user notifies only that user and stops following the escalation policy; low-urgency restrictions.
[^d-notifications]: https://docs.pagerduty.com/incident-management/notifications/notification-troubleshooting/expected-notification-behavior and https://docs.pagerduty.com/incident-management/schedules/escalation-policies — notification/escalation behavior, schedule targets and policy snapshot considerations. They do not prove real delivery in the local service.
[^d-notification-rules]: https://docs.pagerduty.com/account-admin/user-profile/notification-rules — acknowledgement by an unassigned user reassigns ownership and affects who receives status-change notifications.
[^d-incidents]: https://docs.pagerduty.com/incident-management/incidents/overview — acknowledgement is not resolution; resolved lifecycle and current reopening description; notification/ownership overview.
[^d-list-incidents]: https://docs.pagerduty.com/developer/api/reference/rest/incidents/list-incidents — API page max 100, one-month/default/six-month search, service/team/assigned filters, resolved incidents not currently assigned. Native tool has a narrower argument set.
[^d-oncalls]: https://docs.pagerduty.com/developer/api/reference/rest/on-calls/list-on-calls — range overlaps, default current time, future horizon, policy/level/user `earliest` deduplication.
[^d-manage]: https://docs.pagerduty.com/developer/api/reference/rest/incidents/update-incidents and https://docs.pagerduty.com/developer/api/reference/rest/incidents/update-incident — genuine bulk/single REST operations, From/scopes, response forms; only bulk management is exposed by this native pin.
[^d-note]: https://docs.pagerduty.com/developer/api/reference/rest/incidents/create-incident-note — content/From, maximum 2000 notes, documented HTTP 200 and note wrapper. The Rust simulator instead returns 201 for note creation; the SDK accepts both as HTTP success. Do not require its simulator-specific status when validating native semantic success.
