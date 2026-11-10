# Native completion provenance seam (2026-11-04)

## Decision

**Available integration seam:** Harbor's host-side custom `BaseAgent.run()` /
`resume()` implementation, loaded through `AgentFactory.create_agent_from_import_path`,
can collect native protocol output directly and populate `AgentContext.metadata`.
For Codex, the documented `app-server` protocol supplies completed reply items and
turn success/error/interruption; `exec --json` supplies a less expressive completion
stream. Neither requires changing the ordinary user request or asking the agent to
write an answer file. [H1][H2][O1][O2]

**Not an existing authenticity guarantee:** stock Harbor 0.23.0 runs both native
agents inside the agent-controlled container. Host streaming avoids subsequent log
rewrites, but does not authenticate the producer against container root. No examined
stock adapter establishes genuine final native completion under this threat model.
Do not admit an answer merely because a JSON event, exit zero, `trajectory.json`,
or a last-message file looks right. A protected native runtime/control channel is
a separate prerequisite, not something a parser or host-authored hash supplies.
The custom-agent seam is implementable now; a full-functionality, root-resistant
native execution boundary remains an integration/release gate. [H3][H4][H5]

**Artifact timing is independently fixable:** use a host lifecycle boundary to
stop and independently inspect `main` *before* Harbor enters main collection.
Do not implement general delayed downloads that return before copying. Stock
artifact handling has synchronous success bookkeeping, a mounted-directory bypass,
and in-container tar/filter operations. Details below. [H6][H7][H8]

Scope: read-only research on November 4, 2026; only this note was written. No
dependencies, inference, harness/container runs, credentials, operator-home
configuration, model selection, or model migration. Official Codex documentation
was searched/fetched before local inspection; Claude claims below come from the
installed primary Harbor code, not an assumed vendor API.

## Observed versus requested versions

- **Observed:** installed distribution metadata says Harbor **0.23.0**, at
  `/Users/luke/.local/share/uv/tools/harbor/lib/python3.13/site-packages/harbor`.
  Below, `H/` abbreviates that exact directory. This identifies the inspected
  installation, not an upstream Git revision or proof that its files are pristine. [V1]
- **Not observed:** a running Codex or Claude CLI version. `BaseInstalledAgent`
  initializes `_version` from the requested option; `version()` returns that field.
  Setup probes `--version` only when it is unset, best-effort. Both installers
  compare an already installed binary against an explicit request, but a requested
  version in `AgentInfo` is not an independent runtime measurement. [H9]
- Unspecified Codex installation uses `@latest`; unspecified Claude installation
  uses an unversioned install. The Codex source comment mentioning `0.118.0` is
  compatibility commentary, not observed execution. Claude's `0.16.2` constant
  pins the **Zed ACP adapter**, not the native Claude CLI. Current official protocol
  docs likewise do not prove support in any particular benchmark image. [H4][H9][H10]

## Current dataflow and provenance

### Installed Codex and Claude

```text
host Trial -> host agent.run -> environment.exec
  -> docker compose exec -> container bash + native CLI + tee
       -> agent-writable native sessions and text logs
  -> host ExecResult / optional live LogEntry callbacks
  -> sync logs -> post-run trajectory conversion -> usage context
  -> main collection -> stop(main) -> world collection -> separate verifier
```

1. **Codex:** `Codex.run()` invokes `codex exec ... --json`, bypasses native
   sandbox/approvals, merges stderr into stdout and tees into `codex.txt`.
   Its `finally` copies native sessions into the agent log tree.
   `populate_context_post_run()` converts a selected session to ATIF and fills
   usage, not an authenticated final-response/completion record. Selection uses
   the latest/deepest date directory and lexically greatest JSONL filename.
   Malformed session lines can be skipped. These are diagnostic reconstruction
   rules, not completion admission rules. [H4][H11]
2. **Claude:** `ClaudeCode.run()` invokes
   `claude --verbose --output-format=stream-json ... --print`, merges stderr and
   tees into `claude-code.txt`; its sessions are under the agent log directory.
   Its post-run converter also fills usage, not native final-completion evidence.
   `_parse_total_cost_from_stream_json()` reads the tee file and returns cost from
   the first parseable `type == "result"` event. Despite “authoritative” in its
   docstring, this is neither an authenticity check nor a final reply/error
   decoder. The inspected code does **not** establish a complete versioned Claude
   success/error/result-text schema; do not invent one. [H5][H12]
