# Vercel write-operation contract

Research date: **November 7, 2026**. Scope: the three Vercel write scenarios below, using the **captured `nganiet/mcp-vercel` community surface**, not Vercel's official remote MCP. This note uses only the permitted catalog/snapshot, scenario instructions/TOMLs, narrow fixture projections, public official documentation/OpenAPI, and public upstream source. Scenario expectations are local assertions, not evidence that Vercel implements them. No evaluator/backend implementation, live accounts, or benchmark runs were consulted. [L-catalog] [L-tools] [L-gift] [L-rollback] [L-rotate]

## Evidence and dates

- **[D] Documented:** official documentation or API schema retrieved on the research date; mutable, not a historical API-server pin.
- **[P] Source-pinned:** behavior visible at an immutable public source revision; not proof that the replica executes that revision.
- **[L] Local:** permitted scenario data, instructions, or captured tool declarations; not a vendor guarantee.
- **[I] Inferred/recommended:** implementation or evaluator rule derived from the cited evidence.
- **[U] Unresolved:** insufficient evidence or conflicting sources; do not silently turn it into a vendor guarantee.

Each business clock is copied independently from its scenario. **All three currently specify the same clock**; none has been rebased to the research date. Thus “this morning” in the rollback instruction means **October 28, 2026**, not November 7. **[L]** [L-gift] [L-rollback] [L-rotate]

| Scenario | Business clock, UTC | Business invariant |
| --- | --- | --- |
| `enable-gift-cards-in-previews` | `2026-10-28T15:00:00Z` | Configure storefront Preview/Development; preserve Production and all existing variables. |
| `rollback-storefront-production` | `2026-10-28T15:00:00Z` | Redeploy the storefront deployment that served before the bad deployment; change no environment variables or other project. |
| `rotate-checkout-stripe-key` | `2026-10-28T15:00:00Z` | Replace only the correct checkout project's production Stripe value; preserve sensitivity and the non-production test value. |

The table's requirements come from the respective instructions and clocks from the respective TOMLs. **[L]** [L-gift-i] [L-gift] [L-rollback-i] [L-rollback] [L-rotate-i] [L-rotate]

## 1. Capability and provenance boundary

### What is actually captured

The catalog declares Vercel as a **replica** and explicitly attributes its surface to `nganiet/mcp-vercel`. The snapshot declares server `vercel` version `0.1.0`, 15 tools, and advertised digest `83aff2292433c05417d671ba69f7f9a65a7d372929ad71f7e793c4e0e8859655`. This identifies local captured declarations, not an upstream commit or a pinned REST implementation. **[L]** [L-catalog] [L-tools]

| Captured tool | Relevant advertised capability | Important boundary |
| --- | --- | --- |
| `vercel-create-environment-variables` | `projectId`, optional `teamId`, array of `{key,value,target,type,gitBranch?}` | No advertised `upsert`, edit-by-ID, `customEnvironmentIds`, or `visibility`. |
| `vercel-remove-environment-variable` | Project plus environment-variable `id`; optional `customEnvironmentId`, team/slug | No built-in target selector such as “remove Production only.” |
| `vercel-get-environments` | Project ID/name | No advertised team, branch, custom-environment, or decryption selector. |
| `vercel-create-deployment` | Previous `deploymentId`, project/name, explicit target, source/settings overrides, selected flags | Not a rollback, promote, or standalone alias-assignment tool. |
| `vercel-list-all-deployments` | Project, target, state, limit | No advertised cursor, `since`/`until`, rollback-candidate, or team selector. |
| `vercel-get-deployment`, `vercel-find-project`, `vercel-get-project-domain` | Deployment, project, and domain inspection | Domain membership is not itself evidence of which deployment currently receives traffic. |

All advertised signatures and absent tools above are **[L]** facts, not assumptions about the official remote service. Creating a custom environment is separately advertised; that does not supply the missing custom-environment fields on environment-variable creation. **[L]** [L-tools]

### Trustworthy upstream association, but no execution pin

The catalog names the upstream repository; its public source at commit **`e6f917331dcf8e192e8dc4e6399283d5b4a955cd`** has the same 15 tool names and server name/version. This is a useful, source-pinned association, **not proof that this was the capture revision**: the catalog supplies no upstream revision, and a replica need not execute upstream code. **[L/P/U]** [L-catalog] [L-tools] [P-community-tools] [P-community-index]

At that examined revision:

- Environment creation posts the variable array to **`POST /v10/projects/{project}/env`**, adding only the optional team query; it does **not** add `upsert=true`. Its schema exposes the same key/value/target/type/branch fields. **[P]** [P-community-projects] [P-community-project-schema]
- Removal calls **`DELETE /v9/projects/{project}/env/{id}`**, forwarding its documented custom-environment/team/slug selectors. Environment listing calls **`GET /v10/projects/{project}/env`** without a decrypt query and serializes returned `envs`. **[P]** [P-community-projects] [P-community-env]
- Deployment creation posts to **`/v13/deployments`**; its transform defaults a missing target to Production. Listing calls **`/v6/deployments`**, defaults its limit to 50, and returns the deployment array rather than the pagination envelope. **[P]** [P-community-deployments] [P-community-deployment-schema]
- `forceNew` is transformed into a numeric body field in this community source, whereas the current official API documents it as a **query parameter**. Do not promise official deduplication behavior from that captured boolean. **[P/D/U]** [P-community-deployment-schema] [P-community-deployments] [D-deploy-create]
- The fetch helper returns `null` after transport, non-2xx, or JSON-decoding errors. Creation/removal handlers can consequently return misleading success-oriented text without a confirmed mutation. The index also uses `request.params.args` for most handlers, while the environment-listing handler expects `arguments`. These are source compatibility warnings, **not a finding about replica execution**. Do not copy these bugs into the contract or treat a textual success header as authoritative. **[P/I/U]** [P-community-http] [P-community-projects] [P-community-index] [P-community-env]

