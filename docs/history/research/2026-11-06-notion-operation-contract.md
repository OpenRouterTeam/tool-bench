# Notion operation contract — 2026-11-06

Scope: `collab/notion/{delay-web-launch,log-customer-escalation,update-current-runbook}`. Research only: official documentation and published source were read; no Notion workspace was contacted and no benchmark inference was run. **Documented** below means the public contract retrieved on November 6, 2026; **source-verified** means inspected code, not a live-service observation. Recommendations are explicitly distinguished from both.

## 1. Version and transport boundary

- **Local pin:** `@notionhq/notion-mcp-server@2.6.1`; catalog routing uses `BASE_URL={world}` and a synthetic token. The snapshot contains 24 tools, MCP protocol `2025-06-18`, advertised server version `1.0.0`, and recorded schema digest `f9544e7c84986529d5bc4a1c9357bbbb9c2f458133904075319f8fec7d716da9`. These are different version identifiers, not the REST API version. [L1][L2]
- **Published pin:** npm records publication at `2026-10-29T16:13:01.969Z` and `gitHead=cf16e5c20bfbfb98ecec1227f210ef0ae3abebe5`. The published tarball and that commit's OpenAPI file have identical SHA-256 `d33548174dfc6ebad012fc73b1483532c7887d8c97fd6deb62d56c742e96baba`; their HTTP client files also matched byte-for-byte. Package dependencies use ranges, so this package pin alone is not a complete transitive dependency lock. [N1][N2][M1][M2]
- **Source-verified headers:** ordinary operations default to `Notion-Version: 2025-09-03`; Markdown operations default to `2026-03-11`. Explicit globally configured headers override those defaults. The client's version switch comes from per-operation schema parameters, not the OpenAPI information banner. [M1][M2]
- **Source-verified response envelope:** successful tools return `content: [{type:"text", text:JSON.stringify(response.data)}]`. HTTP errors are also returned as JSON text, with an error status/payload; the handler does not set `isError`. Inspect the decoded payload and operation outcome, not merely the presence of an MCP text response. This envelope's `text` is unrelated to Notion rich-text segments. [M3]

Current documentation contains features absent from this pin. Treat it as dated contract evidence, not proof that every current field/behavior was supported by every pinned endpoint version. [M1][D5][D7]

## 2. Rich text: logical text is not rendered appearance

**Documented:** a rich-text response has `type: text|mention|equation`, its corresponding typed object, `annotations`, `plain_text`, and optional/link-related `href`. A text object's literal characters are in `text.content`; `text.link` holds the link target, not the displayed label. Annotations include `strikethrough`. Mention/equation responses are valid Notion rich text, not inherently unsupported input. Inaccessible references can expose placeholders such as `Untitled` or `@Anonymous`; equations carry an expression. [D1]

**Projection recommendation, derived from that structure:** concatenate segments' `plain_text` in array order, **without inserting separators**, for the unformatted text of one rich-text value. For validated text-only requests, concatenate `text.content` instead. Do not require the answer to occupy segment zero. This is not a browser renderer: concatenation loses link destinations, styles, mention identity, and mathematical layout; strikethrough characters remain in `plain_text`. Keep semantic/style validation separate from text matching. [D1][L3]

**Unsupported-handler policy:** a text-only checker should explicitly reject or report an unsupported variant, or use its supplied response `plain_text` for a deliberately lossy read projection while retaining the typed payload for preservation checks. It must not silently drop mentions/equations, fabricate their labels, or rewrite them as ordinary text during an unrelated edit. This is a recommended checker policy, not an API restriction. [L3][D4]

**Local choices:** `model::rich_text` fills response annotations, derives text `plain_text`/`href`, and also accepts strings, individual objects, and `null` as convenience inputs. Its mention handling covers only some kinds and synthesizes names/placeholders; equations use their expression as `plain_text`. `plain()` joins response `plain_text` with no delimiter. Conversely, the pinned reusable `richTextRequest` schema only declares text and link, disallows extra fields, and does not declare annotations, mention, or equation. That restriction applies where this helper is referenced, not to every generic properties/block payload. Neither local convenience parsing nor the narrow helper defines the full Notion API. [L3][L2][M1]

## 3. Page properties and parent identity

### Documented request/response rules

