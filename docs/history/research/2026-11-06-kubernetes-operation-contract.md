# Kubernetes operation-contract research — 2026-11-06

## Scope, evidence, and version boundaries

This is read-only primary-source research for three unchanged requests. Local
instructions determine business scope; TOML expectations and fixtures help locate
the intended objects, but do not establish Kubernetes or Helm semantics. The
diagnostic WIP in `crates/world/tests/kubernetes_contracts.rs` is not upstream
authority and was not used as such.

| Request | Intended change and protected scope |
| --- | --- |
| OOM memory limit | Identify the OOM-killed workload in `analytics`; set its container memory limit to one gibibyte. The fixtures identify container `report-builder` in Deployment `analytics/report-builder`: its Pods record `OOMKilled` and point through the same ReplicaSet UID to this Deployment. Preserve its image, replicas, existing requests, and other configuration; do not change `report-api`, `report-scheduler`, or staging. [L1][L2] |
| Checkout scaling | Set Deployment `shop-prod/checkout` from three to eight desired replicas, preserving its image and other configuration. Do not change `checkout-worker` or `shop-staging`. The fixture HPA targets `catalog`, not checkout. The instruction does not require eight Ready Pods. [L3][L4] |
| Finished load-test uninstall | Uninstall `perf/checkout-loadtest`, cleaning up its ordinary manifest Job and script ConfigMap. Preserve the running search test, k6 operator, and `perf-staging` copy. History has superseded revision 1 and deployed revision 2. The target manifests have no keep-policy/hook annotation; no release hooks are recorded. Its four completed Pods reference the target Job UID. [L5][L6] |

Evidence labels used below:

- **Documented:** an explicit official API/documentation statement.
- **Versioned implementation:** behavior inspected in the specified upstream tag
  or commit, including upstream tests where identified; not a promise about every
  version, client, admission configuration, or mock.
- **Pinned-client fact:** observed published-package contents/build metadata,
  connected to its versioned upstream source.
- **Inference/recommendation:** application of that evidence to these requests,
  not an additional user requirement.
- **Uncertain:** not established by the inspected evidence.

### Server and client are different pins

**Local fact:** discovery advertises Kubernetes `v1.33.4`; the catalog selects npm
`kubernetes-mcp-server@0.1.1`. Neither observation establishes the other's library
version. [L7]

**Pinned-client facts:** npm registry metadata identifies commit
`26dfd9034dea17571ff34c8999f77de211f0f71a` and platform-specific optional packages.
The published JS wrapper selects a native binary and forwards arguments. I also
stream-read, without installing or executing, the published **linux-amd64**
package's native binary. Its embedded build metadata identifies that commit,
module version `v0.1.1`, Go `1.26.10`, Helm `v3.22.1`, and Kubernetes
`api`, `apimachinery`, `client-go`, `cli-runtime`, and `kubectl` modules
`v0.37.1`. The commit's `go.mod` independently agrees. These are observed
distribution facts, not deductions from the npm version number. Other platform
artifacts and the benchmark's actually provisioned binary were not inspected.
[C1][C2][C3][C4]

Accordingly, server behavior below is checked against Kubernetes `v1.33.4` /
`apimachinery v0.33.4`; MCP behavior is checked at the identified commit, and Helm
behavior against `v3.22.1`. Selected Quantity parsing branches were also checked
in the embedded-client module version, `v0.37.1`. Server discovery is not evidence
that this mock implements every corresponding contract. [L7][Q1][Q8][C4]

## 1. Memory quantities and Deployment template updates

### Accepted representations and exact equivalence

**Documented:** memory's base unit is the byte. Decimal and binary suffixes are
different, case-sensitive units: `m` means milli, not mebibytes. Kubernetes
documents multiple decimal/exponent/binary representations for memory.
[Q2]

**Versioned implementation:** the suffix table recognizes DecimalSI
`n`, `u`, `m`, the empty suffix, `k`, `M`, `G`, `T`, `P`, `E`; BinarySI
`Ki`, `Mi`, `Gi`, `Ti`, `Pi`, `Ei`; and decimal exponents beginning with `e`
or `E`. Binary powers increase by ten, decimal SI powers by three. An exponent's
suffix is parsed as an integer; this is not an unrestricted floating-point
literal grammar or a license to combine an exponent with another unit.
[Q3]

