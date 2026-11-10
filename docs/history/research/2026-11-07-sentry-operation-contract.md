# Sentry write-operation contract

Research date: **November 7, 2026**. Scenario business time: **October 20, 2026, 16:00 UTC**. This is a proposed implementation/evaluation contract, not a report of live Sentry operations.

## Evidence boundaries

- **[D] Documented:** current official Sentry documentation/API descriptions retrieved for this research. These pages are mutable; they do not establish what the hosted service did on October 20, 2026.
- **[P] Pinned client:** `@sentry/mcp-server@0.42.0`, published archive SHA-1 `f1b25ee14752986b6ec0726c916c140c9f8bb588`. The downloaded archive was inspected in memory, not installed or executed. Its package identity and relevant bundled behavior were checked against release tag `0.42.0`, commit `54f66d8d77bdee0d036d4754806d782b58217ef7` in the upstream repository, now named `getsentry/toolkit`. Package metadata does not supply a `gitHead`; the tag is a source reference, not an assertion that npm attests that commit.[^package][^tag]
- **[U] Upstream implementation:** Sentry server source at immutable commit `04018d5f3fda0202ddef087830444d92189358b3`, selected during this research. This explains legitimate implementation behavior but is **not** the benchmark backend, a version pin for Sentry Cloud, or evidence of the October 20 deployment.[^server-revision]
- **[L] Local facts:** only the Sentry catalog entry, tool snapshot, the three scenario instructions/TOMLs, and narrow team/project/key/issue/release fixture projections were read. TOML expectations and solution calls are benchmark assertions, **not vendor evidence**.[^local-catalog][^local-snapshot][^local-create][^local-worker][^local-storefront]
- **[I] Recommendation/inference:** proposed evaluator rules below, explicitly derived from the preceding evidence rather than claimed as Sentry guarantees.

No evaluator/backend implementation or local tests were inspected. No account API calls, credentials, resource mutations, package installation, benchmark/model runs, or Docker changes were performed. Public documentation, registry artifacts, and upstream source were fetched unauthenticated. No historical documentation snapshot or hosted behavior was verified.

## 1. Workflow targets: local facts, not global Sentry invariants

All three scenarios use `lumen-labs` and business time `2026-10-20T16:00:00Z`.[^local-create][^local-worker][^local-storefront]

| Workflow | Authorized outcome and disambiguation |
| --- | --- |
| Create notification project | **[L]** Create a **new** project named `notification-service` for the Node.js service, owned by Payments, and write its DSN to `/app/answer.md`. Payments is `payments`, ID `4507219`; `payments-infra`, ID `4507223`, is a different team. No notification project exists in the permitted initial project projection. The supplied solution chooses `platform: "node"`.[^local-create][^local-create-fixtures] |
| Resolve worker timeout | **[L]** Target `PAYMENTS-WORKER-1K`, numeric ID `6184417730`, project `payments-worker`, title containing `could not obtain connection from pool`. Its initial assignee is Payments Infra (`4507223`), and its status is unresolved. The fix is already deployed in `payments-worker@1.15.0`. Preserve `CHECKOUT-API-3F` / `6184420311` and `PAYMENTS-WORKER-1H` / `6184417704`; preserve the target's assignment.[^local-worker][^local-worker-fixtures] |
| Triage storefront map error | **[L]** Target `STOREFRONT-WEB-8D` / `6184421045`, project `storefront-web`, **first** release `storefront@5.3.0`; its last release is already `storefront@5.3.1`. Assign Frontend Web (`frontend-web`, ID `4507214`) and resolve **in the next release**. Preserve older same-project `STOREFRONT-WEB-7Q` / `6184420122` (first release `5.2.0`) and other-project `ADMIN-DASHBOARD-2M` / `6184421050`. The target initially has no assignee.[^local-storefront][^local-storefront-fixtures] |

**[I]** Match issue identity by organization + project + numeric/short-ID relation, and use first-release evidence for storefront. Title alone, last release alone, a team display-name substring, or an issue search that happens to return one result is insufficient. The fixture uses release-version strings; current API detail schemas describe `firstRelease`/`lastRelease` as objects, so normalize the version rather than requiring identical fixture/API shapes.[^local-worker-fixtures][^local-storefront-fixtures][^group-serializer]

## 2. New project, ownership, client key, and DSN

### Documented ownership and creation routes

