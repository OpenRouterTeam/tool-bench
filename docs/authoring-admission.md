# Authoring admission and source coverage

Task generation validates every scenario, including unscored examples, before
replacing generated tasks. The gate checks task layout, datasets, contracts,
bindings, endpoint identities, tool identities and statically checkable arguments.
It is an authoring gate, not proof that a task's evaluator or world is complete.

## Locked reference tools

Each catalog endpoint uses its declared snapshot, or
`servers/snapshots/<endpoint-id>.json`. Snapshots require unique endpoint/tool
identities and input schema objects. The inventory hashes the exact bytes that
were parsed; it does not silently substitute schemas from a running server.

Referenced schemas are compiled with pinned `jsonschema` 0.60.2, external retrieval
disabled, and formats treated as annotations. Local references remain supported.
Neither network nor operator-file schema references may be fetched. Unreferenced
tool schemas are not compiled, so this is not a full catalog conformance check.
Diagnostics identify the scenario, call, endpoint/tool and schema location without
printing argument values.

Static calls must provide object arguments satisfying the complete locked schema.
Captured values cannot be predicted or replaced with convenient validation values:

- A top-level argument property containing a template binding is explicitly
  deferred, including any static siblings nested inside that property.
- Unrelated static top-level properties, required names and closed-schema property
  names are still checked for ordinary object schemas.
- A whole captured argument object is deferred at the root. Root composition,
  references and other value-dependent root constraints also defer the whole root
  when captures occur; the gate does not claim their static parts were checked.
- Deferred constraints need runtime/schema conformance evidence. A successful
  reference execution alone does not establish complete argument validation or
  supported operation semantics.

### Captured-argument constraints

A capture may declare a `constraint`: a JSON Schema for the captured value in the
keyword subset shared by the Rust validator and the generated oracle's
`value-schema.jq` (`type`, `properties`, `required`, `additionalProperties`,
`items`, `enum`, `const`, `minLength`, `maxLength`, `minItems`, `maxItems`,
`pattern`, `minimum`, `maximum`; formats and descriptions stay annotations).
A constraint names exactly one JSON type. Patterns are limited to an ASCII subset
without groups-with-flags or look-around so both engines read them identically.

```toml
[[solution.call.capture]]
name = "refund_id"
pointer = "/content/0/text"
json_pointer = "/id"
constraint = { type = "string", pattern = "^re_[A-Za-z0-9_]+$" }
```

Three checks use the declaration; none fabricates a captured value:

1. **Static authoring.** For every `${binding}` use, the type the binding produces
   (its declared type, or a string when interpolated into text) must be admitted
   by the locked input schema at that argument position, and a declared `enum`
   must be inside the consumer's `enum`. Interpolating a declared object or array
   into text fails. When the source tool has a locked `outputSchema` and the
   capture reads `/structuredContent/...`, the declared type must also overlap
   the source position. A mismatch fails `taskgen` naming the call, binding,
   tool and argument. Positions the locked schema does not constrain are recorded
   as `consumer_unconstrained`, not as verified.
2. **Capture time.** Both oracle implementations (`SolutionCall::capture_result`
   and the generated `capture-constraints.jq` step) validate the actual value
   before it is bound. A violation stops the program; the value is never bound.
3. **Before the consuming call.** The generated oracle validates the fully
   rendered arguments against the consuming tool's locked input schema
   (`call-N.arguments-schema.json`, checked by `argument-schema.jq`) and exits
   before the request is sent if they violate it. `OracleTools::check_resolved`
   is the same check with the complete Rust validator. A task generated without
   the locked schemas still emits the check, so that oracle refuses to send the
   call rather than sending it unvalidated.

The inventory drops `captured_argument_constraints_deferred` for a scenario only
when every call using captured values has a declared constraint for each binding
**and** a locked input schema inside the runtime-checkable subset. Otherwise the
gap stays and the call's `captured.open` list says why
(`undeclared_capture_constraint:<binding>` or
`locked_input_schema_not_runtime_checkable:<keyword>`). Each call's `captured`
entry lists every binding use, its mode and its static result.

