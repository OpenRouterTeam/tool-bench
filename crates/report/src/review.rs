use std::{collections::BTreeMap, path::Path};

use anyhow::{Context, bail};
use mock_core::{
    events::EventKind,
    grading::{CheckDisposition, CheckFailure, CheckKind, Expect, Grade, Outcome},
    provenance::canonical_sha256,
    review::{
        REVIEW_PROTOCOL, ReviewCase, ReviewContext, ReviewIssue, ReviewPolicy, ReviewResolution,
        SignedReview, VerifiedReviews, verify_reviews,
    },
    runner::sha256,
};
use serde::Serialize;
use serde_json::{Value, json};

use crate::provenance::{VerifiedAssessment, load_verified, read};

pub mod coordinator;

pub struct PreparedReview {
    case: ReviewCase,
    assessment: Grade,
    issue_ids: Vec<Option<String>>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ReviewedAssessment {
    pub protocol: String,
    pub raw: Grade,
    pub effective: Grade,
    pub reviews: VerifiedReviews,
    pub signed_review_sha256: BTreeMap<String, String>,
}

fn issue(
    failure: &CheckFailure,
    contract: &Expect,
    initial: &Value,
    state: &Value,
) -> anyhow::Result<ReviewIssue> {
    let parts: Vec<_> = failure.location.split('/').collect();
    let historical = parts.get(2) == Some(&"history");
    let (kind, declarations) = match failure.check {
        CheckKind::Records if parts.first() == Some(&"records") => {
            let index = review_index(&parts)?;
            let check = contract
                .records
                .get(index)
                .context("review names an undeclared record check")?;
            let text: BTreeMap<_, _> = check
                .reviewed_text
                .iter()
                .map(|(pointer, criterion)| {
                    (
                        pointer.clone(),
                        json!({
                            "requirement":criterion.requirement,"format":criterion.format,
                            "optional":criterion.optional,
                            "identities":criterion.bound.as_ref().map(|bound|
                                bound.identities.iter().map(|(name,source)|(
                                    name.clone(),json!({"selector":source.selector,"field":source.field})
                                )).collect::<BTreeMap<_,_>>()
                            )
                        }),
                    )
                })
                .collect();
            let fields=check.reviewed_fields.as_ref().map(|criterion|json!({
                "requirement":criterion.requirement,
                "fields":criterion.accepted.first().map(|example|example.keys().collect::<Vec<_>>())
            }));
            let metric_alarm = (!historical)
                .then_some(check.metric_alarm.as_ref())
                .flatten();
            if text.is_empty()
                && check.html_text.is_empty()
                && fields.is_none()
                && metric_alarm.is_none()
                && check.timestamp_fields.is_empty()
                && check.label_matchers.is_none()
            {
                bail!("record review has no declared content requirements");
            }
            let targets: Vec<_> = check
                .selector
                .selected_identities(Some(initial), state)
                .map_err(anyhow::Error::msg)?;
            (
                if metric_alarm.is_some()
                    || !check.timestamp_fields.is_empty()
                    || check.label_matchers.is_some()
                {
                    "record_semantics"
                } else {
                    "record_content"
                },
                json!({
                    "selector":check.selector,"reviewed_text":text,
                    "html_text":check.html_text,"reviewed_fields":fields,
                    "metric_alarm":metric_alarm,
                    "timestamp_fields":check.timestamp_fields,
                    "label_matchers":check.label_matchers,
                    "final_target_identities":targets
                }),
            )
        }
        CheckKind::State if parts.first() == Some(&"state") => {
            let index = review_index(&parts)?;
            let check = contract
                .state
                .get(index)
                .context("review names an undeclared state check")?;
            if check.text_equals.is_none() {
                bail!("state review is not a declared literal content check");
            }
            ("state_content", serde_json::to_value(check)?)
        }
        CheckKind::Answer if failure.location == "answer" => {
            let check = contract
                .answer
                .as_ref()
                .context("answer review has no declared source-bound content requirement")?;
            if let Some(check) = &check.sentry_dsn {
                let targets = check
                    .project
                    .selected_identities(Some(initial), state)
                    .map_err(anyhow::Error::msg)?;
                (
                    "answer_content",
                    json!({"sentry_dsn":{
                        "project":check.project,"keys":check.keys,
                        "ingestion_origin":check.ingestion_origin,
                        "requirement":check.requirement,"rejected":check.rejected,
                        "final_project_identities":targets,
                        "factual_binding":"passed_by_raw_evaluation_not_reviewable"
                    }}),
                )
            } else if let Some(check) = &check.oncall {
                let targets = check
                    .schedule
                    .selected_identities(Some(initial), state)
                    .map_err(anyhow::Error::msg)?;
                (
                    "answer_content",
                    json!({"oncall":{
                        "schedule":check.schedule,"directory":check.directory,
                        "requirement":check.requirement,
                        "allow_unambiguous_surnames":check.allow_unambiguous_surnames,
                        "final_schedule_identities":targets,
                        "schedule_records":state.pointer(&check.schedule.collection),
                        "directory_records":state.pointer(&check.directory),
                        "factual_binding":"must_be_established_by_review_not_passed_by_raw_evaluation"
                    }}),
                )
            } else {
                bail!("answer review has no declared source-bound content requirement");
            }
        }
        _ => bail!("review cannot target a noncontent or undeclared check"),
    };
    let metric_semantics = kind == "record_semantics";
    let predicate_semantics = metric_semantics
        && (declarations["label_matchers"].is_object()
            || declarations["timestamp_fields"]
                .as_object()
                .is_some_and(|fields| !fields.is_empty()));
    let oncall = declarations["oncall"].is_object();
    Ok(ReviewIssue {
        check: failure.check,
        location: failure.location.clone(),
        criterion: if oncall {
            format!(
                "Establish the complete answer's factual and communicative correctness against the selected authoritative schedule's current-duty identities and the attached directory, including omissions, extra assignees, ambiguous names, negation, other schedules and contradictory surrounding wording. The raw evaluator did NOT establish this answer's factual binding; do not presume it passed. Missing user lookups do not establish empty duty. Abstain when the facts or wording are insufficient. Do not override any known structural or safety failure. Diagnostic: {}",
                failure.reason
            )
        } else if kind == "answer_content" {
            format!(
                "Review the complete collected answer against the instruction and declared content requirement, including contradictory or misleading surrounding wording. The project, active-key and baseline-endpoint binding passed the immutable raw evaluator and is not reviewable. Do not override known URI, structural or safety failures. Diagnostic: {}",
                failure.reason
            )
        } else if predicate_semantics {
            format!(
                "Review ALL applicable declared record semantics and content in the attached scope. Prove the complete conjunctive label predicate equals the requested scope over all valid label strings, including missing/empty labels, negation, extra constraints and anchored RE2 behavior; fixture samples alone are insufficient. Compare timestamps as exact instants without rounding. For history, follow final target identities at every recorded commit and examine changed semantic fields even before intermediate selectors match; unchanged fields are not new claims. Include any declared final metric-alarm goal and content requirements. Do not override known failures or presume an unproved regex equivalent. Abstain when equivalence cannot be established. Diagnostic: {}",
                failure.reason
            )
        } else if metric_semantics {
            format!(
                "Review ALL declared final metric-alarm and content requirements in the attached scope against the raw configuration and instruction. Establish the selected series, account, metric identity, statistic, period, M-of-N, threshold/comparison and missing-data behavior, not merely pointwise examples or keyword overlap. Do not override known failures, treat a valid unevaluated expression as automatically correct, or infer undocumented defaults. Abstain when equivalence cannot be established. Diagnostic: {}",
                failure.reason
            )
        } else {
            format!(
                "Review ALL applicable content requirements in the attached scope, not only the first diagnostic. For history, follow the final target identities even before intermediate metadata matches, and consider every changed nonempty semantic text projection at the recorded commits, ignoring unchanged wording or metadata-only refinements; do not require final construction fields in incomplete intermediate records. Diagnostic: {}",
                failure.reason
            )
        },
        scope: json!({
            "kind":kind,"coverage":if predicate_semantics{"all_applicable_content_and_record_semantic_requirements"}else if metric_semantics{"all_applicable_content_and_metric_requirements"}else{"all_applicable_content_requirements"},
            "phase":if predicate_semantics && historical{"changed_committed_semantics"}else if metric_semantics{"final_semantics"}else if historical{"changed_nonempty_content_at_recorded_commits"}else{"final_content"},
            "declarations":declarations
        }),
        evidence: failure.evidence.clone(),
        event_evidence: failure.event_evidence.clone(),
    })
}

fn review_index(parts: &[&str]) -> anyhow::Result<usize> {
    parts
        .get(1)
        .context("review check lacks a declared index")?
        .parse::<usize>()
        .context("invalid review check index")
}

pub fn implementation_sha256() -> String {
    let mut bytes = Vec::new();
    for (name, content) in [
        ("review.rs", include_bytes!("review.rs").as_slice()),
        ("provenance.rs", include_bytes!("provenance.rs").as_slice()),
        ("lib.rs", include_bytes!("lib.rs").as_slice()),
        (
            "core-review.rs",
            include_bytes!("../../mock-core/src/review.rs").as_slice(),
        ),
        (
            "core-grading.rs",
            include_bytes!("../../mock-core/src/grading.rs").as_slice(),
        ),
        (
            "core-grading-validation.rs",
            include_bytes!("../../mock-core/src/grading/validation.rs").as_slice(),
        ),
        (
            "core-answer-semantics.rs",
            include_bytes!("../../mock-core/src/grading/answers.rs").as_slice(),
        ),
        (
            "core-oncall-answer-semantics.rs",
            include_bytes!("../../mock-core/src/grading/answers/oncall.rs").as_slice(),
        ),
        (
            "core-label-matchers.rs",
            include_bytes!("../../mock-core/src/label_matchers.rs").as_slice(),
        ),
        (
            "core-clock.rs",
            include_bytes!("../../mock-core/src/clock.rs").as_slice(),
        ),
        (
            "core-content.rs",
            include_bytes!("../../mock-core/src/relations.rs").as_slice(),
        ),
        (
            "coordinator.rs",
            include_bytes!("review/coordinator.rs").as_slice(),
        ),
        (
            "bench-review.rs",
            include_bytes!("bin/bench-review.rs").as_slice(),
        ),
        (
            "bench-report.rs",
            include_bytes!("bin/bench-report.rs").as_slice(),
        ),
        (
            "Cargo.lock",
            include_bytes!("../../../Cargo.lock").as_slice(),
        ),
    ] {
        bytes.extend_from_slice(name.as_bytes());
        bytes.push(0);
        bytes.extend_from_slice(&(content.len() as u64).to_be_bytes());
        bytes.extend_from_slice(content);
    }
    sha256(&bytes)
}

fn business_state(value: &Value) -> Value {
    let mut state = value.clone();
    if let Some(object) = state.as_object_mut() {
        object.remove("scenario");
    }
    state
}

pub fn prepare(
    directory: &Path,
    instruction: &str,
    policy: &ReviewPolicy,
) -> anyhow::Result<PreparedReview> {
    policy.validate().map_err(anyhow::Error::msg)?;
    let result_bytes = read(&directory.join("result.json"))?;
    let result: Value = serde_json::from_slice(&result_bytes)?;
    if !result["exception_info"].is_null() {
        bail!("failed agent executions cannot be promoted through content review");
    }
    let verified = load_verified(directory, &result)?
        .context("content review requires checked source and execution provenance")?;
    prepare_verified(verified, &result_bytes, instruction, policy)
}

pub(crate) fn prepare_verified(
    verified: VerifiedAssessment,
    result_bytes: &[u8],
    instruction: &str,
    policy: &ReviewPolicy,
) -> anyhow::Result<PreparedReview> {
    let assessment = verified.assessment;
    if assessment.contract_version != 2
        || !assessment.adjudication_needed
        || !assessment.contract_ok
        || !assessment.environment_ok
        || !assessment.integrity_ok
        || !assessment.runner_admission_checked
        || !assessment.runner_admission_ok
        || !assessment.preservation_checked
        || !assessment.history_checked
        || !matches!(assessment.outcome, Outcome::Fail | Outcome::Unresolved)
    {
        bail!("only verified unresolved content in valid v2 executions is reviewable");
    }
    let context = &verified.provenance.manifest.context;
    let policy_digest = policy.sha256().map_err(anyhow::Error::msg)?;
    if context.review_policy_sha256.as_ref() != Some(&policy_digest) {
        bail!("the exact review policy was not declared in the protected pre-execution context");
    }
    if context.instruction_sha256 != sha256(instruction.as_bytes()) {
        bail!("review instruction differs from the source-bound user request");
    }
    let mut issues = BTreeMap::new();
    let mut issue_ids = Vec::new();
    for failure in &assessment.failures {
        if failure.disposition != CheckDisposition::Unresolved {
            issue_ids.push(None);
            continue;
        }
        let issue = issue(
            failure,
            &verified.contract,
            &verified.evidence.initial_state,
            &verified.evidence.state,
        )?;
        let identity = issue.id();
        issue_ids.push(Some(identity.clone()));
        if issues.insert(identity, issue).is_some() {
            bail!("reviewable content checks must have distinct complete identities");
        }
    }
    let operations: Vec<_> = verified
        .evidence
        .events
        .iter()
        .filter_map(|event| match &event.event {
            EventKind::StateCommitted { records } => {
                Some(json!({"seq":event.seq,"time":event.time,"records":records}))
            }
            _ => None,
        })
        .collect();
    let mut bindings = verified.bindings;
    bindings.insert("instruction".into(), context.instruction_sha256.clone());
    bindings.insert("result".into(), sha256(result_bytes));
    bindings.insert("review-implementation".into(), implementation_sha256());
    if let Some(answer) = &verified.answer {
        bindings.insert("answer".into(), sha256(answer.as_bytes()));
    }
    let case = ReviewCase {
        schema_version: 1,
        protocol: REVIEW_PROTOCOL.into(),
        policy_sha256: policy_digest,
        bindings,
        instruction: instruction.into(),
        context: ReviewContext {
            initial_state: business_state(&verified.evidence.initial_state),
            final_state: business_state(&verified.evidence.state),
            operations: json!(operations),
            legacy_answer_file: verified.answer,
        },
        issues,
    };
    case.validate(policy).map_err(anyhow::Error::msg)?;
    Ok(PreparedReview {
        case,
        assessment,
        issue_ids,
    })
}

impl PreparedReview {
    pub fn case(&self) -> &ReviewCase {
        &self.case
    }

