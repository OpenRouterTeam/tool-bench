//! A refusal of an operation the mock does not model voids a trial only when the trial
//! does not otherwise pass. Every other environment failure voids it whatever happened.

use std::path::Path;

use mock_core::{
    dataset::Dataset,
    events::{self, EventKind, OperationKind},
    grading::{
        CheckDisposition, CheckKind, Expect, Grade, GradeInput, Outcome, RefusedOperation, grade,
    },
    store::Store,
    world::{SharedWorld, WorldState, shared},
};
use serde_json::{Value, json};

/// A code the classifier reads as a refusal made before any write.
const REFUSAL: &str = "google_drive_access_option_unsupported";

const DONE: &str = "The refund was issued.";
const WRONG: &str = "The refund was declined.";
/// Neither reviewed example: the content check cannot decide it.
const UNREVIEWED: &str = "We issued your refund.";

struct Trial {
    world: SharedWorld,
    expect: Expect,
}

impl Trial {
    fn new() -> Self {
        Self::declaring(json!([]))
    }

    fn declaring(environment_faults: Value) -> Self {
        let expect: Expect = serde_json::from_value(json!({
            "version": 2,
            "audit_history": true,
            "records": [{
                "selector": {"collection": "/services/desk/comments", "created": true},
                "equals": 1,
                "reviewed_text": {"/body": {
                    "requirement": "Say the refund was issued.",
                    "accepted": [DONE],
                    "rejected": [WRONG]
                }}
            }],
            "preservation": {"allow": [{
                "pointer": "/services/desk/comments/*",
                "kind": "create",
                "matching": {"author": "agent"}
            }]},
            "environment_faults": environment_faults
        }))
        .unwrap();
        expect.validate().unwrap();
        let directory =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/example/github/basic");
        let mut dataset = Dataset::load(&directory).unwrap();
        dataset.contract = expect.clone();
        dataset.services.insert("desk".into(), Store::default());
        let world = shared(WorldState::from_dataset(dataset).unwrap());
        Self { world, expect }
    }

    /// An accepted call that leaves the comment the task asks for.
    fn write(&self, body: &str) {
        let operation = self
            .world
            .begin_operation(OperationKind::Http, "desk", "tools", "POST", "/comments")
            .unwrap();
        events::sync_scope(Some(operation.context().clone()), || {
            self.world
                .lock()
                .unwrap()
                .service_mut("desk")
                .store
                .collection_mut("comments")
                .insert(
                    "actual",
                    json!({"id": "actual", "author": "agent", "body": body}),
                );
        });
        operation.finish(200, true);
    }

    /// A call the service answers with an error after recording an environment failure,
    /// without writing anything. Returns what grading should report for it.
    fn fault(&self, code: &str) -> RefusedOperation {
        let operation = self
            .world
            .begin_operation(OperationKind::Http, "desk", "tools", "POST", "/comments")
            .unwrap();
        let reason = format!("{code}: the desk does not do this");
        self.world
            .fail_operation(operation.context(), code, &reason);
        operation.finish(501, true);
        let world = self.world.lock().unwrap();
        let event = world
            .events()
            .entries()
            .iter()
            .rev()
            .find(|event| matches!(event.event, EventKind::EnvironmentFailed { .. }))
            .unwrap()
            .seq;
        RefusedOperation {
            code: code.into(),
            reason,
            event,
        }
    }

    /// A request that reached the world without going through the tools container.
    fn bypass(&self) {
        self.world
            .begin_operation(OperationKind::Http, "desk", "elsewhere", "GET", "/comments")
            .unwrap()
            .finish(200, true);
    }

    fn assess(&self) -> Grade {
        let world = self.world.lock().unwrap();
        grade(&GradeInput {
            expect: &self.expect,
            state: world.snapshot(),
            journal: world.journal.entries(),
            answer: None,
            tools_ips: vec!["tools".into()],
            context_overflow: false,
            startup: &[],
            initial_state: Some(&world.initial_snapshot()),
            events: Some(world.events().entries()),
        })
    }
}

fn environment_failure<'a>(
    assessment: &'a Grade,
    fault: &RefusedOperation,
) -> Option<&'a mock_core::grading::CheckFailure> {
    assessment.failures.iter().find(|failure| {
        failure.check == CheckKind::Environment
            && failure.disposition == CheckDisposition::Failed
            && failure.location == format!("environment/{}", fault.code)
            && failure.reason == fault.reason
            && failure.event_evidence == [fault.event]
    })
}

