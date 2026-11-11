# Okta write-operation contract research

Research date: **November 7, 2026**. Scope: the three single-service write scenarios `offboard-contractor`, `transfer-to-platform-team`, and `contractor-self-service-reset`. Their business clock is **March 14, 2026, 09:00 UTC**; “yesterday” in the offboarding instruction means March 13, not November 6.[^l-offboard][^l-transfer][^l-reset]

This note establishes bounded operation/evaluation decisions, **not live Okta qualification**. Only this note is changed. No accounts, credentials, live management API requests, paid inference, installs, Docker, task generation, tests, runtime changes, or scenario/catalog/snapshot/instruction edits were used. No other agents were started. Existing Argo campaigns and frozen source were not changed.

## 1. Evidence and provenance boundaries

- **[D] Documented:** official Okta API references and first-party administrator/developer guides retrieved for this research. These are mutable current documents, not a historical March 14 server specification. Classic Engine help and Identity Engine guidance are identified separately.
- **[P] Pinned source examined:** published Python wheels were fetched publicly, hash-checked, and inspected **in memory**, without installation, import, or execution. The catalog selects `okta-mcp-server==1.1.7`; the dependency lock selects `okta==3.4.6`, `mcp==1.30.0`, and `pydantic==2.14.2`. This establishes examined source behavior, not the dependency tree of an independently running image.[^l-catalog][^l-lock][^p-mcp-package][^p-sdk-package]
- **[L] Local implementation:** observations of the present, already-dirty worktree's catalog, snapshot, scenario assertions, backend, and narrowly read grading logic. Backend comments and existing tests are local assertions, **not vendor evidence**; no tests were run.[^l-users][^l-groups][^l-policies][^l-api][^l-util][^l-grade]
- **[F] Fixture convention:** identities, group relations, policy/rule contents, and desired pointers in these datasets/TOMLs. All eight corresponding Okta fixture files are byte-identical across the three write scenarios. They do not prove real role grants, directory integrations, provisioning, session revocation, or policy execution.[^f-offboard][^f-transfer][^f-reset]
- **[I] Inference/recommendation:** derived evaluator or implementation guidance, not an additional Okta guarantee.
- **[U] Unsettled:** a question not resolved by the examined sources or not qualified through execution. No hosted deployment version, historical behavior, convergence deadline, cross-call transaction, exactly-once execution, or general concurrency guarantee is established here.

| Examined published artifact | SHA-256 |
| --- | --- |
| `okta_mcp_server-1.1.7-py3-none-any.whl` | `8d45003674992645fa111050d8e339d314894463f327805e35e838ee0542ab8a` |
| `okta-3.4.6-py3-none-any.whl` | `93ff7b8249fa0450ceffdedc39282dc2c306656a4e6d309e4b5ccc52a814d788` |

**[L/P] Capture identity:** the snapshot contains **113 tools**, with server info `Okta IDaaS MCP Server`, version `1.30.0`, protocol `2025-06-18`, and recorded digest `e9de8e23b922fcb26e7ea2a6d7ed650b51a52f33a8fd09b2ad6b6e6127a8cd82`. Do not substitute that server-info version for package version `1.1.7`. The older MCP research table says 112 tools and mentions `6fca6b0`; that is not an independently verified source-commit pin in this investigation. The wheel hashes above are the provenance used here.[^l-snapshot][^l-server-notes][^p-mcp-server]

## 2. What workflows are actually present?

### Native MCP versus REST versus local backend

The catalog uses **real-tls** mode: a native Python MCP server targets `https://octo-corp.okta.com`, intercepted by the benchmark's local Okta HTTP backend. A REST capability implemented by that backend, or documented by Okta, does **not** thereby become a native MCP tool.[^l-catalog][^l-api]

| Capability | Captured/native contract | Relevant HTTP operation |
| --- | --- | --- |
| Read/identify users | `list_users`, `get_user`, `get_user_profile_attributes`, `list_user_groups` | User/list/group GETs; list results are not the same representation as a full user read. |
| Update profile | `update_user(user_id, profile)`; no credentials/status/header argument | SDK `UpdateUserRequest.from_dict({"profile": profile})`, then **POST** `/api/v1/users/{id}`. |
| Deactivate user | `deactivate_user(user_id)`; confirmation behavior below | **POST** `/api/v1/users/{id}/lifecycle/deactivate`; no exposed `Prefer`, `sendEmail`, or wait argument. |
| Delete user | `delete_deactivated_user(user_id)` | SDK user DELETE; the tool name is not proof of a pre-call status check. Not authorized for offboarding. |
| Group membership | `add_user_to_group(group_id, user_id)`, `remove_user_from_group(group_id, user_id)` | **PUT** / **DELETE** `/api/v1/groups/{groupId}/users/{userId}`; add first reads the user's memberships and may return without PUT if already present. |
| Password/security policy rule | `list_policies(type=...)`, `get_policy`, `list_policy_rules`, `get_policy_rule`, `update_policy_rule(policy_id, rule_id, rule_data)` | Rule update selects a typed SDK model and calls **PUT** `/api/v1/policies/{policyId}/rules/{ruleId}`. |
| Other policy administration | Create/replace/delete and activate/deactivate policy/rule tools exist | Distinct policy/rule resources and lifecycle operations; their presence does not authorize their use in these tasks. |
| Administrator-role assignment | **No captured user-role/group-role assignment, removal, listing, or role-resource-set-binding tool** | Vendor role APIs are separate; backend user/group dispatch has no role routes. |
| Password reset/unlock/suspend/reactivate | **No captured tools for these user operations** | Several lifecycle routes exist in the local backend, but native availability cannot be inferred from them. |
| Arbitrary REST, user PUT/PATCH, bulk membership replacement, wait/job polling | **No corresponding generic captured tool** | Do not invent a generic request or hidden-header escape hatch. |