3. **Context:** `AgentContext` has token/cost/usage/rollout fields and arbitrary
   `metadata`; no typed final reply, native turn status, or process status.
   `TrialResult` adds timing and exception information. `_sync_agent_output()`
   downloads/prepares logs and backfills context only if it is empty. A new
   adapter setting metadata must populate needed usage itself or deliberately
   arrange backfill; nonempty metadata suppresses the stock post-run call. [H2][H13]
4. **Host capture:** `Trial.add_log_callback()` scopes environment output during
   agent setup/run/verification. `LogEntry` identifies trial, phase, stream,
   optional step, and host timestamp—not native invocation/thread/turn/item IDs.
   Docker's `_compose_exec()` passes `_output_callback()` into
   `_run_docker_compose_command()`; `_collect_streamed_output()` sends decoded
   lines to the callback and returns `ExecResult`. Docker merges stderr into
   stdout even before the native command's own `2>&1`; do not claim two
   independently preserved streams or byte-exact capture here. [H3][H14]
5. **Process status:** `BaseInstalledAgent._exec()` adds `set -o pipefail`, examines
   `ExecResult.return_code`, and raises a classified `NonZeroAgentExitCodeError`.
   Error classification uses regex matches in output, not native structured
   errors. The status is the host-observed Compose exec/shell pipeline result,
   not a separately exposed native CLI PID/signal/result tuple. Both native
   `run()` implementations discard the successful `ExecResult`. Preserve
   wrapper status separately from native terminal status in any replacement. [H15]

### Lifecycle, failure, resume, and isolation

- `Trial._run_agent_phase()` uses `asyncio.wait_for`; `AGENT_END` is emitted in
  `finally`, including timeout/failure. `SingleStepTrial._run_agent()` catches
  timeout/nonzero exit, records an exception, syncs logs, and can continue to
  collection/verifying. At `AGENT_END`, that exception may not yet be recorded.
  Thus neither that event nor verifier execution proves successful completion. [H6][H16]
- `Trial.run()` records cancellation, emits `CANCEL`, recovers outputs and
  re-raises; other exceptions also trigger recovery. Single-step recovery
  collects with `stop_main_before_sidecars=False`. Stop/collect exceptions are
  often logged and swallowed elsewhere. The existing strict guard—not Harbor
  cleanup success—is what prevents unsupported sidecar collection. Docker's
  streamed path terminates the **host subprocess** on cancellation/error;
  that alone does not establish that container descendants stopped. [H6][H14][H16][R1]
- `BaseInstalledAgent.resume()` sets a transient flag around another `run()`.
  Codex copies session logs back and resumes `--last`; Claude uses `--continue`;
  load can seed native or converted ATIF history. These histories are writable,
  and an earlier completed reply must never count for a later attempt.
  Multi-step Harbor stops main before sidecars only on the final separate-verifier
  step; its earlier snapshots cannot inherit final-stop proof. Current repository
  admission explicitly excludes multi-step tasks. [H4][H5][H17][R2]
- Agent logs and the conventional publish directory are bind-mounted from host
  paths, but remain writable by main. “On the host” is not sufficient provenance.
  In separate mode, Harbor uploads declared collected artifacts into a fresh
  verifier environment; it does **not** automatically send arbitrary context
  metadata or a receipt beside `result.json`. Trusted completion delivery needs
  an explicit protected artifact mapping/channel. [H7][H18]
- Claude's available ACP bridge launches `claude-code-acp` in the same environment;
  `acp_runner.run()` stores `conn.prompt()`'s response/error in `acp-summary.json`.
  It does not create an outside-main completion authority. Changing to that
  adapter also changes the execution path; no native-tool parity or authenticated
  Claude final-result capability is established here. [H10]

## Exact available extension points

