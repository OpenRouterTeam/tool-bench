use std::path::Path;

use mock_core::{
    dataset::Dataset,
    grading::{EVALUATOR_VERSION, Expect, GradeInput, Outcome, grade},
    journal::JournalEntry,
    world::WorldState,
};
use serde::Deserialize;
use serde_json::{Value, json};

fn initial() -> Value {
    json!({"services": {"supabase": {"orders": {
        "1077": {"total": 48000, "status": "paid"},
        "1080": {"total": 48000, "status": "paid"}
    }}}})
}

fn expectation(value: Value) -> Expect {
    serde_json::from_value(value).unwrap()
}

fn input<'a>(expect: &'a Expect, initial: Option<&'a Value>, final_state: Value) -> GradeInput<'a> {
    GradeInput {
        expect,
        state: final_state,
        journal: &[],
        answer: Some("Maya"),
        tools_ips: vec!["192.0.2.10".into()],
        context_overflow: false,
        startup: &[],
        initial_state: initial,
        events: None,
    }
}

#[test]
fn version_two_preserves_business_state_by_default() {
    let expect = expectation(json!({"version": 2, "answer": {"contains": ["Maya"]}}));
    let initial = initial();
    let mut after = initial.clone();
    after["services"]["supabase"]["orders"]["1077"]["total"] = json!(0);
    let result = grade(&input(&expect, Some(&initial), after));
    assert!(result.answer_ok);
    assert!(result.preservation_checked);
    assert!(!result.preservation_ok);
    assert_eq!(result.reward, 0.0);
    assert!(
        result
            .failures
            .iter()
            .any(|failure| failure.location == "/services/supabase/orders/1077/total")
    );
}

#[test]
fn version_two_accepts_a_read_only_answer_with_authoritative_baseline() {
    let expect = expectation(json!({"version": 2, "answer": {"contains": ["Maya"]}}));
    let initial = initial();
    let result = grade(&input(&expect, Some(&initial), initial.clone()));
    assert!(result.preservation_checked);
    assert!(result.preservation_ok);
    assert_eq!(result.reward, 1.0);
    assert!(result.failures.is_empty());
}

#[test]
fn every_recorded_failure_is_an_authoritative_reward_gate() {
    let initial = initial();
    for contract in [
        json!({"version": 2, "answer": {"contains": ["Maya"]}}),
        json!({"version": 2, "answer": {"contains": ["Someone else"]}}),
        json!({"required_calls": [{"service": "github"}]}),
        json!({"state": [{"pointer": "/services/missing", "exists": true}]}),
        json!({}),
    ] {
        let expect = expectation(contract);
        let result = grade(&input(&expect, Some(&initial), initial.clone()));
        assert_eq!(result.reward == 1.0, result.failures.is_empty());
        assert_eq!(result.outcome == Outcome::Pass, result.failures.is_empty());
    }
}

#[test]
fn missing_initial_state_is_an_environment_error_not_preserved_state() {
    let expect = expectation(json!({"version": 2, "answer": {"contains": ["Maya"]}}));
    let result = grade(&input(&expect, None, initial()));
    assert_eq!(result.outcome, Outcome::EnvironmentError);
    assert!(!result.environment_ok);
    assert_eq!(result.reward, 0.0);
}

#[test]
fn legacy_contracts_explicitly_report_the_absent_preservation_check() {
    let expect = expectation(json!({"answer": {"contains": ["Maya"]}}));
    let result = grade(&input(&expect, None, initial()));
    assert_eq!(expect.version, 1);
    assert!(!result.preservation_checked);
    assert_eq!(
        result.metrics()["evaluator_version"],
        f64::from(EVALUATOR_VERSION)
    );
}

#[test]
fn only_declared_business_fields_can_change() {
    let expect = expectation(json!({
        "version": 2,
        "state": [{"pointer": "/services/supabase/orders/1077/status", "equals": "refunded"}],
        "preservation": {"allow": [{"pointer": "/services/supabase/orders/1077/status"}]}
    }));
    let initial = initial();
    let mut after = initial.clone();
    after["services"]["supabase"]["orders"]["1077"]["status"] = json!("refunded");
    assert_eq!(
        grade(&input(&expect, Some(&initial), after.clone())).reward,
        1.0
    );
    after["services"]["supabase"]["orders"]["1080"]["status"] = json!("refunded");
    let result = grade(&input(&expect, Some(&initial), after));
    assert!(result.state_ok);
    assert!(!result.preservation_ok);
    assert_eq!(result.reward, 0.0);
}