Sources for this matrix are the complete captured tool-name inventory, relevant schemas, native function bodies, and SDK serializers—not tool descriptions alone.[^l-snapshot][^p-mcp-users][^p-mcp-groups][^p-mcp-policies][^p-sdk-api][^l-users][^l-groups]

**[P/L/U] Authorization boundary:** tool registration/pre-call guards use configured scopes; these catalog settings include users/groups/policies read/manage, but not `okta.roles.manage`. The local backend checks only that an Authorization header exists; it does not establish vendor OAuth/RBAC enforcement. Its synthetic token also advertises fewer scopes than the catalog configuration. A registered tool or accepted local write is therefore not evidence of a real administrator's effective permissions. No authorization provisioning is part of the scenarios.[^l-catalog][^p-mcp-scope][^l-api]

### Administrator roles are not group names or profile text

**[D]** Real role assignment uses separate operations: POST `/api/v1/users/{userId}/roles` or `/api/v1/groups/{groupId}/roles`, with `okta.roles.manage` and an appropriate role payload. Custom roles have resource-set-binding considerations; a standard-role example is not a universal custom-role contract.[^d-user-role][^d-group-role]

**[F/L/I]** The fixtures contain an `OKTA_GROUP` named **Okta Admins** (`00g2oktadm1Wx7Yz8x7`), described as super administrators, containing Ana Moreno and Priya Raman. There are no role-assignment fixtures/routes establishing that description as an actual grant; group stats even hard-code `hasAdminPrivilege: false`. Do not implement a fictional “administrator transfer” scenario from this group name, a job title, `profile.userType`, or a newly stored `role` property. The actual remaining workflows are account deactivation, ordinary profile/membership transfer, and a password-policy-rule change. Preserve unrelated membership/privilege state; do not grant or remove roles to solve them.[^f-reset][^l-groups][^l-users]

## 3. Identity, group assignment, and effective access

### Exact fixture targets

| Workflow | Authoritative local identity and initial state |
| --- | --- |
| Offboard | Contractor **Alex Kim**, `00u1alxkmct4St1Uv2x7`, login `alex.kim@northwind-consulting.example`, organization Northwind Consulting, title Salesforce Consultant, status `ACTIVE`. Remove only his Contractors - Salesforce membership (`00g2cntrsf6Op3Qr4x7`). The employee Alex Kim, `00u1alxkmem2Op9Qr0x7`, login `alex.kim@octo-corp.example`, is a different protected user. |
| Transfer | **Maria Gonzalez**, `00u1mrgnzlz6Wx3Yz4x7`, login `maria.gonzalez@octo-corp.example`, initially `ACTIVE`, title Support Team Lead, department Customer Support. Platform Engineering is `00g2pltfrm7Gh8Ij9x7`; Support Agents is `00g2spprt12Ac5Ef6x7`. |
| Password policy | **Contractors Password Policy**, `00p4pwcntr2Uv9Wx0x7`, type `PASSWORD`, priority 1, active, includes **Contractors** (`00g2cntrct4Kl1Mn2x7`), provider `OKTA`. Its active rule is `0pr5rule0tr2Uv9Wx0x7`, priority 1, network `ANYWHERE`, initially no excluded users. **Contractors is not Contractors - Salesforce.** |

These are fixture facts, not discovered production accounts.[^l-offboard][^l-transfer][^l-reset][^f-offboard][^f-transfer][^f-reset]

**[D/P/I] User identity alternatives:** user read/update/deactivate references permit an ID, login, or **unambiguous** login shortname. Both Alex accounts have shortname `alex.kim`, so that shortname is not an acceptable disambiguation. Discover by supported search/name queries, resolve the organization/login and canonical ID, and use a fresh direct user read before a sensitive write. Vendor `search` is eventually consistent; absence from a search result does not prove deactivation or deletion. Ordinary listing and `q` omit deprovisioned users; documented search/filter can include them. The pinned list tool returns profile/ID pairs in a pagination envelope, not full status-bearing users, and caps `fetch_all` at ten pages.[^d-user-list][^d-user-update][^d-deactivate][^p-mcp-users]

**[P/L/U] Encoding defect to investigate, not a hosted restriction:** the pinned SDK percent-encodes path parameters with an empty safe-character set, so a full login's `@` becomes `%40`. The examined backend splits `Uri.path()` without an explicit percent-decode step before its case-insensitive ID/login/shortname resolver. This is a static compatibility concern for login-addressed native calls, **not an executed failure**. Literal-ID required-call checks also do not establish semantic identity equivalence. Qualify normalization/alias handling before rejecting a vendor-valid full-login alternative; do not add an undocumented “Okta only accepts IDs” rule. Group membership documentation, separately, specifies the existing user's **ID**; resolve aliases before those writes.[^p-sdk-path][^l-api][^l-users][^l-grade][^d-group-add][^d-group-remove]

**[P] Additional native identity constraint:** the MCP validator admits only ASCII letters/digits plus `_`, `-`, `@`, `.`, and `+`, and rejects forbidden traversal/reserved patterns before the SDK call. Vendor documentation allowing a login does not guarantee that every possible schema-valid login is expressible through this validator. Resolve such cases to a supported canonical ID rather than weakening identity checks.[^p-mcp-serialization][^d-user-update]

### Membership is a single edge, not group replacement

**[D]** Group membership writes operate on one `(groupId,userId)` edge and support `OKTA_GROUP`; imported `APP_GROUP` membership is managed by its source import. The fixture's target groups are `OKTA_GROUP`, Everyone is `BUILT_IN`, and `salesforce-imported-profiles` is an `APP_GROUP`. Do not remove Everyone or edit the imported group as a substitute for changing the requested membership.[^d-group-add][^d-group-remove][^f-transfer]

**[L/P]** Local add/remove return 204 with no body; duplicate add and absent-edge removal are local no-ops after identity/type validation. Native add has a separate membership precheck and returns a message when already present; native remove calls the SDK directly. Those observations do not establish atomic precheck-plus-write, universal retry safety, or undocumented hosted duplicate/error semantics.[^l-groups][^p-mcp-groups]

