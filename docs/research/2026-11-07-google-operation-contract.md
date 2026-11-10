# Google Workspace write-operation contract

Research date: **November 7, 2026**. Scope: `collab/google/{mark-northwind-renewed,schedule-q1-planning,share-soc2-with-contoso}`; their authored instructions, datasets, assertions, captured community MCP surface, local `crates/services/google` implementation, and public primary sources. This is **research only**, not implementation, qualification, or approval of an evaluator. The All24 legacy-write scope and full 19-item overhaul remain active; this three-scenario investigation does not replace either. No builds, tests, Docker, evaluated-tool/model calls, live Google accounts, sharing, calendar operations, infrastructure changes, or commits were performed. The parent's frozen Datadog38 source/evidence was not operated on or changed.

## Evidence classes and authored dates

- **[D] Documented vendor fact:** current first-party Google documentation; not a historical hosted-server implementation pin.
- **[P] Pinned published code:** inspected release artifacts or immutable source commits. An upstream lock is distinguished from the dependencies actually installed during a capture.
- **[L] Local convention/observation:** scenario data, assertions, capture declarations, or replica code. Not a Google guarantee.
- **[I] Inference/recommendation:** a proposed business/evaluator interpretation derived from identified evidence.
- **[U] Unestablished:** not settled by these sources; requires an explicit policy or further authorized evidence.

All three scenarios author `now = "2026-10-29T15:00:00Z"`. Preserve that business clock. Priya's requested meeting is **Tuesday, November 3, 2026**, not a date relative to November 7. Lena's new contract end is **November 30, 2027**; “yesterday” in her October 29 email refers to October 28. “Most recent” means the latest qualifying final report in the authored fixture, not the most recently modified arbitrary SOC 2 document at research time. **[L/I]** [L-renew] [L-plan] [L-share] [L-mail]

| Scenario | Authored requirement, resolved from email and data | Preservation boundary |
| --- | --- | --- |
| `mark-northwind-renewed` | Current **Vendor renewals 2026** → **Renewals** → **Northwind Traders**: contract end `2027-11-30`, status `Renewed`. | All other vendors, existing target-row identity/owner/notes, Archive, older trackers, permissions and unrelated resources. |
| `schedule-q1-planning` | New **Q1 planning sync** on the user's primary calendar, November 3, 14:00–14:45 Eastern, invite Priya and Marcus, with Google Meet. | Existing events, especially August's Q4 planning sync; no unrelated email or calendar edits. |
| `share-soc2-with-contoso` | Share the **final 2026 SOC 2 Type II PDF** with **Dana Whitfield at Contoso**, view-only, not public. | Existing owner/access, report identity/content, all other files/folders; no broader audience or ownership transfer. |

The first two preservation columns combine express instructions with the conservative implication that an update/share request does not authorize rewriting unrelated fields. They are not additional requirements to preserve service-managed timestamps byte-for-byte. **[L/I]** [L-renew-i] [L-plan-i] [L-share-i] [L-mail] [L-files]

## 1. What is selected, pinned, and actually reachable

### Local selection is a replica, not a Google REST proxy

The catalog selects service `google`, `mode = "replica"`, `hosts = []`, endpoint `google-workspace`, and `servers/snapshots/google-workspace.json`. Its source description names **`taylorwilsdon/google_workspace_mcp 2.2.1`, complete tier, all services**. The capture script concretely invokes:

```text
uv tool run --python 3.13 workspace-mcp==2.2.1 --tool-tier complete
USER_GOOGLE_EMAIL=ops@octo-corp.example
```

It also supplies synthetic OAuth-client configuration. This is a tool-list capture recipe, not proof of authorized live Google operations. The local service exposes `POST /_mcp/tools/call` and otherwise returns 404; Google REST routes are not implemented there. **[L]** [L-catalog] [L-capture] [L-lib]

**There is no `servers/lock.json` in the inspected checkout.** `servers/npm/package.json` and its package lock contain no selected Google MCP package; this capture uses PyPI/uv instead. Do not invent a Google npm resolution or a missing lock entry. The snapshot contains **119 tools**, protocol `2025-06-18`, `server_info = {name: "google_workspace", version: "4.1.4"}`, and advertised declaration digest `c16a44830ee65a5ad8635d2faa19617a92bb0b009a8479a5976f428dd6384d2d`. Its actual JSON-file SHA-256 is `3decf05668b63c82a330633685386ead70c2d20c06eec7c9539003edfadf8b22`; these are different hash subjects, not contradictory measurements. **[L]** [L-tools] [L-npm]

### Published source and dependency pins

PyPI publishes `workspace-mcp==2.2.1`. The examined source distribution is:

```text
workspace_mcp-2.2.1.tar.gz
SHA-256 becb74712b56a8052341d6feda4eeb557a77750880daed6c314ef0e1d077bc2b
```

GitHub's annotated `v2.2.1` tag object `1e1d0d4830113a1d4f680476fd235cf68c0bb715` resolves to commit **`0801afb3e58f51754f5a40161c07675f50565448`**. The published sdist and that commit match byte-for-byte for the examined Sheets tools/helpers, Calendar tools/helpers, Drive tools/helpers, service decorator, HTTP-error utility, and `pyproject.toml`. Thus the contracts below are tied to the actual published package, not an assumed current branch. **[P]** [P-pypi] [P-sdist] [P-tag] [P-sheets] [P-calendar] [P-drive] [P-auth] [P-errors]

