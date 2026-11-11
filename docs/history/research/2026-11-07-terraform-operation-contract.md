# Terraform write-operation contract research

Research date: **November 7, 2026**. Scope: the ordinary, single-service WRITE scenarios `approve-nat-gateway-change`, `scale-checkout-staging`, and `unlock-orders-prod` under `scenarios/code/terraform`. Their business clock is **October 28, 2026, 15:30 UTC**; “last night” in the orders instruction refers to October 27–28, not November 6–7.[^l-approve][^l-scale][^l-unlock]

This is research and a bounded evaluation recommendation, **not implementation or hosted qualification**. Only this Markdown file is authored. Public first-party source and documentation were read without installing, importing, building, or running the upstream packages. No tests, Docker, task generation, infrastructure/account actions, paid inference, or additional agents were used. Existing Argo32 archives remain frozen; the concurrently live Okta33 implementation and all other pre-existing worktree changes are outside this work.[^l-catalog][^l-image]

## 1. Evidence boundaries and reproducible pins

Labels throughout:

- **[D] Documented:** official Terraform/HCP Terraform REST references and first-party operational guides, retrieved November 7. These describe the documented service, not proof of an October 28 deployment's behavior.
- **[P] Pinned:** inspected source at the catalog-selected MCP release and its declared SDK/codec dependencies. Static source tracing is not a native packet capture or proof of a running image's dependency inventory.
- **[L] Local:** the bounded Rust backend, current grading logic, catalog, and captured tools snapshot in this already-dirty worktree. Local comments, status strings, and tests do not establish hosted guarantees.
- **[F] Fixture:** seeded identities, simulated plans, stored variables, locks, and ordinary scenario assertions.
- **[I] Inference/recommendation:** conclusions or proposed evaluator decisions drawn from the cited evidence; no counterexample below was executed.
- **[U] Uncertain:** an unqualified integration behavior, incomplete vendor specification, or unsupported extrapolation.

### Pins examined

| Component | Pin / observation |
| --- | --- |
| Catalog and build recipe [L] | `github.com/hashicorp/terraform-mcp-server/cmd/terraform-mcp-server@v1.4.1`; `stdio --toolsets all`; `TFE_ADDRESS={world}`; synthetic `TFE_TOKEN`; `ENABLE_TF_OPERATIONS=true`. The tools image independently names the same Go release. |
| MCP source [P] | `v1.4.1`, commit `3781509a674482ca7e2821fab8159a25e3b29bdf`, resolved from the published Go module metadata. Module checksum: `h1:v3Ta4sCNUFnr2XeaTB4mf+BWxWuG4Fhcczs/NCeQK2E=`. |
| HCP Go SDK [P] | `github.com/hashicorp/go-tfe v1.111.2`, commit `76ed5bb13de750863b0a99c3a2a8961fa5edfb46`; declared module checksum `h1:Bzak6GbLANLsy+c335vdteJdBF7fRvMhgcM9fzKSvbs=`. |
| Selected MCP SDK [P] | `github.com/mark3labs/mcp-go v0.58.0`, commit `9740d3a5cd700f5acac8bae47e14b4550a741182`; declared checksum `h1:AWfBk8lgRR0KZYve7PaLbR2MIjpw1oK2eGpBApaNS+Q=`. |
| Codec / retry library [P] | MCP `go.mod`/`go.sum` declare `github.com/hashicorp/jsonapi v1.5.0` and `github.com/hashicorp/go-retryablehttp v0.7.8`. No installation or runtime dependency attestation was performed. |
| Captured surface [L] | `servers/snapshots/terraform.json`: server `terraform-mcp-server`, version `1.4.1`, protocol `2025-06-18`, **61 tools**. Snapshot SHA-256: `d3cf8d324be5338d4cd80ff40e1011add73fde8f49a8828ab3ed628b79d5e6f1`. |
| Official documentation source [D] | Public pages and their linked first-party `hashicorp/web-unified-docs` source. Source revision examined: `965264c6e76c012c7bdc40a0a4d2f5456b49bce0` (commit timestamp November 6, 2026). This is a documentation pin, not a Terraform/HCP server version. |

Sources for the table: catalog/build/snapshot; published module metadata and checksum database; pinned `go.mod`/`go.sum`; official documentation repository.[^l-catalog][^l-image][^l-snapshot][^p-module][^p-manifest][^p-sdk-module][^p-mcp-module][^d-source]

For the local behavior analyzed here, SHA-256 witnesses are:

```text
d8af0153b4247e16082cc9cfe2cde7f1ddd21eb5c96a46a3a8c4dab2511c166e  crates/services/terraform/src/runs.rs
a91235c96308ddb7d212efdade804b3338b012cc79b39f0ff7ef4de46ebee2cc  crates/services/terraform/src/workspaces.rs
357987d82ef0b43ce4beac68a8c46e5ddf95be12df7a5da79cc064188f521651  crates/services/terraform/src/resources.rs
1e033e6acb958df89004c66fcfebdad5e843c4814cde9708fc568122eabee3e8  scenarios/code/terraform/approve-nat-gateway-change/scenario.toml
bcfdc6d1e89f449935712475e46677cc1b97bafd3c83dc4eb90f66f4473b08f8  scenarios/code/terraform/scale-checkout-staging/scenario.toml
e5c1881522f989493ff1c2fc3938a16a7fac640d3313b8a5413d0df26e3fd1b3  scenarios/code/terraform/unlock-orders-prod/scenario.toml
```

All **22** Terraform JSON fixture files are byte-identical across these three scenario directories. Their different instructions and assertions, not different initial worlds, define the authorized task. This does not make any fixture record a discovered production object.[^f-approve][^f-scale][^f-unlock]

## 2. Actual native availability and representation

### Registration is conditional, not just a snapshot entry

**[P/L]** CLI stdio selects the **mark3labs** implementation unless `TF_X_OFFICIAL_SDK_ENABLED` is exactly `"true"`. That override is not set by the inspected catalog; the source also contains an official-SDK implementation, but its mere presence is not evidence that this benchmark selects it. `--toolsets all` enables the Terraform group, and authenticated-session initialization dynamically registers TFE tools. `action_run` and `force_unlock_workspace` additionally require `ENABLE_TF_OPERATIONS=true`. `create_run` selects the unrestricted factory when that gate is enabled and the safe factory otherwise. Workspace-variable update itself is not gated by `RequiresTFOps`.[^l-catalog][^p-cli][^p-register]

**[U]** Availability still depends on a usable initialized session/client and any inherited environment override. The snapshot and catalog establish the intended surface, not a fresh live `tools/list`. No server was launched. A missing tool, client initialization failure, or selected implementation change is an environment/interface issue, not evidence that an attempted infrastructure operation succeeded.[^l-snapshot][^p-register][^p-client]