**Versioned implementation:** `Quantity.UnmarshalJSON` accepts quoted quantity
text and unquoted JSON-number text; `MarshalJSON` produces a quoted quantity.
Thus string-only mock decoding rejects real typed-decoder alternatives.
`null` decodes as zero at this layer, **not** one gibibyte; PATCH deletion
semantics are a separate concern. Booleans, arrays, and objects are not quantity
numbers. [Q1]

**Representation boundary:** quoted quantities can contain a leading sign or a
fractional decimal mantissa, including forms such as `"+1Gi"` or `".5Gi"`.
An unquoted value must first be a legal JSON number: bare `+1`, `.5`, or `1Gi`
are not legal JSON-number tokens. Empty quantity text, unknown unit suffixes,
and non-finite-number names are not supported alternatives. [Q1][Q3][Q4][J1]

**Exact arithmetic inference:** these representations denote the same requested
limit, not merely approximately equal floating-point values:

| Form | Value |
| --- | --- |
| `"1Gi"` | `2^30` bytes |
| `"1024Mi"` | `1024 × 2^20` bytes |
| `"1073741824"` or JSON number `1073741824` | `1073741824` bytes |
| `"1.073741824e9"` | `1.073741824 × 10^9` bytes |
| `"1073741824000m"` | `1073741824000 × 10^-3` bytes |

These follow from the unit rules and decoder; upstream tests explicitly verify
that `1024Mi` re-emits as `1Gi`. `1G` is **not** equivalent: it is
`1000000000` bytes. [Q1][Q2][Q3][Q4]

**Schema boundary:** v1.33.4 OpenAPI v2 describes Quantity as a string, while its
Go type advertises string-or-number alternatives for OpenAPI v3 and has the
numeric decoder described above. Do not assert that every schema-driven
tool/SSA path accepts every numeric spelling merely because typed decoding does.
Conversely, do not globally reject numeric quantities on a path that successfully
uses typed decoding. Strings are the most portable choice. Exact end-to-end
numeric acceptance through this MCP's unstructured Apply route remains
unverified. [A1][Q1][C5]

### Precision, bounds, and comparison pitfalls

**Documented contract versus versioned implementation:** the Quantity type
comment advertises a magnitude ceiling of `2^63−1` and three decimal places,
including a milli-rounding example. The parser and its own tests do **not**
uniformly implement that wording. Do not turn the comment into a supposedly
exact v1.33.4 parser specification. [Q1][Q4]

- **Versioned implementation/tests:** precision reaches nanounits, not merely
  milliunits. Tests accept values below a milliunit; examples include
  `3.001n → 4n`, `1.1E-9 → 2e-9`, and
  `0.0000000001 → 1n`. Nonzero discarded precision rounds away from zero:
  the parser operates on magnitude and restores the sign. Fractional BinarySI
  inputs are supported too. [Q1][Q4][Q5]
- **Versioned implementation/tests:** BinarySI overflow saturates at magnitude
  `9223372036854775807`; tests cover `9Ei` and a huge `Ki` input. DecimalSI
  does not share that unconditional cap: the same test table accepts `12E`,
  which exceeds signed-int64 magnitude. Do not conflate parser acceptance,
  safe conversion, and usable resource limits. [Q1][Q4][Q6]
- **Versioned implementation:** decimal exponent text is parsed into signed
  int64 and then narrowed to int32. Subsequent scales are int32 too. This is
  not evidence that all int64 exponents have safe, intended mathematical
  semantics. Extreme exponents and all overflow interactions were not
  exhaustively verified; an independent parser must not silently promise
  unbounded exact exponent support. [Q3][Q5]
- **Versioned implementation:** `Value()` rounds fractional units; scaled/milli
  conversions can overflow. `AsInt64()` can fail because the representation
  uses a decimal backing value, not solely because the mathematical value is
  out of range. Use exact Quantity/decimal comparison rather than float
  tolerance, a ceil-to-bytes test, or success of a fast int64 accessor.
  Upstream semantic equality compares quantities numerically, ignoring format.
  [Q1][Q6][Q7]

**Recommendation:** accept any supported representation whose parsed quantity
equals one gibibyte. Do not require response text to become exactly `1Gi`,
or treat a nearby fractional value as equal merely because some downstream
integer conversion rounds it to the same byte count. The inspected client
`v0.37.1` retains the same nanounit rounding and BinarySI-only saturation
branches; that does not replace checking server `v1.33.4`. [Q1][Q4][Q7][Q8]

