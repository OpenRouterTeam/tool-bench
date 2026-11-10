# Cloudflare operation-contract research (2026-11-06)

## Scope and evidence

Read-only research for these unchanged user instructions:

- [`add-status-cname`](../../scenarios/cloud/cloudflare/add-status-cname/instruction.md): create the DNS-only status CNAME; preserve the old status record and everything else.
- [`enable-checkout-flag`](../../scenarios/cloud/cloudflare/enable-checkout-flag/instruction.md): change only `enabled` and `rollout` in the production flag's JSON object; preserve the other fields, staging, and preview.
- [`route-orders-api-v2`](../../scenarios/cloud/cloudflare/route-orders-api-v2/instruction.md): route `api.octo-corp.com/v2/*` to `orders-api-v2`; preserve other production traffic and staging.

Cloudflare product/API documentation was checked directly on November 6, 2026,
including the operation references rather than relying on the initial research
summary. Versioned, first-party RFC Editor specifications supply only the JSON
interoperability and URI-fragment rules. No accounts, benchmarks, or deployment
operations were used.

**Evidence labels:** **Documented** means an explicit source statement;
**inference** means a conclusion from those statements; **unverified** means the
reviewed sources do not settle it. Benchmark recommendations below are policy
implications, not additional Cloudflare promises.

## 1. Workers KV: replacement, preservation, and JSON ambiguity

### REST write contracts

| Operation | Verified contract |
| --- | --- |
| Single-key `PUT .../values/{key_name}`, raw `application/octet-stream` | **Documented:** creates or replaces the value, expiration, and metadata. Omitting both expiration parameters makes the key non-expiring; `expiration_ttl` wins if both are present. Consequently, a value-only overwrite does not implicitly retain existing metadata. [K1] |
| Same single-key `PUT`, `multipart/form-data` | **Documented:** `value` plus optional JSON `metadata` parts. It has the same replacement contract, not PATCH semantics. Omitting metadata is not a preservation mechanism. [K1] |
| Bulk `PUT .../bulk`, JSON array | **Documented:** overwrites existing values and expirations; omitted expiration parameters mean no expiration. Each item supports `metadata`. **Unverified:** the bulk description does not explicitly state what happens to existing metadata when that member is omitted. [K2] |

**Important boundary:** treating bulk metadata omission as replacement is a
reasonable **inference** from the single-key contract, but the reviewed bulk
reference alone does not prove it. Neither “bulk preserves omitted metadata”
nor “bulk definitely clears it” should be presented as independently documented
production behavior. Explicitly carrying metadata avoids that uncertainty. [K1][K2]

Bulk responses report `successful_key_count` and `unsuccessful_keys`; do not infer
that bulk KV writes have the all-or-nothing DNS batch contract. [K2][D4]

### Explicitly retaining ancillary state

For a read-modify-write that must preserve KV metadata and expiry:

1. Obtain the old metadata with `GET .../metadata/{key_name}`, or list the exact
   key; the keys listing includes metadata and the absolute expiration when
   present. Listing is paginated and does not return values. [K3][K4]
2. Resend the complete old metadata as the multipart JSON `metadata` part, or
   the bulk item's `metadata` member. Do not put it inside the flag's value
   JSON by accident: these are separate API fields. [K1][K2]
3. Resend the old absolute `expiration` when preserving the original deadline.
   **Inference:** reusing the original relative TTL instead would move the
   deadline. A non-expiring baseline needs neither expiration parameter.
   The binding API uses `expirationTtl`, whereas REST uses `expiration_ttl`.
   Expiration targets less than 60 seconds away are unsupported. [K1][K4][K5]

The binding equivalent is `put(key, newValue, { metadata, expiration })`;
`getWithMetadata()` obtains the value and metadata. This is not a promise of
an atomic read-modify-write: the guide warns that competing writes overwrite
one another and remote readers may observe cached values. [K5][K6]

### Duplicate JSON members

**Documented interoperability limit:** RFC 8259 §4 recommends unique object names,
but duplicate names are not categorically excluded by its grammar. Receivers may
keep the last member, reject the object, or expose multiple members. Universal
last-wins behavior is therefore not guaranteed. Compare decoded names, not just
their literal escape spelling, when checking equality. [J1][J2]

KV's single-write value is a byte sequence, not a documented JSON-object merge.
Acceptance of bytes containing duplicate `enabled` or `rollout` members proves
neither the storefront's interpretation nor preservation of the original fields.
No duplicate-member resolution guarantee was found in the reviewed KV operation
references. Outer bulk JSON and the JSON string stored as its `value` are also
distinct parsing boundaries. [K1][K2][J1]

**Benchmark implications:** check the resulting JSON object and unchanged fields,
not merely a successful write or a parser's last-wins projection. Conservatively
rejecting ambiguous duplicate decoded member names is a defensible benchmark
policy, but not a claim that Cloudflare rejects those value bytes. Separately
model ancillary KV metadata/expiry replacement; the task's “other fields” refers
explicitly to the JSON object, so any broader preservation rule must be identified
as benchmark policy rather than silently added to the user's text. [J1][K1]

