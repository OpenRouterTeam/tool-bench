use std::{collections::BTreeMap, fs};

use ed25519_dalek::{Signer, SigningKey};
use mock_core::{
    grading::{CheckDisposition, CheckKind, Outcome},
    review::{
        ApprovedReviewer, ContentDecision, IssueDecision, REVIEW_PROTOCOL, ReviewCase,
        ReviewPolicy, ReviewStatement, ReviewerKind, SignedReview, hex, signing_bytes,
    },
};
use report::review::prepare;
use serde_json::json;

mod support;

fn policy() -> ReviewPolicy {
    ReviewPolicy {
        schema_version: 1,
        protocol: REVIEW_PROTOCOL.into(),
        reviewers: [("first", 1), ("second", 2)]
            .into_iter()
            .map(|(name, seed)| {
                let key = SigningKey::from_bytes(&[seed; 32]);
                (
                    name.into(),
                    ApprovedReviewer {
                        public_key: hex(&key.verifying_key().to_bytes()),
                        kind: ReviewerKind::ScriptedFixture,
                    },
                )
            })
            .collect(),
    }
}

fn sign(
    case: &ReviewCase,
    policy: &ReviewPolicy,
    name: &str,
    seed: u8,
    decision: ContentDecision,
) -> SignedReview {
    let statement = ReviewStatement {
        schema_version: 1,
        protocol: REVIEW_PROTOCOL.into(),
        case_sha256: case.sha256(policy).unwrap(),
        policy_sha256: policy.sha256().unwrap(),
        reviewer: name.into(),
        decisions: case
            .issues
            .keys()
            .map(|identity| {
                (
                    identity.clone(),
                    IssueDecision {
                        decision,
                        rationale:
                            "Scripted control of authenticated resolution, not a human assessment."
                                .into(),
                    },
                )
            })
            .collect(),
    };
    SignedReview {
        signature: hex(&SigningKey::from_bytes(&[seed; 32])
            .sign(&signing_bytes(&statement))
            .to_bytes()),
        statement,
    }
}

#[test]
fn authenticated_resolution_is_separate_from_the_immutable_original_grade() {
    for decision in [ContentDecision::Satisfied, ContentDecision::Violated] {
        let directory = tempfile::tempdir().unwrap();
        let policy = policy();
        support::write_reviewable(
            directory.path(),
            "trial",
            "example__github__basic",
            &policy,
            false,
        );
        let trial = directory.path().join("trial");
        let original = fs::read(trial.join("verifier/assessment.json")).unwrap();
        let prepared = prepare(&trial, "instruction", &policy).unwrap();
        let reviews = [
            sign(prepared.case(), &policy, "first", 1, decision),
            sign(prepared.case(), &policy, "second", 2, decision),
        ];
        let reviewed = prepared.resolve(&policy, &reviews).unwrap();
        assert_eq!(reviewed.raw.outcome, Outcome::Unresolved);
        assert_eq!(reviewed.raw.reward, 0.0);
        assert!(reviewed.raw.adjudication_needed);
        assert!(!reviewed.effective.adjudication_needed);
        assert_eq!(
            reviewed.effective.outcome,
            if decision == ContentDecision::Satisfied {
                Outcome::Pass
            } else {
                Outcome::Fail
            }
        );
        assert_eq!(
            reviewed.reviews.reviewer_kind,
            ReviewerKind::ScriptedFixture
        );
        assert_eq!(reviewed.signed_review_sha256.len(), 2);
        assert_eq!(
            fs::read(trial.join("verifier/assessment.json")).unwrap(),
            original
        );
        assert_eq!(
            report::load_trials(&trial).unwrap()[0].outcome,
            report::Outcome::Unresolved
        );
    }
}