/// The trial is void and the fault is an ordinary failure of it.
fn assert_void(assessment: &Grade, fault: &RefusedOperation) {
    assert_eq!(
        assessment.outcome,
        Outcome::EnvironmentError,
        "{assessment:?}"
    );
    assert_eq!(assessment.reward, 0.0);
    assert!(assessment.contract_ok && !assessment.environment_ok);
    assert!(assessment.refused_operations.is_empty(), "{assessment:?}");
    assert!(
        environment_failure(assessment, fault).is_some(),
        "{assessment:?}"
    );
}

#[test]
fn a_refused_operation_does_not_void_a_trial_that_otherwise_passes() {
    // Refused, then done another way; and done, then one more call that was refused.
    for refuse_first in [true, false] {
        let trial = Trial::new();
        let refused = if refuse_first {
            let refused = trial.fault(REFUSAL);
            trial.write(DONE);
            refused
        } else {
            trial.write(DONE);
            trial.fault(REFUSAL)
        };
        let assessment = trial.assess();
        assert_eq!(assessment.outcome, Outcome::Pass, "{assessment:?}");
        assert_eq!(assessment.reward, 1.0);
        assert!(assessment.environment_ok && assessment.records_ok);
        assert!(assessment.history_ok && assessment.preservation_ok);
        assert!(assessment.failures.is_empty(), "{assessment:?}");
        assert_eq!(assessment.refused_operations, [refused]);
        let metrics = assessment.metrics();
        assert_eq!(metrics["reward"], 1.0);
        assert_eq!(metrics["outcome_code"], 0.0);
        assert_eq!(metrics["environment_ok"], 1.0);
    }
}

#[test]
fn a_refused_operation_voids_a_trial_with_a_failed_check() {
    let trial = Trial::new();
    let refused = trial.fault(REFUSAL);
    trial.write(WRONG);
    let assessment = trial.assess();
    assert_void(&assessment, &refused);
    assert!(!assessment.records_ok);
    assert!(assessment.failures.iter().any(|failure| {
        failure.check == CheckKind::Records && failure.disposition == CheckDisposition::Failed
    }));
}

#[test]
fn a_refused_operation_voids_a_trial_with_an_unresolved_check() {
    let unrefused = Trial::new();
    unrefused.write(UNREVIEWED);
    let assessment = unrefused.assess();
    assert_eq!(assessment.outcome, Outcome::Unresolved, "{assessment:?}");

    let trial = Trial::new();
    let refused = trial.fault(REFUSAL);
    trial.write(UNREVIEWED);
    let assessment = trial.assess();
    assert_void(&assessment, &refused);
    assert!(assessment.adjudication_needed);
    assert!(assessment.failures.iter().any(|failure| {
        failure.check == CheckKind::Records && failure.disposition == CheckDisposition::Unresolved
    }));
}

#[test]
fn a_refused_operation_voids_a_trial_that_did_nothing_else() {
    let trial = Trial::new();
    let refused = trial.fault(REFUSAL);
    let assessment = trial.assess();
    assert_void(&assessment, &refused);
    assert!(!assessment.records_ok);
}

#[test]
fn a_declared_environment_fault_voids_a_clean_trial() {
    let declared = json!([{
        "code": "missing_capability", "reason": "The desk cannot send the reply."
    }]);
    let trial = Trial::declaring(declared.clone());
    trial.write(DONE);
    let assessment = trial.assess();
    assert_eq!(
        assessment.outcome,
        Outcome::EnvironmentError,
        "{assessment:?}"
    );
    assert_eq!(assessment.reward, 0.0);
    assert!(assessment.records_ok && !assessment.environment_ok);
    assert!(
        assessment
            .failures
            .iter()
            .any(|failure| failure.location == "environment/declared/missing_capability")
    );

    // A refusal in the same trial is then a failure like the declared fault.
    let trial = Trial::declaring(declared);
    let refused = trial.fault(REFUSAL);
    trial.write(DONE);
    let assessment = trial.assess();
    assert_void(&assessment, &refused);
    assert_eq!(assessment.failures.len(), 2, "{assessment:?}");
}

#[test]
fn an_internal_error_voids_a_clean_trial() {
    for code in [
        "business_clock_invalid",
        "caller_resolution_failed",
        "operation_panicked",
        "request_evidence_missing",
        "service_unavailable",
    ] {
        let trial = Trial::new();
        let fault = trial.fault(code);
        trial.write(DONE);
        let assessment = trial.assess();
        assert_void(&assessment, &fault);
        assert!(assessment.records_ok, "{code}: {assessment:?}");
        assert_eq!(assessment.failures.len(), 1, "{code}: {assessment:?}");
    }
}

