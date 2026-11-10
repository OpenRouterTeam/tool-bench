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

The status and deferred JSON Pointer paths appear on each inventory call.
Binding names, capture pointers and forward references are independently validated.
Tools and arguments embedded inside code, GraphQL or SQL strings are not recursively
validated as separate operations.

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
argument admission status, declared result profiles/error permissions and snapshot hashes. Each scenario also binds its source,
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
