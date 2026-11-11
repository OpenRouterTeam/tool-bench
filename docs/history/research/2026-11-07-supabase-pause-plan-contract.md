# Supabase manual-pause plan contract (2026-11-07)

## Scope and conclusion

Independent, bounded research retrieved **November 7, 2026**, America/Chicago. This note rechecks the earlier memo's Free-plan assertion rather than treating it as evidence. Current documentation is not proof of the hosted implementation at the scenario's separate business date, April 2, 2026. [L1, L4]

**Principal finding:** Free-only eligibility for user-initiated pause is an **explicit documented product restriction**, not merely an inference from automatic inactivity pausing. Supabase currently says paid projects cannot be paused, and its manual Dashboard instructions independently restrict pause to Free projects. Pro projects therefore cannot be manually paused **under the documented policy while they remain paid**. The public REST operation schema does **not** specify its plan-enforcement algorithm, the paid-plan rejection status/message, or plan-change race behavior. Those wire-level facts remain unresolved. [D1–D4]

Evidence labels:

- **Documented:** explicit current first-party product documentation or public API schema.
- **Pinned implementation:** inspected upstream source associated with the catalog's MCP package; not the hosted control-plane implementation.
- **Local:** allowed catalog, snapshot, instruction/TOML, or narrow fixture observations.
- **Inference/recommendation:** reasoning from those sources, not an additional vendor guarantee.
- **Unresolved:** public evidence reviewed does not establish the fact.

Only this file was written. No local service/evaluator implementation was inspected. No instructions, fixtures, generated tasks, code, dependencies, or git state were changed; no subagents, installations, tests, benchmark runs, live-account calls, authenticated resource calls, or operations were performed. External retrieval was limited to public documentation/schema, publisher metadata, and upstream source.

## 1. Manual eligibility versus automatic inactivity policy

| Evidence | What it establishes | What it does not establish |
| --- | --- | --- |
| **Documented:** the pausing guide says, “Projects under a paid plan cannot be paused,” separately from its inactivity exemption, and directs paid users to transfer to a Free organization first. [D1] | A general pause restriction, not just an inactivity rule. | A particular HTTP rejection status or message. |
| **Documented:** the deletion guide's alternative explicitly describes clicking **Pause Project** and limits that action to Free projects. [D2] | The restriction includes user-initiated/manual pause. | A separate paid-plan API exception. |
| **Documented:** the Pro-pausing knowledge-base article, updated November 4, 2026, says Pro projects cannot currently be paused; it describes transferring an eligible project to a Free organization before initiating pause. [D3] | Direct corroboration specific to Pro and deliberate pausing. | That every Pro project can transfer, or that transfer itself performs pause. |
| **Documented:** webhook reasons distinguish `api_request` for user API/Dashboard pause from `inactivity` and `other`. [D5] | Manual and automatic pauses are distinct causes. | Different plan eligibility for the API versus Dashboard. |

**Documented, separate policy:** Free projects with insufficient database activity over seven days can be automatically paused. This is not a documented seven-day waiting requirement for a manual pause request. [D1]

**Inference boundary:** read together, the explicit manual restriction and shared API/Dashboard pause terminology support Free-only as the intended manual `POST /v1/projects/{ref}/pause` eligibility contract. The endpoint's short reference alone would not establish this. Conversely, omission of a plan condition from its schema, or from an MCP client, does not demonstrate paid support. No hosted paid-project request was made. [D2, D4, D5, U2, U3]

**Objection to the earlier memo:** its conclusion is supportable, but “Free projects pause for inactivity” would have been insufficient reasoning. The explicit manual/general restriction above supplies the missing distinction. This sidecar does not edit that memo. [L1, D1–D3]

## 2. Which plan is authoritative?

**Documented:** subscriptions belong to organizations. Projects in one organization cannot mix Free and Pro plans; upgrading an organization applies the paid plan's benefits to its projects. The authoritative **plan scope** is therefore the project's owning organization, not a standalone project tier. [D6]