#[test]
fn source_bound_answer_content_reviews_cannot_rewrite_factual_binding_or_raw_bytes() {
    for decision in [ContentDecision::Satisfied, ContentDecision::Violated] {
        let directory = tempfile::tempdir().unwrap();
        let policy = policy();
        support::write_sentry_answer_reviewable(
            directory.path(),
            "trial",
            "example__sentry__dsn",
            &policy,
            false,
        );
        let trial = directory.path().join("trial");
        let original = fs::read(trial.join("verifier/assessment.json")).unwrap();
        let answer = fs::read(trial.join("artifacts/agent/answer.md")).unwrap();
        let prepared = prepare(&trial, "instruction", &policy).unwrap();
        let issue = prepared.case().issues.values().next().unwrap();
        assert_eq!(issue.check, CheckKind::Answer);
        assert_eq!(issue.location, "answer");
        assert_eq!(issue.scope["kind"], "answer_content");
        assert_eq!(
            issue.scope["declarations"]["sentry_dsn"]["factual_binding"],
            "passed_by_raw_evaluation_not_reviewable"
        );
        assert!(prepared.case().bindings.contains_key("answer"));
        assert_eq!(
            prepared.case().context.legacy_answer_file.as_deref(),
            Some(std::str::from_utf8(&answer).unwrap())
        );
        let reviews = [
            sign(prepared.case(), &policy, "first", 1, decision),
            sign(prepared.case(), &policy, "second", 2, decision),
        ];
        let reviewed = prepared.resolve(&policy, &reviews).unwrap();
        assert_eq!(reviewed.raw.outcome, Outcome::Unresolved);
        assert!(!reviewed.raw.answer_ok);
        assert_eq!(
            reviewed.effective.answer_ok,
            decision == ContentDecision::Satisfied
        );
        assert_eq!(
            reviewed.effective.outcome,
            if decision == ContentDecision::Satisfied {
                Outcome::Pass
            } else {
                Outcome::Fail
            }
        );
        assert_eq!(
            fs::read(trial.join("verifier/assessment.json")).unwrap(),
            original
        );
        assert_eq!(
            fs::read(trial.join("artifacts/agent/answer.md")).unwrap(),
            answer
        );
    }
    let directory = tempfile::tempdir().unwrap();
    let policy = policy();
    support::write_sentry_answer_reviewable(
        directory.path(),
        "trial",
        "example__sentry__dsn",
        &policy,
        true,
    );
    let trial = directory.path().join("trial");
    assert!(prepare(&trial, "instruction", &policy).is_err());
}

#[test]
fn oncall_review_requires_actual_factual_binding_and_keeps_raw_failures_and_answer_bytes() {
    for known_failure in [false, true] {
        let directory = tempfile::tempdir().unwrap();
        let policy = policy();
        support::write_oncall_answer_reviewable(
            directory.path(),
            "trial",
            "example__grafana__oncall",
            &policy,
            known_failure,
        );
        let trial = directory.path().join("trial");
        let original = fs::read(trial.join("verifier/assessment.json")).unwrap();
        let answer = fs::read(trial.join("artifacts/agent/answer.md")).unwrap();
        let prepared = prepare(&trial, "instruction", &policy).unwrap();
        let issue = prepared.case().issues.values().next().unwrap();
        let declaration = &issue.scope["declarations"]["oncall"];
        assert_eq!(issue.scope["kind"], "answer_content");
        assert_eq!(
            declaration["factual_binding"],
            "must_be_established_by_review_not_passed_by_raw_evaluation"
        );
        assert_eq!(
            declaration["schedule_records"]["SCK1PRIM"]["on_call_now"],
            json!(["UZ9D6K3L"])
        );
        assert_eq!(
            declaration["final_schedule_identities"],
            json!(["SCK1PRIM"])
        );
        assert_eq!(
            declaration["directory_records"]["UZ9D6K3L"]["name"],
            "Sam Brennan"
        );
        assert!(issue.criterion.contains("did NOT establish"));
        assert!(prepared.case().bindings.contains_key("answer"));
        let reviewed = prepared
            .resolve(
                &policy,
                &[
                    sign(
                        prepared.case(),
                        &policy,
                        "first",
                        1,
                        ContentDecision::Satisfied,
                    ),
                    sign(
                        prepared.case(),
                        &policy,
                        "second",
                        2,
                        ContentDecision::Satisfied,
                    ),
                ],
            )
            .unwrap();
        assert_eq!(
            reviewed.effective.outcome,
            if known_failure {
                Outcome::Fail
            } else {
                Outcome::Pass
            }
        );
        assert_eq!(
            fs::read(trial.join("verifier/assessment.json")).unwrap(),
            original
        );
        assert_eq!(
            fs::read(trial.join("artifacts/agent/answer.md")).unwrap(),
            answer
        );
    }
}