| Native tool [P/L] | Actual bounded behavior |
| --- | --- |
| `action_run` | Required `run_id`, `run_action`; enum only `apply`, `discard`, `cancel`; optional `comment`. No separate approve/confirm tool or confirmation Boolean. |
| `create_run` | Required organization and workspace **name**; optional message; one `run_type` enum: `plan_and_apply`, `refresh_state`, `plan_only`, `allow_empty_apply`, `auto_approve`, `is_destroy`. |
| `force_unlock_workspace` | Required workspace **ID**; reads by ID, rejects an already-unlocked workspace, then POSTs force-unlock. |
| `update_workspace_variable` | Requires organization, workspace name, variable ID, **key and string value**. No category argument; optional description and schema-Boolean `hcl`/`sensitive` have a wrapper type mismatch discussed below. |
| Discovery and verification | `list_terraform_projects`, `list_workspaces`, `get_workspace_details`, `list_runs`, `get_run_details`, variable listing, plan/apply details/logs, and state-version tools are present. |
| Not exposed by this snapshot | Ordinary workspace lock/unlock, run force-cancel/force-execute, direct state-write, workspace-variable delete, arbitrary REST requests, run-specific-variable overrides, resource targeting/replacement, configuration-version selection, saved-plan selection, and standalone run-comment creation. Some related REST routes exist locally; that does not expose a native tool. |

The table is traced through captured schemas, factory selection, and selected handlers, not inferred from REST route names.[^l-snapshot][^p-register][^p-action][^p-create][^p-unlock][^p-vars]

**[P/I] Discovery alternatives:** organization-wide run listing or workspace-filtered run listing can both identify the NAT run. Project-filtered workspace search can establish Commerce membership; an exact organization/name lookup can additionally resolve identity. Do not require the sample solution's exact discovery sequence. Native workspace lists are **summaries** containing ID, name, description, environment, creation time, and execution mode—not lock owner or complete settings. Fetch details for scalar settings, subject to the relationship-loss limit below. Run lists also return summaries. Follow pagination rather than treating page one as exhaustive.[^p-discovery][^p-details][^l-approve][^l-unlock]

**[P/L/I] Results are not one universal JSON shape:**

- Create-run and get-run return MCP **text content containing re-marshaled JSON:API**, not the raw HTTP response.
- Action-run, force-unlock, and variable update return human-readable success text, not a run/workspace/variable object.
- Workspace detail uses a `type: tool` wrapper whose attributes include a Go workspace object and whose variable relations are fetched separately. Variable-read failure is logged and substituted with an empty list; a default README can replace an unavailable workspace README. That wrapper's `success` does not certify every subsidiary read.
- `ToolError`/`ToolErrorf` return an MCP error result (`isError`), ordinarily with a nil Go error. Check that flag, not just successful JSON-RPC transport. Several reads collapse any SDK error into “not found”; this can hide permission or transport distinctions.
- SDK models and re-marshaling can drop unknown/local-only members. Do not address native results with raw fixture-store pointers or demand byte-for-byte REST equality.[^p-action][^p-create][^p-unlock][^p-vars][^p-details][^p-errors][^p-sdk]

**[P/I/U] Ownership/project observability gap:** `WorkspaceToolResponse.Workspace` is tagged as a nested JSON:API **attribute**. The pinned codec serializes such nested structs using only `nested.Attributes`, discarding their primary ID and relationships. Thus this traced `get_workspace_details` result retains scalar `locked` and an outer `workspace_id`, but does **not** expose the workspace's `locked-by`, project, current-run, or state-version relationships. The SDK workspace model also has no `locked-reason` field. Although the SDK internally decodes the polymorphic lock owner, this wrapper does not return it through that attribute. Project-filtered discovery and workspace-bound run reads provide useful evidence, but they are not a direct fresh lock-owner read. Do not require an agent to quote a nonexistent native ownership field, fabricate an owner from a same-named run, or issue an unauthorized settings write merely to obtain a different response shape. Authoritative fixture ownership can be evaluated internally; a complete native/hosted owner-observation path remains unqualified.[^p-details][^p-sdk-workspaces][^p-codec][^p-discovery][^l-snapshot]

## 3. Ordinary fixture identities and authorized boundaries

All rows are **[F]**, not real infrastructure inventory.[^f-approve][^f-scale][^f-unlock]

| Task | Target | Protected look-alikes / collateral |
| --- | --- | --- |
| Approve NAT | `acme-platform/network-core-prod`, workspace `ws-X6Cm5W6LhJnKhlo2`; existing run `run-1q2oy8I2MqSd5oM4`, status `planned`, plan `plan-vOqCAeqHb08EsDyk`, non-destroy/non-speculative/manual apply. Workspace lock belongs to that run. | `network-core-staging`, workspace `ws-aM6dcH7sRwt97v2D`, run `run-blTtGlrNXHe62r7y`, also `planned` with the **same run message**. An older prod draft is already `discarded`. |
| Scale checkout | `checkout-service-staging`, workspace `ws-FxCPzmnaeIMOUbSd`, Commerce project `prj-RGb4nwJ3c5kaUkvv`; existing variable `var-4eRpJG8P0JYsXLTJ`, key `desired_count`, category `terraform`, string value `"2"`, `hcl=false`, `sensitive=false`. | Prod `ws-ZQ7RjwWMr2evfIuC`, existing `desired_count` variable `var-FwwYKSi27hH9ISlj` remains `"6"`. Staging image, autoscaling ceiling `"6"`, environment/region, sensitive `DD_API_KEY`, descriptions, and variable-set attachments are not targets. |
| Unlock orders | `acme-platform/orders-api-prod`, workspace `ws-gOHPQz7D1ZRgqBJq`, **Commerce** project; locked by run `run-jCcDubzVP5ob0NM7`, still `applying`, apply object `apply-UpEcrL68If1kHpYw` still `running`. | `orders-api-prod-legacy`, workspace `ws-QMfGO0wb8mhyWgrq`, **Legacy** project `prj-WtYRcQLbjROqv4f8`; intentionally locked by user `user-Vm0I7MiOQY9x5zBV` (Dmitri), with a migration reason. `acme-sandbox/orders-api-prod`, `ws-i0F81Hk5tvGLpmR9`, is another organization. |

**[F/I]** All three target workspaces initially use remote execution, Terraform `1.9.5`, and workspace `auto-apply=false`. These are seeded settings, not current upstream version recommendations. Preserve organization/project identity, VCS configuration, working directory, runtime version/mode, tags, permissions, variable-set/policy attachments, and unrelated variables. “Do not touch production” in checkout means no production mutation, not a prohibition on read-only disambiguation.[^f-scale][^l-scale]

## 4. Approve NAT: confirmation means one existing run, then completion evidence

### Documented operation versus wrapper language

**[D]** POST `/api/v2/runs/{run_id}/actions/apply` confirms a run paused for approval. Optional `comment` is a **top-level** action-body string. Success is **202 with no response object**, meaning the apply was queued, not completed; 409 means applying is not currently allowed. Apply permission is required; organization tokens cannot invoke this operation. A blocking policy failure must be resolved through its policy workflow before applying. Read run actions/permissions and state, not just a display label.[^d-runs]

