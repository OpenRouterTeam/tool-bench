use std::path::Path;

use mock_core::{
    dataset::Dataset,
    events::{self, HistoryError, HistoryFailureKind, OperationKind, OrderingCheck},
    store::Store,
    world::{SharedWorld, WorldState, shared},
};
use serde_json::{Value, json};

fn world(memberships: usize) -> SharedWorld {
    world_with_retained(memberships, true)
}

fn world_with_retained(memberships: usize, retained: bool) -> SharedWorld {
    let directory =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/example/github/basic");
    let mut dataset = Dataset::load(&directory).unwrap();
    let mut linear = Store::default();
    linear.collection_mut("team_memberships");
    for index in 0..memberships {
        linear.collection_mut("team_memberships").insert(
            format!("membership-{index}"),
            json!({"userId":"offboarded","teamId":format!("team-{index}")}),
        );
    }
    if retained {
        linear.collection_mut("team_memberships").insert(
            "unrelated",
            json!({"userId":"retained","teamId":"other-team"}),
        );
    }
    dataset.services.insert("linear".into(), linear);
    dataset.services.insert("slack".into(), Store::default());
    shared(WorldState::from_dataset(dataset).unwrap())
}

fn definition() -> Value {
    json!({
        "prerequisite":{
            "selector":{
                "collection":"/services/linear/team_memberships",
                "matching":{"userId":"offboarded"}
            },
            "record_count":0
        },
        "action":{
            "selector":{"collection":"/services/slack/messages","created":true},
            "field":"/text","nonempty_text":true
        }
    })
}

fn operation(world: &SharedWorld, service: &str, mutate: impl FnOnce(&SharedWorld)) {
    let operation = world
        .begin_operation(OperationKind::Http, service, "tools", "POST", "/operation")
        .unwrap();
    events::sync_scope(Some(operation.context().clone()), || mutate(world));
    operation.finish(200, true);
}

fn remove(world: &SharedWorld, identity: &str) {
    operation(world, "linear", |world| {
        world
            .lock()
            .unwrap()
            .service_mut("linear")
            .store
            .collection_mut("team_memberships")
            .remove(identity);
    });
}

fn notify(world: &SharedWorld, identity: &str) {
    operation(world, "slack", |world| {
        world
            .lock()
            .unwrap()
            .service_mut("slack")
            .store
            .collection_mut("messages")
            .insert(identity, json!({"text":"Offboarding completed."}));
    });
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
    .map(|failures| {
        assert!(
            failures
                .iter()
                .all(|failure| failure.kind == HistoryFailureKind::Ordering)
        );
        failures.len()
    })
}

#[test]
fn causal_counts_validate_without_a_field_or_competing_predicates() {
    let check: OrderingCheck = serde_json::from_value(definition()).unwrap();
    check.validate().unwrap();
    for (field, value) in [
        ("field", json!("/userId")),
        ("equals", json!(0)),
        ("nonempty_text", json!(true)),
        ("has_value", json!("offboarded")),
    ] {
        let mut invalid = definition();
        invalid["prerequisite"][field] = value;
        let check: OrderingCheck = serde_json::from_value(invalid).unwrap();
        assert!(check.validate().is_err(), "{field}");
    }
}

#[test]
fn complete_removal_must_precede_every_notice() {
    for remove_first in [false, true] {
        let world = world(2);
        remove(&world, "membership-0");
        if !remove_first {
            notify(&world, "early");
        }
        remove(&world, "membership-1");
        notify(&world, "completed");
        assert_eq!(
            assess(&world, definition()).unwrap(),
            usize::from(!remove_first)
        );
    }
}

#[test]
fn restoring_a_membership_invalidates_a_later_notice_even_if_removed_again() {
    let world = world(1);
    remove(&world, "membership-0");
    notify(&world, "first");
    operation(&world, "linear", |world| {
        world
            .lock()
            .unwrap()
            .service_mut("linear")
            .store
            .collection_mut("team_memberships")
            .insert(
                "membership-0",
                json!({"userId":"offboarded","teamId":"team-0"}),
            );
    });
    notify(&world, "premature");
    remove(&world, "membership-0");
    assert_eq!(assess(&world, definition()).unwrap(), 1);
}

#[test]
fn initially_empty_scope_does_not_invent_a_required_transition() {
    let world = world(0);
    notify(&world, "completed");
    assert_eq!(assess(&world, definition()).unwrap(), 0);
    let mut transition = definition();
    transition["require_transition"] = json!(true);
    assert_eq!(assess(&world, transition).unwrap(), 1);
}

#[test]
fn authored_empty_collections_and_removal_of_the_last_record_are_proven_absence() {
    let empty = world_with_retained(0, false);
    notify(&empty, "completed");
    assert_eq!(assess(&empty, definition()).unwrap(), 0);

    let last = world_with_retained(1, false);
    remove(&last, "membership-0");
    notify(&last, "completed");
    assert_eq!(assess(&last, definition()).unwrap(), 0);
}

#[test]
fn same_commit_removal_requires_explicit_atomic_permission() {
    let world = world(1);
    operation(&world, "linear", |world| {
        let mut world = world.lock().unwrap();
        world
            .service_mut("linear")
            .store
            .collection_mut("team_memberships")
            .remove("membership-0");
        world
            .service_mut("slack")
            .store
            .collection_mut("messages")
            .insert("completed", json!({"text":"Offboarding completed."}));
    });
    assert!(matches!(
        assess(&world, definition()),
        Err(HistoryError::MissingEvidence { .. })
    ));
    let mut atomic = definition();
    atomic["allow_same_commit"] = json!(true);
    assert_eq!(assess(&world, atomic).unwrap(), 0);
}

#[test]
fn absent_collection_is_missing_evidence_not_a_proven_empty_scope() {
    let world = world(0);
    notify(&world, "completed");
    let mut missing = definition();
    missing["prerequisite"]["selector"]["collection"] =
        json!("/services/linear/missing_memberships");
    assert!(matches!(
        assess(&world, missing),
        Err(HistoryError::MissingEvidence { .. })
    ));
}
