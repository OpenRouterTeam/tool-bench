# Reference-program result contracts

This is **scripted reference execution**, not a rule that any recoverable tool
error invalidates a model trial. Task outcome, preservation and safety remain in
the declared evaluator contract.

## Two separate checks

Every generated `call` validates:

1. The real response bytes as JSON or a complete SSE stream: one matching RPC
   response, valid result containers, no RPC error and no MCP tool-error flag.
2. The decoded result against the declared, endpoint-qualified tool-result
   contract using the **actual request file**, including rendered bindings.

Both capturing and noncapturing programs use this same `call`. Rejected raw
responses remain in the diagnostic log, but cannot supply captures, authorize
later MCP calls or publish an answer. The host-observed nonzero program return
then prevents authoritative collection and scoring. Agent-written success text
does not substitute for that return.

The module's input is a validated `CallToolResult`, request, actual endpoint port
and generated registry; its output is the unchanged result or an error.
`response.jq` owns transport/RPC validation. `tool-result.jq` owns the reviewed
vendor contract. `result-contracts.json` resolves ports from the actual catalog
and carries exact declared error permissions. Missing or malformed context,
ambiguous catalog routing and unknown profile versions fail closed.

## Reviewed AWS subset

The endpoint/tool pair `aws-api.call_aws` uses `aws_api_v1`; other pairs currently
use `mcp_envelope_v1`. A coincidentally named field or tool on another endpoint
does not activate AWS-specific interpretation.

For explicit-region SDK-style command results, the AWS profile checks:

- One result per actual input command, in the pinned client's input order.
  Repeated commands remain repeated; an empty input batch may return an empty
  result. Empty activity is not evidence of a business operation.
- Exact returned command identity, not just an array position or a success word.
- Absence of wrapper, validation, missing-context and constraint failures.
- A structured response with an integer successful status and no service error,
  or an exact declared API-error exception.

The pinned client catches command failures and may return a successful MCP
envelope with `isError=false`. Service errors are nested; command-validation
exceptions can instead appear on the command item. Ordinary business JSON or
log text containing `error` is not interpreted as a tool failure.

The client runs batch members sequentially and continues after caught failures.
This validator runs after that MCP call returns: it cannot undo effects or stop
later commands **inside the same batch**. Authoritative task history and the
controller's admission checks remain necessary.

Wildcard-region fanout is explicitly unreviewed: the client discovers enabled
regions, can produce a different cardinality, and preserves discovery order
rather than a guaranteed alphabetical ordering. This profile rejects such
reference results instead of incorrectly zipping them with inputs. CLI
customization stdout/stderr results lack a reliable API-status/exit-code contract
and are also unreviewed. Help can produce the SDK-style shape with a synthetic
200, so accepted command results are **not proof of underlying HTTP execution**.
These are reference-admission limits, not statements that AWS lacks those
capabilities or that a native model using them failed its task.

The pinned implementation and primary-source qualifications are recorded in
[the result-contract research](history/research/2026-11-06-aws-cli-result-contract.md).
Other vendors' nested result conventions, full output-schema validation and
trusted MCP/inference correlation remain open.

## PagerDuty write results

`pagerduty_api_v1` is deliberately limited to `manage_incidents`,
`add_note_to_incident` and `create_schedule_override`. The pin is documented in
the [operation contract](history/research/2026-11-07-pagerduty-operation-contract.md).
The three names cover nine current reference calls; unrelated reads and writes
remain on their explicitly weaker generic envelope contract.

- Typed management responses must identify every requested incident and expose
  the supported final status/assignment effect. Empty/actionless responses,
  ignored `assignment` instead of actual `assignement`, missing assignees and
  irrelevant returned incidents are rejected. The native model drops urgency,
  policy and actor fields; response validation does not invent visibility.
- Note results require an allocated identity, actual user/time fields and the
  complete requested content. Incident parentage comes from the request and
  authoritative records, not a field absent from the normalized native note.
- Classic override results are per-item arrays, including when outer HTTP and MCP
  succeed. Every item needs a successful creation status, no errors, an actual
  distinct ID and the request-bound user/instants. Failed items cannot hide behind
  a successful first item. Returned order need not match input order; multiplicity
  and exact tuples must match. Text and structured representations must agree.
- Calendar comparison uses exact Gregorian arithmetic and padded fractional
  strings, not process timezone or floating-point nanosecond rounding. This result
  normalization does not enlarge the local schedule renderer's supported
  precision: unsupported fractional operations are authenticated environment
  faults, never guessed equivalent or ordinary model failures.

The validator runs after a native call. It cannot undo earlier committed members
of a partially failing batch or earlier writes inside a management helper.
Generated-helper fixtures separately verify that failure prevents subsequent
calls, captures, completion markers and answers. They are not native-runtime
qualification. A model may recover from a tool error and still satisfy its
business contract; these are reference-program admission rules, not a universal
failure policy for model trajectories.

## Exact expected negative evidence

A legitimate negative read must not be confused with a failed reference plan.
An author can declare a bounded exception:

```toml
[[solution.call]]
endpoint = "aws-api"
tool = "call_aws"
arguments = { cli_command = "aws s3api get-bucket-policy --bucket policy-audit-bucket" }
allow_api_errors = [{ command = "aws s3api get-bucket-policy --bucket policy-audit-bucket", status = 404, code = "NoSuchBucketPolicy" }]
```

Permissions require an exact static command present in that call, an HTTP
error status and a nonempty SDK code. Invalid/duplicate declarations and
permissions on an unreviewed endpoint/tool are rejected before generation.
At execution all three values must match the actual structured command result.
They do not excuse a wrapper exception, another resource, another code/status,
or another failed command in the batch. A permission allows that negative result;
it does not require its occurrence or replace the task's outcome/evidence checks.
Dynamic command permissions are not silently guessed from future captures.

`example/aws/missing-bucket-policy` exercises this path against a genuine absent
policy, separately from the scored lookup suite. Examples remain excluded by
default. Generate them deliberately for diagnostic execution:

```sh
CARGO_INCREMENTAL=0 cargo run --locked -p scenario --bin taskgen -- \
  --include-examples --out /tmp/tool-bench-diagnostic-tasks
```

Use the same flag for freshness checks on that diagnostic output. The source
inventory still counts **150 scored tasks**, with two unscored examples; including
fixtures in a local output directory does not enlarge the benchmark denominator.

## S3 lookup revision

The retained task ID `cloud/aws/find-unencrypted-bucket` now asks which bucket
defaults to SSE-S3 despite an explicit SSE-KMS policy. Its fixture has one genuine
AES256 outlier, KMS-configured distractors and the same similarly named bucket.
Every default-encryption read succeeds; no missing-default error is needed.
The question concerns **bucket defaults**, not encryption of preexisting objects.

This intentionally changes the user request, data and contract. It is not a
wording-only cleanup, a retroactive improvement to old scores or the same
router-comparison input. The previous missing-default premise was incompatible
with modern automatic SSE-S3 defaults. Historical sources/results remain in the
revision-26 archive; changed source, instruction and world fingerprints must
remain distinct.

The mock retains an explicit historical null-encryption fixture form with its
documented 400 error, not as the current scored task. The separate missing-policy
fixture uses the different 404 `NoSuchBucketPolicy` contract.

The S3 response builder also no longer emits evaluator-owned owner display names
or a labelled HostId. Canonical IDs remain coherent, and authored bucket names
are not sanitized. This removes specific owned cues only: it is not a complete
surface audit or evidence of model unawareness.