**[P]** `action_run(run_action="apply", run_id=..., comment=...)` calls `Runs.Apply` with `RunApplyOptions{Comment: &comment}`. The SDK uses ordinary JSON for those `json` tags, so the wire body is:

```json
{"comment":"Approved per CHG-2291"}
```

It is **not** `data.attributes.comment`. No MCP elicitation, second confirmation parameter, status pre-read, completion polling, or separate comment-create request occurs in this handler. After an accepted SDK call it reports that approval/application succeeded and suggests `get_run_details`; that wording exceeds what HTTP acceptance independently proves. The SDK call returns after request completion without waiting for the apply lifecycle.[^p-action][^p-sdk][^p-request]

**[D/P/I]** `"needs confirmation"` is UI/business wording; the actual seeded API status is `planned`. A real run can pass through checks and queued/confirmed/applying states before terminal `applied`, or fail. Do not equate every `planned` run with an immediately confirmable, policy-cleared run, or require every valid hosted run to expose the same short sequence. Verify the identified run, its workspace, non-speculative mode, confirmability/permission, and eventual result when claiming completion. A result still `apply_queued`/`applying` supports “accepted/in progress,” not “NAT gateway created.”[^d-runs][^d-states][^p-discovery][^f-approve]

### What the bounded backend actually does

**[L]** The apply handler permits statuses `planned`, `cost_estimated`, `policy_checked`, `policy_override`, `planned_and_saved`, and `post_plan_completed`. It does not independently enforce plan-only flags, actual policy outcomes, per-token permissions, or lock ownership before applying. In particular, admitting `policy_override` by status alone is not a vendor policy-override guarantee.[^l-runs][^l-resources]

**[L]** Within the local handler, an accepted apply immediately:

1. Creates a new simulated state-version record from the old record.
2. Adjusts workspace resource count by plan additions minus destructions, changes the current state/run pointers, and clears the lock **only if `locked-by.id` equals this run ID**.
3. Sets run status `applied`, confirmed actor and `confirm-comment`; sets confirmation/apply timestamps to the business clock.
4. Sets the associated apply status to `finished`, records plan counts and state-version relation.
5. Returns **202 with an empty body** despite having already performed the modeled transition.

This is an immediate local state mutation, not a hosted scheduler, transaction across tool calls, durable exactly-once guarantee, or real provider operation. The lock-release comparison checks the owner ID, not a complete ownership/security model.[^l-runs][^l-common]

**[F/L]** NAT's seeded plan has two additions (EIP and NAT gateway), one route-table update, and no destructions. Local apply changes the resource-count summary from 58 to 60, but normal `finish_apply` clones the previous state resource/output contents rather than executing those resource-level diffs. Neither the local status nor that count proves an AWS NAT/EIP/route change, an assigned gateway ID, successful traffic, or infrastructure readiness.[^f-approve][^l-runs]

**[L/P/U] Approval comment evidence:** `confirm-comment` is a local store/grader field, deliberately excluded from the rendered run resource. The action handler does not insert a record into the local `comments` collection. `get_run_comments` therefore need not reveal this newly submitted approval comment locally. Evaluate the correlated apply request's exact top-level comment and accepted transition, plus the store witness where available; do not require a fabricated new comment record or claim a hosted comment-feed representation not established here.[^l-runs][^l-resources][^p-action]

**[I] Bounded acceptance:** apply the **existing prod run**, with exactly `"Approved per CHG-2291"`, and establish the requested completed result in this immediate local world. Leave staging's run, its run-owned lock, and unrelated resources/settings unchanged. In an asynchronous hosted qualification, separate accepted approval from terminal completion and report pending/error honestly. No new run, workspace unlock, policy bypass, discard, cancel, or settings rewrite is needed. Alternate supported reads and extra verification are valid; applying a newly recreated plan is not the same approved change.[^l-approve][^d-runs][^p-action][^f-approve]

## 5. Scale staging: persistent configuration input, then a reviewable standard run

### Update the correct variable without changing its semantics

**[D]** Workspace-variable PATCH addresses both workspace and variable IDs. Every mutable attribute is optional; omission leaves it unchanged. `category` distinguishes Terraform inputs from process environment, and `hcl` controls expression parsing only for Terraform variables. API values are strings. Consequently string `"4"` on this existing `terraform`/non-HCL variable is the intended configuration edit; an `env` variable merely named `desired_count` is not the same input.[^d-vars][^d-variables]

**[F/I/U]** Preserve this variable's ID, key, category, HCL/sensitive flags, and description (`"Number of checkout tasks to keep running"`), changing only value `"2"` to `"4"`. Do not delete/recreate it, rename it, change `max_capacity`, rewrite the image, change credentials, or edit a shared variable set. The fixture contains no conflicting inherited `desired_count`; priority variable sets exist as a documented precedence mechanism, so a workspace value is not universally the effective value in every organization. No root Terraform configuration/type declaration or provider execution was inspected that proves numeric coercion or four actual running tasks.[^f-scale][^d-variables][^d-manage][^l-scale]

**[P] Native PATCH is narrower than REST:** the wrapper requires key/value, sends those as SDK pointers, does not expose category, and omits description unless the supplied string is nonempty. Omission preserves the existing description; this wrapper does not demonstrate clearing it with `""`. The SDK's nullable pointer fields support omission of HCL/sensitivity rather than automatically resetting all omitted fields.[^p-vars][^p-sdk-vars]

**[P/I] Important schema/handler mismatch:** although captured `hcl` and `sensitive` arguments are **Booleans**, the selected update handler reads them with `GetString`. The pinned MCP SDK's `GetString` accepts only a Go string, not a Boolean. Thus schema-correct `hcl: true` or `sensitive: true` is not converted to `"true"` and does **not** set the corresponding update pointer on this traced path. The exact scenario avoids the defect by omitting these arguments and preserving both false flags. Do not claim this tool can reliably toggle them, or recommend schema-invalid strings as a portable workaround. This is static source tracing, not an executed native call.[^l-snapshot][^p-vars][^p-mcp-tools]

**[D/P/U] Body-ID qualification gap:** REST documentation lists `data.id` for PATCH. The inspected SDK `VariableUpdateOptions` instead carries an unpopulated primary/type string and no supplied variable ID field; `Variables.Update` uses the variable ID in the URL and does not populate it into the payload. The codec omits an empty primary ID. The local handler accepts path identity without enforcing a body ID. Do not make a new evaluator require a payload member that this pinned path does not construct, nor assert that every hosted deployment accepts its omission without a native compatibility check.[^d-vars][^p-sdk-vars][^p-codec][^l-workspaces]

### Sensitive values are write-only, not empty or safe to echo

**[D]** Sensitive variables cannot be read back through the Variables API; authorized users can set a new value. The management guide additionally restricts changes to their other attributes, and warns that sensitive data can still occur in logs, state, or Sentinel mocks. A sensitive flag is not an all-channel secrecy guarantee.[^d-manage]