Limits: a declared constraint is the author's claim about a value's shape, not
proof that the value identifies the intended record; schema validity still does
not establish operation semantics; and the jq validator covers only the subset
above (the parity fixture in `crates/scenario/tests/captured_arguments.rs` compares
it with the Rust validator).

The status and deferred JSON Pointer paths appear on each inventory call.
Binding names, capture pointers and forward references are independently validated.
Tools and arguments embedded inside code, GraphQL or SQL strings are not recursively
validated as separate operations.

## Validation-evidence admission

Oracle-passes/no-op-fails evidence is not enough to admit a scored task. Each
scored scenario needs declared tests for an independently authored **valid
alternative** and for at least one **near miss** (`wrong_entity`,
`collateral_change`, `partial`, `undone_prohibited_change`). Declarations live in
`catalog/validation-evidence/<group>.toml`:

```toml
[[scenario]]
id = "code/terraform/unlock-orders-prod"
valid_alternative = [
  "crates/world/tests/terraform_contracts.rs::cancellation_then_speculative_verification_is_a_valid_recovery_alternative",
]

[scenario.near_miss]
collateral_change = [
  "crates/world/tests/terraform_contracts.rs::clearing_the_target_lock_does_not_authorize_legacy_workspace_edits",
]
```

Loading (`OracleTools::load`, therefore every `taskgen` run) fails when a
reference is not `<path under crates/**/tests/>::<function>`, the file is
missing, the function is not a `#[test]`/`#[tokio::test]` function, the file
never names the scenario id, the scenario does not exist, a test is listed twice
for one scenario, or an entry references nothing. A scenario without both a valid
alternative and a near miss is not rejected from generation; it carries the
`validation_evidence_missing` source gap, and the inventory's `authoring` summary
counts scenarios with and without evidence and with every near-miss kind.

The check establishes that the named tests exist and belong to a file about the
scenario. It does not run them, and it cannot verify that a test's meaning matches
its declared kind; that classification is reviewed, not derived. Never add a
reference to clear the gap without a test that really exercises that behavior.

## Reviewed-operation admission

`catalog/operation-support/<service>.toml` is the machine-readable index of
[reviewed operation support](operation-support.md). Each reference call of a
scenario must map to a declared operation (`endpoint` + `tool`, plus `operation`
for generic tools: the Stripe `stripe_api_operation_id` and the first two words of
each `call_aws` command). Undeclared operations are listed per scenario under
`authoring.unreviewed_operations` and raise the `unreviewed_reference_operation`
source gap. Declarations fail loading when the endpoint or locked tool does not
exist, the named `section` is not a heading of `docs/operation-support.md`, a
referenced test does not exist, `partial` support omits its limitations, or a
generic tool is declared without an operation.

## Reference execution errors

Every generated oracle, with or without captures, decodes each tool response before
returning it to the solution. A failed tool-call HTTP request, malformed/duplicate/truncated
response, JSON-RPC error, MCP tool error, invalid result object, missing/non-array
content, or non-object structured content stops the program before subsequent
calls or final answer publication.

The decoder accepts JSON and completed SSE responses with one matching response
identity. It does not grep words such as `Error:` in successful tool data, and it
does not complete a truncated event by appending a newline. These are bounded
envelope/content-container checks, not validation of every MCP content-block type,
tool output schema, or vendor-level success convention.

The shared second-stage [tool-result contract](oracle-results.md) now checks
the actual rendered request against the reviewed AWS command-result subset.
Exact declared negative API evidence is separate from wrapper or execution
failure. All other endpoint/tool pairs still have envelope-only checks unless a
reviewed profile is added; these declarations are not a claim of full vendor
output-schema coverage.

An intentionally invalid call is a reference-program failure, not a normal
valid-execution/wrong-outcome probe. Such protocol/error tests belong in explicit
error campaigns. A workflow near miss should use a supported operation reaching
the wrong business outcome.

This is a reference-program validity rule, not a rule that any recoverable tool
error invalidates a model trial. Agent safety and outcome policies remain in the
declared task contract.

