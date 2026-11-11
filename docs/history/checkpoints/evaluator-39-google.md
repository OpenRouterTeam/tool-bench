# Revision 39: Google source candidate

Status: **initial Drive source migration, not runtime admission**. The source
has 127 v2 contracts and 23 legacy writes. Google's Sheets renewal and Calendar
planning workflows remain legacy. Revision 38 is still the last admitted
bounded scripted native-client checkpoint. The full 19-item goal stays active.

The [Google operation research](../research/2026-11-07-google-operation-contract.md)
distinguishes published upstream code, vendor guarantees, replica conventions
and unresolved dependencies. All 48 Google instruction/data files match the
revision-38 source capsule; authored business dates remain unchanged.

## Drive access is an outcome, not an array position

The report remains the existing final 2026 SOC 2 Type II PDF, and the grantee is
Dana Whitfield at Contoso, not the similarly named internal contact. The contract
requires a direct user-reader permission for that principal and preserves the
existing owner, other collaborators, report content/metadata, folders and every
other resource.

Only a matching new reader permission may be appended. A role correction on
Dana's existing direct permission may change that role without replacing its
identity. Existing safe access may be verified without another grant. A
single-recipient batch, default or disabled notification, and confirming that
link sharing is already off are legitimate alternatives. There is no fixed
array-length requirement and no predicted permission ID.

Complete committed history rejects transient public batch sharing, owner
revocation/replacement, collateral folder sharing, temporary writer escalation
and restored unauthorized report edits. Only the target's sharing flag and
bounded local modification/actor metadata are allowed derived fields.

The local allocator now skips occupied permission IDs. This repairs a fixture
collision, not Google's principal-identity semantics: a vendor permission ID is
opaque and need not represent a new independent object on each grant.

## Unsupported behavior is not silent success

The existing replica ignored expiration and custom notification content, and
appended a new permission for every repeated grant. Those behaviors cannot
establish upstream lifecycle, delivery or duplicate-create guarantees.

Expiration, requested custom notification delivery and repeated-principal
grants now receive authenticated capability failures before any business
mutation. Batch preflight includes all recipients and refuses duplicate
principals within the batch. A disabled notification does not demand delivery
of its unused message. These are explicit local connection limits, not invented
Google validation errors, hosted transaction guarantees or ordinary model
failures.

The supported condition is the authored direct-access fixture. Inherited/group/
domain effective-access resolution, hosted account policy, aliases, ownership
transfer, expiration lifecycle, notification delivery and duplicate-grant
conformance remain unqualified. The contract must not be advertised as a general
Google ACL evaluator or authorization implementation.

Before runtime admission, independently challenge principal canonicalization
(including case variants), effective-access representations and valid retry
alternatives. The current canonical-address fixture controls do not certify
those cases. Unqualified native semantics must not become ordinary model
failures merely because a local string or array representation differs.

## Validation and admission boundaries

The permanent workflow suite exercises valid alternatives, prior collaborators,
already-satisfied access, generated-ID collision, direct-role correction,
wrong/broader grants, collateral resources and complete history. Source checks
do not substitute for a frozen runtime condition or independent business audit.

A first historical-source replay reused workspace build artifacts despite
different retained source bytes. Its output even identified evaluator 39 against
revision-38 source; it is a **mixed-artifact diagnostic**, not a valid baseline
or admission result. The replacement replay uses a separate target directory
and an executable revision-38 identity assertion. Both attempts are retained.
The qualified revision-38 capsule itself is unchanged.

The final isolated historical-source run executes the same 28 controls:
11 pass and 17 expose pre-repair mismatches. These comprise seven incorrect
business passes, two valid-plan grading failures, one local permission-ID
collision and seven silently accepted unqualified operations. Its log is
`/tmp/tool-bench-e39-drive-frozen-isolated-baseline-final.log`; the revision
identity assertion passes separately. The candidate's 28 targeted controls
all pass at `/tmp/tool-bench-e39-drive-targeted-final.log`.

The complete source gate passes **1,883 Rust tests, zero failed or ignored**,
strict workspace/all-target clippy, formatting and canonical task/inventory
freshness. All six existing Google implementation tests also pass; those six
are included in the workspace count, not an additional denominator.

Source evidence is retained at
`/tmp/tool-bench-e39-source-evidence.3iCEkW`: the revision-38 source archive plus
a checksummed complete source overlay, identical historical/candidate control
inputs, executable historical revision assertion and raw gate/diagnostic logs.
This is source retention with no candidate runtime images, not a Google
qualification capsule or hermetic build attestation.

Intermediate candidates and the earlier 24-control historical run remain
diagnostics. In particular, the initial v2 contract rejected a valid direct-role
correction, and the new duplicate-grant capability check exposed a test that
attempted an unqualified duplicate instead of inspecting existing conflicting
grants. Those cases were repaired explicitly, not counted as qualified runtime
attempts or folded into the historical mismatch denominator.

No Google runtime campaign, independent business auditor, result-specific
profile, native-model trial or hosted operation is admitted here. The catalog's
Google endpoint is a Rust replica of the captured MCP surface; it does not
execute the published Python Workspace MCP or Google SDK. The source-package pin
must not be relabeled as an attested capture-time dependency closure.