| Layer | Published requirement or examined pin | What it establishes |
| --- | --- | --- |
| `workspace-mcp` | Exactly `2.2.1` in capture recipe; immutable sdist/hash above. | Package selection and examined source. Not the installed dependency graph. |
| Direct dependencies in its published metadata | `fastmcp>=4.0.10`, `google-api-python-client>=2.201.0`, `google-auth-httplib2>=0.2.0`, `google-auth-oauthlib>=1.2.2`, `pytz>=2026.1.post1`, `tzdata>=2026.1`. | Lower bounds, not exact transitive pins. |
| Upstream commit's `uv.lock` | `fastmcp`/`fastmcp-slim 4.0.10`, `mcp`/`mcp-types 2.2.0`, `pydantic 2.13.0`, `google-api-python-client 2.201.0`, `google-auth 2.49.2`, `google-auth-httplib2 0.3.1`, `google-auth-oauthlib 1.3.1`, `httplib2 0.32.0`, `tzdata 2026.3`. | A reproducible **upstream source-lock reference**, not capture-time installation evidence. The editable root still says `workspace-mcp 2.2.0`. |
| Published Google discovery SDK examined | `google_api_python_client-2.201.0-py3-none-any.whl`, SHA-256 `2d9bf1ba3f12eee8ed3d0f1791ce0605d163432f496baa72d3677faa2cf097d6`; source commit `3974f97bbdfe42c27f0f3f246be2184e41e64291`. | Exact SDK source and bundled REST discovery descriptions, not the Google backend version or proven capture dependency. |
| Framework cross-check | Published `fastmcp-slim 4.1.4` wheel, SHA-256 `0f611b246504ff0d0ee7ab77123f529b5c9aed68670eaee9c6f960a59bb024da`. Its `fastmcp/server/server.py:458` defaults server version to the framework version when no version is provided. Upstream Workspace server construction provides no version. | Explains why snapshot `server_info.version = 4.1.4` can identify the framework rather than contradict package `2.2.1`. This is an association, not a full environment attestation. |

Requirements and lock entries are **[P]**; conclusions about the capture's unresolved environment are **[U/I]**. `uv tool run workspace-mcp==2.2.1` does not consume the repository's absent-sdist `uv.lock` with a frozen sync. The sdist contains no `uv.lock`; package metadata has ranges. Therefore neither the upstream lock nor snapshot version establishes the actual installed Google SDK, auth, timezone database, MCP SDK, or validation versions used by the historical capture. **[P/L/U]** [P-project] [P-upstream-lock] [P-sdist] [L-capture] [P-fastmcp] [P-server]

The examined Google SDK wheel bundles discovery revisions **Sheets `20260921`**, **Calendar `20260708`**, and **Drive `20260916`**. `build()` defaults to static discovery when no discovery-service URL is supplied. These pin client schemas; they do not pin hosted responses, policy, delivery, or normalization. **[P/U]** [P-sdk-wheel] [P-sdk-discovery] [P-sdk-sheets] [P-sdk-calendar] [P-sdk-drive]

### Capability boundary

| Business effect | Captured/native route | REST-only or not demonstrated on this route |
| --- | --- | --- |
| Read/update tracker cells | `list_spreadsheets`, `get_spreadsheet_info`, `read_sheet_values`, `modify_sheet_values`. | Arbitrary `spreadsheets.values.batchUpdate`, a generic Sheets batch/CAS transaction, unrestricted grid/error representations. Smart-chip branches are specialized, not arbitrary REST passthrough. |
| Create/check meeting | `manage_event(action="create")`; follow with `get_events`, optionally `list_calendars`. | Supplying an event's `id` on insert, arbitrary extended properties, raw `Event` responses, raw conference-request status inspection. |
| Share/check file | `search_drive_files`, `get_drive_file_permissions`, `get_drive_shareable_link`, `manage_drive_access(grant/grant_batch/update/revoke)`. | Arbitrary `fields`, domain-admin escalation, arbitrary permission REST queries or transactions. Internal REST reads do not imply the caller can send arbitrary REST requests. |

These distinctions follow the captured signatures and pinned forwarding code. Official REST alternatives can inform an improved contract, but are not presently callable simply by adding unadvertised arguments. The three tools' schemas set `additionalProperties: false`; the local Rust dispatcher only checks required-field presence and is more permissive. Do not credit a replica-only unknown-argument trick as a valid native plan. **[L/P/I]** [L-tools] [L-lib] [P-sheets] [P-calendar] [P-drive]

## 2. Shared actor, identity, result, and error contract

### Actors are not interchangeable email strings

The capture/default actor is **`ops@octo-corp.example`**. Lena, Priya, Marcus and Dana are request authors/attendees/grantees, not credentials to impersonate. The fixture's primary calendar belongs to Ops; its report/tracker owner permission is Ops, ID **`16118283490155935341`**. The shared Ops team calendar is a different resource, even though Ops can write it. **[L]** [L-capture] [L-calendars] [L-files] [L-mail]

Pinned native authentication selects/validates credentials; in managed/OAuth modes it derives or overrides the user from authenticated identity. OAuth scopes and resource capabilities constrain actual operations. A `user_google_email` argument does not grant arbitrary identity or ownership. By contrast, the local replica hard-codes `USER`, obtains its shared state without an OAuth check, and ignores caller email for these mutations. That is a **local actor/authentication limitation**, not evidence that Google permits cross-user writes. **[P/L/D/I]** [P-auth] [L-util] [L-lib] [D-calendar-create] [D-drive-sharing]

### IDs refer to different things

- **Sheets:** an existing spreadsheet/Drive file ID plus a tab title or numeric `sheetId` plus cell coordinates. Cells have no standalone resource ID. Updating C2:D2 creates neither a spreadsheet nor a business-vendor identity. **[D/L]** [D-sheets-concepts] [L-files]
- **Calendar:** creating the meeting creates an Event; Google supplies `id` when none is specified. `iCalUID`, event `id`, calendar ID, conference `requestId`, and Meet URL are not interchangeable identifiers. Native `manage_event` does **not** forward `event_id` into the create body. The raw REST API supports caller-chosen event IDs with documented format/uniqueness constraints; the native create tool does not expose that control. **[D/P]** [D-calendar-insert] [D-events] [P-calendar]
- **Drive:** share an existing `fileId`; a returned Permission `id` identifies the grantee and is opaque, not a new report ID or universally fresh per-call audit ID. Sharing does not create a replacement file, a per-recipient URL, or demonstrate creation of a Google user account. Update/revoke address `fileId + permissionId`. **[D/I]** [D-permissions] [D-drive-sharing]

The local meeting ID **`445b671f493e8a06958e4ff0dd`** is a fixture-seed-specific first generated event, not an input requirement or vendor ID guarantee. Rust `hex_id("event",26)` consumes its deterministic per-kind generator; a failed create after allocation can consume an ID without persisting the event. Local permission grants always append counter-generated IDs starting from the `permission` namespace's `7_330_000_000_000_000_000` base. Neither generator is a valid model of hosted principal identity or exactly-once creation. **[L/I]** [L-calendar-code] [L-util] [L-drive-code] [L-tests]

### Text-shaped results and capture traps