## 2. DNS: CNAME validity, TTL, and batch atomicity

### Creation and CNAME constraints

- **Documented:** `POST /zones/{zone_id}/dns_records` creates a record.
  CNAME fields include `name`, `type: "CNAME"`, `content`, `ttl`, and `proxied`.
  The target must be a valid hostname and must not equal the record's own name;
  it is not an arbitrary URL with a scheme/path. Use explicit `proxied: false`
  for the requested DNS-only record. [D1][D2]
- **Documented:** a new CNAME cannot share its owner name with an existing
  A/AAAA or CNAME record. The restriction is not limited to identical targets:
  another CNAME with a different target is not an allowed same-name alternative.
  Creation is not a documented conflict-resolving upsert. [D3][D1]
- **Documented:** names are represented in Punycode in the API. That statement
  does not establish every case-folding/trailing-dot normalization detail, nor
  a deterministic identifier for a newly created record. [D1]

For the requested record, the intended body is structurally
`{"type":"CNAME","name":"status.octo-corp.com","content":"octocorp.statuspage.io","proxied":false,"ttl":1}`.
This is a proposed valid request, not evidence that it was submitted. [D1]

### TTL is plan-dependent

**Documented:** DNS-only custom TTLs range from **60–86400 seconds** on
non-Enterprise zones, or **30–86400 seconds** on Enterprise zones. API `ttl: 1`
means **Auto**, not a one-second TTL; the DNS guide describes Auto as 300 seconds.
Proxied-record TTL is a separate, fixed-Auto case. [D1][D5]

**Benchmark implication:** rejecting every `ttl: 30` is wrong without a
non-Enterprise plan assumption; accepting it on every plan is also wrong.
The status-page instruction specifies no TTL, so do not require one arbitrary
numeric custom TTL instead of permitting Auto. [D5]

### Batch rollback is not propagation atomicity

**Documented:** `POST .../dns_records/batch` executes categories in the order
`deletes → patches → puts → posts`, preserving order within each category.
If an action fails, no changes are applied and the first error is returned.
The database transaction is atomic, but distributed propagation of individual
record changes is not. A successful batch therefore does not promise a globally
simultaneous DNS cutover. [D4][D6]

| Operation | Unspecified fields |
| --- | --- |
| `PATCH .../dns_records/{id}` / batch `patches` | Remain unchanged; specify the fields being updated. [D4][D7] |
| `PUT .../dns_records/{id}` / batch `puts` | Overwrite: unspecified fields take defaults. Batch puts require `id`, `content`, `name`, and `type`. [D4][D8] |
| Batch `posts` | Create records; no existing record ID is required. [D4] |

**Benchmark implications:** validate the entire batch and its resulting state,
not just one requested post. A later invalid action must not leave earlier
changes behind. Conversely, an entirely valid batch remains a valid way to
create the CNAME. Preserve the old status record and unrelated records; do not
equate PUT omission with preservation or hard-code the new record's generated
ID. Account/zone scope, conflicts, target validity, TTL, and proxy status matter
even when a mock returns success. [D1][D3][D4][D5]

## 3. Worker routes: matching is not storage identity

### Documented matching rules

- **Creation-date boundary:** routes created **on or after October 15, 2023**
  match host components case-insensitively; older routes are documented as
  host-case-sensitive. Paths remain case-sensitive. This is a route-creation
  boundary, not a stated Worker compatibility-date switch. [R1]
- **Schemes:** `http://` covers HTTP only; `https://` covers HTTPS only;
  omitting the scheme covers both. [R2]
- **Specificity:** the most specific matching route wins; a more-specific route
  without a script can negate a broader route. [R1]
- **Grammar:** `*` is the only operator. Host wildcards belong at the beginning;
  path wildcards at the end. Infix wildcards such as `/*.jpg` and literal query
  parameters are invalid. `*://...` is not a documented scheme form. [R1][R2]
- **Query matching:** matching includes the query string, so a trailing `*`
  is necessary to cover query-bearing URLs. `*host` and `*.host` also have
  different coverage; do not broaden the requested exact hostname. [R1][R2]

**Documented caveat:** the known-issues page shows `/images*` unexpectedly taking
precedence over `/images/*`, including for `/images/hello`. Do not invent an
unqualified longest-string specificity algorithm for every wildcard combination.
The ordinary `/v2/*` versus `/*` case is supported by the route guide's example,
but broader competing patterns need separate consideration. [R3][R1]

### Fragments and duplicate equivalent hosts: evidence limits

**Fragments:** URI fragments are separated before dereferencing and are not
server-side request selectors. A literal `#fragment` is not a documented Workers
route selector. **Unverified:** the reviewed route guide and create/replace
references do not specify whether every fragment-bearing input is rejected,
stripped, or stored. Do not assert a particular HTTP error, or accept a
fragment-qualified mock route as proof of requested traffic coverage.
Percent-encoded path data is not the same thing as a URI fragment. [U1][R2][R4][R5]