### Update invariants and completion boundary

**Versioned implementation:** resource quantities must be nonnegative; when both
a memory request and limit exist, request must not exceed limit. For this target,
the existing `384Mi` request remains valid under `1Gi`, so raising the request,
CPU allocation, or replicas is unnecessary. Generic non-overcommittable-resource
rules must not be incorrectly applied to ordinary memory. [R1][L2]

**Documented:** if a request is absent, Kubernetes can default it from the limit
unless admission already supplied a request. That is not a rule to replace an
existing request whenever a limit changes. Additional quota/LimitRange/admission
constraints are cluster-dependent and were not established by these fixtures.
[Q2][R1]

**Documented:** a Deployment expresses desired state. Changing its Pod template
normally initiates a rollout; scaling alone does not. Paused Deployments defer
template rollout. Therefore a persisted memory-limit change establishes the
requested desired configuration, **not** proof that old Pods were resized, a new
ReplicaSet became Ready, or reports were delivered. None of those additional
outcomes is demanded by this user instruction. A Pod-only change that leaves the
Deployment template unchanged does not durably configure this workload.
[D1][L1]

**Preservation recommendation:** update the identified container by its name,
not by assuming index zero is its universal identity. Strategic merge and SSA
use schema-defined container-list keys; JSON merge patch instead replaces arrays.
A partial container array must not erase its image, requests, environment,
siblings, or other configuration. A full PUT is legitimate only if it preserves
the required fields and passes validation. [A1][A2][C5][L1][L2]

## 2. Deployment and Scale writes, metadata, and controller effects

### Legitimate write variants

**Documented/versioned API:** both the Deployment resource and its `/scale`
subresource expose PUT and PATCH in v1.33.4. The scale object uses
`autoscaling/v1`, even though its URL is beneath `/apis/apps/v1/...`.
The Scale handler projects desired replicas into the parent Deployment rather
than storing an independent workload. [A1][A3]

| Variant | Legitimate interpretation for this request |
| --- | --- |
| Deployment JSON merge PATCH | A body carrying only `spec.replicas: 8` can update that field. |
| Deployment strategic-merge PATCH | Another supported partial-update form for this built-in API. |
| Deployment JSON Patch | An operation list updating `/spec/replicas`, optionally with tests/preconditions, is not an object-shaped merge patch. |
| Deployment server-side Apply | A schema-valid apply object with identity and the replica value is a supported alternative; field ownership/conflicts matter. |
| Deployment PUT | Replacement/update of a valid Deployment representation, preserving template and unrelated desired fields. |
| Scale PATCH | Supported patch forms can update desired replicas on the scale projection. |
| Scale PUT | A valid Scale object read, modified, and written back is the ordinary update pattern. Echoed metadata/status do not turn it into a different business operation. |

The API advertises JSON Patch, JSON merge patch, strategic merge patch, YAML
Apply, and CBOR Apply media types for both PATCH routes. CBOR support also depends
on server feature configuration; its advertised media type is not an assurance
that this mock/client enables it. A minimal replicas-only merge body is **not**
a universal valid PUT body, and media types are not interchangeable. [A1][A2]

**Pinned-client fact:** `ResourcesScale` GETs the scale object, changes
`spec.replicas`, and calls dynamic-client Update on `"scale"`; it does not send
only an exact replicas-only JSON object. `ResourcesCreateOrUpdate` removes
supplied top-level status and calls server-side Apply with a field manager and
force enabled. This makes both PUT `/scale` and Apply of the Deployment important
alternatives, not hypothetical deviations. [C5]

**Versioned client confirmation:** client-go `v0.33.4` also implements
`UpdateScale` using HTTP PUT and exposes `ApplyScale`. This independently supports
the API variants; it is **not** the MCP's embedded-client version. [A4][C3]

**Documented:** SSA field ownership can change and conflicting ownership can be
forced. Reapplying an incomplete object is not a universal guarantee that all
omitted fields survive; previously managed fields can be removed when omitted.
Evaluate preservation from resulting desired state and relevant ownership, not
from a claim that all Apply calls are harmless merges. [A5]

### Resource versions, generations, and identity

