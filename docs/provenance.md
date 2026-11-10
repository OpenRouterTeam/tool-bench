# Provenance and licences

Repository-authored instructions, datasets, expectations and mock code are written
by OpenRouter and released under this repository's MIT licence (`LICENSE`).
The upstream material listed below retains its own licence and notices.
No customer data or real account data is included: names, IDs, tokens and
records in the datasets are invented.

Upstream material vendored into the mocks or the tools image:

| Path | Source | Licence |
|---|---|---|
| `crates/services/cloudflare/assets/spec.json` | Trimmed from [cloudflare/mcp](https://github.com/cloudflare/mcp) `src/generated/spec/spec.json.txt` at `68c7ec0`, itself generated from [cloudflare/api-schemas](https://github.com/cloudflare/api-schemas) (`trim-spec.py`) | BSD-3-Clause (api-schemas) |
| `crates/services/stripe/assets/operations.json`, `objects.json` | Built from [Kiln-AI/stripe_world](https://github.com/Kiln-AI/stripe_world) and Stripe's [OpenAPI spec](https://github.com/stripe/openapi) (`build_assets.py`) | MIT (both) |
| `crates/services/aws/assets/service-reference*` | Copies of the public [AWS service reference](https://docs.aws.amazon.com/service-authorization/latest/reference/service-reference.html) documents served at `servicereference.us-east-1.amazonaws.com` | AWS documentation; no licence file upstream |
| `crates/services/supabase/assets/native-introspection-v0_15_0.json` (`tables_prefix`, `extensions_query`; used by `crates/services/supabase/src/native_queries.rs`) | Verbatim SQL text extracted from the published npm package [`@supabase/mcp-server-supabase`](https://www.npmjs.com/package/@supabase/mcp-server-supabase) 0.15.0 (tarball sha512 `+Zhu0iKj…jkCGQ==`; repo [supabase/mcp](https://github.com/supabase/mcp) tag `mcp-server-supabase-v0.15.0` = commit `0edc736f2568`). Source files `src/pg-meta/{tables,columns,extensions}.sql`, read from the package's `dist/chunk-RARMLKMM.js.map` (sha256 `c2068194…279fd3`, recorded in the JSON as `source_map_sha256`; `runtime_sha256` is `dist/chunk-RARMLKMM.js`). Those queries are in turn adapted by Supabase from [supabase/postgres-meta](https://github.com/supabase/postgres-meta) (`src/lib/sql/table.sql.ts`, `columns.sql.ts`, `extensions.sql.ts`; no commit pin recorded). | Apache-2.0, Copyright 2025 Supabase (supabase/mcp `LICENSE`); postgres-meta: Apache-2.0 (see `THIRD_PARTY_NOTICES.md` for a discrepancy). Neither upstream ships a `NOTICE` file. |
| `servers/snapshots/google-workspace.json` (also the schema source for the `crates/services/google` replica; no separate Google schema or discovery document is vendored) | Tool list captured from [taylorwilsdon/google_workspace_mcp](https://github.com/taylorwilsdon/google_workspace_mcp) (PyPI `workspace-mcp`) 2.2.1 | MIT, Copyright (c) 2025 Taylor Wilsdon |
| `servers/snapshots/argocd.json` | Tool list captured from [argoproj-labs/mcp-for-argocd](https://github.com/argoproj-labs/mcp-for-argocd) (npm `argocd-mcp`) 0.9.0; the `update_application_spec` entry is written for this repository | Apache-2.0, Copyright 2025 Argoproj Contributors |
| `servers/snapshots/shopify.json` | Tool list captured from [GeLi2001/shopify-mcp](https://github.com/GeLi2001/shopify-mcp) (npm `shopify-mcp`) 1.0.8; the `get-publications` and `publish-product` entries are written for this repository | MIT, Copyright (c) 2023 Shopify MCP Server Contributors |
| `servers/snapshots/*.json` (all others) | Tool lists captured from each pinned MCP server in `catalog/services.toml` (`deploy/capture-stack.sh`) | Each server's own licence (see its pin in the catalog) |
| `servers/extensions/argocd`, `servers/extensions/shopify` | Original companion code and tool descriptions written for this benchmark. They wrap the installed upstream package at runtime and do not copy upstream source or tool descriptions | MIT (this repository) |
| `servers/npm`, `servers/python` | Lockfiles for the pinned MCP servers; the packages are installed into the tools image at build time, not vendored | Each package's own licence |

`native-introspection-v0_15_0.json` is a modified copy: the SQL strings are unchanged, but the file wraps them in repository-authored
metadata (package, version, hashes, scope), and `tables_prefix` combines the package's table and column queries into one string (the JSON's `compiled_and_source_generators_match` records that the shipped generator and the source-map generator agree). `system_schemas` is the list in the package's `src/pg-meta/index.ts`. The 0.15.0 package's SQL text was compared with the JSON on 2026-11-09 (`extensions_query` equals `extensions.sql`; the `tables.sql` and `columns.sql` text appears inside `tables_prefix`).
The JSON also carries its own `modification_notice`. The complete npm tarball
integrity is pinned in `servers/npm/package-lock.json`:
`sha512-+Zhu0iKjgKfN1gJlGuaQmzjiXvlHtN+LHMj22piHgErC/miWsre+TmLtCH2AVse+n0xB2eehyHhhFQOJbjkCGQ==`.
The complete Supabase MCP source commit is
`0edc736f256871b1d539a320def43a001bd8c7a8`.

This file records sources and licences for maintenance. It is not legal advice or a licence determination.

Upstream licence texts are reproduced in `THIRD_PARTY_NOTICES.md`.
