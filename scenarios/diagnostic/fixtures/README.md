# Diagnostic fixtures

Unscored diagnostic scenarios for testing the materialization, family,
clarification, and release-design modules. These use a shared synthetic
organization base ("synth-ops") with task-specific overlays.

These fixtures are NOT counted in the scored task denominator.
They exist to prove materialized business differences through executable
interfaces and to pilot the no-action and clarification difficulty families.

## Fixture inventory

- `no-action-already-refunded/` — No-action family: charge was already refunded.
- `clarification-ambiguous-order/` — Clarification family: two similar orders, agent must ask.

Both share the `synth-ops` organization base defined in `base.json`.
