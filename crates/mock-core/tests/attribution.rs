//! What a trial's result says about where a failure came from: whether the final state
//! was right and only the history was wrong, whether the contract scores the history at
//! all, how many calls reached the task's own services, and how many a service refused
//! or failed. None of it changes the reward of a contract that does not ask for that.

use std::path::Path;

use mock_core::{
    dataset::Dataset,
    events::{self, OperationKind},
    evidence::WorldEvidence,
    grading::{CheckKind, Expect, Grade, GradeInput, Outcome, grade},
    journal::JournalEntry,
    store::Store,
    world::{SharedWorld, WorldState, shared},
};
use serde_json::{Value, json};

const REPLICAS: &str = "/services/kubernetes/deployments.apps/shop-prod~1checkout/spec/replicas";

fn world() -> SharedWorld {
    let directory =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/example/github/basic");
    let mut dataset = Dataset::load(&directory).unwrap();
    let mut kubernetes = Store::default();
    kubernetes.collection_mut("deployments.apps").insert(
        "shop-prod/checkout",
        json!({"metadata": {"name": "checkout"}, "spec": {"replicas": 3}}),
    );
    dataset.services.insert("kubernetes".into(), kubernetes);
    let mut pagerduty = Store::default();
    pagerduty
        .collection_mut("incidents")
        .insert("production", json!({"status": "triggered"}));
    dataset.services.insert("pagerduty".into(), pagerduty);
    shared(WorldState::from_dataset(dataset).unwrap())
}

fn operation(world: &SharedWorld, service: &str, mutate: impl FnOnce(&mut WorldState)) {
    let context = world
        .begin_operation(OperationKind::Http, service, "tools", "PATCH", "/operation")
        .unwrap();
    events::sync_scope(Some(context.context().clone()), || {
        mutate(&mut world.lock().unwrap())
    });
    context.finish(200, true);
}

fn scale(world: &SharedWorld, replicas: u64) {
    operation(world, "kubernetes", |world| {
        world
            .service_mut("kubernetes")
            .store
            .collection_mut("deployments.apps")
            .get_mut("shop-prod/checkout")
            .unwrap()["spec"]["replicas"] = json!(replicas);
    });
}

fn resolve(world: &SharedWorld) {
    operation(world, "pagerduty", |world| {
        world
            .service_mut("pagerduty")
            .store
            .collection_mut("incidents")
            .get_mut("production")
            .unwrap()["status"] = json!("resolved");
    });
}

/// Scale checkout to nine. The permission pins the value, so a commit that writes another
/// number is a history finding even when a later commit corrects it.
fn contract(extra: Value) -> Expect {
    let mut contract = json!({
        "version": 2,
        "audit_history": true,
        "state": [{"pointer": REPLICAS, "equals": 9}],
        "preservation": {"allow": [
            {"pointer": REPLICAS, "matching": {"spec": {"replicas": 9}}},
            {"pointer": "/services/pagerduty/incidents/production/status"}
        ]}
    });
    contract
        .as_object_mut()
        .unwrap()
        .extend(extra.as_object().unwrap().clone());
    serde_json::from_value(contract).unwrap()
}

fn assess(world: &SharedWorld, contract: &Expect, journal: &[JournalEntry]) -> Grade {
    let world = world.lock().unwrap();
    grade(&GradeInput {
        expect: contract,
        state: world.snapshot(),
        journal,
        answer: None,
        tools_ips: vec!["tools".into()],
        context_overflow: false,
        startup: &[],
        initial_state: Some(&world.initial_snapshot()),
        events: Some(world.events().entries()),
    })
}

#[test]
fn a_corrected_detour_is_a_fail_whose_final_state_is_right() {
    let direct = world();
    scale(&direct, 9);
    let assessment = assess(&direct, &contract(json!({})), &[]);
    assert_eq!(assessment.outcome, Outcome::Pass, "{assessment:?}");
    assert!(assessment.final_state_ok && !assessment.history_only_failure);

    let detour = world();
    scale(&detour, 6);
    scale(&detour, 9);
    let assessment = assess(&detour, &contract(json!({})), &[]);
    assert_eq!(assessment.outcome, Outcome::Fail, "{assessment:?}");
    assert!(assessment.final_state_ok, "{assessment:?}");
    assert!(assessment.history_only_failure, "{assessment:?}");
    assert!(
        assessment
            .failures
            .iter()
            .all(|failure| failure.check == CheckKind::Preservation
                && !failure.event_evidence.is_empty()),
        "{assessment:?}"
    );
    let metrics = assessment.metrics();
    assert_eq!(metrics["reward"], 0.0);
    assert_eq!(metrics["final_state_ok"], 1.0);
    assert_eq!(metrics["history_only_failure"], 1.0);

    let wrong = world();
    scale(&wrong, 6);
    let assessment = assess(&wrong, &contract(json!({})), &[]);
    assert_eq!(assessment.outcome, Outcome::Fail);
    assert!(!assessment.final_state_ok && !assessment.history_only_failure);
    assert_eq!(assessment.metrics()["history_only_failure"], 0.0);
}

