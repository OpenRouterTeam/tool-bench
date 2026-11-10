use std::collections::BTreeMap;

use ed25519_dalek::{Signer, SigningKey};
use mock_core::{
    grading::CheckKind,
    review::{
        ApprovedReviewer, ContentDecision, IssueDecision, REVIEW_PROTOCOL, ReviewCase,
        ReviewContext, ReviewIssue, ReviewPolicy, ReviewResolution, ReviewStatement, ReviewerKind,
        SignedReview, hex, signing_bytes, verify_reviews,
    },
    runner::sha256,
};
use serde_json::json;

fn key(index: u8) -> SigningKey {
    SigningKey::from_bytes(&[index; 32])
}

fn policy() -> ReviewPolicy {
    ReviewPolicy {
        schema_version: 1,
        protocol: REVIEW_PROTOCOL.into(),
        reviewers: [("first", 1), ("second", 2)]
            .into_iter()
            .map(|(name, index)| {
                (
                    name.into(),
                    ApprovedReviewer {
                        public_key: hex(&key(index).verifying_key().to_bytes()),
                        kind: ReviewerKind::ScriptedFixture,
                    },
                )
            })
            .collect(),
    }
}

fn case(policy: &ReviewPolicy) -> ReviewCase {
    let issue = ReviewIssue {
        check: CheckKind::Records,
        location: "records/0/history/actual".into(),
        criterion: "The complete customer reply must truthfully report the actual refund.".into(),
        scope: json!({
            "kind":"record_content","coverage":"all_applicable_content_requirements",
            "phase":"final_content","declarations":{"reply":"Report the actual refund truthfully."}
        }),
        evidence: vec![2],
        event_evidence: vec![4],
    };
    let instruction = "Refund the duplicate charge and tell the customer.".to_owned();
    let mut bindings: BTreeMap<_, _> = [
        "assessment",
        "contract",
        "evidence",
        "instruction",
        "manifest",
        "review-implementation",
        "result",
        "runner-admission",
        "source",
        "task-context",
        "verification",
    ]
    .into_iter()
    .map(|name| (name.into(), sha256(name.as_bytes())))
    .collect();
    bindings.insert("instruction".into(), sha256(instruction.as_bytes()));
    ReviewCase {
        schema_version: 1,
        protocol: REVIEW_PROTOCOL.into(),
        policy_sha256: policy.sha256().unwrap(),
        bindings,
        instruction,
        context: ReviewContext {
            initial_state: json!({"refunds":{}}),
            final_state: json!({"refunds":{"real":{"amount":25}}}),
            operations: json!([{"event":4,"refund":"real"}]),
            legacy_answer_file: None,
        },
        issues: BTreeMap::from([(issue.id(), issue)]),
    }
}

fn review(
    policy: &ReviewPolicy,
    case: &ReviewCase,
    name: &str,
    index: u8,
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
                        rationale: "Independent scripted protocol control, not human evaluation."
                            .into(),
                    },
                )
            })
            .collect(),
    };
    let signature = key(index).sign(&signing_bytes(&statement));
    SignedReview {
        statement,
        signature: hex(&signature.to_bytes()),
    }
}

#[test]
fn only_a_unanimous_distinct_key_quorum_resolves_a_content_issue() {
    for decision in [ContentDecision::Satisfied, ContentDecision::Violated] {
        let policy = policy();
        let case = case(&policy);
        let reviews = [
            review(&policy, &case, "first", 1, decision),
            review(&policy, &case, "second", 2, decision),
        ];
        let result = verify_reviews(&policy, &case, &reviews).unwrap();
        assert_eq!(result.reviewers, ["first", "second"]);
        assert_eq!(result.reviewer_kind, ReviewerKind::ScriptedFixture);
        assert!(result.decisions.values().all(|resolution| {
            *resolution
                == match decision {
                    ContentDecision::Satisfied => ReviewResolution::Satisfied,
                    ContentDecision::Violated => ReviewResolution::Violated,
                    ContentDecision::Abstain => unreachable!(),
                }
        }));
    }
}

#[test]
fn declared_metric_semantics_use_explicit_scope_and_do_not_expand_content_history_review() {
    let policy = policy();
    let mut case = case(&policy);
    let mut issue = case.issues.values().next().unwrap().clone();
    issue.location = "records/0".into();
    issue.scope = json!({
        "kind":"record_semantics",
        "coverage":"all_applicable_content_and_metric_requirements",
        "phase":"final_semantics",
        "declarations":{"metric_alarm":{"metric":"CPUUtilization"}}
    });
    case.issues = BTreeMap::from([(issue.id(), issue.clone())]);
    let reviews = [
        review(&policy, &case, "first", 1, ContentDecision::Satisfied),
        review(&policy, &case, "second", 2, ContentDecision::Satisfied),
    ];
    assert!(
        verify_reviews(&policy, &case, &reviews)
            .unwrap()
            .decisions
            .values()
            .all(|decision| *decision == ReviewResolution::Satisfied)
    );
    for (field, value) in [
        (
            "phase",
            json!("changed_nonempty_content_at_recorded_commits"),
        ),
        ("coverage", json!("all_applicable_content_requirements")),
        (
            "declarations",
            json!({"other":"undeclared broad semantics"}),
        ),
    ] {
        let mut invalid = issue.clone();
        invalid.scope[field] = value;
        case.issues = BTreeMap::from([(invalid.id(), invalid)]);
        assert!(case.validate(&policy).is_err());
    }
}