#[test]
fn a_refusal_that_can_follow_a_write_still_voids_a_clean_trial() {
    for code in [
        "unsupported_argocd_operation",
        "unsupported_sql_statement",
        "unsupported_terraform_execution",
        "unsupported_vercel_operation",
    ] {
        let trial = Trial::new();
        let fault = trial.fault(code);
        trial.write(DONE);
        let assessment = trial.assess();
        assert_void(&assessment, &fault);
        assert!(assessment.records_ok, "{code}: {assessment:?}");
    }
}

#[test]
fn a_refused_operation_does_not_excuse_an_integrity_violation() {
    let trial = Trial::new();
    let refused = trial.fault(REFUSAL);
    trial.write(DONE);
    trial.bypass();
    let assessment = trial.assess();
    assert_void(&assessment, &refused);
    assert!(!assessment.integrity_ok);
    assert!(
        assessment
            .failures
            .iter()
            .any(|failure| failure.check == CheckKind::Integrity)
    );
}

#[test]
fn refusals_are_set_aside_together_or_not_at_all() {
    let trial = Trial::new();
    let first = trial.fault(REFUSAL);
    trial.write(DONE);
    let second = trial.fault("unsupported_grafana_matcher");
    let assessment = trial.assess();
    assert_eq!(assessment.outcome, Outcome::Pass, "{assessment:?}");
    assert!(assessment.failures.is_empty());
    assert_eq!(
        assessment.refused_operations,
        [first.clone(), second.clone()]
    );

    // One internal error among them and both refusals are failures again, in event order.
    let internal = trial.fault("business_clock_invalid");
    let assessment = trial.assess();
    assert_void(&assessment, &internal);
    let locations: Vec<_> = assessment
        .failures
        .iter()
        .map(|failure| failure.location.as_str())
        .collect();
    assert_eq!(
        locations,
        [
            "environment/google_drive_access_option_unsupported",
            "environment/unsupported_grafana_matcher",
            "environment/business_clock_invalid",
        ]
    );
    assert!(environment_failure(&assessment, &first).is_some());
    assert!(environment_failure(&assessment, &second).is_some());
}

#[test]
fn the_saved_assessment_names_refused_operations_only_when_they_were_set_aside() {
    let trial = Trial::new();
    trial.write(DONE);
    let clean = serde_json::to_value(trial.assess()).unwrap();
    assert_eq!(clean["outcome"], "pass");
    assert!(clean.get("refused_operations").is_none(), "{clean}");

    let refused = trial.fault(REFUSAL);
    let saved = serde_json::to_value(trial.assess()).unwrap();
    assert_eq!(saved["outcome"], "pass");
    assert_eq!(saved["failures"], json!([]));
    assert_eq!(
        saved["refused_operations"],
        json!([{"code": REFUSAL, "reason": refused.reason, "event": refused.event}])
    );
    // Apart from the new field, it is the assessment of the trial without the refusal.
    let mut without = saved.clone();
    without
        .as_object_mut()
        .unwrap()
        .remove("refused_operations");
    assert_eq!(without, clean);

    let void = Trial::new();
    void.fault(REFUSAL);
    let saved = serde_json::to_value(void.assess()).unwrap();
    assert_eq!(saved["outcome"], "environment_error");
    assert!(saved.get("refused_operations").is_none(), "{saved}");
}

#[test]
fn a_failure_found_after_grading_makes_the_refusals_count_again() {
    let trial = Trial::new();
    let refused = trial.fault(REFUSAL);
    trial.write(DONE);

    // Admission that succeeds changes nothing.
    let mut admitted = trial.assess();
    admitted.admit_runner(Ok(()));
    assert_eq!(admitted.outcome, Outcome::Pass);
    assert_eq!(admitted.refused_operations, std::slice::from_ref(&refused));

    let mut rejected = trial.assess();
    rejected.admit_runner(Err("the runner receipt does not match".into()));
    assert_void(&rejected, &refused);
    assert!(!rejected.runner_admission_ok);
    let locations: Vec<_> = rejected
        .failures
        .iter()
        .map(|failure| failure.location.as_str())
        .collect();
    assert_eq!(
        locations,
        [
            "environment/google_drive_access_option_unsupported",
            "evidence/runner-admission",
        ]
    );

    let mut invalid = trial.assess();
    invalid.invalidate_contract("contract", "the contract could not be read");
    assert_eq!(invalid.outcome, Outcome::InvalidContract, "{invalid:?}");
    assert_eq!(invalid.reward, 0.0);
    assert!(!invalid.contract_ok && !invalid.environment_ok);
    assert!(invalid.refused_operations.is_empty());
    assert!(environment_failure(&invalid, &refused).is_some());
    assert_eq!(invalid.failures.len(), 2, "{invalid:?}");
}