1. **[D]** `POST /api/0/teams/{organization_id_or_slug}/{team_id_or_slug}/projects/` creates a project bound to that team. Organization/team path components can be IDs or slugs. `name` is required; `slug` is optional and otherwise generated from the name; `platform` is optional. Projects can have multiple teams: the API exposes a `teams` collection, not an exclusive single-team ownership type.[^create-project][^add-team]
2. **[U]** The ordinary team-create implementation creates the project in the team's organization, adds that team, applies default project settings, and emits project-created signals. This supports the expected Payments association, but not a universal claim that every project can have only one owner/team.[^team-create-source]
3. **[D]** There is also `POST /api/0/organizations/{organization_id_or_slug}/projects/`. Its documented behavior creates a **personal team** for the caller and binds the project to it. It is not a team-neutral equivalent of creating directly under Payments. `POST /api/0/projects/{org}/{project}/teams/{team}/` separately grants team access.[^org-create-project][^add-team]
4. **[I]** Prefer direct creation under existing Payments. A vendor-valid organization-create/add-team sequence is not automatically task-compliant: it introduces a personal team and potentially additional ownership. If alternative routes are supported, check final Payments ownership and explicitly adjudicate those extra resources; do not pass solely because Payments was eventually added. Never repurpose an existing unrelated project to satisfy “new.” The local exact `teams = ["payments"]` expectation is a fixture/task policy, not the vendor's general ownership model.[^local-create][^create-project][^org-create-project][^add-team]

### What automatic DSN creation does—and does not—guarantee

- **[D]** Sentry's DSN concept page says project creation automatically provides a DSN. Its project-create API page, however, returns project data and does **not** specify an atomic guarantee of exactly one active client key named `Default`, a returned DSN field, or a fixed key/project ID.[^dsn][^create-project]
- **[U]** The current ordinary project post-save receiver creates a key labeled `Default` when default-key creation is enabled, the save is new/non-raw, and no key exists. The source also contains an explicit default-key-creation disabling context. Treat this as evidence of the normal implementation, not an unconditional hosted guarantee. The key model normally generates fresh public/secret material on save.[^default-key-source][^key-model]
- **[P]** `create_project` does **more than project POST**: after creating the project it lists client keys, prefers an active `Default` with a nonempty public DSN, otherwise accepts another active usable key, and otherwise attempts `POST .../keys/` with `name: "Default"`. A key-list error also reaches that fallback. Key setup errors are caught; the tool can return successful project creation with `SENTRY_DSN` unavailable and suggest `create_dsn` recovery.[^mcp-create-project]
- **[I]** Therefore do not require a separate agent-issued key-create tool call, forbid the wrapper's fallback key POST, or assume the initial project POST alone guarantees the deliverable. A failed list followed by key creation can create another key even if automatic provisioning already succeeded; exact key count and key-list success must not be invented. Project-only success without a usable DSN in the requested answer file is incomplete.[^mcp-create-project][^local-create]
- **[D/P]** Listing and explicitly creating project client keys are legitimate routes: `GET` / `POST /api/0/projects/{org}/{project}/keys/`. The API creates server-generated public/secret key material and permits an optional key name; the pinned `create_dsn` requires a name and returns a public DSN. Pinned `find_dsns` returns key ID, name, and public DSN, but drops `isActive`; its existence alone is not evidence that a selected key is active.[^list-keys][^create-key][^mcp-dsns]
- **[P/I]** The pin provides `create_dsn` for an existing project, `update_dsn` to rename/activate/deactivate an existing key, and `add_team_to_project` with a current-access pre-check. The recovery recommendation is to reuse a suitable active key first, following `create_project`'s preference. These capabilities do not authorize unrelated project/key changes or gratuitous redundant resources.[^mcp-dsns][^mcp-update-dsn][^mcp-add-team][^mcp-create-project]

### Identity and DSN representations

**[D/P/U]** Use the actual created project's returned identity and an actual active key bound to it. API examples serialize project IDs as strings; the pinned schemas accept string or numeric project/key IDs, and the current key serializer exposes a client-key API `id` equal to its public key. Internal numeric key primary keys, public keys, labels, project IDs, and local storage keys such as `notification-service/Default` are not interchangeable identifiers.[^create-project][^mcp-schema][^key-serializer][^local-create-fixtures]

**[D]** A DSN encodes protocol, public key, ingestion host, optional path, and project ID; the secret component is optional/deprecated. Both self-hosted-style URLs and organization-specific ingestion hosts are legitimate. Client-key API responses distinguish `dsn.public`, `dsn.secret`, and specialized ingestion endpoints such as CSP/minidump URLs; those specialized endpoints are not interchangeable SDK DSNs. More than one client key/DSN can belong to one project.[^dsn][^list-keys][^create-key]

**[I] Implementation acceptance:**