#[test]
fn predicate_semantics_review_is_explicitly_scoped_to_final_or_changed_committed_fields() {
    let policy = policy();
    for phase in ["final_semantics", "changed_committed_semantics"] {
        let mut case = case(&policy);
        let mut issue = case.issues.values().next().unwrap().clone();
        issue.scope = json!({
            "kind":"record_semantics",
            "coverage":"all_applicable_content_and_record_semantic_requirements",
            "phase":phase,
            "declarations":{"label_matchers":{"field":"/matchers","equals":{"instance":"pg-replica-2"}}}
        });
        case.issues = BTreeMap::from([(issue.id(), issue.clone())]);
        case.validate(&policy).unwrap();
        let reviews = [
            review(&policy, &case, "first", 1, ContentDecision::Abstain),
            review(&policy, &case, "second", 2, ContentDecision::Abstain),
        ];
        assert!(
            verify_reviews(&policy, &case, &reviews)
                .unwrap()
                .decisions
                .values()
                .all(|decision| *decision == ReviewResolution::Unresolved)
        );
        for (field, value) in [
            (
                "phase",
                json!("changed_nonempty_content_at_recorded_commits"),
            ),
            ("coverage", json!("all_applicable_content_requirements")),
            ("declarations", json!({"timestamp_fields":{}})),
        ] {
            let mut invalid = issue.clone();
            invalid.scope[field] = value;
            case.issues = BTreeMap::from([(invalid.id(), invalid)]);
            assert!(case.validate(&policy).is_err());
        }
    }
}

#[test]
fn missing_reviews_abstentions_and_disagreement_stay_explicitly_unresolved() {
    let policy = policy();
    let case = case(&policy);
    let first = review(&policy, &case, "first", 1, ContentDecision::Satisfied);
    for reviews in [
        vec![],
        vec![first.clone()],
        vec![
            first.clone(),
            review(&policy, &case, "second", 2, ContentDecision::Abstain),
        ],
        vec![
            first.clone(),
            review(&policy, &case, "second", 2, ContentDecision::Violated),
        ],
    ] {
        assert!(
            verify_reviews(&policy, &case, &reviews)
                .unwrap()
                .decisions
                .values()
                .all(|decision| *decision == ReviewResolution::Unresolved)
        );
    }
}

#[test]
fn forged_signatures_reviewer_substitution_and_duplicate_votes_are_rejected() {
    let policy = policy();
    let case = case(&policy);
    let first = review(&policy, &case, "first", 1, ContentDecision::Satisfied);
    let mut forged = first.clone();
    forged.signature = "0".repeat(128);
    let mut substituted = first.clone();
    substituted.statement.reviewer = "second".into();
    let mut renamed = first.clone();
    renamed.statement.reviewer = "not-approved".into();
    for reviews in [
        vec![forged],
        vec![substituted],
        vec![renamed],
        vec![first.clone(), first.clone()],
        vec![review(
            &policy,
            &case,
            "first",
            3,
            ContentDecision::Satisfied,
        )],
    ] {
        assert!(verify_reviews(&policy, &case, &reviews).is_err());
    }
}

#[test]
fn signatures_bind_every_decision_and_its_rationale() {
    let policy = policy();
    let case = case(&policy);
    let original = review(&policy, &case, "first", 1, ContentDecision::Satisfied);
    for change in ["decision", "rationale", "empty", "extra", "protocol"] {
        let mut changed = original.clone();
        match change {
            "decision" => {
                changed
                    .statement
                    .decisions
                    .values_mut()
                    .next()
                    .unwrap()
                    .decision = ContentDecision::Violated;
            }
            "rationale" => {
                changed
                    .statement
                    .decisions
                    .values_mut()
                    .next()
                    .unwrap()
                    .rationale = "A different explanation.".into();
            }
            "empty" => changed.statement.decisions.clear(),
            "extra" => {
                let decision = changed.statement.decisions.values().next().unwrap().clone();
                changed
                    .statement
                    .decisions
                    .insert(sha256(b"other"), decision);
            }
            _ => changed.statement.protocol = "a different signing protocol".into(),
        }
        assert!(
            verify_reviews(&policy, &case, &[changed]).is_err(),
            "{change}"
        );
    }
}

