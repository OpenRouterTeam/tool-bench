# Google Calendar invitations versus notifications

Research date: **November 7, 2026**. Bounded review of `collab/google/schedule-q1-planning`, official Google documentation, and supplied `workspace-mcp==2.2.1` source pin **`0801afb3e58f51754f5a40161c07675f50565448`**. Research only: no implementation/instruction/capsule changes, tests/builds, live accounts, infrastructure actions, benchmark/model inference, or delegation. This note neither narrows nor completes the full **19-item goal**; existing uncommitted work and frozen evidence remain untouched.

Evidence labels: **[D]** documented Google contract; **[P]** inspected immutable MCP source; **[L]** authored/local convention or supplied candidate premise; **[I]** evaluator inference/decision; **[U]** unresolved by inspected evidence. Google documentation is mutable; the MCP commit is pinned, not Google's hosted backend.

## Bottom line

**Reject a universal `send_updates="all"` requirement.** Adding attendees and selecting guest email notifications are separate documented controls. `externalOnly` can support an invitation without emailing Google Calendar guests; `none` can preserve the requested organizer event and attendee association, but Google explicitly warns that using it on insertion can impair synchronization or lose events for some users. Therefore neither “`all` is always required to invite” nor “`none` always merely suppresses email” is defensible. **[D/I]** [D-create] [D-insert] [D-invite]

**Reject requiring a notification request to carry the final Meet URI.** Conference creation is asynchronous, and the pinned native tool sends a creation request rather than a previously known URI. Adding Meet later with `send_updates="none"` is supported by the API/tool surface; exact silent propagation to every recipient is not established. **[D/P/I/U]** [D-create] [D-patch] [P-create] [P-update]

## 1. Authored requirement, not current implementation

The instruction requests the user's primary calendar, exactly matching Priya's title/day/time/length/attendees/video link. Email `355ad002b14a79a3` asks for **Q1 planning sync**, **Tuesday, November 3, 2026, 2:00–2:45 pm Eastern**, inviting **`priya.natarajan@octo-corp.example`** and **`marcus.webb@octo-corp.example`**, with Google Meet. The primary fixture calendar is **`ops@octo-corp.example`**. Neither text requires email delivery, a notification mode, or a particular operation sequence. The business date remains November 3 despite the November 7 research date. **[L]** [L-instruction] [L-mail] [L-calendars]

The current scenario permits invitation-ledger additions only for the two recipients with `mode="all"` and `delivery="unobserved"`. The supplied candidate premise additionally couples an invitation request to the final meeting URI. These are **local evaluator choices**, not vendor facts or independent evidence of user intent. Mutable implementation is not used to resolve Google's semantics. **[L]** [L-scenario]

## 2. Documented facts and their limits

| Topic | Google documents | Evaluator consequence |
| --- | --- | --- |
| Attendee association | The create-events guide distinguishes including attendees from emailing them through `sendUpdates`. Invitation concepts describes inviting through the `attendees` property. | An absent guest email is not, by itself, an absent attendee/invitation. **[D/I]** [D-create] [D-invite] |
| `all` | Requests notifications to all guests. | Suitable default, not proof of receipt, acceptance, or attendance. **[D/I]** [D-insert] |
| `externalOnly` | Requests notifications only to **non-Google Calendar** guests. | Not “outside the organizer's email domain.” Suppressing Google Calendar guest email is an explicit mode, not invalid attendance data. **[D/I]** [D-insert] |
| `none` on insertion | No notifications are requested, with a warning about external-calendar synchronization and events being lost for some users. The parameter description also cautions that some emails might still be sent. | Valid input, but not a universal pure-email switch or an absolute zero-email guarantee. **[D/I]** [D-insert] |
| Recipient visibility | Guest invitation settings can require an email RSVP before an event appears; being in the same organization helps only under the known-sender setting. Prepopulating organizer-side RSVP does not force guest visibility. | Do not assume these `.example` addresses establish actual Calendar enrollment, settings, delivery, or acceptance. **[D/L/I/U]** [D-invite] [L-mail] |
| Updates | Invitation concepts describes organizer changes propagating to attendee copies. PATCH separately accepts `all`, `externalOnly`, and `none`; omitted fields remain unchanged and supplied arrays replace previous arrays. | Notification selection and event modification are distinct; this does not prove every silent conference update reaches every client. **[D/I/U]** [D-invite] [D-patch] |

**Reconciliation:** Google's broad propagation explanation must be read alongside the insertion warning and guest settings, not instead of them. It supports rejecting an `all`-only invariant; it does not establish notification-independent, universal recipient synchronization. Even `all` cannot establish actual human attendance. **[D/I/U]** [D-insert] [D-invite]

## 3. Pinned native tool, independently inspected

At the supplied commit:

- `manage_event` validates all three notification values; omitted input resolves to **`"all"`** on create and update. Explicit `"none"` and `"externalOnly"` survive forwarding. This is a wrapper default, not an instruction requirement. **[P/I]** [P-dispatch]
- Create sends normalized attendees independently of `sendUpdates`. `add_google_meet=true` supplies `conferenceData.createRequest` with a fresh UUID and `hangoutsMeet`, then calls `events.insert` with `conferenceDataVersion=1` and the selected notification value. The outbound body need not contain a final URI. **[P]** [P-create]
- Update can add Meet using the same creation-request shape. It reads the existing event, preserves omitted mapped fields including attendees, and calls **`events.patch`**, not full-resource PUT, with `conferenceDataVersion=1` and the selected `sendUpdates`. **[P]** [P-update]
- Confirmation text includes a meeting link only if the returned event already exposes one. The helper extracts a video entry-point URI, falling back to `hangoutLink`; it performs no delivery verification. **[P]** [P-create] [P-update] [P-helpers]