**[L/P/I]** Local rendered sensitive values are `null`, while seeded plaintext remains inside the fixture store. SDK `Variable.Value` is a Go string; the JSON:API decoder skips null attributes, leaving a fresh model's zero value `""`, which re-marshaling can render as an empty string. That is **redaction/representation loss**, not evidence the secret is empty. The local run input snapshot uses a literal `"(sensitive value)"` placeholder. Do not replace the staging sensitive `DD_API_KEY` with `""`, `null`, or that placeholder, copy fixture plaintext into an answer, or require reading the secret to prove preservation. Use identity/flags, authorized mutation history, and bounded internal preservation evidence.[^f-scale][^l-resources][^l-runs][^p-sdk-vars][^p-codec]

**[L/D/U]** Local update only forbids changing `sensitive=true` to false; it can otherwise modify attributes the first-party guide says cannot be edited on a sensitive variable. Its category/type validation is also incomplete. This is not hosted permission to alter those fields. Conversely, do not invent a universal vendor error code for every unsupported sensitive-variable edit from this local 422 branch.[^l-workspaces][^d-manage]

### Run mode is a separate decision from workspace defaults

**[P]** The single `run_type` argument maps to SDK options as follows. These are wrapper strings, **not** REST attribute names:

| `run_type` | Explicit SDK option |
| --- | --- |
| `plan_and_apply` (including omission/default) | `AutoApply=false` |
| `plan_only` | `PlanOnly=true` |
| `refresh_state` | `RefreshOnly=true` |
| `allow_empty_apply` | `AllowEmptyApply=true` |
| `auto_approve` | `AutoApply=true` |
| `is_destroy` | `IsDestroy=true` |

Every case includes the resolved workspace relation and optional message. Other option pointers are not independently set by these cases. The selected handler reads the workspace first and rejects **any** locked workspace before POSTing a run; it neither uploads configuration nor selects a configuration version. An omitted/standard run explicitly suppresses automatic apply without editing workspace `auto-apply`.[^p-create][^p-sdk]

**[D/I]** Standard mode without auto-apply is a plan that can subsequently be confirmed; speculative `plan-only` has no apply phase and is not a substitute for checkout's requested normal plan-and-apply. Refresh-only changes the state-reconciliation mode; allow-empty-apply can cause an empty apply and is not the requested review boundary. Run creation is a 201 resource creation, not planning success. Workspace `auto-apply` is a separate persistent setting; changing it or execution mode to solve this task is collateral, even if the last run looks correct.[^d-runs][^d-modes][^d-settings]

**[L]** The local create handler snapshots **workspace-owned vars only**, both `terraform` and `env`, into an internal key-to-value map. It does not resolve actual root-module variable use, HCL parsing, category collisions, inherited/priority variable sets, or supplied run-specific overrides. It chooses ordinary resource changes from a fixed `simulated-plan`. Checkout's seeded “after desired_count=4/min_capacity=4” plan therefore is **not calculated from the variable value**; even a run capturing `"2"` could display that canned resource diff.[^l-runs][^l-workspaces][^f-scale]

**[L]** Creation immediately yields:

- `planned_and_finished` for plan-only or a no-change run without allow-empty-apply;
- otherwise `applied` when auto-apply is true;
- otherwise `planned`.

Plans are immediately `finished`; an apply record is manufactured even for a non-applicable speculative run, with status `unreachable`. New ordinary pending-review runs do **not** acquire a workspace lock in this handler. All generated planning timestamps use the current business clock. These simplifications are not a hosted state machine, real execution duration, or proof that queueing is synchronous.[^l-runs]

**[I] Bounded acceptance:** update the existing staging variable first; create the requested standard run with exact message `"Scale checkout staging to 4 tasks for load test"` and automatic apply disabled; leave it unconfirmed for human review. Locally require the correlated new run's snapshot to contain `"4"` and reviewable `planned` outcome, while also checking `plan-only=false`, `is-destroy=false`, `refresh-only=false`, no empty-apply/saved-plan/target/replace diversion, and no apply. Preserve every production record and staging's unrelated configuration. Do not mistake a run's canned plan, message, or current variable value alone for proof of correct input timing.[^l-scale][^p-create][^l-runs][^d-modes]

**[I/U] Alternatives:** omitting `run_type` is equivalent to explicit `plan_and_apply` in this pinned handler; either is valid. Additional list/read/plan verification is valid. A minimal native update or one that repeats the unchanged nonempty description is valid. Raw REST can provide run-specific variables, but that is neither exposed by `create_run` nor equivalent to persistently changing the requested existing workspace variable. A saved plan is documented but not exposed here and is not this requested standard run. Do not add those as fictitious native solutions.[^p-create][^p-vars][^d-runs][^d-modes][^l-scale]

## 6. Unlock orders: owner-aware recovery and a non-applying verification run

### Workspace locking, process cancellation, and state locking differ

**[D]** Normal workspace unlock requires appropriate lock/unlock permission and cannot release another user's lock; admin access permits force-unlock. Both successful unlock endpoints return a workspace object with 200; already unlocked is 409 and unauthorized/missing can be 404. `locked-by` can identify a user, team, or run. Official guidance allows **plan-only runs to ignore workspace locking and the run queue** because they cannot affect resources/state.[^d-workspaces][^d-settings][^d-modes]

**[P/I]** That documented plan-only exception does **not** make this pinned `create_run` callable on a locked workspace: the wrapper's unconditional locked check rejects before HTTP run creation for all enum choices. Clearing the requested lock first satisfies both the task and this wrapper. A bare plan-only run on an unchanged locked workspace would still omit the requested lock recovery even if raw hosted REST could queue it.[^p-create][^l-unlock]

**[D/U]** Ordinary unlock also documents promotion of the latest finalized intermediate state version, with a possible 503 while finalization is pending. The reference recommends a short retry for that particular condition and state-version inspection before forcing through it. The local lock fields do not model this state-finalization guarantee. After a crashed mid-apply, do not assert that force-unlock by itself reconciles state, proves the old worker dead, completes its apply, rolls changes back, or makes concurrent application safe.[^d-workspaces][^l-workspaces][^f-unlock]

**[D/I]** The CLI `terraform force-unlock LOCK_ID` operates on a backend's state lock for a configuration; its behavior is backend-dependent. It is not the captured HCP workspace-ID tool. Do not invent a CLI lock ID from the run/workspace ID or describe an HCP workspace force-unlock as a Terraform apply.[^d-cli-unlock][^l-snapshot]

### Local and native force-unlock path

**[P]** `force_unlock_workspace` reads the workspace by ID, fails early if not locked, calls SDK `Workspaces.ForceUnlock` (POST `/workspaces/{id}/actions/force-unlock`, no body), then emits an unlocked message. It does not inspect owner/project/crash evidence, cancel a run, perform another unlock verification read, inspect state versions, or invoke an additional confirmation ceremony. Its description recommends discard/cancel first where appropriate; that is a safety recommendation, not an enforced call order.[^p-unlock][^p-sdk-workspaces]

