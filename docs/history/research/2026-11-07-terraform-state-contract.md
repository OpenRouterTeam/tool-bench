# Terraform/HCP state representation contract

Research date: **November 7, 2026**. Scope: state representation for the live Terraform overhaul, especially `crates/services/terraform/src/{runs,resources,execution}.rs` and representative `state_versions.json`, `workspaces.json`, and `plans.json` fixtures. This supplements, rather than silently revises, `2026-11-07-terraform-operation-contract.md`. The earlier note describes an older implementation; its cloned-state and 58→60 count observations must not be applied to the newly inspected execution model.[^l-prior][^l-execution][^l-runs]

**Research only.** Only this Markdown file is authored. No implementation, fixture, frozen Okta33 archive, or other pre-existing worktree change is modified. No installs, Terraform/provider execution, tests, Docker, taskgen, infrastructure/account operations, paid inference, or additional agents were used. Local JSON inspection and arithmetic are static evidence, not runtime qualification.

## 1. Bottom line

- **[D/P] Keep four representations distinct:** workspace metadata, StateVersion extracted summaries, raw internal state v4, and the public `terraform show -json` representation. Sharing a word such as `resources` does not make their structures interchangeable.[^d-workspaces][^d-state-api][^p-v4][^p-jsonstate]
- **[F/I] Deriving the fixture's workspace count from modeled managed instances is coherent as an explicit closed-world fixture convention.** It does not establish that HCP always computes its `resource-count` by summing every StateVersion summary, nor that the fixture contains a complete provider-valid state. The old 58 and the modeled 13 cannot both describe the same complete, synchronized managed-instance inventory without additional explanation.[^f-world][^l-initialize][^d-counts][^d-state-api]
- **[L/I] Two direct representation objections remain:** both advertised raw and stable-JSON state URLs return raw v4; generated plan JSON emits literal unknown sentinels with an empty `after_unknown`. Those are contract differences, not harmless alternative field names.[^l-download][^l-plan-json][^d-state-api][^d-json]
- **[D/P/I] Lineage identifies a state history, not a workspace label or run; serial orders snapshots within that history.** Stable lineage and increasing serial are appropriate local invariants. Exactly one increment per apply, globally comparable serials, or lineage derived from an HCP ID are not vendor guarantees.[^p-file][^p-meta][^p-remote][^d-state-api]
- **[F/I] Unindexed and numeric-indexed addresses cover the inspected fixture's modeled identity needs.** Summary counts alone cannot recover actual instance keys; synthesizing contiguous numeric keys, especially omitting an index at count one, must remain a fixture rule, not a Terraform parser rule.[^f-world][^l-instances][^d-address][^d-count]
- **[I] Missing provider-computed values are a capability boundary.** Planning/review can preserve unknowns. A claimed modeled apply needs explicit resolved outcomes for required unknown values; absent values must not be invented, guessed from labels, copied from an unrelated run, or disguised as successful live infrastructure.[^l-apply][^p-apply][^p-compatible]

## 2. Evidence and pins

Labels: **[D]** official documented contract; **[P]** inspected pinned first-party source; **[L]** inspected local implementation; **[F]** authored fixture; **[I]** inference/recommendation; **[U]** unresolved or unqualified behavior. A source implementation is not a public compatibility promise merely because it is readable.

| Evidence | Exact scope |
| --- | --- |
| Terraform Core **[P]** | Fixture-selected **1.9.5**, commit `2770cb376c53a9ef6ea63482d7eaf83c0bfe887e`; published module metadata identifies the tag and August 20, 2024 timestamp. This is deliberately not a claim about the latest Terraform release.[^p-release] |
| Terraform documentation **[D]** | Core documentation inspected at the same commit, plus current official public pages. Core source and prose are distinguished where the prose is less specific. |
| HCP documentation **[D]** | Official pages and first-party `web-unified-docs` source pinned to `965264c6e76c012c7bdc40a0a4d2f5456b49bce0`, also used by the earlier operation note. This documentation revision is not an HCP server release pin.[^d-state-api][^d-workspaces][^d-counts] |
| HCP SDK **[P]** | `go-tfe` **v1.111.2**, commit `76ed5bb13de750863b0a99c3a2a8961fa5edfb46`, the dependency pin identified in the earlier operation note; relevant source re-inspected here.[^p-sdk-state][^p-sdk-workspace][^l-prior] |
| Local worlds **[F]** | Representative NAT, checkout, orders-unlock, and payments-failure datasets have byte-identical `workspaces.json`, `state_versions.json`, and `plans.json`. They contain 13 workspaces, 11 state versions, and 19 plans each; that identity does not prove every other file is identical.[^f-world] |

Local SHA-256 witnesses captured during the final source recheck, **not a freeze of the concurrently live worktree**. Concurrent edits added a derived current-resource collection and changed integer-input parsing; the representation/count/unknown findings below were rechecked against those edits.