#[test]
fn content_approvals_cannot_clear_a_known_structural_failure() {
    let directory = tempfile::tempdir().unwrap();
    let policy = policy();
    support::write_reviewable(
        directory.path(),
        "trial",
        "example__github__basic",
        &policy,
        true,
    );
    let prepared = prepare(&directory.path().join("trial"), "instruction", &policy).unwrap();
    let reviewed = prepared
        .resolve(
            &policy,
            &[
                sign(
                    prepared.case(),
                    &policy,
                    "first",
                    1,
                    ContentDecision::Satisfied,
                ),
                sign(
                    prepared.case(),
                    &policy,
                    "second",
                    2,
                    ContentDecision::Satisfied,
                ),
            ],
        )
        .unwrap();
    assert_eq!(reviewed.raw.outcome, Outcome::Fail);
    assert_eq!(reviewed.effective.outcome, Outcome::Fail);
    assert_eq!(reviewed.effective.reward, 0.0);
    assert!(!reviewed.effective.state_ok);
    assert!(reviewed.effective.records_ok);
    let known: Vec<_> = reviewed
        .raw
        .failures
        .iter()
        .filter(|failure| failure.disposition == CheckDisposition::Failed)
        .cloned()
        .collect();
    assert!(!known.is_empty());
    assert_eq!(reviewed.effective.failures, known);
}

#[test]
fn missing_conflicting_and_abstaining_reviews_do_not_produce_a_final_decision() {
    let directory = tempfile::tempdir().unwrap();
    let policy = policy();
    support::write_reviewable(
        directory.path(),
        "trial",
        "example__github__basic",
        &policy,
        false,
    );
    let prepared = prepare(&directory.path().join("trial"), "instruction", &policy).unwrap();
    let first = sign(
        prepared.case(),
        &policy,
        "first",
        1,
        ContentDecision::Satisfied,
    );
    for reviews in [
        vec![],
        vec![first.clone()],
        vec![
            first.clone(),
            sign(
                prepared.case(),
                &policy,
                "second",
                2,
                ContentDecision::Violated,
            ),
        ],
        vec![
            first,
            sign(
                prepared.case(),
                &policy,
                "second",
                2,
                ContentDecision::Abstain,
            ),
        ],
    ] {
        let reviewed = prepared.resolve(&policy, &reviews).unwrap();
        assert_eq!(reviewed.effective.outcome, Outcome::Unresolved);
        assert!(reviewed.effective.adjudication_needed);
        assert_eq!(reviewed.effective.reward, 0.0);
    }
}

#[test]
fn later_policy_selection_wrong_instructions_and_resolved_trials_are_not_reviewable() {
    let directory = tempfile::tempdir().unwrap();
    let policy = policy();
    support::write_unresolved(directory.path(), "legacy", "example__github__basic");
    assert!(prepare(&directory.path().join("legacy"), "instruction", &policy).is_err());
    support::write_reviewable(
        directory.path(),
        "bound",
        "example__github__basic",
        &policy,
        false,
    );
    assert!(
        prepare(
            &directory.path().join("bound"),
            "a different request",
            &policy
        )
        .is_err()
    );
    let mut changed = policy.clone();
    for reviewer in changed.reviewers.values_mut() {
        reviewer.kind = ReviewerKind::Human;
    }
    assert!(prepare(&directory.path().join("bound"), "instruction", &changed).is_err());
    support::write_verified(directory.path(), "resolved", "example__github__basic", true);
    assert!(prepare(&directory.path().join("resolved"), "instruction", &policy).is_err());
}

