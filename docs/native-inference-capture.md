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
are not observed by this adapter; they use the provider-proxy capture below.

## Provider-proxy capture for container-side CLI agents

The off / native / universal arms run Claude Code inside the task container. No
host object sees its turns, so the capture sits at the one seam the host owns:
the provider proxy, `deploy/arm-proxy.ts`.

**What the agent holds.** `ProviderProxyCapture` (`deploy/runner/proxy_capture.py`)
starts the proxy on the host with the real credential in the proxy's
environment. The agent gets three variables: the proxy address, a per-trial
placeholder key, and the arm's `ENABLE_TOOL_SEARCH` switch. The proxy answers
401 to any other key and swaps the placeholder for the real credential
upstream. Before the trial starts the controller checks the environment Harbor
will actually pass to the container (`ClaudeCode._resolve_auth_env`) and refuses
the trial if a host OAuth token, Bedrock setting, or the real key would ride
along. `run.py -a claude-code` removes the credential from its own environment
before Harbor resolves agent settings.

**What is recorded.** One hash-chained event per request and per response, in a
host-only temporary directory outside every trial mount: request and response
hashes, HTTP status, provider response id and model, stop reason, content block
types, usage, provider-reported cost, latency and time to first byte. Streamed
(SSE) and non-streamed Anthropic Messages responses are both parsed. Thinking
blocks, tool inputs and tool results are never stored. The text of a reply is
kept only when the provider ended the turn (`end_turn` / `stop_sequence`) with
no tool call.

**Main thread.** An exchange belongs to the task conversation when its first
user message contains the task instruction. Side requests (titles, subagents)
are auxiliary: counted and costed separately, never a turn and never the final
reply. The final response is the last main-thread reply, and only if it is the
last main-thread exchange.

**Sealing.** At agent end the runner stops the proxy with SIGTERM, which writes a
`finalized` event stating how many exchanges were still open. The sealer then
builds `InferenceEvidence` with adapter
`host-proxy-native-anthropic-messages-v1` and a `transport` section listing
every exchange. Its code identity covers `arm-proxy.ts`, `proxy_capture.py` and
`inference.py`; generated policy pins it as `proxy_inference_adapter`.

**Binding and consumers.** The sealed object goes through the same
`bind_inference_evidence` → receipt → world-only publication path as Computer1.
`inference::load_bound` selects the proxy policy when the realized manifest
names `claude-code`, and calls `transport::check`, which recomputes the
classification, turn kinds, retry counts, token counts and final-reply binding
from the recorded exchanges and rejects any document that disagrees.
`bench-verify` and the report both go through that call.

### Failure classification

`mock_core::transport::derive` is the single classifier of a proxy capture;
`proxy_capture.classify` mirrors it in the runner, and a mismatch is rejected at
load. `mock_core::failure::classify` then combines it with the graded outcome
into one label and fault domain, written to `verification.json` by
`bench-verify`, shown per trial by `bench-report --trials`, and counted by
`tally-arms.py --verified`.

| Ending observed at the proxy | Classification | Graded as |
| --- | --- | --- |
| Last main reply `end_turn` with text | `completed` | by contract |
| Same, after a retried 429/529/5xx/cut stream | `recovered_rate_limit` | by contract |
| Provider rejects turn 0 for length | `first_turn_context_rejection` | context overflow |
| Provider rejects a later turn for length | `later_turn_context_rejection` | context overflow |
| Last main attempt 429/529/5xx/transport/cut stream | `unrecovered_rate_limit` | environment fault |
| First request 400 (not length), nothing ever succeeded | `schema_admission_failure` | environment fault |
| No main request, or first request 401/403 | `environment_startup_failure` | environment fault |
| Reply unparseable or tool-call JSON invalid | `malformed_model_output` | fail |
| Ends on a tool call, refusal, or empty text | `no_public_response` | by contract |
| Ends on `max_tokens`, or on a tool call at the turn budget | `budget_exhausted` | fail |
| Agent phase raised after a clean reply; later non-length 4xx | `failed_finalization` | environment fault |
| Any capture fault (below) | `failed_finalization` | environment fault |
| First main request already carries assistant turns | `resumed_session` | rejected at binding |

A length rejection is recognised from the provider's own error object at the
proxy (HTTP 413, `request_too_large`, `context_length_exceeded`, or a 400
`invalid_request_error` whose provider message states the limit). Agent
transcripts, tool output and world journals are never searched, so MCP startup
traffic cannot hide it and tool output cannot fake it.

