# Runner admission

## Offline Verifier Networking

The reviewed Linux verifier uses Docker `network_mode: none`, not Harbor's
nftables egress-control sidecar. Its final private Compose override is applied
only to the separate verifier. The host independently inspects the actual
container before `start` returns and rejects missing metadata, a mode other than
`none`, or another attached network. Observation failure prevents grading and
triggers cleanup; a configuration declaration alone is not proof.

The verifier advertises only static no-network enforcement. Windows, allowlists,
public-network phases and runtime policy weakening are rejected. Agent/container
network capabilities remain delegated to the ordinary Docker implementation.
This is not permission to suppress network-policy validation generally.

The distinction matters on Harbor 0.23.0: the default Docker no-network capability
depends on an nftables kernel-probe result. A failed probe can reject the separate
verifier before startup even when Linux Docker's hard `none` mode remains usable.
The retained constructor regression forces that probe false and exercises the
actual adapter/base-constructor path in milliseconds. A real no-op trial verifies
the full path. Engine unavailability still fails closed.

The updated adapter has a new approved implementation digest. Rematerialize tasks
and do not pool its trials with the previous collector implementation, even when
the evaluator/world/tools images are otherwise identical. Complete realized
verifier/hardware manifests and external production integration remain separate
open work.

Since evaluator revision 8, generated scored tasks require a trusted runner receipt.
An ordinary Harbor result, a successful collect hook, or a sealed world by itself
does not prove that the agent was stopped before evidence collection.
Revision 11 requires `tool-bench-runner-v2`: main artifact collection must also
follow independently inspected shutdown, and legacy answer bytes are receipt-bound.
Revision 15 requires `tool-bench-runner-v3` and `offline-world-evidence-v5`,
including protected source/context/execution inputs. It admits only local scripted
calibration until native runtime provenance is implemented.

## Supported execution path

The host-side `deploy.runner.controller` uses Harbor's trial lifecycle hooks and
the `deploy.runner.strict_docker:StrictDockerEnvironment` adapter uses its
environment import seam, not a patched global installation. Their current
compatibility target is Harbor **0.23.0** with Linux Docker environments. Other
versions, shared verifiers, unsupported protocols, and stale adapter policies are
rejected. Install `deploy/runner/requirements.txt` into the Harbor Python
environment when provisioning a runner.

From the repository root:

```sh
sh deploy/run-trial.sh \
  -p tasks/business__stripe__customer-mrr \
  -a oracle \
  -o /tmp/tool-bench-admitted
```

The wrapper uses the installed Harbor launcher's Python interpreter; set
`HARBOR_PYTHON` explicitly if that launcher does not have an absolute Python
shebang. It currently exposes only scripted `oracle` and `nop` diagnostics, not
restored native/universal/upfront model runners.

For locally built images, `deploy/oracle.sh <scenario-id>` generates tasks with
explicit image overrides and selects this controller automatically. Selecting
only the strict environment with stock `harbor run` is no longer sufficient:
without the controller's agent-end hook, authoritative collection is rejected.

Harbor applies the same environment adapter to both execution roles. The adapter
admits only the materialized `environment/` and `tests/` contexts; the latter must
have no network, receives no admin credential, and cannot access sidecar services.
It does not overwrite the agent environment's host-owned receipt.

The adapter creates a fresh admin credential for each agent environment. It supplies
the credential only to Compose interpolation of the world's environment, not to the
agent, tools, verifier, receipt, or task files. A caller-supplied global token does
not become the adapter's trial token. Manual Compose diagnostics still need their
own token.

## Scripted Oracle completion

The controller registers the exact solution command emitted by the pinned
`OracleAgent` before execution. The environment observes the host-side Docker
execution result for that command: exactly one execution must return an actual
integer zero. Nonzero or malformed results, execution exceptions, duplicates,
and missing execution invalidate collection before main shutdown or artifact
collection. Setup, chmod and verifier commands cannot establish completion.

This closes a distinct gap: Harbor 0.23.0's Oracle writes a nonzero solution
status to `agent/exit-code.txt` without raising. A program can successfully change
business state and then fail while a normal trial still receives reward 1.
Neither `exception_info: null`, successful state grading, an Oracle log nor an
agent-writable exit-code file proves successful reference-program completion.
The controller does not read those writable files as completion evidence.

Run `sh deploy/test-oracle-completion.sh` with explicit `WORLD_IMAGE` and
`TOOLS_IMAGE` to exercise actual successful GitLab mutations followed by exit 17,
a forged success file/log, and genuine exit zero with a misleading failure file.
The first two must produce no authoritative evidence or scored assessment; the
third must remain admissible. Python regressions also run the pinned agent's
actual `run` method through controller registration.

The hardened adapter has a distinct implementation digest. Historical admissions
prove the execution/collection properties implemented at their checkpoint, not
this later successful-program-completion requirement. Preserve them and the
original mis-admitted failed program as diagnostic evidence; do not pool them
with the hardened condition. Protocol v3 and evidence schema 5 are unchanged:
the private approved adapter digest distinguishes the implementation.

This is scripted process-completion evidence, not authenticated native assistant
completion or proof that every business requirement was met. The workflow grader
still independently evaluates outcomes, preservation, content and history.

## Collection protocol

1. Admit the task's declared protocol and approved runner implementation before
   starting the environment.
2. Register an `AGENT_END` hook before running the trial. The hook also runs when
   the agent raises, times out, or is cancelled; it does not imply native success.
   For an Oracle, first require its registered command's successful host-observed
   completion. Prepare host log access, then stop `main` with a bounded shutdown
   command.