All three captured mutation tools advertise `outputSchema: {result: string}` with FastMCP wrapping. The Rust success envelope duplicates a human-readable string in `content[0].text` and `structuredContent.result`; it is **not** `{id, event, permission, values}`. Rust failures return `isError: true` and text prefixed `Error calling tool '<name>': ...`; HTTP-level acceptance alone is not tool success. **[L/P]** [L-tools] [L-lib] [P-sheets] [P-calendar] [P-drive]

| Result | Actual useful observation | Trap |
| --- | --- | --- |
| Sheets write | Confirmation naming spreadsheet/range and updated cells/rows/columns. Native checks returned rendered values for spreadsheet-error tokens and may append error details. | Counts or the word “Successfully” do not prove correct vendor/date, absence of formula errors, or preservation outside the selected range. No raw `updatedData` object is returned to the caller. |
| Calendar create | Confirmation with returned summary, `htmlLink`, saved-time text, and a meeting URI **only if returned**. The event ID is logged internally, not returned as a separate result field. | No `/structuredContent/id` or JSON Event inside `result`; absence of a Meet URI can mean asynchronous pending, not insert failure. Do not decode assumed `htmlLink` internals as a guaranteed event-ID protocol. |
| Drive grant | File name, formatted permission including grantee/role/opaque permission ID, view link. | Link existence does not establish an ACL change; notification success is not proved. A permission ID must not replace `file_id` in subsequent calls. |

The table is **[P/L/I]**. Native permission inspection also contains prescriptive “Anyone with the link” advice; in this task that generic prose is **not authorization** to make the report public. `get_drive_file_permissions` catches errors in its final metadata-read block and returns an `"Error getting file permissions: ..."` string, rather than always raising a tool error. Its initial shortcut resolution is outside that block. Thus a non-error envelope can still contain an unsuccessful inspection. **[P/L/I]** [P-drive] [L-drive-code]

The local oracle capture facility supports JSON pointers, optional JSON decoding of a string, and exact string-field selection from arrays; it rejects `isError: true` and requires exactly one selected row. It has no regex/prose/URL parser. None of these three scenarios currently declares captures. Capturing `structuredContent.result` captures prose; adding `json_pointer="/id"` cannot turn it into a resource object. Reading Sheets produces Python-style list prose, not a JSON array. Correct dynamic-ID evaluation therefore needs an explicitly supported observation/selection contract, not a fabricated JSON path or the deterministic first-event literal. **[L/I]** [L-bindings] [L-renew] [L-plan] [L-share] [P-sheets]

### HTTP and errors

The pinned discovery SDK constructs the API-specific HTTP requests below, deserializes successful response bodies, and raises `googleapiclient.errors.HttpError` for HTTP status `>=300`. Its `HttpRequest.execute()` defaults `num_retries=0`. The pinned Workspace `handle_http_errors` decorator defaults `is_read_only=False`; the relevant writes therefore do not take its read-only SSL-retry branch. HTTP failures are wrapped with tool/API context; input errors are raised separately. This does **not** establish one physical attempt end-to-end: the separately examined `google-auth-httplib2 0.3.1` transport can refresh credentials and resend a request. **[P]** [P-sdk-http] [P-errors] [P-auth-http]

Google documents authentication, authorization/policy, invalid-input, not-found, quota and server errors. Drive specifically documents **`400 invalidSharingRequest` where sharing succeeded but notification email could not be sent**, as well as an ACL-change-denied case with the same reason family. An error is not universally proof of rollback; inspect the returned details and reconcile state before retrying. Exact status/message precedence for mixed failures, email canonicalization, and hosted delivery timing are **[U]**, not suitable invented replica guarantees. **[D/I/U]** [D-drive-errors]

## 3. Northwind: cell update, not row replacement or append

### Resolve the business row from the unchanged data

Lena's email **`a9de6f0a996bc25f`** explicitly says **Northwind Traders**, new contract end `2027-11-30`, status `Renewed`, and **Northwood Logistics remains pending**. The policy document says to use the current-year tracker. Resolve:

```text
spreadsheet_id: 1p91PmGrfYlol_BkGN9Op8j9dWzibNwgg  ("Vendor renewals 2026")
tab: Renewals; sheetId: 0
header: Vendor | Owner | Contract end | Status | Notes
row 2 before: Northwind Traders | Lena Fischer | 2026-11-30 | Pending renewal | Quote received 10/14
target: Renewals!C2:D2 → [["2027-11-30", "Renewed"]]
```

The older workbook is **`1_x8cbfTBtuujAmxfx9ZG2B5yGE5HDyhh`**; the current workbook's **Archive** tab has sheet ID **`812400331`**. Northwood, Contoso, Fabrikam, the header and Archive are not update targets. These are **[L]** identifiers, not name-based uniqueness guarantees. **[L/I]** [L-mail] [L-files] [L-renew]

Row 2 is established by this read, not by a universal “second row” rule. Spreadsheet IDs are stable across renames; row coordinates are not vendor identities. Re-read the tab/header and use an exact business-row match before writing; do not fuzzy-match Northwood, choose a duplicate by first occurrence, treat a result-relative `Row 1` label as absolute spreadsheet row 1, or infer current tracker from modification order alone. For a future duplicate Northwind row, use owner/contract context or report ambiguity; no disambiguation rule for indistinguishable duplicates is established. Both native and local read prose number returned rows from 1, relative to the requested range. **[D/P/L/I/U]** [D-sheets-concepts] [P-sheets] [L-drive-code]

### Native arguments and forwarding

`modify_sheet_values` requires `spreadsheet_id`, `range_name`; `values` is optional in the schema only because clearing/chips are alternative modes. For a normal update, supply a nonempty two-dimensional value list or a JSON-encoded list of lists. Direct list typing is `List[List[str]]`; JSON-string parsing verifies the list/row structure. Defaults: `value_input_option="USER_ENTERED"`, `clear_values=false`. Invalid JSON/row shape raises input errors. Chips cannot be combined with values/clear; clear with values is not subject to that same chips-specific exclusivity check. **[P/L]** [L-tools] [P-sheets]

Ordinary update forwards:

```text
PUT https://sheets.googleapis.com/v4/spreadsheets/{spreadsheetId}/values/{range}
query: valueInputOption=<RAW|USER_ENTERED>
       includeValuesInResponse=true
       responseValueRenderOption=FORMATTED_VALUE
body: {"values": [[...], ...]}
```