**[F/L/I] Effective-access boundary:** Support Agents is assigned to Zendesk; Contractors - Salesforce and Salesforce Users are separate group assignments to Salesforce. The backend derives user app assignments from direct and group relations. Removing Maria from Support therefore removes that group-derived Zendesk path, while Taylor Brooks' existing Support membership and every group-to-app assignment remain intact. Removing contractor Alex from the Salesforce group removes that membership-derived path; it must not remove the Salesforce app's assignment to the entire group or the employee group's assignment. Multiple grants would require evaluating their union, not merely one deleted edge.[^f-transfer][^f-offboard][^l-access]

Assignment records, app links, actual sign-in permission, downstream provisioned accounts, and downstream data are different states. In particular, the local app-link derivation checks application status but not user status; a deactivated user's remaining links are **not proof of live access** or successful downstream deprovisioning.[^l-access][^d-deactivate]

## 4. Maria's profile: partial POST versus complete PUT

**[D]** POST user update has partial semantics. A user PUT has strict replacement semantics: all profile properties must be supplied; omitted properties are deleted/set to null according to the operation descriptions. Do not implement Maria's two-field update as a profile-clearing PUT or apply generic JSON merge behavior to every Okta write.[^d-user-update][^d-user-replace]

**[P/L]** Native `update_user` takes the profile directly, wraps it once, and uses POST. The scenario's minimal profile is:

```json
{
  "title": "Site Reliability Engineer",
  "department": "Platform Engineering"
}
```

Passing `{"profile": {...}}` as the tool's `profile` argument double-nests it. Passing `status`, `credentials`, or administrator roles *inside* that profile is not a top-level lifecycle/credential/role operation. The local handler merges profile keys for POST, rebuilds them for PUT, validates the resulting required base fields, and does not interpret a profile property as account status.[^p-mcp-users][^p-sdk-api][^l-users][^l-transfer]

**[P/I] SDK transport matters:** `UserProfile.from_dict` populates known model members, `to_dict` can represent explicit nulls, and `ApiClient` serializes through `to_dict`. However, the generated operation uses `keep_empty_params=False`; `RequestExecutor.clear_empty_params` recursively removes nulls, empty strings, empty arrays, and empty objects, while retaining `0` and `false`. Consequently:

- The minimal non-empty title/department update follows a preservation-compatible POST path.
- Do **not** infer that this native tool can clear a property by sending null or clear a collection with `[]`, even if raw REST supports such input.
- Do **not** simulate native requests by handing the original tool-argument JSON directly to the backend; the wire body can differ.
- Do not treat every profile dictionary key as a valid vendor extension. The SDK can retain additional profile properties, but official docs require custom properties to exist in the user schema. Local base-field validation does not certify that schema or write permissions.

These are source-level transport observations and implications, not executed packet captures.[^p-sdk-profile][^p-sdk-cleanup][^d-user-update][^l-users]

**[P/I] Read-back representation caveat:** native response normalization prefers Pydantic `model_dump(by_alias=True, exclude_none=True)` rather than returning the raw REST body or invoking every nested model's `to_dict`. The profile model separately represents custom keys in `additional_properties`. Blindly echoing a normalized user/profile response is therefore not a demonstrated lossless custom-field/null round-trip. For this fixture the requested minimal two-field update avoids that dependency; future custom-field preservation cases must qualify the actual read/write representation.[^p-mcp-serialization][^p-sdk-profile]

**[I] Transfer acceptance:** require exactly the requested title/department and Maria's Platform membership, absence of her Support membership, and preservation of her other logical profile fields, status, credentials/provider, user type, and unrelated memberships. In these fixtures, Everyone and Taylor's Support membership are explicit witnesses. The current TOML checks login, surname, employee number, status, and those edges, but not all account fields or all other users. Timestamps and membership-update metadata may legitimately change; preserving the account does not mean freezing every server-generated timestamp.[^l-transfer][^f-transfer][^l-users][^l-groups]

**[I/U] Alternatives:** a schema-compatible full profile sent through the native POST can also satisfy the request if it preserves unrelated values; a two-field patch is not the only valid shape. A complete REST user PUT might achieve the same logical outcome, but it is **not exposed by this pinned tool**, and a partial PUT is not valid preservation. Profile update/add/remove have no documented required order here: grade the authorized outcome and intermediate safety, not one arbitrary three-call ordering. No cross-operation atomicity is established.[^l-snapshot][^d-user-update][^d-user-replace][^l-transfer]

## 5. Alex's offboarding: lifecycle completion is not deletion

**[D]** Deactivation is POST `/users/{id}/lifecycle/deactivate`, intended for a user not already `DEPROVISIONED`. Completion is indicated by `status: DEPROVISIONED`; `transitioningToStatus: DEPROVISIONED` indicates an unfinished transition. The operation supports asynchronous invocation using `Prefer: respond-async`; the documented success is 200 with no content. Deactivation retains the Okta user but deprovisions assigned applications and can irreversibly destroy downstream data. “Keep the account for audit” is not a guarantee that downstream email/files survive.[^d-deactivate]

**[P]** Native deactivation attempts MCP elicitation. Decline/non-confirmation returns a cancellation message without the SDK write. When elicitation is unavailable, or errors through the examined fallback branches, `auto_confirm_on_fallback=True` permits execution. The function sends no explicit async preference, performs no completion read/wait, and returns its own success sentence after inspecting the SDK error position. A successful MCP message is therefore not an independent lifecycle-completion witness, and another confirmation tool/argument must not be invented.[^p-mcp-users][^p-mcp-elicit]

**[L/U]** Local deactivation changes status immediately, clears `activated`, updates timestamps, and logs success; it retains group memberships until an explicit removal. It also accepts an already-deprovisioned user, unlike the vendor instruction to perform the operation only on other statuses. The response is local 200 `{}`, not a modeled hosted async job. It does **not** implement downstream app deprovisioning or a transition scheduler. These are simulation limitations, not evidence that real Okta completion is instantaneous or idempotent.[^l-users][^l-access]