```text
4100805b99da97008441d112e166805834b4cebc190bb0bc8ddab560ac5b7da0  crates/services/terraform/src/runs.rs
0c20446efb346518e43a778a24ca58dae01b491cfe43f74dab89fcd90668c07e  crates/services/terraform/src/resources.rs
14e058e22b0e11bd4efc58298c66abb484bae2878022f9e117ad5013c7a8b707  crates/services/terraform/src/execution.rs
df0e3a9964b6d507257d677f51223613e6b67fa1a489f432b18d417569e9f8ab  docs/research/2026-11-07-terraform-operation-contract.md
```

Representative NAT dataset file witnesses:

```text
86524be1fbd8eb9bcaa297ae1d5e47e6347b698ca55870b700bab0ca69296910  workspaces.json
c04f282646a25675ff6a86991f893edae830f9656905600494d233a988d5d159  state_versions.json
43ae97af728d52b6a6161873b8c4d874c06c1369c2c418bb45a5b7afe8a7ed76  plans.json
48b6d89dded61c0a20ebcadc6892d8c0c77090a687bba2635c6527f492cf27f5  execution_outcomes.json
6e0acd279aa6cfe9f7d92d480fdf3efc7088988824ae9eebe8ef36733a5c6d88  planning_bindings.json
```

## 3. Representation boundaries and public fields

| Surface | Established meaning | What it does not establish |
| --- | --- | --- |
| Workspace `attributes.resource-count` **[D/P]** | An exposed workspace response field, also represented by `go-tfe.Workspace.ResourceCount`. | The inspected workspace reference does not give an exact cross-object formula or synchronization guarantee tying it to every StateVersion summary. Do not turn a response example into such a guarantee.[^d-workspaces][^p-sdk-workspace] |
| StateVersion `attributes.resources` **[D/P]** | Asynchronously extracted information. Official examples and SDK structure contain `name`, `type`, `count`, `module`, `provider`; the examples include **data sources**, using a type such as `data.terraform_remote_state`. | Not raw resource state, instance identities, attribute values, or necessarily a managed-only/billable count. `resources-processed=false` means empty-looking extraction may be unfinished, not zero resources.[^d-state-api][^p-sdk-state] |
| Raw download **[D/P]** | `hosted-state-download-url` serves Terraform's internal state format. Core 1.9.5 v4 has top-level `version`, `terraform_version`, `serial`, `lineage`, `outputs`, `resources`, and `check_results`. Resource entries group instance objects and include `mode`, `type`, `name`, provider-configuration address, and optional module path. | Not the long-term external integration format. A top-level resource entry is not necessarily one managed instance; `instances` can also contain current and deposed objects for the same key.[^d-state-api][^d-state-format][^p-v4] |
| Stable JSON download **[D/P]** | `hosted-json-state-download-url` is documented for the integration format, available for state created by Terraform 1.3+. Core 1.9.5 JSON state uses `format_version: "1.0"` and `values.root_module`, recursively nested `child_modules`, per-instance resource records, and `values` rather than raw `attributes`. | Not raw v4 renamed, and not a documented source of raw `lineage`/`serial`. The pinned JSON-state top-level struct has neither field.[^d-state-api][^d-json][^p-jsonstate] |
| Plan JSON **[D/P]** | Public change representation has `before`, `after`, `after_unknown`, actions, and sensitivity masks; resource identity metadata separates `name`, `index`, and module address. Core 1.9.5's plan JSON format version is `"1.2"`. | Not final applied state, not the internal binary saved-plan format, and not evidence that unknown provider values have already resolved.[^d-json][^p-jsonplan] |

**[D/P] Version numbers are different axes:** raw `version: 4` / HCP `state-version: 4` is the internal file format; `terraform-version: "1.9.5"` names the writer; `serial: 96` is a snapshot sequence value; state JSON `"1.0"` and plan JSON `"1.2"` are public representation versions. The documentation's introductory `"1.0"` example does not override the inspected plan serializer's `"1.2"` constant. Public format consumers should tolerate unknown properties on supported minor versions and reject unsupported major versions.[^d-state-api][^d-json][^p-jsonstate][^p-jsonplan]

### Raw state is inspectable, but implementation-specific

**[D/P]** Official state documentation warns that raw JSON can change and directs external integrations toward CLI JSON output. The pinned v4 codec supports managed/data mode, numeric/string/no instance keys, provider configuration addresses, schema versions, sensitivity paths, provider-private bytes, dependencies, taint, create-before-destroy, and deposed objects. These are inspected implementation details of that pin; their presence is not an invitation to invent missing provider data.[^d-state-format][^p-v4]

