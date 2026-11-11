use mock_core::faults::{
    FaultDecision, FaultKind, FaultRule, FaultSchedule, FaultScheduleError, SCHEDULE_VERSION,
};
use serde_json::json;

fn transient(resource: Option<&str>, status: u16, occurrences: Vec<u32>) -> FaultRule {
    FaultRule {
        service: "stripe".into(),
        operation_id: "GetChargesCharge".into(),
        resource: resource.map(str::to_owned),
        fault: FaultKind::TransientError {
            status,
            occurrences,
        },
    }
}

#[test]
fn unsupported_operation_is_not_silent() {
    let mut rule = transient(None, 503, vec![1]);
    rule.operation_id = "GetChargesId".into();
    assert!(matches!(
        FaultSchedule::new(vec![rule]),
        Err(FaultScheduleError::UnsupportedOperation { .. })
    ));
}

#[test]
fn invalid_status_empty_occurrences_and_duplicates_are_rejected() {
    for status in [0, 200, 400, 401, 404, 600] {
        assert!(FaultSchedule::new(vec![transient(None, status, vec![1])]).is_err());
    }
    for occurrences in [vec![], vec![0], vec![1, 1]] {
        assert!(FaultSchedule::new(vec![transient(None, 503, occurrences)]).is_err());
    }
}

#[test]
fn conflicting_rules_are_rejected_regardless_of_declaration_order() {
    let specific = transient(Some("ch_target"), 429, vec![1]);
    let broad = transient(None, 503, vec![1]);
    assert!(FaultSchedule::new(vec![specific.clone(), broad.clone()]).is_err());
    assert!(FaultSchedule::new(vec![broad, specific]).is_err());
    assert!(
        FaultSchedule::new(vec![
            transient(Some("ch_first"), 503, vec![1]),
            transient(Some("ch_second"), 503, vec![1]),
        ])
        .is_ok()
    );
}

#[test]
fn read_operations_cannot_claim_post_commit_response_loss() {
    let mut rule = transient(None, 503, vec![1]);
    rule.fault = FaultKind::ResponseLost { occurrence: 1 };
    assert_eq!(
        FaultSchedule::new(vec![rule]).unwrap_err(),
        FaultScheduleError::ResponseLossOnRead
    );
}

#[test]
fn collection_operations_cannot_silently_ignore_resource_selectors() {
    let mut rule = transient(Some("ch_target"), 503, vec![1]);
    rule.operation_id = "GetCharges".into();
    assert_eq!(
        FaultSchedule::new(vec![rule]).unwrap_err(),
        FaultScheduleError::UnsupportedResourceSelector
    );
}

#[test]
fn empty_schedule_is_valid() {
    assert!(FaultSchedule::new(vec![]).is_ok());
}

#[test]
fn resource_counters_are_independent() {
    let mut schedule =
        FaultSchedule::new(vec![transient(Some("ch_target"), 503, vec![1])]).unwrap();
    let (decision, _) = schedule.check("stripe", "GetChargesCharge", Some("ch_other"));
    assert_eq!(decision, FaultDecision::None);
    let (decision, record) = schedule.check("stripe", "GetChargesCharge", Some("ch_target"));
    assert_eq!(decision, FaultDecision::TransientError { status: 503 });
    assert_eq!(record.occurrence, 1);
    let (decision, record) = schedule.check("stripe", "GetChargesCharge", Some("ch_target"));
    assert_eq!(decision, FaultDecision::None);
    assert_eq!(record.occurrence, 2);
}

#[test]
fn wildcard_rules_count_per_resource_instead_of_request_order() {
    let mut schedule = FaultSchedule::new(vec![transient(None, 429, vec![1, 3])]).unwrap();
    for resource in ["ch_first", "ch_second"] {
        for expected in [
            FaultDecision::TransientError { status: 429 },
            FaultDecision::None,
            FaultDecision::TransientError { status: 429 },
        ] {
            assert_eq!(
                schedule
                    .check("stripe", "GetChargesCharge", Some(resource))
                    .0,
                expected
            );
        }
    }
}