| Seam | Useful implementation | What it does not establish |
|---|---|---|
| `AgentFactory.create_agent_from_import_path()`; `BaseAgent.setup/run/resume` | Host adapter owns native invocation, decoder and attempt state; ordinary instruction stays ordinary | Installing the adapter outside main does not move its native subprocess/tools outside main |
| `AgentContext.metadata` | Host receipt reference, decoder/provenance status | Arbitrary metadata is not a typed/authenticated native result |
| `Trial.create()`; `add_log_callback()` | Programmatic controller captures live diagnostic evidence before filesystem rewriting | Log callbacks have no per-exec terminal status and cannot authenticate container-root output |
| `BaseEnvironment.scoped_output_callback()` and `exec()` / `ExecResult` | Custom agent scopes capture to one known invocation and retains its return code | Stock Docker combines streams and observes a wrapper, not an attested emitter |
| `Trial.add_hook(TrialEvent.AGENT_END, callback)` | Host controller stops main before subsequent artifact collection; callback can close over `trial.agent_environment` | Event fires on failure too; this is not a native success callback |
| `EnvironmentFactory.create_environment_from_import_path()` | Extend strict Docker admission, main copies/probes/filtering and protected delivery without patching installed Harbor | No public replaceable artifact-handler or “after stop, before all artifacts” hook was found |

These are installed Python APIs, not a claim that `harbor run` exposes a CLI option
for each callback. A controller must construct the trial and register callbacks
before `run()`. Sources: [H1][H2][H3][H8][H16][H19].

### Native protocol intake, not security by serialization

- **Codex app-server:** initialize a connection, start/resume a thread, start a
  turn; match `item/completed` reply items and `turn/completed` by identity.
  Reply `phase` is optional; when available, `final_answer` distinguishes final
  text. Terminal status can be completed, failed, or interrupted, with errors;
  retry notifications do not necessarily terminate the turn. Require a
  version-tested final discriminator; missing/ambiguous phase is unsupported
  unless that exact runtime has another validated final-reply rule. [O1]
- **Codex exec:** official docs describe JSONL item/turn/error events, ordinary
  stdout final output and `--output-last-message`. These enable machine intake,
  not protection against root rewriting files, replacing the CLI, or injecting
  output. SDK `finalResponse` / `final_response` is also a convenient interface,
  not documented root-resistant attestation. [O2][O3]
- **Claude:** the installed stream-json invocation and cost parser establish a
  place to add a decoder, not its full terminal contract. Obtain a version-pinned
  primary schema/implementation and fixtures before admitting Claude; a
  `type:"result"` string alone is inadequate. [H5][H12]

**Threat-boundary inference:** direct host collection protects captured evidence
from *later filesystem edits*. Container root can still control binaries, session
files and processes, and may reach another process's descriptors or otherwise
fabricate the stream; precise exploitability depends on namespace/capability/
process protections not established by these sources. JSON framing, host timestamps,
random attempt IDs, exit zero, a protected output filename, or hashes alone do
not establish a genuine native producer. A provider relay alone also does not
prove which upstream text the native harness accepted as its final turn.

For authenticity, the native runtime, state, control transport and receipt writer
must be beyond the root shell's modification/injection surface, while native
file/shell operations still execute in the intended task environment. Official
app-server docs describe a remote Code Mode host connection, but do not establish
a drop-in Harbor Docker executor or full native-tool equivalence for this
benchmark. Do not promise that bridge, move unrestricted tools onto the trusted
host, replace native tools with a few MCP tools, or disable shell/file access and
call it the same condition. Until isolation **and** tool parity are validated,
record capture as untrusted diagnostics and native authenticity as unsupported. [O1]

## Main-artifact timing: verified gap and workable seam

`SingleStepTrial._run()` calls `_collect_artifacts_phased()` before stopping the
environment. That method runs main collect hooks and main downloads first.
Only afterward, **and only if sidecars exist**, it optionally calls
`stop_service("main")`, catches its failure, then collects sidecars. The strict
adapter gates non-main `service_exec` and `service_download_*`, not main copies;
its receipt therefore currently proves world collection quiescence only. [H6][H7][R1]

**Do not queue downloads and return success until the later stop:**

- Awaiting a later stop inside an early download deadlocks this sequential caller.
  Returning early makes `ArtifactHandler._download_artifact()` claim the target
  and report `ok` immediately; manifest writing is immediate and best-effort,
  with no pending/future collection contract. [H7]
