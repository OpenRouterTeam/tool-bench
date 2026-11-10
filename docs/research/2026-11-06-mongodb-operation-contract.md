# MongoDB operation-contract research — 2026-11-06

## Scope and evidence boundaries

This note researches the three **unchanged** user requests below. Instructions
establish business intent; TOML and narrow fixture projections identify targets
and protected scope, not upstream semantics. No local service/evaluator
implementation was inspected. No dependencies were installed, tests run, or live
Atlas, database, payment, or other cloud-resource operations performed.

| Request | Intended change | Protected scope |
| --- | --- | --- |
| Pause idle staging | Pause `analytics-staging` in project `octo-shop-staging` (`65c0ffee0000000000000002`), organization Octo Retail (`65a1b2c3d4e5f60718293a4b`). The target is an unpaused, `IDLE`, AWS `M10` replica set. | Preserve its configured tier and topology; leave all other clusters unchanged, including the identically named cluster in `octo-payments-staging`, `shop-staging`, and production. [L1][L2][L7] |
| Purge QA accounts | Delete all customers on `@qa.octoshop.test` from **`crm.customers`**. Fixture target ObjectIds end in `0002`, `0005`, `0008`, and `000a`. | Preserve the other seven production documents, including `qa.team@octoshop.com`, `ben@qa.octoshop.tech`, and `lena.qa@gmail.com`; preserve all `crm_staging` data and unrelated collections. [L3][L4][L8] |
| Refund order | In **`shop.orders`**, mark Priya Natarajan's order `10482`, ObjectId `65f1a000000000000000000a`, refunded; set reason `damaged in transit` and numeric amount equal to its `214.5` total. The embedded customer identifies Priya and her customer ObjectId. | Preserve the target's other fields and all other orders, notably Diego's `10428`, Priya's `10517`, and the staging copy of `10482` whose total is `2`. This is an order-record update, **not authorization to initiate a payment transfer**. [L5][L6][L9] |

Evidence labels:

- **Local fact:** observed instruction, TOML, captured schema, or projected fixture.
- **Documented:** explicit official documentation/specification.
- **Versioned implementation:** inspected published package or fixed upstream
  source; not proof of behavior in this repository's replica.
- **Inference/recommendation:** application of that evidence to the request.
- **Uncertain:** not established by the inspected evidence.

### The runtime is a replica, not the upstream MCP server or Atlas

**Local fact:** `catalog/services.toml` declares MongoDB `mode = "replica"` and
endpoint `runtime = "replica"`, with source
`mongodb-mcp-server 3.2.2 (vendor), captured with Atlas API credentials set so Atlas tools register`.
The capture is `servers/snapshots/mongodb.json`; it advertises MongoDB MCP Server
`3.2.2`, protocol `2025-06-18`, and 49 tools. Fixture deployment records advertise
MongoDB `8.0.15`. None of these facts establishes a running upstream server,
a real MongoDB engine, an actual Atlas deployment, or implementation parity.
[L10][L11][L12]

**Published-version verification:** npm lists `mongodb-mcp-server@3.2.2`, published
`2026-10-29T09:26:07.174Z`, with repository
`mongodb-js/mongodb-mcp-server`. Its Atlas and MongoDB tool packages are exact
`3.2.2` dependencies. Both scoped packages publish npm provenance statements
identifying commit `0af7e43bd2c8d0188644b355bbc25da8c7c62d98`; the fixed source
package also declares version `3.2.2`. The two tool tarballs were stream-read
without installation, and their computed SHA-512 values matched their registry
integrity metadata. Registry metadata did not provide a `gitHead`; provenance,
not an assumed tag name, supplies the recorded commit. [P1][P2][P3][P4][P5][P6]

**Limits of this verification:** provenance payloads were decoded, not subjected
to independent signature/certificate verification. This establishes inspectable
published-version evidence, **not** the identity of whatever vendor build
originally produced the capture. No byte-for-byte link between that build and
the snapshot was established. Transitive dependency ranges are not exact runtime
pins: the MongoDB tools declare, for example, `bson: ^7.2.0` and
`mongodb: ^7.1.1`. The replica's actual BSON dependency/runtime is unverified.
[P2][P3][P4][P5][L10][L11]