### Official Vercel is a different surface

Official Vercel MCP is a remote OAuth service. Its current catalog documents `create_project_env` with `upsert` and `edit_project_env`, and official deployment creation uses a `requestBody` shape. Tool availability and parameters depend on the connection. None of that retroactively adds capabilities to the captured community tools. **[D/I]** [D-mcp] [D-mcp-tools] [D-mcp-env] [D-mcp-deployments]

The official REST schema separately documents environment PATCH/upsert, rollback, promote, and alias APIs. REST availability is not proof that a given MCP connection exposes them. **[D/I]** [D-env-create] [D-env-edit] [D-openapi]

## 2. Environment-variable operation contract

### Identity, scope, and collision handling

1. **Record identity is not the key alone.** Edit/delete identify a variable by **project plus environment-variable ID**. Creation accepts a key with one or more built-in targets, optional Preview branch, and—in official REST—custom-environment IDs. The same name can legitimately have different values in different environments. **[D]** [D-env-create] [D-env-edit] [D-env-delete] [D-env-across]
2. **Model effective scope, not array order.** Use account/team → project → key → built-in/custom environment → Preview branch to resolve effective values, while retaining record IDs and shared-target membership. This is a recommended semantic model, **not a claimed database unique-index definition**. A single record can cover several targets; changing its value affects the scopes attached to that record. **[D/I]** [D-env-create] [D-env-edit] [P-cli-update]
3. **Branch-specific Preview variables override the default Preview value.** Omitting a branch configures the default Preview scope; it is not equivalent to writing only `feat/gift-cards`. Official API descriptions require Preview scope for `gitBranch`. Preserve branch overrides unless the task explicitly authorizes changing them. **[D/I]** [D-env] [D-env-create]
4. **Custom environments are separate scope.** The official create schema distinguishes `target: ["production"|"preview"|"development"]` from `customEnvironmentIds`; listing has custom-environment selectors. Do not flatten a custom environment into ordinary Preview because some deployment metadata calls it Preview. The captured create tool cannot advertiseably express the official custom-environment association. **[D/L/I]** [D-env-create] [D-env-list] [D-env] [L-tools]
5. **Create is not implicit upsert.** Official creation documents explicit `upsert=true` to update an existing value and documents an already-existing-variable failure. The response supports `created` and per-item `failed` results; neither HTTP success nor the community success sentence proves every requested item succeeded. Do not assume batch all-or-nothing semantics. **[D/P/I]** [D-env-create] [P-community-projects]
6. **Uniqueness is scope-aware, but the complete algorithm is unresolved.** Separate production and non-production rows with the same key are legitimate. Conversely, appending another row that conflicts in the same effective scope and relying on “last row wins” is not a demonstrated replacement operation. The cited public schema does not specify every mixed-target/branch/custom-environment collision, merge, or upsert-splitting rule. Treat those combinations as **[U]**, not as an invented exact unique constraint, error code, or status mapping. **[D/I/U]** [D-env-create] [D-env-across]
7. **Delete-by-ID is not target subtraction.** Without a custom-environment selector, deleting a shared record deletes the record, not just Production. The official CLI explicitly PATCHes remaining targets when removing one target from a shared record. Its examined implementation uses v10 for that PATCH; the published REST edit schema is v9. The custom-environment deletion selector's complete detachment semantics are not specified by the short public description; do not extrapolate them. **[P/D/U]** [P-cli-remove] [D-env-delete] [D-env-edit]

**Implementation recommendation [I]:** reject or reconcile conflicting same-scope creations instead of allowing duplicate records to “shadow” one another. Preserve legal disjoint-scope rows and default/branch override relationships. Do not use `type` or a newly generated record ID as a way to evade a scope collision. Specify unresolved overlap behavior before adding evaluator cases for it. [D-env-create] [D-env] [D-env-across]

### Representations, sensitivity, and Development

| Representation | Supported conclusion | What must not be inferred |
| --- | --- | --- |
| `plain` / `encrypted` | Current docs map these to readable Config values; environment variables are encrypted at rest. **[D]** | `plain` does not establish unencrypted storage at rest; `encrypted` does not by itself mean write-only Sensitive. |
| `sensitive` | Existing Sensitive variables are treated as Secret; a replacement can be supplied while the saved value remains hidden. **[D]** | A hidden value cannot be recovered merely because a response schema has a `value` string field. |
| `visibility: "config"|"secret"` | Current response schemas call this authoritative for new code paths when present; legacy records fall back to `type`. **[D]** | This field is not advertised by the captured environment-write tool. |

These distinctions are documented in the current classification guide and API schemas. **[D/L]** [D-env-types] [D-env] [D-env-create] [L-tools]