    pub fn resolve(
        &self,
        policy: &ReviewPolicy,
        reviews: &[SignedReview],
    ) -> anyhow::Result<ReviewedAssessment> {
        let verified = verify_reviews(policy, &self.case, reviews).map_err(anyhow::Error::msg)?;
        let mut effective = self.assessment.clone();
        effective.failures.clear();
        for (index, failure) in self.assessment.failures.iter().enumerate() {
            if failure.disposition == CheckDisposition::Failed {
                effective.failures.push(failure.clone());
                continue;
            }
            match verified
                .decisions
                .get(
                    self.issue_ids
                        .get(index)
                        .and_then(Option::as_ref)
                        .context("missing complete review scope for an unresolved check")?,
                )
                .context("missing verified content decision")?
            {
                ReviewResolution::Satisfied => {}
                ReviewResolution::Violated => {
                    let mut failure = failure.clone();
                    failure.disposition = CheckDisposition::Failed;
                    failure.reason = format!(
                        "authorized content reviewers marked the requirement unsatisfied: {}",
                        failure.reason
                    );
                    effective.failures.push(failure);
                }
                ReviewResolution::Unresolved => effective.failures.push(failure.clone()),
            }
        }
        effective.state_ok = !effective
            .failures
            .iter()
            .any(|failure| failure.check == CheckKind::State);
        effective.records_ok = !effective
            .failures
            .iter()
            .any(|failure| failure.check == CheckKind::Records);
        effective.answer_ok = !effective
            .failures
            .iter()
            .any(|failure| failure.check == CheckKind::Answer);
        effective.adjudication_needed = effective
            .failures
            .iter()
            .any(|failure| failure.disposition == CheckDisposition::Unresolved);
        effective.outcome = if effective.failures.is_empty() {
            Outcome::Pass
        } else if effective
            .failures
            .iter()
            .any(|failure| failure.disposition == CheckDisposition::Failed)
        {
            Outcome::Fail
        } else {
            Outcome::Unresolved
        };
        effective.reward = if effective.outcome == Outcome::Pass {
            1.0
        } else {
            0.0
        };
        Ok(ReviewedAssessment {
            protocol: REVIEW_PROTOCOL.into(),
            raw: self.assessment.clone(),
            effective,
            reviews: verified,
            signed_review_sha256: reviews
                .iter()
                .map(|review| (review.statement.reviewer.clone(), canonical_sha256(review)))
                .collect(),
        })
    }
}

#[cfg(test)]
mod metric_tests {
    use super::*;