| Concern | Operational contract |
| --- | --- |
| Addressing | Page writes accept property **names or IDs**, matching the parent data source's schema. IDs survive renames; returned IDs can be URL-encoded. Do not require human-readable property keys in the request. [D2][D3] |
| Omission | Omitted properties keep their existing values on update. Submitted arrays replace their whole property value; they do not append. Preserve existing array members when the task requires them. [D3] |
| Clearing | Use `[]` for title/rich text/people and other array-valued properties; `null` for select/date and nullable scalar values; `false` for checkbox. Current docs say `status:null` resets to a default option if configured. None is equivalent to omission. These are typed values, e.g. `{"date":null}`, not data-source schema deletion. [D2] |
| People | Write user IDs. Responses can contain partial user objects; names/emails are not guaranteed. Compare identity, not serialized display details. [D2] |
| Select/status | Both accept an existing option's ID or name. A new select name may extend the schema when authorized; commas are disallowed. A page write does **not** create a new status option. [D2] |
| Result | Create returns a new page; update returns the updated page. Standalone child pages only support title properties; data-source rows follow their source schema. A page response may truncate large people/reference values; retrieve paginated property items before a preservation-sensitive rewrite. [D3][D5][D12] |

**Versioned parent rule:** API `2025-09-03` separates database containers from data sources; a database can have several sources. The official migration directs page creation to an explicit `data_source_id` and discovery through the database's `data_sources`. A database ID is not a substitute for selecting an arbitrary source; old database-parent workflows fail once the database has multiple sources. [D6]

**Pinned schema mismatch and usable alternative:** the helper named `dataSourceIdParentRequest` actually declares `database_id` and `type:"database_id"`, not `data_source_id`. `API-post-page.parent` also accepts a string, and the proxy recursively decodes JSON-encoded objects/arrays. Thus a JSON-string parent containing the explicit data-source shape is a schema-advertised route through this pin; do not grade exclusively for the oracle's database-parent object. Direct structured data-source parents do not match the declared object alternatives, even though the mock accepts them. Host-specific acceptance remains untested. [L2][M1][M3][S2]

**Local choices/gaps:** properties resolve by schema name or exact stored ID; updates replace only supplied properties. Select creation mutates the source schema; unknown statuses and `status:null` are rejected, unlike the current documented reset rule. Missing stored properties are synthesized as empty/null values in `page_out`; that is response shaping, not evidence of persisted clearing/default assignment. `resolve_parent` maps a database parent to its sole source and rejects zero/multiple sources. These choices are not a complete versioned reproduction of Notion. [L4]

## 4. Block updates versus targeted Markdown

### Documented operation boundary

- `PATCH /v1/blocks/{block_id}` updates that block and returns its updated object. Supplied fields are replaced entirely; omitted fields are unchanged. It cannot directly update children. The REST paragraph payload is `{"paragraph":{"rich_text":[...]}}`, with optional string `type:"paragraph"`, **not** an object-valued `type` wrapper. A minimal update of the existing block is the straightforward identity-preserving operation; it is not a request to recreate/reorder siblings. [D4][D8]
- `PATCH /v1/pages/{page_id}/markdown` with `type:"update_content"` takes `content_updates:[{old_str,new_str,...}]`; matching is case-sensitive. Duplicate matches require `replace_all_matches:true`. `replace_content` replaces the entire page; legacy insertion/range commands remain documented. Child-page/database deletion is guarded unless explicitly allowed, but that guard is **not** blanket protection for every ordinary block. [D7][D9]
- Synchronous Markdown updates return a `page_markdown` object with resulting Markdown, `truncated`, and `unknown_block_ids`. Current docs additionally describe warnings and optional asynchronous responses. The pinned tool does not advertise `markdown_version` or `allow_async`; do not silently substitute current hosted-MCP tool names or assume these newer controls are available. [D7][L2][M1]

### Source-verified differences

The pinned `API-update-a-block` advertises `type:{paragraph:{...}}` and `archived.default:true`. Its proxy/HTTP client forwards body arguments without an explicit wrapper-flattening step or body-default insertion. The mock deliberately accepts both wrapper and REST forms. **The oracle wrapper working against the mock does not establish upstream REST validity.** Undeclared top-level fields are not explicitly forbidden by this tool's root schema, but actual host handling has not been exercised. Likewise, JSON Schema `default` alone does not fill absent request fields; do not infer that omitting `archived` archives the block. [L2][L5][M1][M2][M3][J1][S3]

Local direct block updates replace rich text while retaining the block ID, parent/order and unrelated fields. Local Markdown has a plain-text single-block fast path; matching flat block shapes can also reuse IDs. Structural fallback removes/rebuilds non-page blocks. It is not a general lossless Markdown round-trip, and it picks the first match without the documented ambiguity rejection when `replace_all_matches` is false. The source comment asserting Notion-like ID retention is not independent evidence of Notion's guarantee. [L5]

### Metadata and atomicity: do not overstate

**Documented:** page/block `last_edited_time` and `last_edited_by` describe last edits. The reviewed references do not specify exact ancestor propagation, strict timestamp advancement, equality across objects, no-op behavior, or a compare-and-swap contract. The status-code reference's use of “transaction” in `409 conflict_error` is not a transaction API or an all-or-nothing batch guarantee. [D8][D10][D11]