| Surface | Documented or pinned representation | Consequence |
| --- | --- | --- |
| `GET /v1/projects/{ref}` | Identifies `organization_slug`; `organization_id` is deprecated in the current schema. No subscription-plan property is declared in its project response. [D7, S1] | Resolve the owner; do not invent `project.plan` as the public subscription source. |
| `GET /v1/organizations/{slug}` | Its `plan` property has enum `free`, `pro`, `team`, `enterprise`, `platform`; **`plan` is not required** in the current schema. [D8, S1] | This is the documented public organization-tier field when supplied. Missing plan is not documented to mean Free. |
| `GET /v1/organizations` | List entries declare `id`, `slug`, and `name`, not `plan`. [D9] | A successful organization list alone need not establish tier. |
| Project creation's legacy `plan` input | Deprecated; the current schema says organization-level subscription determines the plan and this input is ignored. [S1] | A requested historical project tier is not authoritative current billing evidence. |
| Pinned `get_organization({"id":"<organization-slug>"})` | MCP's argument is named `id`, but the platform forwards it as REST path parameter `slug`; its organization output makes `plan` optional. [U2, U4] | Preserve native tool argument names; do not substitute an invented `slug` MCP argument. |

**Unresolved:** these public representations do not reveal the hidden server's exact plan lookup, missing-value policy, cache freshness, transitional billing fields, or atomic relationship between an organization read and a later pause POST. The tier field is documented evidence of organization plan, not a serialized eligibility decision or concurrency token. [D8, S1; evidence limitation]

**Local:** the allowed fixture projection has `northwind-marketing.plan = "free"` and `northwind-labs.plan = "pro"`; the projected project records contain neither `plan` nor `subscription_plan`. The target `marketing-site-legacy` is `wgtplxkqrzvnmbcdhfye`, owned by `northwind-marketing`; current `marketing-site` is `bdnfrjskvlqmzpxtwcgh`. These are fixture observations, not a reason to synthesize missing plan fields or require an extra preflight call absent from the native pause path. [L5, U2]

## 3. What the pinned native MCP actually does

**Pin provenance:** the catalog selects `@supabase/mcp-server-supabase@0.15.0`. Publisher registry metadata and its decoded provenance associate that package with commit `0edc736f256871b1d539a320def43a001bd8c7a8`. Provenance signatures were not cryptographically verified, and no installation, build, or complete transitive-dependency audit was performed. The following statements concern that source association, not a hosted-server version. [L2, U1]

**Pinned implementation:** `pause_project` accepts `project_id`; its implementation rejects MCP read-only mode, awaits `account.pauseProject(project_id)`, then returns `{success:true}`. The account platform issues one `POST /v1/projects/{ref}/pause`, with the ref in the path, and calls `assertSuccess`. There is no organization-plan fetch/check, status polling, or application-level retry loop on this inspected path. This is **not proof** paid pause succeeds: eligibility can be enforced remotely. [L3, U2, U3]

**Pinned representation:** organization `plan` is optional; the project output schema has identity, organization, name, status, creation time, and region, but no plan. Neither the tool's `idempotentHint:false` annotation nor its `{success:true}` result supplies exactly-once execution or terminal-state verification. [L3, U3, U4]

## 4. Errors, acceptance, completion, and retries

**Documented REST surface:** the pause operation requires bearer authorization, OAuth scope `projects:write`, and the documented fine-grained permission `project_admin_write`. Its declared responses are `200`, `401`, `403`, and `429`. The public schema assigns no response content schema to those pause responses; the rendered reference shows an empty success-schema display, not a promise of a particular body or terminal project state. [D4, S1]

**Unresolved paid-plan error:** neither this operation definition nor the explicit product restriction maps paid eligibility failure to an exact status, error code, or message. A generic `403` listing does not establish that a Pro pause returns `403`; no exact `400`, `402`, `409`, `422`, or message is established either. Error precedence against permissions, missing projects, current lifecycle state, and concurrent plan changes is also unspecified. Do not invent an exact vendor rejection fixture from the eligibility policy. [D1–D4, S1; evidence limitation]

**Pinned error adaptation:** pause uses ordinary `assertSuccess`, not the separate project-scoped `403` helper. It specially formats `401`; otherwise it throws a parsed string `message` from the API or falls back to `Failed to pause project`. It does not preserve an HTTP status/structured error envelope as its thrown result. This cannot determine the server's plan-error text. [U2, U5]

**Documented lifecycle vocabulary:** `PAUSING` means pausing is in progress, `GOING_DOWN` means shutting down, `PAUSE_FAILED` means pause failed, and `INACTIVE` means inactive. The docs do not prescribe one mandatory sequence, deadline, or number of observations for manual pause. A successful MCP result is not independently observed completion. [D5, U3]

