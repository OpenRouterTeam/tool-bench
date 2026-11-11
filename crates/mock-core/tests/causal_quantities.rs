use std::path::Path;

use mock_core::{
    dataset::Dataset,
    events::{self, HistoryError, OperationKind, OrderingCheck},
    store::Store,
    world::{SharedWorld, WorldState, shared},
};
use serde_json::{Value, json};

fn rule() -> Value {
    json!({
        "prerequisite":{
            "selector":{"collection":"/services/kubernetes/containers","matching":{"name":"worker"}},
            "field":"/memory","quantity_equals":"1Gi"
        },
        "action":{
            "selector":{"collection":"/services/sentry/issues","matching":{"id":"target"}},
            "field":"/status","equals":"resolved"
        }
    })
}

fn world(memory: Value) -> SharedWorld {
    let directory =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/example/github/basic");
    let mut dataset = Dataset::load(&directory).unwrap();
    let mut kubernetes = Store::default();
    kubernetes
        .collection_mut("containers")
        .insert("worker", json!({"name":"worker","memory":memory}));
    dataset.services.insert("kubernetes".into(), kubernetes);
    let mut sentry = Store::default();
    sentry
        .collection_mut("issues")
        .insert("target", json!({"id":"target","status":"unresolved"}));
    dataset.services.insert("sentry".into(), sentry);
    shared(WorldState::from_dataset(dataset).unwrap())
}

fn change(
    world: &SharedWorld,
    service: &str,
    collection: &str,
    key: &str,
    field: &str,
    value: Value,
) {
    let operation = world
        .begin_operation(OperationKind::Http, service, "tools", "PUT", "/operation")
        .unwrap();
    events::sync_scope(Some(operation.context().clone()), || {
        world
            .lock()
            .unwrap()
            .service_mut(service)
            .store
            .collection_mut(collection)
            .get_mut(key)
            .unwrap()[field] = value;
    });
    operation.finish(200, true);
}

fn assess(world: &SharedWorld, definition: Value) -> Result<usize, HistoryError> {
    let check: OrderingCheck = serde_json::from_value(definition).unwrap();
    check.validate().unwrap();
    let world = world.lock().unwrap();
    events::audit(
        &world.initial_snapshot(),
        &world.snapshot(),
        world.events().entries(),
        None,
        &[check],
    )
    .map(|failures| failures.len())
}

fn resolve(world: &SharedWorld) {
    change(
        world,
        "sentry",
        "issues",
        "target",
        "status",
        json!("resolved"),
    );
}

#[test]
fn causal_quantity_contracts_validate_exactly_one_supported_predicate() {
    let valid: OrderingCheck = serde_json::from_value(rule()).unwrap();
    valid.validate().unwrap();
    for expected in [
        Value::Null,
        json!(false),
        json!("nonsense"),
        json!("1e1000001"),
    ] {
        let mut definition = rule();
        definition["prerequisite"]["quantity_equals"] = expected;
        let check: OrderingCheck = serde_json::from_value(definition).unwrap();
        assert!(check.validate().is_err());
    }
    for (predicate, value) in [
        ("equals", json!("1Gi")),
        ("numeric_equals", json!(1073741824)),
        ("nonempty_text", json!(true)),
        ("has_value", json!("1Gi")),
    ] {
        let mut definition = rule();
        definition["prerequisite"][predicate] = value;
        let check: OrderingCheck = serde_json::from_value(definition).unwrap();
        assert!(check.validate().is_err(), "{predicate}");
    }
}

#[test]
fn equivalent_prerequisites_accept_native_quantity_spellings_and_numeric_values() {
    for quantity in [
        json!("1Gi"),
        json!("1024Mi"),
        json!("1073741824"),
        json!(1073741824),
        json!("1.073741824e9"),
    ] {
        let world = world(json!("512Mi"));
        change(
            &world,
            "kubernetes",
            "containers",
            "worker",
            "memory",
            quantity,
        );
        resolve(&world);
        assert_eq!(assess(&world, rule()).unwrap(), 0);
    }
}

#[test]
fn wrong_invalid_or_absent_prerequisites_do_not_satisfy_quantity_ordering() {
    for quantity in [
        json!("512Mi"),
        json!("1G"),
        json!("2Gi"),
        json!("garbage"),
        Value::Null,
        json!(true),
    ] {
        let world = world(quantity);
        resolve(&world);
        assert_eq!(assess(&world, rule()).unwrap(), 1);
    }
    let world = world(json!("1Gi"));
    resolve(&world);
    let mut definition = rule();
    definition["prerequisite"]["field"] = json!("/absent");
    assert_eq!(assess(&world, definition).unwrap(), 1);
}

#[test]
fn later_remediation_does_not_repair_earlier_premature_resolution() {
    let world = world(json!("512Mi"));
    resolve(&world);
    change(
        &world,
        "kubernetes",
        "containers",
        "worker",
        "memory",
        json!("1024Mi"),
    );
    assert_eq!(assess(&world, rule()).unwrap(), 1);
}

#[test]
fn already_correct_quantities_do_not_force_pointless_rewrites() {
    let world = world(json!("1024Mi"));
    resolve(&world);
    assert_eq!(assess(&world, rule()).unwrap(), 0);
    let mut definition = rule();
    definition["require_transition"] = json!(true);
    assert_eq!(assess(&world, definition).unwrap(), 1);
}

#[test]
fn unsupported_causal_quantity_evidence_requires_adjudication_not_a_model_failure() {
    for quantity in [json!("1e1000001"), json!("1".repeat(4097))] {
        let world = world(quantity);
        resolve(&world);
        assert!(matches!(
            assess(&world, rule()),
            Err(HistoryError::MissingEvidence { .. })
        ));
    }
}