The scenario clock is `2026-03-14T09:00:00Z`; the research date and published
package date above are separate provenance dates, not claims that the scenario
executed against a real service on either date. MongoDB 8.0 documentation and,
where useful, source tag `r8.0.15` provide the database comparison contract;
rolling Atlas/MCP documentation is not itself a historical implementation pin.
[L2][L4][L6][L12]

## 1. Atlas pause/resume

### Eligibility, desired configuration, and completion are different

**Documented Atlas eligibility:** manual pause supports dedicated `M10+`
clusters without NVMe storage. The cluster must be healthy with disk utilization
below 95%; a recently resumed cluster must run at least 60 minutes before
another pause. Flex cannot be paused. Free-cluster automatic inactivity pause
is a different facility, not permission to invoke this MCP's dedicated-cluster
pause operation against Free. Dedicated pause lasts at most 30 days before
automatic resume. Project Cluster Manager or higher is required. [A1]

**Fixture inference:** the target's `M10` tier and `IDLE` state support the intended
choice, but the projected fixture does not establish every real Atlas health,
disk, permission, or cooldown precondition. Do not turn missing fixture fields
into assertions about actual Atlas eligibility, or invent an additional
requirement to prove the user-provided history of inactivity. [L1][L7][A1]

**Versioned MCP implementation:** the tool sends only
`{ "paused": true }` for `PAUSE` or `{ "paused": false }` for `RESUME` to
`updateCluster`, scoped by project ID and cluster name. Its API client uses
`PATCH /api/atlas/v2/groups/{groupId}/clusters/{clusterName}` with Atlas media
version `2025-03-12`. It neither upgrades the tier nor supplies new
`replicationSpecs`, node counts, disk settings, or autoscaling configuration.
[V1][V2]

**Documented API distinction:** `paused` is a boolean, separate from `stateName`.
`IDLE` describes the control plane having finished requested changes;
`UPDATING` describes an in-progress change. A paused boolean is not a new
instance-size string or a special `PAUSED` member of the documented state enum.
[A2]

**Versioned implementation and inference:** the tool returns wording that the
cluster **is being paused/resumed**, without waiting for transition completion,
testing connectivity, or reconciling an invoice. Therefore evaluate acceptance
of the desired pause separately from eventual operational or billing completion.
Do not require an immediately observed outage, zero accrued bill, or a fabricated
terminal state. If an environment explicitly models asynchronous completion,
inspect its separate completion state; do not call a submitted request proof
that every downstream effect has completed. [V1][A2]

**Documented cost boundary:** paused clusters retain storage charges; compute
savings do not mean a zero bill. Atlas's billing page says paused clusters are
charged for storage, not other services or data transfer. That policy is not an
instantaneous billing-completion guarantee. [A3]

**Preservation inference:** keep the target's configured `M10`, three electable
nodes, AWS region, replication configuration, and autoscaling settings. Preserve
every other cluster's configuration. A pause is not a downgrade, deletion, or
recreation. Preserve configuration, not the fiction that every operational
subsystem remains active: Atlas documents stopped backups/alerts, retained
snapshots until expiry, and Search-node data/index rebuild effects. Those
platform effects do not authorize unrelated configuration edits. [L1][L7][V1][A1]

### Retries are not uniformly idempotent successes

- **Documented Atlas errors:** `CLUSTER_ALREADY_PAUSED` is HTTP 409, as are
  `CANNOT_PAUSE_CLUSTER_WITH_PENDING_CHANGES`, `CANNOT_PAUSE_NVME_CLUSTER`,
  `CANNOT_UPDATE_AND_PAUSE_CLUSTER`, and `CANNOT_UPDATE_PAUSED_CLUSTER`.
  `CANNOT_PAUSE_RECENTLY_RESUMED_CLUSTER` is HTTP 400. Repeated pause must not be
  assumed to return success. [A4]
- **Versioned MCP advertised behavior:** pause requires `IDLE` and reports an
  error if already paused; resume of an unpaused cluster is described as a
  no-op. The implementation itself makes the PATCH without first checking
  paused/state, and propagates API failure. Thus the description is not evidence
  of a client-side guard that synthesizes those outcomes. [V1][V2]
- **Recommendation:** after a timeout or ambiguous result, inspect the identified
  cluster's desired state before retrying. Distinguish “the requested state is
  already satisfied” from “this duplicate call succeeded.” Retry a transient
  unhealthy/pending state only after that condition resolves; a Free/Flex/NVMe
  eligibility failure is not repaired by identical retries. The inspected
  pause tool has no wait/retry loop; injected HTTP transport retry behavior was
  not established. No polling was performed for this research. [A1][A4][V1]