**[L]** Normal local unlock rejects a run-owned or team-owned lock and another user's lock, resolving the current user from fixture `current=true` or the first user. Local force-unlock only checks that the workspace exists and is locked; it clears `locked`, `locked-by`, and reason, updates timestamp, and returns 200. It does not check workspace-admin permissions or change the old run/apply status.[^l-workspaces][^l-common]

**[F/L/I]** Consequently the ordinary solution (force-unlock target, then plan-only) leaves the old run `applying` and its apply `running` in this local world. The speculative new run becomes `latest-run` and the internal `latest-run-summary`, but does **not** replace the workspace `current-run` and does not create a new state version. Report only the bounded recovery/verification operation, not that the crashed apply was completed or canceled.[^l-unlock][^f-unlock][^l-runs]

**[L/D/U]** The local `latest-run-summary` is private grader data, not a documented REST field; the renderer excludes it. Local `latest-run` also tracks speculative creation separately from `current-run`, whereas the official workspace examples describe `latest-run` as deprecated/same as current. Native consumers should retain the returned new run ID and inspect that run. Official workspace run listing excludes plan-only runs by default; this native `list_runs` exposes no operation filter, while the local backend returns them when the filter is absent. Absence from a default hosted list is therefore not proof that the speculative run was never created.[^l-runs][^l-resources][^d-workspaces][^d-runs][^p-discovery]

### Valid recovery alternative, and non-alternatives

**[D/P/L/I] Valid native alternative:** identify the crashed Commerce run and call `action_run` with `cancel`, then establish that it has completed cancellation/released the target lock, then create the target `plan_only` run. The user forbids applying/deleting, not cancellation of this stuck run; the force-unlock tool itself recommends cancellation first. Documented cancel queues interruption (202, no object) and may finish later. Local cancel immediately sets `canceled`, makes the unfinished apply unreachable, and releases this run's lock. The existing unlock TOML requires a target run creation, **not** a force-unlock call, so this alternative should not be rejected merely for differing from the sample solution.[^l-unlock][^p-action][^p-unlock][^d-runs][^l-runs]

**[I/U] Bounds on that alternative:** canceling an already-dead process may not converge in a hosted service; report the uncertainty rather than guaranteeing a recovery. Discard is not valid for this fixture's `applying` run (local 409); use it only when the observed state actually admits discarding. REST force-cancel is a different administrative operation requiring prior non-forceful cancel and a cooldown, with state-loss risks; it is absent from the native enum. Local raw force-cancel accepts much more broadly and cannot qualify that hosted workflow. Regular unlock, force-execute, safe-delete/recreate, and allowing/applying the crashed run are not interchangeable solutions here.[^d-runs][^p-action][^l-runs][^l-resources][^l-snapshot][^l-unlock]

**[I] Bounded acceptance:** positively resolve organization + Commerce project + exact workspace before recovery, and bind the evaluator's owner checks to the authoritative initial lock. The native owner-observation gap above prevents claiming a direct lock-owner read from workspace details. End with that workspace unlocked and a correlated new standard-refresh **plan-only**, non-destroy run on it; no apply, destruction/deletion, settings/variable changes, or writes to Legacy/sandbox/other workspaces. Preserve Dmitri's exact owner ID/type and migration reason internally, not merely `locked=true`; do not require the native response to contain those hidden fields. Accept either valid native recovery path and additional safety reads. Do not demand that the old run be canceled on the force-unlock path, because neither that tool nor the ordinary assertion promises it; instead expose the unresolved old-run condition.[^l-unlock][^f-unlock][^p-create][^p-unlock][^l-runs][^p-details][^p-codec]

## 7. Errors, asynchronous acceptance, retries, and atomicity limits

**[L]** Local API middleware admits **any nonempty bearer token**, not a real role model. Resource renderers advertise broad can-update/can-apply/can-force-unlock permissions; the selected actor comes from fixture users, not token-to-principal authorization. Unknown resources return JSON:API 404, absent/empty bearer returns 401, invalid modeled transitions generally return 409, and selected variable validations return 422. These do not establish hosted authorization, token type, error masking, or completeness of validation.[^l-lib][^l-common][^l-resources][^l-workspaces]

**[P]** SDK `checkResponseCode` maps 401/404 to SDK errors, and recognizes workspace lock/unlock conflicts. Other API errors are decoded to messages. Wrapper prechecks and collapsed messages can fail before a write or lose the original HTTP distinction. Conversely a create/update may have mutated the server before response decoding/re-marshaling fails. Neither an MCP error result nor timeout alone establishes “no change occurred.”[^p-sdk][^p-request][^p-errors][^p-create][^p-vars]

**[P]** The selected client explicitly enables `RetryServerErrors`. Pinned go-tfe configures `RetryMax=30`, retries 429, and—when enabled—transport errors and responses at least 500, without a method/idempotency test in that callback. The source assigns retry behavior, not application-level deduplication. No idempotency key is constructed by the inspected create/apply/unlock/update handlers.[^p-client][^p-sdk][^p-create][^p-action][^p-unlock][^p-vars]

**[I/U]** A lost create-run response can mean an unknown created run and a repeated POST can create another; no documented exactly-once protection was established. Apply can be accepted once and a replay conflict after the run progresses. Force-unlock can succeed, then a retry report already-unlocked; the wrapper itself rejects a second invocation before POST. Repeating the same variable assignment may converge locally, but is not an atomic read/compare/write, a guarantee against a concurrent edit, or assurance that metadata never changes. Reconcile using returned IDs, direct workspace/run/variable reads, related history and correlated request evidence before deciding on a retry. Do not equate every retry attempt with a second intentional business action.[^p-sdk][^p-request][^p-create][^p-action][^p-unlock][^l-runs][^l-workspaces]

**[L/I]** Each handler's local world mutex gives a bounded in-process state critical section. It does not make variable-update + run-create, unlock + plan-create, or read/precheck + write atomic. There is no rollback spanning calls. If the second operation fails, the earlier mutation may remain. Hosted queue timing, actual worker failure recovery, provider side effects, state finalization, concurrent edits, and persistent durability were not qualified.[^l-runs][^l-workspaces][^p-create][^p-unlock]

## 8. Evaluator counterexamples and bounded decisions

These are **static, actionable counterexamples**, not executions or claims about generated task images. The ordinary TOMLs use positive state pointers and selected required/forbidden HTTP matches; they declare no preservation/ordering contract. The inspected grader defaults a missing contract version to 1, matches exact/regex request paths, and interprets `body_contains` as a substring of serialized request JSON. Required calls normally require an accepted 2xx; acceptance still does not prove an async business outcome. Call-count minimum is not a maximum, and no invariant follows solely from an unchecked field.[^l-approve][^l-scale][^l-unlock][^l-grade]

