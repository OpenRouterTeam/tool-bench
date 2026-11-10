# Supabase write-family operation contract (2026-11-06)

## Scope, evidence, and isolation

Independent research for the **next** Supabase overhaul, not a change to the frozen MongoDB runtime campaign. Local date at writing: **November 6, 2026, America/Chicago**. The scenarios' business clock remains **April 2, 2026, 09:00 UTC**; “last month” in the marketing instruction is relative to that clock, not this research date. [L1–L4]

Only this note was written. No local service/evaluator implementation was read; no code, scenarios, instructions, fixtures, dependencies, branches, or git state were changed. No subagents, package installations, tests, benchmark/model runs, live-account calls, or resource operations were used. Upstream archives were streamed for inspection/hash calculation without installation or extraction to disk.

Evidence labels throughout:

- **Documented:** explicit PostgreSQL/Supabase product or API documentation.
- **Pinned implementation:** inspected upstream MCP source associated with the catalog package, not a promise about Supabase's hosted control-plane internals.
- **Local:** catalog, captured schema, unchanged instructions/TOMLs, or narrowly selected fixture data.
- **Recommendation/inference:** proposed benchmark contract, not an assertion about an inspected evaluator.
- **Unresolved:** not established by those sources.

### Native server does not mean native database

**Local:** `catalog/services.toml:331` configures Supabase as `mode = "real"`, with the **native npm MCP server** `@supabase/mcp-server-supabase@0.15.0`, MCP port `7118`, and management API redirected by `--api-url={world}` to the world service on port `8118`. Documentation is likewise redirected by `--content-api-url={world}/docs/api/graphql`; no project restriction or read-only flag is specified. Unlike adjacent MongoDB, Supabase is **not** cataloged as a captured-tool replica. Its snapshot is evidence of the captured interface, not the catalog's launch mechanism. [L1, L2]

This establishes a native MCP-to-world HTTP boundary, **not** that the world executes a real PostgreSQL server or implements every PostgreSQL feature. Fixture project metadata advertises PostgreSQL engine `17`, version `17.4.1.064`; that is fixture data, not an observed database process. PostgreSQL 17 documentation below defines the intended SQL semantics, with actual world support deliberately uninspected. [L1, L5, P1–P6]

**Pin provenance:** registry metadata identifies `0.15.0`, repository `supabase/mcp`, and archive SHA-1 `c3995acdf429a4088fbddf6f4cba8496efa1773b`. Its published SLSA statement names commit **`0edc736f256871b1d539a320def43a001bd8c7a8`**. The streamed archive's computed SHA-512 matched both registry integrity and that statement:

```text
f9986ed222a380a7cdd602651ae6909b38e25ef947b4df8b1cc8f6da9887804ac2fe6896b2b7be4e62ed087d8056c7be9f4c41d9e7a1c878611503896e390219
```

The attestation payload was decoded, **not cryptographically verified**. Commit links below are consequently a registry-provenance association, not a supply-chain certification. The package's dependencies include semver ranges; this research did not establish a fully locked transitive dependency graph or inspect the running image. Current hosted documentation and a separately versioned CLI example must not be relabeled as the MCP pin's server-side implementation. [U1–U3]

## 1. Exact exposed MCP arguments and legitimate alternatives

All rows below come from `servers/snapshots/supabase.json`; the captured tools disallow additional argument properties. Include the explicitly required `schemas` and `verbose` on `list_tables`, notwithstanding its defaults and the conflicting description of the schema default. [L2]

| Tool | Captured argument object | Purpose/boundary |
| --- | --- | --- |
| `list_projects` | `{}` | Discover project identity; pinned tool returns a `projects` collection. |
| `get_project` | `{"id":"<project-ref>"}` | Read one project; **`id`, not `project_id`**. |
| `pause_project` | `{"project_id":"<project-ref>"}` | Submit pause; no status, wait, retry, or idempotency-key argument. |
| `restore_project` | `{"project_id":"<project-ref>"}` | Exposed, but restoration is not a pause alternative or authorized compensation here. |
| `list_tables` | `{"project_id":"<ref>","schemas":["public"],"verbose":true}` | Inspect tables/columns/constraints; not a full-state preservation proof. |
| `execute_sql` | `{"project_id":"<ref>","query":"<SQL>"}` | SQL reads and cancellation DML; no captured `parameters`, `sql`, or client-controlled `read_only` argument. |
| `get_advisors` | `{"project_id":"<ref>","type":"security"}` | Captured `type` enum: `security`, `performance`, `health`. |
| `list_migrations` | `{"project_id":"<ref>"}` | History discovery, not proof of stored SQL or schema effect. |
| `apply_migration` | `{"project_id":"<ref>","name":"enable_rls_on_invoices","query":"alter table public.invoices enable row level security;"}` | Dedicated apply-and-track path; `name` is described as snake_case. |
| `search_docs` | `{"graphql_query":"<GraphQL>"}` | Documentation only; not a mutation surface. |