**Important counterexample:** current Markdown docs explicitly allow matching content-changing edits to apply while unmatched edits appear in `warnings`; both Markdown versions are covered. Therefore a multi-edit request is not documented as “any miss rolls everything back.” The pinned schema's “up to 100 … applied in order” description does not add such a guarantee, nor establish when the newer warning behavior began. For other validation/failure classes, rollback semantics remain unspecified here. [D9][M1]

**Local choices:** `blocks::touch` walks block parents, stops at the first containing page (maximum 20 iterations), and writes that page's metadata; it does not touch every ancestor page. Both page/block paths use the current mock clock, so timestamp increases are not guaranteed. The service mutex serializes local execution, not a rollback transaction: helpers can mutate the source schema before later request validation fails. Allow operation-related edited metadata changes in preservation checks; do not require byte-identical ancestor pages or a particular timestamp delta. [L3][L4][L5][L6]

## 5. Access, trash, and locks

**Documented:** access to the resource and the relevant capability are separate prerequisites. Reading needs read-content capability; row creation needs insert-content; existing page/block/Markdown edits need update-content. User-information capability affects user discovery/details. A missing capability can yield `403`; `404` can conceal missing access as well as nonexistence. Search is title-based, paginated, and eventually indexed, not an authoritative inventory or a unique current-runbook selector. [D3][D4][D5][D7][D11][D13][D14]

Pages can be trashed/restored by explicitly setting `in_trash`; trashing is not permanent deletion. The trash guide says `archived` is an older-version alias removed from writes in `2026-03-11`, while the pinned ordinary operations use `2025-09-03`. Current block-update docs list trashed blocks among `404` cases. A page's `is_locked` controls UI editing and **does not prevent API updates**; do not equate locking with lost permission. [D3][D4][D15][M1]

**Local gaps:** the service accepts any nonempty bearer token; there is no per-resource sharing/capability enforcement in these handlers. Page-property/Markdown paths check the page's own trash flag, but direct block update checks the block's flag, not the entire ancestor chain, and returns a local `400` for a trashed block. It does not reject edits merely for `is_locked`. An accessible child-shaped mock record is not proof that editing beneath a trashed/inaccessible ancestor is valid upstream. For this task, never restore/edit a deleted copy just to make the update succeed. [L4][L5][L6][S3]

## 6. Scenario-specific grading counterexamples

These are recommendations derived from the user instructions and contracts, not claims about a live Notion run.

| Scenario | Oracle-shaped failure / stronger acceptance boundary |
| --- | --- |
| Delay launch | An update using property IDs and status-option ID is equivalent to the oracle's names. Accept either and verify only the intended Web 4.12 row changes to Delayed / `2026-11-10`; preserve other properties and other rows. Its current checks protect selected fields, not the whole unrelated state. [D2][D3][S1][I1] |
| Escalation | Two text segments spelling the requested Summary/Customer are legitimate; `/title/0/plain_text` and `/rich_text/0/plain_text` can reject them. Compare complete ordered text and separately retain formatting/typed-content policy. [L3][S2][I2] |
| Escalation | Bind the created row to the successful creation response/event and correct source, not solely the seed's first-page UUID. Still require exactly the intended new row with all six requested values; “today” is explicitly **October 29, 2026**, not this research date. The deterministic UUID is a local fixture choice. [D5][L3][S2][I2] |
| Escalation | Accept an explicit source parent, including the pin's JSON-string route, rather than requiring the database-parent oracle syntax. Match Owner by resolved user ID and select/status by schema option identity, not response object shape or display-name serialization. [D2][D6][M3][S2] |
| Runbook | A single block update or targeted Markdown replacement can implement the instruction. Do not require the oracle tool sequence. The scenario expressly imposes retaining this contact block's ID; that is a **local scenario requirement**, not a documented universal Markdown-ID guarantee. [D4][D7][S3][I3] |
| Runbook | Replacing the whole paragraph with only the new contact can satisfy the current substring assertion while deleting the prefix/suffix. Appending the new contact while retaining the old one can also pass. Require the intended replacement, preservation of surrounding content/annotations, and retention of unrelated blocks' content, order, IDs and older/deleted copies. The existing assertions do not fully establish this. [D4][L5][S3][I3] |
| All three | “No DELETE” does not prohibit destructive `PATCH` trash flags, rich-text erasure, or Markdown replacement. Check resulting preservation and write effects, not just HTTP verbs. Conversely, do not reject legitimate edited metadata as collateral damage. [D3][D4][D7][D15][L5][S1][S2][S3] |
| All three | A successful MCP text envelope is not proof of success; a Markdown response may report missed edits. Check decoded errors/warnings and the intended final state. Do not assume atomic rollback, strict timestamp increments, or all-ancestor timestamp updates. [M3][D9][D10][D11][L5] |

