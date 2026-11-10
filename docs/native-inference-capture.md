# Host-side inference capture

This is a source implementation, not a qualified model campaign, production
Harbor integration, published-image admission or held-out release.

## Actual host seam

For pinned Harbor 0.23.0 `Computer1`, `Computer1HostCapture` wraps bound methods
on the particular host agent and recorder instances. It does not edit the
installed Harbor package or patch process-global classes.

The recorder's real `record_agent_step` and `record_parse_error_step` methods
provide episode metrics and response identities. The agent's real
`_write_final_answer` argument supplies the public result selected by the
harness. Raw response JSON, analysis, reasoning fields, screenshots, container
answer files and container-authored trajectories are not used as the public
answer. A first terminal-action confirmation candidate is not mistaken for
the final reply.

If the generic harness falls back to raw action JSON because no explicit
public result was supplied, it is recorded as `no_public_response`, not as
user-facing prose containing analysis. A natural-response contract then fails
its missing-answer check; an explicitly requested artifact can still satisfy
its own contract. No final message or turn reference is fabricated. Session
finalization remains explicit even when `final_response` is absent.

Finalization requires the original observer bindings to remain installed.
Native evidence binding additionally requires the exact sealed in-memory
capture object; an actor-authored document with a familiar adapter name and
public code hash has no such authority. An unbound inference document cannot
certify host provenance.

The capture is tied to the observed host agent and the current task/session.
That identity is also embedded in its bytes and checked against the receipt
and realized manifest, preventing cross-trial capture reuse.

The source tests call the installed recorder and final writer, and exercise
the real generic episode loop with offline responses. They do not call a paid
provider or qualify a real desktop/container condition.

`Computer1` is a computer-use harness. Host-native inference capture is **not**
the benchmark's native-MCP exposure arm. Installed container-side CLI agents
remain unsupported by this capture adapter.

## Protected consumer path

The controller installs observation before `trial.run`, seals its evidence at
agent end and always attempts shutdown even if capture validation fails.
The strict runner writes inference evidence outside the actor's mounted log
directories, checks its bytes again before publication, and publishes it via
the world-only artifact channel. The receipt binds its exact hash.

Generated policy pins the observer implementation as well as the whole runner
implementation. The realized manifest records configured model/provider and
settings before execution. Rust `inference::load_bound`, shared by
`bench-verify` and report loading, checks policy, receipt, artifact, identity,
scripted/native consistency and finalized response structure.

The report reproduces the assessment using the same answer selection and
classification logic, rejects substituted artifact/copy pairs, and retains
classification, model identities and nullable cost accounting. Inference bytes
also participate in content-review bindings.

## Answer channels

Existing contracts default to `legacy_answer_file`; this preserves authored
requests that actually ask for an artifact and preserves their canonical
contract identities. They are not silently reinterpreted as chat tasks.

New version-two contracts may declare:

```toml
[expect.answer]
source = "final_response"
```

They must also declare meaningful answer predicates. Scenario validation no
longer forces such requests to mention `/app/answer.md`, and rejects a final
response declaration that would replace an explicitly requested answer file.
For host-captured runs, the verifier grades the captured response for that
channel even if an actor file contains a different answer.

Oracle/nop remain explicitly scripted controls; their reference-answer files
are not natural model replies. Existing task instructions are unchanged by
this source slice. Deliberate natural-response task migrations, semantic
answer hardening and their evaluator release remain outstanding.

## Identity, failures and costs

Inference schema 2 distinguishes unavailable retry observations (`null`) from
known zero counts. Costs and token totals are unknown when any captured
episode lacks that measurement; known zero values stay zero.

Per-turn identifiers retain observed model strings and available response
IDs. Generic `LLMResponse.model_name` comes from the response. Vendor-provider
paths that echo the configured model are not treated as observations; an
available response-payload model can establish identity, otherwise it remains
unknown. Immutable provider revisions are never invented.

Captured totals describe host-observed episodes, not guaranteed full provider
billing. Hidden router inference, failed requests without usage, internal SDK
retries and other unobserved stages are not assumed free. Retry breakdowns
remain unknown.

Typed context rejection is classified separately from startup traffic; a
legacy context flag cannot override structured native evidence. Turn-budget
exhaustion is not mislabeled normal completion. Transport capture does not
certify a policy-correct refusal or semantic task success.

The historical tally path remains labeled legacy/non-comparable. The
`--verified --report-bin <bench-report> <run-directory>...` tally path consumes
the report loader's checked summary, not `.meta` or transcript grep.

## Remaining admission work

- Run authorized model controls with declared budgets on an appropriate,
  qualified computer-use environment. No paid inference has been performed.
- Qualify other inference failure paths, provider retries, hidden inference
  costs and transport timing rather than inferring them from text.
- Qualify canonical failure export when agent setup fails or Harbor skips
  verification, along with effective provider sampling and the host SDK
  dependency closure. Configured settings are not invented effective settings.
- Audit actual resolved runtime surfaces. In particular, upstream
  `Computer1._build_observation` emits grading-related confirmation language;
  this source change does not remove it or certify a blinded condition.
- Integrate the separate source slices, bump the evaluator deliberately,
  regenerate canonical tasks/inventory and qualify final published images.
- Validate the actual external production consumer and preserve durable
  evidence custody. The draft PR remains do-not-merge.
