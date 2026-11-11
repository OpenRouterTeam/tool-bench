# Deterministic semantic fault schedules

This is a source implementation, not native-model, published-image or production
qualification. Existing scored scenarios do not acquire failures implicitly.

## Declaration

An optional top-level schedule in `scenario.toml` is loaded by `Dataset::load`.
The scenario source hash covers its exact bytes; the initial snapshot includes
the validated schedule, and sealed evidence includes causal fault selections.

```toml
[fault_schedule]
version = "fault-schedule-v1"

[[fault_schedule.rules]]
service = "stripe"
operation_id = "PostRefunds"
resource = "ch_example"

[fault_schedule.rules.fault]
type = "response_lost"
occurrence = 1
```

For a pre-execution transient failure, use
`{ type = "transient_error", status = 503, occurrences = [1] }` as the fault.
Only 429 and server-error status codes are accepted. Unknown schedule versions,
unknown fields, uninstrumented operations, zero or duplicate occurrences,
overlapping selectors and unsupported resource selectors fail loading.

## Selection and delivery

Occurrences count uncached, validated semantic dispatch attempts separately for each
`(service, operation_id, resource)`. Authentication, MCP account validation,
path argument validation and unknown-parameter rejection precede dispatch.
Other resources and other operations cannot consume a target's occurrence.
Idempotency cache hits and key-mismatch rejections do not execute the handler
or consume a fault occurrence.
A missing resource selector means the rule applies independently to each
resource, not to a single shared request counter.

The registry in `mock-core::faults::SUPPORTED_OPERATIONS` contains eight actual
Stripe operation IDs. REST and MCP use the same dispatch seam. Individual
charge, refund and customer reads use the path resource; refund creation uses
its charge, resolving a payment-intent alternative to that charge. Collection
reads and customer creation have no stable pre-execution resource selector and
reject a configured resource filter.

A transient error skips the handler and commits no business changes. REST
returns the declared status with a neutral synthetic error; MCP returns its
ordinary tool-error envelope. These messages are not claimed as exact hosted
Stripe error strings.

A selected response loss applies to a successful write. The handler commits
and the trusted recorder captures the complete response; a separate
`response_delivery_lost` event records that it was withheld from the client.
The network body fails instead of becoming a successful empty body.
Information prerequisites do not treat this withheld response as delivered
evidence. A failed write is not represented as a committed response loss.

World evidence schema 6 introduced causal selections and delivery-loss events.
The unpublished evaluator 45 emits schema 7, which also carries scripted-user
exchanges; older evidence must be assessed with its matching release. Fault
records must replay the source-bound schedule, match captured requests, precede
their effects and agree with delivery events. The shared evaluator release
number and generated tasks still require central integration; this source
slice does not redefine historical evaluator-44 evidence.

## Idempotency and reset

POST requests with `Idempotency-Key` cache successful results in the Stripe
session store. Fingerprints bind the operation, path resources and recursively
canonicalized parameters; scalar JSON numbers and booleans normalize to form
strings. Reordered fields do not change the fingerprint. Reusing a key for
different parameters or a different path resource is rejected. GET requests
are never cached.

This is bounded trial-local successful-write replay, not a claim to emulate
all hosted Stripe retention, concurrent-key or error-caching behavior. The MCP
tool schemas are unchanged and expose no new idempotency-key parameter. A
valid MCP recovery may read existing refunds and reconcile the committed
effect instead of blindly repeating the write.

Reset clears session caches and fault evidence and restores fresh counters.
Clones do not share counters. Runtime schedule replacement after trial
activity is rejected; actor-facing endpoints cannot install a schedule.

## Validation

Focused tests cover real REST and MCP dispatch, unrelated-resource isolation,
a real loopback TCP failure after commit, keyed retry without duplicate
refund/ledger/event effects, an unkeyed duplicate-refund control, parameter and
path mismatch, schedule loading/source mutation, reset and causal-evidence
substitution. They do not constitute a model or held-out experiment.