### MCP-managed connections and temporary users are client behavior

**Versioned implementation:** after a successful pause API call, the tool finds
all registry entries whose recorded `atlasCluster.projectId` and `clusterName`
match the target, disconnects them, and reports their IDs in
`disconnectedConnectionIds`. This is broader than the description's older
“current active connection” wording, but narrower than “disconnect every
connection everywhere.” Connections to another cluster/project are not selected.
No analogous disconnection occurs in the resume branch. [V1][V3]

For ordinary dynamically created entries, disconnect removes the entry and runs
its revocation callback. `atlas-connect-cluster` wires that callback to
best-effort deletion of **the temporary user created for that entry**. The user
is cluster-scoped and has a configurable expiration, with a four-hour fallback.
Cleanup failures are caught/logged; deletion success is not guaranteed. This is
MCP lifecycle behavior, not an Atlas PATCH requirement to delete application
users. The fixture's ordinary `analytics-etl` and `shop-api` users are not a
license for arbitrary user deletion. [V3][V4][L7]

**Boundary/uncertainty:** the predicate uses recorded Atlas metadata, not a
universal host-matching scan. A preconfigured connection has special disconnect/
redial handling; no claim is established that every URI-created or preconfigured
connection can be recognized as this cluster. The source performs the Atlas
update before connection cleanup, so a later cleanup error cannot be assumed to
roll the pause back. Resume does not restore revoked handles or recreate their
temporary users. [V1][V3][V4]

**Captured schema:** `atlas-pause-resume-cluster` accepts exactly `projectId`,
`clusterName`, and uppercase `action: "PAUSE" | "RESUME"`, all required, with
`additionalProperties: false`. There is no `wait`, `watch`, `force`, `dryRun`,
timeout, tier, or arbitrary Atlas update-body option. Atlas API/CLI capabilities
are not automatically MCP parameters. The captured `atlas-inspect-cluster`
and list tools provide inspection, not an implied wait option. [L11]

## 2. Delete only the exact QA email domain

### Regex, options, anchoring, and policy boundaries

**Documented query contract:** MongoDB query `$regex` accepts a pattern string
and sibling `$options`; `i` enables case-insensitive matching, `m` changes
`^`/`$` to line anchors, and `g` is unsupported. MongoDB 8.0 uses PCRE2, not
an arbitrary host-language regex dialect. Regex matching is not made
case-insensitive by a case-insensitive collation. This is the query predicate,
not MongoDB Search's different regex operator. [D1]

For the fixture's ordinary lowercase scalar emails, the supplied solution filter
is appropriately scoped:

```json
{"email":{"$regex":"@qa\\.octoshop\\.test$"}}
```

**Inference:** the literal `@` establishes the domain delimiter; escaped dots
prevent wildcard matching; the terminal anchor prevents
`@qa.octoshop.test.evil`. It does not match `@sub.qa.octoshop.test`,
`@qa.octoshop.tech`, or a local part merely containing `qa`. A starting `^`
is not necessary for this domain-suffix predicate on these fixtures. The filter
selects four production customers, not their staging copies: namespace selection
is independent of regex correctness. [L3][L4][L8][D1]

**Documented edge case:** PCRE2's default `$` can also match immediately before a
terminal newline; `\z` means strict end-of-subject and is unaffected by multiline
mode. Thus, for a strict terminal boundary, an available string-pattern form is
`"@qa\\.octoshop\\.test\\z"`; `[.]` is another spelling for a literal dot.
Do not assume `$` and `\z` are identical for arbitrary stored strings, or pass a
PCRE-only escape through a JavaScript `RegExp` constructor and assume parity.
[D1][D2]

**Policy inference, not a new requirement:** a domain-only pattern with
`"$options":"i"` is a reasonable case-folded domain interpretation; it does not
introduce local-part case-folding because the local part is not in the pattern.
The existing fixtures are lowercase, so they do not adjudicate mixed-case policy.
Do not invent email deliverability/DNS checks, trimming, plus-tag rewriting,
whole-address normalization, a new local-part grammar, or a requirement for an
initial `^`. Newline/malformed-value cases should test an explicitly stated
boundary, not smuggle in an unstated email-validation policy. [L3][L8][D1][D2]

### Exposed alternatives, not imaginary tools