No `majorDimension` is sent; Sheets defaults it to `ROWS`. REST returns `UpdateValuesResponse`, including range/counts and requested returned values; native reduces this to confirmation/error-detail prose. The clear branch instead uses `POST .../values/{range}:clear`. Neither is append or workbook replacement. **[P/D]** [P-sheets] [P-sdk-sheets] [D-sheets-update] [D-value-range]

### Value normalization and genuine alternatives

- **`RAW` is the least ambiguous literal plan:** C2 gets `"2027-11-30"`, D2 gets `"Renewed"` without UI parsing. With **`USER_ENTERED`**, Google can parse a string as a date/number/formula according to UI rules. A correct date can consequently be stored as a serial and rendered according to locale/format rather than as the original ISO text. This is a legitimate semantic alternative unless the task explicitly requires plain text, which it does not. Do not make the fixture's string representation a universal date contract. **[D/I]** [D-input-option] [D-sheets-update] [L-renew-i]
- A contiguous C2:D2 update or two precise single-cell updates can satisfy the business request. A fully preserved A2:E2 rewrite can be equivalent in this simple fixture, but is needlessly risky: reads are formatted by default and overwriting formulas, rich values or concurrent changes with displayed strings is not preservation. Never append a second “renewed Northwind” row or replace the workbook to satisfy selected pointers. **[P/D/L/I]** [P-sheets] [D-value-range] [L-files]
- REST `ValueRange` skips `null` cells; an empty string clears a cell. Do not conflate “omit/preserve” and “clear.” The captured direct-list schema does not promise general REST scalar/null support; a JSON-string route and direct-list route need not have identical validation. The local implementation instead converts null to empty text, stringifies other scalars, and treats non-RAW strings by stripping leading apostrophes; it does not implement Google's date/formula parsing. **[D/P/L]** [D-value-range] [L-tools] [P-sheets] [L-drive-code]
- The local writer takes the payload dimensions from the range's start, does not enforce its parsed end bounds, and does not validate `value_input_option` beyond testing for `"RAW"`. It can therefore accept malformed/oversized or invalid-option writes differently from Google. This is a compatibility risk, not a documented Google rule. Exact mixed range/payload error behavior remains **[U]** without a specific vendor contract. **[L/I/U]** [L-drive-code]

**Preservation/history [I]:** verify all existing non-target values/tabs/workbooks, file IDs, ownership and permissions; allow appropriate service-managed modification metadata. The local writer updates `modifiedTime`/`lastModifyingUser` through `touch()`. No cited contract promises a particular revision-history entry count, old-value rollback, cross-call transaction, or concurrency lock. A two-call C2/D2 plan can partially complete; a read-before-write is not a compare-and-swap. Reconcile the same business row and finish only the missing intended update. **[L/P/D/I/U]** [L-drive-code] [P-sheets] [D-sheets-update]

## 4. Calendar: event, invitations, civil time, and conference are distinct effects

### The exact meeting

Priya's email **`355ad002b14a79a3`** requests **Q1 planning sync**, Tuesday November 3 at **2:00 pm Eastern**, **45 minutes**, with herself and **`marcus.webb@octo-corp.example`**, and Google Meet. Her address is **`priya.natarajan@octo-corp.example`**. Fixture primary calendar **`ops@octo-corp.example`** uses **`America/New_York`**; the separate shared calendar `c_ops_team@group.calendar.google.com` is not “my primary.” **[L]** [L-mail] [L-calendars]

The target boundaries are **`2026-11-03T14:00:00-05:00`** and **`2026-11-03T14:45:00-05:00`**, equivalent to **19:00–19:45 UTC**. The existing August Q4 meeting **`ae2a3372cbeee330eecbe86872`** uses `-04:00`; copying its offset into November shifts the meeting by an hour. Preserve that old event rather than repurposing it. The authored request is not an invitation to find another free slot or rename the meeting to match a conventional fiscal quarter. Free/busy inspection does not reserve a slot or prove attendee availability/acceptance. **[L/P/I]** [L-plan] [L-events] [P-calendar-helpers]

### Native argument and time normalization contract

`manage_event` requires `action`; create additionally checks nonempty `summary`, `start_time`, `end_time`. `calendar_id` defaults to `"primary"`. Valid action strings are normalized by trimming/lowercasing; notification values are validated as exact `"all"`, `"externalOnly"`, or `"none"`. Native create defaults `send_updates` to **`"all"`**, distinct from relying on the raw REST default. **[P/L]** [P-calendar] [L-tools]

For each boundary, native `start_timezone`/`end_timezone` overrides event-wide `timezone`. `_build_time_boundary`:

1. Treats strings without `T` as all-day `date` input; that is not this timed meeting.
2. Parses a timestamp; when an IANA zone is supplied, validates it using `ZoneInfo`.
3. Converts an offset-bearing timestamp into that zone **without changing its instant**.
4. For offset-free timestamps, forwards local `dateTime` with the supplied `timeZone`, leaving resolution to Google; it does not invent UTC.
5. With no zone, forwards `dateTime` as supplied; the API requires an offset unless `timeZone` is explicitly supplied.

Thus local `14:00`/`14:45` plus `America/New_York`, explicit `-05:00` boundaries, or correctly converted UTC boundaries plus the intended zone are defensible alternatives. A wrong explicit `-04:00` plus `America/New_York` is converted to **13:00**, not “fixed” to the intended 14:00. The November slot itself is not a DST fold/gap; no universal ambiguous/nonexistent-time policy is established here. Rust has only a small hand-written timezone model and defaults unknown slash-containing zones to UTC via its fallback; it is not a full IANA implementation. **[P/D/L/I/U]** [P-calendar-helpers] [D-calendar-insert] [L-calendar-code] [L-tz]

Attendee email strings normalize to `{email: ...}`; supplied attendee objects with an `email` key pass through. Invalid element shapes raise input errors. Google controls attendee identity and response state; `needsAction` is recommended for new invitations. Creating the organizer's event does not authorize fabricating Priya's/Marcus's acceptance. Rust adds `responseStatus="needsAction"` when absent, but that is its local normalization, not evidence that everyone received or accepted an invite. **[P/D/L/I]** [P-calendar-helpers] [D-calendar-insert] [L-calendar-code]

### HTTP and notification semantics

With Meet requested, native issues:

```text
POST https://www.googleapis.com/calendar/v3/calendars/{calendarId}/events
query: conferenceDataVersion=1&sendUpdates=all
body:
  summary: "Q1 planning sync"
  start/end: resolved dateTime and optional timeZone
  attendees: [{email: Priya}, {email: Marcus}]
  conferenceData.createRequest:
    requestId: <fresh UUID>
    conferenceSolutionKey.type: "hangoutsMeet"
```