**Documented throttling:** the general Management API reference currently describes a standard 120 requests/minute limit per user/scope/endpoint, `429` until its window resets, and `X-RateLimit-Reset` in seconds. It recommends exponential backoff and using that header. This is general API guidance, not a manual-pause completion SLA or a prescribed MCP retry count. [D10]

**Recommendation, not guarantee:** after a timeout or lost response, reconcile the exact project before repeating a mutation. Observe progress rather than submitting duplicate pauses when already pausing; do not retry a policy rejection indefinitely or switch targets. The inspected contract supplies no idempotency key, duplicate-request behavior, exactly-once guarantee, or proof that an ambiguous transport failure left no committed effect. [L3, D4, U2, U3]

## 5. Plan transitions: established facts and limits

- **Documented:** subscription upgrades and downgrades take effect immediately. That billing statement does not specify ordering or transactional consistency against an already-running pause request, nor a propagation bound across API projections. [D11; limitation]
- **Documented:** transferring to a Free organization is the published path before pausing a formerly paid project. Transfers have ownership/membership and integration constraints, check Free-project limits, and can entail downtime or feature loss. The Pro-pausing article additionally describes an under-500-MB case. These are not an unconditional transfer guarantee or an instruction to add a universal pause-time size guard. [D1, D3, D12]
- **Documented:** upgrading while a project is already paused can reenable expired one-click restore options; the upgrade guide warns restoration may fail. **Inference:** the inability to initiate a new paid pause must not become a blanket invariant that every paid organization must always have an active project or that upgrading necessarily synchronously restores every paused project. Exact upgrade/pause race results remain unresolved. [D13]
- **Documented:** the billing FAQ separately describes pausing projects and downgrading the organization for overdue invoices, and lists pausing among possible fair-use restrictions. These system actions are not evidence that a paying user can invoke manual pause on a Pro project. Their control-plane ordering is not specified here. [D14]

**Local authorization boundary:** this task requests pausing the legacy project while leaving the current marketing site and every other project unchanged. It does not authorize transfers, subscription changes, deleting projects, or restoration as a workaround. Even a documented workaround is outside this instruction's scope. [L4]

## 6. Contract objections and remaining unknowns

1. **Keep the established distinction:** Free-only manual eligibility has affirmative first-party support; a manual inactivity threshold does not. Do not convert inactivity-pausing documentation into a manual waiting/activity guard. [D1–D3]
2. **Do not overspecify enforcement:** public evidence establishes product eligibility, but not a literal server-side `organization.plan !== "free"` algorithm, absent-plan default, exact rejection response, error precedence, or atomic plan-check instant. Free membership is not an unconditional success guarantee. [D4, D8, S1]
3. **Do not infer remote support from a thin client:** no MCP plan preflight is evidence about the wrapper, not paid eligibility. Do not require that preflight as a new native introspection contract. [U2–U4]
4. **Separate observation from mutation:** neither accepted POST nor `{success:true}` proves `INACTIVE`; no exact transition schedule or duplicate-call semantics were established. [D4, D5, U2, U3]
5. **Leave source conditions untouched:** the existing Free marketing organization is consistent with documented eligibility. This note adds no guards, changes no instruction or fixture, and makes no inference from a local evaluator, service, benchmark result, or authenticated experiment. [L4, L5]

Still unresolved: the hosted paid-plan rejection's exact status/message/body; repeated pause while inactive/pausing/failed; failure side effects; hidden eligibility prerequisites; behavior for absent/unknown plan values; concurrent transfers/upgrades/downgrades; and completion timing. Resolving those would require additional first-party specification or separately authorized evidence, not invented vendor guarantees.

## Sources

All external material below was retrieved November 7, 2026. Mutable documentation describes the current public policy; pinned links identify upstream client source.

### First-party product and API documentation