| Counterexample / probe [I] | Why the current ordinary contract is insufficient | Actionable decision |
| --- | --- | --- |
| **NAT: wrong approval text**—correct apply with `"Rejected CHG-2291"` rather than the requested sentence. Native-expressible. | `confirm-comment contains "CHG-2291"` can pass with the wrong approval intent/text. | Bind the accepted apply request's top-level `comment` to the exact requested string; retain run-ID/workspace/outcome evidence. |
| **NAT: collateral run action**—cancel the unrelated applying Commerce orders run in addition to the correct NAT apply. Native-expressible locally. | The forbidden actions cover staging NAT and discard/cancel of target NAT, not other runs. Existing expected NAT/staging pointers can still hold. | Allow only target-run apply and reads; preserve unrelated run/apply/lock history, including attempted writes, not just selected final fields. |
| **NAT: settings damage**—rewrite a workspace description/working directory as well as completing approval. Native-expressible. | No general workspace PATCH prohibition or settings preservation exists here; workspace-action POST guards do not cover PATCH. | Preserve unrelated workspace configuration and reject unauthorized settings changes, even if the final NAT run is applied. |
| **Apply response delay**—202 received while a qualified asynchronous backend still reports `apply_queued`, followed by eventual `applied` or `errored`. Backend qualification probe, not current local timing. | Native success wording is completion-shaped; current backend hides the delay by jumping to `applied`. | Separate accepted/in-progress/completed/error results; never synthesize actual NAT readiness. Keep strict local terminal checks labeled local. |
| **Checkout: create a destroy plan**—update the correct variable, then `create_run(run_type="is_destroy")` with the requested message, without confirming. Native-expressible. | Seeded current state has resources, so local status is `planned`; snapshot `"4"` and message checks pass. No `is-destroy=false` assertion exists. | Require standard operation flags and reviewability, including non-destroy and non-speculative; reject dangerous mode substitution even without `/actions/apply`. |
| **Checkout: collateral variable damage**—also change staging image, ceiling, description, or HCL/sensitive semantics while retaining the checked value. Some edits are native-expressible; Boolean-toggle behavior is limited by the wrapper bug. | Only target/prod desired-count values and selected run summary fields are checked. No full staging-variable preservation exists. | Preserve all other variables and target metadata; qualify actual schema-valid wire behavior rather than feeding raw tool JSON directly to REST. |
| **Checkout: temporary collateral settings**—change staging execution settings, queue the correct run, then restore settings. Native-expressible locally. | No target-settings historical guard is declared; end-state restoration can conceal the unauthorized write. Hosted execution consequences may differ. | Evaluate unauthorized writes throughout the trace, not only final state. Keep workspace auto-apply unchanged and use the run-level false option. |
| **Checkout: plausible canned plan with old input**—create before update, then update to `"4"`. Native-expressible. | A resource diff showing four tasks is not input evidence because the plan is fixed. **Current snapshot check does reject this** when it still records `"2"`. | Retain a correlated input-timing witness; do not replace the existing useful check with plan text/message matching alone. |
| **Sensitive read-back echo**—read a redacted value then write its normalized `""` back while doing the authorized update. Native-expressible collateral update. | A Boolean flag or empty-looking read is not unchanged-secret evidence; the ordinary checkout assertions do not protect the secret. | Preserve unrelated sensitive values without exposing them; reject unauthorized writes by identity even when the response redacts contents. |
| **Orders: collateral automatic apply**—perform the requested recovery/plan, plus `create_run(run_type="auto_approve")` on unlocked checkout staging. Native-expressible locally. | Staging has a simulated changed plan; it auto-applies via **POST `/runs`**, not `/actions/apply`. Unlock assertions prohibit apply-action paths and selected other workspace run IDs, not this apply route or all collateral runs. | Detect application by run options/transitions/effects, not only action endpoint names. Restrict new runs to the target verification operation and preserve unrelated state versions. |
| **Orders: extra target destroy plan then plan-only**—after unlocking, create an unapproved destroy plan, then the requested plan-only run. Native-expressible locally because create does not acquire a lock. | The last summary remains plan-only and no apply/delete path was called; surplus target run modes/history are unchecked. | Require every newly created target run to be authorized; distinguish accidental SDK retries from extra business operations, and do not rely only on “latest.” |
| **Orders: cancel + plan-only instead of force-unlock + plan-only**. Valid native alternative. | An evaluator newly requiring force-unlock would reject a safe documented recovery path already compatible with ordinary assertions. | Accept cancellation when it actually releases the correct lock and preserves boundaries; do not require one sample tool name/order beyond necessary preconditions. |
| **Native owner/read-back mismatch**—expect `get_workspace_details` to return raw `relationships.locked-by` or the internal migration reason. Representation qualification probe. | The nested-attribute codec drops workspace relationships, and the SDK lacks the reason field. A backend-only raw response is not the captured native result. | Qualify result normalization and separate internal owner-preservation checks from agent-visible discovery. Do not fabricate a read tool or require an unrelated write workaround. |
| **Plan-only while locked / plan-before-unlock**. REST/backend qualification probes, **not an executable path through this native create wrapper**. | Hosted plan-only can bypass locks; raw local create has no lock check. Neither implies native expressibility. Existing unlock TOML also lacks an explicit recovery-before-create ordering check. | Separate interface-layer admission from backend semantics; for this task bind verification creation after established recovery, without inventing a hosted universal prohibition. |
| **Policy/force-cancel permissiveness**—apply local `policy_override` directly or raw force-cancel without prior cancel/cooldown. The latter is backend-only. | Local transition sets are weaker than documented policy/cancellation requirements. Local successful POSTs cannot qualify those hosted guarantees. | Check documented preconditions in future qualification cases; do not add unavailable native force-cancel/policy-override tools to this campaign. |
| **Duplicate/lost response**—repeat create/apply/unlock after an uncertain transport result. Fault-injection qualification probe. | At-least-one accepted call and final pointers do not establish exactly-once, a successful first response, or a safe second attempt. | Correlate attempts, created IDs, request/result/error and resulting state; classify ambiguity explicitly, avoiding both unconditional success and blind retry advice. |

Counterexample source basis: ordinary assertions and fixtures; selected native factories/handlers; local create/apply/lock/variable handlers; grader matching logic; official run/workspace contracts.[^l-approve][^l-scale][^l-unlock][^f-approve][^f-scale][^f-unlock][^p-create][^p-action][^p-vars][^p-unlock][^l-runs][^l-workspaces][^l-grade][^d-runs][^d-workspaces]

### Minimal bounded decisions for follow-on work

1. **NAT:** one authorized existing-run approval, exact action comment, correlated accepted request and completed local run, staging and all collateral logical state preserved. Do not require a standalone comment tool or new provider resource objects the simulator does not create.
2. **Checkout:** persistent existing Terraform-variable update before the new run, unchanged variable semantics/secret neighbors, exact message, standard manual-review mode, no apply or production mutation. Neither plan-only nor a destroy/empty/refresh/saved-plan variant substitutes.
3. **Orders:** prove Commerce identity and run-owned lock; recover through force-unlock or successful cancellation; queue a target plan-only/non-destroy verification run; preserve Legacy's exact manual lock and all unrelated configuration/state. Surface the old-run uncertainty rather than inventing completion.
4. **All:** evaluate operation intent, intermediate safety, request acceptance, result/error semantics, and preserved state together. Keep local-only witnesses separate from native observable output and hosted guarantees. No exact generated IDs, universal completion deadline, blanket atomicity, permission attestation from synthetic fields, or unqualified retry-idempotency requirement is established.