No event `id`, organizer impersonation, or separate Gmail message is sent by this branch. REST returns an Event; native returns the reduced prose described above. An `event_id` argument on create does not become an idempotency key. The conference UUID deduplicates a conference-creation request according to Calendar's conference contract; it does **not** deduplicate event insertion, and another native create generates a fresh UUID. **[P/D/I]** [P-calendar] [P-sdk-calendar] [D-calendar-insert] [D-events]

`sendUpdates="all"` requests Calendar guest notifications; `"externalOnly"` is about non-Google Calendar guests, not simply everyone with another email domain. `"none"` is not equivalent to asking Google to invite both guests normally, and Google warns about synchronization/data-loss consequences. Even notification switches do not prove mailbox delivery. The scenario's ban on `send_gmail_message` should not be interpreted as banning Calendar's own invitations. Reminders are separate from invitations. No cited source provides a distributed transaction across organizer creation, recipient calendar visibility, notification delivery, and Meet provisioning. **[D/P/L/I/U]** [D-calendar-insert] [D-calendar-create] [P-calendar] [L-plan]

### Meet request accepted is not Meet ready

Google documents asynchronous conference provisioning: `createRequest.status.statusCode` can be **`pending`**, **`success`**, or **`failure`**. Success has usable entry points; failure has none. Allowed conference types depend on calendar capabilities. The tool's raw `conference_data` or third-party provider helper is not a license to invent a Meet URL or substitute Zoom when Priya asked for Google Meet. Native input rejects combining Meet creation with supplied conference data/provider helpers. **[D/P/I]** [D-calendar-create] [D-events] [P-calendar-helpers] [P-calendar]

The replica immediately fabricates a Meet URI, `hangoutLink`, and `conferenceSolution.key.type="hangoutsMeet"`; there is no pending/failure job, notification ledger, or attendee-calendar copy in that creation path. **[L]** [L-calendar-code]

**Completion/recovery [I]:** distinguish (a) one correct organizer event saved, (b) correct attendee list and notification request, (c) a real usable Meet video entry point. If insertion succeeds but conferencing is pending, check the **same event** rather than creating another meeting. `get_events` exposes event IDs/times/meeting links in prose, but not a raw conference-request state object; exact pending-versus-failure may remain unobservable on this native surface. Do not claim complete video setup without a returned/re-read link. A create followed by a supported update to add Meet or attendees is a valid final-state plan, though it has partial-completion/notification risks. Changing no other existing event is stronger and more appropriate than preserving only Q4's start timestamp. **[P/L/D/I/U]** [P-calendar] [L-calendar-code] [D-calendar-create] [L-plan-i]

## 5. Drive: identify the existing report, then change only its intended access

### File and recipient disambiguation

Dana's email **`fd9cbd3c80017bc0`**, from **`dana.whitfield@contoso.example`**, requests the most recent SOC 2 **Type II report**, view-only, not public. **Dana Whitman**, `dana.whitman@octo-corp.example` in Finance, is a contact decoy, not the grantee. Do not substitute a first-name contact match or share to the whole `contoso.example` domain. **[L/I]** [L-mail] [L-contacts]

| File | Fixture identity and qualification |
| --- | --- |
| **Correct final report** | `150dluRbaiTmEP9Spu-wAk0p_lQupubYY`, `SOC 2 Type II Report 2026.pdf`, PDF in Compliance, September 18; description says final, audit period `2025-08-01`–`2026-07-31`. |
| Older report | `12klLxpfyaiox7A1n8sigN7b3qTt3DYXh`, 2025 report, explicitly superseded. |
| Newer modified draft | `1HqJ_ArU5VCMWHwGz4FSEQR-Ykydr82k5`, 2026 **DRAFT**, October 2, in Drafts; not for distribution. |
| Newer bridge letter | `1mLKl11xTzeA9uvedTdrOCPmtCHy8DAH-`, bridge letter, October 5; not the Type II report. |

These are **[L]** facts. `modifiedTime desc` alone selects the wrong kind of document. Search with Drive `q` syntax, retain IDs/type/parent/version context, and inspect qualifying candidates; names are not unique IDs. Native search excludes trash unless explicitly overridden, supports pagination and corpora, and can report incomplete searches; one page is not a completeness guarantee. The pinned grant/read helpers follow shortcuts to their target using metadata, which must not silently widen this authorization to a different report. The fixture contains an ordinary PDF, not a shortcut or shared-drive item. **[L/P/D/I]** [L-files] [P-drive] [P-drive-helpers] [D-drive-search]

### Native grant contract

For this fixture, the direct plan is:

```text
manage_drive_access(
  file_id="150dluRbaiTmEP9Spu-wAk0p_lQupubYY",
  action="grant",
  share_with="dana.whitfield@contoso.example",
  role="reader",
  share_type="user"
)
```

Defaults are `role="reader"` when omitted, `share_type="user"`, `send_notification=true`. Native action validation is case-sensitive, unlike Calendar's normalization. It validates share type/role, resolves the file/shortcut, and forwards:

```text
GET https://www.googleapis.com/drive/v3/files/{fileId}
    supportsAllDrives=true; selected metadata including name/webViewLink
POST https://www.googleapis.com/drive/v3/files/{resolvedFileId}/permissions
query: supportsAllDrives=true
       fields=id,type,role,emailAddress,domain,expirationTime
       sendNotificationEmail=true
body: {"type":"user","role":"reader",
       "emailAddress":"dana.whitfield@contoso.example"}
```

For user/group shares, `email_message` becomes `emailMessage` if present and `send_notification` becomes `sendNotificationEmail`. Domain/anyone requests omit those notification parameters; `allowFileDiscovery` is applicable only to domain/anyone shares and does **not** make an `anyone` grant private. Optional expiration is forwarded only after validation; imposing one is not requested here and could later defeat the intended access. REST returns a Permission, which native formats as prose. **[P/D/L/I]** [P-drive] [P-drive-helpers] [P-sdk-drive] [L-tools] [D-permission-create] [D-permissions]

