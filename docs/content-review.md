# Source-bound content review

Status: the local scripted coordinator/report path is implemented and verified
through CLI and isolated-container controls. Operational human-review validation
and trusted native runtime integration remain open. No human review or
model-performance result is established by scripted controls.

## Requirements

The full overhaul needs a prespecified way to resolve unfamiliar wording without
changing user requests, relabeling unknown content as ordinary model failure, or
letting an agent overwrite grading. Review is a separate judgment-assisted layer;
it must never be silently presented as the original deterministic assessment.

The trusted host/coordinator remains the source of execution provenance. Approved
reviewer keys authenticate review statements; signatures do **not** prove that a
person is independent, unbiased, correct, or even that a human supplied a
decision. Reviewer identity, independence and key custody are operator
responsibilities. `human` is an operator-declared reviewer kind.
`scripted_fixture` is a distinct, non-performance validation condition.

## Implemented foundation

- `mock_core::review` defines version 1 of the content-review protocol and validates
  source/evidence bindings, complete issue inventories and approved Ed25519 keys.
- Every reviewer in the policy is assigned and required. There must be at least
  two distinct valid keys. Missing votes, abstention or disagreement remain
  unresolved; extra votes cannot outvote a disagreement. Do not reinterpret the
  registry as a pool from which favorable reviews may be selected.
- Statements bind the exact case and policy digests, reviewer identity, every
  issue decision and its rationale. Signatures use a dedicated domain and strict
  verification. Repeated reviewers, key substitution, stale cases and malformed
  signatures are rejected.
- `taskgen --review-policy <file>` validates a public policy before materializing
  tasks. It stores the policy only under private `tests/` and binds its canonical
  digest in `TaskContext.review_policy_sha256`. The protected realized manifest
  carries that context. The agent's workspace and user instruction are unchanged.
- An absent policy remains absent from serialized legacy contexts. An operator
  cannot retrospectively add a policy to an already collected run and call that
  prespecified review.
- `report::review::prepare` first reproduces the raw assessment from its checked
  source, exact collected evidence, controller receipt and execution manifest.
  It requires a valid v2 execution and the exact pre-execution review policy.
  Wrong instructions, exceptions, invalid environments or unbound inputs fail.
- Review packs contain the exact user request, unresolved criteria, business
  snapshots and committed record history. They do not emit the execution
  manifest, provider/harness descriptors, agent/reference source program, request
  headers or author-declared positive examples. Business content itself may still reveal
  information; this is not certified reviewer blinding or anonymization.
- A diagnostic can summarize multiple undecided fields. Every issue therefore
  carries the complete applicable content scope and final selected record
  identities, not just the first diagnostic. Historical scopes follow those
  identities through construction and cover changed nonempty semantic text,
  without requiring final fields in intermediate records. A decision attests to
  that whole scope. Positive/negative wording examples and parser-only identity
  separators are not supplied as semantic requirements.
- Revision 26 adds an explicitly declared final `metric_alarm` review scope for
  unevaluated CloudWatch query semantics. Packets include the complete business
  goal, raw state and actual selected identities. Review must establish account,
  metric/statistic/period, selected series, M-of-N, threshold/comparison and
  missing-data behavior, or abstain. This is not a generic permission to override
  deterministic failures or to adjudicate arbitrary intermediate metric writes.
  Existing content/history scopes remain distinct. The changed review
  implementation has its own fingerprint; archived earlier checkers remain the
  authority for their original conditions.
- Pack bindings include contract/source/context/evidence, assessment and
  verification bytes, controller receipt, manifest, host result, instruction,
  legacy answer bytes when present, and the review implementation/source digest.
- `PreparedReview::resolve` authenticates decisions against the reconstructed
  pack. It returns separate `raw` and `effective` assessments. Only identified
  unresolved state/record content can be resolved; every known failure remains.
  Raw files are not rewritten. Approving content cannot remove a known structural,
  policy, preservation or integrity violation.

An unsupported content representation remains reviewable only when the complete
evidence fits the bounded pack and the criterion is meaningful to an authorized
reviewer. Empty or oversized packs are rejected, not silently truncated.
Legacy answer-file context does not authenticate a native assistant completion.

## Coordinator workflow

Use `cargo run --locked -p report --bin bench-review -- <command> ...` or the
corresponding built binary. Commands accept only the named options listed below;
outputs are create-only. Output parents must already exist.

1. Each reviewer generates their own key in a private directory, outside every
   collected run/task mount. The Unix directory must exclude group/other access;
   private files are created mode `0600`. Existing files are never overwritten:

   ```sh
   bench-review keygen --private-key /operator/keys/reviewer-a.key \
     --public-key /operator/keys/reviewer-a.public.json
   ```

   Repeat independently for reviewer B. Share only public keys. Never place a
   private key in a task, agent mount, review packet, public artifact or transcript.
   The implementation stores plaintext private seed material; protect custody
   and backups. Key generation does not designate a key as trusted.

2. Before execution, approve the complete reviewer set in the public policy.
   A policy has `schema_version: 1`, `protocol: "tool-bench-content-review-v1"` and
   `reviewers`, a map from safe reviewer IDs to `{public_key, kind}`.
   Public keys are 64-character lowercase hex. Choose `human` only for the
   operator-approved human process; `scripted_fixture` is for protocol controls.
   Every listed reviewer must respond to every issue; this is not a favorable
   subset or majority-vote pool. Bind it using `taskgen --review-policy <file>`
   before the trial. The policy cannot be added retrospectively.