- **Documented:** `metadata.resourceVersion` is an opaque version identifier,
  not a counter a grader should increment or numerically compare. A supplied
  stale version can produce `409 Conflict`; conditional PATCH is legitimate.
  Do not prescribe an exact next resourceVersion. [A2]
- **Versioned implementation:** Deployment permits unconditional update.
  Generic storage uses the current version when an allowed unconditional PUT
  omits its version; a supplied version is checked. Consequently, requiring a
  resourceVersion on **every** Deployment/Scale write is stricter than this
  server implementation, though read-modify-write with a version is safer.
  The scale handler shares the Deployment store. [A3][A6][A7]
- **Versioned implementation:** Deployment strategy increments generation when
  spec **or annotations** differ semantically. Thus either changing replicas or
  changing template memory increments generation; a status-only observation
  does not establish a new desired spec. It preserves old status during an
  ordinary parent update. Generation, rollout revision, resourceVersion, and
  observedGeneration must not be treated as interchangeable. [A6][D1]
- **Versioned implementation:** Scale carries the parent's UID/resourceVersion
  and desired replicas; its status replica count reflects observed parent
  status. Its projection does not copy generation. Scale updates validate a
  supplied UID and transfer replicas/resourceVersion/managed-field information
  back to the Deployment. Do not require Scale itself to echo a newly bumped
  generation. [A3]
- **Documented/versioned implementation:** owner references include kind,
  API version, name, UID, and controller identity. Controller claiming rejects
  an object already controlled by a **different UID**, even if a name matches.
  Names, prefixes, and labels alone do not prove ownership; a recreated
  same-name owner is a different object. Orphan adoption is possible under
  selector and safety checks, so “all descendants must have been in the initial
  fixture” is also too strong. [O1][O2]

### Derived changes are asynchronous, not unrelated user edits

**Documented:** Deployment controllers reconcile desired state through
ReplicaSets, which manage Pods. Scaling during a rolling update can distribute
replicas across active ReplicaSets; a template change may leave old and new
replicas coexisting. Capacity, scheduling, image pulls, quota, readiness, and
controller operation can prevent or delay convergence. The successful write is
not an atomic transaction updating all Pods and status. [D1]

**Recommendation:** accept the requested parent desired-state change without
requiring exactly eight Ready Pods, eight extant Pod objects, immediate matching
ReplicaSet counts, or immediate observedGeneration convergence. Conversely, a
status-only edit or manually scaling a child while leaving the Deployment at
three does not satisfy the production Deployment request. [D1][L3]

**Preservation recommendation:** allow causally justified controller effects
within the correctly UID-owned workload graph: ReplicaSet scaling, new/replaced
Pods, template hashes/revisions, observed status, and server-managed metadata.
Do not freeze those values, but do not globally ignore every object with the same
prefix/labels either. Protect independent workloads, staging, and unrelated
desired fields. Controller-owned descendants are not an excuse for direct
unrequested agent changes to those resources. [D1][O1][O2][L1][L3]

## 3. Helm uninstall, retained history, and cleanup

### Purge versus history retention

**Documented/versioned CLI:** ordinary Helm uninstall defaults to
`keep-history=false`, background cascade, enabled hooks, and `wait=false`.
`--keep-history` retains release records while uninstalling resources; it is
not equivalent to keeping the Job. These are CLI defaults, not defaults every
embedding application must use. [H1]

**Versioned implementation:** without history retention, successful uninstall
purges **all stored revisions** for the selected release. With retention, it
marks the latest revision `uninstalled`; earlier deployed revisions are
superseded, not all rewritten to `uninstalled`. Already superseded history
remains historical. For this fixture, retained final history should therefore
have revision 1 `superseded` and revision 2 `uninstalled`. `uninstalling` is a
transitional status, not proof of successful completion. [H2][H3][L6]

**Pinned-client fact:** this MCP's uninstall wrapper sets `IgnoreNotFound=true`,
`Wait=true`, and a five-minute timeout. It does not enable KeepHistory or set
DeletionPropagation. The Helm action therefore purges history and falls back to
background propagation; this MCP tool's schema exposes only name and namespace,
not a history-retention switch. Do not mistake the wrapper's wait behavior for
the Helm CLI default. [C6][C7][H2]

**Business-scope inference:** the user requested uninstall and Job/script cleanup,
not history erasure. A correctly completed `--keep-history` uninstall is a
legitimate Helm-level alternative and must not be rejected solely because release
history remains, if that operation is available. It is not directly selectable
through the inspected `helm_uninstall` tool. The TOML's absent-history assertions
describe the default tool path, not a universal meaning of “uninstall.”
[L5][H1][C7]

