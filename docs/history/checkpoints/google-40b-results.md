# Google 40B: selected-replica result and transport condition

Status: **admitted bounded scripted selected-replica MCP checkpoint**. Business evaluation
remains revision **40**, with 129 v2 contracts and 21 legacy tasks. This changes
the reference-program result adapter, its fixtures and packaging, not business
grading. It is a distinct condition from the
[original revision-40 source checkpoint](evaluator-40-google.md), whose retained
archive and 154 verified payloads remain unchanged. Neither condition completes
the full 19-item overhaul. This corrected 40B campaign succeeds revision 38 as
the latest bounded scripted-client checkpoint, without pooling its evidence.

## What the adapter actually observes

`google_workspace_replica_v1` is deliberately named for the **selected Rust MCP
replica**, not Google REST or the published Python Workspace server/SDK.
The replica returns one text content block and the identical string in
`structuredContent.result`. The adapter rejects conflicting copies, malformed
envelopes, reported errors and fabricated SDK-object representations. Ordinary
business text containing “Error” or negative public-sharing guidance is not
itself a tool failure.

Five explicitly reviewed tools use the profile. Four current reference calls
declare it: the three single-service writes and the existing Slack/Calendar
cross-service creation. Unreviewed Google tools keep the generic MCP envelope
contract; this is not qualification of all 119 advertised operations.

- Sheets confirmations bind the printed spreadsheet and range, and counts of
  non-null submitted cells, rows and columns. JSON-string values and null-as-skip
  are supported. Clearing has its own acknowledgment and does not invent update
  counts. Unreviewed chip representations are not certified.
- Calendar creation binds the provided title and resolved calendar encoded in
  the actual event link. Updates bind actual returned/requested identities
  without predicting a seeded ID. Deletion and RSVP validate their printed
  acknowledgments. Timing summaries must be structurally complete and internally
  consistent; conferencing suffixes cannot hide extra failure text.
- Drive grants bind printed principal/type/role and the requested file URL.
  Batches require every requested row and a complete zero-failure summary.
  Update/revoke bind the permission fields actually printed; they do **not**
  fabricate an absent file-ID field. Ownership-transfer completion is unreviewed.
- Permission/link reads bind the printed resource and view URL. Permission-list
  counts must cover every displayed row. The real local permission-read handler
  can return an error string inside a non-error MCP envelope; that is rejected
  before captures, subsequent calls or completion.

These are acknowledgment/representation checks, **not business-state proof**.
They do not prove exact persisted nanoseconds, provider provisioning, guest
delivery, permission effectiveness, hosted authorization or model receipt.
The independent business/history checker continues to decide actual outcomes.
Notification modes are not an invented delivery field or all-only completion
requirement.

The jq implementation is a separate module shipped with every generated
reference program. The generic dispatcher selects it explicitly; no fake
structured fields are added to tool responses. Valid payloads pass unchanged.

## Evidence and the schema correction

Eighteen result-adapter Rust test methods cover valid text, request binding,
malformed/conflicting envelopes, sparse/JSON-string cells, valid default
spellings, batch partial failures, opaque identities, calendar suffixes,
notification modes and permission-read error strings. Seven Python tests cover
replay context, duplicate identities/JSON members, altered adapter output and
vacuous evidence.

All **54 generated-program fixtures** pass: six operation forms, each with
seven invalid and two valid representations. Each invalid result stops before
the second MCP call, capture-dependent use, answer and completion markers. Valid
results forward the complete captured text unchanged. These mock-curl fixtures
test the generated reference helper, not a live MCP client or vendor backend.

The original direct Rust serial-date control supplied numeric list cells.
That was a backend-semantic test, not an advertised native argument shape:
the captured schema allows a JSON string or a list of string lists. In 40B the
same date-value alternative uses the **JSON-string route**, and every direct
Rust control call is checked against the captured schema before dispatch.
The historical 26-test revision-39 replay already used a JSON-string serial
route and is not reclassified or relabeled.

All **17 direct Rust business controls** still independently agree: eight
passes and nine deliberate failures. Permission readback leaves business state
unchanged. All **26 genuine direct HTTP results** replay through the new adapter
unchanged. Constructed JSON-RPC framing around those exports is **not** evidence
that an MCP client, SDK or model received them.

The current-condition source gate passes **1,960 Rust tests**, zero failed or
ignored, strict workspace/all-target clippy, formatting and canonical
150-task/inventory freshness. All **257 independent/consumer Python tests**,
**61 pinned-Harbor runner tests**, the 54 Google helper fixtures and existing
cross-profile/helper/runner/isolation regressions pass. These counts are not
borrowed from the original revision-40 source condition.