Capture faults, each of which fails the capture instead of producing a score:
`proxy_not_finalized`, `proxy_exited_early`, `in_flight_at_finalization`,
`exchange_without_response`, `event_chain_broken`, `event_chain_truncated`,
`capture_not_started`, `capture_implementation_changed`,
`unsupported_wire_format`, `main_thread_unidentified`, `ambiguous_main_thread`,
`unobserved_assistant_turn` (a request carries more assistant turns than the
proxy served) and `untracked_continuation` (tool-bearing work continues outside
the main thread after its last reply, as after a compaction).

### Accounting

Retries are known at this seam, because every attempt is its own request:
`retried` / `retry_count` per turn and `total_retry_attempts` are real counts.
Token and cost totals cover every main-thread attempt and are `null` when any
attempt reported no usage; a rejected or rate-limited request is not assumed
free. `transport.exchanges_without_usage` says how many. Cost is recorded only
when the provider reports it. Auxiliary requests have their own sums. Router
inference behind the proxy target and tool execution time are not measured.

### Explainable result

`bench-report --trials <dir> --json` attaches an `explanation` to every trial:
failure label and fault domain, each failed check with its location, reason and
the artifact it was decided from, the answer channel, the last provider
exchange, accounting with an explicit `unknown` list, and the hashes of the
bound inputs. A trial whose assessment cannot be reproduced is reported as
`verified: false` with the reason and no failure class.

### Local smoke path

`deploy/smoke-arms.sh` runs the same proxy with `--capture` and seals it with
the same code into `<run>.inference.json`; `tally-arms.py` reads only that. The
transcript and `.meta` greps are gone. Runs without a sealed capture are counted
as unclassified, so the 2026-10-31 table's overflow column cannot be reproduced
from its historical files. `deploy/smoke-ori.sh` has no proxy and no longer
classifies context rejections. Local smoke evidence is unbound: the agent runs
on the host next to the capture file, so it is diagnostic only.

### What this does not establish

- No Claude Code trial has been run through the strict runner. The tests drive
  the real proxy, sealer, controller hook, receipt binding, `bench-verify` and
  report against an offline fake provider. Positive completion and recovered
  retry fixtures traverse the pinned `ClaudeCode.run` with a fake Environment
  returning host-observed `ExecResult` stdout; the real CLI is not executed.
  The real CLI's request sequence, its retry bodies and its behaviour after a
  length rejection are unqualified; where they differ from the assumptions
  above the capture fails closed.
- Container-to-host reachability of the proxy and the task egress policy are
  not qualified. `--proxy-listen` / `--proxy-advertise-host` exist; nothing
  here proves the main container can reach them under strict networking.
- A completed proxy capture binds the provider reply to the host-observed CLI
  terminal `result` envelope. It requires one init/result session, integer zero
  exit status, explicit success metadata and a matching terminal-text digest.
  Agent-edited answer files do not certify this channel. This does not qualify
  real CLI behavior, its UI, or a harness that adds its own final prompt.
- A genuine provider failure remains independently classifiable when the CLI
  reports an error envelope. Carrier faults remain visible; they never admit
  an otherwise unbound or faulted normal completion.
- On a nonzero exit, pinned Harbor can raise before `exec_as_agent` returns
  its `ExecResult` to the observer. That path certifies no terminal reply;
  classification uses the independently captured provider exchanges, never
  a reconstructed stdout value from the exception message.
- Only Anthropic Messages traffic is parsed. Chat Completions and Responses
  requests are recorded as unsupported and fail the capture.
- `tasks/*/tests/runner-policy.json` must be regenerated before any generated
  task admits this adapter.

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

The same channel now works for the proxied arms: a `final_response` contract is
graded from the proxy-captured reply even when the container wrote a different
answer file, and a `legacy_answer_file` contract is still graded from the
collected file.

No scored task was moved. 56 of the 152 scenarios declare an answer contract,
and all 56 instructions ask for the answer to be written to `/app/answer.md`.
That is an authored artifact request, so they stay `legacy_answer_file`, and
scenario validation rejects a `final_response` declaration on them. The other
96 scenarios have no answer contract. Grading any of the 56 from the natural
reply needs a new authored instruction, which is a task migration, not a
capture change.

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

The `--verified --report-bin <bench-report> <run-directory>...` tally path
consumes the report loader's checked summary and explanation. The local tally
path reads sealed proxy captures only; neither reads `.meta` or transcripts.

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