### Manifest deletion is distinct from release-record deletion

**Versioned implementation:** Helm builds its normal deletion set from the
selected release's **stored manifest**, orders it for uninstall, filters retained
resources, and deletes the remaining objects. It does not establish membership
by a broad release-name substring or delete every revision's historical manifest.
Corrupt manifests and deletion/hook failures can produce errors and partial
effects; this is not an atomic cluster-wide transaction. [H2]

**Inference for this fixture:** both the target Job and script ConfigMap are in
revision 2's ordinary manifest and have no retention exception, so merely
deleting Helm's bookkeeping records is insufficient. Likewise, deleting the Job
and ConfigMap while leaving the latest release deployed is not a completed
uninstall. Preserve the separate search/operator/staging releases and their
resources. [L5][L6][H2]

**Versioned storage facts:** Helm's action configuration defaults to Secrets
storage; ConfigMap and SQL drivers also exist. The storage key includes the
release name and revision. In this local default, deletion of
`sh.helm.release.v1.checkout-loadtest.v2` is consistent with purging revision 2,
but that one DELETE alone proves neither manifest cleanup nor purge of revision
1. Do not impose a Secret URL as a backend-independent Helm contract.
[H4][H5][L7]

### Keep annotations, hooks, propagation, and Job Pods

- **Versioned implementation:** `helm.sh/resource-policy: keep` can exclude an
  ordinary manifest resource from uninstall's deletion set. This is separate
  from `--keep-history`. A retained resource is therefore not universally
  evidence that uninstall failed. This fixture has no such annotation; do not
  invent one to excuse an undeleted target Job or script. [H6][L6]
- **Versioned implementation:** uninstall may run pre-delete/post-delete hooks.
  Hook-created resources are handled through the separate hook lifecycle and
  delete policies, not a blanket “delete every associated object” rule.
  The default hook deletion policy is before-hook-creation, rather than an
  unconditional after-success cleanup. No hooks are recorded here, so hook
  semantics do not justify requiring extra fixture changes. [H2][H7][L6]
- **Documented:** background cascade can remove an owner before dependent
  deletion finishes; foreground keeps the owner pending while relevant blocking
  dependents are removed; orphan propagation leaves dependents. Finalizers and
  controller availability can delay either deletion process. These modes are
  legitimate API alternatives, not identical immediate states. [G1]
- **Inference from ownership:** the completed checkout Pods are dependents of
  the specific Job UID, so cascading deletion can legitimately remove them even
  though they are not individual documents in the Helm manifest. Do not classify
  these deletions as changes to “everything else,” require the old completed
  Pods to remain, or delete other same-name/prefix Pods without owner identity.
  Orphaned Pods need not vanish merely because the Job did. [L6][O1][G1]
- **Versioned wait boundary:** Helm's deletion waiter checks the resource list
  supplied to it; it does not recursively enumerate every dependent. Therefore,
  even this MCP's `Wait=true` does not prove that all background-GC Job Pods
  disappeared before return. Foreground cascade or explicit dependent
  verification can offer stronger evidence, but the user did not ask for a
  particular cascade/wait strategy. [H8][H9][C6][G1]

**Recommendation:** distinguish successful uninstall, disappearance of the
requested manifest Job/script, retained release history, and eventual dependent
cleanup. Retained history is valid; an active target release or missing cleanup
is not. For orphan propagation, assess the user's actual cleanup scope rather
than declaring all orphan behavior either automatically sufficient or always
forbidden. [L5][H1][H2][G1]

## 4. Actionable objections to an over-restrictive contract

1. **Reject literal `1Gi` matching.** Compare exact parsed quantity values,
   including `1024Mi` and byte/exponent forms; distinguish typed JSON-number
   acceptance from unverified schema-specific paths. [Q1][Q3][Q4][A1]
2. **Reject the three-decimal/all-form-int64 parser shortcut.** Account for the
   verified nanounit and DecimalSI/BinarySI differences; explicitly bound any
   unsupported extreme-exponent behavior instead of calling it upstream parity.
   [Q3][Q4][Q5][Q6]
3. **Reject mandatory request increases.** Preserve the valid existing request;
   limit-only change is enough for this target. [R1][L2]
