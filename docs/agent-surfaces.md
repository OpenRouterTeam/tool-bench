# Agent-visible surfaces and account origins

Status: source/local, both-account MCP surface, regression and retained-evidence
checks pass on the declared condition. This is an exposure audit,
not a behavioral model-awareness experiment or certified blinding.

## Concrete defect and repair

GitLab's configured `GITLAB_API_URL` previously named `http://world:8107/api/v4`.
Its authentic resource renderers use the request origin for `web_url` and other
links, so actual MCP results disclosed that transport hostname. Scrubbing text
afterward would obscure evidence and risk changing legitimate content.

The new declared connection uses ordinary HTTPS account names:

- Northwind fixtures: `https://gitlab.northwind.internal`.
- The cross-service Lumen fixture: `https://gitlab.lumen.internal`, consistent
  with its Argo CD repository references.

The initial instance descriptor's `public_origin` selects an account connection,
not task-specific target hints. Catalog aliases and the world's private TLS
routes cover both names. The tools process trusts the world's local CA; the main
container still has no backend-network access. Responses and functional
pagination links derive naturally from the configured request, without vendor
definition rewriting or blanket replacement of user text.

## Configuration interface

Endpoints explicitly opt into `runtime_origin_env`. Only those existing
HTTPS connection variables may be overridden at launch. Overrides must use a
declared service alias, retain their exact API path, omit URL credentials,
query/fragment/control characters and keep REST/GraphQL on the same account.
Credentials and deterministic hash seeds cannot be replaced through this interface.

`connections::materialize` builds private connection metadata and the tools'
ordinary SDK environment variables. It writes `tests/connections.json`, renders
the tools environment in Compose, and binds expected value hashes in
`TaskContext.connector_env_sha256`. User instructions and the main workspace
remain unchanged. Invalid instance origins fail authoring rather than silently
falling back.

The host independently reads the actual tools container's declared connection
variables, not only the defaults baked into its image. Missing, changed or
duplicate values invalidate admission. Manifests retain only hashes, and the
offline verifier requires observed values to match the protected context.
Actual captured GitLab request authorities/schemes provide the additional
client-use evidence for the tested condition.
`deploy/test-connector-observation.sh` deliberately gives the host a validly shaped
but incorrect declared fingerprint. The actual container observation rejects it
before agent execution/authoritative collection; no scored assessment is produced.

This is **not** full child-process memory/configuration attestation or complete
runtime-environment coverage for every service. Existing general image
configuration fingerprints still describe image defaults. Broader resolved
provider/harness/verifier/runtime/hardware capture remains open.

## Routing and content boundaries

- Authority-only requests retain their account hostname downstream. The earlier
  TLS dispatcher could select a route by URI authority but leave renderers using
  `localhost`; that alternative path now has a regression.
- The TLS dispatcher determines HTTPS, overriding untrusted forwarded-scheme
  input on that path.
- Unknown TLS authorities return a neutral 421 response, not an evaluator-owned
  “mock service” message or reflected host detail.
- Pagination continues to use routable account origins and preserves filters.
- Gravatar links remain authentic vendor-shaped external links. They are not
  forced to the GitLab account host.
- Authored business text containing an internal-looking URL is preserved exactly.
  The repair is not a blacklist for words such as “world”, “test” or “mock”.

## Actual-client capture

`deploy/test-gitlab-surfaces.sh` retains initialization bodies/headers, the exact
tool definitions and ordinary project/user/pagination results for both accounts,
alongside authoritative world request evidence and runtime connection hashes.
The probe also completes each original lookup task, rather than treating an
unscored HTTP success as its only end-to-end check.

Tool definitions are compared with the pinned snapshot, and server identity
remains the upstream `zereight-gitlab-mcp-server` version 2.3.16. Initialization
guidance is retained verbatim; useful authentic vendor workflow instructions
must not be removed to make a surface look more neutral.

## Remaining audit scope

- The captured proxy session header still uses a `tb-...` identifier pattern.
  Do not ignore or silently relabel that observation as proof of unawareness.
- Initialization instructions are a real information surface. Their delivery and
  hashing across native/universal/upfront exposure need resolved-harness auditing;
  tool-definition parity alone does not establish matched information.
- Other services still have host-derived URL/error surfaces. This GitLab slice
  does not certify all 32 endpoints, network failures, logs, process metadata,
  environment inspection or account/permission responses.
- Actual native prompts, resolved configurations, system date context and agent
  workspaces need private end-to-end inspection. No native model experiment is
  implied by scripted captures.
- Neutral/evaluation framing sensitivity requires prespecified matched runs and
  behavioral outcomes, not a claim that a model cannot recognize evaluation.

## Reproduction boundaries

The earlier signed-review condition retains its matching compiled coordinator
and reporter under `/tmp/tool-bench-content-review.qHCVEA/checker-v1`, with
checksums and Darwin arm64 platform metadata. That frozen coordinator has
successfully re-verified a prior signed case after current source changes.
It is a local/platform-bound reproduction artifact, not a portable release.
The prior derived comparison has also reproduced its original release digest
after current workspace changes, using that matching frozen reporter.

Future content-review campaigns use frozen coordinator binaries throughout and
retain their fingerprinted input sources. They do not depend on a mutable shared
debug binary or the latest workspace inventory when interpreting old signatures.
Private fixture keys are still removed after signing. Full portable source/
toolchain archiving and final release discipline remain open.