**Important date/version correction:** Vercel announced Config/Secret classification on **August 24, 2026**, before these scenarios' October 28 business date. Current docs explicitly allow Secrets in **Development**, reversing the earlier Production/Preview-only Sensitive rule. Integration connections still have a documented Development/Sensitive limitation. Therefore, “sensitive + development is always invalid” is **not a valid current universal contract**; nor does the captured schema prove that its replica implements the newer rule. Pin the chosen behavior explicitly rather than importing a historical restriction. **[D/L/U/I]** [D-types-change] [D-env-types] [D-env-across] [L-tools]

**Schema drift [U]:** the current classification guide demonstrates writes with `visibility`; the retrieved official create/edit request schemas do not list it, although response schemas do. Official CLI source at **`84878e532142f0ac584953695658b324eb9e5b80`** sends it when supplied. This is documented/source evidence of a newer model, not permission to assume unadvertised community arguments are forwarded. For these captured scenarios, `type: "sensitive"` remains the expressible production-secret representation. [D-env-types] [D-env-create] [D-env-edit] [P-cli-add] [P-cli-update] [L-tools]

**Redaction is an observation policy, not mutation truth.** Listing omits, masks, or encrypts a secret value without changing the stored logical value. The community read does not request decryption; the current REST `decrypt` selector is deprecated, and write-only Secrets cannot be retrieved after saving. A masked read is neither proof of equality with the replacement nor proof of failure. Verification must use a trusted mutation result and/or protected authoritative state, without exposing the value. **[D/P/I]** [D-env-types] [D-env-list] [P-community-env]

**Configuration versus deployed behavior:** changes apply to **new deployments**, not existing deployments. An environment-variable write must not silently rewrite an old deployment's environment snapshot or claim to have changed already-running code. Current rotation guidance requires redeployment for the new credential to become effective. **[D/I]** [D-env] [D-rotation]

## 3. Gift cards: preserve Production while adding non-production scope

### Initial evidence and required result

The fixture identifies `storefront` as **`prj_3dw2VoUa166qmaLV3P6dm828isAX`**. Its existing production-only `PAYMENTS_API_URL` record is **`9oK7J2kVMUskD5Tg`**, with value `https://pay.acme-shop.com` and type `plain`. The requested gift-card key and Preview sandbox binding are absent from the inspected storefront environment records. `storefront-admin` is a different project. **[L]** [L-gift-projects]

Required effective values, with no branch/custom-environment restriction:

| Key | Production | Preview | Development |
| --- | --- | --- | --- |
| `NEXT_PUBLIC_ENABLE_GIFT_CARDS` | Remains absent | `"true"` | `"true"` |
| `PAYMENTS_API_URL` | Preserve `https://pay.acme-shop.com` | `https://sandbox.pay.acme-shop.com` | Remains absent |

The requirements are **[L]**; branchless/default scope is **[I]** from the instruction's general Preview request and documented branch precedence. Preserve every pre-existing variable and leave other projects untouched. [L-gift-i] [L-gift] [L-gift-projects] [D-env]

### Plan and genuine alternatives

- **Captured-surface plan [L/P/I]:** resolve storefront; create the flag with `target: ["preview","development"]`, and the sandbox URL with `target: ["preview"]`. Supply no `gitBranch`. Leave the existing production URL row unchanged. The examined community client issues ordinary creation, which is sufficient because these effective scopes are missing. [L-tools] [L-gift-projects] [P-community-projects] [D-env-create]
- **Equivalent plans [D/I]:** the flag can be one shared Preview/Development record or two disjoint single-target records. The two keys can be created in one batch or separate calls, checking each result. `plain` is natural for these non-secret settings; an expressible readable `encrypted` representation need not fail an evaluator solely for differing from the sample's `plain`. [D-env-create] [D-env-types] [L-gift-i]
- **Not equivalent [I]:** updating the production URL row's value to the sandbox URL, widening it to include the sandbox target while sharing one value, using a branch-only flag, adding Production to the flag, or deleting/recreating existing variables. These violate scope or preservation requirements even if a flattened Preview projection looks right. [L-gift-i] [D-env-create] [D-env]

**Evaluator contract [I]:** assess effective scope and preservation, not exact new IDs, array order, target order, one batch versus several calls, or one shared flag row versus two disjoint rows. Reject duplicate conflicting scope rather than crediting an arbitrary projection winner. Do not require a production deployment; the task asks for configuration, and a new deployment is a separate effect. [L-gift-i] [D-env-create] [D-env]

## 4. Stripe rotation: a real authoritative replacement, not a masked lookalike

### Resolve ownership before mutating

The fixture associates **`api.acme-shop.com`** with **`checkout-api-next` / `prj_9NauQLxYvOeIyWZWq6mqvavv9xuH`**. The similarly named **`checkout-api` / `prj_Tt5m7wZLjgqPCBUfmEh1nWJN9ZIW`** serves `api-legacy.acme-shop.com` and is out of scope. Domain ownership, not the closest project name, resolves this task. **[L/I]** [L-rotate-domains] [L-rotate-i]

Within the correct project, `STRIPE_SECRET_KEY` is already split:

- **`rN1Dlb7NzGIlkNJD`**: `type: "sensitive"`, `target: ["production"]`.
- **`JSGgKKujZeGcSdw8`**: `type: "encrypted"`, `target: ["preview","development"]`, retaining the existing test value.