Drive notifications are separate from Gmail sending and separate from ACL mutation. The instruction asks for sharing, not a custom message or proof of delivery; default notification is a reasonable native plan. Disabling it can still satisfy the stated access outcome because no email notification is explicitly requested; an evaluator should not invent that additional obligation. In either case do not assert delivery without evidence. The documented share-succeeded/email-failed error requires reconciliation, not a blind second grant. **[D/L/I]** [L-mail] [D-permission-create] [D-drive-errors]

### Existing access, inheritance, and least scope

The fixture starts with only Ops' owner permission on every listed file and the relevant folders. It shows no Dana permission, group membership, public permission, shared-drive membership, or alternate access witness. Consequently a direct Dana `user/reader` grant is the clear supported solution. The missing inherited/group data is not evidence that every real Google file behaves this simply. **[L/U/I]** [L-files]

Vendor semantics to preserve:

- A permission's `id` is an opaque **grantee** identifier. Inspect existing permissions before retry/update; do not assume every successful create returns a new unique ACL-event identity or that repeated grants append independent duplicate readers. The exact response to every duplicate create/alias/case combination is not established by the create reference. **[D/U/I]** [D-permissions] [D-permission-create]
- Effective access can arise from a direct entry, group/domain access, parent inheritance, or shared-drive membership. A direct reader entry does not cap an already broader inherited/group writer role. Do not claim “view-only” merely because one matching reader object exists. **[D/I]** [D-drive-sharing] [D-permissions]
- Current sharing documentation says inherited permissions cannot simply be reduced on a child; changes belong at their origin or require supported limited-access behavior. Inherited shared-drive updates/deletes have a documented failure. Do not change a parent/group/shared drive to force view-only when that would share or affect other resources. Exact account/policy/limited-access applicability remains a boundary, not a new authorization. **[D/I/U]** [D-drive-sharing] [D-drive-errors]
- Authority to share depends on actor capabilities and policy, not merely a local owner-shaped array entry. Workspace external-sharing restrictions, `drive.file` authorization, shared-drive restrictions, and `writersCanShare` can block operations. No fixture establishes the live policies, visitor/account eligibility or effective group memberships for `.example` identities. **[D/L/U]** [D-drive-sharing] [D-drive-errors] [P-auth] [L-files]
- Reader is view access, not a promise that copying/downloading/printing is disabled. Do not change those settings, transfer ownership, publish a link, or add domain/group access to satisfy a simple user-reader request. Sharing a parent would affect other files; making a copy would share a different report identity and not preserve the original's history. **[D/L/I]** [D-drive-sharing] [L-share-i] [L-tools]

Native `get_drive_file_permissions` requests permission details and, for shared-drive metadata, lists permissions with pagination internally. Its inherited-source formatting can be useful but is still a prose projection, not complete group-membership/effective-access proof. The local replica instead reads its inline array, appends every grant, ignores notification/expiration/discovery semantics in `manage_access`, lacks inherited-access/actor-policy evaluation, and updates file modification metadata. These are specific **[L/P]** differences, not hosted guarantees. **[P/L/U]** [P-drive] [P-drive-helpers] [L-drive-code]

### Valid alternatives and recovery

**[I]** Accept a single-recipient `grant_batch` with the same Dana/user/reader scope; it executes individual permission creates and reports per-recipient successes/failures, not an atomic batch. If an authorized rerun finds Dana already has the intended effective reader access, a no-op after verification may be correct. If a directly managed existing Dana permission needs a role correction, native `update` addresses that permission ID. Neither case justifies deleting the owner or changing anyone else's grant. **[P/D/I]** [P-drive] [D-drive-sharing]

For uncertain outcomes, re-inspect the resolved file and grantee before retrying. Native grant has no caller-supplied idempotency key. Google warns that concurrent permission modifications on the same item are unsupported and only the last update applies; serialize local recovery, but do not invent a global lock. For batch partial success, retry only unresolved recipients. If broader inherited access prevents view-only within the authorized scope, report that limitation rather than secretly rewriting the parent's ACL. Preserve the PDF/content, ownership, parentage, and existing unrelated permissions. No precise Drive audit/revision count or transaction across sharing and notification is documented here. **[P/D/I/U]** [P-drive] [D-permission-create] [D-drive-errors] [D-drive-sharing]

## 6. Current static evaluator: exact false-positive/false-negative objections

These are source-reading findings and proposed improvements, **not qualification results**. The generic grader uses object-subset/array-member matching for `contains`, substring matching for string `contains`, and exact JSON equality for `equals`. Required calls ordinarily need an accepted HTTP status and no `isError`; they do not infer semantic correctness from a complete business contract. **[L]** [L-grader]

### Northwind assertions

**What is checked [L]:** fixed current-workbook pointers for Northwind name/date/status; complete Northwood and Fabrikam rows; the old tracker's Northwind row; one successful `modify_sheet_values` call naming the current workbook. The forbidden call only matches `modify_sheet_values` on the old workbook. **[L-renew]**

- **False positives [I]:** changing Northwind owner/notes, headers, Contoso, Archive, other older sheets, file permissions or unrelated resources can escape those state assertions. A destructive temporary edit then restoration can also escape final-state checks; the limited forbidden matcher does not cover every tool that could change the old workbook.
- **False negatives [I]:** a semantically correct USER_ENTERED date/format or a future reordered/expanded fixture can fail exact string/index assertions. Two-cell or preserved full-row updates should not be rejected merely for differing from the sample; neither currently violates the required-call matcher.
- **Boundary decision [I/U]:** score the uniquely identified original business row's contract end/status and preserve the complete non-target region, with an explicit date-value equivalence policy. If fixtures intentionally require ISO plain text, declare that local convention rather than claiming USER_ENTERED always stores ISO strings. Preserve formulas/formatting only where present/required; do not invent revision-history requirements.

### Calendar assertions

**What is checked [L]:** fields on literal event ID `445b671f493e8a06958e4ff0dd`; summary **contains** the requested title; exact primary-calendar ID and timestamp strings; attendee entries contain each email; conference solution type equals `hangoutsMeet`; Q4's start is preserved. One successful `manage_event(action="create")` is required and `send_gmail_message` is forbidden. **[L-plan]**