Source retention: `/tmp/tool-bench-google40b-source.G5dOTv94`. Candidate build
artifacts: `/tmp/tool-bench-google40b-build.83JKwVZC`. The source manifest records
the actual candidate image identities; runtime admission remains separate.

| Role | Declared condition |
| --- | --- |
| World | `tool-bench-world:google40b-83JKwVZC` |
| World engine identity | `sha256:f6cdbd0a6edb2a36fd0eeb0cc6ea6a1e92bdd39be7b3a4a7585e580c347ec30f` |
| Tools | Existing `tool-bench-tools:datadog-contracts-v38-candidate-b` |
| Tools engine identity | `sha256:872fdca0798b178eed1fa3a45f0d6eda6fd0456ef4d28c559472cdf59378e855` |

All **2,797 world-input files** and executable flags match across the build;
the input tag is `2edf2ef2c3f0cef5`. Both world executables are retained and
checksummed from an owned image-copy container, which was removed. The container
and engine image identities agree. Eighty-five selected tools-source files
(bridge/catalog/catalog data/server assets/image recipe) match the retained
revision-38 source. Reusing that exact tools image is explicit; it is **not**
a claim that it was rebuilt from the new full context, whose root lock has the
Google dependency-record difference. No package/version/source keys changed.

The source archive, local build records and observed binaries are not a hermetic
build or hosted-service attestation. Actual transport evidence and broader
runtime campaigning remain separate from this source/build retention.

## Actual MCP preflight and separate host-auditor correction

The selected image pair completed all three reference/no-op pairs over MCP:
three passes and three deliberate business failures. A separate Calendar trial
completed the reference, derived the actual created event ID from its returned
link, captured initialization and all **119 locked tool definitions**, and
exercised permission read, a genuine bare permission-error string inside
`isError:false`, an unknown-tool RPC error and an invalid event update requesting
Meet creation. The failed update left business state preserved. The result and
common response guards rejected their respective actual errors.

The original surface host driver exited **2**, after the actual trial completed
and was validly assessed. The independent host auditor incorrectly required
Google-only initial state. The real world prepares every registered service
before listeners, whereas the direct in-process source harness prepares only
Google. The original rejection is retained, not relabeled as an original green
driver run, and the populated surface driver was not rerun.

A **post-source-freeze host-checker correction** admits only either the authored
Google-only baseline or that baseline plus the complete, exact source-reviewed
unselected bootstrap. It does not ignore arbitrary service state. Partial or
unknown stores, changed default records, changed Google baselines and
unauthorized unselected mutations/restorations still reject or fail through
full-history replay. Seven new regressions bring the Google auditor to
**57 tests**; the then-current combined host Python suite passes **264 tests**.
This changes analysis tooling only, not the business evaluator, world, task,
reference program or image. All **2,797 world-input files and executable flags**
remain identical to the frozen build inputs.

The existing seven assessments independently agree under the corrected auditor:
**four passes / three failures**. All seven also reproduce exactly using the
packaged `bench-verify` in owned, network-none containers with read-only roots
and inputs, dropped capabilities and no-new-privileges. Assessment, reward and
verification JSON agree with the original byte/manifest/runner-bound inputs.
These trials have no final answer bytes; no answer or model-receipt claim is
inferred.

Separate preflight/correction retention:
`/tmp/tool-bench-google40b-preflight-freeze.ahofoudc`, with **7,277 independently
verified payload checksums**. It includes complete original attempts, original
rejection and re-audit logs, the exact correction/diff/source basis, isolated
reproductions and ownership results. The original source capsule's **129**
checksums still verify unchanged. The ownership scan finds zero survivors among
**28 owned container IDs / seven project namespaces**, without shared cleanup.
An initial retention-only probe rejected Docker's lowercase “no such object”
message; its incomplete capture and failure log remain separate. No runtime
admission is based on that failed retention attempt.

This is a bounded preflight, **not full Google qualification**, execution of the
published Python Workspace MCP/SDK, hosted Google conformance, a model run or
admission of all advertised operations. At the preflight stage, revision 38
remained the last admitted scripted native-client checkpoint.

## Expanded campaign and retained correction

The separate host-driver capsule
`/tmp/tool-bench-google40b-native-drivers.du9z2xhw` retains **42 checksummed
payloads**, including original/adapted scripts and actual host executables.
Adaptations only fix the repository root and select retained executables.
Forty-three schema-admitted business programs cover legitimate date/time/guest/
permission alternatives, wrong entities and exact boundaries, duplicates,
unrequested scope and restoration history. Runtime validation is now complete
under the corrected campaign below; generation alone was never admission.

