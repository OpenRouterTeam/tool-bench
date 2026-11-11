# Shopify publication companion

This repository-owned MCP companion adds immediate publication discovery and
product publication to the configured Shopify endpoint. It is **not** the unchanged
upstream `shopify-mcp` package.

- The original fourteen tools are served by the installed `shopify-mcp` 1.0.8
  child process. Their definitions, argument validation/defaults and execution
  remain upstream behavior under the catalog's static-token configuration.
- `get-publications` and `publish-product` use the actual Admin GraphQL
  `publications` and `publishablePublish` operations.
- The companion announces version `1.0.8-publications.1`. Its source is recorded
  in the catalog and is part of the tools image; snapshots must be captured from
  the actual composed server, never fabricated by appending schemas offline.
- It reuses the MCP SDK already installed under the locked npm dependency tree.
  No operator home configuration or client credentials are imported. This
  companion currently supports the catalog's explicit static-token mode only.
- Cancellation is forwarded to the child and to new HTTP requests. GraphQL,
  transport and user errors cannot be returned as successful empty publications.
- The new client exposes immediate product publication only. Collection
  publication, scheduling, unpublishing and general GraphQL execution are not
  advertised capabilities.

`tests.mjs` checks API binding/authentication, cancellation, argument validation,
error handling, actual stdio composition and equality of all fourteen original
tool definitions. Integration validation must also cover the mock and the real
MCP-to-API path before removing a task's capability fault.

The ordinary tools Dockerfile includes this companion. For a local candidate,
`images/tools/Dockerfile.publications` layers only the new catalog, companion and
captured snapshots onto an explicitly recorded existing tools image. Such a
development overlay does not establish validation of a fresh full-image rebuild.