    #[test]
    fn metric_review_exports_the_complete_declared_goal_and_actual_targets() {
        let contract: Expect = serde_json::from_value(json!({
            "version":2,"audit_history":true,
            "records":[{
                "selector":{"collection":"/services/aws/alarms","matching":{"AlarmName":"cpu"}},
                "equals":1,
                "metric_alarm":{
                    "account":"481562039417","namespace":"AWS/EC2","metric":"CPUUtilization",
                    "dimensions":{"InstanceId":"i-prod"},"statistic":"Average","period":300,
                    "evaluation_periods":3,"threshold":80,"comparison":"GreaterThanThreshold",
                    "unit":"Percent"
                }
            }]
        }))
        .unwrap();
        contract.validate().unwrap();
        let initial = json!({"services":{"aws":{"alarms":{}}}});
        let state = json!({"services":{"aws":{"alarms":{"actual":{"AlarmName":"cpu",
            "Metrics":[{"Id":"selected","Expression":"IF(cpu > 80, 100, 0)","ReturnData":true}]}}}}});
        let failure = CheckFailure {
            check: CheckKind::Records,
            disposition: CheckDisposition::Unresolved,
            location: "records/0".into(),
            reason: "unevaluated metric math".into(),
            evidence: Vec::new(),
            event_evidence: Vec::new(),
        };
        let issue = issue(&failure, &contract, &initial, &state).unwrap();
        assert_eq!(issue.scope["kind"], "record_semantics");
        assert_eq!(issue.scope["phase"], "final_semantics");
        assert_eq!(
            issue.scope["declarations"]["metric_alarm"],
            serde_json::to_value(&contract.records[0].metric_alarm).unwrap()
        );
        assert_eq!(
            issue.scope["declarations"]["final_target_identities"],
            json!(["actual"])
        );
        assert!(issue.criterion.contains("missing-data"));
        let mut historical = failure;
        historical.location = "records/0/history/actual".into();
        assert!(super::issue(&historical, &contract, &initial, &state).is_err());
    }