This pins forwarding behavior, not recipient-side delivery or the historical capture's complete dependency environment. The prior memo's package/source association is background; this review independently inspected the specified commit, not a mutable branch. **[P/U]** [L-prior]

## 4. Adding conferencing later without guest email

**Known [D/P]:** Google demonstrates adding conferencing to an existing event with PATCH and `conferenceDataVersion=1`. Its example uses `sendUpdates="all"`, but PATCH's parameter contract also permits `"none"` and does not make conference creation conditional on `"all"`. The initial response can be pending; populated entry points arrive after success. The pinned tool exposes the silent-update combination directly. **[D/P]** [D-create] [D-patch] [P-update]

**Not established [U]:** these sources do not specify the exact propagation timing of a silently added Meet link to every attendee/client, whether every already-sent invitation message will reflect it, or an absolute no-email guarantee across all backend behavior. An organizer's final URI alone is not proof of guest receipt. Conversely, missing notification evidence is not proof that no attendee can access the final event. **[U/I]** [D-invite] [D-insert] [D-patch]

**Concrete alternative [I]:** create the correct event with both attendees and `send_updates="all"`; add Meet to that same event with `send_updates="none"`; observe the eventual usable Meet URI. This satisfies the requested organizer-side outcome and records an initial guest notification request without requesting another guest email for conferencing. Requiring a later `all` request containing that URI rejects this supported sequence without an authored requirement. Guest-side propagation remains the uncertainty above, not a fabricated guarantee. **[I/U]** [L-instruction] [D-patch] [P-update]

## 5. Evaluator objections and decisions

1. **Drop mode equality as the attendance test.** Require the correct active primary-calendar event, exact meeting details, both attendees, usable Google Meet link, and preservation. Accept `externalOnly` or `none` when evaluating that organizer-side contract; report notification/synchronization limitations separately. This is a proposed scoring interpretation, not proof of universal guest visibility. **[I]** [L-instruction] [D-create] [D-insert]
2. **If email intent is a separate requirement, say so.** Only an explicit all-guests-notification policy justifies requiring `all`; actual receipt needs different evidence. Do not silently translate “invite” into “email every guest with the final URI.” **[I]** [D-invite] [D-insert]
3. **Decouple conference readiness from notification history.** Bind final conference evidence to the correct event. Do not require the URI inside a creation request: native `createRequest` does not know it yet. Pending is incomplete, not a reason to create a duplicate or fabricate a URL. **[I]** [D-create] [P-create]
4. **Classify false failures explicitly.** An `all`-only rule rejects valid Google Calendar invitations using `externalOnly`; a final-URI notification rule rejects asynchronous creation and the create-then-silent-Meet sequence. A blanket rejection of `none` also rejects otherwise correct organizer-side outcomes, although guest delivery cannot be assumed. These are genuine false-failure risks under the authored outcome, not permission to ignore Google's sync warning. **[I/U]** [D-insert] [D-patch] [L-instruction]
5. **Keep local evidence honest.** An invitation ledger can attest an attempted notification mode, not mailbox delivery. A conferencing ledger can attest recorded provisioning, not that a guest read the link. Do not reject allowed side effects solely because their mode differs from a preferred sample. **[I]** [L-scenario]

The prior memo's warning that `none` is not equivalent to normal guest notification remains useful. It does **not** establish `all` as the only valid invitation mechanism, nor justify coupling notifications to a ready URI. **[I]** [L-prior] [D-insert]

## Sources

Official Google sources inspected read-only; legacy `/calendar/` URLs now also resolve under `/workspace/calendar/`.

[D-insert]: https://developers.google.com/workspace/calendar/api/v3/reference/events/insert
[D-patch]: https://developers.google.com/workspace/calendar/api/v3/reference/events/patch
[D-create]: https://developers.google.com/calendar/api/guides/create-events
[D-invite]: https://developers.google.com/workspace/calendar/api/concepts/inviting-attendees-to-events

Pinned source anchors:

[P-create]: https://github.com/taylorwilsdon/google_workspace_mcp/blob/0801afb3e58f51754f5a40161c07675f50565448/gcalendar/calendar_tools.py#L345-L512
[P-update]: https://github.com/taylorwilsdon/google_workspace_mcp/blob/0801afb3e58f51754f5a40161c07675f50565448/gcalendar/calendar_tools.py#L516-L782
[P-dispatch]: https://github.com/taylorwilsdon/google_workspace_mcp/blob/0801afb3e58f51754f5a40161c07675f50565448/gcalendar/calendar_tools.py#L1018-L1106
[P-helpers]: https://github.com/taylorwilsdon/google_workspace_mcp/blob/0801afb3e58f51754f5a40161c07675f50565448/gcalendar/calendar_helpers.py#L198-L210

Local sources; scenario configuration is mutable and non-authoritative about Google:

[L-instruction]: ../../scenarios/collab/google/schedule-q1-planning/instruction.md
[L-mail]: ../../scenarios/collab/google/schedule-q1-planning/dataset/google/gmail_messages.json
[L-calendars]: ../../scenarios/collab/google/schedule-q1-planning/dataset/google/calendar_calendars.json
[L-scenario]: ../../scenarios/collab/google/schedule-q1-planning/scenario.toml
[L-prior]: 2026-11-07-google-operation-contract.md