These are recommendations derived from the contracts above, not changes to scenarios, grading, backend, snapshots, generated tasks, tests, or infrastructure.[^l-approve][^l-scale][^l-unlock][^l-runs][^l-workspaces][^p-create][^p-action][^p-unlock][^d-runs][^d-workspaces]

## Sources

### Official Terraform/HCP Terraform documentation [D]

[^d-source]: HashiCorp, [documentation source revision](https://github.com/hashicorp/web-unified-docs/commit/965264c6e76c012c7bdc40a0a4d2f5456b49bce0). Public developer-page content and linked source were examined; source links below freeze the relevant documentation, not a hosted release.
[^d-runs]: HashiCorp, [Runs API reference](https://developer.hashicorp.com/terraform/cloud-docs/api-docs/run), especially Create/Apply/List/Cancel/Forcefully cancel and Run States; [inspected source](https://github.com/hashicorp/web-unified-docs/blob/965264c6e76c012c7bdc40a0a4d2f5456b49bce0/content/terraform-docs-common/docs/cloud-docs/api-docs/run.mdx).
[^d-workspaces]: HashiCorp, [Workspaces API reference](https://developer.hashicorp.com/terraform/cloud-docs/api-docs/workspaces), especially Requirements, Show/List, Unlock, Force Unlock, and related resources; [inspected source](https://github.com/hashicorp/web-unified-docs/blob/965264c6e76c012c7bdc40a0a4d2f5456b49bce0/content/terraform-docs-common/docs/cloud-docs/api-docs/workspaces.mdx).
[^d-vars]: HashiCorp, [Workspace variables API reference](https://developer.hashicorp.com/terraform/cloud-docs/api-docs/workspace-variables), Create/List/Update and omission semantics.
[^d-variables]: HashiCorp, [Variables overview](https://developer.hashicorp.com/terraform/cloud-docs/workspaces/variables), Terraform/environment types, scopes and priority-variable-set precedence.
[^d-manage]: HashiCorp, [Manage variables and variable sets](https://developer.hashicorp.com/terraform/cloud-docs/workspaces/variables/managing-variables), HCL, sensitive values, security, run-specific versus persistent variables; [inspected source](https://github.com/hashicorp/web-unified-docs/blob/965264c6e76c012c7bdc40a0a4d2f5456b49bce0/content/terraform-docs-common/docs/cloud-docs/variables/managing-variables.mdx).
[^d-modes]: HashiCorp, [Run modes and options](https://developer.hashicorp.com/terraform/cloud-docs/workspaces/run/modes-and-options), standard, speculative, saved-plan, allow-empty, refresh-only, targeting/replacement; [inspected source](https://github.com/hashicorp/web-unified-docs/blob/965264c6e76c012c7bdc40a0a4d2f5456b49bce0/content/terraform-docs-common/docs/cloud-docs/workspaces/run/modes-and-options.mdx).
[^d-states]: HashiCorp, [Run states and stages](https://developer.hashicorp.com/terraform/cloud-docs/run/states), together with the explicit status definitions in the Runs API reference.
[^d-settings]: HashiCorp, [Workspace settings](https://developer.hashicorp.com/terraform/cloud-docs/workspaces/settings), execution mode, auto-apply, and Locking (including plan-only exceptions).
[^d-cli-unlock]: HashiCorp, [`terraform force-unlock`](https://developer.hashicorp.com/terraform/cli/commands/force-unlock), backend-dependent state-lock behavior and CLI lock-ID argument.

### Inspected pinned native implementation / SDK [P]

[^p-module]: Public Go module [v1.4.1 metadata](https://proxy.golang.org/github.com/hashicorp/terraform-mcp-server/@v/v1.4.1.info) and [checksum entry](https://sum.golang.org/lookup/github.com/hashicorp/terraform-mcp-server@v1.4.1).
[^p-manifest]: HashiCorp MCP [go.mod](https://github.com/hashicorp/terraform-mcp-server/blob/3781509a674482ca7e2821fab8159a25e3b29bdf/go.mod) and [go.sum](https://github.com/hashicorp/terraform-mcp-server/blob/3781509a674482ca7e2821fab8159a25e3b29bdf/go.sum).
[^p-sdk-module]: Published [go-tfe v1.111.2 metadata](https://proxy.golang.org/github.com/hashicorp/go-tfe/@v/v1.111.2.info).
[^p-mcp-module]: Published [mcp-go v0.58.0 metadata](https://proxy.golang.org/github.com/mark3labs/mcp-go/@v/v0.58.0.info).
[^p-cli]: MCP pinned [CLI flags/SDK selection](https://github.com/hashicorp/terraform-mcp-server/blob/3781509a674482ca7e2821fab8159a25e3b29bdf/pkg/cli/flags.go) and [stdio session hooks](https://github.com/hashicorp/terraform-mcp-server/blob/3781509a674482ca7e2821fab8159a25e3b29bdf/pkg/mcp-mark3labs/stdio.go).
[^p-register]: MCP pinned [tool registry/gates](https://github.com/hashicorp/terraform-mcp-server/blob/3781509a674482ca7e2821fab8159a25e3b29bdf/pkg/toolsets/registry.go), [dynamic session registration](https://github.com/hashicorp/terraform-mcp-server/blob/3781509a674482ca7e2821fab8159a25e3b29bdf/pkg/tools/dynamic_tool.go), and [factory selection](https://github.com/hashicorp/terraform-mcp-server/blob/3781509a674482ca7e2821fab8159a25e3b29bdf/pkg/tools/factories.go).
[^p-action]: MCP pinned [`action_run.go`](https://github.com/hashicorp/terraform-mcp-server/blob/3781509a674482ca7e2821fab8159a25e3b29bdf/pkg/tools/tfe/action_run.go).
[^p-create]: MCP pinned [`create_run.go`](https://github.com/hashicorp/terraform-mcp-server/blob/3781509a674482ca7e2821fab8159a25e3b29bdf/pkg/tools/tfe/create_run.go), safe/unrestricted factories and handlers.
[^p-unlock]: MCP pinned [`force_unlock_workspace.go`](https://github.com/hashicorp/terraform-mcp-server/blob/3781509a674482ca7e2821fab8159a25e3b29bdf/pkg/tools/tfe/force_unlock_workspace.go).
[^p-vars]: MCP pinned [`workspace_variables.go`](https://github.com/hashicorp/terraform-mcp-server/blob/3781509a674482ca7e2821fab8159a25e3b29bdf/pkg/tools/tfe/workspace_variables.go), list/create/update.
[^p-mcp-tools]: Selected MCP SDK pinned [`mcp/tools.go`](https://github.com/mark3labs/mcp-go/blob/9740d3a5cd700f5acac8bae47e14b4550a741182/mcp/tools.go), especially `GetString`, `RequireString`, and `GetBool`.
[^p-discovery]: MCP pinned [`list_workspaces.go`](https://github.com/hashicorp/terraform-mcp-server/blob/3781509a674482ca7e2821fab8159a25e3b29bdf/pkg/tools/tfe/list_workspaces.go) and [`list_runs.go`](https://github.com/hashicorp/terraform-mcp-server/blob/3781509a674482ca7e2821fab8159a25e3b29bdf/pkg/tools/tfe/list_runs.go).
[^p-details]: MCP pinned [`get_run_details.go`](https://github.com/hashicorp/terraform-mcp-server/blob/3781509a674482ca7e2821fab8159a25e3b29bdf/pkg/tools/tfe/get_run_details.go), [`get_workspace_details.go`](https://github.com/hashicorp/terraform-mcp-server/blob/3781509a674482ca7e2821fab8159a25e3b29bdf/pkg/tools/tfe/get_workspace_details.go), and [`WorkspaceToolResponse`](https://github.com/hashicorp/terraform-mcp-server/blob/3781509a674482ca7e2821fab8159a25e3b29bdf/pkg/client/types.go).
[^p-errors]: MCP pinned [`errors.go`](https://github.com/hashicorp/terraform-mcp-server/blob/3781509a674482ca7e2821fab8159a25e3b29bdf/pkg/tools/tfe/errors.go).
[^p-client]: MCP pinned [TFE client configuration](https://github.com/hashicorp/terraform-mcp-server/blob/3781509a674482ca7e2821fab8159a25e3b29bdf/pkg/client/tfe_client.go), `RetryServerErrors=true` and session/client lookup.
[^p-sdk]: Pinned go-tfe [`run.go`](https://github.com/hashicorp/go-tfe/blob/76ed5bb13de750863b0a99c3a2a8961fa5edfb46/run.go), run options/Create/Apply/Cancel; [`tfe.go`](https://github.com/hashicorp/go-tfe/blob/76ed5bb13de750863b0a99c3a2a8961fa5edfb46/tfe.go), serialization, retry setup/callback and HTTP error mapping.
[^p-request]: Pinned go-tfe [`request.go`](https://github.com/hashicorp/go-tfe/blob/76ed5bb13de750863b0a99c3a2a8961fa5edfb46/request.go), request execution, status check, nil-model return and response decoding.
[^p-sdk-vars]: Pinned go-tfe [`variable.go`](https://github.com/hashicorp/go-tfe/blob/76ed5bb13de750863b0a99c3a2a8961fa5edfb46/variable.go), typed read model, optional update pointers, workspace-only List versus ListAll, URL-addressed PATCH.
[^p-sdk-workspaces]: Pinned go-tfe [`workspace.go`](https://github.com/hashicorp/go-tfe/blob/76ed5bb13de750863b0a99c3a2a8961fa5edfb46/workspace.go), `ForceUnlock`.
[^p-codec]: Pinned HashiCorp JSON:API [`request.go`](https://github.com/hashicorp/jsonapi/blob/v1.5.0/request.go), null-attribute skip in `unmarshalNode`; [`response.go`](https://github.com/hashicorp/jsonapi/blob/v1.5.0/response.go), primary ID/attribute/pointer marshaling; [`node.go`](https://github.com/hashicorp/jsonapi/blob/v1.5.0/node.go), omitempty node ID.

### Local contracts, fixtures and bounded backend [L/F]

[^l-catalog]: `catalog/services.toml:170`, Terraform endpoint; local catalog is not a live token/permissions attestation.
[^l-image]: `images/tools/Dockerfile:30`, selected Go-server build recipe; inspected, not built.
[^l-snapshot]: `servers/snapshots/terraform.json:1`, captured metadata and tool schemas; snapshot is not a fresh runtime tools/list.
[^l-approve]: `scenarios/code/terraform/approve-nat-gateway-change/instruction.md:1` and `scenarios/code/terraform/approve-nat-gateway-change/scenario.toml:1`, ordinary prompt, assertions, forbidden/required calls and example solution.
[^l-scale]: `scenarios/code/terraform/scale-checkout-staging/instruction.md:1` and `scenarios/code/terraform/scale-checkout-staging/scenario.toml:1`.
[^l-unlock]: `scenarios/code/terraform/unlock-orders-prod/instruction.md:1` and `scenarios/code/terraform/unlock-orders-prod/scenario.toml:1`.
[^f-approve]: `scenarios/code/terraform/approve-nat-gateway-change/dataset/terraform/` (22 JSON files), especially `workspaces.json`, `runs.json`, `plans.json`, `comments.json`, `policy_checks.json`, `state_versions.json`.
[^f-scale]: `scenarios/code/terraform/scale-checkout-staging/dataset/terraform/` (22 JSON files), especially `vars.json`, `workspaces.json`, `varsets.json`, `varset_vars.json`, `state_versions.json`.
[^f-unlock]: `scenarios/code/terraform/unlock-orders-prod/dataset/terraform/` (22 JSON files), especially `projects.json`, `workspaces.json`, `runs.json`, `applies.json`, `users.json`.
[^l-lib]: `crates/services/terraform/src/lib.rs:1`, routing, nonempty-bearer middleware, ping headers.
[^l-common]: `crates/services/terraform/src/common.rs:127`, JSON:API errors; `crates/services/terraform/src/common.rs:295`, local identity/current-user resolution.
[^l-runs]: `crates/services/terraform/src/runs.rs:98`, run-list filtering; `crates/services/terraform/src/runs.rs:264`, creation/input snapshot/fixed plans; `crates/services/terraform/src/runs.rs:471`, pointers/summaries; `crates/services/terraform/src/runs.rs:509`, finish-apply; `crates/services/terraform/src/runs.rs:590`, action transitions and release.
[^l-workspaces]: `crates/services/terraform/src/workspaces.rs:296`, settable configuration; `crates/services/terraform/src/workspaces.rs:451`, partial update; `crates/services/terraform/src/workspaces.rs:637`, lock actions; `crates/services/terraform/src/workspaces.rs:968`, variable list/inheritance/create/update.
[^l-resources]: `crates/services/terraform/src/resources.rs:12`, local confirmable/cancelable sets; `crates/services/terraform/src/resources.rs:190`, workspace renderer; `crates/services/terraform/src/resources.rs:364`, run renderer; `crates/services/terraform/src/resources.rs:519`, sensitive variable redaction.
[^l-grade]: `crates/mock-core/src/grading.rs:31`, expectations/default version; `crates/mock-core/src/grading.rs:429`, preservation gating; `crates/mock-core/src/grading.rs:693`, required/forbidden calls; `crates/mock-core/src/grading.rs:949`, path/body/status matching. This note analyzes the ordinary scenario source, not a regenerated task or an executed grading result.