4. **Reject one method, endpoint suffix, or exact body shape as the only scaling
   evidence.** Admit valid parent/Scale PUT and PATCH forms, including operation
   arrays, Apply objects, and read-modify-write Scale objects. Verify the resulting
   desired field and preservation. [A1][A3][A4][C5]
5. **Reject invented readiness requirements.** Neither request requires a
   completed rollout, eight Ready Pods, or successful quarterly report delivery.
   Do not forge controller success as part of accepting a configuration write.
   [L1][L3][D1]
6. **Reject metadata freezing or guessed increments.** Separate resourceVersion,
   generation, observedGeneration, rollout revision, and managed fields; allow
   the actual server-managed changes without allowing unrelated desired changes.
   [A2][A3][A5][A6]
7. **Reject name-only ownership.** Use UID-qualified controller ownership and
   namespace boundaries; allow legitimate derived changes, but not an unrestricted
   same-prefix mutation allowance. [O1][O2][D1]
8. **Reject “all revisions absent” as the only successful Helm uninstall.** Default
   purge and retained-history uninstall are distinct legitimate semantics.
   Do not mark every retained revision uninstalled. [H1][H2][H3]
9. **Reject bookkeeping-only Helm success.** Require the target manifest
   Job/script cleanup as requested; a single release-Secret DELETE cannot prove
   that outcome. [L5][H2][H4][H5]
10. **Reject flat preservation of all non-manifest resources or instantaneous
    dependent deletion.** Correct Job-owned Pod GC is a legitimate consequence;
    hook/keep exceptions and background deletion timing must be modeled
    separately. [L6][H6][H7][H8][G1]

### Remaining uncertainty and deliberately unverified details

- No real cluster, benchmark run, model run, installation, or test was used.
  Runtime admission, feature gates, controller liveness, backend configuration,
  and the mock's degree of upstream fidelity remain unverified.
- The published linux-amd64 artifact was inspected, not the actual provisioned
  runner or every platform package. Registry provenance/build metadata are not
  an independent reproducible-build attestation. [C1][C2][C3]
- Numeric Quantity forms are verified at the typed decoder, not through every
  SSA/schema/client path; exact extreme-exponent/overflow behavior was not
  exhaustively characterized. [Q1][Q3][A1]
- Background GC timing, finalizer completion, future readiness, and arbitrary
  hook side effects cannot be promised from a successful parent write or ordinary
  manifest-only deletion wait. [D1][G1][H7][H8]
- Whether a benchmark intentionally requires more than these unchanged user
  requests is a product-policy question. Such additions must be named as policy,
  not attributed to Kubernetes or Helm.

## Primary-source references

### Local scope and configuration