**[L/I]** The new local `resource-instances` array, derived `managed_resources` collection, `execution_outcomes`, `planning_bindings`, `resource-changes-detail`, and `simulated-plan` are **private fixture/execution-model data**, not documented HCP REST fields or Core v4 keys. `resources::state_version` correctly excludes `resource-instances` from attributes. Local summaries store a bare provider source name; HCP examples use provider-configuration strings such as `provider["registry.terraform.io/hashicorp/null"]`. Raw provider binding, public JSON `provider_name`, and local shorthand should not be collapsed into one universal string field.[^l-execution][^l-state-render][^f-outcomes][^d-state-api][^d-json][^p-v4]

**[L/I] Direct objection:** `resources::state_version` advertises two download URLs, but `archivist_object` routes both `state-…` and `state-json-…` through `state_file`, returning raw v4. A consumer following the stable JSON URL will not receive `format_version` / `values.root_module`. Provide genuinely separate renderings, or explicitly leave the unsupported integration surface unavailable; do not qualify the present alias as vendor-compatible.[^l-state-render][^l-download][^d-state-api][^p-jsonstate]

## 4. Counts: managed instances, summaries, and the 58-versus-13 fixture

### What the first-party evidence supports

**[D]** HCP's managed-resource guide describes the organization's count as derived from workspace state: managed resources in modules count, each `count`/`for_each` instance counts, and data-source mode does not count. Its examples distinguish two instances inside one raw resource container. This supports **instance counting with a managed-mode filter**, not `len(raw.resources)` and not indiscriminately summing summaries that include data sources.[^d-counts]

**[D/P/U]** StateVersion summaries are extracted asynchronously and their documented examples include data resources. The public SDK exposes only summary fields, not their complete original instance identities. Neither the cited API reference nor SDK defines an exhaustive aggregation algorithm for aliases, deposed objects, partially processed state, or all workspace-count timing. `billable-rum-count` is separately documented with visibility/plan conditions; a default local zero is not evidence that managed resources do not exist or that usage is free.[^d-state-api][^p-sdk-state][^l-state-render]

**[I]** For a complete, processed snapshot containing only ordinary current managed instances, summing exact per-group instance counts and counting its modeled instances can agree. That is a useful fixture consistency invariant. It is **not** an established universal equality among workspace metadata, arbitrary historical/intermediate StateVersions, all summary rows, raw serialized object entries, and billing.

### What the inspected fixture actually says

All rows below are **[F]**, not live infrastructure inventory.[^f-world]

| Workspace | Seed workspace count | Current state summary rows | Sum of summary counts | Initializer's modeled instance count |
| --- | ---: | ---: | ---: | ---: |
| `network-core-prod` | 58 | 7 | 13 | 13 |
| `network-core-staging` | 52 | 6 | 12 | 12 |
| `checkout-service-staging` | 18 | 6 | 6 | 6 |

**[F]** Prod NAT's `sv-2HkRH31vgu6V7Nh1`, serial 96, summarizes: one VPC, three private subnets, three public subnets, one existing NAT gateway, one EIP, three route tables, and one transit-gateway attachment. The seven groups total **13 instances**, not 58 and not seven instances. This is authored model inventory; it is not discovery of actual AWS objects.[^f-nat-state]

**[L]** `initialize` materializes instances from each summary unless an explicit `resource-instances` array already exists. It synthesizes `[0]…[count-1]` when count exceeds one and no index otherwise; absent attributes become `{}`. For current states it seeds available `before` attributes from declared workspace templates and confirmable current-run plans, asserting against contradictory existing attributes. It then overwrites each workspace's count with the length of its current state's materialized instance array. This is a **startup world transformation**, before an agent requests any operation, not a user-induced loss of 45 resources.[^l-initialize][^l-instances][^l-seed]

**[L/I]** The rechecked initializer and successful apply also mirror the current state's instances into `managed_resources`, keyed by workspace/address and removing superseded current rows. This is a derived local observation surface, not a new provider execution or evidence of AWS account inventory. Historical state snapshots and this current-only view have different purposes.[^l-current-resources][^l-finish]

**[F/L/I] Coherent explicit convention:** treat each summary as the **entire modeled managed inventory**, make materialized instances authoritative, and derive workspace summaries from those instances. Under that convention the old display count 58 is obsolete fixture metadata; it should not survive as an independent authoritative inventory claim. Do not fabricate 45 anonymous AWS instances to preserve it. An alternative partial-inventory model is possible, but would need explicit unmodeled inventory/count semantics and must not claim the rendered modeled state is complete. No inspected source explains 58 as real omitted infrastructure, stale HCP processing, or a billing adjustment.[^f-nat-state][^l-initialize][^d-state-api]

**[F/L/I] Expected bounded consequences, statically derived rather than executed:** the declared NAT plan creates two modeled instances and updates one existing route-table instance. With its supplied outcomes, the model goes **13→15**, with seven→nine raw resource groups and prod serial **96→97** for this seeded history—not 58→60. Checkout's variable changes `desired_count` and `min_capacity`, not Terraform's resource `count` meta-argument: two→four modeled service capacity does not create two more Terraform resource instances. Its managed count remains six if that update is later modeled as applied; the requested review-only run does not itself publish a new state.[^f-nat-plan][^f-outcomes][^f-checkout][^l-apply][^l-finish][^l-create][^d-count]