**Pinned implementation:** account discovery uses `GET /v1/projects` or `GET /v1/projects/{ref}`; pause uses `POST /v1/projects/{ref}/pause`. SQL uses `POST /v1/projects/{ref}/database/query`; migration listing/apply use `GET`/`POST /v1/projects/{ref}/database/migrations`. The platform accepts SQL parameters internally, but the captured MCP input does not expose them. Do not invent tool arguments from the underlying REST API. [U4–U7]

**Legitimate alternatives:** project discovery can use the list or an already-grounded exact-ID lookup; SQL verification can use `execute_sql` rather than rely solely on table summaries. Cancellation admits several equivalent SQL forms (§2). An actual **data migration** through `apply_migration` can also perform cancellation, provided it uses discovered identity and creates honest history; the tool description explicitly contemplates data migrations. Whether that extra history is allowed must be an explicit preservation rule, not accidental rejection by a required `/database/query` call. For the RLS task, however, SQL-only DDL is **not** a substitute for its expressly requested tracked migration. No exposed generic HTTP tool, project-status setter, or SQL-based cloud pause is established. [L2, L3, U4–U7]

## 2. Cancel every actual pending order for the exact-email customer

### Grounded target and complete affected set

**Local:** production is `storefront-prod`, project `qmvbxtzkplrnwdhyfaco`, organization `northwind-labs`; staging is `hzrkcwpxnqdlmvbtejsa`. The production customer with exact email `priya.shah@fastmail.com` is **1042**. Customer **1107** has the same full name but email `priya.shah@acme.io`; **1110** is Priya Shahani. The schema projection declares email `text NOT NULL UNIQUE`. [L3, L5]

The inspected production fixture has pending target orders **5003** (`89.9 USD`) and **5011** (`45.0 USD`), plus paid **5007** (`120.0 USD`). Duplicate-name pending orders **5020/5021**, unrelated pending **5009**, every shipped order, and all staging data must remain unchanged. Staging has its own exact-email customer **3**, with pending orders **3/4**. These are fixture observations, **not an exhaustive hardcoded evaluator definition for future fixture variants**. [L3, L5]

**Recommended invariant:** derive the eligible set from the complete admitted pre-state:

```text
E = production orders whose customer_id identifies the exact-email customer
    and whose admitted status is pending
```

Require every member of `E` to become `cancelled`; require every non-status field on those rows and all fields on all other business rows to retain their prior values. Check full row membership as well as values. Do not accept only the two sentinel IDs if another eligible order exists, offsetting changes to totals, or delete-and-reinsert substitutes. [L3, L5; recommendation]

**Business ambiguity:** PostgreSQL does not define “unpaid.” This fixture has a status enum (`pending`, `paid`, `shipped`, `refunded`, `cancelled`) and no independent payment-state column. Mapping “not paid yet” to **pending** is supported by this task's title, comments, exemplar, and data model—not a Supabase guarantee. `status <> 'paid'` wrongly includes shipped/refunded/cancelled rows; excluding only paid/shipped remains overbroad. If future data adds partial-payment, authorization, null/unknown statuses, or contradictory payment evidence, establish eligibility explicitly rather than silently broadening the predicate. [L3, L5]

### Valid SQL forms, not one string

The following is a documented PostgreSQL `UPDATE`/`EXISTS` form, submitted as `execute_sql`'s `query` against the production project:

```sql
UPDATE public.orders AS orders
SET status = 'cancelled'
WHERE orders.status = 'pending'
  AND EXISTS (
    SELECT 1
    FROM public.customers AS customers
    WHERE customers.id = orders.customer_id
      AND customers.email = 'priya.shah@fastmail.com'
  )
RETURNING orders.id, orders.status, orders.total;
```