- Mounted convention artifacts bypass `service_download_*` entirely:
  `_record_mounted_artifacts_dir()` inspects the live host mount and reports
  `ok`/`empty`. Type detection uses `service_is_dir()` first; failure falls back
  to guessing from a filename suffix. [H7]
- Excluded directories use `_download_dir_with_exclusions_impl()`: run tar
  **inside main**, download the temporary archive, then remove it. Deferring
  only its download races cleanup; stopping first prevents tar creation.
  Normal Docker file/directory downloads use Compose/engine `cp`, which avoids
  container exec, but that does not make all handler operations post-stop safe.
  Post-stop Docker behavior still needs an actual isolated regression. [H8][H20]
- Recovery may omit the early main stop, no-sidecar tasks omit it, and multi-step
  collection has different boundaries. A queue flushed in `stop_service()` alone
  does not cover these paths. [H6][H7][H17]

**Proposed bounded fix:** for the already admitted single-step, Linux Docker,
separate-verifier condition, a programmatic host controller registers
`AGENT_END` to call the strict adapter's bounded `stop_service("main")`.
This is after the native `run()` cleanup and before log sync/main collection.
Keep the poisoned failure state even if Harbor later recovers; repeated strict
stop already re-inspects identity/state instead of rehabilitating failure.
On failure, deny *all* authoritative collection and verifier admission. [H16][R1][R2]

Then extend the environment seam so every main probe/copy rechecks the same
stopped container; inspect/copy through the host engine without restarting main.
Implement type detection and exclusions against a safely staged host snapshot,
or initially reject main exclusions and main collect hooks explicitly. Do not
silently guess types or fall back to `docker exec` after stopping. Mounted
publish directories need an explicit post-stop, host-owned snapshot/hash, since
the handler bypasses transfer methods. Linux log ownership preparation also
normally uses in-container operations and is best-effort: arrange access before
stopping or via a separately validated host-side path. [H7][H8][H20][H21]

This closes timing without pretending to authenticate agent-authored files.
If supporting arbitrary main collect hooks, intermediate resumed steps, or
generic filtering is required immediately, a deliberately revised Harbor
collection API/controller orchestration is needed; no supported artifact-handler
injection is exposed by the inspected trial initialization. Avoid monkey-patching
`Trial._collect_artifacts_phased()` as though it were a public plugin seam. [H19]

## Proposed minimal receipt and admission

This is a design, **not implemented or currently admitted**:

1. **Open a host attempt.** Bind trial UUID, scenario/task/contract hash, host
   session, attempt nonce, invocation ID, and exact unchanged instruction hash.
   Record requested versions separately from independently observed protected
   runtime versions/binary or image digests, protocol/schema digest, adapter
   implementation digest, and execution-isolation policy. Never read authority
   back from a main-mounted log.
2. **Decode one native invocation.** Custom `BaseAgent.run()` owns its transport
   and exit/error/cancellation handling. Maintain started -> active -> exactly
   one terminal state; bind native thread/turn/item IDs and resumed-parent receipt.
   Capture final user-facing text only from validated native reply events.
   Ignore reasoning events/content; do not store chain-of-thought. Limit event/
   text sizes and store protocol diagnostics with sensitive fields excluded.
3. **Seal separate outcomes.** Retain native terminal state/error, transport
   completeness, host wrapper/CLI status if available, controller exception,
   timeout/cancellation, final-text bytes/hash, and source trust classification.
   A live app-server need not exit for a successful turn; an exec process must
   have the version-tested exit result. Never promote partial text on EOF,
   nonzero exit, malformed/duplicate terminal events, replay, or cancellation.
   A native failure remains a native failure even if some text exists.
4. **Quiesce and bind evidence.** Stop/independently inspect main before its
   final snapshot and world collection. Bind receipt to that container identity,
   stop/check observations, snapshot hashes, world bytes and final text bytes.
   Keep native completion and artifact quiescence as independent predicates.
5. **Deliver through a protected channel.** Store authoritative receipt/text
   outside all main mounts; publish through the world-only channel, with an
   explicit new artifact mapping for the separate verifier, or another validated
   host-to-verifier input. The captured text file is collector-authored—not a
   requested agent answer file. Update private approved policy to bind collector
   code/schema/isolation, not just the existing two-file adapter hash. Hashes
   bind bytes; authenticity derives from the protected producer and delivery
   path, not the receipt filename. [R1][R2][H18]