These IDs, classifications, and scopes are **[L]** fixture facts. Secret contents are intentionally omitted. The production webhook secret, other variables, and legacy checkout project must remain unchanged. [L-rotate-projects] [L-rotate-i] [L-rotate]

### Plan, alternatives, and limits

1. **Captured-surface replacement [L/P/I]:** inspect the correct project's variables; remove **only the production-only record ID**; create a new production-only `STRIPE_SECRET_KEY` with **the instruction-provided replacement value** and `type: "sensitive"`. Do not read a redacted response back into a write. The test record is not removed or broadened. This sequence is expressible because the captured surface has delete/create but no advertised edit/upsert. [L-tools] [P-community-projects] [L-rotate-projects]
2. **Non-atomicity [I/U]:** delete and create are distinct requests, with an observable failure/response-loss gap. No cited source promises a transaction across them. If creation is uncertain or fails, reconcile the production binding and report the incomplete configuration; never restore a leaked key by pretending a masked value is the old plaintext. [D-env-delete] [D-env-create] [D-env-types] [D-mcp-tools]
3. **Official-REST/official-MCP alternative [D/I]:** edit the existing production row by ID with the new value and retained secret classification, avoiding the delete/create gap. Explicitly scoped `upsert=true` is another official creation facility, but its behavior on mixed shared records is not fully specified here. Neither alternative is callable merely by adding an unadvertised argument to the captured community tool. [D-env-edit] [D-env-create] [D-mcp-env] [L-tools]
4. **Valid split-record representations [D/I]:** a production-only Sensitive row alongside a separate encrypted Preview/Development test row is legitimate. The two non-production targets may also be represented by disjoint rows with the same preserved test value/classification if the evaluator evaluates business scope rather than row identity. There is no reason to rewrite the existing shared test row in this fixture. A same-key Production row is not inherently a duplicate of a Preview/Development row. [D-env-create] [D-env-across] [L-rotate-projects]
5. **If encountering a shared production/non-production row [D/P/I]:** first separate its scopes while retaining the non-production value, then replace Production. Delete-by-ID alone would remove every target on that record. This is a general alternative, not the supplied initial state; rebuilding a hidden old value is not possible from a write-only read. [P-cli-remove] [D-env-types] [L-rotate-projects]

**Evaluator contract [I]:** verify that the authoritative production binding now contains the supplied replacement and is Sensitive/Secret, that no conflicting old production binding remains, and that non-production effective value and classification are preserved. Changing only `type`, returning a success sentence, writing a mask, or appending a duplicate production row must not count as rotation. Do not require preservation of the deleted production record's ID. [L-rotate-i] [D-env-types] [D-env-create] [D-env-delete]

**Deployment boundary [D/I/U]:** a successful configuration replacement does not prove the live checkout deployment uses the new credential. The instruction says the Stripe-side roll has already occurred; do not manufacture an additional Stripe operation. If the benchmark intends *runtime* rotation, explicitly authorize and evaluate a new successful deployment and routing cutover. Otherwise evaluate configuration rotation and report that redeployment is still needed for running deployments. Do not silently add deployment success to an environment-only task. [L-rotate-i] [D-env] [D-rotation]

## 5. Storefront recovery: predecessor selection and redeployment are separate obligations

### Select the deployment that actually served immediately before the bad one

The permitted fixture projection has the following storefront records. Dates below are conversions of the fixture's `createdAt` milliseconds to UTC, **not** inferred alias-assignment or traffic-start timestamps. **[L/I]** [L-rollback-deployments]