#[test]
fn malformed_contracts_cannot_pass_on_vacuous_predicates() {
    for contract in [
        json!({}),
        json!({"state": [{"pointer": "/services/supabase/orders"}]}),
        json!({"answer": {}}),
        json!({"answer": {"regex": "("}}),
        json!({"answer": {"regex": ".*"}}),
        json!({"required_calls": [{"service": "github", "min_count": 0}]}),
        json!({"required_calls": [{"service": "github", "response_status": [700]}]}),
        json!({"required_calls": [{"service": "github", "allow_tool_error": true}]}),
        json!({"state": [{"pointer": "services", "exists": true}]}),
        json!({"state": [{"pointer": "/services", "exists": false, "equals": {}}]}),
        json!({"version": 2}),
        json!({"version": 300}),
    ] {
        let expect = expectation(contract.clone());
        assert!(expect.validate().is_err(), "{contract}");
        let initial = initial();
        let result = grade(&input(&expect, Some(&initial), initial.clone()));
        assert_eq!(result.outcome, Outcome::InvalidContract, "{contract}");
        assert_eq!(result.reward, 0.0, "{contract}");
    }
}

#[test]
fn unknown_contract_fields_are_not_silently_ignored() {
    assert!(
        serde_json::from_value::<Expect>(json!({
            "answer": {"contains": ["Maya"]},
            "preservaton": {"allow": []}
        }))
        .is_err()
    );
}

#[test]
fn null_has_an_explicit_state_predicate() {
    let expect = expectation(json!({
        "state": [{"pointer": "/services/supabase/orders/1077/status", "is_null": true}]
    }));
    let mut state = initial();
    assert!(!grade(&input(&expect, None, state.clone())).state_ok);
    state["services"]["supabase"]["orders"]["1077"]["status"] = Value::Null;
    assert!(grade(&input(&expect, None, state)).state_ok);
}

#[test]
fn record_counts_and_field_relationships_are_authoritative_outcomes() {
    let state = json!({"services": {"github": {"issues": {
        "one": {"id": 1},
        "two": {"id": 2, "parent_id": 7}
    }}}});
    for contract in [
        json!({"records": [{
            "selector": {"collection": "/services/github/issues", "matching": {"id": 1}},
            "equals": 2
        }]}),
        json!({"relations": [{
            "source": {"collection": "/services/github/issues", "matching": {"id": 1}},
            "source_field": "/id",
            "target": {"collection": "/services/github/issues", "matching": {"id": 2}},
            "target_field": "/parent_id"
        }]}),
    ] {
        let contract = expectation(contract);
        let result = grade(&input(&contract, None, state.clone()));
        assert_eq!(result.reward, 0.0);
        assert!(!result.records_ok || !result.relations_ok);
    }
}

#[test]
fn direct_access_is_only_accepted_by_an_explicit_track_policy() {
    let expect = expectation(json!({
        "interfaces": "direct_allowed",
        "required_calls": [{"service": "github"}]
    }));
    let journal = [JournalEntry {
        seq: 1,
        operation_id: None,
        replay: None,
        time: "2026-11-03T15:00:00Z".into(),
        service: "github".into(),
        source: "192.0.2.20".into(),
        method: "GET".into(),
        host: None,
        scheme: "http".into(),
        content_type: None,
        path: "/api/v3/user".into(),
        query: None,
        request_body: Value::Null,
        status: 200,
        response_body: json!({"login": "maya"}),
    }];
    let mut evidence = input(&expect, None, initial());
    evidence.journal = &journal;
    let result = grade(&evidence);
    assert!(result.integrity_ok);
    assert!(result.required_calls_ok);
    assert_eq!(result.mcp_bypass, 1);
    assert_eq!(result.reward, 1.0);
}