- Find the new project relationally: absent initially, present finally in the intended org, correct name/Node.js platform intent, intended team relationship. Prefer the natural `notification-service` slug, but do not confuse a requested project **name** with a vendor-guaranteed slug or ID allocation rule.[^local-create][^create-project]
- Extract the DSN from `/app/answer.md` regardless of surrounding Markdown, code fencing, or an environment-assignment label. Parse it as a URL and verify public key + project-ID binding to an active key for that newly created project, using its recorded/returned ingestion endpoint. Prefer `dsn.public`; a deprecated secret-bearing DSN is not structurally invalid, although unnecessarily exposing the secret is undesirable.[^local-create][^dsn][^key-serializer]
- Do not require the next project ID to be `4508812006`, a precomputed public key, label `Default` as the only valid DSN source, or globally require `@world:8113/`. Those are local TOML/sample-answer assumptions, not Sentry's identity contract. Conversely, arbitrary external URLs are not valid merely because they look like DSNs: preserve provenance and binding. The configured test-world ingestion address can remain a fixture-specific constraint, distinctly labeled as such.[^local-create][^dsn][^mcp-create-project]

## 3. Issue status, next-release resolution, and assignment

### Status is not the request spelling

| Intent | Documented/pinned request | Required semantic distinction |
| --- | --- | --- |
| Worker fix already live | **[P]** `update_issue(..., status="resolved")`; omit `assignedTo` | **[I]** Target is resolved now, not newly placed into a pending-next-release workflow; preserve Payments Infra assignment and all look-alikes.[^mcp-update-issue][^local-worker] |
| Storefront fix not deployed | **[P]** `update_issue(..., status="resolvedInNextRelease", assignedTo="team:4507214")` | **[D/U]** Issue is resolved with release-aware future-resolution intent. Persisted/API `status` is ordinarily `resolved`, not the literal action string `resolvedInNextRelease`.[^mcp-update-issue][^resolution-source][^group-serializer] |

- **[D]** Plain resolution and release-aware resolution differ in regression behavior. Current docs describe plain resolution as treating later occurrences as regressions. Next-release resolution records release context and handles semver/date-based ordering; it is not an instruction to create a release or deploy software.[^issue-status][^releases]
- **[D/U]** The mutation API also accepts `statusDetails`; its nested schema includes `inNextRelease`, `inRelease`, and `inCommit`. For a fresh unresolved target, `status:"resolved"` with `statusDetails:{"inNextRelease":true}` reaches the same upstream next-release branch as `status:"resolvedInNextRelease"`. Release-bearing status details are restricted to one project. Explicit resolution in an already existing shipped release is another API capability, not a next-release action.[^update-issue][^status-details-source][^resolution-source]
- **[P]** These API alternatives do **not** mean the pinned MCP tool accepts a `statusDetails` argument: it does not expose one. The pinned tool forwards its status action, and the server derives release metadata. Do not invent a `resolve_in_release` tool or a generic HTTP escape hatch.[^mcp-update-issue][^mcp-client][^mcp-catalog]
- **[U]** On the ordinary pending-next-release path, resolution produces `status:"resolved"` and `statusDetails.inNextRelease:true`; additional actor metadata can be present. Resolution clears the unresolved substatus (`new`/`ongoing`) to null. Immediate plain resolution normally has no release-specific status details; requiring an unchanged `"new"`/`"ongoing"` substatus would be wrong.[^resolution-source][^group-serializer]
- **[U, important limitation]** An exact final `inNextRelease:true` is **not universal**. `process_group_resolution` can convert a next-release request into an `in_release` record when a semver anchor or an already-known next release is available. The read serializer then emits `inRelease` instead of `inNextRelease`. With no usable release, the source also does not guarantee a persisted release-resolution record merely because the update delta names next-release intent. Do not equate one response delta with durable full-detail state.[^resolution-source][^group-serializer]
- **[I]** For this benchmark, either explicitly model a stable pending-next-release abstraction and document that simplification, or validate request intent plus the resulting release-resolution relationship. Do not blindly broaden acceptance to **any** `resolved`/`inRelease` record: it must be demonstrably derived from the authorized next-release operation, not an agent choosing ordinary resolution. Do not fabricate the next storefront version or stamp resolution into `5.3.0`, which is target-identification evidence, not the future fixed release.[^local-storefront][^resolution-source][^releases]

### Team assignment: stable identity first