**Local schema plus versioned implementation:** `delete-many` accepts
`connectionId`, `database`, `collection`, and an optional open-object `filter`;
`find` accepts an open-object filter and projection; `count` calls its filter
argument **`query`**; `aggregate` accepts an array of open-object stages.
The pinned tools deserialize EJSON before dispatch. The snapshot does **not**
expose `delete-one`, `bulkWrite`, arbitrary `runCommand`, or top-level deletion
options such as `collation`, `hint`, or `let`. [L11][V5][V6][V7]

Legitimate routes on the exposed surface include:

1. A direct escaped, domain-anchored `$regex` filter, optionally with `i`,
   passed to `delete-many` in `crm.customers`. [L11][D1]
2. Discover every matching document with `find` or `aggregate`, retain typed
   `_id`s, then delete using `_id: { "$in": [...] }`, `_id` equality in several
   calls, or an equivalent `$or`. For these fixtures, exact matching-email
   equality/`$in` also reaches the intended set if every matched address was
   first established. This is not permission to hard-code a partial list in a
   changing database. [D3][D4][L8][L11]
3. Use `$expr` with a suitable aggregation expression inside a filter, for
   example `$regexMatch` over `$email` with the same anchored pattern/options;
   or use aggregation `$match`/`$project` to identify IDs, followed by
   `delete-many`. `$expr` permits expressions, and `$regexMatch` is documented.
   Expression inputs need the appropriate string type; the current fixture
   projects scalar strings, not an arbitrary malformed-email schema. [D5][D6][L8][L11]

**Important distinctions:** a read aggregation alone does not delete anything.
Do not require the solution's exact regex spelling or a single delete call if
another exposed route deletes precisely the intended set. Conversely,
`$in: [{ "$regex": "..." }]` is not the query-operator syntax for regex inside
`$in`. EJSON can represent a BSON regular-expression value, but that is different
from embedding a `$regex` predicate object there. [D1][D4][E1][V6]

**Safety recommendation:** always supply a narrow filter. `{}` means all
documents to real `deleteMany`; schema optionality is not permission to omit
business criteria. A discovery-based deletion must account for `find`'s captured
default `limit: 10` and response/configured result caps; do not infer “every
match” from a truncated page. The current fixture has only four matches.
[D3][L8][L11]

## 3. Mark an order refunded, preserving numeric meaning

### Ordinary update operators are not aggregation expressions

**Documented database contract:** `updateMany` changes every document matching
its filter; one match can be selected by `_id` even though the method name says
“many.” Each document's write is atomic, not the operation across an arbitrary
set. MongoDB supports both an update-operator document and an aggregation
pipeline at its database API boundary. [U1]

**Captured schema/versioned MCP restriction:** this `update-many.update` is an
**object** described as an update-operator document. `zEJSON()` is a loose-object
schema with an EJSON transform, not an array union. Consequently an update array
is **not exposed by this captured tool**, despite underlying MongoDB support.
There is no captured `update-one`/`replaceOne` tool or arbitrary command escape
hatch. `upsert` is exposed, but it is unnecessary and potentially unsafe when
the task is to modify an existing, identified order. [L11][V6][V8]

A supported modifier document, after reading the target total, is:

```json
{
  "$set": {
    "status": "refunded",
    "refund.reason": "damaged in transit",
    "refund.amount": 214.5
  }
}
```

**Versioned database implementation:** ordinary `$set` stores the supplied BSON
element. In `r8.0.15`, `SetNode` retains the operand and sets the element directly;
there is no field-reference expression evaluation in that path. Therefore
`{"$set":{"refund.amount":"$total"}}` stores the literal string **`"$total"`**,
not `214.5`. Converting that string into a successful numeric refund in a replica
would misrepresent the upstream contract. [U2]

**Documented contrasting expression semantics:** within a database-supported
pipeline such as `[{"$set":{"refund.amount":"$total"}}]`, the aggregation `$set`
stage evaluates an expression and can copy another field's value. `$literal`
is how aggregation prevents dollar-prefixed strings from being evaluated.
This explains the distinction; it does **not** make that array valid input to
the captured `update-many` tool. [U3][U4][L11]