    #[test]
    fn matcher_and_timestamp_review_includes_actual_targets_and_changed_committed_semantics() {
        let contract: Expect = serde_json::from_value(json!({
            "version":2,"audit_history":true,
            "records":[{
                "selector":{"collection":"/services/grafana/silences","created":true},
                "equals":1,
                "timestamp_fields":{"/startsAt":"2026-11-13T02:00:00Z"},
                "label_matchers":{"field":"/matchers","equals":{"instance":"pg-replica-2"}}
            }]
        }))
        .unwrap();
        contract.validate().unwrap();
        let initial = json!({"services":{"grafana":{"silences":{}}}});
        let state = json!({"services":{"grafana":{"silences":{"actual":{
            "startsAt":"2026-11-13T02:00:00Z",
            "matchers":[{"name":"instance","value":"\\p{L}+","isRegex":true}]
        }}}}});
        for historical in [false, true] {
            let failure = CheckFailure {
                check: CheckKind::Records,
                disposition: CheckDisposition::Unresolved,
                location: if historical {
                    "records/0/history/actual"
                } else {
                    "records/0"
                }
                .into(),
                reason: "unqualified regex language".into(),
                evidence: Vec::new(),
                event_evidence: if historical { vec![4] } else { Vec::new() },
            };
            let issue = issue(&failure, &contract, &initial, &state).unwrap();
            assert_eq!(issue.scope["kind"], "record_semantics");
            assert_eq!(
                issue.scope["phase"],
                if historical {
                    "changed_committed_semantics"
                } else {
                    "final_semantics"
                }
            );
            assert_eq!(
                issue.scope["declarations"]["label_matchers"],
                serde_json::to_value(&contract.records[0].label_matchers).unwrap()
            );
            assert_eq!(
                issue.scope["declarations"]["timestamp_fields"],
                serde_json::to_value(&contract.records[0].timestamp_fields).unwrap()
            );
            assert_eq!(
                issue.scope["declarations"]["final_target_identities"],
                json!(["actual"])
            );
            assert!(
                issue
                    .criterion
                    .contains("fixture samples alone are insufficient")
            );
            assert_eq!(issue.event_evidence, failure.event_evidence);
        }
    }
}