**Equivalent hosts:** the documented equivalence of new `API.octo-corp.com/v2/*`
and `api.octo-corp.com/v2/*` is a matching statement. The create API documents
`POST .../workers/routes`, `pattern`, optional `script`, and a returned ID;
replacement uses `PUT .../workers/routes/{route_id}`. **Unverified:** these sources
do not settle whether a second equivalent-host POST conflicts, coalesces,
updates, or creates another stored row. They do not promise lowercase stored
patterns, a canonical returned spelling, deterministic/reused IDs, or tie-breaking
between duplicate equivalent matches. Do not infer these from matching
equivalence or a mock's behavior. [R1][R4][R5]

### Benchmark implications for the orders route

Treat these as derived acceptance guidance, not new user instructions:

- Evaluate coverage and Worker selection, not literal pattern spelling alone.
  An unschemed route covers both protocols; HTTPS-only does not. Two separately
  valid protocol-specific routes can provide the same coverage if unrelated
  state remains preserved. Host case can be equivalent; changing `/v2/` to
  `/V2/` cannot be assumed equivalent. [R1][R2]
- Retain the current fallback route instead of replacing it with the new Worker;
  check `/v1` and other production paths still select `orders-api`. Avoid treating
  `/v2*` as interchangeable with `/v2/*`: it broadens the requested prefix. [R1][R2]
- Validate supported patterns and operation outcomes before granting semantic
  equivalence. A stored regex-like pattern, ambiguous duplicate route, or
  fragment-qualified pattern is not automatically a valid solution.
  Keep staging untouched and do not require a particular newly generated route
  ID or lowercase storage representation. [R1][R2][R4][R5]

## Primary sources

Links identify the precise operation or section used. Cloudflare documentation is
live; the RFC numbers identify immutable specification versions.

- **[K1]** [KV single-key write: operation description, query/body parameters](https://developers.cloudflare.com/api/resources/kv/subresources/namespaces/subresources/values/methods/update/).
- **[K2]** [KV bulk write: description, item fields, success/failure results](https://developers.cloudflare.com/api/resources/kv/subresources/namespaces/methods/bulk_update/).
- **[K3]** [KV get metadata](https://developers.cloudflare.com/api/resources/kv/subresources/namespaces/subresources/metadata/methods/get/).
- **[K4]** [KV list keys: description and `expiration`/`metadata` results](https://developers.cloudflare.com/api/resources/kv/subresources/namespaces/subresources/keys/methods/list/).
- **[K5]** [KV binding writes: parameters, expiration, metadata, concurrent writes](https://developers.cloudflare.com/kv/api/write-key-value-pairs/).
- **[K6]** [KV binding reads: `getWithMetadata()`](https://developers.cloudflare.com/kv/api/read-key-value-pairs/#getwithmetadata-method).
- **[J1]** [RFC 8259 §4: object-name uniqueness and duplicate-member interoperability](https://www.rfc-editor.org/rfc/rfc8259.html#section-4).
- **[J2]** [RFC 8259 §8.3: decoded string comparison](https://www.rfc-editor.org/rfc/rfc8259.html#section-8.3).
- **[D1]** [DNS record creation: POST and CNAME request/response schema](https://developers.cloudflare.com/api/resources/dns/subresources/records/methods/create/).
- **[D2]** [DNS-only proxy status](https://developers.cloudflare.com/dns/proxy-status/#dns-only-records).
- **[D3]** [Same-name record restrictions: Causes](https://developers.cloudflare.com/dns/manage-dns-records/troubleshooting/records-with-same-name/#causes).
- **[D4]** [DNS batch guide: Use the API, operation order, failure behavior, unspecified fields](https://developers.cloudflare.com/dns/manage-dns-records/how-to/batch-record-changes/#use-the-api).
- **[D5]** [DNS TTL: proxied and unproxied records](https://developers.cloudflare.com/dns/manage-dns-records/reference/ttl/).
- **[D6]** [DNS batch API: transaction and propagation notes](https://developers.cloudflare.com/api/resources/dns/subresources/records/methods/batch/).
- **[D7]** [DNS PATCH: Update DNS Record](https://developers.cloudflare.com/api/resources/dns/subresources/records/methods/edit/).
- **[D8]** [DNS PUT: Overwrite DNS Record](https://developers.cloudflare.com/api/resources/dns/subresources/records/methods/update/).
- **[R1]** [Worker routes: Matching behavior](https://developers.cloudflare.com/workers/configuration/routing/routes/#matching-behavior).
- **[R2]** [Worker routes: Validity](https://developers.cloudflare.com/workers/configuration/routing/routes/#validity).
- **[R3]** [Workers known issues: Route specificity](https://developers.cloudflare.com/workers/platform/known-issues/#route-specificity).
- **[R4]** [Worker route creation: request and returned ID/pattern](https://developers.cloudflare.com/api/resources/workers/subresources/routes/methods/create/).
- **[R5]** [Worker route replacement: PUT by route ID](https://developers.cloudflare.com/api/resources/workers/subresources/routes/methods/update/).
- **[U1]** [RFC 3986 §3.5: fragment processing](https://www.rfc-editor.org/rfc/rfc3986.html#section-3.5).