**Actually exposed aggregation alternative:** captured `aggregate.pipeline`
accepts generic stages. Versioned code explicitly handles `$merge`/`$out`
write pipelines, rather than always treating aggregation as read-only; configured
read-only/disabled-write restrictions and confirmation handling can reject or
gate them. MongoDB `$merge` can write to the same collection. A carefully
targeted `$match` + expression `$set` + `$merge` with appropriate match/no-match
behavior is therefore a conditional upstream-MCP alternative, **not** evidence
that the replica supports it. Do not claim every aggregation is read-only from
the tool annotation alone, or that a read-only `$set` pipeline persists changes.
Same-collection `$merge` has documented iteration hazards and needs particular
care; the ordinary targeted update is simpler here. [L11][V7][V10][U5]

### Preserve fields and do not invent timestamps

**Documented:** dotted `$set` updates the specified embedded fields and creates
missing embedded documents as necessary. It preserves unspecified fields;
setting the entire `refund` object instead would replace that subdocument and
could erase existing refund metadata. No existing `refund` or `updatedAt` is
present on the fixture target. The fixture's `total`, customer, currency, items,
and existing date fields should remain unchanged. [U6][L9]

**Versioned implementation:** `UpdateManyTool` passes the decoded filter/update
and the `upsert` option to the provider; it adds no `updatedAt`, `refundedAt`,
refund identifier, or payment call. MongoDB provides explicit `$currentDate`
and aggregation time expressions when a caller wants timestamp changes; these
are not an automatic application-field policy. External triggers/middleware
were not established, so neither invent such side effects nor require them for
this user request. [V8][U7][L5]

**Recommendation:** identify both order number and customer, then use the
discovered ObjectId or equivalently precise filter in `shop.orders`. Repeated
assignment of the same values can legitimately produce `modifiedCount: 0`
after an earlier success; judge final state and scope, not a mandatory positive
count on every retry. Do not add `upsert: true`, rewrite other orders, or issue a
real payment/refund transfer. [L5][L9][U1][V8]

### EJSON, ObjectId, and which refund representations are numbers

**Versioned MCP decoding:** `zEJSON()` calls
`EJSON.deserialize(value, { relaxed: false })` recursively within the object.
Thus canonical wrappers are transport encodings of BSON types, not automatically
application strings/documents. Result conversion uses Compass idiomatic EJSON,
so inspect typed value, not JSON display shape alone. The exact transitive BSON
version and the replica's encoding behavior remain unverified. [V6][V9][P3]

**Documented ObjectId distinction:** an ObjectId filter can use
`{"_id":{"$oid":"65f1a000000000000000000a"}}` through that decoder. The bare
hexadecimal JSON string is BSON string data, not an ObjectId merely because its
characters look like one. `ObjectId(...)` and `NumberDecimal(...)` are shell
expressions, not JSON argument literals. Preserve type when carrying discovered
IDs into `$in` or an exact filter. [E1][E2][V6][L9]

The following table assumes values are inside a valid update object and pass
through the pinned EJSON decoder. Numeric **type** and correct **amount** are
separate conditions. [E1][E2][E3][V6]

| JSON/EJSON value for `refund.amount` | BSON meaning | Meets this request? |
| --- | --- | --- |
| `214.5` | Numeric value; this non-integral EJSON number is represented as a double. | Yes. |
| `{"$numberDouble":"214.5"}` | Explicit BSON double. | Yes. |
| `{"$numberDecimal":"214.5"}` or `{"$numberDecimal":"214.50"}` | BSON Decimal128, not a string despite the string payload in its encoding. | Yes: same numeric amount; user did not demand double specifically. |
| `{"$numberLong":"214"}` | BSON signed int64/Long. It is genuinely numeric. | No: wrong value. No integer type can represent `214.5` exactly. |
| `{"$numberInt":"214"}` | BSON int32, also genuinely numeric. | No: wrong value. |
| `"214.5"` | BSON string. | No. |
| `"$total"` in ordinary `$set` | Literal BSON string. | No. |
| `{"amount":214.5}` | An embedded document, not a numeric scalar. | No. |

**Documented numeric family:** MongoDB `$isNumber` recognizes int32, int64/Long,
double, and decimal; it does not treat numeric-looking strings as numbers.
Canonical int64 encodings are especially important beyond JavaScript's exact
integer range. Decimal128 and double have different precision characteristics;
do not indiscriminately coerce arbitrary int64/decimal values through a host
double to compare them. [E1][E2][E3]