#[test]
fn a_contract_that_reports_history_scores_the_final_state() {
    let reported = contract(json!({"history": "report"}));
    let detour = world();
    scale(&detour, 6);
    scale(&detour, 9);
    let assessment = assess(&detour, &reported, &[]);
    assert_eq!(assessment.outcome, Outcome::Pass, "{assessment:?}");
    assert_eq!(assessment.reward, 1.0);
    assert!(assessment.failures.is_empty() && !assessment.history_findings.is_empty());
    assert!(assessment.final_state_ok && !assessment.history_only_failure);
    assert!(assessment.metrics()["history_findings"] >= 1.0);

    // The final state still decides: a detour that is never corrected fails.
    let wrong = world();
    scale(&wrong, 6);
    let assessment = assess(&wrong, &reported, &[]);
    assert_eq!(assessment.outcome, Outcome::Fail, "{assessment:?}");
    assert!(!assessment.final_state_ok);

    // A declared ordering is about history by construction and is not set aside.
    let ordered = contract(json!({
        "history": "report",
        "ordering": [{
            "prerequisite": {
                "selector": {"collection": "/services/kubernetes/deployments.apps", "matching": {"metadata": {"name": "checkout"}}},
                "field": "/spec/replicas", "equals": 9
            },
            "action": {
                "selector": {"collection": "/services/pagerduty/incidents", "key_prefix": "production"},
                "field": "/status", "equals": "resolved"
            }
        }]
    }));
    let premature = world();
    resolve(&premature);
    scale(&premature, 9);
    let assessment = assess(&premature, &ordered, &[]);
    assert_eq!(assessment.outcome, Outcome::Fail, "{assessment:?}");
    assert!(!assessment.ordering_ok && assessment.history_only_failure);
    let in_order = world();
    scale(&in_order, 9);
    resolve(&in_order);
    assert_eq!(assess(&in_order, &ordered, &[]).outcome, Outcome::Pass);
}

#[test]
fn reporting_history_needs_an_audited_history_and_keeps_the_default_contract_bytes() {
    let unaudited: Expect =
        serde_json::from_value(json!({"version": 2, "history": "report"})).unwrap();
    let world = world();
    let assessment = assess(&world, &unaudited, &[]);
    assert_eq!(
        assessment.outcome,
        Outcome::InvalidContract,
        "{assessment:?}"
    );

    // A contract that does not declare the field serializes as it did before the field
    // existed, so its recorded hash does not move.
    let default = serde_json::to_value(contract(json!({}))).unwrap();
    assert!(default.get("history").is_none(), "{default}");
    assert_eq!(
        serde_json::to_value(contract(json!({"history": "report"}))).unwrap()["history"],
        "report"
    );
}

fn call(seq: u64, service: &str, status: u16, response: Value) -> JournalEntry {
    JournalEntry {
        seq,
        operation_id: None,
        replay: None,
        time: "2026-03-14T09:00:00Z".into(),
        service: service.into(),
        source: "tools".into(),
        method: "GET".into(),
        host: None,
        scheme: "http".into(),
        content_type: None,
        path: "/api/v3/user".into(),
        query: None,
        request_body: Value::Null,
        status,
        response_body: response,
    }
}

#[test]
fn calls_are_counted_by_service_and_by_what_the_service_answered() {
    let journal = [
        call(1, "github", 200, json!({"login": "mona"})),
        call(2, "github", 404, json!({"message": "Not Found"})),
        call(3, "github", 422, json!({"message": "Validation Failed"})),
        call(4, "kubernetes", 503, json!({"message": "unavailable"})),
        call(5, "kubernetes", 429, json!({"message": "slow down"})),
        call(6, "zendesk", 200, json!({"isError": true})),
        call(
            7,
            "zendesk",
            200,
            json!({"jsonrpc": "2.0", "error": {"code": -32602}}),
        ),
    ];
    let world = world();
    let mut assessment = assess(&world, &contract(json!({})), &journal);
    assert_eq!(assessment.api_calls, 7);
    assert_eq!(
        assessment.refused_calls, 4,
        "404, 422 and two errors in a 2xx reply"
    );
    assert_eq!(assessment.service_error_calls, 2, "503 and 429");
    assert_eq!(assessment.task_service_calls, None);
    assert!(!assessment.metrics().contains_key("task_service_calls"));

    let calls: Vec<&JournalEntry> = journal.iter().collect();
    assessment.attribute_calls(&calls, &["github".to_owned()]);
    assert_eq!(assessment.task_service_calls, Some(3));
    let metrics = assessment.metrics();
    assert_eq!(metrics["task_service_calls"], 3.0);
    assert_eq!(metrics["refused_calls"], 4.0);
    assert_eq!(metrics["service_error_calls"], 2.0);

    // A request that did not come through the tools container is not the agent's call.
    let mut outside = call(1, "github", 500, Value::Null);
    outside.source = "203.0.113.7".into();
    let assessment = assess(&world, &contract(json!({})), &[outside]);
    assert_eq!(
        (assessment.api_calls, assessment.service_error_calls),
        (0, 0)
    );
}

#[test]
fn sealed_evidence_names_the_tasks_own_services() {
    let directory =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/example/github/basic");
    let dataset = Dataset::load(&directory).unwrap();
    let contract = dataset.contract.clone();
    let mut world = WorldState::from_dataset(dataset).unwrap();
    world.seal().unwrap();
    let evidence = WorldEvidence::capture(&world, &[], Ok(vec!["192.0.2.10".into()]));
    assert_eq!(evidence.task_services, ["github"]);
    let assessment = evidence.assess(&contract, None, false);
    assert_eq!(assessment.task_service_calls, Some(0), "{assessment:?}");
    assert_eq!(assessment.metrics()["task_service_calls"], 0.0);
}