- **[D]** The issue-update API documents `team:<team_id>` alongside user forms. Bare numeric IDs are user identities, not an alternative team syntax. Use `team:4507214` for Frontend Web in this fixture; neither Sofia nor a similarly named team is the requested assignee.[^update-issue][^actor-parser][^local-storefront-fixtures]
- **[P]** The pinned tool advertises `team:ID_OR_SLUG`, passes that string through, and accepts `assignedTo:null` to unassign. Omission leaves assignment unrequested; the client serializes null as the API's empty-string unassignment representation. Do not treat omission and null as equivalent.[^mcp-update-issue][^mcp-client]
- **[U, compatibility objection]** Current upstream `Actor.from_identifier` resolves a nonnumeric `team:...` suffix against **`Team.name`**, not `Team.slug`. The team model keeps separate name/slug fields. Thus the pinned tool's advertised slug spelling is not independently established as universally accepted by the server source, particularly with this fixture's `name:"Frontend Web"` versus `slug:"frontend-web"`. Numeric team ID is the documented, unambiguous route. The local solution's `team:frontend-web` is a client/fixture choice, not proof of hosted acceptance.[^actor-parser][^team-model][^local-storefront][^local-storefront-fixtures]
- **[U/P]** The normal serialized team assignee is `{type:"team", id:"4507214", name:"frontend-web"}`: actor `name` is the **slug**, unlike the fixture team's display name `Frontend Web`. The pinned response parser additionally tolerates string assignees, numeric/string IDs, and extra fields. Resolve identity rather than demanding an exact three-field object or mistaking `name` for the human display label.[^actor-serializer][^mcp-schema][^local-storefront-fixtures]
- **[U]** Assignment is organization-validated and can also be constrained by project membership and organization settings. Existing project-team ownership and an issue's assignee are different relationships; assigning the storefront issue must not silently change project ownership.[^actor-parser][^assignment-validator][^add-team]

### Allowed automatic/derived effects, not arbitrary collateral

**[D/U]** “Only submitted attributes are modified” describes the public update surface; it does not eliminate domain side effects. Current resolution code updates resolution time, clears substatus, removes the issue from the inbox, maintains resolution/open-period/history/activity records, can subscribe the acting user, and can self-assign an **unassigned** issue according to user preferences. Assignment has its own activity/subscription behavior. These are conditional source facts, not promises of a particular audit JSON shape, timestamp, actor, count, or notification schedule.[^update-issue][^resolution-source][^assignment-source]

**[I]** Protect unrelated resources and independent business attributes, but allow only source-justified target-derived changes. In the worker fixture the target is already assigned, so the current self-assignment branch does not justify changing Payments Infra. In storefront, the explicitly requested final Frontend Web assignee remains mandatory even if resolution could otherwise self-assign. Do not grant blanket permission to alter priority, title, release observations, existing keys/projects, or other issues merely because “metadata may change.” No exact `resolvedAt`, `dateCreated`, audit-history entry schema, or number/order of activity records is supported as a benchmark requirement here.[^local-worker-fixtures][^local-storefront-fixtures][^resolution-source][^assignment-source]

## 4. Issue identities, valid routes, and repetition

### Route equivalence is about the affected identity set

| Route | Evidence and scope |
| --- | --- |
| `PUT /api/0/organizations/{org}/issues/{issue}/` | **[D/P/U]** Documented individual update and the pinned client's route. The client/parser and upstream resolver accept numeric issue IDs or organization-scoped qualified short IDs.[^update-issue][^mcp-client][^mcp-issue-parser][^group-resolver] |
| `PUT /api/0/issues/{numeric_id}/` | **[U]** Still registered as a legacy/deprecated route to the same individual endpoint. Short-ID resolution requires organization context; do not assume an unscoped short ID is equivalent. This route is not what the pinned client emits.[^issue-routes][^group-resolver][^group-endpoint] |
| `PUT /api/0/projects/{org}/{project}/issues/?id={numeric_id}` | **[D]** Documented project bulk mutation can target exactly one issue. Repeated `id` parameters select multiple issues. Omitting IDs for a status mutation can mean update-all; out-of-scope IDs can produce success without mutation.[^project-bulk] |
| `PUT /api/0/organizations/{org}/issues/?id={numeric_id}&project={project}` | **[D]** Documented organization bulk mutation, with project filtering and numeric issue-ID selection. Status updates without IDs can affect the filtered issue set; successful responses can also mean no mutation. This is not a separate exposed bulk-mutation MCP tool in the pin.[^org-bulk][^mcp-catalog] |

**[I]** A semantic evaluator should accept an individually scoped mutation, or an explicitly target-only documented bulk route if that surface is available, rather than hard-requiring one URI spelling. Validate selected IDs, filters, body, and final state—not HTTP success alone. Never admit a broad title-only update-all merely because only the target ultimately changed. Keep unintended writes to every look-alike forbidden. Canonicalization must retain project/org boundaries; numeric IDs and short IDs are aliases for a known identity, not freely interchangeable strings.[^project-bulk][^org-bulk][^group-resolver][^local-worker][^local-storefront]

### Retries, no-ops, and split updates