- **False positives [I]:** extra/wrong-title suffixes, extra attendees, fabricated accepted/declined statuses, duplicates elsewhere, a cancelled event, or a conference-type marker without any usable URI can pass the selected checks. The assertions do not prove notification intent/delivery, native conference completion, or preservation of Q4's title/end/attendees and other events.
- **False negatives [I]:** the correct meeting at a legitimate newly generated ID after another or failed create; equivalent RFC3339 representations of the same instants; a valid asynchronous pending state treated as an immediate failure rather than incomplete work. Calling an eventual Meet failure “success” is not a valid alternative—report partial completion.
- **Boundary decision [I/U]:** identify a single qualifying newly created event through authoritative state diff and supported returned/read identity, not a first-generator literal; verify exact requested title, correct instants/local intent/duration, correct attendee set allowing service-added organizer metadata, live status, usable Meet URI and old-event preservation. Distinguish invite intent from impossible-to-prove delivery/acceptance. Do not require a nonexistent structured `/id` capture.

### Drive assertions

**What is checked [L]:** the correct file's inline permissions contain Dana/user/reader and have no index 2; each of the three named decoy reports has no permission index 1. No write call is required. `set_drive_file_permissions` is forbidden wholesale; `manage_drive_access` with top-level `share_type="anyone"` is forbidden. **[L-share]**

- **False positives [I]:** at-most-two array entries is not preservation of Ops' owner, absence of broad effective access, or proof of a real permission create. Deleting/replacing the owner and inserting matching Dana could satisfy the narrow state projection. Sharing Compliance/Drafts/Vendors, another unasserted file, a group/domain, or `grant_batch` with nested public recipients is not comprehensively constrained by the top-level-anyone matcher. A temporary broader grant later revoked can escape final arrays. Local unused `share_type` or unsupported behavior must not become a native bypass.
- **False negatives [I]:** preserving legitimate pre-existing collaborators would violate fixed array-length guards in a richer fixture; inherited/normalized native permission representations need not resemble this inline array order. A benign `set_drive_file_permissions(link_sharing="off")` is blanket-forbidden although it is not itself public sharing. It is unnecessary here, but its prohibition is a local tool-path policy, not a universal interpretation of “not public.”
- **Boundary decision [I/U]:** compare initial/final grants by resolved file/principal/type/role, preserve existing ownership/access, establish effective reader access without any new broader audience, and protect every other resource plus forbidden transient exposure where observability supports it. Do not require a new grant if verified access already satisfies the instruction; do not accept a redundant reader that masks effective writer/public access.

## 7. Exact limits and decisions for the parent

1. **Pin boundary:** package/source pin is established; capture-time SDK/auth/framework/timezone dependency closure is not. Upstream `uv.lock` is a reference, not a local capture lock. The package/server-version distinction is explained without asserting an attested environment. **[P/L/U]** [P-project] [P-upstream-lock] [L-tools]
2. **Date boundary:** preserve all authored dates; choose semantic date/instant comparisons versus intentional literal fixture representation. Do not let November 7 replace October 29/November 3 context. **[L/I]** [L-renew] [L-plan] [L-share]
3. **Identity/observability boundary:** fixed Sheets/Drive IDs identify existing fixture resources; the meeting ID is generated. Actual native results are prose, and native read tools do not expose arbitrary JSON or full conference status. Any richer capture mechanism must be explicit, not fabricated. **[P/L/I/U]** [P-calendar] [L-bindings] [L-tools]
4. **Access boundary:** the fixture is single-actor/owner-only, not proof of RBAC, group/parent inheritance, external eligibility, or shared-drive policy parity. Local ignored actor/notification fields and duplicated permission counters must not define Google semantics. **[L/D/I/U]** [L-util] [L-drive-code] [D-drive-sharing]
5. **Partial-effect boundary:** Sheets multi-call writes, Calendar invitations/conferencing, and Drive notifications are not a documented cross-effect transaction. Google explicitly documents a share-success/notification-failure case; conference provisioning is explicitly asynchronous. Retry by reconciliation, not by assuming every error rolled back. **[D/P/I]** [D-drive-errors] [D-calendar-create] [P-sheets]
6. **Preservation/history boundary:** preserve original resource identity/content, unrelated rows/events/grants and required historical objects. Do not invent fixed audit entries, exact timestamp/ETag changes, automatic recipient acceptance, exactly-once notification, or universal duplicate-create behavior. **[L/D/I/U]** [L-files] [L-events] [D-permissions] [D-events]
7. **Scope boundary:** REST facts explain semantics but do not create a reachable REST tool. This memo proposes no implementation or qualification and makes no claim about the frozen Datadog38 run or completion of All24/full19. **[L/I]** [L-catalog] [L-lib]

## Primary-source register

Remote material was inspected read-only without Google authentication. Release-artifact hashes and commit URLs pin examined code; live Google documentation remains mutable. Local paths refer to the checkout as inspected, not to vendor-hosted state. The five main fixture projections (`gmail_messages`, `drive_files`, `calendar_calendars`, `calendar_events`, `contacts`) are byte-identical across these three scenarios; one scenario's paths are used below for shared data.

### Local source and fixtures

[L-catalog]: ../../catalog/services.toml#L521
[L-capture]: ../../servers/capture/replicas.sh#L31
[L-tools]: ../../servers/snapshots/google-workspace.json
[L-npm]: ../../servers/npm/package.json
[L-lib]: ../../crates/services/google/src/lib.rs#L1
[L-util]: ../../crates/services/google/src/util.rs#L9
[L-drive-code]: ../../crates/services/google/src/drive.rs#L61
[L-calendar-code]: ../../crates/services/google/src/calendar.rs#L760
[L-tz]: ../../crates/services/google/src/tz.rs#L1
[L-tests]: ../../crates/services/google/tests/google.rs#L130
[L-bindings]: ../../crates/scenario/src/solution.rs#L50
[L-grader]: ../../crates/mock-core/src/grading.rs#L947
[L-renew]: ../../scenarios/collab/google/mark-northwind-renewed/scenario.toml
[L-renew-i]: ../../scenarios/collab/google/mark-northwind-renewed/instruction.md
[L-plan]: ../../scenarios/collab/google/schedule-q1-planning/scenario.toml
[L-plan-i]: ../../scenarios/collab/google/schedule-q1-planning/instruction.md
[L-share]: ../../scenarios/collab/google/share-soc2-with-contoso/scenario.toml
[L-share-i]: ../../scenarios/collab/google/share-soc2-with-contoso/instruction.md
[L-mail]: ../../scenarios/collab/google/mark-northwind-renewed/dataset/google/gmail_messages.json
[L-files]: ../../scenarios/collab/google/mark-northwind-renewed/dataset/google/drive_files.json
[L-calendars]: ../../scenarios/collab/google/schedule-q1-planning/dataset/google/calendar_calendars.json
[L-events]: ../../scenarios/collab/google/schedule-q1-planning/dataset/google/calendar_events.json
[L-contacts]: ../../scenarios/collab/google/share-soc2-with-contoso/dataset/google/contacts.json