**[D/I] DELETE is not a safe substitute:** the documented user DELETE deactivates a not-yet-deactivated user on the first invocation and permanently deletes an already-deprovisioned user. The local backend follows this broad two-stage behavior, while the pinned tool named `delete_deactivated_user` does not itself first fetch/assert status. Although a first DELETE can produce a retained deprovisioned user, the explicit “don't delete” instruction and scenario's blanket user-DELETE prohibition make it an unauthorized alternative here. Suspension, group removal alone, password reset, or writing a profile property cannot replace deactivation.[^d-user-delete][^p-mcp-users][^l-users][^l-offboard]

**[I] Offboarding acceptance:** retain contractor Alex's canonical user identity/profile for the audit trail, reach terminal `DEPROVISIONED`, remove the exact Contractors - Salesforce edge, and preserve the employee Alex's entire logical account and all unrelated memberships/assignments. Preserve the contractor's other group edges unless a separately established lifecycle contract requires otherwise. Do not demand a universal remove-before-deactivate or deactivate-before-remove order, universal automatic group removal, exact generated log IDs, or a fixed completion deadline. Resolve uncertain response loss with a direct identity/status/membership read rather than assuming no mutation occurred or blindly retrying a destructive operation.[^l-offboard][^f-offboard][^d-deactivate][^l-users][^l-groups]

## 6. Contractors' password policy: rule access and applicability

### Authorized effect and preserved policy settings

The requested change is to the existing **PASSWORD rule's actions**, not the policy's complexity, recovery-factor settings, lockout settings, group targeting, priority, or activation status. The intended action set is:

```json
{
  "passwordChange": {"access": "ALLOW"},
  "selfServicePasswordReset": {"access": "DENY"},
  "selfServiceUnlock": {"access": "DENY"}
}
```

**[D/F/I]** The three action names distinguish signed-in password change from forgotten-password recovery and self-service unlock. Classic Engine administration describes these as separate rule choices. Preserve `passwordChange: ALLOW` and deny the latter two; do not disable the rule/policy, remove contractors from its scope, or weaken complexity as a substitute. The existing contractor policy has minimum length 10, maximum age 60 days, history 6, lockout after 5 failures, and **automatic unlock after 30 minutes**. Its recovery email factor remains active. Automatic timed unlock requires no user action and is distinct from self-service unlock; this instruction does not silently authorize changing that timer or dismantling recovery factors.[^d-password-help][^l-reset][^f-reset]

**[D/I]** Management password reset is a different operation: it creates a one-time recovery token and moves a user to `RECOVERY`. Administrative unlock is also a separate operation with its own status/provider conditions. Neither is the policy change requested here. Do not mutate current users' status/passwords/sessions as a way to “turn off self-service,” or infer that denying self-service revokes all administrator recovery capabilities. No help-desk workflow, authentication ceremony, or actual end-user reset/unlock attempt was executed.[^d-password-reset][^d-user-unlock][^l-reset]

### Replacement and the exact native payload boundary

**[D/P]** Native `update_policy_rule` means REST **Replace a policy rule**, not PATCH. `rule_data` is an unrestricted dictionary in the captured schema, but is converted using a typed model map: `"PASSWORD"` selects `PasswordPolicyRule`. Its actions/conditions are explicitly parsed into nested SDK models. Supplying `type: "PASSWORD"` is material; with this SDK, omitting/using an unsupported discriminator can reach `PolicyRule.from_dict` and fail before a request. The source comment about a base model dropping fields is not a substitute for tracing the installed-version model implementation.[^d-rule-replace][^p-mcp-policies][^p-sdk-rule]

**[I] Safe bounded plan:** discover `PASSWORD` policies, identify the Contractors group/provider/priority rather than only matching a name, fetch the target rule, then carry forward its supported mutable name, type, priority, conditions, existing status, and complete action structures while changing only the two access decisions. Preserve any existing supported recovery requirements. Do not copy fixture `_key`, response links, IDs, or timestamps as if they were all writable policy fields. Read the resulting policy/rule and evaluate scope/status, not just the mutation response.[^l-reset][^f-reset][^d-rule-replace][^p-sdk-rule]

**[D/U] Omission limit:** the vendor reference calls this replacement but does not provide a complete field-by-field contract for every omitted `PASSWORD` member. Its generic example is an access-policy rule, not proof that omitted password actions or conditions merge. Do **not** claim “all omitted rule fields become null,” “omission always preserves,” or a universal exact error for a partial rule. Explicitly carrying applicable supported values avoids depending on those unsettled omission cases.[^d-rule-replace]

**[L] Local behavior is more specific, and must not be generalized:**

- `replace_rule` requires a nonempty name, builds a replacement object, retains old `id`, `type`, `system`, `created`, and `status`, and retains priority only when absent. It **does not retain omitted actions or conditions**.
- It does not validate rule action access enums, complete nested conditions, or a mismatched supplied type before overwriting type with the old one.
- The separate **policy** replacement handler preserves omitted priority, conditions, settings, and description. Its inline “Okta keeps” comment and an existing local test are not independent vendor confirmation of those omission semantics.
- There is no policy evaluator in this module proving that the stored actions actually govern user self-service.

Thus a local 200 can retain the right three action strings while losing targeting, or accept data the native model cannot transmit.[^l-policies][^l-okta-tests][^p-sdk-rule]

**[P/I] Empty-container preservation trap:** the pinned SDK request cleanup also applies to rule PUTs. In this exact solution, `people.users.exclude: []` is recursively removed; the empty `users`/`people` containers then disappear, while `network.connection: ANYWHERE` survives. This is static wire-path inference, not an observed packet. Do not require the wire body to equal TOML `rule_data` literally or silently treat this as preservation of a non-empty exclusion. An evaluator should assess applicable users/conditions and explicitly choose how absent versus empty optional exclusions are normalized; no universal vendor canonical JSON shape is established. Non-empty exclusions, network zones, or additional supported recovery requirements must not be lost under that normalization.[^p-sdk-cleanup][^p-sdk-rule][^l-reset][^f-reset]