- **[P]** `update_issue` reads the current issue first, compares requested status (including the `inNextRelease` display distinction) and assignee, suppresses already-satisfied fields, and can return a no-change result with **no PUT**. Assignment comparison recognizes team numeric ID or the serialized team name. Combined status+assignment and two separate target-only updates are both supported.[^mcp-update-issue]
- **[P]** The client automatically retries certain gateway-failing **GETs** (502/503/504), but does not automatically replay POST/PUT mutations. The `update_issue` MCP annotation is `idempotentHint:false`; this is not evidence that every repeated business request necessarily mutates state.[^mcp-client][^mcp-update-issue]
- **[U]** Current resolution handling skips processing an already-resolved group, so a repeated update is not guaranteed to refresh timestamps/activity or change the resolution mode. Do not infer a guaranteed successful mode switch from a repeated 200-shaped response; inspect resulting state.[^resolution-source]
- **[P]** Optional `reason` posts a separate issue comment, even on the no-change path; comment failure can be reported separately after the issue update. A repeated tool call with `reason` is therefore not globally side-effect-free. None of these instructions requests a comment.[^mcp-update-issue][^local-worker][^local-storefront]
- **[I]** Accept harmless repeated reads, equivalent repeated target-only requests, and separate assignment/status updates when they meet the requested final semantics and do not introduce forbidden transient changes. Do not require exactly one mutation, exactly one MCP call, or an audit entry on a true no-op. Conversely, a create retry is not generically idempotent: re-read identity after uncertainty and do not accept duplicate projects/needless keys solely because the requests resemble retries. No undocumented conflict status, error text, retry token, or delivery guarantee is assumed.[^mcp-create-project][^mcp-update-issue][^create-project]

## 5. Actual pinned MCP surface

**[L/P]** The local catalog pins `@sentry/mcp-server` `0.42.0` and passes `--all-skills`. Its snapshot exposes exactly nine top-level tools, matching upstream surface policy: `find_organizations`, `find_projects`, `update_issue`, `search_events`, `analyze_issue_with_seer`, `search_issues`, `get_sentry_resource`, `search_sentry_tools`, and `execute_sentry_tool`.[^local-catalog][^local-snapshot][^mcp-surfaces]

**[P]** `--all-skills` grants active skills, including `triage` and `project-management`. `update_issue` belongs to `triage`; project/DSN creation and team-access management belong to `project-management`. `find_teams` is available under inspect/triage/project-management. Underlying Sentry API permissions still apply: enabling a skill is not a guarantee that remote authorization succeeds.[^mcp-skills][^mcp-cli][^mcp-update-issue][^mcp-create-project][^mcp-dsns][^mcp-find-teams][^update-issue][^create-project]

**[P]** `search_sentry_tools` discovers catalog operations available for the session. `execute_sentry_tool(name=..., arguments={...})` looks up an available catalog entry, injects constraints, validates its arguments, then invokes its handler. It is neither arbitrary HTTP nor unrestricted access to all names. Search is recommended for discovery, not a technical prerequisite if the correct executable name/schema is already known. Skills and constraints can hide otherwise registered operations.[^mcp-execute][^mcp-availability]

| Needed capability | Actual name and calling surface |
| --- | --- |
| Find owning/assigned team | `execute_sentry_tool(name="find_teams", arguments={organizationSlug:"lumen-labs", ...})`; result includes IDs and slugs.[^mcp-find-teams] |
| Create new team-owned project | Catalog `create_project`: required org/team/name; optional slug/platform/region/repository. For this task use existing Payments; no repository linking requested.[^mcp-create-project] |
| Re-read/create/recover a DSN | Catalog `find_dsns`, `create_dsn`, `update_dsn`; their HTTP methods operate on project keys. There is no catalog tool named `create_client_key` or `list_client_keys` merely because client methods have those meanings.[^mcp-dsns][^mcp-update-dsn][^mcp-catalog] |
| Adjust project team access | Catalog `add_team_to_project` / `remove_team_from_project`, distinct from `update_project` metadata. The add tool returns unchanged without POST when the team already has access.[^mcp-add-team][^mcp-catalog] |
| Inspect/select the exact issue | Direct `search_issues` / `get_sentry_resource`; catalog `get_issue_details`, `find_releases`, `get_release_details`, and `get_issue_activity` also exist.[^local-snapshot][^mcp-catalog] |
| Resolve/assign the issue | Direct `update_issue` or catalog execution of the same operation. No separate `resolve_issue`, `assign_issue`, `resolve_in_release`, `create_release`, or arbitrary-request tool appears in this pinned catalog.[^mcp-update-issue][^mcp-catalog][^mcp-surfaces] |

**[I]** Evaluate the underlying operation and target rather than the outer `execute_sentry_tool` name. Its broad mutation annotations do not turn `find_teams` into a write. Conversely, absence of `create_project` from top-level `tools/list` does not mean project creation is unavailable. Do not require Seer analysis: it is an unnecessary additional capability for these narrowly specified writes.[^mcp-execute][^mcp-find-teams][^mcp-surfaces][^local-create][^local-worker][^local-storefront]

## 6. Actionable evaluator objections and implementation decisions

These are **[I] recommendations**, not findings about uninspected evaluator code.