3. Independently inspect the actual main-container identity and state. A successful
   stop command is insufficient if the container remains running, paused, or restarting.
4. Snapshot the stopped main's mounted publish directory into a protected host
   directory. Main probes and copies also require stopped-state proof, including
   unchanged engine start/finish timestamps; a restart followed by another stop
   cannot reuse the earlier observation.
5. Obtain `/app/answer.md` through a bounded host-engine tar stream, never an
   in-container type probe or tar command. Safely stage files/directories, rejecting
   links, special files, unsafe paths and excessive resources. Return only after
   the actual synchronous host copy succeeds. Missing answers are explicit, not
   fabricated empty files; unexpected copy failures poison admission.
6. Check Harbor's main manifest and the delivered bytes against protected
   snapshots, including its convention-directory transfer bypass. Finalize the
   world, then hash the exact exported evidence bytes.
7. Publish the independently observed execution manifest and runner-authored
   receipt through the world-only artifact channel. The
   receipt binds the scenario, host session, main-container proof, runner version and
   implementation hash, contract bytes, collected evidence bytes, main snapshot
   digest, legacy answer bytes/absence and realized manifest.
8. The separate offline verifier checks that receipt against the task's private
   `tests/runner-policy.json`. Missing, mismatched, unsupported, or failed proof
   produces `environment_error`, never ordinary model failure or reward 1.

A stop exception, cancellation, failed inspection, changed container identity, or
restarted main permanently invalidates collection for that attempt. A subsequent
successful stop cannot rehabilitate it. Even if Harbor catches the exception and
continues, the adapter refuses sidecar access and the verifier refuses missing
proof. Preserve the failed attempt and apply only the declared replacement policy.

The host retains `runner-admission.json` beside the trial result, outside the
agent-mounted artifact/log directories. The successfully collected companion is
`artifacts/world/runner-admission.json`. Receipt hashes bind bytes; they are **not
signatures**. Trust comes from the host controller, private approved policy, and
service-scoped artifact provenance. Agent-written files at look-alike paths are not
authoritative.

Protected copies remain under `main-snapshot/` beside the trial result. The
`main_artifacts_sha256` binds the answer tree (or absence) and mounted publish
tree. `answer_sha256` separately binds the raw file bytes read by `bench-verify`;
a later answer substitution or omission invalidates admission even when its text
would satisfy the answer predicate.

This is deliberately not a generic replacement for Harbor's artifact handler.
The admitted task layout has the generated main answer entry and conventional
publish directory. Main collect hooks, main exclusions, extra artifact mappings,
extra runtime mounts/Compose overrides, multi-step/resume, regrades and simulated
users are unsupported. Snapshot bounds are 64 MiB of file content, 10,000 entries
and 64 path elements per tree; archive transfer has an additional bounded overhead.
Unsupported cases fail closed rather than receiving guessed types or early `ok`
manifest entries. Engine errors other than the exact validated missing-path
response are collection failures, not proof of absence.

## Assessment and diagnostics

`runner_admission_checked` distinguishes an admitted scored attempt from a
world-only diagnostic assessment. `runner_admission_ok` is meaningful as evidence
of admission only when the checked flag is true.

`history_checked` means mutation-history checks were requested. `history_ok`
also incorporates preservation and ordering outcomes: it is **not** a
standalone history-evidence admission flag. A valid negative control can have
`environment_ok=1`, `history_checked=1` and `history_ok=0` because the complete
history proves an unauthorized change. Structural history/evidence failures
must still block admission and comparison. Positive controls require safe
history; negative controls require their expected business failure with valid,
checked evidence, not a blanket `history_ok=1` assertion.

Generated verifier commands always supply both `--runner-policy` and
`--runner-admission`, with `--task-source`, `--task-context` and
`--realized-manifest`. Direct diagnostic invocations without runner inputs remain
possible, but cannot establish runner safety. `verification.json` records the
actual policy and receipt input hashes alongside the existing world/contract hashes.

The runner receipt does not prove genuine harness final-response completion, complete
inference accounting, model unawareness, or a leakage-free resolved native prompt.
Those remain separate release gates.
Version-3 source/execution checks, image equivalence and reporting restrictions
are detailed in [execution provenance](execution-provenance.md).

Stock Harbor collects main-service artifacts before stopping `main`. The version-2
controller advances shutdown to the agent-end hook and validates main collection
independently of that stock ordering. This proves post-stop file timing only for
the admitted condition, not authenticated native final-response provenance.

## Validation and production integration

`deploy/test-runner-admission.sh` exercises the real runner with the strict adapter,
an injected main-stop failure, false-success shutdown, the unsupported stock
environment, and the strict environment without its required controller. It uses
scripted oracle executions, not paid inference. Behavioral guard and verifier
regressions cover cancellation, missing inspection, restart/identity changes,
unapproved versions, byte mismatches, and missing proof.
`deploy/test-main-snapshot.sh` adds background writers, mounted publish snapshots,
missing answers, dotted-directory types, copy failures, unsafe links and a real
restart/stop between state inspection and main archive transfer. Hook failures
may terminate the trial without a verifier assessment; that is an invalid attempt,
not a scored model failure. Do not synthesize a reward to fill the missing result.

The external Argo/GKE runner must integrate an independently validated equivalent
control path and its own explicitly approved policy. Metadata declarations alone
do not grant support, and changing the approved hash just to admit an unvalidated
adapter is not a valid integration. Until that work is validated, this revision is
not admitted for that production execution path.