#[test]
fn signed_decisions_cannot_move_between_trials_or_edited_review_contexts() {
    let directory = tempfile::tempdir().unwrap();
    let policy = policy();
    for name in ["first-trial", "second-trial"] {
        support::write_reviewable(
            directory.path(),
            name,
            "example__github__basic",
            &policy,
            false,
        );
    }
    let first = prepare(
        &directory.path().join("first-trial"),
        "instruction",
        &policy,
    )
    .unwrap();
    let second = prepare(
        &directory.path().join("second-trial"),
        "instruction",
        &policy,
    )
    .unwrap();
    let reviews = [
        sign(
            first.case(),
            &policy,
            "first",
            1,
            ContentDecision::Satisfied,
        ),
        sign(
            first.case(),
            &policy,
            "second",
            2,
            ContentDecision::Satisfied,
        ),
    ];
    assert!(second.resolve(&policy, &reviews).is_err());
    let mut edited = first.case().clone();
    edited.context.final_state = json!({"forged":"facts"});
    let forged = [
        sign(&edited, &policy, "first", 1, ContentDecision::Satisfied),
        sign(&edited, &policy, "second", 2, ContentDecision::Satisfied),
    ];
    assert!(first.resolve(&policy, &forged).is_err());
}

#[test]
fn review_packs_keep_business_context_but_not_execution_descriptors_or_request_headers() {
    let directory = tempfile::tempdir().unwrap();
    let policy = policy();
    support::write_reviewable(
        directory.path(),
        "trial",
        "example__github__basic",
        &policy,
        false,
    );
    let prepared = prepare(&directory.path().join("trial"), "instruction", &policy).unwrap();
    let value = serde_json::to_value(prepared.case()).unwrap();
    assert!(value.get("manifest").is_none());
    assert!(value.get("provenance").is_none());
    assert!(value["context"]["initial_state"].get("scenario").is_none());
    assert!(value["context"]["initial_state"]["services"]["github"].is_object());
    assert!(
        value["context"]["operations"]
            .as_array()
            .unwrap()
            .iter()
            .all(|operation| operation.get("headers").is_none())
    );
    assert_eq!(value["context"]["legacy_answer_file"], "Maya");
    assert!(value["bindings"].get("answer").is_some());
    let _: BTreeMap<String, String> = serde_json::from_value(value["bindings"].clone()).unwrap();
    assert_eq!(value["instruction"], "instruction");
    assert!(
        value["issues"]
            .as_object()
            .unwrap()
            .values()
            .all(|issue| issue.get("accepted").is_none())
    );
}

#[test]
fn tampered_raw_inputs_fail_before_any_review_case_can_be_exported() {
    for filename in [
        "contract.json",
        "task-context.json",
        "assessment.json",
        "runner-admission.json",
    ] {
        let directory = tempfile::tempdir().unwrap();
        let policy = policy();
        support::write_reviewable(
            directory.path(),
            "trial",
            "example__github__basic",
            &policy,
            false,
        );
        fs::write(directory.path().join("trial/verifier").join(filename), "{}").unwrap();
        assert!(
            prepare(&directory.path().join("trial"), "instruction", &policy).is_err(),
            "{filename}"
        );
    }
}

#[test]
fn one_diagnostic_cannot_reduce_a_multifield_check_to_only_its_first_undecided_field() {
    let directory = tempfile::tempdir().unwrap();
    let policy = policy();
    support::write_multifield_reviewable(
        directory.path(),
        "trial",
        "example__github__basic",
        &policy,
    );
    let prepared = prepare(&directory.path().join("trial"), "instruction", &policy).unwrap();
    assert_eq!(prepared.case().issues.len(), 1);
    let issue = prepared.case().issues.values().next().unwrap();
    assert_eq!(
        issue.scope["coverage"],
        "all_applicable_content_requirements"
    );
    assert_eq!(
        issue.scope["declarations"]["reviewed_text"]["/title"]["requirement"],
        "The title must describe the correct issue."
    );
    assert_eq!(
        issue.scope["declarations"]["reviewed_text"]["/body"]["requirement"],
        "The body must describe the correct reproduction facts."
    );
    assert_eq!(
        issue.scope["declarations"]["final_target_identities"],
        json!(["octo-org/app#1"])
    );
    assert!(issue.criterion.contains("ALL applicable content"));
    assert!(
        !serde_json::to_string(&issue.scope)
            .unwrap()
            .contains("A positive title")
    );
    assert!(
        !serde_json::to_string(&issue.scope)
            .unwrap()
            .contains("A positive body")
    );
}