**[P/D/I] Unsupported fields are not a preservation mechanism:** unknown password-rule/action keys are not generally round-tripped by these `from_dict` models. The valid access enum is `ALLOW`/`DENY`; an invented `enabled: false`, `selfServiceReset`, or lowercase enum is not the documented denial. `requirement.accessControl` belongs under `actions.selfServicePasswordReset` in the newer model, not at arbitrary top level. Profile/custom-schema extensibility does not imply policy-rule extensibility. Reject wrong-path success claims and distinguish pre-HTTP model errors, dropped keys, and server rejections instead of promising one universal 400 envelope.[^p-sdk-rule][^p-sdk-sspr][^d-account-management][^d-user-update]

### Effective policy selection and engine/source variants

**[D/F/I]** Okta evaluates policy priorities and matching conditions, then rule priorities/conditions; an unmatched rule can allow evaluation to continue elsewhere. In this fixture, the active Contractors policy is priority 1 with an all-network, no-exclusion rule. Employee policy priority 2 includes **Everyone**, so it also covers contractor members if the higher-priority contractor policy/rule stops matching; Default policy is priority 3. Merely storing two `DENY` strings in an inactive, excluded, wrong-provider, or shadowed rule does not establish the requested effective denial.[^d-policy][^f-reset]

For the fixture's **active Okta-sourced** users, the bounded expected decision is:

| Subject/context | Applicable fixture rule | Password change | Self-service reset | Self-service unlock |
| --- | --- | --- | --- | --- |
| Contractor Alex in Contractors; matching network | Contractors rule, still active/priority 1 | Allow | Deny | Deny |
| Maria or employee Alex, not in Contractors | Employees rule, unchanged | Allow | Allow | Allow |
| A context reaching the default rule | Default rule, unchanged | Allow | Allow | Deny |

This is **[I/F] configuration-evaluation reasoning**, not evidence of live sign-in/recovery tests. Preserve all employee/default policy settings and rules, not just the few asserted access leaves. Reordering policies, replacing group conditions with an empty object, changing provider, creating an overriding rule, or excluding contractors can defeat the desired outcome without changing those leaves.[^f-reset][^l-reset][^d-policy]

**[D/P/U] Engine and identity-source boundaries:**

- The fixtures explicitly use provider `OKTA`; they do not identify an installed Classic/Identity Engine version. The current Policy API overview limits its password-policy statement to Okta/AD users, while Classic administrator help also discusses LDAP policy and source-side restrictions. Do not collapse those sources into a universal claim that all directory users behave identically, or that LDAP recovery is never supported.[^d-policy][^d-password-help][^f-reset]
- Identity Engine can delegate recovery authentication requirements to the **Okta account management policy**, an `ACCESS_POLICY` identified by `_embedded.resourceType: END_USER_ACCOUNT_MANAGEMENT`. The official guide shows nested `requirement.accessControl: AUTH_POLICY`/`LEGACY`; the pinned SDK supports that nested requirement. No such account-management policy or requirement is established in these fixtures. This is not an app-sign-in policy task, and does not authorize creating/deleting an account-management policy.[^d-account-management][^p-sdk-sspr][^f-reset]
- Documented administrative unlock also covers an `ACTIVE` Okta-sourced user blocked from unknown devices and does not support directory-sourced accounts such as AD. The local route only accepts `LOCKED_OUT` and never checks provider. Those broader lifecycle variants are outside the captured native tool surface and are unqualified; do not certify them from the mock or use them as negative password-policy tests.[^d-user-unlock][^l-users][^l-snapshot]
- Policy configuration success establishes neither retroactive password/session mutation nor a measured propagation time. Do not demand that these unchanged active users immediately become locked, expired, or recovering, or claim that every downstream login path is now tested.[^d-user-update][^d-password-reset][^l-reset]

## 7. Error, response-loss, and concurrency boundaries

| Layer | Examined contract and evaluator implication |
| --- | --- |
| Vendor HTTP | Error documents define `errorCode`, `errorSummary`, `errorLink`, `errorId`, and optional `errorCauses`; status and preconditions are operation-specific. Group membership success is 204/no content; deactivation is 200/no content; rule replacement returns a rule. Do not require a JSON object on every successful write.[^d-errors][^d-group-add][^d-group-remove][^d-deactivate][^d-rule-replace] |
| Local HTTP | Errors use an Okta-shaped JSON envelope with a fixed synthetic error ID; missing resource/endpoint is 404 `E0000007`, malformed body is 400 `E0000003`, validation is 400 `E0000001`. Membership against non-`OKTA_GROUP` returns local 400 `E0000060`, whereas the general vendor error catalog labels `E0000060` as 501. That mismatch must not be reported as a verified exact vendor status. Headers contain synthetic request/rate-limit values.[^l-util][^l-groups][^d-errors] |
| Native user/group tools | Handled failures return ordinary lists such as `[{"error": "..."}]` or `[{"exception": "..."}]`; confirmation cancellation and already-member are messages. These are not proof of an HTTP mutation.[^p-mcp-users][^p-mcp-groups] |
| Native policy tools | Handled failures generally return `{"error": "..."}`. Missing success bodies for body-returning operations become `none_body_error`; the serialization wrapper can return `{ok:false,error:{type,message,tool},status_code:null,raw:{}}`. Do not assume all failures have a useful HTTP status or that outer MCP transport success means task success.[^p-mcp-policies][^p-mcp-serialization] |
| Current evaluator | Required-call matching normally requires a 2xx journal entry and rejects a limited set of tool-error shapes. It is **not merely a method/path existence check**. However, exact ID paths restrict identity alternatives, and the generic response matcher does not recognize every native `error`/`exception` value. Native results, authoritative state, preservation, and effective scope still need their own checks.[^l-grade][^l-offboard][^l-transfer][^l-reset] |

