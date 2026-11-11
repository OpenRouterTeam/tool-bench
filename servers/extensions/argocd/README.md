# Argo CD Application-spec companion

This repository-owned companion is an explicit candidate connector improvement,
not unchanged `argocd-mcp`. Its candidate catalog integration is not a qualified
scored release yet.

The pinned upstream `argocd-mcp` 0.9.0 full-Application update schema cannot
represent existing labels, annotations or finalizers. Upstream Argo CD's full
update replaces those fields, so silently exempting their loss from preservation
would weaken the business contract.

The companion keeps all sixteen upstream definitions and adds
`update_application_spec`. It uses the actual upstream
`PUT /api/v1/applications/{name}/spec` operation: the body is the complete spec,
and the Application namespace is the `appNamespace` query parameter. The server
retains the existing Application metadata. This is not a merge patch; independent
spec fields still need preservation. Acceptance does not prove rollout.

The existing `sync_application` definition remains unchanged, but its execution
explicitly repairs the published client's wire/result bug: operation sync options
are encoded as the protobuf message `{ "items": [...] }`, not a bare array;
Application namespace belongs in the request body; and HTTP failures become
root MCP errors. It does not invent a new sync operation or treat acceptance as
completion.

The other native operations use an explicitly hardened fetch transport.
Non-2xx API responses throw instead of being wrapped as MCP successes. Legacy
full-Application updates read the current Application and preserve metadata and
spec fields that their subset schema cannot represent. Representable optional
fields still follow full-update omission semantics. This is a client repair,
not a backend metadata exception, atomic compare-and-swap or exactly-once guarantee.

The composed server announces `0.9.0-operations.2`. It uses the already locked MCP
TypeScript SDK, not a newly installed package or an operator's home environment.
The two companion-handled operations use the configured default origin and API credential only;
an equivalent explicit default origin is accepted, but a different per-call
origin cannot receive that credential. Original tools retain the upstream
credential-registry behavior.

The companion operations reject HTTP failures, incomplete/non-JSON responses,
redirects and invalid arguments, and propagates cancellation. Its response
uncertainty requires read-before-retry reconciliation. It does not add a general
HTTP, patch, rollback or Kubernetes tool.

Before admission: qualify the actual composed stdio surface, implement and test
the matching bounded backend operation, recapture the actual seventeen-tool
snapshot, declare a new catalog/tools/world condition, and validate positive,
near-miss, provenance, lifecycle and preservation cases. Never append a schema
to the captured snapshot offline or describe this as the original sixteen-tool
condition.

Primary operation source: Argo CD comparison baseline `v3.2.5`,
commit `c56f4400f22c7e9fe9c5c12b85576b74369fb6b8`,
`server/application/application.proto` (`UpdateSpec`, body binding `spec`) and
`server/application/application.go` (`UpdateSpec` loads the current Application
before replacing its spec). See
`docs/history/research/2026-11-07-argocd-operation-contract.md` for pins, upstream references
and unresolved vendor/controller guarantees.