An equivalent documented join form is:

```sql
UPDATE public.orders AS orders
SET status = 'cancelled'
FROM public.customers AS customers
WHERE orders.customer_id = customers.id
  AND customers.email = 'priya.shah@fastmail.com'
  AND orders.status = 'pending'
RETURNING orders.id, orders.status, orders.total;
```

Other legitimate forms include the scenario's scalar subquery, an `IN (SELECT id ... WHERE email = ...)` predicate, a read-only CTE resolving the customer followed by `UPDATE`, or discovered-ID updates retaining the pending guard. `RETURNING` is optional for mutation correctness; row order is not an identity guarantee. With `UPDATE ... FROM`, ensure one join match per order; the fixture's unique email/primary key supplies that condition. A scalar subquery yields NULL for no row and errors for multiple rows, unlike arbitrarily choosing `LIMIT 1`. [P1–P3, P10, P12, L3, L5]

**Recommended acceptance:** aliases, identifier quoting, keyword case, casts compatible with the actual schema, whitespace, comments, and equivalent predicates must not determine success. Narrow sequential updates can be correct if they cover the complete discovered set and preserve every other row. A data-modifying CTE that attempts to update the same row twice has no reliable ordering/result guarantee; it is not an equivalence shortcut. [P3, P11; recommendation]

### Accounting, concurrency, and retries

- **Documented:** `UPDATE` retains columns not named in `SET`. PostgreSQL `numeric` is exact decimal and recommended for monetary quantities; binary floating-point types are inexact. Consequently, do not rewrite `total`, convert currency, zero amounts, issue refunds, or touch invoices merely to cancel a status. Compare per-row monetary values exactly in the semantic state; `89.9` and `89.90` need not fail solely because a JSON rendering differs. Schema/type and currency preservation remain separate checks. Fixture JSON numbers do not prove the world's numeric precision. [P1, P4, L5]
- **Documented:** at Read Committed, an updater encountering a concurrently updated row rechecks its `WHERE` condition on the updated version. Keeping `status = 'pending'` in the write therefore matters; a prior read of pending IDs is not a replacement for that guard. This does not promise continuous cancellation of orders created after the statement's snapshot, nor cross-tool transaction continuity. Define the evaluation cut/snapshot. [P5]
- **Inference:** a repeated guarded update normally has no further eligible rows, but zero returned rows can mean already done, wrong identity, insufficient visibility, or concurrent change. Do not manufacture completion from a zero count or assume the non-idempotent tool annotation promises exactly-once behavior. Triggers, RLS, and permissions can affect execution; trigger-generated unrelated side effects must be evidenced, not silently allowlisted. [L2, P1, P5, P6, P7]

**Unresolved:** actual world parser coverage, role, transaction reuse, trigger inventory, and nondeterministic email-collation behavior were not inspected. The fixture's unique-text declaration is not runtime proof of all those properties. [L5]

## 3. Enable RLS, with no policies, through a genuine tracked migration

### Requested effect and security limits

**Local:** the target is production `public.invoices`. Customers/orders are already RLS-enabled; staging invoices are not. The instruction requires **no policies**, an actual tracked migration, and no staging changes. The raw invoices table projection omits a `policies` property rather than explicitly containing `[]`; use policy-catalog evidence for the real no-policy condition, not JSON omission as proof. [L4, L6]

**Documented:** `ALTER TABLE public.invoices ENABLE ROW LEVEL SECURITY` enables RLS. With no applicable policies, ordinary roles subject to RLS face default denial. Table owners normally bypass RLS; superusers and `BYPASSRLS` roles bypass it, with `FORCE ROW LEVEL SECURITY` affecting the owner case. `ENABLE` is not `FORCE`, and the user did not request `FORCE`, policy creation, privilege changes, or modifications to customers/orders. [P6, S1]

Supabase service keys can bypass RLS, but must stay server-side; a service-key client operating with a signed-in user's Authorization context follows that user's RLS. Service-role bypass does not establish that every backend code path actually uses that role or remove ordinary privilege requirements. Verify configuration assumptions rather than “fixing” backend access with a permissive policy. [S1, S2]