**[I/U] Response loss:** a timeout or client/model/serialization error does not prove the write was unapplied. Re-read exact user/membership/rule state, distinguish pending deactivation from completion, and then choose bounded recovery. The add-tool precheck does not establish exactly-once writes. No source here establishes an idempotency key, a whole-workflow transaction, a durable async job/poll URL, a retry count, or a fixed convergence time for these native operations.[^p-mcp-users][^p-mcp-groups][^p-mcp-policies][^d-deactivate]

**[D/P/L/U] Concurrency:** current user POST/PUT references document optional `If-Match`; this native `update_user` signature does not expose it. The policy-rule tool exposes no request-header/version parameter either. The local backend's per-request world lock is not a hosted multi-call concurrency guarantee. Read-modify-write can avoid accidental omission but is not thereby a conditional or atomic update. Do not invent ETag forwarding, stale-write rejection, lost-update protection, or safe concurrent replacement for this surface.[^d-user-update][^d-user-replace][^l-snapshot][^p-mcp-users][^p-mcp-policies][^l-api]

## 8. Bounded decisions and actionable evaluator objections

These are **[I] proposed decisions**, not implementation changes or new ordinary user instructions.

1. **Keep the scope at the three actual workflows.** No role-transfer task exists. Admin-role APIs, arbitrary lifecycle tools, generic REST, and account-management-policy provisioning must not be silently added because their REST documentation exists.[^l-snapshot][^l-offboard][^l-transfer][^l-reset]
2. **Bind writes to resolved identities.** Both Alex accounts share a display name and shortname. Verify target organization/login and canonical ID; protect all unrelated users. Investigate percent-encoded-login handling and exact-path grading before qualifying ID/login equivalence. Do not accept a mutation of the wrong Alex followed by restoring a few checked leaves.[^f-offboard][^p-sdk-path][^l-api][^l-grade]
3. **Preserve profile semantics, not just three witnesses.** Maria's TOML samples login/surname/employee number/status but does not cover all profile/credential/type fields or unrelated memberships. Evaluate logical preservation with allowed server metadata changes, and distinguish partial POST from destructive partial PUT.[^l-transfer][^d-user-update][^d-user-replace]
4. **Keep membership, role grants, and app assignments separate.** Grade the two intended edge changes, preserve Everyone/Taylor/unrelated members and app-group grants, and check derived assignment paths where modeled. Group description/title text must never stand in for real administrator grants or live app access.[^f-transfer][^l-groups][^l-access][^d-user-role][^d-group-role]
5. **Require terminal offboarding and retained identity.** A request, success message, pending transition, search omission, or deleted user is insufficient. Preserve the retained profile and employee account; model downstream deprovisioning only with an explicitly chosen, source-backed abstraction. Do not manufacture hosted completion timing or downstream data preservation.[^l-offboard][^d-deactivate][^d-user-list]
6. **Validate rule applicability, not only action strings.** Preserve active policy/rule status, priorities, contractor-group/provider conditions, network restrictions/exclusions, other actions/requirements, and all employee/default settings. Check that no newly created higher-priority policy/rule re-enables self-service. The current TOML's leaf checks and forbidden PUT/DELETE/deactivation paths do not establish these broader invariants.[^l-reset][^f-reset][^d-policy]
7. **Trace the actual native request.** Test design must account for typed discriminator handling, dropped unknown keys, and recursive empty-value stripping. The fixture's empty exclusion list is especially unsuitable for asserting raw-wire equality. Decide semantic normalization explicitly; never generalize it to non-empty conditions.[^p-mcp-policies][^p-sdk-rule][^p-sdk-cleanup][^l-reset]
8. **Reject unsupported-success shortcuts at the correct layer.** Examples include `profile.status`, fictitious policy `enabled` flags, wrong action paths, invalid access enums, imported-group edits, and arbitrary nested role text. Distinguish native preflight/model failures, silent field loss, local acceptance, and documented vendor errors; do not invent a single hosted error envelope/status for all of them.[^p-sdk-rule][^p-sdk-profile][^l-users][^l-policies][^d-errors]
9. **Do not overconstrain legitimate execution choices.** Different discovery queries, supported extra reads, a complete preservation-safe native profile POST, and different safe ordering of independent membership/profile/offboarding calls need not be rejected. A first user DELETE is nevertheless excluded by this task's explicit no-deletion boundary. A new shadow policy/rule is not automatically equivalent to changing the requested existing rule.[^l-offboard][^l-transfer][^l-reset][^d-user-delete][^d-policy]
10. **Keep recovery, enforcement, and live qualification separate.** Denying self-service does not mean disabling automatic timed unlock, administrator reset, signed-in password change, or current sessions. Configuration evaluation for the finite `OKTA` fixture is supportable; AD/LDAP, unknown-device blocks, Identity Engine delegated recovery, concurrency, and real propagation need separate qualification before evaluator expansion.[^d-password-help][^d-password-reset][^d-user-unlock][^d-account-management][^f-reset]

**Remaining uncertainty [U]:** precise hosted omission/default behavior for all `PASSWORD` replacement members; canonical treatment of absent versus empty conditions; installed engine/features/dependency provenance; effective real role grants; downstream provisioning/session effects; encoded-login execution through the full harness; and response-loss/concurrent-write behavior remain unqualified. The available primary documents and examined source support the bounded plans above, but do not settle those broader guarantees. No runtime experiment was used to fill these gaps.

## Primary-source register

Remote material was read publicly without account authentication. Wheel-member paths/line numbers below refer to the hash-checked published archives, not an assumed Git commit. Local references describe the worktree at inspection time.

### Local scenario, capture, and implementation evidence