Useful exact local code anchors: `drive.rs:828` permission inspection; `drive.rs:1146` permission generation; `drive.rs:1171` access mutation; `drive.rs:1353` link/settings mutation; `drive.rs:2210` onward A1 helpers; `drive.rs:2382` read-result rendering; `drive.rs:2441` value writes; `calendar.rs:538` onward time/attendee helpers; `calendar.rs:708` onward conference generation; `calendar.rs:760` event mutation; `util.rs:117` generated IDs; `lib.rs:246` onward success/failure wrappers and required-argument dispatch. These are navigation aids, not source-pinned published contracts.

### Published Workspace MCP source

[P-pypi]: https://pypi.org/pypi/workspace-mcp/2.2.1/json
[P-sdist]: https://files.pythonhosted.org/packages/19/27/5676ea8f37281c0581c3adf8fb640d2054fb0f613b7167b48fe37ebb7e5d/workspace_mcp-2.2.1.tar.gz
[P-tag]: https://api.github.com/repos/taylorwilsdon/google_workspace_mcp/git/tags/1e1d0d4830113a1d4f680476fd235cf68c0bb715
[P-project]: https://github.com/taylorwilsdon/google_workspace_mcp/blob/0801afb3e58f51754f5a40161c07675f50565448/pyproject.toml
[P-upstream-lock]: https://github.com/taylorwilsdon/google_workspace_mcp/blob/0801afb3e58f51754f5a40161c07675f50565448/uv.lock
[P-sheets]: https://github.com/taylorwilsdon/google_workspace_mcp/blob/0801afb3e58f51754f5a40161c07675f50565448/gsheets/sheets_tools.py#L374
[P-calendar]: https://github.com/taylorwilsdon/google_workspace_mcp/blob/0801afb3e58f51754f5a40161c07675f50565448/gcalendar/calendar_tools.py#L345
[P-calendar-helpers]: https://github.com/taylorwilsdon/google_workspace_mcp/blob/0801afb3e58f51754f5a40161c07675f50565448/gcalendar/calendar_helpers.py#L676
[P-drive]: https://github.com/taylorwilsdon/google_workspace_mcp/blob/0801afb3e58f51754f5a40161c07675f50565448/gdrive/drive_tools.py#L2344
[P-drive-helpers]: https://github.com/taylorwilsdon/google_workspace_mcp/blob/0801afb3e58f51754f5a40161c07675f50565448/gdrive/drive_helpers.py#L497
[P-auth]: https://github.com/taylorwilsdon/google_workspace_mcp/blob/0801afb3e58f51754f5a40161c07675f50565448/auth/service_decorator.py
[P-errors]: https://github.com/taylorwilsdon/google_workspace_mcp/blob/0801afb3e58f51754f5a40161c07675f50565448/core/utils.py#L1242
[P-server]: https://github.com/taylorwilsdon/google_workspace_mcp/blob/0801afb3e58f51754f5a40161c07675f50565448/core/server.py#L534
[P-fastmcp]: https://files.pythonhosted.org/packages/4a/d2/bbbf9cb1a92e7f98c34845c8f06f21516fd38efdcd579ecbc0131bb6d4d5/fastmcp_slim-4.1.4-py3-none-any.whl

### Exact Google SDK and transport source

[P-sdk-wheel]: https://files.pythonhosted.org/packages/8b/c0/221313acdee33b8e5d3fe8a06ba398fc18cfd14627816764bb00318a6065/google_api_python_client-2.201.0-py3-none-any.whl
[P-sdk-http]: https://github.com/googleapis/google-api-python-client/blob/3974f97bbdfe42c27f0f3f246be2184e41e64291/googleapiclient/http.py#L957
[P-sdk-discovery]: https://github.com/googleapis/google-api-python-client/blob/3974f97bbdfe42c27f0f3f246be2184e41e64291/googleapiclient/discovery.py#L288
[P-sdk-sheets]: https://github.com/googleapis/google-api-python-client/blob/3974f97bbdfe42c27f0f3f246be2184e41e64291/googleapiclient/discovery_cache/documents/sheets.v4.json
[P-sdk-calendar]: https://github.com/googleapis/google-api-python-client/blob/3974f97bbdfe42c27f0f3f246be2184e41e64291/googleapiclient/discovery_cache/documents/calendar.v3.json
[P-sdk-drive]: https://github.com/googleapis/google-api-python-client/blob/3974f97bbdfe42c27f0f3f246be2184e41e64291/googleapiclient/discovery_cache/documents/drive.v3.json
[P-auth-http]: https://files.pythonhosted.org/packages/97/e9/93afb14d23a949acaa3f4e7cc51a0024671174e116e35f42850764b99634/google_auth_httplib2-0.3.1-py3-none-any.whl

The transport wheel SHA-256 is `682356a90ef4ba3d06548c37e9112eea6fc00395a11b0303a644c1a86abc275c`; examined member `google_auth_httplib2.py:199` onward implements credential refresh/retry. It is an inspected dependency reference, not an asserted capture-time installation.

### Google vendor documentation

[D-sheets-concepts]: https://developers.google.com/sheets/api/guides/concepts
[D-sheets-update]: https://developers.google.com/sheets/api/reference/rest/v4/spreadsheets.values/update
[D-input-option]: https://developers.google.com/sheets/api/reference/rest/v4/ValueInputOption
[D-value-range]: https://developers.google.com/sheets/api/reference/rest/v4/spreadsheets.values#ValueRange
[D-calendar-insert]: https://developers.google.com/calendar/api/v3/reference/events/insert
[D-events]: https://developers.google.com/calendar/api/v3/reference/events
[D-calendar-create]: https://developers.google.com/calendar/api/guides/create-events
[D-drive-search]: https://developers.google.com/drive/api/guides/search-files
[D-drive-sharing]: https://developers.google.com/drive/api/guides/manage-sharing
[D-permission-create]: https://developers.google.com/drive/api/v3/reference/permissions/create
[D-permissions]: https://developers.google.com/drive/api/v3/reference/permissions
[D-drive-errors]: https://developers.google.com/drive/api/guides/handle-errors