The original finding `0013 rls_disabled_in_public` should cease to apply once RLS is enabled. A separate `0008 rls_enabled_no_policy` advisory is compatible with the **explicitly requested** no-policy outcome; do not require an empty advisor list or modify unrelated findings. The inspected raw `advisors.json` contains only an unrelated performance notice, not the security finding described by the scenario. Whether security findings are derived dynamically is deliberately **unresolved**, not evidence that the user instruction is wrong. [S3, S4, L4, L6]

### What “tracked migration” actually requires

**Documented:** the Management API migration endpoint automatically creates a migration in the `supabase_migrations` schema and executes it; failed migrations roll back. SQL-query endpoints are separately documented operations, not an automatic history guarantee. The captured `apply_migration` is the natural authorized route. [S5, S6, L2]

**Pinned implementation:** `apply_migration` passes the actual `name` and `query` to that endpoint, awaits it, and returns `{success:true}`; it intentionally discards the endpoint's result. It does not fabricate a version, itself insert SQL into a history table, or verify a read-back. `list_migrations` returns a `migrations` collection whose declared entries contain `version` and optional `name`; the exposed output schema does not promise stored statements. [U5–U7]

**Documented public representation:** the REST history-list response likewise contains version/name, while the single-migration endpoint `GET /v1/projects/{ref}/database/migrations/{version}` documents `statements` and `rollback` arrays plus identity/attribution fields. That single-record operation is **not** a captured MCP tool. A permitted `execute_sql` read of `version, name, statements` from `supabase_migrations.schema_migrations` can supply deeper evidence if the actual schema/role permits; do not invent a `get_migration` MCP call or expose the REST response's `idempotency_key` as an unsupported tool argument. [S9, L2, C1]

**Versioned representation evidence, not the hosted server pin:** Supabase CLI **v2.65.5** creates `supabase_migrations.schema_migrations` with primary-key `version text` and adds `name text` and `statements text[]`. It inserts the migration's statement array together with its version/name; its executor batches the statements plus history insert in an implicitly transactional batch. That supplies a concrete first-party history/SQL-storage model, but does **not** prove the hosted Management API runs that CLI version or preserves byte-identical SQL formatting. [C1, C2]

**Recommended minimum success evidence:**

1. The production table's real `relrowsecurity` condition changes to enabled, with its policy set still empty and its rows/other schema properties preserved. SQL inspection of `pg_class`/`pg_policies` is a legitimate exposed verification path. [P6, P8, P9]
2. A successful migration operation is tied to a **new authentic production history entry**, including a real version/identity and attribution to the applied operation. Preserve all existing production history and all staging history. The fixture baseline has four migrations per storefront project, with names, versions, and nonempty statement arrays. [L6; recommendation]
3. Where history stores SQL, retain the applied SQL—not merely a name or empty placeholder. The new entry must correspond to the RLS change. Whitespace/statement-splitting differences are not automatically fraud, and existing legacy nullable statement storage must not be retroactively rejected. The minimal list response alone cannot establish stored-SQL integrity. [C1, C2, U7; recommendation]
4. Failure of the requested ordinary RLS migration must not leave the RLS effect committed without its history, or a successful-looking history row without the effect. This is the relevant atomicity obligation; the public rollback guarantee does not specify whether initial creation of tracking infrastructure also disappears after every failure. [S5; recommendation]

**Not success:** `execute_sql` doing only `ALTER TABLE`; changing only an `rls_enabled` projection; a migration POST with no real effect; `SELECT 1`/empty/fake SQL recorded while DDL ran elsewhere; hand-inserting a cosmetic ledger row; an empty history list; or a success envelope without the committed schema/history evidence. Manual history repair is not proof that this requested change was applied-and-tracked. Genuine alternative snake_case names and semantically equivalent DDL should work; the exemplar name is not user-mandated. [L2, L4, S5; recommendation]

**Retry caution:** enabling RLS again may be harmless as a schema action, but another `apply_migration` can create another history entry. The captured MCP exposes no caller-selected version or idempotency key. After a timeout/ambiguous response, reconcile table state **and** history before retrying; do not equate an HTTP error with guaranteed non-commit when the response was lost. Hosted version allocation, duplicate-name handling, exact stored-SQL layout, and retry deduplication remain unresolved. [L2, U5–U7; inference]

## 4. Pause the identified legacy marketing project

### Identity and authorized scope