### Counterexamples and operational risks

- **[D/I] Mixed data and managed state:** an extracted `data.terraform_remote_state` summary contributes to an unfiltered sum but not the documented managed-resource count. The local serializer hardcodes every modeled resource to `mode:"managed"`; reject or explicitly model data resources before broadening the fixture.[^d-state-api][^d-counts][^l-state-resources]
- **[D/I] Wrong snapshot or unfinished extraction:** a historical/intermediate StateVersion need not be the workspace's current input state, and unfinished extraction is not an empty inventory. Do not reconcile counts without snapshot identity and processing context.[^d-state-api][^d-counts]
- **[P/I] Deposed objects:** raw v4 can serialize a current and a deposed object under the same instance key. Blindly counting every serialized object or keying solely by address is not a general solution; the inspected HCP docs do not settle every corresponding billing/summary rule.[^p-v4]
- **[F/I] Preservation baseline:** compare authorized-operation changes against the **initialized** world, not unnormalized dataset bytes. Otherwise startup normalization appears as collateral mutation across unrelated workspaces. Decide explicitly whether preserving seed display counts or closed-world consistency is the intended contract.[^l-initialize]
- **[L/I] Explicit-instance overrides:** startup trusts an existing `resource-instances` array without regenerating the StateVersion's `resources` summaries. Future fixtures that supply both must validate their agreement or regenerate summaries; otherwise the count normalization can coexist with contradictory public summaries.[^l-instances][^l-initialize]

## 5. Stable lineage and serial semantics

**[P]** Core's state-file model says lineage is assigned to a new blank history and remains unchanged; it is opaque and should be compared byte-for-byte. `SnapshotMeta` orders two snapshots by serial only when they share the same nonempty lineage. Migration can preserve both metadata fields; lineage therefore need not change when storage/workspace placement changes. Import validation rejects unrelated lineage and rejects different state contents with the same lineage/serial, unless an applicable override bypasses validation.[^p-file][^p-meta][^p-migrate]

**[D]** The state-push CLI documents protections for different lineage and a higher destination serial. HCP state creation requires the supplied serial to match raw state; optional lineage should match raw state. The inspected HCP StateVersion response documentation and pinned SDK do **not** establish a routinely returned `lineage` response attribute: the SDK exposes it in create options, not its StateVersion read model. Do not require an agent to read lineage from a native metadata result that does not carry it.[^d-push][^d-state-api][^p-sdk-state]

**[P/D/I]** Serial is state-history metadata, not number of applies, managed resources, or plan revisions. The pinned remote persister can skip unchanged persistence; HCP documents multiple intermediate snapshots during a run. Hence “each apply increments exactly once,” “each plan increments,” “current state always has the greatest serial across unrelated histories,” and “a serial increase proves successful infrastructure application” are all unjustified generalizations.[^p-remote][^d-counts][^p-meta]

**[L/F]** Local `state_file` synthesizes lineage on every download from the first 40 hex characters of SHA-256 of the workspace ID. This happens to be stable for that ID across local snapshots, rather than being newly generated per apply. `finish_apply` selects the maximum stored serial for that workspace plus one and assigns the same value to the new StateVersion and its rendered raw state.[^l-download][^l-digest][^l-finish]

**[I] Bounded acceptance:** stable nonempty lineage across ordinary successors, consistent API/raw serial, immutable older modeled snapshots, and monotonically increasing successor serial are sensible local checks. Deterministic workspace-derived lineage and workspace-wide `max+1` are acceptable explicitly limited shortcuts only while the fixture excludes migration, lineage replacement, imported unrelated histories, and intermediate persistence. They cannot represent importing a history under another workspace ID without changing lineage, or starting a genuinely new history under the same workspace ID. Exact UUID syntax is not the semantic requirement; don't replace one invented lineage convention with another.

**[L/I] Further limitation:** `finish_apply` has no saved base-lineage/base-serial comparison. Its partial `before` checks detect some conflicting resource attributes, but cannot establish full saved-plan freshness: a disjoint state change can escape those checks. Serial advancement after mutation is not itself optimistic concurrency control or proof that an immutable plan was applied to its original base.[^l-finish][^l-apply][^p-meta]

## 6. Plan intent, unknowns, and resolved apply values

### Public semantics

**[D/P]** `before` and `after` describe object values around an action, not an arbitrary JSON Merge Patch. Create has no prior object; delete has no subsequent object. `after` can be incomplete at plan time. `after_unknown` carries a matching structured mask: unknown leaves are `true`, known leaves are omitted. Plain values representations can omit unknowns or encode them as null, losing the distinction from absence/null unless the caller also uses plan-specific structures. Known null is not automatically an unknown.[^d-json][^p-jsonplan]

