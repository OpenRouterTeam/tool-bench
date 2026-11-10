# Kubernetes namespace/identity seam — 2026-11-06

## Scope and evidence boundaries

This supplements, rather than repeats,
`2026-11-06-kubernetes-operation-contract.md`: only request-namespace
normalization, empty versus stale update metadata, and Helm's preliminary
uninstall storage update are considered.

**Versioned implementation** means the cited Kubernetes `v1.33.4`,
apiserver/apimachinery `v0.33.4`, or Helm `v3.22.1` source, not observed
real-cluster behavior. **Documented** identifies upstream API comments.
**Inference** identifies conclusions drawn from those implementations.
**Local evidence** identifies the two supplied scripted prerelease artifacts
and the task's accompanying run summary. No installations, tests, Docker,
cluster/cloud requests, benchmark execution, or model trials were performed.

## 1. Namespace is normalized against the request, not required on the wire

**Versioned implementation:** Secrets and Deployments are namespace-scoped.
For an otherwise-valid request routed to namespace `N`, the shared namespace
helper behaves as follows. [K1][K2][N1]

| Decoded object's `metadata.namespace` | Result at the namespace check |
| --- | --- |
| Omitted or `""` | Set to request namespace `N`. |
| Exactly `N` | Preserve it. |
| A different nonempty namespace | Reject with `BadRequest` / HTTP 400; do not reroute the write. |

The omitted/empty equivalence follows from the non-pointer metadata field.
Upstream's version-pinned helper test explicitly covers empty, matching, and
conflicting namespaces. The helper's expected namespace is the URL/request
namespace: a request to `.../namespaces/perf/...` is not redirected to
`default` merely because the body omits namespace. [M1][N1][N2][E1]

**Versioned implementation:** POST and PUT handlers invoke this helper after
decoding; PATCH invokes it on the resulting candidate object. Generic
`BeforeCreate` and `BeforeUpdate` also enforce namespace consistency before
common validation. Consequently, reading the metadata validator's
namespace-required check in isolation misses earlier defaulting. [N3][N4][P1][N5][U1][M2]

**Inference:** identity validation of a Helm storage Secret should compare its
embedded release namespace against the **normalized effective namespace**.
Comparing against an empty raw-body namespace rejects a permitted representation;
silently replacing a conflicting nonempty namespace would instead weaken the
upstream check. [N1][N4]

## 2. PUT metadata: absent conditions are not stale conditions

**Documented:** UID and resourceVersion are system-populated metadata; clients
must treat resourceVersion as opaque. Both are non-pointer fields, so omitted
and explicitly empty values decode to the same values here. [M1]

**Versioned implementation:** both Secret and Deployment strategies permit
unconditional updates. For an existing, otherwise-valid object, PUT has these
distinct cases. [K1][K2]

| Submitted metadata | Version-pinned handling |
| --- | --- |
| UID omitted or `""` | No UID precondition is extracted; `BeforeUpdate` restores the existing UID. [U1] |
| Nonempty UID equal to the existing UID | A UID storage precondition is extracted and matches. [U1][S1] |
| Nonempty UID different from the existing UID | The ordinary PUT storage precondition fails; its update error maps to HTTP 409 Conflict. [N4][U1][G1][S1][S2][S3][E1] |
| resourceVersion omitted or `""` | The versioner yields zero; the unconditional-update branch installs the current stored version before update validation. [V1][G1] |
| Current nonzero resourceVersion | The version check matches. [G1] |
| A genuinely stale, nonzero resourceVersion | The version check returns HTTP 409 Conflict rather than overwriting the object. [G1][E1] |

**Versioned implementation:** the common metadata validator does require a
resourceVersion and enforce UID immutability, but it runs after the store's
unconditional-version hydration and the UID restoration above. Those checks
do not make empty metadata a stale precondition. [M2][G1][U1]

**Important edge:** the pinned versioner also parses the nonempty string `"0"`
as zero, reaching the unconditional branch. Therefore “reject every nonempty
resourceVersion that differs” is not an exact description of this implementation.
This is not advice for clients to manufacture resource versions; malformed
strings are not established valid stale-version examples either. [V1][G1][M1]

**Inference:** empty UID does not cancel a stale resourceVersion, and empty
resourceVersion does not cancel a conflicting nonempty UID. Keep those
conditions independent rather than either requiring all metadata or ignoring
all server-managed fields. [U1][G1][S1]

**Precondition-type boundary:** this concerns ObjectMeta. A non-nil explicit
storage `Preconditions.UID` or `.ResourceVersion` pointer is checked even when
its pointed-to value is empty; it is not equivalent to a missing pointer.
Do not transfer the ObjectMeta omission rule to explicit precondition objects.
[S1]

**POST boundary:** create does not use an incoming UID as an existing-object
condition: the handler clears system fields and storage assigns a fresh UID.
The pinned etcd create implementation rejects a nonzero incoming resourceVersion.
These are not PUT's optimistic-concurrency semantics. [N3][N1][G1][S3]