**Local:** `marketing-site-legacy` is **`wgtplxkqrzvnmbcdhfye`**, in `northwind-marketing`, created June 18, 2024. Current `marketing-site` is **`bdnfrjskvlqmzpxtwcgh`**, created January 12, 2026. Both initially have `ACTIVE_HEALTHY` status; both storefront projects must also remain untouched. Resolve name, organization, and stable ref together, not “first marketing match” or an invented relative-age criterion. The organization fixture identifies `northwind-marketing` as **free**, consistent with the documented Free-plan pause restriction. [L7, S7]

Only `pause_project({"project_id":"wgtplxkqrzvnmbcdhfye"})` is the established exposed mutation. Deletion, rename, recreation, plan changes, restoring another project, or manipulating application SQL cannot substitute for the requested pause. [L2, L7, U4]

### Desired, submitted, and completed are different

| Stage | Sufficient evidence / reporting |
| --- | --- |
| **Desired** | The user requested the exact legacy project stop running; no operation has necessarily occurred. |
| **Submitted/accepted** | The pause endpoint returned successfully. Pinned MCP returns `{success:true}` immediately after that awaited request and performs **no status polling**. This alone does not assert a terminal status. [U4, U6] |
| **In progress** | Supabase documents `PAUSING` and `GOING_DOWN` as transitional statuses. Their existence does not establish one mandatory sequence for every pause. [S8] |
| **Completed** | A subsequent `get_project({"id":"wgtplxkqrzvnmbcdhfye"})` or `list_projects({})` observes the grounded paused/inactive outcome. The local scenario's terminal target is `INACTIVE`; a pause request, queued job, or desired-state field is not equivalent. [L2, L7, S8] |
| **Failed/unknown** | `PAUSE_FAILED`, a rejected request, unresolved timeout, or no terminal observation is not completed success. Report the actual stage rather than fabricating `INACTIVE`. [S8; recommendation] |

**Documented preservation:** restoring a paused project returns its previous data and configurations; the pausing guide discusses backups with limited retention. **Inference:** pause is a lifecycle stop, not deletion or permission to clear rows, credentials, functions, buckets, storage configuration, or project identity. Do not infer the exact snapshot mechanism, promise zero total organization billing, or assign undocumented per-component shutdown/completion times. [S7]

**Pinned errors/retries:** the inspected platform performs one pause POST, then its success assertion; no application-level completion wait or retry loop appears on that path. Its error helper specially reports 401, otherwise surfaces an API `message` or fallback “Failed to pause project.” `pause_project` is captured as `idempotentHint:false`; neither docs nor that annotation provide an exactly-once/deduplication guarantee. [U4, U6, U8, L2]

**Recommended error behavior:** after an ambiguous response, read the exact project before retrying. If already inactive, verify and report that outcome; if pausing, observe progress rather than repeatedly issuing mutations. Permission/not-found/plan failures require diagnosis, not retrying a different project or changing its plan. Rate-limit/transient retries should be bounded and respect server guidance; do not invent status-code-specific guarantees, retry counts, or a completion deadline absent source support. No live retry behavior was tested. [S6–S8; recommendation]

**Preservation:** compare every other project's complete admitted state, not only its status. For the target, allow only the pause operation and evidenced lifecycle consequences; preserve identity, organization, region, configuration, schema, and business data. A fixed blanket “all metadata unchanged” rule could wrongly reject genuine lifecycle timestamps, whereas unrestricted target-project changes would accept configuration destruction. Define the narrow operation-derived delta explicitly. [L7, S7; recommendation]

## 5. Full-state/history objections for the next evaluator

These are independent contract objections, **not findings from reading evaluator code**:

| Existing scenario-source signal | Why it is insufficient alone | Bounded next-contract requirement |
| --- | --- | --- |
| Cancellation checks selected IDs/statuses, one total, and a query-route call. [L3] | A read-only query satisfies route presence; extra eligible rows can be missed, and unchecked monetary/other rows can be changed. | Derive all eligible rows from admitted state; require exact per-row allowed delta and unchanged complements. Accept equivalent legitimate SQL/tool routes with real effects. |
| RLS checks a few flags/rows, `policies = []`, and a migration-route call. [L4] | A successful request or flag alone does not prove real migration history, truthful stored SQL, or schema/history atomicity. | Tie committed RLS and no policies to authentic appended history; preserve all prior history/business state; model failure rollback. |
| Pause checks terminal target status, three other statuses, and pause-route presence. [L7] | Desired-state writes, false immediate completion, or unrelated config mutations can escape such probes. | Separate request acceptance from lifecycle completion; verify exact target and full unchanged other-project/config state. |
| A correct final state without operation history. | Paid/shipped or another project could have been changed and later “restored”; duplicate migrations or failed partial writes can be hidden. | Inspect committed mutation history plus full final state; distinguish rejected/rolled-back attempts from actual side effects. |