**[D/P]** Human-readable “known after apply” wording is not a JSON unknown value. The pinned plan serializer removes unknown values from the ordinary object representation and emits the separate mask. Successful final provider results are expected to resolve unknowns and preserve known planned values subject to Core's schema/compatibility handling. Core reports invalid unknown results after apply; it may still save known recovery data with unknowns replaced by null **alongside an error**. Therefore a saved state or null value alone does not certify a successful apply.[^p-jsonplan][^p-compatible][^p-apply]

### Current local model and actionable differences

**[F/L]** NAT's authored plan uses the string `"(known after apply)"` for its NAT allocation reference and route-table target. `execution::apply` treats this exact string as a wildcard, finds outcome data by **plan ID + resource address**, verifies its supplied known values against the partial plan, and requires selected result attributes to be an object without sentinel strings. Explicit provider bindings for creates are taken from outcomes or the change record. Missing modeled execution values return an unsupported capability result before publishing the successful state/run transition.[^f-nat-plan][^f-outcomes][^l-apply][^l-finish]

**[F/I]** Existing `execution_outcomes.json` is an explicit simulated outcome, not provider observation. Its EIP, NAT, and route-reference values can establish internally consistent modeled effects for this plan. This research supplies **no new AWS fields or values**. In particular, an existing fixture key such as the route-table object's `nat_gateway_id` is treated as an authored simulation attribute, not proof that this partial object matches a particular AWS provider resource schema.[^f-outcomes][^f-nat-plan]

**[L/I]** The fallback `plan_json` copies those strings into `change.after` but always supplies `after_unknown:{}`. That falsely represents them as known literal strings. It also names `aws_route_table.private[1]` as `name:"private[1]"` without `index:1`, and emits empty `planned_values.root_module` even for known changed values. The `json-output` override only changes the displayed JSON; `execution::apply` still consumes `resource-changes-detail`. Treat contradictory display/execution representations as a qualification failure, not two independent authoritative plans.[^l-plan-json][^l-apply][^d-json]

**[I] Recommended boundary:** normalize the private sentinel convention into a structured unknown mask at the public boundary; never emit it as if it were a resolved JSON value. A genuinely configured string with the same text must not become a global wildcard. Retain a single coherent immutable plan identity and known-value content across display and execution, without requiring a provider to resolve values before review.

**[L/I]** Initial `seed_before` observations and update merging are useful **partial-fixture conventions**: an update checks only supplied top-level `before` fields and merges supplied top-level `after` fields into existing attributes. That preserves omitted modeled fields, but is not a general Terraform object-state replacement algorithm. It cannot distinguish every omitted unknown, known null, unmodeled computed attribute, collection/set identity, or schema-driven normalization. The absence of the sentinel is not proof of provider-state completeness.[^l-seed][^l-apply][^p-compatible][^p-jsonstate]

## 7. Address support: sufficient fixture subset, not general reconstruction

**[D/P]** A resource's block name is distinct from its instance key. `count=1` still gives an instance such as `type.name[0]`; an unindexed `type.name` is the single instance when neither `count` nor `for_each` is used. `for_each` uses string keys. Module instance paths may be nested and indexed. Public JSON tells consumers to treat the absolute `address` as opaque and supplies separate identity metadata; raw v4 represents integer/string keys in `index_key`.[^d-address][^d-count][^d-json][^p-v4]

**[F/L/I]** All inspected stored plan-change addresses are root-resource addresses; the only indexed change address is `aws_route_table.private[1]`. Materialized summaries also include simple `module.iam` and `module.vpc_lookup` paths with numeric counts. A bounded parser and explicit fixture expansion can cover those cases. `address_parts` accepts unindexed resources and unsigned numeric resource keys, and retains a prefix as module text; it does not implement Terraform's complete address grammar.[^f-world][^l-address][^l-instances]

**[I] Do not generalize count summaries into identities.** A summary `count:1` could describe an unkeyed instance, `count` instance `[0]`, or a `for_each` instance with a string key. Count two does not recover string keys or prove the surviving keys are `[0]` and `[1]`. Partially changed state can have sparse indices. Summary grouping also loses deposed identity. Synthesize contiguous indices only because the fixture explicitly chooses them; for broader support, require actual instance identities or return unsupported.[^d-count][^d-address][^p-v4][^p-sdk-state]

**[L/I] Additional parser/rendering bounds:** ordinary string-keyed resource addresses are rejected by the local creation parser, but dots inside quoted keys can defeat its dot-splitting and be misparsed rather than safely rejected; module text is not validated either. Provider aliases/module bindings are absent from the modeled provider shorthand. `provider_of` guesses a provider from an underscore-bearing address segment, which the public format's provider-name discussion expressly shows is not universally safe. Current root AWS-named fixture changes do not validate that heuristic for arbitrary providers/modules.[^l-address][^l-plan-json][^l-state-resources][^d-json]