| Created, UTC | Deployment | State / target | Relevance |
| --- | --- | --- | --- |
| October 24, 2026, 09:42 | `dpl_8o704GjcwumkLAaxPwjoxb4V6QDF` | `READY` / Production | Commit `c3a9f12e8b47d05a6e21f9b3c7d84e50a1b6f2d9`; intended previous serving deployment. |
| October 25, 2026, 15:03 | `dpl_sxzR1ENpPTbZwLOubuwxzYolZyx0` | `READY` / Preview (`target: null`) | Cart-badge preview, not the production predecessor. |
| October 26, 2026, 11:18 | `dpl_PHkhNPm61EU1aF79kakZb9UhQNNQ` | `ERROR` / Production | Commit beginning `2f6c8a1`; failed eslint build, not the last serving build. |
| October 28, 2026, 08:15 | `dpl_7wC1vKiBnuwDgHLXzdLRklqAYci5` | `READY` / Production | Bad “Switch to payments client v3 (#491)” deployment, commit beginning `e07a3c9`; owns the production aliases in the fixture. |
| October 28, 2026, 09:31 | `dpl_1KT70nyZgoWTlCSRmxLe0svxe4Xx` | `READY` / Preview (`target: null`) | Newer gift-card preview, not a recovery candidate. |

The records are **[L]**; the intended previous serving identity is explicitly supplied by the local scenario TOML's comment/expectation. Official deployment responses also document that `target: null` can mean Preview. [L-rollback-deployments] [L-rollback] [D-deploy-get]

**Selection contract [D/I]:** identify the bad deployment within storefront, then establish the last production-serving deployment immediately before its routing change. Do not pick the second-newest deployment, newest Preview, latest Production-targeted attempt, or any older successful deployment. A Production target or `READY` state alone is insufficient: staged production deployments can be ready without serving. Prefer authoritative routing history/eligible rollback records, not commit-message similarity or creation-time sorting alone. [D-promoting] [D-rollback] [D-deploy-list]

**Evidence limitation [L/U/I]:** the inspected fixture supplies current aliases and timestamps but no explicit served-before journal. The local TOML identifies the intended predecessor; the READY/Production records make it the plausible candidate in this finite history, but do not establish a universal inference from `createdAt`. An overhaul should expose a serving-history/predecessor witness, or explicitly constrain the fixture so this inference is sound. The community list also lacks advertised pagination and discards the examined upstream pagination envelope; increasing a limit is not a general proof of complete history. [L-rollback-deployments] [L-rollback] [L-tools] [P-community-deployments]

### Operation choice: “redeploy” is not “reuse the compiled artifact”

| Plan | Vendor meaning | Relation to this instruction and captured surface |
| --- | --- | --- |
| Create from the predecessor `deploymentId` | Ordinary source-backed redeployment creates a new deployment/build tied to the selected deployment. | **Expressible**; matches the explicit request to redeploy the selected build in the API's deployment-reference sense. |
| Instant Rollback | Reassign production domains to an eligible existing production deployment without rebuilding it. | Best match for literal existing-artifact reuse, but **not captured** and not the requested create/redeploy operation. |
| REST promote of an existing deployment | The REST operation explicitly does not rebuild. Staged-production promotion differs from dashboard Preview-to-Production, which the guide describes as rebuilding. | **Not captured**; not interchangeable with redeploy merely because production routing changes. |
| Rebuild current branch/source, even with old commit metadata | Uses the source actually requested/resolved, not necessarily the predecessor deployment. | Does not satisfy predecessor identity. Metadata alone cannot make it equivalent. |

The vendor distinctions are **[D]**; availability and instruction-fit conclusions are **[L/I]**. [D-deploy-create] [D-openapi] [D-rollback] [D-promoting] [L-tools] [L-rollback-i]

**Captured-surface plan [L/D/I]:** call `vercel-create-deployment` using `name: "storefront"`, `deploymentId: "dpl_8o704GjcwumkLAaxPwjoxb4V6QDF"`, and `target: "production"`. Keep `withLatestCommit` false or omitted, and do not override source, files, routes, or settings to something else. Official documentation says `withLatestCommit: true` removes the selected deployment's Git SHA and uses the latest commit instead. Await and verify the returned deployment, rather than relabeling the old/bad object. [L-tools] [D-deploy-create] [L-rollback]

**Artifact-equivalence limit [D/P/U]:** the current create schema describes inherited settings/environment variables and a new ID, URL, and build. The official CLI warns that **prebuilt** redeployments can instead reuse prebuilt output without picking up new settings. Neither a matching commit SHA, a selected cache option, nor `deploymentId` alone establishes byte-for-byte build equality for all deployment types. The supplied fixture does not establish a prebuilt-artifact guarantee. If “exact build” means immutable compiled output, revise the task/surface to support eligible Instant Rollback; do not invent that guarantee for generic redeploy. [D-deploy-create] [P-cli-redeploy] [L-rollback-deployments] [D-rollback]

**Environment-resolution conflict [U]:** the current REST `deploymentId` description says environment variables are inherited from the original unless overridden; the general secret-rotation guide says redeployments pick up latest environment variables. The prebuilt exception adds another distinction. These sources do not justify one universal inheritance/freshness rule. Pin the applicable source-backed/prebuilt semantics before evaluating deployment environment contents, especially if expanding the rotation task into runtime rollout. The recovery task itself authorizes no environment-variable writes. [D-deploy-create] [D-rotation] [P-cli-redeploy] [L-rollback-i]

### Routing, asynchronous states, and derived effects

- **Creation acceptance is not completion [D/I].** Official creation describes queued/initializing/building progress and instructs clients to poll `readyState`. `READY` is build success; `ERROR` and `CANCELED` are unsuccessful outcomes. The current schema additionally exposes `BLOCKED` and, in list responses, `DELETED`; neither should be treated as successful production recovery. Do not invent exact durations, require every intermediate state to be observed, or assume an ordinary build `READY` means checkout is healthy. [D-deploy-create] [D-deploy-get] [D-deploy-list] [L-rollback-deployments]
- **Aliasing is a separate success dimension [D/P/I].** Deployment responses expose `aliasAssigned` and `aliasError`; the official CLI separately waits for readiness/checks and alias assignment. Current project schemas also expose `lastAliasRequest` with pending/in-progress/succeeded/failed/skipped job states, and the promote API can return 202. A ready build with failed/pending aliasing is not a completed traffic cutover. [D-deploy-get] [P-cli-redeploy] [D-openapi]
- **Verify actual production binding [D/I].** Require `acme-shop.com` and the applicable project production routes to resolve to the successful selected-source redeployment, with the project's current production target consistent with that routing. Domain ownership/verification alone is not a deployment binding. The official alias API returns the bound deployment; alias assignment moves a name from its old deployment to the new one. These REST alias operations are not captured tools. [D-alias-get] [D-alias-assign] [D-project-domain] [D-openapi] [L-tools]
- **Derived replica views must agree [I].** On successful recovery, deployment detail/list, any materialized project production target, and authoritative alias routing must consistently describe the new selected-source deployment. Preserve original deployment identities/source metadata, unrelated Preview aliases, redirects/domain ownership, other projects, and project environment records. A new object in a list without a traffic switch—or only a changed project pointer with no corresponding deployment—is insufficient. [L-rollback-i] [D-deploy-create] [D-deploy-get] [D-alias-get] [D-openapi]
- **Do not confuse historical alias arrays with live ownership [D/I].** The get-deployment schema describes `alias` as aliases assigned upon creation. Consequently, a historical alias string need not be an authoritative current-binding journal. If a replica maintains a separate current-alias projection, move that projection consistently; do not require rewriting historical fields merely to make them look current. [D-deploy-get] [D-alias-get]
- **Failure must not masquerade as recovery [I].** An accepted request that later errors/cancels, or fails aliasing, must remain observable as such. Do not install that failed deployment as the successful current production build. Reconcile actual routing before claiming whether the bad build still serves. Missing routing evidence is uncertainty, not success. [D-deploy-create] [D-deploy-get] [D-alias-get]
- **Instant Rollback has additional documented effects [D/I].** It disables automatic production-domain assignment until undone and can restore deployment-associated cron definitions. It does not rebuild environment values. Do not attribute those effects to ordinary redeploy, or accept rollback as a side-effect-free substitute for the instruction's requested operation. Exact-artifact rollback is a genuine alternative only after acknowledging the changed operation and its effects. [D-rollback] [L-rollback-i]

**Evaluator contract [I]:** require the selected predecessor identity/provenance, successful deployment outcome, and actual production routing—not merely commit text or an `acme-shop.com` string somewhere in a response. Do not pin the new ID/URL/timestamp, cached-byte identity, build duration, or an unsupported exact-once deployment count. Keep the instruction's redeploy requirement distinct from a relaxed “restore traffic by any supported operation” variant. [L-rollback-i] [L-rollback] [D-deploy-create] [D-alias-get]

## 6. Retry and response-loss contract

Official MCP guidance explicitly says to inspect the resource after an uncertain write before retrying. The examined community fetch helper can collapse a remote failure or decoding failure into `null`, so its result text is especially weak evidence. **[D/P]** [D-mcp-tools] [P-community-http]

| Uncertain operation | Reconciliation and permissible recovery **[I]** |
| --- | --- |
| Environment create/batch | Re-read scoped records; inspect item results if received. Preserve already-created correct items and retry only unresolved work. A duplicate-create failure is not proof that the existing value is the intended one. |
| Delete → create rotation | Determine whether the production-only old row remains, is absent, or has been replaced. Resume the missing step; do not delete the test row, invent an old secret from a mask, or call the sequence atomic. |
| Secret edit/upsert on a different authorized surface | Metadata-only/read-redacted equality cannot prove the value. Use trusted write acknowledgement/protected authoritative verification, or deliberately perform a supported replacement again while reporting uncertainty appropriately. |
| Deployment creation | If the returned ID is known, inspect that ID. If lost, search within the correct project and operation window/source context; a caller-supplied non-secret correlation metadata field can help but is not server-enforced idempotency. Do not choose an unrelated “latest” concurrent deployment. |
| Promotion/rollback/aliasing on another authorized surface | Inspect project job state and live alias binding, including failed mappings, before retrying or reporting completion. |

The table derives from documented resource identities, create/batch responses, deployment metadata, alias/job observability, and write-only secret behavior. [D-env-create] [D-env-edit] [D-env-delete] [D-env-types] [D-deploy-create] [D-alias-get] [D-openapi] [D-mcp-tools]

**No invented exactly-once guarantee [D/L/U/I]:** neither the captured signatures nor the examined endpoint schemas document an idempotency-key facility for these calls. Official deployment `forceNew` controls deduplication and can force a fresh build; it is not a documented transaction/exactly-once guarantee, and the community forwarding mismatch further limits inference. Response loss can leave a write applied without a usable acknowledgement. Evaluate recovery by consistent final scope/source/routing and bounded, justified retries, not by assuming the first write failed or requiring exactly one call/resource under all failures. [L-tools] [D-deploy-create] [D-openapi] [P-community-deployment-schema] [D-mcp-tools]

**Derived environment effects [I]:** an environment mutation must be reflected in authoritative project records and any effective per-target/branch/custom-environment view derived from them. Redacted response views must remain separate from those values. Existing deployment snapshots must not be retroactively changed. Recompute scoped views from the record set rather than letting mutation order pick a duplicate winner. This is a recommended replica consistency rule, not a claim that Vercel exposes the benchmark's local `env_by_target` structure. [D-env] [D-env-create] [D-env-types] [L-gift] [L-rotate]

## 7. Actionable evaluator objections and decisions

These are recommendations **[I]**, not observations of evaluator implementation:

1. **Do not reward projection-only rotation.** Require a real protected value replacement, preserved secret classification, and no conflicting production binding. Redaction/type-only edits cannot satisfy it. [L-rotate-i] [D-env-types] [D-env-create]
2. **Do not require one arbitrary record shape.** Accept disjoint target records and semantically identical batching when permitted; preserve effective scope and existing data. Retaining test-row identity is stronger than retaining the test value and must be an explicit requirement if desired. [L-gift-i] [L-rotate-i] [D-env-create]
3. **Do not identify the predecessor by recency alone.** Add an observable serving-history witness or a clearly declared finite-history constraint; include the intervening failed production build and newer Preview as decoys. [L-rollback-deployments] [D-promoting] [D-rollback]
4. **Resolve “exact build.”** The captured task can require redeployment **from the exact predecessor deployment ID**. Literal artifact reuse requires a different operation/surface and must not be claimed as generic redeploy behavior. [L-tools] [D-deploy-create] [D-rollback] [P-cli-redeploy]
5. **Separate acceptance, build completion, and routing completion.** Exercise pending and failed builds/alias jobs; do not immediately update every view to a fabricated READY success. Use logical ordering, not invented exact errors or timestamps. [D-deploy-create] [D-deploy-get] [D-openapi]
6. **Choose classification/version semantics explicitly.** Current Development Secrets are documented, but not pinned by this community capture. Do not hard-code the historical prohibition or inject unsupported `visibility` fields. [D-env-types] [D-types-change] [L-tools]
7. **Choose configuration-only versus runtime rotation.** The existing text supports a configuration contract; deploying the changed credential is a further observable obligation only if authorized and specified. Resolve the documented redeploy environment-inheritance conflict before asserting runtime values. [L-rotate-i] [D-env] [D-rotation] [D-deploy-create]
8. **Allow honest uncertainty and recovery.** Lost responses, partial batch results, unsupported pagination, and masked secrets cannot be converted into exact-once guarantees or unsupported success assertions. Pin any stricter simulator policy as local policy, not vendor truth. [D-mcp-tools] [D-env-create] [D-env-types] [P-community-deployments]

## Primary-source register

All remote sources were read publicly without authentication on November 7, 2026. Official documentation/OpenAPI links are **current, unpinned** sources; source links containing commit hashes are immutable examined revisions. Local links identify only the explicitly permitted context. The unresolved matters above remain unresolved; no live operation was used to settle them.

### Local context

- **L-catalog:** Vercel catalog entry, lines 184–195.
- **L-tools:** captured community tool declarations.
- **L-gift / L-rollback / L-rotate:** scenario clocks and local assertions.
- **L-*-i:** scenario business instructions.
- **L-*-projects / L-*-domains / L-*-deployments:** the narrow fixture projections described in the note; secret values omitted.

[L-catalog]: ../../catalog/services.toml#L184
[L-tools]: ../../servers/snapshots/vercel.json
[L-gift]: ../../scenarios/code/vercel/enable-gift-cards-in-previews/scenario.toml
[L-gift-i]: ../../scenarios/code/vercel/enable-gift-cards-in-previews/instruction.md
[L-gift-projects]: ../../scenarios/code/vercel/enable-gift-cards-in-previews/dataset/vercel/projects.json
[L-rollback]: ../../scenarios/code/vercel/rollback-storefront-production/scenario.toml
[L-rollback-i]: ../../scenarios/code/vercel/rollback-storefront-production/instruction.md
[L-rollback-deployments]: ../../scenarios/code/vercel/rollback-storefront-production/dataset/vercel/deployments.json
[L-rotate]: ../../scenarios/code/vercel/rotate-checkout-stripe-key/scenario.toml
[L-rotate-i]: ../../scenarios/code/vercel/rotate-checkout-stripe-key/instruction.md
[L-rotate-projects]: ../../scenarios/code/vercel/rotate-checkout-stripe-key/dataset/vercel/projects.json
[L-rotate-domains]: ../../scenarios/code/vercel/rotate-checkout-stripe-key/dataset/vercel/domains.json

### Official Vercel documentation and schemas

The OpenAPI operations used are `createProjectEnv`, `editProjectEnv`, `removeProjectEnv`, `filterProjectEnvs`, `createDeployment`, `getDeployment`, `getDeployments`, `getProject`, `requestRollback`, `requestPromote`, `listPromoteAliases`, `getAlias`, and `assignAlias`. In particular, inspect `getProject`'s `lastAliasRequest` and `targets`, `getDeployment`'s readiness/alias fields, and `createProjectEnv`'s `created`/`failed` response union. **[D]** [D-openapi]

[D-openapi]: https://vercel.com/openapi.json "Official OpenAPI schema; mutable current version"
[D-env-create]: https://vercel.com/docs/rest-api/projects/create-one-or-more-environment-variables "POST /v10/projects/{idOrName}/env"
[D-env-edit]: https://vercel.com/docs/rest-api/projects/edit-an-environment-variable "PATCH /v9/projects/{idOrName}/env/{id}"
[D-env-delete]: https://vercel.com/docs/rest-api/projects/remove-an-environment-variable "DELETE /v9/projects/{idOrName}/env/{id}"
[D-env-list]: https://vercel.com/docs/rest-api/projects/retrieve-the-environment-variables-of-a-project-by-id-or-name "GET /v10/projects/{idOrName}/env"
[D-env]: https://vercel.com/docs/environment-variables "Environment scope, branch overrides, encrypted-at-rest storage, deployment application"
[D-env-types]: https://vercel.com/docs/environment-variables/sensitive-environment-variables "Current Config/Secret semantics, Development, editing, integration limitation"
[D-env-across]: https://vercel.com/docs/environment-variables/manage-across-environments "Separate environment values and branch-specific variables"
[D-types-change]: https://vercel.com/changelog/environment-variables-now-use-config-and-secret-types "Vercel announcement, August 24, 2026"
[D-rotation]: https://vercel.com/docs/environment-variables/rotating-secrets "Configuration replacement versus effective deployment rotation"
[D-deploy-create]: https://vercel.com/docs/rest-api/deployments/create-a-new-deployment "POST /v13/deployments; deploymentId, withLatestCommit, forceNew and asynchronous states"
[D-deploy-get]: https://vercel.com/docs/rest-api/deployments/get-a-deployment-by-id-or-url "GET /v13/deployments/{idOrUrl}"
[D-deploy-list]: https://vercel.com/docs/rest-api/deployments/list-deployments "Current GET /v7/deployments; filters, states, pagination"
[D-promoting]: https://vercel.com/docs/deployments/promoting-a-deployment "Current, staged, promoted; dashboard Preview promotion versus production promotion"
[D-rollback]: https://vercel.com/docs/instant-rollback "Previously serving eligibility, artifact reuse, routing and additional project effects"
[D-alias-get]: https://vercel.com/docs/rest-api/aliases/get-an-alias "GET /v4/aliases/{idOrAlias}; actual deployment binding"
[D-alias-assign]: https://vercel.com/docs/rest-api/aliases/assign-an-alias "POST /v2/deployments/{id}/aliases; reassigns aliases"
[D-project-domain]: https://vercel.com/docs/rest-api/projects/get-a-project-domain "Project domain membership and verification"
[D-mcp]: https://vercel.com/docs/agent-resources/vercel-mcp "Official remote MCP and connection-dependent capabilities"
[D-mcp-tools]: https://vercel.com/docs/agent-resources/vercel-mcp/tools "Tool availability and inspect-before-retry guidance"
[D-mcp-env]: https://vercel.com/docs/agent-resources/vercel-mcp/tools/environment-variables "Official MCP environment creation/upsert and editing"
[D-mcp-deployments]: https://vercel.com/docs/agent-resources/vercel-mcp/tools/deployments "Official MCP deployment input shapes and asynchronous workflows"

### Immutable public source examined

Community source revision: `e6f917331dcf8e192e8dc4e6399283d5b4a955cd`; official Vercel CLI source revision: `84878e532142f0ac584953695658b324eb9e5b80`. These pin the source observations, **not** the local capture's executable provenance or the remote API server. **[P/U]** [P-community-index] [P-cli-redeploy] [L-catalog]

[P-community-tools]: https://github.com/nganiet/mcp-vercel/blob/e6f917331dcf8e192e8dc4e6399283d5b4a955cd/src/constants/tools.ts "15 community declarations, including the relevant environment and deployment tools"
[P-community-index]: https://github.com/nganiet/mcp-vercel/blob/e6f917331dcf8e192e8dc4e6399283d5b4a955cd/src/index.ts "Server identity and dispatch"
[P-community-projects]: https://github.com/nganiet/mcp-vercel/blob/e6f917331dcf8e192e8dc4e6399283d5b4a955cd/src/tools/projects/handlers.ts#L25 "Environment POST and DELETE; project/domain reads"
[P-community-project-schema]: https://github.com/nganiet/mcp-vercel/blob/e6f917331dcf8e192e8dc4e6399283d5b4a955cd/src/tools/projects/schema.ts#L3 "EnvironmentVariableSchema and CreateEnvironmentVariablesSchema"
[P-community-env]: https://github.com/nganiet/mcp-vercel/blob/e6f917331dcf8e192e8dc4e6399283d5b4a955cd/src/tools/environments/handlers.ts#L10 "Environment listing without decryption selector"
[P-community-deployments]: https://github.com/nganiet/mcp-vercel/blob/e6f917331dcf8e192e8dc4e6399283d5b4a955cd/src/tools/deployments/handlers.ts#L45 "Deployment POST, get, and list forwarding"
[P-community-deployment-schema]: https://github.com/nganiet/mcp-vercel/blob/e6f917331dcf8e192e8dc4e6399283d5b4a955cd/src/tools/deployments/schema.ts#L65 "Production default, source flags, forceNew transform"
[P-community-http]: https://github.com/nganiet/mcp-vercel/blob/e6f917331dcf8e192e8dc4e6399283d5b4a955cd/src/utils/api.ts#L4 "Fetch/JSON errors return null"
[P-cli-add]: https://github.com/vercel/vercel/blob/84878e532142f0ac584953695658b324eb9e5b80/packages/cli/src/util/env/add-env-record.ts "Official CLI create/upsert, built-in/custom target separation, visibility"
[P-cli-update]: https://github.com/vercel/vercel/blob/84878e532142f0ac584953695658b324eb9e5b80/packages/cli/src/util/env/update-env-record.ts "Official CLI update by record ID"
[P-cli-remove]: https://github.com/vercel/vercel/blob/84878e532142f0ac584953695658b324eb9e5b80/packages/cli/src/commands/env/rm.ts#L352 "PATCH remaining targets instead of deleting a shared record"
[P-cli-redeploy]: https://github.com/vercel/vercel/blob/84878e532142f0ac584953695658b324eb9e5b80/packages/cli/src/commands/redeploy/index.ts#L141 "Prebuilt exception, forceNew query, readiness and alias waiting"