[^l-offboard]: [`instruction.md`](../../scenarios/cloud/okta/offboard-contractor/instruction.md) and [`scenario.toml`](../../scenarios/cloud/okta/offboard-contractor/scenario.toml): authorized identity, retained account, membership removal, checks/forbidden calls, proposed native solution.
[^l-transfer]: [`instruction.md`](../../scenarios/cloud/okta/transfer-to-platform-team/instruction.md) and [`scenario.toml`](../../scenarios/cloud/okta/transfer-to-platform-team/scenario.toml): two profile fields, group edges, preservation samples, proposed native solution.
[^l-reset]: [`instruction.md`](../../scenarios/cloud/okta/contractor-self-service-reset/instruction.md) and [`scenario.toml`](../../scenarios/cloud/okta/contractor-self-service-reset/scenario.toml): policy/rule target, action leaves, forbidden calls, supplied replacement.
[^f-offboard]: [`offboard-contractor/dataset/okta`](../../scenarios/cloud/okta/offboard-contractor/dataset/okta): `users.json`, `groups.json`, `group_members.json`, `apps.json`, `app_groups.json`, `policies.json`, `policy_rules.json`; `logs.json` was compared for byte identity, not used as a live operation witness.
[^f-transfer]: [`transfer-to-platform-team/dataset/okta`](../../scenarios/cloud/okta/transfer-to-platform-team/dataset/okta): corresponding fixture files, including Taylor's retained membership and app-group grants.
[^f-reset]: [`contractor-self-service-reset/dataset/okta`](../../scenarios/cloud/okta/contractor-self-service-reset/dataset/okta): corresponding fixture files, provider/group conditions, priorities, settings, and complete rule actions.
[^l-catalog]: [`catalog/services.toml`](../../catalog/services.toml), Okta entry at line 366: transport/origin, package/version, configured scopes.
[^l-lock]: [`servers/python/okta.txt`](../../servers/python/okta.txt): locked MCP/SDK/MCP-protocol/Pydantic dependencies.
[^l-snapshot]: [`servers/snapshots/okta.json`](../../servers/snapshots/okta.json): metadata, all 113 tool names, relevant input/output schemas.
[^l-server-notes]: [`docs/research/mcp-servers.md`](mcp-servers.md), Okta row: older inventory/provenance annotation, not independently re-attested here.
[^l-api]: [`crates/services/okta/src/lib.rs`](../../crates/services/okta/src/lib.rs), `api`, OAuth stubs and token claims: origin routing, per-request locking, header-presence authentication, raw path segmentation, module dispatch.
[^l-users]: [`crates/services/okta/src/users.rs`](../../crates/services/okta/src/users.rs), `handle`, `find`, `list_users`, `update_user`, `delete_user`, `set_status`, and `lifecycle`: local user identity/update/lifecycle implementation.
[^l-access]: [`crates/services/okta/src/users.rs`](../../crates/services/okta/src/users.rs), `user_groups`, `assigned_app_ids`, and `app_links` at lines 597, 607, and 623: derived membership/app views.
[^l-groups]: [`crates/services/okta/src/groups.rs`](../../crates/services/okta/src/groups.rs), `handle`, `add_member`, `touch_membership`, and `list_groups`: edge operations, group-type restrictions, metadata, synthetic admin stats.
[^l-policies]: [`crates/services/okta/src/policies.rs`](../../crates/services/okta/src/policies.rs), dispatch, `replace_policy` at line 299, `replace_rule` at line 388: replacement/preservation differences and absent effective-policy evaluator.
[^l-util]: [`crates/services/okta/src/util.rs`](../../crates/services/okta/src/util.rs), `into_response`, error constructors, and `Req::object`: HTTP envelopes/headers and malformed-body handling.
[^l-okta-tests]: [`crates/services/okta/tests/okta.rs`](../../crates/services/okta/tests/okta.rs), `policies_and_rules` at line 471: existing assertions/comments only; read but not run.
[^l-grade]: [`crates/mock-core/src/grading.rs`](../../crates/mock-core/src/grading.rs), required-call evaluation near line 693, `call_matches` near line 949, and `required_response_matches` near line 1040: path matching and accepted-response filtering.

### Published pinned MCP and SDK source