## 3. PATCH omission is evaluated through the candidate object

**Versioned implementation:** JSON/merge patch and strategic merge patch start
from the current object; SSA uses the field manager. All then reach the shared
candidate namespace check and registry update path. Leaving metadata out of a
partial patch is therefore not the same operation as submitting an empty
replacement object. [P1]

**Inference from that call chain:** ordinary patches that leave UID and
resourceVersion untouched inherit them from the live object. If a valid patch
instead produces empty metadata fields, namespace normalization, UID restoration,
and the permitted unconditional-version branch still apply. A stale nonzero
version retained in the resulting object still conflicts. [P1][N1][U1][G1]

**Error-path distinction:** PATCH constructs `DefaultUpdatedObjectInfo(nil, ...)`,
so it does not extract PUT's upfront UID precondition. An ordinary update patch
that leaves a conflicting nonempty UID in the candidate instead reaches immutable
UID validation: `Invalid` / HTTP 422, assuming no earlier failure. Thus rejection
is required, but “all stale UIDs always return 409” overstates the contract.
JSON Patch tests can also fail before these metadata checks. [P1][U1][M2][E1]

**Uncertainty:** these common paths do not prove every arbitrary SSA payload,
ownership configuration, schema, or admission policy reaches the same error
first. SSA merging can fail earlier; no universal SSA error-order or ownership
claim is needed to establish this seam. [P1]

## 4. Helm can succeed after the preliminary release update fails

**Versioned implementation:** Helm's storage `Update` forwards to its driver.
The Secret driver builds a new Secret with name, labels, type, and encoded
release data, but no namespace, UID, or resourceVersion, then calls its injected
Secret client's `Update`. It does not first GET and copy those metadata fields.
That representation is consistent with the namespace/defaulting and
unconditional Secret-update paths above. [H1][H2][N1][K1][U1][G1]

**Versioned implementation, Helm `Uninstall.Run`:**

1. It marks the in-memory release `uninstalling`, sets its deletion time, and
   attempts `Releases.Update`. That error is **logged only**: it is neither
   returned nor appended to the later error list. [H3]
2. It proceeds to resource deletion, any requested deletion wait, and post-delete
   hooks. It then marks the in-memory release `uninstalled`. [H3]
3. With `KeepHistory=false`, it purges the loaded revisions. If deletion, applicable
   wait/hooks, and purge succeed, it returns the response with **nil error**,
   despite the failed preliminary update. Deletion or accumulated wait/hook/purge
   errors can still fail the action. [H3]

**Inference:** a successful uninstall response does not prove the intermediate
`uninstalling` Secret write succeeded. That success is compatible with the
observed 400; it does not make an incorrect mock rejection conformant. [H3][L1][L3]

## 5. What the supplied prerelease evidence establishes

- **Local evidence:** the failure artifact records the path
  `/api/v1/namespaces/perf/secrets/sh.helm.release.v1.checkout-loadtest.v2`,
  status 400, and message `release identity differs from its storage Secret`.
  Its request-body string begins with the Kubernetes protobuf framing
  `k8s\u0000`. [L1]
- **Local evidence:** the decoded release artifact identifies
  `checkout-loadtest`, namespace `perf`, revision 2, with status `uninstalling`.
  It is the intended intermediate release, not evidence of a persisted final
  Secret state. [L2]
- **Supplied run summary, not independently reverified here:** this was the
  native protobuf Secret PUT with no nonempty namespace, and the tool still
  reported successful uninstall/purged resources. The two JSON files alone do
  not contain the complete tool-result transcript or a final resource inventory.
  The saved protobuf-as-text also contains replacement characters, so it is not
  a lossless wire capture from which to prove every field's exact encoding.
  In particular, “omitted” here must not imply proof that a zero-length field
  was absent from the wire rather than semantically empty. [L1][L2][L3]

**Narrow objection/recommendation:** do not reject this native update solely for
empty Secret metadata namespace/UID/resourceVersion. Normalize namespace from the
request, preserve identity, and apply the relevant conditions only when present.
Continue rejecting conflicting nonempty namespace/UID and genuinely stale
nonzero resourceVersion. Which precise mock comparison produced this particular
message is not proven by these two artifacts alone. [N1][U1][G1][L1][L2]

**Remaining uncertainty:** no real-cluster conformance or model trial is claimed.
The evidence establishes a local scripted failure plus an upstream explanation
for why a failed preliminary update need not fail Helm uninstall; it does not
independently certify final purge, actual cluster admission, or every client
transport/platform. [L1][L2][L3][H3]

## Primary-source references

### Kubernetes: exact version pins