## 7. Remaining evidence gaps

- No live observations establish wrapper acceptance, host validation/default insertion, inherited-trash behavior, or permission-specific error combinations. Pinning a synthetic mock does not resolve these upstream questions. [M1][M2][M3][L6]
- Current docs' status-reset and Markdown-warning behavior are not dated per pinned API version. Record the documented/local disagreement; do not retrofit either behavior into the other without version-specific evidence. [D2][D9][L4][L5]
- The reviewed official documents give no blanket Markdown block-ID retention, exact ancestor-edited metadata, or general rollback/isolation guarantee. If benchmark requirements need stronger preservation, declare that local contract and test it independently of oracle serialization. [D4][D7][D8][D10][D11][S3]

## 8. Local runtime follow-up

After this research, evaluator revision 23 exercised the exact published client
against the isolated local world; no real Notion workspace was contacted.
The emitted snapshot permits `type` as either object **or string** and does not
forbid extra root fields. `/tmp/tool-bench-notion.oqED6u` verifies that a string
`type:"paragraph"` with top-level `paragraph.rich_text` actually reaches the
recorded HTTP request and completes the modeled task. Thus there is a tested
REST-shaped local route, not just the legacy wrapper route. This does not certify
the wrapper or every host against live Notion. [L2][D4]

`/tmp/tool-bench-notion.MPqCKB` also verifies the explicit data-source JSON-string
parent and retrieval of the actually returned page ID. The legacy evaluator's
block-ID-retention comment is a local modeling constraint, **not an explicit user
instruction** or a universal Notion guarantee. General replacement-ID editing
alternatives remain outside the reviewed local subset. See the revision-23
checkpoint in `../implementation-progress.md` for complete evidence and limits.

## Sources

Official pages were retrieved on **2026-11-06**; they are mutable references, not publication-date assertions.

[D1]: https://developers.notion.com/reference/rich-text
[D2]: https://developers.notion.com/reference/page-property-values
[D3]: https://developers.notion.com/reference/patch-page
[D4]: https://developers.notion.com/reference/update-a-block
[D5]: https://developers.notion.com/reference/post-page
[D6]: https://developers.notion.com/guides/get-started/upgrade-guide-2025-09-03
[D7]: https://developers.notion.com/reference/update-page-markdown
[D8]: https://developers.notion.com/reference/block
[D9]: https://developers.notion.com/guides/data-apis/working-with-markdown-content
[D10]: https://developers.notion.com/reference/page
[D11]: https://developers.notion.com/reference/status-codes
[D12]: https://developers.notion.com/reference/retrieve-a-page
[D13]: https://developers.notion.com/reference/capabilities
[D14]: https://developers.notion.com/reference/search-optimizations-and-limitations
[D15]: https://developers.notion.com/reference/trash-page
[J1]: https://json-schema.org/understanding-json-schema/reference/annotations
[N1]: https://registry.npmjs.org/@notionhq/notion-mcp-server/2.6.1
[N2]: https://registry.npmjs.org/@notionhq%2fnotion-mcp-server
[M1]: https://github.com/makenotion/notion-mcp-server/blob/cf16e5c20bfbfb98ecec1227f210ef0ae3abebe5/scripts/notion-openapi.json
[M2]: https://github.com/makenotion/notion-mcp-server/blob/cf16e5c20bfbfb98ecec1227f210ef0ae3abebe5/src/openapi-mcp-server/client/http-client.ts
[M3]: https://github.com/makenotion/notion-mcp-server/blob/cf16e5c20bfbfb98ecec1227f210ef0ae3abebe5/src/openapi-mcp-server/mcp/proxy.ts
[L1]: ../../catalog/services.toml#L498
[L2]: ../../servers/snapshots/notion.json#L1782
[L3]: ../../crates/services/notion/src/model.rs#L180
[L4]: ../../crates/services/notion/src/pages.rs#L67
[L5]: ../../crates/services/notion/src/blocks.rs#L305
[L6]: ../../crates/services/notion/src/lib.rs#L125
[S1]: ../../scenarios/collab/notion/delay-web-launch/scenario.toml#L1
[S2]: ../../scenarios/collab/notion/log-customer-escalation/scenario.toml#L1
[S3]: ../../scenarios/collab/notion/update-current-runbook/scenario.toml#L1
[I1]: ../../scenarios/collab/notion/delay-web-launch/instruction.md#L1
[I2]: ../../scenarios/collab/notion/log-customer-escalation/instruction.md#L1
[I3]: ../../scenarios/collab/notion/update-current-runbook/instruction.md#L1