The first batch stopped after **33 completed business controls**, when a planned
“repeated grant” positive hit the already documented duplicate-grant capability
boundary. Its first grant succeeded; the second returned a real MCP error and
host admission withheld authoritative evidence and reward. The original
expected-positive declaration, nonzero batch exit and failed attempt remain
unchanged. This was a control-plan mistake, not a reason to remove the capability
guard, weaken grading or classify an ordinary model failure.

The separately retained corrected driver at
`/tmp/tool-bench-google40b-native-drivers.5kd4hy2c` also has 42 payload checksums.
It replaces that case with disabled notification plus permission readback,
which is a reviewed access-completion alternative. Its ten-case continuation
ran only that replacement and the nine unexecuted controls; the 33 completed
controls were not rerun. The world, tools, evaluator, authored tasks and reference
programs stay unchanged. Every actual program is byte-identical to its corrected
full-generation counterpart before combined admission.
The original duplicate attempt stays an additional unscored diagnostic.

All **ten actual tool-error programs** complete their expected failure controls:
missing resources/permissions, malformed values, invalid intervals/timezones,
the real bare permission error and explicitly unsupported raw conferencing stop
before continuation, answer publication or authoritative assessment. Completed
source-binding, host-observed completion, shutdown/admission and isolation/
forgery controls retain invalid or unscored attempts separately; they are not
ordinary business negatives.

A separate post-hoc checker validates actual dispatched replica HTTP bytes,
rather than merely unused program JSON files, against the locked argument
schemas. Ten checker tests and four business-admission predicate tests bring
the current combined host suite to **278 passing tests**. This is separate
post-source analysis/CI work, not a replacement for the frozen 257-test source
gate. Its conservative template policy and other limits are documented in
[authoring admission](../authoring-admission.md#realized-google-control-arguments).
No universal runtime schema-enforcement claim is made.

## Qualified condition and complete retention

All **43 business controls** independently agree: **20 passes / 23 deliberate
failures**, zero audit rejections. Together with the preflight, **50 Google
assessments** receive independent source/business/history agreement. The separate
actual-byte checker validates **116 dispatched operations across 47 trials**;
the three no-op trials are explicitly outside that nonvacuous call interface.

All **55 primary assessments** reproduce exactly: **28 passes / 27 failures**.
Each runs the packaged verifier from the declared world image in an owned
network-none container. Engine inspection verifies read-only roots and input
mounts, dropped capabilities, no-new-privileges and no privileged execution.
Assessment, reward and verification JSON match; source, contract, context,
manifest, runner-policy/receipt and available answer/evidence bytes bind.
The retained reporting executable accepts each resolved self-comparison.

Four environment/source/admission assessments remain excluded, and their paired
reports refuse to publish scores. **Fifteen failed programs stay unscored**:
ten formal tool-error controls, two failed-completion controls, two shutdown
controls and the original duplicate-grant diagnostic. None has an authoritative
world evidence bundle or numeric assessment. These are not business negatives.

Qualification: `/tmp/tool-bench-google40b-qualification-freeze.hscvsqh4`.
Its **1,086 payload checksums** independently verify, including the retained
archive whose **4,598 payload files / 5,578 entries** also round-trip byte-for-byte
against their originals. Retention accounts for **74 distinct trials / 75
terminal result files**, including the explicitly tampered copy. Both driver
capsules, corrected full generation, every original attempt and failure,
independent decisions, isolated reproductions and post-source host tooling are
retained. All **2,797 world-input files and executable flags** still match the
frozen build inputs. The source and separate bootstrap-correction capsules
remain unchanged.

Scoped ownership finds **zero survivors** among **282 owned container IDs / 149
project namespaces**, including probes and verifier-copy/reproduction IDs.
Only owned reproduction containers were removed; no shared pruning occurred.

This admits the **bounded scripted selected Rust replica MCP condition**,
not execution of the published Python server/Google SDK, hosted semantics,
native-model completion/receipt, every advertised tool/parameter or the full
benchmark release. Original revision 40, original preflight rejection and the
wrongly authored duplicate control remain distinct historical conditions.

## Broader gates still open

The generic replica bridge does not yet establish universal runtime argument
schema enforcement or the upstream Python dependency closure. Checking these
control programs does not close those broader authoring/runtime gates.
Native-model completion and receipt provenance, hosted conformance, operational
human review, awareness/discovery experiments, remaining legacy contracts and
held-out/common-release acceptance all remain open.
