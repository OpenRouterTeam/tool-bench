# Organization bases and task overlays

This is a source implementation with consumer-path tests, not a qualified
container image, model campaign, production release, or family split.
Existing scored scenarios and their instructions are unchanged.

## Authoring

An optional top-level declaration selects materialization instead of a
standalone `dataset/` directory:

```toml
[materialization]
schema_version = 1
base = "_organizations/synth-ops/base.json"
overlay = "overlay.json"
```

The base path is relative to the scenarios source tree; the overlay path is
relative to the declaring scenario. The scenario ID must match its
`group/service/task` source path. Both paths must be canonical relative JSON
paths. Absolute paths, traversal, ambiguous separators, links, special files,
and simultaneous standalone data are rejected. Materialized scenarios require
a version-two task contract.

The base format is:

```json
{
  "schema_version": 1,
  "id": "synth-ops",
  "revision": 1,
  "description": "Fictional shared organization",
  "datasets": {
    "github": {
      "issues": [
        {"_key": "octo-org/app#1", "number": 1, "title": "An existing issue"}
      ]
    }
  }
}
```

The overlay names that exact base and revision:

```json
{
  "schema_version": 1,
  "task_id": "example/github/materialized",
  "base_id": "synth-ops",
  "base_revision": 1,
  "removes": {},
  "inserts": {
    "github": {
      "issues": [
        {"_key": "octo-org/app#1", "number": 1, "title": "A task-specific issue"}
      ]
    }
  }
}
```

Records use the standalone loader's `_key`, then `id`, identity convention.
An overlay replacement is explicit and preserves an existing record's
insertion position. New records append in authored order. Removes must name
existing records. Duplicate document identities within a base or overlay,
duplicate JSON object members, unknown fields, unsupported versions, and
precision-losing numeric literals fail loading.

## Actual consumers

`Dataset::load` reads the base and overlay once, materializes independent
stores, and returns a source manifest plus an inspectable private bundle.
`WorldState::from_dataset` clones those stores for each trial. Normal service
initialization can add defaults; the authored render is not falsely asserted
to equal the initialized world. The existing sealed initial-state evidence
still records that runtime state.

The manifest records semantic base/overlay hashes, exact input byte hashes,
the ordered complete render hash, and ordered collection hashes. Record order
is retained because pagination can depend on it. A whitespace-only shared
base edit changes every dependent dataset identity even when the rendered
state is unchanged. Standalone datasets retain their historical digest and
omit materialization fields.

Task generation writes four files only into the private verifier context:

- `tests/materialization/base.json`
- `tests/materialization/overlay.json`
- `tests/materialization/manifest.json`
- `tests/materialization/dataset.json`

`task-context.json` binds all four exact file hashes; `task-source.json` binds
the manifest. None is copied into the agent environment or reference program.
The runner checks the complete file inventory and bytes before execution.

The offline verifier uses the same core replay validator as the report
loader. It requires the private context for materialized evidence, replays
the original inputs, checks the render, and copies successful inputs into its
output. Missing, changed, linked, or rehashed-but-inconsistent renders are
environment faults, not model task failures. The report independently repeats
those checks and includes the bundle hashes in content-review bindings.

The existing world Dockerfile copies the whole scenarios tree, including
shared bases. The existing world image content hash covers that tree.
Consumer tests exercise loading under the image-style path and show that
base and overlay edits change the actual image tag function. They do not
prove that an image was built, published, or admitted.

## Validation and limits

Focused tests cover the real dataset loader, generated private context,
offline verifier executable, report reproduction, runner pre-execution check,
world reset/isolation, and image source identity. They include altered raw
inputs, missing files, links, duplicate keys, unsafe paths, source changes
shared by two tasks, and malicious renders with recomputed artifact hashes.

This does not introduce scored family assignments, validated clarification or
no-action tasks, a user simulator, frozen release admission, new profile
policies, or held-out governance. Those improvement-plan requirements remain
open. No existing scored task is bulk-migrated to a shared base by this slice.
