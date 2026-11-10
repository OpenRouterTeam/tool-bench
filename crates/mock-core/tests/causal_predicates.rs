use std::path::Path;

use mock_core::{
    dataset::Dataset,
    events::{self, HistoryFailureKind, OperationKind, OrderingCheck},
    store::Store,
    world::{SharedWorld, WorldState, shared},
};
use serde_json::{Value, json};

fn world(value: Value) -> SharedWorld {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/example/github/basic");
    let mut dataset = Dataset::load(&root).unwrap();
    let mut store = Store::default();
    store
        .collection_mut("records")
        .insert("target", json!({"id":"target","value":value,"done":false}));
    dataset.services.insert("workflow".into(), store);
    shared(WorldState::from_dataset(dataset).unwrap())
}

fn set(world: &SharedWorld, field: &str, value: Value) {
    let operation = world
        .begin_operation(OperationKind::Http, "workflow", "tools", "PUT", "/record")
        .unwrap();
    events::sync_scope(Some(operation.context().clone()), || {
        world
            .lock()
            .unwrap()
            .service_mut("workflow")
            .store
            .collection_mut("records")
            .get_mut("target")
            .unwrap()[field] = value;
    });
    operation.finish(200, true);
}

fn definition(predicate: Value) -> Value {
    let mut prerequisite = json!({
        "selector":{"collection":"/services/workflow/records","matching":{"id":"target"}},
        "field":"/value"
    });
    prerequisite
        .as_object_mut()
        .unwrap()
        .extend(predicate.as_object().unwrap().clone());
    json!({
        "prerequisite":prerequisite,
        "action":{
            "selector":{"collection":"/services/workflow/records","matching":{"id":"target"}},
            "field":"/done","equals":true
        }
    })
}

fn check(world: &SharedWorld, predicate: Value) -> Vec<events::HistoryFailure> {
    let ordering: OrderingCheck = serde_json::from_value(definition(predicate)).unwrap();
    let world = world.lock().unwrap();
    events::audit(
        &world.initial_snapshot(),
        &world.snapshot(),
        world.events().entries(),
        None,
        &[ordering],
    )
    .unwrap()
}

#[test]
fn nonempty_causal_text_does_not_prescribe_wording_but_must_exist_before_the_action() {
    for body in ["The changelog was updated.", "Added in commit 24f172e5."] {
        let trial = world(json!(""));
        set(&trial, "value", json!(body));
        set(&trial, "done", json!(true));
        assert!(check(&trial, json!({"nonempty_text":true})).is_empty());
    }
    for body in [json!(""), json!(" \n\t"), Value::Null, json!(42)] {
        let trial = world(body);
        set(&trial, "done", json!(true));
        set(
            &trial,
            "value",
            json!("A later reply cannot repair early resolution."),
        );
        assert!(
            check(&trial, json!({"nonempty_text":true}))
                .iter()
                .any(|failure| failure.kind == HistoryFailureKind::Ordering)
        );
    }
}

#[test]
fn numeric_causal_prerequisites_use_exact_values_and_cannot_be_repaired_later() {
    for amount in [json!(176500), json!(176500.0), json!("176500.00")] {
        let trial = world(json!(184000));
        set(&trial, "value", amount);
        set(&trial, "done", json!(true));
        assert!(check(&trial, json!({"numeric_equals":"176500"})).is_empty());
    }
    let trial = world(json!(184000));
    set(&trial, "done", json!(true));
    set(&trial, "value", json!(176500));
    assert!(
        check(&trial, json!({"numeric_equals":"176500"}))
            .iter()
            .any(|failure| failure.kind == HistoryFailureKind::Ordering)
    );
}

#[test]
fn exact_causal_array_membership_preserves_other_approvals_and_does_not_match_substrings() {
    let trial = world(json!(["mei.chen"]));
    set(&trial, "value", json!(["mei.chen", "sam.okafor"]));
    set(&trial, "done", json!(true));
    assert!(check(&trial, json!({"has_value":"sam.okafor"})).is_empty());
    for approvals in [
        json!(["sam.okafor-bot"]),
        json!("sam.okafor"),
        json!([41]),
        json!([]),
    ] {
        let trial = world(approvals);
        set(&trial, "done", json!(true));
        assert!(
            check(&trial, json!({"has_value":"sam.okafor"}))
                .iter()
                .any(|failure| failure.kind == HistoryFailureKind::Ordering)
        );
    }
}

#[test]
fn causal_predicates_require_one_explicit_meaningful_condition() {
    for predicate in [
        json!({}),
        json!({"nonempty_text":false}),
        json!({"equals":"x","has_value":"x"}),
        json!({"equals":"x","nonempty_text":true}),
        json!({"has_value":"x","nonempty_text":true}),
    ] {
        if let Ok(check) = serde_json::from_value::<OrderingCheck>(definition(predicate)) {
            assert!(check.validate().is_err());
        }
    }
}

#[test]
fn explicit_null_equality_retains_its_old_contract_meaning_and_serialization() {
    let trial = world(Value::Null);
    set(&trial, "done", json!(true));
    let ordering: OrderingCheck =
        serde_json::from_value(definition(json!({"equals":null}))).unwrap();
    ordering.validate().unwrap();
    let serialized = serde_json::to_value(ordering).unwrap();
    assert!(serialized["prerequisite"].get("equals").unwrap().is_null());
    assert!(serialized["prerequisite"].get("has_value").is_none());
    assert!(serialized["prerequisite"].get("nonempty_text").is_none());
    assert!(check(&trial, json!({"equals":null})).is_empty());
}