#[test]
fn decisions_cannot_be_replayed_after_evidence_source_instruction_or_content_changes() {
    let policy = policy();
    let original_case = case(&policy);
    let original_review = review(
        &policy,
        &original_case,
        "first",
        1,
        ContentDecision::Satisfied,
    );
    for change in [
        "evidence",
        "source",
        "manifest",
        "review-implementation",
        "instruction",
        "content",
        "criterion",
    ] {
        let mut changed = original_case.clone();
        match change {
            "instruction" => {
                changed.instruction = "A different authorized request.".into();
                changed
                    .bindings
                    .insert("instruction".into(), sha256(changed.instruction.as_bytes()));
            }
            "content" => changed.context.final_state = json!({"refunds":{}}),
            "criterion" => {
                let mut issue = changed.issues.values().next().unwrap().clone();
                issue.criterion = "A different criterion.".into();
                changed.issues = BTreeMap::from([(issue.id(), issue)]);
            }
            field => {
                changed.bindings.insert(field.into(), sha256(b"changed"));
            }
        }
        assert!(
            verify_reviews(&policy, &changed, std::slice::from_ref(&original_review)).is_err(),
            "{change}"
        );
    }
}

#[test]
fn policy_changes_mixed_reviewer_kinds_and_duplicate_or_weak_keys_are_not_admitted() {
    let original = policy();
    for change in [
        "quorum",
        "duplicate",
        "weak",
        "mixed",
        "identity",
        "version",
    ] {
        let mut changed = original.clone();
        match change {
            "quorum" => {
                changed.reviewers.remove("second");
            }
            "duplicate" => {
                changed.reviewers.get_mut("second").unwrap().public_key =
                    changed.reviewers["first"].public_key.clone();
            }
            "weak" => changed.reviewers.get_mut("first").unwrap().public_key = "0".repeat(64),
            "mixed" => changed.reviewers.get_mut("first").unwrap().kind = ReviewerKind::Human,
            "identity" => {
                let reviewer = changed.reviewers.remove("first").unwrap();
                changed.reviewers.insert("../first".into(), reviewer);
            }
            _ => changed.schema_version += 1,
        }
        assert!(changed.validate().is_err(), "{change}");
    }
    let case = case(&original);
    let review = review(&original, &case, "first", 1, ContentDecision::Satisfied);
    let mut changed = original.clone();
    changed.reviewers.insert(
        "third".into(),
        ApprovedReviewer {
            public_key: hex(&key(3).verifying_key().to_bytes()),
            kind: ReviewerKind::ScriptedFixture,
        },
    );
    assert!(verify_reviews(&changed, &case, &[review]).is_err());
}

#[test]
fn review_cases_cannot_name_noncontent_checks_or_unbound_sources() {
    let policy = policy();
    let original = case(&policy);
    for check in [
        CheckKind::Environment,
        CheckKind::Integrity,
        CheckKind::Preservation,
        CheckKind::Ordering,
        CheckKind::Relations,
        CheckKind::Contract,
    ] {
        let mut changed = original.clone();
        let mut issue = changed.issues.values().next().unwrap().clone();
        issue.check = check;
        changed.issues = BTreeMap::from([(issue.id(), issue)]);
        assert!(changed.validate(&policy).is_err(), "{check:?}");
    }
    for field in original.bindings.keys() {
        let mut changed = original.clone();
        changed.bindings.remove(field);
        assert!(changed.validate(&policy).is_err(), "{field}");
    }
}

#[test]
fn legacy_answer_bytes_are_bound_explicitly_and_do_not_imply_native_completion() {
    let policy = policy();
    let mut case = case(&policy);
    case.context.legacy_answer_file = Some("An actual collected answer.".into());
    assert!(case.validate(&policy).is_err());
    case.bindings.insert(
        "answer".into(),
        sha256(case.context.legacy_answer_file.as_ref().unwrap().as_bytes()),
    );
    case.validate(&policy).unwrap();
    case.context.legacy_answer_file = Some("A substituted answer.".into());
    assert!(case.validate(&policy).is_err());
}

#[test]
fn empty_rationales_cannot_be_authenticated_into_silent_approvals() {
    let policy = policy();
    let case = case(&policy);
    let mut review = review(&policy, &case, "first", 1, ContentDecision::Satisfied);
    review
        .statement
        .decisions
        .values_mut()
        .next()
        .unwrap()
        .rationale = " ".into();
    review.signature = hex(&key(1).sign(&signing_bytes(&review.statement)).to_bytes());
    assert!(verify_reviews(&policy, &case, &[review]).is_err());
}

#[test]
fn omitting_a_required_review_or_outvoting_disagreement_cannot_manufacture_consensus() {
    let mut policy = policy();
    policy.reviewers.insert(
        "third".into(),
        ApprovedReviewer {
            public_key: hex(&key(3).verifying_key().to_bytes()),
            kind: ReviewerKind::ScriptedFixture,
        },
    );
    let case = case(&policy);
    let mut reviews = vec![
        review(&policy, &case, "first", 1, ContentDecision::Satisfied),
        review(&policy, &case, "second", 2, ContentDecision::Satisfied),
    ];
    assert!(
        verify_reviews(&policy, &case, &reviews)
            .unwrap()
            .decisions
            .values()
            .all(|decision| *decision == ReviewResolution::Unresolved)
    );
    reviews.push(review(
        &policy,
        &case,
        "third",
        3,
        ContentDecision::Violated,
    ));
    assert!(
        verify_reviews(&policy, &case, &reviews)
            .unwrap()
            .decisions
            .values()
            .all(|decision| *decision == ReviewResolution::Unresolved)
    );
}