- [K1] [Kubernetes v1.33.4 Secret strategy](https://github.com/kubernetes/kubernetes/blob/v1.33.4/pkg/registry/core/secret/strategy.go#L51): `NamespaceScoped`, `AllowUnconditionalUpdate`.
- [K2] [Kubernetes v1.33.4 Deployment strategy](https://github.com/kubernetes/kubernetes/blob/v1.33.4/pkg/registry/apps/deployment/strategy.go#L61): `NamespaceScoped`; `AllowUnconditionalUpdate` at line 169.
- [N1] [apiserver v0.33.4 metadata helpers](https://github.com/kubernetes/apiserver/blob/v0.33.4/pkg/registry/rest/meta.go#L29): system-field clearing/filling, namespace normalization, expected namespace.
- [N2] [apiserver v0.33.4 namespace-helper tests](https://github.com/kubernetes/apiserver/blob/v0.33.4/pkg/registry/rest/meta_test.go#L75): inspected, not executed.
- [N3] [apiserver v0.33.4 POST handler](https://github.com/kubernetes/apiserver/blob/v0.33.4/pkg/endpoints/handlers/create.go#L165): system-field clearing and namespace normalization.
- [N4] [apiserver v0.33.4 PUT handler](https://github.com/kubernetes/apiserver/blob/v0.33.4/pkg/endpoints/handlers/update.go#L142): namespace check; `DefaultUpdatedObjectInfo(obj, ...)` at line 211.
- [N5] [apiserver v0.33.4 `BeforeCreate`](https://github.com/kubernetes/apiserver/blob/v0.33.4/pkg/registry/rest/create.go#L111): namespace normalization before validation.
- [P1] [apiserver v0.33.4 PATCH handler](https://github.com/kubernetes/apiserver/blob/v0.33.4/pkg/endpoints/handlers/patch.go#L323): JSON/merge patch at lines 323–431, strategic merge at 446–466, SSA at 500–537, candidate namespace check at 609–619, nil initial update object at 697–706.
- [U1] [apiserver v0.33.4 update preparation/preconditions](https://github.com/kubernetes/apiserver/blob/v0.33.4/pkg/registry/rest/update.go#L107): UID restoration before validation; `Preconditions` at lines 186–202.
- [G1] [apiserver v0.33.4 generic registry](https://github.com/kubernetes/apiserver/blob/v0.33.4/pkg/registry/generic/registry/store.go#L628): precondition forwarding, zero/nonzero version handling, then `BeforeUpdate`; create UID initialization at lines 480–484.
- [V1] [apiserver v0.33.4 object versioner](https://github.com/kubernetes/apiserver/blob/v0.33.4/pkg/storage/api_object_versioner.go#L73): `ObjectResourceVersion`.
- [S1] [apiserver v0.33.4 storage preconditions](https://github.com/kubernetes/apiserver/blob/v0.33.4/pkg/storage/interfaces.go#L121): pointer types and `Check`.
- [S2] [apiserver v0.33.4 update-error translation](https://github.com/kubernetes/apiserver/blob/v0.33.4/pkg/storage/errors/storage.go#L76): invalid storage object/precondition becomes Conflict.
- [S3] [apiserver v0.33.4 etcd store](https://github.com/kubernetes/apiserver/blob/v0.33.4/pkg/storage/etcd3/store.go#L249): create version check at 261–262; `GuaranteedUpdate` precondition execution at 473–491.
- [M1] [apimachinery v0.33.4 ObjectMeta API declarations/comments](https://github.com/kubernetes/apimachinery/blob/v0.33.4/pkg/apis/meta/v1/types.go#L136): namespace, UID, resourceVersion.
- [M2] [apimachinery v0.33.4 metadata validation](https://github.com/kubernetes/apimachinery/blob/v0.33.4/pkg/api/validation/objectmeta.go#L173): namespace requirement; update resourceVersion/immutability checks at 242–255.
- [E1] [apimachinery v0.33.4 API errors](https://github.com/kubernetes/apimachinery/blob/v0.33.4/pkg/api/errors/errors.go#L232): Conflict 409, Invalid 422, BadRequest 400.

### Helm: exact version pin

- [H1] [Helm v3.22.1 release-storage forwarding](https://github.com/helm/helm/blob/v3.22.1/pkg/storage/storage.go#L72): `Storage.Update`.
- [H2] [Helm v3.22.1 Secret driver](https://github.com/helm/helm/blob/v3.22.1/pkg/storage/driver/secrets.go#L173): `Update`; new metadata construction at lines 249–255.
- [H3] [Helm v3.22.1 uninstall action](https://github.com/helm/helm/blob/v3.22.1/pkg/action/uninstall.go#L103): preliminary update at 119–121, deletion/wait/hooks, purge/error handling at 155–167, `purgeReleases` at 193–199.

### Local, read-only evidence

- [L1] `/tmp/tool-bench-e27-helm-failed-update.json`: failed request record, read only.
- [L2] `/tmp/tool-bench-e27-native-release-update.json`: decoded intermediate release, read only.
- [L3] This research task's accompanying evidence description: native PUT namespace omission and reported successful uninstall/purge; not a separate final-state artifact.