- **[D1]** [Project Pausing](https://supabase.com/docs/guides/platform/free-project-pausing): general paid restriction, transfer advice, and separate inactivity policy.
- **[D2]** [Deleting Your Project — Alternative: Pause your project](https://supabase.com/docs/guides/platform/delete-project#alternative-pause-your-project): manual action and Free-only restriction.
- **[D3]** [Pro projects can't be paused: workarounds](https://supabase.com/kb/troubleshooting/pausing-pro-projects-vNL-2a): updated November 4, 2026.
- **[D4]** [Pause a project — Management API](https://supabase.com/docs/reference/api/v1-pause-a-project).
- **[D5]** [Platform Webhook Events](https://supabase.com/docs/guides/platform/webhooks/events#v1projectpaused): manual/system reasons and the subsequent status-change definitions.
- **[D6]** [About billing on Supabase — Organization-based billing](https://supabase.com/docs/guides/platform/billing-on-supabase#organization-based-billing).
- **[D7]** [Get project — Management API](https://supabase.com/docs/reference/api/v1-get-project).
- **[D8]** [Get an organization — Management API](https://supabase.com/docs/reference/api/v1-get-an-organization).
- **[D9]** [List all organizations — Management API](https://supabase.com/docs/reference/api/v1-list-all-organizations).
- **[D10]** [Management API introduction — Rate limits](https://supabase.com/docs/reference/api/introduction#rate-limits).
- **[D11]** [Manage your subscription](https://supabase.com/docs/guides/platform/manage-your-subscription).
- **[D12]** [Project Transfers](https://supabase.com/docs/guides/platform/project-transfer).
- **[D13]** [Upgrading — Time limits](https://supabase.com/docs/guides/platform/upgrading#time-limits): subscription upgrade while paused and restoration caveats.
- **[D14]** [Billing FAQ](https://supabase.com/docs/guides/platform/billing-faq): fair-use restrictions and overdue invoices.
- **[S1]** [Public Management API OpenAPI JSON](https://api.supabase.com/api/v1-json), retrieved unauthenticated without resource calls. Inspected narrow projections of `paths["/v1/projects/{ref}/pause"].post`, project/organization GET responses, `V1OrganizationSlugResponse_Output`, `OrganizationResponseV1_Output`, `V1ProjectWithDatabaseResponse_Output`, and deprecated `V1CreateProjectBody.plan`.

### Publisher provenance and pinned upstream MCP

- **[U1]** [Package metadata for 0.15.0](https://registry.npmjs.org/@supabase%2fmcp-server-supabase/0.15.0) and [published attestations](https://registry.npmjs.org/-/npm/v1/attestations/@supabase%2fmcp-server-supabase@0.15.0), decoded but not cryptographically verified.
- **[U2]** [Pinned API platform: organization lookup, project lookup, and pause](https://github.com/supabase/mcp/blob/0edc736f256871b1d539a320def43a001bd8c7a8/packages/mcp-server-supabase/src/platform/api-platform.ts#L111).
- **[U3]** [Pinned account tool: read-only guard, awaited pause, success result](https://github.com/supabase/mcp/blob/0edc736f256871b1d539a320def43a001bd8c7a8/packages/mcp-server-supabase/src/tools/account-tools.ts#L442).
- **[U4]** [Pinned organization/project output schemas](https://github.com/supabase/mcp/blob/0edc736f256871b1d539a320def43a001bd8c7a8/packages/mcp-server-supabase/src/platform/types.ts#L26).
- **[U5]** [Pinned Management API client and error assertions](https://github.com/supabase/mcp/blob/0edc736f256871b1d539a320def43a001bd8c7a8/packages/mcp-server-supabase/src/management-api/index.ts#L14).

### Allowed local context

- **[L1]** Earlier [operation-contract memo](2026-11-06-supabase-operation-contract.md), read as a claim to verify, not independent vendor evidence.
- **[L2]** [Catalog Supabase entry](../../catalog/services.toml#L331): package/version and redirected native MCP boundary.
- **[L3]** [Captured MCP schema](../../servers/snapshots/supabase.json#L1): organization/project discovery and pause/restore arguments and annotations.
- **[L4]** Pause scenario [instruction](../../scenarios/cloud/supabase/pause-legacy-marketing-project/instruction.md#L1) and [TOML](../../scenarios/cloud/supabase/pause-legacy-marketing-project/scenario.toml#L1); generated [instruction](../../tasks/cloud__supabase__pause-legacy-marketing-project/instruction.md#L1) and [task TOML](../../tasks/cloud__supabase__pause-legacy-marketing-project/task.toml#L1). Read only; no evaluator implementation inspected.
- **[L5]** Narrow identity/ownership/status/creation/plan projections from pause-scenario [projects](../../scenarios/cloud/supabase/pause-legacy-marketing-project/dataset/supabase/projects.json#L1) and [organizations](../../scenarios/cloud/supabase/pause-legacy-marketing-project/dataset/supabase/organizations.json#L1).