## Realized Google control arguments

The Google campaign checks retained static program requests before execution.
Its separate `deploy/check-google-dispatched-arguments.py` checker also validates
the **actual replica HTTP request bytes** exported in a completed trial's
journal. It rejects missing or contradictory replay bytes, duplicate JSON
members/operation identities, nonfinite numbers, incomplete checker coverage and
wrong tool/snapshot bindings. It preserves exact numeric request text rather
than round-tripping it through binary floating point. Boolean and numeric
values cannot silently compare equal.

This closes the unused-static-request gap for the particular checked control
trials, including actual generated event identities. It does not establish
universal runtime enforcement in the replica bridge, validation of all sixteen
deferred source calls, supported operation semantics, authenticated evidence
collection, MCP/client receipt, SDK execution or hosted conformance. Run it
alongside the independent source/business/history and runner/evidence audits,
not instead of those audits. No-op trials are explicitly outside its nonvacuous
call-validation interface.

The checker uses `programcheck`'s conservative rejection of possible template
strings. Literal `${...}` business data needs a separately reviewed realized-value
mode; a rejected analysis check is not by itself a model/business failure.
The qualifying executable and exact locked definition hash must be retained.

## Trusted scripted request observations

Generated reference helpers optionally accept `ORACLE_REQUEST_CAPTURE_DIR`.
Each tool call retains an endpoint and a copy of the already-rendered JSON input
immediately before dispatch. The binding regression verifies that a returned
identity, not an unresolved template, is retained in the later request.
Qualification drivers validate these observed files against the locked input
schemas after execution; generating a template is not realized-argument proof.

This is trusted scripted-reference instrumentation. Main-owned files are not
authenticated native-model receipt or completion evidence. The separate
runner/isolation/source gates remain required, and this option certifies neither
operation support nor the business meaning of a response.

## Reproducible inventory

```sh
CARGO_INCREMENTAL=0 cargo run --locked -p scenario --bin taskgen -- \
  --inventory docs/coverage/scenario-inventory.json
CARGO_INCREMENTAL=0 cargo run --locked -p scenario --bin taskgen -- \
  --check --inventory docs/coverage/scenario-inventory.json
```

The inventory includes scored/example counts, declared contract/preservation/history
requirements, content-check kinds, effect permissions, declared faults,
quantity/latest-record/interval-union checks, exact array-length permissions,
request-cause-scoped permissions, reference
argument admission status, declared result profiles/error permissions and snapshot hashes. The top-level
`authoring` object and each scenario's `authoring` entry report captured-argument
validation, validation evidence and reviewed-operation coverage with their own limits. Each scenario also binds its source,
contract, dataset and instruction bytes. Generation is repeatable; source changes
that leave all declaration counts unchanged still invalidate freshness.

`--check` never repairs the inventory. A stale requested inventory fails before
task regeneration. Source admission failures leave existing tasks and inventory
untouched. Keep the inventory outside `--out`; the task output directory is
reserved for generated task directories.

The summary's task/contract/type counts exclude examples, while reference-call and
argument-schema totals include all validated scenarios, including examples.
`scored_tasks` means declared suite membership, not validated scoreability.
`scored_declared_unscorable` separately counts members with an unconditional
declared environment fault; those rows retain their fault codes and the
`declared_environment_fault` source gap even if the contract is version 2.
Such a trial cannot supply an ordinary model-performance score. Zero declared
faults likewise does not prove runtime support or complete answer semantics.
`--include-examples` explicitly materializes diagnostic fixtures; without it the
generated suite remains the 150 scored tasks. The inventory marks both examples
unscored regardless of which output selection was requested.
The inventory's scope and limits are machine-readable. It does **not** certify:

- Operation support, alternative-plan coverage, or evaluator semantic completeness.
- Successful oracle/negative campaigns, trial admission or immutable runtime state.
- Native completion provenance, production integration, model scores or blinding.

Runtime evidence stays in separately identified release checkpoints. Do not infer
release readiness from declaration counts or mix older execution evidence with
new reference programs.