Do not require a particular spelling, SQL return shape/order, migration name/version, read-call sequence, or fixed number of retries that the unchanged instruction does not require. Conversely, semantic equivalence is not permission to accept fabricated history or a submitted cloud job as completed. Harness audit/operation records can legitimately grow; business-state preservation and infrastructure-history preservation need separate, explicit rules. [L2–L7, P1, S5, S8; recommendations]

### Unresolved facts to settle before implementation

1. World's actual PostgreSQL/subset support, transaction/error behavior, precision, privileges, triggers, and fixture-projection defaults: intentionally not inspected. Native MCP pin does not settle these.
2. Hosted Management API's exact version allocation, statement serialization, duplicate-name/deduplication behavior, and tracking-infrastructure initialization atomicity: not fixed by the reviewed public guarantees or external CLI example.
3. Whether future variants define unpaid beyond pending, and whether a cancellation data migration's additional honest history is allowed: explicit benchmark policy needed without rewriting the user instruction.
4. Pause transition sequence, permitted lifecycle metadata delta, completion delay/failure model, and repeated-request responses: need an explicit bounded world contract, not an invented hosted guarantee.
5. Raw advisor projection lacks the described security notice, and invoices projection omits policies. Treat these as observation/projection questions; do not “repair” instructions or fixtures from this research note.

## Sources and local provenance

External sources are first-party PostgreSQL/Supabase documentation, first-party upstream source, and publisher registry metadata, retrieved November 6, 2026. CLI v2.65.5 is a deliberately separate versioned representation example, **not a catalog dependency**.

### Local inputs