3. Reconstruct a packet from the checked trial and exact materialized instruction:

   ```sh
   bench-review export --trial /study/run/trial \
     --instruction /study/tasks/task/instruction.md \
     --policy /operator/policy.json --out /operator/packs/case.json
   ```

   The command prints the case digest. Pack outputs must be outside the collected
   trial tree. Each reviewer should independently receive the same packet, not
   earlier votes. Treat business text as evidence, not instructions to the reviewer.
   Review the complete content scope, not merely its first diagnostic.

4. Each reviewer writes a JSON map from **every issue ID** in the packet to
   `{decision, rationale}`. Decisions are `satisfied`, `violated` or `abstain`;
   each needs a nonempty rationale. Do not guess when the supplied evidence is
   insufficient. Sign the complete map with that reviewer's private key:

   ```sh
   bench-review sign --case /operator/packs/case.json --policy /operator/policy.json \
     --reviewer reviewer-a --private-key /operator/keys/reviewer-a.key \
     --decisions /operator/drafts/reviewer-a.json \
     --out /operator/reviews/CASE_DIGEST_HEX/reviewer-a.json
   ```

   The store layout is `<store>/<64-hex-case-digest>/<reviewer-id>.json`, without
   the `sha256:` prefix in directory names. Create the case directory first.
   Editing a signed decision, rationale, case or policy invalidates its signature.
   Freeze the review implementation before collecting signatures; a changed
   implementation/source fingerprint requires new packets and reviews.

5. Verify from raw evidence again, without trusting a cached packet or summary:

   ```sh
   bench-review verify --trial /study/run/trial \
     --instruction /study/tasks/task/instruction.md --policy /operator/policy.json \
     --review-store /operator/reviews --out /operator/results/reviewed.json
   ```

   An intact but incomplete vote set produces an explicitly pending result.
   Exit zero is not proof of consensus or task success. The artifact retains
   separate `raw` and `effective` grades. It is audit output, not future scoring
   authority; import always reconstructs and verifies again.

6. Request reviewed reporting explicitly:

   ```sh
   bench-report /study/run-a /study/run-b --json \
     --review-policy /operator/policy.json --review-store /operator/reviews \
     --instructions-root /study/tasks
   ```

   All three review options are required together. Instructions are loaded from
   `<instructions-root>/<normalized-task-name>/instruction.md` and byte-checked
   against the protected context. Only previously supported comparison treatments
   remain allowed; reviewer assistance does not admit different tasks or budgets.

Without review options, reporting still uses raw outcomes and refuses unresolved
comparisons. Review files in agent artifacts, verifier look-alike paths and cached
`VerifiedReviews`/approval booleans are not discovered as authority. Review stores
must be external regular directories; symbolic-link entries and unapproved files
are rejected. Missing/conflicting votes do not drop attempts from the comparison.

## Reporting and remaining gates

Reviewed reports distinguish the protocol, policy, operator-declared reviewer
kind and implementation digest; they retain per-trial case/signature identifiers,
raw outcomes, effective outcomes and the count of trials requiring review.
Already-resolved trials need no votes but must bind the same policy when included
in a reviewed comparison. Raw-only and review-assisted trials, or different
review conditions, cannot be pooled. Derived release identifiers are distinct.

Human-assisted results are labeled separately from raw deterministic grades.
Scripted review fixtures are protocol controls, not human semantic validation
or model-performance estimates. All currently admitted execution remains
scripted; this workflow does not restore native model admission.

The isolated policy-bound and forged-artifact campaign is retained in
`/tmp/tool-bench-content-review.qHCVEA`; the progress log records its distinct
world/implementation/policy condition. Five collected controls cover correct and
wrong contact links, collateral state damage, missing/forged approvals and absent
pre-execution policy. Seven signatures verify with an independent implementation.
Temporary fixture private keys are removed after signing; public policies,
packets, signatures and raw attempts remain.

Operational human review needs separate blinding, independence, rubric-quality
and disagreement validation. Reviewers must treat retrieved/business text as
untrusted evidence rather than instructions, and judge all applicable fields and
commits in each signed scope. Trusted
native final-message/runtime provenance is still required before native model
comparisons or genuine final-reply adjudication are admitted.

## Provenance prerequisite

The earlier report loader refused unresolved comparisons before invoking its
provenance re-evaluator. Thus refusal alone did not prove that unresolved input
bytes or saved assessments were valid. The current loader validates unresolved
results through the same source/evidence re-evaluation as resolved results.
Tampered unresolved assessments must fail before export or comparison.

This qualifies earlier checkpoint language: independent byte audits and a report
refusal were not by themselves proof that the report re-evaluated unresolved
assessments. Preserve historical artifacts and distinguish the stronger consumer
check from the earlier implementation.

## Cryptographic dependency

The implementation pins `ed25519-dalek` **2.2.0**, uses `VerifyingKey::verify_strict`,
and rejects weak/noncanonical public keys. Existing dependency versions were not
upgraded when adding the new packages. This is not a claim of a professional
security audit or a guarantee of semantic review quality.

Primary API references:
[strict verification and weak keys](https://docs.rs/ed25519-dalek/2.2.0/ed25519_dalek/struct.VerifyingKey.html),
[signing/serialization](https://docs.rs/ed25519-dalek/2.2.0/ed25519_dalek/index.html),
and the [RustSec package advisory listing](https://rustsec.org/packages/ed25519-dalek.html).