| Objection to existing scenario assertions or likely overconstraints | Implementable correction |
| --- | --- |
| Create TOML comment asserts every new project has a `Default` key; checks key by label and fixes DSN project ID at `4508812006`. | Discover the newly created project/key relationship; accept actual generated identity and usable active DSN. Permit automatic provisioning, wrapper fallback, and necessary explicit key recovery. Label/count rules need an explicit fixture policy, not a claimed vendor guarantee.[^local-create][^dsn][^mcp-create-project] |
| Exact team array or one create-path requirement is mistaken for a universal ownership invariant. | Keep Payments identity mandatory. Prefer team-create; separately model team-access routes and the org-create route's personal-team side effects. Do not blanket-accept extra teams/resources.[^local-create][^create-project][^org-create-project][^add-team] |
| Worker evaluation equates any “resolved” action with pending-next-release resolution or requires assignment to remain byte-identical while ignoring identity. | Require resolved-now intent/result and unchanged existing assignee identity; normalize serialized form without permitting a new owner.[^local-worker][^local-worker-fixtures][^mcp-update-issue][^resolution-source] |
| Storefront checks only `status:"resolved"`, or universally insists on final `statusDetails == {"inNextRelease":true}`. | Preserve the next-release intent and release-resolution relationship; allow extra derived actor details and justified server normalization. If testing a simplified persistent boolean, document that abstraction and its limits.[^local-storefront][^resolution-source][^group-serializer] |
| Only `team:frontend-web` or display name `Frontend Web` passes assignment. | Accept documented numeric team identity `4507214`; normal actor response `name` is the slug. Explicitly record the pinned-client/current-server slug-vs-name discrepancy instead of asserting hosted equivalence.[^local-storefront][^update-issue][^actor-parser][^actor-serializer] |
| Exactly one individual PUT URI, or exactly one tool invocation, is required. | Recognize numeric/short-ID aliases, safely scoped bulk alternatives where exposed, split updates, no-op suppression, and harmless repetitions. Validate actual affected identity set and final semantics; exclude broad update-all and unauthorized transient writes.[^local-worker][^local-storefront][^mcp-update-issue][^project-bulk][^org-bulk] |
| “Don't change anything else” rejects all derived target metadata or allows every metadata change. | Use a narrow operation-specific derived-effect allowance; preserve unrelated issues/resources and independent attributes. Do not demand fabricated fixed timestamps/history payloads, or use automatic self-assignment as an excuse to lose an explicitly required assignee.[^resolution-source][^assignment-source] |
| Success response, wrapper prose, or final-chat DSN alone proves completion. | Read semantic post-state and the requested answer artifact. Creation with unavailable DSN is partial; bulk success can be a no-op; “resolved” output does not establish durable next-release metadata.[^local-create][^mcp-create-project][^project-bulk][^org-bulk][^group-endpoint] |

### Explicit unresolved hosted behavior

1. Whether the relevant hosted Sentry deployment matches the inspected server revision, and whether any cited current behavior existed on **October 20, 2026**.
2. Which default-key hooks/settings are enabled, whether provisioning completes atomically/observably with project creation in that deployment, and the exact initial key count/name. Normal automatic DSN provisioning is documented; a stronger universal `Default` guarantee is not established.
3. Whether that deployment accepts a team **slug** when its separate name differs; numeric team ID is the documented alternative.
4. Which release-ordering/features/observed-release anchors cause a next-release request to serialize as `inNextRelease` versus `inRelease`, and how quickly automatic future-release processing occurs.
5. Acting-user identity/preferences, conditional subscriptions/self-assignment, exact audit/activity payloads, timestamps, error messages, and notification delivery.

These require deployment-specific evidence or separately authorized verification; no such operations were performed. Do not fill these gaps with local backend/test behavior or freeze November 7 documentation into invented October 20 guarantees.

## Sources

All external references are first-party Sentry documentation, Sentry-maintained source, or the published Sentry package. Source links are immutable where possible; documentation links describe the pages retrieved for this research.