- [L1] [OOM instruction and TOML](../../scenarios/ops/kubernetes/oomkilled-memory-limit/scenario.toml); [unchanged user instruction](../../scenarios/ops/kubernetes/oomkilled-memory-limit/instruction.md).
- [L2] OOM fixture projections: [Deployment](../../scenarios/ops/kubernetes/oomkilled-memory-limit/dataset/kubernetes/deployments.apps.json), [ReplicaSet](../../scenarios/ops/kubernetes/oomkilled-memory-limit/dataset/kubernetes/replicasets.apps.json), [Pods](../../scenarios/ops/kubernetes/oomkilled-memory-limit/dataset/kubernetes/pods.json).
- [L3] [Scaling TOML](../../scenarios/ops/kubernetes/scale-checkout-for-sale/scenario.toml); [unchanged user instruction](../../scenarios/ops/kubernetes/scale-checkout-for-sale/instruction.md).
- [L4] Scaling fixture projections: [Deployments](../../scenarios/ops/kubernetes/scale-checkout-for-sale/dataset/kubernetes/deployments.apps.json), [HPA](../../scenarios/ops/kubernetes/scale-checkout-for-sale/dataset/kubernetes/horizontalpodautoscalers.autoscaling.json).
- [L5] [Uninstall TOML](../../scenarios/ops/kubernetes/uninstall-finished-loadtest/scenario.toml); [unchanged user instruction](../../scenarios/ops/kubernetes/uninstall-finished-loadtest/instruction.md).
- [L6] Uninstall fixture projections: [release history/manifests](../../scenarios/ops/kubernetes/uninstall-finished-loadtest/dataset/kubernetes/helm_releases.json), [Jobs](../../scenarios/ops/kubernetes/uninstall-finished-loadtest/dataset/kubernetes/jobs.batch.json), [ConfigMaps](../../scenarios/ops/kubernetes/uninstall-finished-loadtest/dataset/kubernetes/configmaps.json), [Pod owner references](../../scenarios/ops/kubernetes/uninstall-finished-loadtest/dataset/kubernetes/pods.json).
- [L7] [Catalog package pin](../../catalog/services.toml#L272), [discovery implementation](../../crates/services/kubernetes/src/lib.rs#L2235), [runtime MCP configuration](../../servers/config/kubernetes/config.toml).

### Published client and commit-pinned MCP implementation

- [C1] npm's [exact wrapper-version metadata](https://registry.npmjs.org/kubernetes-mcp-server/0.1.1) and [published wrapper archive](https://registry.npmjs.org/kubernetes-mcp-server/-/kubernetes-mcp-server-0.1.1.tgz), `package/bin/index.js`.
- [C2] npm's [exact linux-amd64 metadata](https://registry.npmjs.org/kubernetes-mcp-server-linux-amd64/0.1.1).
- [C3] [Published linux-amd64 archive](https://registry.npmjs.org/kubernetes-mcp-server-linux-amd64/-/kubernetes-mcp-server-linux-amd64-0.1.1.tgz), streamed `package/bin/kubernetes-mcp-server-linux-amd64`; embedded module and VCS/build strings.
- [C4] [Commit-pinned MCP `go.mod`](https://github.com/containers/kubernetes-mcp-server/blob/26dfd9034dea17571ff34c8999f77de211f0f71a/go.mod).
- [C5] [Commit-pinned resource operations](https://github.com/containers/kubernetes-mcp-server/blob/26dfd9034dea17571ff34c8999f77de211f0f71a/pkg/kubernetes/resources.go), `ResourcesScale`, `ResourcesCreateOrUpdate`, `resourcesCreateOrUpdate`.
- [C6] [Commit-pinned Helm wrapper](https://github.com/containers/kubernetes-mcp-server/blob/26dfd9034dea17571ff34c8999f77de211f0f71a/pkg/helm/helm.go), `Uninstall`, `newAction`.
- [C7] [Commit-pinned Helm tool schema/handler](https://github.com/containers/kubernetes-mcp-server/blob/26dfd9034dea17571ff34c8999f77de211f0f71a/pkg/toolsets/helm/helm.go).

### Quantities and validation

- [J1] [RFC 8259 §6, JSON number grammar](https://www.rfc-editor.org/rfc/rfc8259.html#section-6).
- [Q1] [apimachinery v0.33.4 `quantity.go`](https://github.com/kubernetes/apimachinery/blob/v0.33.4/pkg/api/resource/quantity.go): type comment, `ParseQuantity`, JSON/schema methods, canonicalization, comparison/conversion methods.
- [Q2] [Official Kubernetes release-1.33 resource-management documentation](https://github.com/kubernetes/website/blob/release-1.33/content/en/docs/concepts/configuration/manage-resources-containers.md): memory units, requests/limits, defaulting, scheduling constraints.
- [Q3] [apimachinery v0.33.4 suffix implementation](https://github.com/kubernetes/apimachinery/blob/v0.33.4/pkg/api/resource/suffix.go).
- [Q4] [apimachinery v0.33.4 upstream Quantity tests](https://github.com/kubernetes/apimachinery/blob/v0.33.4/pkg/api/resource/quantity_test.go), particularly parse tables and `TestQuantityParseEmit`; inspected, not executed.
- [Q5] [apimachinery v0.33.4 amount/scale implementation](https://github.com/kubernetes/apimachinery/blob/v0.33.4/pkg/api/resource/amount.go).
- [Q6] [apimachinery v0.33.4 math constants and overflow helpers](https://github.com/kubernetes/apimachinery/blob/v0.33.4/pkg/api/resource/math.go).
- [Q7] [apimachinery v0.33.4 semantic equality](https://github.com/kubernetes/apimachinery/blob/v0.33.4/pkg/api/equality/semantic.go).
- [Q8] [Embedded-client apimachinery v0.37.1 Quantity implementation](https://github.com/kubernetes/apimachinery/blob/v0.37.1/pkg/api/resource/quantity.go).
- [R1] [Kubernetes v1.33.4 core validation](https://github.com/kubernetes/kubernetes/blob/v1.33.4/pkg/apis/core/validation/validation.go), `validateResourceRequirements`, `ValidateResourceQuantityValue`.

### API writes, ownership, and reconciliation

- [A1] [Kubernetes v1.33.4 generated OpenAPI](https://github.com/kubernetes/kubernetes/blob/v1.33.4/api/openapi-spec/swagger.json): Deployment and Scale PUT/PATCH paths, Quantity definition, PodSpec container-list merge metadata.
- [A2] [Official release-1.33 API concepts](https://github.com/kubernetes/website/blob/release-1.33/content/en/docs/reference/using-api/api-concepts.md): update mechanisms, media types, optimistic concurrency, opaque resource versions.
- [A3] [Kubernetes v1.33.4 Deployment/Scale storage](https://github.com/kubernetes/kubernetes/blob/v1.33.4/pkg/registry/apps/deployment/storage/storage.go), `scaleFromDeployment`, `ScaleREST.Update`, `scaleUpdatedObjectInfo.UpdatedObject`.
- [A4] [client-go v0.33.4 Deployment methods](https://github.com/kubernetes/client-go/blob/v0.33.4/kubernetes/typed/apps/v1/deployment.go), `UpdateScale`, `ApplyScale`.
- [A5] [Official release-1.33 server-side Apply documentation](https://github.com/kubernetes/website/blob/release-1.33/content/en/docs/reference/using-api/server-side-apply.md).
- [A6] [Kubernetes v1.33.4 Deployment update strategy](https://github.com/kubernetes/kubernetes/blob/v1.33.4/pkg/registry/apps/deployment/strategy.go), `PrepareForUpdate`, `AllowUnconditionalUpdate`.
- [A7] [apiserver v0.33.4 generic storage](https://github.com/kubernetes/apiserver/blob/v0.33.4/pkg/registry/generic/registry/store.go), conditional/unconditional update handling.
- [D1] [Official release-1.33 Deployment documentation](https://github.com/kubernetes/website/blob/release-1.33/content/en/docs/concepts/workloads/controllers/deployment.md): desired state, template-triggered rollout, scaling, revision history, paused state, progress and failure.
- [O1] [apimachinery v0.33.4 metadata types](https://github.com/kubernetes/apimachinery/blob/v0.33.4/pkg/apis/meta/v1/types.go), `OwnerReference`.
- [O2] [Kubernetes v1.33.4 controller-reference manager](https://github.com/kubernetes/kubernetes/blob/v1.33.4/pkg/controller/controller_ref_manager.go), `BaseControllerRefManager.ClaimObject`.
- [G1] [Official release-1.33 garbage-collection documentation](https://github.com/kubernetes/website/blob/release-1.33/content/en/docs/concepts/architecture/garbage-collection.md): owner namespace rules, foreground/background cascade, finalizers, orphans.

### Helm v3.22.1

- [H1] [Versioned uninstall CLI](https://github.com/helm/helm/blob/v3.22.1/cmd/helm/uninstall.go), flag defaults and validation.
- [H2] [Versioned uninstall action](https://github.com/helm/helm/blob/v3.22.1/pkg/action/uninstall.go), `Run`, `purgeReleases`, `deleteRelease`, cascade selection.
- [H3] [Versioned release-status definitions](https://github.com/helm/helm/blob/v3.22.1/pkg/release/status.go).
- [H4] [Versioned action configuration/storage-driver selection](https://github.com/helm/helm/blob/v3.22.1/pkg/action/action.go).
- [H5] [Versioned release storage/key construction](https://github.com/helm/helm/blob/v3.22.1/pkg/storage/storage.go).
- [H6] [Versioned manifest retention-policy filter](https://github.com/helm/helm/blob/v3.22.1/pkg/action/resource_policy.go).
- [H7] [Versioned hook execution/deletion policies](https://github.com/helm/helm/blob/v3.22.1/pkg/action/hooks.go).
- [H8] [Versioned deletion waiter](https://github.com/helm/helm/blob/v3.22.1/pkg/kube/wait.go), `waitForDeletedResources`.
- [H9] [Versioned Kubernetes client](https://github.com/helm/helm/blob/v3.22.1/pkg/kube/client.go), `WaitForDelete`, `DeleteWithPropagationPolicy`.