**Arithmetic inference:** `214.5 = 429/2` is exactly representable in binary, so
double-versus-decimal approximation is not a reason to reject this fixture's
correct amount. Conversely, an integer `21450` is not the order total: no request
authorizes switching units to cents. NaN/infinity are not this finite total.
The evaluator should compare a genuine BSON numeric value with the original
numeric total, not accept arbitrary parseable strings or require one EJSON
serialization spelling. [L5][L9][E1][E3]

## Actionable objections to evaluator/fixture design

These are recommendations from the contracts above, **not findings from an audit
of local implementation**.

1. **Keep provenance layers separate.** Record the captured schema/version and
   the inspected published commit, but do not label replica execution “real
   Atlas/MongoDB” or assume capture identity proves package execution. [L10][L11][P4][P5]
2. **Grade pause intent separately from completion.** Require the correctly
   scoped paused configuration, preserve full target configuration, and do not
   require tier changes, instantaneous billing evidence, a `PAUSED` state enum,
   or synchronous readiness. [L1][L2][V1][A2][A3]
3. **Strengthen preservation coverage.** Current pause TOML checks selected
   clusters' paused flags and the target tier, not every cluster field.
   Purge checks survivor emails, not whole survivor documents. Refund checks
   selected distractor fields, not every other order or untouched target field.
   Compare protected fixture objects/collections comprehensively, allowing only
   explicitly modeled operational metadata transitions. [L2][L4][L6]
4. **Do not fabricate eligibility evidence.** Add explicit health/disk/cooldown
   data only when testing those documented conditions; retain the base request
   as a valid dedicated-target pause rather than requiring unknown metrics.
   Keep Free automatic pause distinct from manual tool eligibility. [A1][L7]
5. **Model retry outcomes faithfully.** An already-paused target may satisfy
   intent even though another pause returns 409; pending changes and resume
   readiness are separate states. Do not universalize success-on-repeat.
   Cleanup failure must not magically roll back an accepted Atlas update. [A4][V1]
6. **Scope client-side cleanup correctly.** If connection behavior is tested,
   seed registry entries with matching Atlas metadata and explicit temporary
   user ownership. Test unrelated connections/users surviving and best-effort
   cleanup failures; do not require blanket application-user deletion. [V1][V3][V4]
7. **Accept equivalent exposed deletion plans.** Evaluate the exact deleted set
   and preserved scope, not the canned regex text. Accept discovered typed-ID,
   equivalent filter, and aggregation-discovery routes, including several
   `delete-many` calls when necessary. Do not demand nonexistent tools/options.
   [L3][L4][L11][D3][D4][D5]
8. **Make domain boundary tests intentional.** Add suffix-extension,
   look-alike-dot, subdomain, and mixed-domain-case examples to distinguish
   unsafe broad predicates. State the expected interpretation of mixed-case or
   malformed/newline values rather than deriving an email deliverability policy
   from lowercase fixtures. Respect `$options` and PCRE2 anchor distinctions.
   [D1][D2][L8]
9. **Keep query and expression evaluation distinct.** Ordinary `$set` with
   `"$total"` must not pass as a numeric copy. Conversely, do not ban expression
   use in a supported `$expr` filter merely because ordinary modifier operands
   are literal. [U2][D5][D6]
10. **Do not advertise unsupported pipeline updates.** The captured
    `update-many.update` is object-only. If aggregation-write alternatives are
    admitted, enforce the documented write restrictions and grade their result
    instead of a mandatory `update-many` call. Otherwise explicitly document
    that narrower admission boundary without pretending MongoDB lacks pipeline
    updates. [L6][L11][V6][V7][V10][U1]
11. **Check numeric type and value after EJSON decoding.** Accept a correct
    Decimal128/double amount; reject plain `"214.5"`, `"$total"`, arbitrary
    documents, wrong integer values, and wrong unit conversions. An int64 is a
    number, just incapable of this fractional amount. Preserve ObjectId typing
    in filters; do not compare canonical numeric wrappers as ordinary strings.
    [E1][E2][E3][U2][V6]
12. **Do not invent workflow effects.** Require only the specified refund
    record fields. Preserve existing dates/other metadata; do not require a new
    automatic timestamp, payment gateway call, transfer, or refund identifier.
    A repeat no-op is not failure if the correct final state already holds.
    [L5][L9][V8][U1][U7]

## Sources

Local citations are repository-relative. Package/source references below were
read as public research material, not installed or executed. Documentation is
rolling unless a version is included in its URL.

### Local scope and captured surface