6. **Fail closed.** Only an approved producer/boundary + unambiguous native success
   + complete capture + valid quiescence + matching protected delivery may admit
   the native reply. Keep `untrusted_capture`, `unsupported_protocol`,
   `native_failed`, `cancelled`, `transport_incomplete`, and `collection_failed`
   distinct. Missing support/provenance is an environment/admission problem,
   not ordinary incorrect-answer reward zero. Successful refusals, clarification,
   or no-action replies remain genuine replies; semantic grading is separate.

## Supported versus unsupported now

| Case | Defensible status |
|---|---|
| Existing strict Docker world receipt | Existing admitted world-sidecar collection only; neither main final snapshot nor native reply authenticity |
| Stock Codex/Claude tee or native-session parsing | Diagnostic, agent-writable evidence; not authoritative completion |
| Host live callback + host-observed pipeline status | Better capture provenance; still untrusted emitter under container-root threat |
| Custom host agent + validated protocol decoder | Available extension; native authenticity conditional on independently validated runtime isolation |
| Protected Codex app-server reply/terminal collection | Documented protocol can support it; benchmark-compatible isolation/tool bridge not validated here |
| Claude result decoding / ACP substitution | Terminal schema and/or execution equivalence unresolved; not admitted |
| Shared verifier, multi-step/resume history as authority, external Argo/GKE | Not covered by current strict policy or this proposed bounded implementation |

These limits preserve the separate release gates in `docs/runner-admission.md`
and proposal 11 / ordered gate 2 of `docs/overhaul-checklist.md`. [R3]

## Next bounded code/test slice

1. Add a host receipt state machine and a custom-agent integration interface;
   test with synthetic protocol events and fake `ExecResult`s, not paid runs.
   Initially mark stock in-container emitters `untrusted_capture`; no native
   answer-contract success can be manufactured by this first slice.
2. Add offline Codex fixtures for commentary versus final, missing phase,
   terminal-before-final, retry then success, failure after text, EOF/nonzero,
   duplicate/out-of-order/cross-turn events, stale resumed turns and cancellation.
   Claude remains unsupported until a versioned primary terminal schema is
   obtained. Do not infer its success fields from the cost parser.
3. Exercise a programmatic `AGENT_END` stop gate plus strict main-copy guard.
   Cover timeout/nonzero (hook still fires), failure during hook/recovery,
   false-success stop, restart/identity changes, mounted convention directories,
   file/directory type detection, rejected exclusions/main hooks, copy failure,
   and absent sidecars. Assert stop/inspect precedes **every authoritative main
   snapshot and world command**, not merely eventual environment cleanup.
4. Follow with one explicitly approved, no-model Docker fixture regression:
   background writer + main file and mounted publish directory + world sidecar;
   verify post-stop bytes, manifest failure handling and separate-verifier
   delivery. Attempt forged native events/files and ensure they stay untrusted.
   No model/harness execution was performed for this research.

Do not remove legacy answer collection, mark the native-completion release gate
closed, or approve a new runner hash until the protected native runtime retains
the intended native tools and passes adversarial provenance tests.

## Implementation addendum

Subsequent revision-11 work implements the narrow artifact-timing seam described
above: an `AGENT_END` controller, stopped-main file staging, protected mounted
snapshots and exact legacy-answer byte binding. The actual supported scope and
validation evidence are in [runner admission](../runner-admission.md) and
[implementation progress](../implementation-progress.md).

This does not implement the proposed protected native runtime, protocol decoders
or completion receipts. Stock in-container Codex/Claude emitters remain unsupported
as authoritative native completion sources under the container-root threat model.

## Primary sources

Official OpenAI pages fetched November 4, 2026:

- [O1] [Codex app-server](https://developers.openai.com/codex/app-server.md):
  protocol, lifecycle, item/turn events, errors, remote Code Mode host.
- [O2] [Non-interactive mode](https://developers.openai.com/codex/noninteractive.md):
  stdout, JSONL events, last-message output, resume.
- [O3] [Codex SDK](https://developers.openai.com/codex/sdk.md):
  native thread/final-response convenience APIs.

Installed local primary code; `H/` expands to the exact directory above:

- [V1] `H/../harbor-0.23.0.dist-info/METADATA:3`, distribution `Version`.
- [H1] `H/agents/factory.py:174`, `AgentFactory.create_agent_from_import_path`;
  `H/agents/base.py:338`, setup/run/resume contract.
- [H2] `H/models/agent/context.py:21`, `AgentContext`.
- [H3] `H/trial/trial.py:364`, callbacks and scoped phase logging;
  `H/trial/hooks.py:62`, `LogEntry`.
- [H4] `H/agents/installed/codex.py:1330`, `Codex.run`.
- [H5] `H/agents/installed/claude_code.py:1800`, `ClaudeCode.run`.
- [H6] `H/trial/single_step.py:38`, run/recovery and agent error handling.
- [H7] `H/trial/trial.py:1365`, phased collection;
  `H/trial/artifact_handler.py:132`, download/upload;
  `H/trial/artifact_handler.py:234`, synchronous claims/probes/copies;
  `H/trial/artifact_handler.py:343`, mounted bypass;
  `H/trial/artifact_handler.py:392`, manifest writing.
- [H8] `H/environments/base.py:965`, exclusion transfer;
  `H/environments/base.py:1284`, service type probe.
- [H9] `H/agents/installed/base.py:632`, requested version storage;
  `H/agents/installed/base.py:833`, version accessor;
  `H/agents/installed/base.py:1040`, setup/probe;
  `H/agents/installed/codex.py:328`, installation;
  `H/agents/installed/claude_code.py:410`, installation.
- [H10] `H/agents/installed/claude_code.py:134`, ACP version;
  `H/agents/installed/claude_code.py:473`, ACP command/install;
  `H/agents/installed/acp_runner.py:804`, prompt response/summary/error.
- [H11] `H/agents/installed/codex.py:384`, session directory selection;
  `H/agents/installed/codex.py:781`, session conversion;
  `H/agents/installed/codex.py:1158`, post-run context.
- [H12] `H/agents/installed/claude_code.py:944`, tee result-cost parser;
  `H/agents/installed/claude_code.py:1055`, session conversion;
  `H/agents/installed/claude_code.py:1591`, post-run context.
- [H13] `H/models/trial/result.py:70`, `TrialResult`;
  `H/trial/trial.py:935`, context backfill/sync.
- [H14] `H/environments/docker/docker.py:644`, subprocess launch/stream merge;
  `H/environments/docker/docker.py:776`, streaming/error cleanup;
  `H/environments/docker/docker.py:1289`, `_compose_exec`.
- [H15] `H/agents/installed/base.py:875`, error classification;
  `H/agents/installed/base.py:913`, `_exec`;
  `H/environments/base.py:86`, `ExecResult`.
- [H16] `H/trial/trial.py:399`, lifecycle dispatch/run/finalize;
  `H/trial/trial.py:496`, agent timeout/finally.
- [H17] `H/agents/installed/base.py:1118`, resume/load;
  `H/trial/multi_step.py:94`, step collection order.
- [H18] `H/trial/trial.py:787`, separate-verifier artifact upload;
  `H/trial/trial.py:1749`, agent mounts.
- [H19] `H/environments/factory.py:285`, environment import seam;
  `H/trial/trial.py:282`, `Trial.create`;
  `H/trial/trial.py:1257`, artifact-handler construction.
- [H20] `H/environments/docker/docker_unix.py:179`, host engine transfers;
  `H/environments/docker/docker_unix.py:237`, file/directory copy.
- [H21] `H/environments/docker/docker.py:1073`, ownership preparation/stop.

Repository primary sources:

- [R1] `deploy/runner/strict_docker.py:84`, protected receipt;
  `deploy/runner/strict_docker.py:126`, inspect/stop/admission;
  `deploy/runner/strict_docker.py:178`, sidecar-only collection gates.
- [R2] `deploy/runner/admission.py:19`, current implementation hash;
  `deploy/runner/admission.py:30`, admitted conditions;
  `deploy/runner/admission.py:124`, sticky stop/identity guard.
- [R3] `docs/runner-admission.md:1`; `docs/overhaul-checklist.md:1`.