[^package]: Published package metadata: https://registry.npmjs.org/@sentry%2fmcp-server/0.42.0 ; exact archive: https://registry.npmjs.org/@sentry/mcp-server/-/mcp-server-0.42.0.tgz
[^tag]: Release ref: https://api.github.com/repos/getsentry/toolkit/git/ref/tags/0.42.0 ; source tree: https://github.com/getsentry/toolkit/tree/54f66d8d77bdee0d036d4754806d782b58217ef7
[^server-revision]: https://github.com/getsentry/sentry/commit/04018d5f3fda0202ddef087830444d92189358b3
[^create-project]: https://docs.sentry.io/api/projects/create-a-new-project/
[^org-create-project]: https://docs.sentry.io/api/projects/create-a-project-for-an-organization/
[^add-team]: https://docs.sentry.io/api/projects/add-a-team-to-a-project/
[^dsn]: https://docs.sentry.io/concepts/key-terms/dsn-explainer/
[^create-key]: https://docs.sentry.io/api/projects/create-a-new-client-key/
[^list-keys]: https://docs.sentry.io/api/projects/list-a-projects-client-keys/
[^update-issue]: https://docs.sentry.io/api/events/update-an-issue/
[^project-bulk]: https://docs.sentry.io/api/events/bulk-mutate-a-list-of-issues/
[^org-bulk]: https://docs.sentry.io/api/events/bulk-mutate-an-organizations-issues/
[^issue-status]: https://docs.sentry.io/product/issues/states-triage/
[^releases]: https://docs.sentry.io/product/releases/#3-different-ways-to-resolve-issues
[^team-create-source]: https://github.com/getsentry/sentry/blob/04018d5f3fda0202ddef087830444d92189358b3/src/sentry/core/endpoints/team_projects.py#L272-L348
[^default-key-source]: https://github.com/getsentry/sentry/blob/04018d5f3fda0202ddef087830444d92189358b3/src/sentry/receivers/core.py#L134-L188 ; disabling context: same file, lines 35–46.
[^key-model]: https://github.com/getsentry/sentry/blob/04018d5f3fda0202ddef087830444d92189358b3/src/sentry/models/projectkey.py#L229-L251
[^key-serializer]: https://github.com/getsentry/sentry/blob/04018d5f3fda0202ddef087830444d92189358b3/src/sentry/api/serializers/models/project_key.py#L63-L145
[^resolution-source]: https://github.com/getsentry/sentry/blob/04018d5f3fda0202ddef087830444d92189358b3/src/sentry/api/helpers/group_index/update.py#L384-L684 ; subscription/self-assignment guard: same file, lines 137–155.
[^status-details-source]: https://github.com/getsentry/sentry/blob/04018d5f3fda0202ddef087830444d92189358b3/src/sentry/api/helpers/group_index/validators/status_details.py#L46-L125
[^group-serializer]: https://github.com/getsentry/sentry/blob/04018d5f3fda0202ddef087830444d92189358b3/src/sentry/api/serializers/models/group.py#L652-L710 ; issue detail release shapes: same file, lines 248–257.
[^actor-parser]: https://github.com/getsentry/sentry/blob/04018d5f3fda0202ddef087830444d92189358b3/src/sentry/types/actor.py#L184-L242 ; validation entry: same file, lines 311 onward.
[^actor-serializer]: https://github.com/getsentry/sentry/blob/04018d5f3fda0202ddef087830444d92189358b3/src/sentry/api/serializers/models/actor.py#L12-L57
[^team-model]: https://github.com/getsentry/sentry/blob/04018d5f3fda0202ddef087830444d92189358b3/src/sentry/models/team.py#L113-L138
[^assignment-validator]: https://github.com/getsentry/sentry/blob/04018d5f3fda0202ddef087830444d92189358b3/src/sentry/api/helpers/group_index/validators/group.py#L110-L125
[^assignment-source]: https://github.com/getsentry/sentry/blob/04018d5f3fda0202ddef087830444d92189358b3/src/sentry/api/helpers/group_index/update.py#L1080-L1120 ; assignment model: https://github.com/getsentry/sentry/blob/04018d5f3fda0202ddef087830444d92189358b3/src/sentry/models/groupassignee.py
[^group-resolver]: https://github.com/getsentry/sentry/blob/04018d5f3fda0202ddef087830444d92189358b3/src/sentry/models/group.py#L113-L185 ; endpoint organization binding: https://github.com/getsentry/sentry/blob/04018d5f3fda0202ddef087830444d92189358b3/src/sentry/issues/endpoints/bases/group.py#L49-L91
[^group-endpoint]: https://github.com/getsentry/sentry/blob/04018d5f3fda0202ddef087830444d92189358b3/src/sentry/issues/endpoints/group_details.py#L443-L481
[^issue-routes]: https://github.com/getsentry/sentry/blob/04018d5f3fda0202ddef087830444d92189358b3/src/sentry/api/urls.py#L953-L960 ; legacy route registration: same file, lines 3823–3826.
[^mcp-create-project]: https://github.com/getsentry/toolkit/blob/54f66d8d77bdee0d036d4754806d782b58217ef7/packages/mcp-core/src/tools/catalog/create-project.ts
[^mcp-dsns]: https://github.com/getsentry/toolkit/blob/54f66d8d77bdee0d036d4754806d782b58217ef7/packages/mcp-core/src/tools/catalog/create-dsn.ts ; listing: https://github.com/getsentry/toolkit/blob/54f66d8d77bdee0d036d4754806d782b58217ef7/packages/mcp-core/src/tools/catalog/find-dsns.ts
[^mcp-update-dsn]: https://github.com/getsentry/toolkit/blob/54f66d8d77bdee0d036d4754806d782b58217ef7/packages/mcp-core/src/tools/catalog/update-dsn.ts
[^mcp-add-team]: https://github.com/getsentry/toolkit/blob/54f66d8d77bdee0d036d4754806d782b58217ef7/packages/mcp-core/src/tools/catalog/add-team-to-project.ts
[^mcp-update-issue]: https://github.com/getsentry/toolkit/blob/54f66d8d77bdee0d036d4754806d782b58217ef7/packages/mcp-core/src/tools/catalog/update-issue.ts ; no-op comparisons and execution: lines 715–815; assignee comparison: lines 340–386.
[^mcp-client]: https://github.com/getsentry/toolkit/blob/54f66d8d77bdee0d036d4754806d782b58217ef7/packages/mcp-core/src/api-client/client.ts ; retry gate: lines 704–737; project creation: lines 1849–1883; keys: lines 2675–2785; issue update: lines 4412–4470.
[^mcp-schema]: https://github.com/getsentry/toolkit/blob/54f66d8d77bdee0d036d4754806d782b58217ef7/packages/mcp-core/src/api-client/schema.ts ; client keys: lines 524–555; assignee/issue schemas: lines 856–918.
[^mcp-issue-parser]: https://github.com/getsentry/toolkit/blob/54f66d8d77bdee0d036d4754806d782b58217ef7/packages/mcp-core/src/internal/issue-helpers.ts#L89-L166
[^mcp-catalog]: https://github.com/getsentry/toolkit/blob/54f66d8d77bdee0d036d4754806d782b58217ef7/packages/mcp-core/src/tools/catalog/index.ts#L96-L161
[^mcp-surfaces]: https://github.com/getsentry/toolkit/blob/54f66d8d77bdee0d036d4754806d782b58217ef7/packages/mcp-core/src/tools/surfaces.ts#L10-L30
[^mcp-skills]: https://github.com/getsentry/toolkit/blob/54f66d8d77bdee0d036d4754806d782b58217ef7/packages/mcp-core/src/skills.ts
[^mcp-cli]: https://github.com/getsentry/toolkit/blob/54f66d8d77bdee0d036d4754806d782b58217ef7/packages/mcp-server/src/cli/resolve.ts#L74-L103
[^mcp-execute]: https://github.com/getsentry/toolkit/blob/54f66d8d77bdee0d036d4754806d782b58217ef7/packages/mcp-core/src/tools/special/execute-tool.ts
[^mcp-availability]: https://github.com/getsentry/toolkit/blob/54f66d8d77bdee0d036d4754806d782b58217ef7/packages/mcp-core/src/tools/catalog-runtime/availability.ts
[^mcp-find-teams]: https://github.com/getsentry/toolkit/blob/54f66d8d77bdee0d036d4754806d782b58217ef7/packages/mcp-core/src/tools/catalog/find-teams.ts
[^local-catalog]: Local primary context: [`catalog/services.toml`](../../catalog/services.toml), Sentry entry (`version = "0.42.0"`, `--all-skills`).
[^local-snapshot]: Local primary context: [`servers/snapshots/sentry.json`](../../servers/snapshots/sentry.json), tool names, schemas, descriptions, and annotations.
[^local-create]: Local task/assertions: [`instruction.md`](../../scenarios/ops/sentry/create-notification-project/instruction.md), [`scenario.toml`](../../scenarios/ops/sentry/create-notification-project/scenario.toml).
[^local-worker]: Local task/assertions: [`instruction.md`](../../scenarios/ops/sentry/resolve-worker-pool-timeouts/instruction.md), [`scenario.toml`](../../scenarios/ops/sentry/resolve-worker-pool-timeouts/scenario.toml).
[^local-storefront]: Local task/assertions: [`instruction.md`](../../scenarios/ops/sentry/triage-storefront-map-error/instruction.md), [`scenario.toml`](../../scenarios/ops/sentry/triage-storefront-map-error/scenario.toml).
[^local-create-fixtures]: Narrow initial-state projections: [`teams.json`](../../scenarios/ops/sentry/create-notification-project/dataset/sentry/teams.json), [`projects.json`](../../scenarios/ops/sentry/create-notification-project/dataset/sentry/projects.json), [`keys.json`](../../scenarios/ops/sentry/create-notification-project/dataset/sentry/keys.json); key material was not included in the projection.
[^local-worker-fixtures]: Narrow issue identity/status/assignment/release projection: [`issues.json`](../../scenarios/ops/sentry/resolve-worker-pool-timeouts/dataset/sentry/issues.json).
[^local-storefront-fixtures]: Narrow issue/team/release projections: [`issues.json`](../../scenarios/ops/sentry/triage-storefront-map-error/dataset/sentry/issues.json), [`teams.json`](../../scenarios/ops/sentry/triage-storefront-map-error/dataset/sentry/teams.json), [`releases.json`](../../scenarios/ops/sentry/triage-storefront-map-error/dataset/sentry/releases.json).