#[test]
fn other_operations_do_not_consume_selected_occurrences() {
    let mut schedule = FaultSchedule::new(vec![transient(None, 503, vec![1])]).unwrap();
    schedule.check("stripe", "GetCustomers", None);
    assert_eq!(
        schedule.check("stripe", "GetChargesCharge", None).0,
        FaultDecision::TransientError { status: 503 },
    );
}

#[test]
fn resets_and_clones_are_isolated() {
    let mut schedule = FaultSchedule::new(vec![transient(None, 503, vec![1])]).unwrap();
    let mut other = schedule.clone();
    assert_eq!(
        schedule.check("stripe", "GetChargesCharge", None).0,
        FaultDecision::TransientError { status: 503 }
    );
    assert_eq!(
        other.check("stripe", "GetChargesCharge", None).0,
        FaultDecision::TransientError { status: 503 }
    );
    schedule.reset();
    assert_eq!(
        schedule.check("stripe", "GetChargesCharge", None).0,
        FaultDecision::TransientError { status: 503 }
    );
    assert_eq!(
        other.check("stripe", "GetChargesCharge", None).0,
        FaultDecision::None
    );
}

#[test]
fn serialization_preserves_configuration_but_starts_fresh_counters() {
    let mut schedule = FaultSchedule::new(vec![transient(None, 503, vec![1])]).unwrap();
    schedule.check("stripe", "GetChargesCharge", None);
    let value = serde_json::to_value(&schedule).unwrap();
    assert_eq!(value["version"], SCHEDULE_VERSION);
    assert!(value.get("counters").is_none());
    let mut restored: FaultSchedule = serde_json::from_value(value).unwrap();
    assert_eq!(restored.rules(), schedule.rules());
    assert_eq!(
        restored.check("stripe", "GetChargesCharge", None).0,
        FaultDecision::TransientError { status: 503 }
    );
}

#[test]
fn malformed_deserialized_schedules_are_rejected_immediately() {
    for value in [
        json!({"version": "fault-schedule-v99", "rules": []}),
        json!({"version": SCHEDULE_VERSION, "rules": [], "counters": []}),
        json!({"version": SCHEDULE_VERSION, "rules": [{
            "service": "stripe", "operation_id": "GetChargesCharge",
            "fault": {"type": "transient_error", "status": 200, "occurrences": [1]}
        }]}),
    ] {
        assert!(serde_json::from_value::<FaultSchedule>(value).is_err());
    }
}

#[test]
fn dataset_loads_and_source_binds_the_toml_schedule() {
    let directory = tempfile::tempdir().unwrap();
    let scenario = r#"
[scenario]
id = "diagnostic/stripe/retry"
now = "2026-11-09T12:00:00Z"
seed = "retry"
[fault_schedule]
version = "fault-schedule-v1"
[[fault_schedule.rules]]
service = "stripe"
operation_id = "PostRefunds"
resource = "ch_target"
[fault_schedule.rules.fault]
type = "response_lost"
occurrence = 1
"#;
    let path = directory.path().join("scenario.toml");
    std::fs::write(&path, scenario).unwrap();
    let dataset = mock_core::dataset::Dataset::load(directory.path()).unwrap();
    let hash = dataset.source.scenario_sha256.clone();
    let mut world = mock_core::world::WorldState::from_dataset(dataset).unwrap();
    assert_eq!(
        world.check_fault("stripe", "PostRefunds", Some("ch_target")),
        FaultDecision::ResponseLost
    );
    world.reset().unwrap();
    assert_eq!(
        world.check_fault("stripe", "PostRefunds", Some("ch_target")),
        FaultDecision::ResponseLost
    );
    std::fs::write(&path, scenario.replace("occurrence = 1", "occurrence = 2")).unwrap();
    let changed = mock_core::dataset::Dataset::load(directory.path()).unwrap();
    assert_ne!(changed.source.scenario_sha256, hash);
    std::fs::write(&path, scenario.replace("response_lost", "unsupported")).unwrap();
    assert!(mock_core::dataset::Dataset::load(directory.path()).is_err());
}
