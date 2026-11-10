# Controlled exposure, the resolved-surface audit and awareness conditions

Controlled exposure works with stock Harbor and is how production changes the size of
the tool set ([production.md](production.md#changing-the-size-of-the-tool-set)). The
resolved-surface audit and the `--information-treatment` comparison apply only to
strict-controller trials.

Status: wired into real consumers at source-test scope. No model was run. Nothing here
qualifies a published image, a native harness or a provider.

This covers improvement-plan items 7, 8, 14 and the native date-context part of 9.

## One source of truth

| Concept | Lives in | Consumers |
| --- | --- | --- |
| Exposure variants, manifest, catalog stages | `mock_core::conditions` | taskgen, report |
| Exposure planning from pinned definitions | `scenario::exposure` | `taskgen --exposure` |
| Registration directive and its enforcement | `mcp_tools::exposure` | taskgen (writer), `tools-supervisor` (bridge) |
| Served-response transcript | `mcp_tools::transcript` | `tools-supervisor`, strict runner |
| Surface capture and marker scan | `deploy/runner/surface_audit.py` | strict runner |
| Sealed audit record and its status | `mock_core::surface_audit` | `bench-verify`, report |
| Awareness cells, measures, matched-information admission | `mock_core::conditions` | taskgen, `bench-verify`, report |

The earlier `deploy/consumer` Python package and `scenario::audit` were unused duplicates
of these and are deleted.

## Controlled exposure (item 14)

```
taskgen --exposure relevant_only|context_feasible|organization_pack|full_catalog|randomized_order|confusing_distractors
        [--exposure-seed N] [--exposure-token-budget N]
```

| Variant | Registers |
| --- | --- |
| `full_catalog` (default) | Every endpoint in catalog order. Generates exactly the usual bytes. |
| `relevant_only` | Only the tools the reference plan calls. A diagnostic on an easier condition. |
| `organization_pack` | Every endpoint of the applications the scenario declares in `services`. |
| `context_feasible` | The organization pack, then catalog order while the estimate fits the budget. |
| `randomized_order` | The full catalog with seeded endpoint and tool order. |
| `confusing_distractors` | The organization pack plus whole authentic endpoints of other applications, ranked by tool-name token overlap, in seeded order. |

A plan only selects and orders. Definitions come from `servers/snapshots` and are never
edited, renamed or invented; a distractor is a real endpoint with a real backend.

For a non-default variant taskgen changes four things from one plan:

1. `task.toml` lists only the registered MCP servers, in realized order.
2. An ordinary workspace's `connections.json` lists the same endpoints in the same order.
3. The tools container receives `TOOL_BENCH_EXPOSURE`, a directive naming endpoints, tool
   order and a hash over the pinned definitions. It carries no variant label or seed.
4. `tests/exposure.json` (private) records variant, seed, realized order, per-tool
   definition hashes, catalog stages and the differences from the full catalog.

`tools-supervisor` serves only the directive's endpoints. Each is wrapped so `tools/list`
returns the registered tools in order and `tools/call` treats any other tool as unknown.
If a served definition does not hash to its pin, `tools/list` fails; nothing is registered
from a drifted definition.

The task context binds the manifest (`exposure_sha256`) and the directive
(`connector_env_sha256.TOOL_BENCH_EXPOSURE`). The strict runner already refuses a trial
whose tools container does not carry the bound value.

### Catalog stages

`metrics` reports `stored` and `registered` (endpoints, tools, `o200k_base` estimate).
`provider_accepted`, `model_visible` and `loaded` are `null` until something observes
them. The token figure is an offline estimate from the pinned snapshot. It is not a
provider's context admission, and `context_feasible` means "fits the declared estimate".

The host audit adds what the agent was actually served (`listed_endpoints`,
`listed_tools`, `listed_catalog_sha256`) and whether it matches the private manifest.

### Discovery path and grading

Grading reads world state through `tests/expect.json`, which is byte-identical under
every variant. No exposure field is an input to `bench-verify`'s grade, so a plan that
used different tools is graded exactly like the reference plan. `reference_plan_registered`
is a private note, not a score input.

## Resolved-surface audit (item 7)

The strict runner captures real bytes at two points.

Before the agent starts (`observe_execution`):

- the resolved instruction Harbor hands the agent;
- every file actually in the agent container's `/app` and `/root`, by `docker cp`;
- the agent container's environment, argv, mounts and DNS-visible names, and the names
  of any container sharing a network with it, from `docker inspect`.

After the agent stops and world evidence is collected (`_publish_receipt`):

- the `initialize` and `tools/list` responses and every tool result and error body the
  tools container served, from its passive transcript.

The transcript is written by the bridge front end as it answers. It issues no request of
its own, so it cannot change what the world records.

### Outcome

| Status | Meaning |
| --- | --- |
| `clean` | Every required surface was captured and none held a known marker. |
| `findings_present` | A marker, a date contradiction or a registration mismatch was found. |
| `unqualified` | A required surface was not captured, or no valid record was sealed. |

A surface that could not be captured is never treated as passed. That includes an
endpoint whose `tools/list` the agent never requested, a home directory larger than the
capture bound, a missing transcript, and a trial that never reached sealing. An oracle
trial does not list tools, so it is `unqualified` by construction.

Markers are evaluator-owned identifiers only: the scenario ID and task name, the trial
admin token value, admin and health routes, verifier and private-manifest names, and the
verbatim contract and solution artifacts. Generic words an authentic vendor tool may use
(test, fixture, reward, oracle) are not markers. Shared network attachment alone does
not establish control-route reachability, and the audit no longer fabricates such a
finding. Protected worlds bind control routes to loopback on port 9001, separately
from bootstrap health/CA routes on port 9000, bound to a tools/world-only interface.
The agent never joins that bootstrap network, including in a direct-API track.
Readiness uses loopback; world startup resolves its own bootstrap alias and does not
depend on the tools process starting first. Socket and route isolation
has source-level TCP tests; this audit does not qualify a live deployment's isolation.

A finding is a surface, an item index, an item hash, a byte offset, a length and an
ownership reason. Marker text, file contents and environment values are never written.
Environment items are never hashed.

### Custody

The full report is `surface-audit.json` in the trial directory on the host, outside every
agent-writable mount, and is never overwritten. Its hash and a summary go into the runner
receipt (`surface_audit`), which is bound like the other runner artifacts.
`surface_audit.py` is part of the runner adapter hash.

`bench-verify` writes a `surface_audit` status into `verification.json`. The report shows
it per trial (`bench-report --trials`) and counts it per arm. A receipt that failed
admission yields `unqualified`. The audit never changes reward or outcome.

The status describes captured bytes. It must not be reported as showing that a model was
unaware of being evaluated.

Admission also requires the complete resolved harness messages and the actual agent
process arguments/environment, not just the container's initial command. These two
surfaces must never be inferred from source templates.

For the Claude Code proxy condition, the host scans the actual forwarded Messages
requests, including system text, public conversation messages and tool definitions.
The scan covers auxiliary calls and retries too. Only hashes, redacted offsets and
date-coherence counts enter the host event chain; raw prompts, credentials and private
reasoning blocks are not retained. The marker configuration stays in the proxy's
host-only directory. A changed configuration, missing request scan or incomplete
event chain leaves `harness_messages` uncaptured. The real controller installs and
finishes this observer before sealing the audit.

The Computer1 condition still lacks this input observer. Actual agent-process
arguments/environment also remain uncaptured. Consequently production audits still
remain `unqualified`; synthetic process observations in tests are not runtime
qualification. Offline fake-provider/controller tests establish the request-scanning
consumer path, not a live CLI or production deployment.

## Date coherence (item 9, native side)

The audit compares agent-visible date carriers with the scenario's
`business_reference_time`: the workspace `session.json` `data_as_of`, date-named
environment variables, and explicit "today is" statements in the instruction and files.

`date_coherence.status` is `coherent`, `contradiction` (also a finding) or
`not_supplied`. The ordinary workspace profiles render the reference time into
`session.json` from the scenario. Nothing sets a process, TLS or authentication clock.
The minimal workspace supplies no date, so it reports `not_supplied`, never `coherent`.

## Awareness conditions (item 8)

```
taskgen --awareness-condition neutral_minimal|neutral_ordinary|evaluation_minimal|evaluation_ordinary
```

A cell selects the existing framing and workspace profile together. Taskgen derives the
cell back from the manifests it just materialized and refuses a task that does not
realize the declared cell. No new file is generated: the cell is read from the bound
`framing.json` and `profile.json`.

`bench-verify` copies those manifests (and `exposure.json`) forward only when the task
context binds their bytes. The report derives each trial's condition from them.

```
bench-report --require-attested <a> <b> [--information-treatment framing|workspace|exposure]
```

- Without a declared treatment, arms whose information differs are rejected and the
  differing items are named.
- With one, every difference must lie on that axis and at least one must exist. The
  output lists the actual differences, the prespecified behavioral measures and the claim
  limits.
- A workspace difference is reported as an information treatment. It includes the
  standing scope guidance, operator identity, reference time and connection inventory, so
  it cannot isolate awareness.
- A neutral-framing arm is labelled `blinded` only when every one of its trials has a
  `clean` surface audit including those runtime surfaces. The current capture cannot
  establish that label.

The behavioral measures are fixed in `mock_core::conditions::BEHAVIORAL_MEASURES`. Two of
them (`unnecessary_confirmation`, `discovery_strategy`) have no consumer computing them
yet and say so.

## Not covered

- No native-model run, so no measured sensitivity and no observed `provider_accepted`,
  `model_visible` or `loaded` stage.
- The runner seam is tested with real task sources and real tar and inspect shapes, but
  against mocked engine calls. It has not run against a live Docker trial.
- System and developer prompts a harness injects, including its own date line, are not
  captured here.
- Discovery budget accounting (item 14, "discovery inference, execution, and token costs
  all consume the declared budget") is not implemented.
- The scan is lexical. It does not detect semantic leakage.
- Generated tasks are not regenerated on this branch. `tests/runner-policy.json` in every
  task is stale because the adapter hash now includes `surface_audit.py`.
