# Historical local image overlays

These Dockerfiles retain diagnostic build recipes used during the overhaul.
They are not used by the release workflow, are not inputs to the canonical image
tags, and do not qualify any published image. Build contexts, base-image
digests and the resulting binaries must be recorded separately for a new trial.

- `Dockerfile.argocd` overlays catalog, captured schemas and connector extensions
  onto an explicitly supplied `BASE_TOOLS_IMAGE`.
- `Dockerfile.connections` rebuilds the catalog/supervisor over a historical
  local publication candidate. Its default base is deliberately retained for
  provenance, not recommended as a current release.

Use `images/tools/Dockerfile` and `images/world/Dockerfile` for release builds.