- **[L1]** [`catalog/services.toml:1`](../../catalog/services.toml#L1), catalog mode definitions; [`catalog/services.toml:331`](../../catalog/services.toml#L331), Supabase native-server pin/world redirect; adjacent MongoDB replica boundary at line 349.
- **[L2]** [`servers/snapshots/supabase.json:1`](../../servers/snapshots/supabase.json#L1), captured server version `0.15.0`, 36 tools; relevant definitions start at lines 78, 95, 250, 274, 298, 363, 387, 421, 492. Snapshot's recorded SHA-256: `a4906781585b6ca811f4ba43b4290184e59a074cdd0d847279a0ef3fa5f3e86c`.
- **[L3]** Cancellation [`instruction.md:1`](../../scenarios/cloud/supabase/cancel-pending-orders-duplicate-customer/instruction.md#L1), [`scenario.toml:1`](../../scenarios/cloud/supabase/cancel-pending-orders-duplicate-customer/scenario.toml#L1); generated [`instruction.md:1`](../../tasks/cloud__supabase__cancel-pending-orders-duplicate-customer/instruction.md#L1), [`task.toml:1`](../../tasks/cloud__supabase__cancel-pending-orders-duplicate-customer/task.toml#L1). Both instruction SHA-256 values: `de8935a696da695f3527f9c925919e7e01208b52f10cc8f3da5e9412eb1bef4a`.
- **[L4]** RLS [`instruction.md:1`](../../scenarios/cloud/supabase/enable-rls-flagged-table/instruction.md#L1), [`scenario.toml:1`](../../scenarios/cloud/supabase/enable-rls-flagged-table/scenario.toml#L1); generated [`instruction.md:1`](../../tasks/cloud__supabase__enable-rls-flagged-table/instruction.md#L1), [`task.toml:1`](../../tasks/cloud__supabase__enable-rls-flagged-table/task.toml#L1). Both instruction SHA-256 values: `9c51c7be86860e5565e9f3610b8074485a1f11824cabeb0f543950b0e9ce6e7e`.
- **[L5]** Cancellation fixture: [`projects.json:1`](../../scenarios/cloud/supabase/cancel-pending-orders-duplicate-customer/dataset/supabase/projects.json#L1), [`tables.json:1`](../../scenarios/cloud/supabase/cancel-pending-orders-duplicate-customer/dataset/supabase/tables.json#L1); narrowly relevant customer/order projections: [`production customers:58`](../../scenarios/cloud/supabase/cancel-pending-orders-duplicate-customer/dataset/supabase/db.qmvbxtzkplrnwdhyfaco.public.customers.json#L58), [`production orders:1`](../../scenarios/cloud/supabase/cancel-pending-orders-duplicate-customer/dataset/supabase/db.qmvbxtzkplrnwdhyfaco.public.orders.json#L1), [`staging customers:1`](../../scenarios/cloud/supabase/cancel-pending-orders-duplicate-customer/dataset/supabase/db.hzrkcwpxnqdlmvbtejsa.public.customers.json#L1), [`staging orders:1`](../../scenarios/cloud/supabase/cancel-pending-orders-duplicate-customer/dataset/supabase/db.hzrkcwpxnqdlmvbtejsa.public.orders.json#L1).
- **[L6]** RLS fixture only: [`tables.json:140`](../../scenarios/cloud/supabase/enable-rls-flagged-table/dataset/supabase/tables.json#L140), [`advisors.json:1`](../../scenarios/cloud/supabase/enable-rls-flagged-table/dataset/supabase/advisors.json#L1), [`migrations.json:1`](../../scenarios/cloud/supabase/enable-rls-flagged-table/dataset/supabase/migrations.json#L1), [`projects.json:1`](../../scenarios/cloud/supabase/enable-rls-flagged-table/dataset/supabase/projects.json#L1).
- **[L7]** Pause [`instruction.md:1`](../../scenarios/cloud/supabase/pause-legacy-marketing-project/instruction.md#L1), [`scenario.toml:1`](../../scenarios/cloud/supabase/pause-legacy-marketing-project/scenario.toml#L1), [`projects.json:1`](../../scenarios/cloud/supabase/pause-legacy-marketing-project/dataset/supabase/projects.json#L1), [`organizations.json:14`](../../scenarios/cloud/supabase/pause-legacy-marketing-project/dataset/supabase/organizations.json#L14); generated [`instruction.md:1`](../../tasks/cloud__supabase__pause-legacy-marketing-project/instruction.md#L1), [`task.toml:1`](../../tasks/cloud__supabase__pause-legacy-marketing-project/task.toml#L1). Both instruction SHA-256 values: `f09fe263f95dcc255c67d00dca2d51be01e2e12a0f7ec402e52e8df7ff7611d5`.

Generated task TOMLs, `tests/task-context.json` image/context metadata, and environment compose descriptors were read for these three tasks only. No generated verifier program, local Supabase service code, SQL implementation, or MongoDB campaign artifact was inspected.

### Pinned MCP/package evidence

- **[U1]** [Publisher registry metadata, 0.15.0](https://registry.npmjs.org/@supabase%2fmcp-server-supabase/0.15.0).
- **[U2]** [Published provenance statement](https://registry.npmjs.org/-/npm/v1/attestations/@supabase%2fmcp-server-supabase@0.15.0).
- **[U3]** [Exact package archive](https://registry.npmjs.org/@supabase/mcp-server-supabase/-/mcp-server-supabase-0.15.0.tgz).
- **[U4]** [Pinned API platform, account operations and database routing](https://github.com/supabase/mcp/blob/0edc736f256871b1d539a320def43a001bd8c7a8/packages/mcp-server-supabase/src/platform/api-platform.ts#L135).
- **[U5]** [Pinned API platform, migration listing/apply and discarded response](https://github.com/supabase/mcp/blob/0edc736f256871b1d539a320def43a001bd8c7a8/packages/mcp-server-supabase/src/platform/api-platform.ts#L236).
- **[U6]** [Pinned account tool implementation, discovery and pause success](https://github.com/supabase/mcp/blob/0edc736f256871b1d539a320def43a001bd8c7a8/packages/mcp-server-supabase/src/tools/account-tools.ts#L442).
- **[U7]** [Pinned database tools](https://github.com/supabase/mcp/blob/0edc736f256871b1d539a320def43a001bd8c7a8/packages/mcp-server-supabase/src/tools/database-operation-tools.ts#L453), [migration output schema](https://github.com/supabase/mcp/blob/0edc736f256871b1d539a320def43a001bd8c7a8/packages/mcp-server-supabase/src/platform/types.ts#L130).
- **[U8]** [Pinned management client and error assertions](https://github.com/supabase/mcp/blob/0edc736f256871b1d539a320def43a001bd8c7a8/packages/mcp-server-supabase/src/management-api/index.ts#L14).

### PostgreSQL 17

- **[P1]** [UPDATE: retained columns, FROM, WHERE, RETURNING, join ambiguity](https://www.postgresql.org/docs/17/sql-update.html).
- **[P2]** [Scalar subqueries](https://www.postgresql.org/docs/17/sql-expressions.html#SQL-SYNTAX-SCALAR-SUBQUERIES).
- **[P3]** [Data-modifying WITH statements and same-row double modification](https://www.postgresql.org/docs/17/queries-with.html#QUERIES-WITH-MODIFYING).
- **[P4]** [Exact numeric versus inexact floating-point types](https://www.postgresql.org/docs/17/datatype-numeric.html).
- **[P5]** [Read Committed snapshots and concurrent UPDATE predicate recheck](https://www.postgresql.org/docs/17/transaction-iso.html#XACT-READ-COMMITTED).
- **[P6]** [Row security, default deny, owners, BYPASSRLS and FORCE](https://www.postgresql.org/docs/17/ddl-rowsecurity.html).
- **[P7]** [Trigger behavior](https://www.postgresql.org/docs/17/trigger-definition.html).
- **[P8]** [pg_class, including relrowsecurity and relforcerowsecurity](https://www.postgresql.org/docs/17/catalog-pg-class.html).
- **[P9]** [pg_policies](https://www.postgresql.org/docs/17/view-pg-policies.html).
- **[P10]** [EXISTS and IN subquery expressions](https://www.postgresql.org/docs/17/functions-subquery.html).
- **[P11]** [Lexical rules: identifiers, keyword case, comments](https://www.postgresql.org/docs/17/sql-syntax-lexical.html).
- **[P12]** [Query ordering requires an explicit sort](https://www.postgresql.org/docs/17/queries-order.html).

### Supabase product/API guarantees and separate CLI example

- **[S1]** [RLS guide, enabling and bypass caveats](https://supabase.com/docs/guides/database/postgres/row-level-security#bypassing-row-level-security).
- **[S2]** [Service-role API-key context and user Authorization troubleshooting](https://supabase.com/docs/guides/troubleshooting/why-is-my-service-role-key-client-getting-rls-errors-or-not-returning-data-7_1K9z).
- **[S3]** [Security advisor 0013: RLS disabled in public](https://supabase.com/docs/guides/database/database-linter?lint=0013_rls_disabled_in_public), [current first-party lint definition](https://github.com/supabase/splinter/blob/main/lints/0013_rls_disabled_in_public.sql) (not MCP-pinned).
- **[S4]** [Security advisor 0008: RLS enabled, no policy](https://supabase.com/docs/guides/database/database-linter?lint=0008_rls_enabled_no_policy), [current first-party lint definition](https://github.com/supabase/splinter/blob/main/lints/0008_rls_enabled_no_policy.sql) (not MCP-pinned).
- **[S5]** [Supabase for Platforms: migration endpoint, automatic tracking and rollback](https://supabase.com/docs/guides/integrations/supabase-for-platforms#make-database-changes).
- **[S6]** [Management API reference](https://supabase.com/docs/reference/api/introduction), [pause operation](https://supabase.com/docs/reference/api/v1-pause-a-project), [migration apply operation](https://supabase.com/docs/reference/api/v1-apply-a-migration).
- **[S7]** [Pausing: Free-plan restriction, backup retention and restored data/configuration](https://supabase.com/docs/guides/platform/free-project-pausing).
- **[S8]** [Official lifecycle/status and paused-event semantics](https://supabase.com/docs/guides/platform/webhooks/events#v1projectstatuschanged).
- **[S9]** [Single migration and stored SQL response](https://supabase.com/docs/reference/api/v1-get-a-migration), [minimal history-list response](https://supabase.com/docs/reference/api/v1-list-migration-history).
- **[C1]** [CLI v2.65.5 history schema and statements storage](https://github.com/supabase/cli/blob/v2.65.5/pkg/migration/history.go#L12).
- **[C2]** [CLI v2.65.5 statement execution and transactional history insertion](https://github.com/supabase/cli/blob/v2.65.5/pkg/migration/file.go#L72).