## 8. Support boundaries and prioritized objections

| Priority | Actionable objection / decision | Bounded acceptance |
| --- | --- | --- |
| **1 — representation** | Separate stable JSON state from raw v4. | Public URL returns the public shape, raw URL returns v4; unsupported surfaces are not advertised as implemented.[^l-download][^d-state-api] |
| **1 — unknown values** | Fix plan unknown masks and indexed identity metadata; reconcile display and execution data. | Keep unknowns at review time; require explicit resolution for a modeled completed apply. Never store the sentinel as a successful resolved provider value.[^l-plan-json][^l-apply][^d-json] |
| **1 — fixture counts** | Declare the modeled-instance convention and initialize the preservation baseline accordingly. | For this NAT fixture, normalize 58 to 13 and model 13→15; don't claim a vendor formula or invent omitted resources. Keep unrelated initialized inventories unchanged.[^l-initialize][^f-nat-state][^f-nat-plan] |
| **1 — absent outcomes** | Missing required computed values/provider binding means unsupported execution, not generated success. | Plan/review can remain supported without apply support. Explicit authored outcomes qualify only the declared model, not AWS behavior.[^l-apply][^f-outcomes][^p-apply] |
| **2 — identity/history** | Preserve lineage per history, align serial across representations, and state unsupported concurrency/import cases. | Don't use resource count, workspace labels, content hashes, or a serial increment as substitutes for saved-plan base identity.[^p-file][^p-meta][^l-finish] |
| **2 — model breadth** | Explicitly exclude data mode, string/sparse identity reconstruction, deposed/replacement lifecycle, unknown collections, and provider-schema fidelity until modeled. | Simple current managed resources with declared numeric/unindexed identities are enough for this fixture, not arbitrary Terraform state.[^p-v4][^l-instances][^l-state-resources] |

**[I] Absent provider-computed values: the precise limit.** It is reasonable to support a declared known-value update against a partial existing modeled object, or a review-only plan with unknowns, without manufacturing every provider attribute. It is not reasonable to call that a full provider-valid raw state or infer that all missing attributes are optional. In particular:

1. A create plan whose visible `after` happens to contain no sentinel can still omit computed fields; that alone does not justify a completed provider object. Provider schema and explicit outcome provenance are separate requirements.
2. An update can affect computed values not mentioned in its partial template. Preserving old fields is a fixture assumption, not a universal provider guarantee.
3. Hardcoded `schema_version:0`, absent provider-private data/dependencies/sensitive paths, and `{}` attributes for unobserved initial instances do not establish that Terraform with a real provider could refresh or consume the generated state.
4. Output values are cloned locally except destruction-related clearing/ID changes; this does not implement arbitrary computed output recalculation.
5. Accept/reject should be based on declared support, not whether a plausible ID can be constructed. Report unavailable/unsupported when the requested outcome depends on unprovided values; don't silently downgrade success to a count-only change.[^l-instances][^l-apply][^l-state-resources][^l-finish][^p-jsonstate][^p-v4]

**[U] Unqualified:** actual HCP `resource-count` update timing and exhaustive extraction/counting algorithms; hosted queue/intermediate-state races; actual installed AWS provider/version/schema behavior; Terraform acceptance of the partial generated state; and real infrastructure existence, health, connectivity, or capacity. State, outputs, tags, workspace names, and synthetic successful statuses are records of this fixture/model, not observations proving any of those properties. No counterexample above was executed.

## Sources

### Official public contracts and pinned documentation [D]

