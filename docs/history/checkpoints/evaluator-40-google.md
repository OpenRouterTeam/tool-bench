# Revision 40: Google date, meeting and access source candidate

Status: **source candidate, not Google runtime admission**. Current source has
129 v2 contracts and 21 legacy writes across 150 scored tasks plus two examples.
Revision 38 remains the last admitted bounded scripted native-client checkpoint.
The full 19-item overhaul stays active; contract migration is not completion.

This record describes the original retained source condition. The subsequent
[40B result/transport extension](google-40b-results.md) is a distinct program
condition and must not be silently pooled with this checkpoint.

## Business outcomes and valid alternatives

The renewal contract selects Northwind Traders and Lena Fischer by row identity,
not row two. The required end date is **November 30, 2027**, status `Renewed`.
Typed exact Sheets serial **46721**, ISO text and reviewed equivalent dates are
accepted. The bounded local parser evaluates reviewed constant `DATE` and
`DATEVALUE` forms, preserves null-as-skip behavior and declines unknown expressions
before mutation. RAW formulas remain text. Other vendors, identity/owner/notes,
Archive, old trackers and unrelated resources remain protected through history.
An identical-value old-tracker write still changes metadata and is not a no-op.

Planning requires one actual newly created confirmed, non-recurring event on the
primary calendar, exact title **Q1 planning sync**, Priya and Marcus, and
**November 3, 2026, 19:00–19:45 UTC**. Exact supported fractional instants are not
rounded into the requested window. A verified modeled Meet resource is required;
a type label or invented URI is not enough. A known usable Meet URI in location
and a staged create/update plan are legitimate alternatives. Existing events and
other resources stay protected.

The Drive contract accepts ASCII case-equivalent principal identities without
Unicode homoglyph folding, multiple equivalent preexisting direct reader rows,
and correcting only the intended principal's direct role. Complete history still
rejects transient escalation even when another reader row remains correct.
Preexisting owner/collaborator identities and all unrelated file data are preserved.

## Rejected notification overconstraint

The earlier candidate incorrectly required `send_updates="all"` notification
rows for both guests and coupled them to the final ready Meet URI. The
[primary-source research](../research/2026-11-07-google-invitation-notification-contract.md)
shows that attendee association and notification selection are distinct.
The instruction does not require guest email delivery or a notification mode.

That stricter candidate is rejected, not relabeled as correct. Permanent controls
accept explicit `none`, `externalOnly` and create-then-silent-Meet-update plans.
The request ledger is now named `calendar_notification_requests`: each row
records the mode and the organizer-side attendee snapshot, without guessing
Calendar enrollment from email domains. `delivery="unobserved"` is a limitation,
not proof of a successful send. Guest-side synchronization, visibility, receipt
and acceptance remain unqualified, including Google's documented `none` caveats.

## Precision, mutation and contract safety

IANA zones replace handwritten limited-zone offsets. Native explicit-offset
input with a submitted timezone reproduces the pinned Python helper's
microsecond truncation; observed timestamps and an existing display zone do not
invent that conversion. An update-precision regression first reproduced a false
pass, then passed after separating submitted from stored zone context.
Civil-time ambiguity, unsupported precision and unknown expressions fail before
business mutation as capability limitations.

Raw `conference_data.createRequest` provisioning was another silently accepted,
unqualified path. Its request identity, asynchronous progress and retries are
not modeled. It now declines as an authenticated capability limitation before
any event or conference mutation, rather than becoming an ordinary model failure.
The reviewed `add_google_meet` model and known existing resources remain
separate. Conflicting conferencing options keep their ordinary validation.

An invalid time update previously provisioned an orphan conference before
rejecting the event. Interval validation now precedes provisioning. Its permanent
control checks that a rejected update leaves the complete business state unchanged.

Calendar goal schemas reject selectors hiding other created events through
`latest_by`, alternate identity fields, matching or references. Reviewed field
projections, URL-host predicates and raw owned-field checks cannot shadow the
semantic goal. Date and meeting goals require audited version-2 contracts.

## Validation and evidence

All **70 Google workflow tests** pass: 32 Drive and 38 Sheets/Calendar.
The 18 focused core tests and eleven Google unit/six existing implementation
tests pass. The final source gate passes **1,942 Rust tests**, zero failed or
ignored, strict workspace/all-target clippy, formatting, whitespace and canonical
150-task/inventory freshness. The 250 independent/consumer Python tests and
61 pinned-Harbor runner tests pass. No earlier workspace denominator is substituted.
These are test-method counts, not unique trial or model-run denominators; some
methods exercise several variants. The seventeen retained source controls below
are counted by their distinct exported assessments.

A separate Python business/history checker is now implemented. Its **50 unit
tests** are included in the **250-test independent/consumer suite**, not added
again to that denominator. It resolves reviewed source email/instructions,
selects business identities independently, replays every committed record,
correlates raw HTTP receipts and rejects forged or incomplete evidence. It
shares only generic JSON, exact-value, timestamp and HTTP-trace utilities with
the Datadog checker; it never reads Rust expectation predicates to decide.

All **17 direct in-process Rust controls** independently agree: eight passes
and nine deliberate failures, zero audit rejections. They include three
reference/no-op pairs, serial dates, case variants, all notification modes,
a silent Meet update, wrong fractional instants, duplicate meetings, writer/
wrong-principal grants and restored neighbor edits. The reusable source-fixture
driver also checks each declared positive/negative outcome and refuses a
populated output directory. CI runs this gate separately from auditor unit tests.
These are real Rust HTTP/grade fixtures, **not native-MCP-client execution,
runner admission, a frozen runtime campaign or a model result**.

The identical initial 26 Sheets/Calendar controls were replayed against an
isolated revision-39 source root with an executable revision-identity assertion:
six passed and twenty exposed mismatches, comprising thirteen incorrect business
passes and seven valid-plan failures. Later controls are not part of that
historical denominator. The invalid supposed old-tracker “no-op” was corrected
as a test expectation, not accommodated by widening preservation.

The rejected notification candidate is retained at
`/tmp/tool-bench-e40-rejected-notification.7waFpPOY`; red diagnostic logs are
`/tmp/tool-bench-e40-notification-core-red.log` and
`/tmp/tool-bench-e40-notification-workflow-red.log`. The latter separately
reproduces three valid-plan failures and the orphan-conference mutation.
Precision red/green logs and the isolated historical run remain separate.

Final source evidence is retained at
`/tmp/tool-bench-e40-source-evidence.aLReaton`. It contains the checksummed
revision-38 source base, complete current overlay, revision-39 historical overlay,
executable historical identity guard, distinct historical/final test inputs,
all seventeen direct source-control exports, rejected notification candidate and
raw validation/diagnostic logs. The partial broad run and initial test-only
clippy failure remain diagnostics; the final complete workspace rerun includes
the test cleanup. Every retained payload checksum is independently re-read.
This is private source retention, not a hermetic build or runtime attestation.

Google's catalog endpoint is the **Rust MCP replica**, not execution of the
published Python Workspace server or Google SDK. The direct pinned chrono
dependency adds no package/version/source keys to the retained lockfile.
Capture-time upstream dependency closure is not established. There are no
revision-40 Google runtime images, admitted campaigns or Google-specific result
profile. The independent checker still needs a declared frozen runtime campaign;
source-fixture agreement does not supply that admission.
