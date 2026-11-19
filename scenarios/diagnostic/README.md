# Diagnostic scenarios

Unscored. These never enter the scored denominator, `tasks/`, or a release
manifest. They run through the real loader, world and grader.

- `stripe/` exercises the no-action and clarification families; see
  `docs/families-clarification-release.md`. Both materialize from the shared
  `scenarios/_organizations/synth-ops/base.json` organization base plus their
  own `overlay.json`.
- `multi/*-any-search` holds refined variants of five scored search lookups,
  kept here so the 150 scored tasks and the release manifest stay as they
  are; see `docs/research/2026-11-19-search-task-review.md`.

`taskgen --include-examples` writes these beside the scored tasks.