[^p-mcp-package]: [PyPI metadata, `okta-mcp-server` 1.1.7](https://pypi.org/pypi/okta-mcp-server/1.1.7/json); [examined wheel](https://files.pythonhosted.org/packages/25/35/f23382233cb169f4e96516daffd9770563d314aaf4e7f902cf128f5d10b9/okta_mcp_server-1.1.7-py3-none-any.whl). Metadata identifies Okta's official repository; wheel digest was independently computed.
[^p-sdk-package]: [PyPI metadata, `okta` 3.4.6](https://pypi.org/pypi/okta/3.4.6/json); [examined wheel](https://files.pythonhosted.org/packages/88/73/09ecb97feed562edd39ab1111f490949c1ed93dbe21e669e36b00eeee71e/okta-3.4.6-py3-none-any.whl). Metadata identifies `okta/okta-sdk-python`; wheel digest was independently computed.
[^p-mcp-server]: MCP wheel member `okta_mcp_server/server.py`, lines 30–52 and 140–153: initialization, configured-scope pruning, imported tool modules.
[^p-mcp-users]: MCP wheel member `okta_mcp_server/tools/users/users.py`: `list_users` lines 31–196; `get_user` 242–280; `update_user` 358–399; `deactivate_user` 406–449; `delete_deactivated_user` 456–498. User-group reads are in `tools/user_resources/user_resources.py`, lines 22–60.
[^p-mcp-groups]: MCP wheel member `okta_mcp_server/tools/groups/groups.py`: `add_user_to_group` lines 562–607, `remove_user_from_group` 614–645.
[^p-mcp-policies]: MCP wheel member `okta_mcp_server/tools/policies/policies.py`: typed model map/helper lines 31–58; `update_policy` 243–274; `get_policy_rule` 473–504; `create_policy_rule` 511–548; `update_policy_rule` 555–590.
[^p-mcp-elicit]: MCP wheel member `okta_mcp_server/utils/elicitation.py`, `elicit_or_fallback` lines 93–191: acceptance/cancellation, unsupported-client and exception fallbacks.
[^p-mcp-scope]: MCP wheel members `utils/scope_registry.py`; `utils/scope_guard.py`, `get_configured_scopes` lines 93–108, `require_scopes` 136–200, startup pruning 203 onward; `utils/client.py` lines 15–29. Guards use manager configuration, not proof of granted server-side privileges.
[^p-mcp-serialization]: MCP wheel member `utils/serialization.py`: `to_jsonable`/model handling lines 108–242; failure envelope 264–288; missing-body error 295–315; `json_response` 322–362. ID validation is in `utils/validation.py`, regex line 55, `validate_okta_id` 68–114, `validate_ids` 333–411.
[^p-sdk-api]: SDK wheel members `okta/api/user_api.py`, `update_user`/serializer lines 1478–1699; `api/group_api.py`, assign 219–318 and unassign 1565–1664; `api/policy_api.py`, replace rule 3418–3598; `api/user_lifecycle_api.py`, deactivate 236–366.
[^p-sdk-path]: SDK wheel members `okta/api_client.py`, path quoting lines 227–235; `okta/configuration.py`, `safe_chars_for_path_param = ""` at line 227.
[^p-sdk-profile]: SDK wheel members `models/update_user_request.py`, `to_dict` 75–113, `from_dict` 116–144; `models/user_profile.py`, property definitions/additional properties, `to_dict` 279–445, `from_dict` 448–496; `api_client.py`, `sanitize_for_serialization` 385–430.
[^p-sdk-cleanup]: SDK wheel member `okta/request_executor.py`, `clear_empty_params` lines 107–129 and `create_request` 242–251. Generated user/rule methods set `keep_empty_params=False`; serializers invoke `ApiClient.param_serialize`, lines 178–273.
[^p-sdk-rule]: SDK wheel members `models/policy_rule.py`, discriminator dispatch 202–273; `models/password_policy_rule.py`, typed fields and `to_dict`/`from_dict` 40–172; `models/password_policy_rule_actions.py`, 45–152; `models/password_policy_rule_action.py`, 42–94; `models/password_policy_rule_conditions.py`, 102–124; `models/policy_access.py`, `ALLOW`/`DENY` enum.
[^p-sdk-sspr]: SDK wheel members `models/self_service_password_reset_action.py`, fields 44–50 and `from_dict` 109–128; `models/sspr_requirement.py`, nested `accessControl` documentation and `from_dict` 120–143.

### Official Okta documentation

[^d-user-update]: [Update a user](https://developer.okta.com/docs/api/openapi/okta-management/management/tags/user/other/updateuser): POST partial semantics, user identity alternatives, custom-schema requirement, administrative credential distinction, optional If-Match, read-only status.
[^d-user-replace]: [Replace a user](https://developer.okta.com/docs/api/openapi/okta-management/management/tags/user/other/replaceuser): strict PUT profile replacement and optional If-Match.
[^d-user-list]: [List all users](https://developer.okta.com/docs/api/openapi/okta-management/management/tags/user/other/listusers): deprovisioned-user visibility, search consistency, query limitations, ID read recommendation.
[^d-deactivate]: [Deactivate a user](https://developer.okta.com/docs/api/openapi/okta-management/management/tags/userlifecycle/other/deactivateuser): status preconditions/completion, async preference, destructive downstream effects, success response.
[^d-user-delete]: [Delete a user](https://developer.okta.com/docs/api/openapi/okta-management/management/tags/user/other/deleteuser): permanent deletion, first-DELETE deactivation, asynchronous option.
[^d-group-add]: [Assign a user to a group](https://developer.okta.com/docs/api/openapi/okta-management/management/tags/group/other/assignusertogroup): single-edge PUT, ID parameters, `OKTA_GROUP`/import distinction, 204.
[^d-group-remove]: [Unassign a user from a group](https://developer.okta.com/docs/api/openapi/okta-management/management/tags/group/other/unassignuserfromgroup): single-edge DELETE, group-type limitation, 204.
[^d-user-role]: [Assign a user role](https://developer.okta.com/docs/api/openapi/okta-management/management/tags/roleassignmentauser/other/assignroletouser): role scope/payload, standard/custom distinction and resource-set bindings.
[^d-group-role]: [Assign a role to a group](https://developer.okta.com/docs/api/openapi/okta-management/management/tags/roleassignmentbgroup/other/assignroletogroup): group-role resource, scope, and standard/custom distinction.
[^d-policy]: [Policies](https://developer.okta.com/docs/api/openapi/okta-management/management/tags/policy): evaluation/priority/condition framework, policy types, engine/default-rule and password-policy limitations.
[^d-rule-replace]: [Replace a policy rule](https://developer.okta.com/docs/api/openapi/okta-management/management/tags/policy/other/replacepolicyrule): PUT identity, body fields, action/condition descriptions, returned rule. Generic access-policy example does not settle every password-rule omission.
[^d-password-help]: [Configure a password policy — Classic Engine administrator help](https://help.okta.com/en-us/Content/Topics/security/policies/configure-password-policies.htm): separate password-change/reset/unlock choices, source-directory distinctions, recovery settings, automatic unlock without user action.
[^d-account-management]: [Configure an Okta account management policy — Identity Engine](https://developer.okta.com/docs/guides/okta-account-management-policy/main/): account-management resource identity, immutable policy versus editable rules, delegated recovery and nested AUTH_POLICY/LEGACY requirements.
[^d-password-reset]: [Reset a password](https://developer.okta.com/docs/api/openapi/okta-management/management/tags/usercred/other/resetpassword): token generation, RECOVERY transition, session option, provider-conversion distinction.
[^d-user-unlock]: [Unlock a user](https://developer.okta.com/docs/api/openapi/okta-management/management/tags/userlifecycle/other/unlockuser): LOCKED_OUT and unknown-device-blocked ACTIVE variants; Okta-source/directory limitation.
[^d-errors]: [Okta API error codes](https://developer.okta.com/docs/reference/error-codes/): common envelope, endpoint-dependent validation, malformed body, and documented E0000060 status.