#[test]
fn a_bypass_cannot_disappear_when_its_request_never_finishes() {
    let directory =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/example/github/basic");
    let world = mock_core::world::shared(
        WorldState::from_dataset(Dataset::load(&directory).unwrap()).unwrap(),
    );
    let operation = world
        .begin_operation(
            mock_core::events::OperationKind::Http,
            "github",
            "192.0.2.20",
            "GET",
            "/api/v3/user",
        )
        .unwrap();
    let contract = expectation(json!({"answer": {"contains": ["Maya"]}}));
    let world = world.lock().unwrap();
    let mut evidence = input(&contract, None, world.snapshot());
    evidence.events = Some(world.events().entries());
    let result = grade(&evidence);
    assert!(result.answer_ok);
    assert_eq!(result.mcp_bypass, 1);
    assert_eq!(result.outcome, Outcome::IntegrityViolation);
    assert_eq!(result.reward, 0.0);
    assert!(
        result
            .failures
            .iter()
            .any(|failure| failure.event_evidence.contains(&operation.id))
    );
    drop(world);
    drop(operation);
}

#[test]
fn completed_bypasses_are_not_double_counted_across_the_two_evidence_streams() {
    let directory =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/example/github/basic");
    let world = mock_core::world::shared(
        WorldState::from_dataset(Dataset::load(&directory).unwrap()).unwrap(),
    );
    let operation = world
        .begin_operation(
            mock_core::events::OperationKind::Http,
            "github",
            "192.0.2.20",
            "GET",
            "/api/v3/user",
        )
        .unwrap();
    operation.record_head(mock_core::request::RequestHead {
        host: None,
        scheme: "http".into(),
        content_type: None,
        query: None,
        headers: Vec::new(),
    });
    operation.record_body(mock_core::request::CapturedBody::capture(b""));
    operation.record_response(mock_core::journal::ResponseRecord {
        status: 200,
        response_body: json!({"login": "maya"}),
    });
    operation.finish(200, true);
    let contract = expectation(json!({"answer": {"contains": ["Maya"]}}));
    let world = world.lock().unwrap();
    let mut evidence = input(&contract, None, world.snapshot());
    evidence.journal = world.journal.entries();
    evidence.events = Some(world.events().entries());
    let result = grade(&evidence);
    assert_eq!(result.mcp_bypass, 1);
    assert_eq!(result.reward, 0.0);
}

#[test]
fn the_original_supabase_counterexample_is_rejected_by_a_strict_contract() {
    #[derive(Deserialize)]
    struct Scenario {
        expect: Expect,
    }
    let directory = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../scenarios/cloud/supabase/top-customer-march-revenue");
    let scenario: Scenario =
        toml::from_str(&std::fs::read_to_string(directory.join("scenario.toml")).unwrap()).unwrap();
    assert_eq!(scenario.expect.version, 2);
    let world = mock_core::world::shared(
        WorldState::from_dataset(Dataset::load(&directory).unwrap()).unwrap(),
    );
    let initial = world.lock().unwrap().initial_snapshot();
    let operation = world
        .begin_operation(
            mock_core::events::OperationKind::Http,
            "supabase",
            "192.0.2.10",
            "POST",
            "/v1/projects/qmvbxtzkplrnwdhyfaco/database/query",
        )
        .unwrap();
    mock_core::events::sync_scope(Some(operation.context().clone()), || {
        let mut world = world.lock().unwrap();
        let orders = world
            .service_mut("supabase")
            .store
            .collection_mut("db.qmvbxtzkplrnwdhyfaco.public.orders");
        let key = orders.iter().next().unwrap().0.to_owned();
        orders.get_mut(&key).unwrap()["total"] = json!(0);
    });
    operation.finish(200, true);
    let world = world.lock().unwrap();
    let mut evidence = input(&scenario.expect, Some(&initial), world.snapshot());
    evidence.answer = Some("Grace Kim (grace.kim@example.kr): $1,234.50.");
    evidence.events = Some(world.events().entries());
    let result = grade(&evidence);
    assert!(result.answer_ok);
    assert!(!result.preservation_ok);
    assert!(result.environment_ok);
    assert_eq!(result.outcome, Outcome::Fail);
    assert_eq!(result.reward, 0.0);
}