[L1]: ../../scenarios/cloud/mongodb/pause-idle-staging-cluster/instruction.md
[L2]: ../../scenarios/cloud/mongodb/pause-idle-staging-cluster/scenario.toml
[L3]: ../../scenarios/cloud/mongodb/purge-qa-accounts/instruction.md
[L4]: ../../scenarios/cloud/mongodb/purge-qa-accounts/scenario.toml
[L5]: ../../scenarios/cloud/mongodb/refund-order/instruction.md
[L6]: ../../scenarios/cloud/mongodb/refund-order/scenario.toml
[L7]: ../../scenarios/cloud/mongodb/pause-idle-staging-cluster/dataset/mongodb/clusters.json
[L8]: ../../scenarios/cloud/mongodb/purge-qa-accounts/dataset/mongodb/db.crm.customers.json
[L9]: ../../scenarios/cloud/mongodb/refund-order/dataset/mongodb/db.shop.orders.json
[L10]: ../../catalog/services.toml
[L11]: ../../servers/snapshots/mongodb.json
[L12]: ../../scenarios/cloud/mongodb/refund-order/dataset/mongodb/deployment.json

- [L1], [L2]: pause request and expectations.
- [L3], [L4]: deletion request and expectations.
- [L5], [L6]: refund request and expectations.
- [L7]: cluster projection; organization/project linkage was also read from
  sibling `orgs.json` / `projects.json`, and ordinary-user scope from
  `db_users.json`. No full application-data scan was needed.
- [L8]: production `_id`/email projection; staging `_id`/email projection came
  from sibling `db.crm_staging.customers.json`.
- [L9]: target/distractor order identity, customer, total, status, items, and
  field-presence projection; customer and staging cross-checks used sibling
  `db.shop.customers.json` and `db.shop_staging.orders.json`.
- [L10], [L11]: replica declaration and captured tool schemas/metadata.
- [L12]: fixture-advertised deployment version; the other two scenarios also
  advertise `8.0.15`.

### Published-package identity

[P1]: https://registry.npmjs.org/mongodb-mcp-server/3.2.2
[P2]: https://registry.npmjs.org/@mongodb-js%2fmcp-tools-atlas/3.2.2
[P3]: https://registry.npmjs.org/@mongodb-js%2fmcp-tools-mongodb/3.2.2
[P4]: https://registry.npmjs.org/-/npm/v1/attestations/@mongodb-js%2fmcp-tools-atlas@3.2.2
[P5]: https://registry.npmjs.org/-/npm/v1/attestations/@mongodb-js%2fmcp-tools-mongodb@3.2.2
[P6]: https://registry.npmjs.org/mongodb-mcp-server

- [P1]: top-level version/repository and exact scoped-package dependencies.
- [P2], [P3]: tool versions, dependency ranges, tarball URLs, and integrity.
  Inspected archive members include `dist/tools/update/pauseResumeCluster.js`,
  `dist/tools/connect/connectCluster.js`, `dist/tools/update/updateMany.js`,
  `dist/tools/delete/deleteMany.js`, `dist/tools/read/aggregate.js`,
  `dist/args.js`, and `dist/common/connectionStore.js`.
- [P4], [P5]: publisher-declared SLSA source commit and package digests.
- [P6]: registry `time["3.2.2"]` publication timestamp.

### Fixed upstream MCP source

All source links in this group use provenance commit
`0af7e43bd2c8d0188644b355bbc25da8c7c62d98`.

