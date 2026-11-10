# Source and execution provenance

## Current admission scope

Evaluator 15, evidence schema 5, `offline-world-evidence-v5`, and
`tool-bench-runner-v3` add source-bound local scripted execution. This admits
calibration with the exact host `OracleAgent` and `NopAgent`, not native/model
benchmark comparisons. Unverified native producers are rejected before setup;
trusted native runtime/completion and isolated product-condition support remain
required full-overhaul work.

Do not describe a source-bound scripted comparison as a model result, a complete
realized native manifest, a production admission certificate, or proof of
unawareness.

## World-to-contract chain

The dataset loader parses and fingerprints the same bytes it reads:

- `scenario_sha256` binds the raw `scenario.toml`, including its authored context.
- `dataset_sha256` binds the sorted service-directory and collection-file paths
  and exact file bytes, independent of the checkout's absolute path.
- `contract_sha256` binds the canonical decoded contract. Object key order is
  normalized; JSON types and array order are not silently changed.
- `initial_state_sha256` separately binds the actual initialized baseline,
  including provider defaults and effective clock state.

The world retains this descriptor independently of a verifier request.
Evidence refuses a substituted contract even if the new rubric is easier and
would otherwise pass. `tests/task-source.json` also binds the materialized task's
expected scenario/data/contract sources; mismatched world artifacts are setup
faults, not model failures.

The authoring path rejects in-memory contract changes that have not been authored
into the matching scenario source. Deliberate tracks need corresponding source
variants and world materialization, not a verifier-only rubric override.

## Protected execution observation

The approved host controller records `realized-manifest.json` outside every
agent-mounted directory. It observes execution at `AGENT_START`, after the
supported scripted setup and before agent execution. Harbor's event named
`ENVIRONMENT_START` occurs before containers start and is not used as evidence
that an image exists.

The manifest records:

- Actual container and immutable image IDs for main, world and tools, requested
  sidecar references and observed CPU/memory limits.
- Host Harbor version and source inventory fingerprint, Python version, actual
  scripted agent implementation and the oracle program's file inventory.
- Resolved user-instruction hash, profile/framing descriptors, materialized
  environment file inventory and resolved execution/verifier timeouts.

Images, containers and resource observations are rechecked before publication.
The manifest is delivered through the world-only artifact channel and its exact
bytes are bound to the runner receipt, session and stopped main identity.
Extra instructions, unapproved scripted settings and native producer names cannot
masquerade as that approved execution.

### Main-image equivalence

Harbor/Compose builds per-trial main images. Real validation found different
image IDs caused solely by `com.docker.compose.project`, even when root filesystem
layers and runtime defaults matched.

The raw IDs and project labels remain recorded. Main-image compatibility instead
requires exact root-filesystem and image-runtime-default fingerprints after
removing **only that orchestration project label**. Other labels, user, environment,
entrypoint, workdir and other configuration remain fingerprinted. Component hashes
aid diagnosis without exporting raw environment values. World/tools still require
the same immutable image IDs; this is not generic image equivalence.

These are observed image defaults and limits, not authentication of a native
process's accepted prompt, provider settings or final reply. Full native/runtime
and hardware-condition accounting remains open.

## Comparison admission

Retain the whole trial export, including `artifacts/world/evidence.json`,
the collected answer when present, and the separate verifier's exact input copies.
`verification.json` identifies those raw bytes; its evidence hash does not refer
to a reserialized diagnostic copy.

The report loader:

1. Discovers host trial roots without descending into agent, artifact, verifier
   or snapshot directories. Agent-written nested `result.json` files are not trials.
   Source discovery rejects symbolic links and malformed task identities.
2. Checks all bound input hashes, task/source identities and runner/manifest binding.
3. Reproduces the assessment from those inputs and requires saved assessments
   and reported host metrics to agree.
4. Requires one frozen evaluator/world/tools/runner release, homogeneous repeats,
   identical task inventories and repeat counts, and compatible task contexts,
   sources, initialized state, resources and budgets.

Scalar historical results remain diagnostically readable, but missing/old
provenance cannot establish a final verified comparison. Invalid setup, contract,
unknown/missing assessment, host exception, provisional unresolved result or
inconsistent metrics cannot become a measured model failure or silently disappear
from the denominator.

The only currently supported declared cross-arm difference is agent identity:

```sh
cargo run --locked -p report --bin bench-report -- run-a run-b --json --allow-agent-difference
```

It is recorded in JSON and Markdown and does not permit other condition changes.
It cannot hide heterogeneous agents within an arm. Outputs explicitly label
`admission_level = "source_bound_scripted_only"` and record the release fingerprint.
The CLI declaration does **not** authenticate preregistration: prospective design
binding, replacement policy and complete treatment/matched-information admission
remain release requirements. Do not use the flag to retrofit an experiment.

Hashes bind bytes; they are not signatures. Trust comes from the protected host,
approved collector and private source policy, service-scoped collection and
separate verification—not a plausible filename or an agent-authored manifest.