[^d-state-api]: HashiCorp, [State Versions API](https://developer.hashicorp.com/terraform/cloud-docs/api-docs/state-versions), Attributes, Create, and current-state retrieval; [pinned documentation source](https://github.com/hashicorp/web-unified-docs/blob/965264c6e76c012c7bdc40a0a4d2f5456b49bce0/content/terraform-docs-common/docs/cloud-docs/api-docs/state-versions.mdx#L45). Distinct downloads, processing, summary examples including data mode, serial/lineage request metadata, and intermediate snapshots.
[^d-workspaces]: HashiCorp, [Workspaces API](https://developer.hashicorp.com/terraform/cloud-docs/api-docs/workspaces); [pinned source](https://github.com/hashicorp/web-unified-docs/blob/965264c6e76c012c7bdc40a0a4d2f5456b49bce0/content/terraform-docs-common/docs/cloud-docs/api-docs/workspaces.mdx#L408). `resource-count` is exposed in examples; no exact cross-summary formula was found in the inspected reference.
[^d-counts]: HashiCorp, [Manage workspace state](https://developer.hashicorp.com/terraform/cloud-docs/workspaces/state), State usage and View managed resources; [pinned source](https://github.com/hashicorp/web-unified-docs/blob/965264c6e76c012c7bdc40a0a4d2f5456b49bce0/content/terraform-docs-common/docs/cloud-docs/workspaces/state.mdx#L25). Managed instance counting, data exclusion, intermediate snapshots, and drift limitations.
[^d-state-format]: HashiCorp, [State](https://developer.hashicorp.com/terraform/language/state), Format; [Core 1.9.5 documentation source](https://github.com/hashicorp/terraform/blob/2770cb376c53a9ef6ea63482d7eaf83c0bfe887e/website/docs/language/state/index.mdx#L59). Raw format evolution versus external integration points.
[^d-json]: HashiCorp, [JSON Output Format](https://developer.hashicorp.com/terraform/internals/json-format); [Core 1.9.5 documentation source](https://github.com/hashicorp/terraform/blob/2770cb376c53a9ef6ea63482d7eaf83c0bfe887e/website/docs/internals/json-format.mdx#L18), including State, Values, and Change Representation. Public version policy, identity metadata, provider name versus configuration address, schema-dependent values, and unknown/sensitivity masks.
[^d-push]: HashiCorp, [`terraform state push`](https://developer.hashicorp.com/terraform/cli/commands/state/push); [Core 1.9.5 source](https://github.com/hashicorp/terraform/blob/2770cb376c53a9ef6ea63482d7eaf83c0bfe887e/website/docs/cli/commands/state/push.mdx#L28). Documentation only; no push performed.
[^d-address]: HashiCorp, [Resource Addressing](https://developer.hashicorp.com/terraform/cli/state/resource-addressing); [Core 1.9.5 source](https://github.com/hashicorp/terraform/blob/2770cb376c53a9ef6ea63482d7eaf83c0bfe887e/website/docs/cli/state/resource-addressing.mdx#L25).
[^d-count]: HashiCorp, [`count` reference](https://developer.hashicorp.com/terraform/language/meta-arguments/count); [Core 1.9.5 source](https://github.com/hashicorp/terraform/blob/2770cb376c53a9ef6ea63482d7eaf83c0bfe887e/website/docs/language/meta-arguments/count.mdx#L70). Distinguishes resource/module blocks, indexed instances, and unindexed non-repeated resources.

### Inspected first-party implementation [P]

[^p-release]: Public Go proxy [Terraform v1.9.5 metadata](https://proxy.golang.org/github.com/hashicorp/terraform/@v/v1.9.5.info), `Origin.Hash` and tag resolution. No module installed.
[^p-v4]: Core pinned [`internal/states/statefile/version4.go`](https://github.com/hashicorp/terraform/blob/2770cb376c53a9ef6ea63482d7eaf83c0bfe887e/internal/states/statefile/version4.go#L60), reader mode/key handling; resource writer at line 359, current/deposed emission at line 389, structures at line 659. Raw codec details are implementation evidence.
[^p-file]: Core pinned [`internal/states/statefile/file.go`](https://github.com/hashicorp/terraform/blob/2770cb376c53a9ef6ea63482d7eaf83c0bfe887e/internal/states/statefile/file.go#L13), `File.Serial` and `File.Lineage` semantics.
[^p-meta]: Core pinned [`internal/states/statemgr/persistent.go`](https://github.com/hashicorp/terraform/blob/2770cb376c53a9ef6ea63482d7eaf83c0bfe887e/internal/states/statemgr/persistent.go#L108), `SnapshotMeta`.
[^p-migrate]: Core pinned [`internal/states/statemgr/migrate.go`](https://github.com/hashicorp/terraform/blob/2770cb376c53a9ef6ea63482d7eaf83c0bfe887e/internal/states/statemgr/migrate.go#L41), metadata preservation, `SnapshotMeta.Compare`, and `CheckValidImport`.
[^p-remote]: Core pinned [`internal/states/remote/state.go`](https://github.com/hashicorp/terraform/blob/2770cb376c53a9ef6ea63482d7eaf83c0bfe887e/internal/states/remote/state.go#L168), `PersistState`, unchanged-state skip and generated initial lineage. This is an inspected persister, not an exhaustive specification of all HCP backend behavior.
[^p-jsonstate]: Core pinned [`internal/command/jsonstate/state.go`](https://github.com/hashicorp/terraform/blob/2770cb376c53a9ef6ea63482d7eaf83c0bfe887e/internal/command/jsonstate/state.go#L23), state format `"1.0"`, structures, resource identity/schema/sensitivity, and provider-schema-dependent marshaling.
[^p-jsonplan]: Core pinned [`internal/command/jsonplan/plan.go`](https://github.com/hashicorp/terraform/blob/2770cb376c53a9ef6ea63482d7eaf83c0bfe887e/internal/command/jsonplan/plan.go#L30), format `"1.2"`, Change representation and unknown handling at line 457, `omitUnknowns`.
[^p-compatible]: Core pinned [`internal/plans/objchange/compatible.go`](https://github.com/hashicorp/terraform/blob/2770cb376c53a9ef6ea63482d7eaf83c0bfe887e/internal/plans/objchange/compatible.go#L17), schema-dependent planned/actual compatibility, known leaves, types, and collection behavior.
[^p-apply]: Core pinned [`internal/terraform/node_resource_abstract_instance.go`](https://github.com/hashicorp/terraform/blob/2770cb376c53a9ef6ea63482d7eaf83c0bfe887e/internal/terraform/node_resource_abstract_instance.go#L2536), error and best-effort persistence for unknown post-apply results; compatibility checking and legacy-provider handling at line 2589.
[^p-sdk-state]: HashiCorp go-tfe pinned [`state_version.go`](https://github.com/hashicorp/go-tfe/blob/76ed5bb13de750863b0a99c3a2a8961fa5edfb46/state_version.go#L94), StateVersion, create options, and `StateVersionResources` at line 241. SDK structures do not independently guarantee hosted extraction algorithms.
[^p-sdk-workspace]: HashiCorp go-tfe pinned [`workspace.go`](https://github.com/hashicorp/go-tfe/blob/76ed5bb13de750863b0a99c3a2a8961fa5edfb46/workspace.go#L223), `ResourceCount` JSON:API attribute.

### Local implementation and authored fixtures [L/F]

All local references describe inspected, unexecuted source; line numbers can move during the live overhaul.

[^l-prior]: `docs/research/2026-11-07-terraform-operation-contract.md:1`, earlier operation research and native/SDK pins. Its earlier implementation observations are not current state-model guarantees.
[^l-execution]: `crates/services/terraform/src/execution.rs:1`, current bounded execution model.
[^l-runs]: `crates/services/terraform/src/runs.rs:1`, run lifecycle and plan/state rendering.
[^l-address]: `crates/services/terraform/src/execution.rs:25`, `address_parts`.
[^l-instances]: `crates/services/terraform/src/execution.rs:49`, summary materialization and explicit-instance precedence; grouping at line 87.
[^l-seed]: `crates/services/terraform/src/execution.rs:106`, `seed_before`.
[^l-initialize]: `crates/services/terraform/src/execution.rs:128`, initialization and workspace count normalization; `crates/services/terraform/src/lib.rs:55`, initializer hook.
[^l-current-resources]: `crates/services/terraform/src/execution.rs:169`, `record_current_resources`, called by initialization and successful modeled apply.
[^l-apply]: `crates/services/terraform/src/execution.rs:269`, partial-plan compatibility, outcome selection, unknown rejection, and modeled apply.
[^l-state-resources]: `crates/services/terraform/src/execution.rs:372`, raw grouping, hardcoded managed mode/schema version, and provider rendering.
[^l-create]: `crates/services/terraform/src/runs.rs:265`, immediate local plan generation and conditional auto-apply; planning bindings in `crates/services/terraform/src/execution.rs:204`.
[^l-finish]: `crates/services/terraform/src/runs.rs:537`, outcome application before publishing new state, workspace-wide serial allocation, output cloning/clearing, and derived count.
[^l-plan-json]: `crates/services/terraform/src/runs.rs:1067`, provider heuristic and `plan_json` fallback/override.
[^l-download]: `crates/services/terraform/src/runs.rs:1275`, `state_file`; shared raw/JSON download handling at line 1334.
[^l-state-render]: `crates/services/terraform/src/resources.rs:860`, public metadata rendering, advertised download URLs, private-instance exclusion, and default extraction/billing fields.
[^l-digest]: `crates/services/terraform/src/common.rs:415`, truncated SHA-256 `hex_digest`.
[^f-world]: `scenarios/code/terraform/{approve-nat-gateway-change,scale-checkout-staging,unlock-orders-prod,payments-db-upgrade-failure}/dataset/terraform/`, static comparison of `workspaces.json`, `state_versions.json`, and `plans.json`; hashes reported above.
[^f-nat-state]: `scenarios/code/terraform/approve-nat-gateway-change/dataset/terraform/workspaces.json:424` and `scenarios/code/terraform/approve-nat-gateway-change/dataset/terraform/state_versions.json:538`, prod NAT identity, seed display count, current state serial and seven resource summaries.
[^f-nat-plan]: `scenarios/code/terraform/approve-nat-gateway-change/dataset/terraform/plans.json:465`, NAT plan `plan-vOqCAeqHb08EsDyk`, two creates, indexed update, partial attributes and authored unknown strings.
[^f-outcomes]: `scenarios/code/terraform/approve-nat-gateway-change/dataset/terraform/execution_outcomes.json:1`, supplied prod/staging NAT modeled outcomes, not live provider results.
[^f-checkout]: `scenarios/code/terraform/scale-checkout-staging/dataset/terraform/workspaces.json:97`, checkout staging partial template; `scenarios/code/terraform/scale-checkout-staging/dataset/terraform/state_versions.json:177`, six summary instances; `scenarios/code/terraform/scale-checkout-staging/dataset/terraform/planning_bindings.json:1`, declared integer bindings.