[V1]: https://github.com/mongodb-js/mongodb-mcp-server/blob/0af7e43bd2c8d0188644b355bbc25da8c7c62d98/packages/tools-atlas/src/tools/update/pauseResumeCluster.ts
[V2]: https://github.com/mongodb-js/mongodb-mcp-server/blob/0af7e43bd2c8d0188644b355bbc25da8c7c62d98/packages/atlas-api-client/src/apiClient.ts
[V3]: https://github.com/mongodb-js/mongodb-mcp-server/blob/0af7e43bd2c8d0188644b355bbc25da8c7c62d98/packages/tools-mongodb/src/common/connectionStore.ts
[V4]: https://github.com/mongodb-js/mongodb-mcp-server/blob/0af7e43bd2c8d0188644b355bbc25da8c7c62d98/packages/tools-atlas/src/tools/connect/connectCluster.ts
[V5]: https://github.com/mongodb-js/mongodb-mcp-server/blob/0af7e43bd2c8d0188644b355bbc25da8c7c62d98/packages/tools-mongodb/src/tools/delete/deleteMany.ts
[V6]: https://github.com/mongodb-js/mongodb-mcp-server/blob/0af7e43bd2c8d0188644b355bbc25da8c7c62d98/packages/tools-mongodb/src/args.ts
[V7]: https://github.com/mongodb-js/mongodb-mcp-server/blob/0af7e43bd2c8d0188644b355bbc25da8c7c62d98/packages/tools-mongodb/src/tools/read/aggregate.ts
[V8]: https://github.com/mongodb-js/mongodb-mcp-server/blob/0af7e43bd2c8d0188644b355bbc25da8c7c62d98/packages/tools-mongodb/src/tools/update/updateMany.ts
[V9]: https://github.com/mongodb-js/mongodb-mcp-server/blob/0af7e43bd2c8d0188644b355bbc25da8c7c62d98/packages/tools-mongodb/src/helpers/bsonToJson.ts
[V10]: https://github.com/mongodb-js/mongodb-mcp-server/blob/0af7e43bd2c8d0188644b355bbc25da8c7c62d98/packages/tools-mongodb/src/mongodbTool.ts

- [V1]: PATCH body, advertised retries, asynchronous wording, selected disconnects.
- [V2]: API media version and PATCH endpoint/error propagation.
- [V3], [V4]: revocation and temporary-user lifecycle; `ConnectionEntry` cleanup
  ownership is additionally defined in sibling `common/connectionRegistry.ts`.
- [V5], [V6], [V8]: exposed argument shapes, EJSON decoding, provider dispatch.
- [V7], [V10]: aggregation write handling and configuration/confirmation gates.
  Generic stage EJSON decoding is defined in sibling `mongodbSchemas.ts`.
- [V9]: BSON-to-idiomatic-EJSON result conversion.

### Official Atlas, MongoDB, and PCRE contracts

[A1]: https://www.mongodb.com/docs/atlas/pause-terminate-cluster/
[A2]: https://www.mongodb.com/docs/api/doc/atlas-admin-api-v2/2025-03-12/operation/operation-updategroupcluster
[A3]: https://www.mongodb.com/docs/atlas/billing/cluster-configuration-costs/
[A4]: https://www.mongodb.com/docs/atlas/reference/api-errors/
[D1]: https://www.mongodb.com/docs/v8.0/reference/operator/query/regex/
[D2]: https://www.pcre.org/current/doc/html/pcre2pattern.html
[D3]: https://www.mongodb.com/docs/v8.0/reference/method/db.collection.deleteMany/
[D4]: https://www.mongodb.com/docs/v8.0/reference/operator/query/in/
[D5]: https://www.mongodb.com/docs/v8.0/reference/operator/query/expr/
[D6]: https://www.mongodb.com/docs/v8.0/reference/operator/aggregation/regexMatch/
[U1]: https://www.mongodb.com/docs/v8.0/reference/method/db.collection.updateMany/
[U2]: https://github.com/mongodb/mongo/blob/r8.0.15/src/mongo/db/update/set_node.cpp
[U3]: https://www.mongodb.com/docs/v8.0/tutorial/update-documents-with-aggregation-pipeline/
[U4]: https://www.mongodb.com/docs/v8.0/reference/operator/aggregation/set/
[U5]: https://www.mongodb.com/docs/v8.0/reference/operator/aggregation/merge/
[U6]: https://www.mongodb.com/docs/v8.0/reference/operator/update/set/
[U7]: https://www.mongodb.com/docs/v8.0/reference/operator/update/currentDate/
[E1]: https://www.mongodb.com/docs/v8.0/reference/mongodb-extended-json/
[E2]: https://github.com/mongodb/specifications/blob/master/source/extended-json/extended-json.md
[E3]: https://www.mongodb.com/docs/v8.0/reference/operator/aggregation/isNumber/

- [A1]–[A4]: pause eligibility/effects, versioned API fields, storage billing,
  and documented errors. No live API endpoint was called.
- [D1]–[D6]: query regex/options, PCRE2 anchors, deletion and alternative filters.
- [U1]–[U7]: update document versus pipeline, literal modifier implementation,
  aggregation-write semantics, field preservation, and explicit timestamps.
- [E1]–[E3]: transport encodings, BSON numeric/ObjectId types, and numeric tests.
