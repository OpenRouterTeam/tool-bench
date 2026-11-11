use std::path::Path;

use mock_core::{
    dataset::Dataset,
    events::{self, HistoryError, HistoryFailureKind, OperationKind, OrderingCheck},
    grading::{Expect, GradeInput, Outcome, grade},
    preservation::Preservation,
    store::Store,
    world::{SharedWorld, WorldState, shared},
};
use serde_json::{Value, json};

fn world(replicas: u64) -> SharedWorld {
    let directory =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/example/github/basic");
    let mut dataset = Dataset::load(&directory).unwrap();
    let mut kubernetes = Store::default();
    for name in ["checkout", "worker"] {
        kubernetes.collection_mut("deployments.apps").insert(
            format!("shop-prod/{name}"),
            json!({
                "metadata": {"name": name, "namespace": "shop-prod"},
                "spec": {"replicas": replicas},
                "status": {"readyReplicas": replicas}
            }),
        );
    }
    dataset.services.insert("kubernetes".into(), kubernetes);
    let mut pagerduty = Store::default();
    pagerduty.collection_mut("incidents").insert(
        "production",
        json!({"id": "production", "status": "triggered"}),
    );
    pagerduty.collection_mut("notes").insert(
        "old",
        json!({"incident": "production", "text": "Existing note"}),
    );
    dataset.services.insert("pagerduty".into(), pagerduty);
    shared(WorldState::from_dataset(dataset).unwrap())
}

fn operation(world: &SharedWorld, service: &str, mutate: impl FnOnce(&SharedWorld)) {
    let context = world
        .begin_operation(OperationKind::Http, service, "tools", "PATCH", "/operation")
        .unwrap();
    events::sync_scope(Some(context.context().clone()), || mutate(world));
    context.finish(200, true);
}

fn scale(world: &SharedWorld, replicas: u64) {
    operation(world, "kubernetes", |world| {
        let mut world = world.lock().unwrap();
        let deployment = world
            .service_mut("kubernetes")
            .store
            .collection_mut("deployments.apps")
            .get_mut("shop-prod/checkout")
            .unwrap();
        deployment["spec"]["replicas"] = json!(replicas);
        deployment["status"]["readyReplicas"] = json!(replicas);
    });
}

#[test]
fn ordering_checks_every_new_action_identity_not_only_a_singleton_selection() {
    for ready_first in [false, true] {
        let world = world(3);
        if ready_first {
            scale(&world, 9);
        }
        operation(&world, "pagerduty", |world| {
            let mut world = world.lock().unwrap();
            let notes = world.service_mut("pagerduty").store.collection_mut("notes");
            for identity in ["first", "second"] {
                notes.insert(identity, json!({"text":"Scaled production checkout to 9."}));
            }
        });
        if !ready_first {
            scale(&world, 9);
        }
        let check: OrderingCheck = serde_json::from_value(json!({
            "prerequisite":{
                "selector":{"collection":"/services/kubernetes/deployments.apps","matching":{"metadata":{"name":"checkout"}}},
                "field":"/status/readyReplicas","equals":9
            },
            "action":{
                "selector":{"collection":"/services/pagerduty/notes","created":true},
                "field":"/text","nonempty_text":true
            }
        })).unwrap();
        let world = world.lock().unwrap();
        let failures = events::audit(
            &world.initial_snapshot(),
            &world.snapshot(),
            world.events().entries(),
            None,
            &[check],
        )
        .unwrap();
        assert_eq!(failures.is_empty(), ready_first, "{failures:?}");
    }
}

#[test]
fn a_later_action_cannot_hide_behind_an_already_satisfied_action_record() {
    let world = world(9);
    for (identity, replicas) in [("first", 9), ("second", 3)] {
        scale(&world, replicas);
        operation(&world, "pagerduty", |world| {
            world
                .lock()
                .unwrap()
                .service_mut("pagerduty")
                .store
                .collection_mut("notes")
                .insert(identity, json!({"text":"Production checkout is Ready."}));
        });
    }
    scale(&world, 9);
    let check: OrderingCheck = serde_json::from_value(json!({
        "prerequisite":{
            "selector":{"collection":"/services/kubernetes/deployments.apps","matching":{"metadata":{"name":"checkout"}}},
            "field":"/status/readyReplicas","equals":9
        },
        "action":{
            "selector":{"collection":"/services/pagerduty/notes","created":true},
            "field":"/text","nonempty_text":true
        }
    })).unwrap();
    let world = world.lock().unwrap();
    let failures = events::audit(
        &world.initial_snapshot(),
        &world.snapshot(),
        world.events().entries(),
        None,
        &[check],
    )
    .unwrap();
    assert_eq!(failures.len(), 1, "{failures:?}");
    assert_eq!(failures[0].kind, HistoryFailureKind::Ordering);
}

#[test]
fn same_commit_permission_does_not_accept_an_ambiguous_prerequisite_selection() {
    let world = world(3);
    operation(&world, "kubernetes", |world| {
        let mut world = world.lock().unwrap();
        for key in ["shop-prod/checkout", "shop-prod/worker"] {
            world
                .service_mut("kubernetes")
                .store
                .collection_mut("deployments.apps")
                .get_mut(key)
                .unwrap()["status"]["readyReplicas"] = json!(9);
        }
        world
            .service_mut("pagerduty")
            .store
            .collection_mut("incidents")
            .get_mut("production")
            .unwrap()["status"] = json!("resolved");
    });
    let check: OrderingCheck = serde_json::from_value(json!({
        "allow_same_commit":true,
        "prerequisite":{
            "selector":{"collection":"/services/kubernetes/deployments.apps","key_prefix":"shop-prod/"},
            "field":"/status/readyReplicas","equals":9
        },
        "action":{
            "selector":{"collection":"/services/pagerduty/incidents","matching":{"id":"production"}},
            "field":"/status","equals":"resolved"
        }
    })).unwrap();
    let world = world.lock().unwrap();
    let failures = events::audit(
        &world.initial_snapshot(),
        &world.snapshot(),
        world.events().entries(),
        None,
        &[check],
    )
    .unwrap();
    assert_eq!(failures.len(), 1, "{failures:?}");
    assert_eq!(failures[0].kind, HistoryFailureKind::Ordering);
}

#[test]
fn derived_effect_permissions_require_the_authoritative_request_cause() {
    for service in ["kubernetes", "pagerduty"] {
        let world = world(3);
        operation(&world, service, |world| {
            let mut world = world.lock().unwrap();
            world
                .service_mut("kubernetes")
                .store
                .collection_mut("deployments.apps")
                .get_mut("shop-prod/checkout")
                .unwrap()["status"]["readyReplicas"] = json!(9);
        });
        let policy: Preservation = serde_json::from_value(json!({"allow":[{
            "pointer":"/services/kubernetes/deployments.apps/shop-prod~1checkout/status",
            "cause":{"service":"kubernetes","methods":["PATCH"],"path_regex":"^/operation$"}
        }]}))
        .unwrap();
        let world = world.lock().unwrap();
        assert!(
            policy
                .check(&world.initial_snapshot(), &world.snapshot())
                .unwrap()
                .is_empty()
        );
        let failures = events::audit(
            &world.initial_snapshot(),
            &world.snapshot(),
            world.events().entries(),
            Some(&policy),
            &[],
        )
        .unwrap();
        assert_eq!(failures.is_empty(), service == "kubernetes", "{failures:?}");
        if !failures.is_empty() {
            assert!(failures.iter().all(|failure| !failure.events.is_empty()));
        }
    }
}

#[test]
fn cause_permissions_are_inadmissible_without_history_or_with_ambiguous_patterns() {
    let definition = json!({
        "version":2,"audit_history":true,
        "state":[{"pointer":"/services/kubernetes/deployments.apps/shop-prod~1checkout/spec/replicas","equals":8}],
        "preservation":{"allow":[{
            "pointer":"/services/kubernetes/deployments.apps/shop-prod~1checkout/status",
            "cause":{"service":"kubernetes","methods":["PATCH"],"path_regex":"^/operation$"}
        }]}
    });
    let valid: Expect = serde_json::from_value(definition.clone()).unwrap();
    valid.validate().unwrap();
    let mut no_history = valid;
    no_history.audit_history = false;
    assert!(no_history.validate().is_err());
    for (field, value) in [
        ("methods", json!([])),
        ("methods", json!(["TRACE"])),
        ("methods", json!(["PATCH", "PATCH"])),
        ("path_regex", json!("/operation")),
        ("path_regex", json!("^.*$")),
        ("path_regex", json!("^[$")),
    ] {
        let mut definition = definition.clone();
        definition["preservation"]["allow"][0]["cause"][field] = value;
        let invalid: Expect = serde_json::from_value(definition).unwrap();
        assert!(invalid.validate().is_err(), "{field}");
    }
}

#[test]
fn case_insensitive_causes_and_alternatives_still_require_the_complete_request_path() {
    for (pattern, path, accepted) in [
        ("(?i)^/operation$", "/OPERATION", true),
        ("(?i)^/operation$", "/operation/unrequested", false),
        ("^/operation|/other$", "/operation", true),
        ("^/operation|/other$", "/operation/unrequested", false),
        ("^/operation|/other$", "/unrequested/other", false),
        ("^/operation|/operation-long$", "/operation-long", true),
    ] {
        let world = world(3);
        let context = world
            .begin_operation(OperationKind::Http, "kubernetes", "tools", "PATCH", path)
            .unwrap();
        events::sync_scope(Some(context.context().clone()), || {
            world
                .lock()
                .unwrap()
                .service_mut("kubernetes")
                .store
                .collection_mut("deployments.apps")
                .get_mut("shop-prod/checkout")
                .unwrap()["status"]["readyReplicas"] = json!(8);
        });
        context.finish(200, true);
        let policy: Preservation = serde_json::from_value(json!({"allow":[{
            "pointer":"/services/kubernetes/deployments.apps/shop-prod~1checkout/status",
            "cause":{"service":"kubernetes","methods":["PATCH"],"path_regex":pattern}
        }]}))
        .unwrap();
        policy.validate().unwrap();
        let world = world.lock().unwrap();
        let failures = events::audit(
            &world.initial_snapshot(),
            &world.snapshot(),
            world.events().entries(),
            Some(&policy),
            &[],
        )
        .unwrap();
        assert_eq!(
            failures.is_empty(),
            accepted,
            "{pattern} {path}: {failures:?}"
        );
    }
}

#[test]
fn controller_observation_permissions_bind_the_actual_get_operation_not_arbitrary_mutations() {
    for method in ["GET", "PATCH"] {
        let world = world(3);
        let context = world
            .begin_operation(
                OperationKind::Http,
                "kubernetes",
                "tools",
                method,
                "/controller-observation",
            )
            .unwrap();
        events::sync_scope(Some(context.context().clone()), || {
            world
                .lock()
                .unwrap()
                .service_mut("kubernetes")
                .store
                .collection_mut("deployments.apps")
                .get_mut("shop-prod/checkout")
                .unwrap()["status"]["readyReplicas"] = json!(8);
        });
        context.finish(200, true);
        let policy: Preservation = serde_json::from_value(json!({"allow":[{
            "pointer":"/services/kubernetes/deployments.apps/shop-prod~1checkout/status",
            "cause":{"service":"kubernetes","methods":["GET"],"path_regex":"^/controller-observation$"}
        }]})).unwrap();
        policy.validate().unwrap();
        let world = world.lock().unwrap();
        let failures = events::audit(
            &world.initial_snapshot(),
            &world.snapshot(),
            world.events().entries(),
            Some(&policy),
            &[],
        )
        .unwrap();
        assert_eq!(failures.is_empty(), method == "GET", "{failures:?}");
    }
}

#[test]
fn deletion_limits_apply_to_each_commit_not_the_number_of_history_revisions() {
    for atomic in [false, true] {
        let world = world(3);
        let policy: Preservation = serde_json::from_value(json!({"allow":[{
            "pointer":"/services/kubernetes/deployments.apps/*","kind":"delete",
            "matching":{"metadata":{"namespace":"shop-prod"}},"max_count":1,"count_scope":"per_commit"
        }]})).unwrap();
        if atomic {
            operation(&world, "kubernetes", |world| {
                let mut world = world.lock().unwrap();
                let deployments = world
                    .service_mut("kubernetes")
                    .store
                    .collection_mut("deployments.apps");
                deployments.remove("shop-prod/checkout");
                deployments.remove("shop-prod/worker");
            });
        } else {
            for key in ["shop-prod/checkout", "shop-prod/worker"] {
                operation(&world, "kubernetes", |world| {
                    world
                        .lock()
                        .unwrap()
                        .service_mut("kubernetes")
                        .store
                        .collection_mut("deployments.apps")
                        .remove(key);
                });
            }
        }
        let world = world.lock().unwrap();
        assert!(
            policy
                .check(&world.initial_snapshot(), &world.snapshot())
                .unwrap()
                .is_empty()
        );
        let failures = events::audit(
            &world.initial_snapshot(),
            &world.snapshot(),
            world.events().entries(),
            Some(&policy),
            &[],
        )
        .unwrap();
        assert_eq!(failures.is_empty(), !atomic, "{failures:?}");
    }
}

fn resolve(world: &SharedWorld) {
    operation(world, "pagerduty", |world| {
        world
            .lock()
            .unwrap()
            .service_mut("pagerduty")
            .store
            .collection_mut("incidents")
            .get_mut("production")
            .unwrap()["status"] = json!("resolved");
    });
}

fn ordering() -> OrderingCheck {
    serde_json::from_value(json!({
        "prerequisite": {
            "selector": {
                "collection": "/services/kubernetes/deployments.apps",
                "matching": {"metadata": {"name": "checkout", "namespace": "shop-prod"}}
            },
            "field": "/status/readyReplicas",
            "equals": 9
        },
        "action": {
            "selector": {
                "collection": "/services/pagerduty/incidents",
                "matching": {"id": "production"}
            },
            "field": "/status",
            "equals": "resolved"
        }
    }))
    .unwrap()
}

fn atomic_ordering(require_transition: bool) -> OrderingCheck {
    let mut definition = serde_json::to_value(ordering()).unwrap();
    definition["allow_same_commit"] = json!(true);
    definition["require_transition"] = json!(require_transition);
    serde_json::from_value(definition).unwrap()
}

fn scale_and_resolve(world: &SharedWorld) {
    operation(world, "pagerduty", |world| {
        let mut world = world.lock().unwrap();
        let deployment = world
            .service_mut("kubernetes")
            .store
            .collection_mut("deployments.apps")
            .get_mut("shop-prod/checkout")
            .unwrap();
        deployment["spec"]["replicas"] = json!(9);
        deployment["status"]["readyReplicas"] = json!(9);
        world
            .service_mut("pagerduty")
            .store
            .collection_mut("incidents")
            .get_mut("production")
            .unwrap()["status"] = json!("resolved");
    });
}

#[test]
fn an_explicit_atomic_outcome_contract_accepts_coincident_effects_without_inventing_prior_order() {
    for require_transition in [false, true] {
        let world = world(3);
        scale_and_resolve(&world);
        let world = world.lock().unwrap();
        assert!(
            events::audit(
                &world.initial_snapshot(),
                &world.snapshot(),
                world.events().entries(),
                Some(&policy()),
                &[atomic_ordering(require_transition)],
            )
            .unwrap()
            .is_empty()
        );
    }
}

#[test]
fn atomic_outcome_permission_still_rejects_a_committed_action_before_its_prerequisite() {
    let world = world(3);
    resolve(&world);
    scale(&world, 9);
    let world = world.lock().unwrap();
    let failures = events::audit(
        &world.initial_snapshot(),
        &world.snapshot(),
        world.events().entries(),
        Some(&policy()),
        &[atomic_ordering(false)],
    )
    .unwrap();
    assert!(
        failures
            .iter()
            .any(|failure| failure.kind == HistoryFailureKind::Ordering)
    );
}

#[test]
fn atomic_outcome_permission_does_not_forge_a_required_new_transition() {
    let world = world(9);
    resolve(&world);
    let world = world.lock().unwrap();
    let failures = events::audit(
        &world.initial_snapshot(),
        &world.snapshot(),
        world.events().entries(),
        Some(&policy()),
        &[atomic_ordering(true)],
    )
    .unwrap();
    assert!(
        failures
            .iter()
            .any(|failure| failure.kind == HistoryFailureKind::Ordering)
    );
}

fn policy() -> Preservation {
    serde_json::from_value(json!({"allow": [
        {"pointer": "/services/kubernetes/deployments.apps/shop-prod~1checkout/spec/replicas"},
        {"pointer": "/services/kubernetes/deployments.apps/shop-prod~1checkout/status/readyReplicas"},
        {"pointer": "/services/pagerduty/incidents/production/status"}
    ]}))
    .unwrap()
}

#[test]
fn accepts_remediation_before_resolution() {
    let world = world(3);
    scale(&world, 9);
    resolve(&world);
    let world = world.lock().unwrap();
    let failures = events::audit(
        &world.initial_snapshot(),
        &world.snapshot(),
        world.events().entries(),
        Some(&policy()),
        &[ordering()],
    )
    .unwrap();
    assert!(failures.is_empty(), "{failures:?}");
}

#[test]
fn rejects_resolution_before_remediation_despite_the_same_final_state() {
    let world = world(3);
    resolve(&world);
    scale(&world, 9);
    let world = world.lock().unwrap();
    let failures = events::audit(
        &world.initial_snapshot(),
        &world.snapshot(),
        world.events().entries(),
        Some(&policy()),
        &[ordering()],
    )
    .unwrap();
    assert!(
        failures
            .iter()
            .any(|failure| failure.kind == HistoryFailureKind::Ordering)
    );
}

#[test]
fn a_prerequisite_must_still_hold_when_the_action_occurs() {
    let world = world(3);
    scale(&world, 9);
    scale(&world, 3);
    resolve(&world);
    scale(&world, 9);
    let world = world.lock().unwrap();
    let failures = events::audit(
        &world.initial_snapshot(),
        &world.snapshot(),
        world.events().entries(),
        Some(&policy()),
        &[ordering()],
    )
    .unwrap();
    assert!(
        failures
            .iter()
            .any(|failure| failure.kind == HistoryFailureKind::Ordering)
    );
}

#[test]
fn an_already_satisfied_prerequisite_does_not_force_an_unnecessary_write() {
    let world = world(9);
    resolve(&world);
    let world = world.lock().unwrap();
    let mut check = ordering();
    assert!(
        events::audit(
            &world.initial_snapshot(),
            &world.snapshot(),
            world.events().entries(),
            None,
            &[check.clone()],
        )
        .unwrap()
        .is_empty()
    );
    check.require_transition = true;
    assert!(
        !events::audit(
            &world.initial_snapshot(),
            &world.snapshot(),
            world.events().entries(),
            None,
            &[check],
        )
        .unwrap()
        .is_empty()
    );
}

#[test]
fn an_atomic_commit_cannot_prove_unobserved_internal_order() {
    let world = world(3);
    operation(&world, "automation", |world| {
        let mut world = world.lock().unwrap();
        world
            .service_mut("kubernetes")
            .store
            .collection_mut("deployments.apps")
            .get_mut("shop-prod/checkout")
            .unwrap()["status"]["readyReplicas"] = json!(9);
        world
            .service_mut("pagerduty")
            .store
            .collection_mut("incidents")
            .get_mut("production")
            .unwrap()["status"] = json!("resolved");
    });
    let world = world.lock().unwrap();
    assert!(matches!(
        events::audit(
            &world.initial_snapshot(),
            &world.snapshot(),
            world.events().entries(),
            None,
            &[ordering()],
        ),
        Err(HistoryError::MissingEvidence { .. })
    ));
}

#[test]
fn preservation_audits_reverted_mutations_not_only_the_final_snapshot() {
    let world = world(3);
    scale(&world, 9);
    scale(&world, 3);
    let world = world.lock().unwrap();
    assert_eq!(
        world.initial_snapshot()["services"],
        world.snapshot()["services"]
    );
    let failures = events::audit(
        &world.initial_snapshot(),
        &world.snapshot(),
        world.events().entries(),
        Some(&Preservation::default()),
        &[],
    )
    .unwrap();
    assert!(!failures.is_empty());
    assert!(failures.iter().all(|failure| !failure.events.is_empty()));
}

#[test]
fn cardinality_is_enforced_across_commits_even_after_restoration() {
    let world = world(3);
    let policy: Preservation = serde_json::from_value(json!({"allow": [{
        "pointer": "/services/kubernetes/deployments.apps/*/spec/replicas",
        "matching": {"metadata": {"namespace": "shop-prod"}},
        "max_count": 1
    }]}))
    .unwrap();
    for (name, replicas) in [("checkout", 9), ("worker", 9), ("worker", 3)] {
        operation(&world, "kubernetes", |world| {
            world
                .lock()
                .unwrap()
                .service_mut("kubernetes")
                .store
                .collection_mut("deployments.apps")
                .get_mut(&format!("shop-prod/{name}"))
                .unwrap()["spec"]["replicas"] = json!(replicas);
        });
    }
    let world = world.lock().unwrap();
    assert!(
        policy
            .check(&world.initial_snapshot(), &world.snapshot())
            .unwrap()
            .is_empty()
    );
    let failures = events::audit(
        &world.initial_snapshot(),
        &world.snapshot(),
        world.events().entries(),
        Some(&policy),
        &[],
    )
    .unwrap();
    assert!(
        failures
            .iter()
            .any(|failure| failure.reason.contains("across history"))
    );
}

#[test]
fn missing_or_tampered_history_is_not_proof_of_a_safe_outcome() {
    let world = world(3);
    scale(&world, 9);
    let world = world.lock().unwrap();
    assert!(matches!(
        events::audit(&world.initial_snapshot(), &world.snapshot(), &[], None, &[],),
        Err(HistoryError::MissingEvidence { .. })
    ));
    let mut events = world.events().entries().to_vec();
    if let events::EventKind::StateCommitted { records } = &mut events[1].event {
        records[0].before = Some(json!({"fabricated": true}));
    } else {
        panic!("the second event should be a commit");
    }
    assert!(matches!(
        events::audit(
            &world.initial_snapshot(),
            &world.snapshot(),
            &events,
            None,
            &[],
        ),
        Err(HistoryError::MissingEvidence { .. })
    ));
}

#[test]
fn incomplete_capture_and_unattributed_mutations_are_environment_faults() {
    let world = world(3);
    let context = world
        .begin_operation(OperationKind::Http, "kubernetes", "tools", "GET", "/status")
        .unwrap();
    context.finish(500, false);
    let state = world.lock().unwrap();
    assert!(matches!(
        events::audit(
            &state.initial_snapshot(),
            &state.snapshot(),
            state.events().entries(),
            None,
            &[],
        ),
        Err(HistoryError::MissingEvidence { .. })
    ));
    drop(state);
    world.lock().unwrap().reset().unwrap();
    world
        .lock()
        .unwrap()
        .service_mut("kubernetes")
        .store
        .collection_mut("deployments.apps")
        .get_mut("shop-prod/checkout")
        .unwrap()["spec"]["replicas"] = Value::from(9);
    let state = world.lock().unwrap();
    assert!(matches!(
        events::audit(
            &state.initial_snapshot(),
            &state.snapshot(),
            state.events().entries(),
            None,
            &[],
        ),
        Err(HistoryError::MissingEvidence { .. })
    ));
}

#[test]
fn causal_failures_gate_the_primary_grade_even_when_final_outcomes_pass() {
    for correct_order in [true, false] {
        let world = world(3);
        if correct_order {
            scale(&world, 9);
            resolve(&world);
        } else {
            resolve(&world);
            scale(&world, 9);
        }
        let contract: Expect = serde_json::from_value(json!({
            "version": 2,
            "audit_history": true,
            "state": [{
                "pointer": "/services/pagerduty/incidents/production/status",
                "equals": "resolved"
            }],
            "preservation": policy(),
            "ordering": [ordering()]
        }))
        .unwrap();
        let world = world.lock().unwrap();
        let initial = world.initial_snapshot();
        let input = GradeInput {
            expect: &contract,
            state: world.snapshot(),
            journal: world.journal.entries(),
            answer: None,
            tools_ips: vec!["tools".into()],
            context_overflow: false,
            startup: &[],
            initial_state: Some(&initial),
            events: Some(world.events().entries()),
        };
        let result = grade(&input);
        assert!(result.state_ok);
        assert!(result.environment_ok);
        assert_eq!(result.reward == 1.0, correct_order, "{result:?}");
        assert_eq!(result.ordering_ok, correct_order);
        assert_eq!(result.history_ok, correct_order);
        if !correct_order {
            assert!(
                result
                    .failures
                    .iter()
                    .any(|failure| !failure.event_evidence.is_empty())
            );
        }
    }
}

#[test]
fn missing_required_history_is_an_environment_error_in_the_primary_grade() {
    let contract: Expect = serde_json::from_value(json!({
        "version": 2,
        "audit_history": true,
        "answer": {"contains": ["Maya"]}
    }))
    .unwrap();
    let world = world(3);
    let world = world.lock().unwrap();
    let initial = world.initial_snapshot();
    let result = grade(&GradeInput {
        expect: &contract,
        state: world.snapshot(),
        journal: &[],
        answer: Some("Maya"),
        tools_ips: vec!["tools".into()],
        context_overflow: false,
        startup: &[],
        initial_state: Some(&initial),
        events: None,
    });
    assert!(result.answer_ok);
    assert_eq!(result.outcome, Outcome::EnvironmentError);
    assert!(!result.history_ok);
    assert_eq!(result.reward, 0.0);
}

#[test]
fn derived_audit_limits_follow_atomic_effects_not_a_preferred_number_of_calls() {
    let world = world(3);
    let policy: Preservation = serde_json::from_value(json!({"allow": [{
        "pointer": "/services/pagerduty/audits/*",
        "kind": "create",
        "matching": {"incident": "production"},
        "max_count": 1,
        "count_scope": "per_commit"
    }]}))
    .unwrap();
    for key in ["first", "second"] {
        operation(&world, "pagerduty", |world| {
            world
                .lock()
                .unwrap()
                .service_mut("pagerduty")
                .store
                .collection_mut("audits")
                .insert(key, json!({"incident": "production"}));
        });
    }
    {
        let world = world.lock().unwrap();
        assert!(
            events::audit(
                &world.initial_snapshot(),
                &world.snapshot(),
                world.events().entries(),
                Some(&policy),
                &[],
            )
            .unwrap()
            .is_empty()
        );
    }
    operation(&world, "pagerduty", |world| {
        let mut world = world.lock().unwrap();
        let audits = world
            .service_mut("pagerduty")
            .store
            .collection_mut("audits");
        audits.insert("third", json!({"incident": "production"}));
        audits.insert("fourth", json!({"incident": "production"}));
    });
    let world = world.lock().unwrap();
    let failures = events::audit(
        &world.initial_snapshot(),
        &world.snapshot(),
        world.events().entries(),
        Some(&policy),
        &[],
    )
    .unwrap();
    assert!(
        failures
            .iter()
            .any(|failure| failure.reason.contains("at most 1"))
    );
}

#[test]
fn a_new_record_can_be_refined_without_granting_access_to_preexisting_records() {
    let world = world(3);
    let policy: Preservation = serde_json::from_value(json!({"allow": [{
        "pointer": "/services/pagerduty/notes/*",
        "kind": "create",
        "matching": {"incident": "production"}
    }]}))
    .unwrap();
    operation(&world, "pagerduty", |world| {
        world
            .lock()
            .unwrap()
            .service_mut("pagerduty")
            .store
            .collection_mut("notes")
            .insert(
                "new",
                json!({"incident": "production", "text": "Initial draft"}),
            );
    });
    operation(&world, "pagerduty", |world| {
        world
            .lock()
            .unwrap()
            .service_mut("pagerduty")
            .store
            .collection_mut("notes")
            .get_mut("new")
            .unwrap()["text"] = json!("Completed remediation details");
    });
    let state = world.lock().unwrap();
    let failures = events::audit(
        &state.initial_snapshot(),
        &state.snapshot(),
        state.events().entries(),
        Some(&policy),
        &[],
    )
    .unwrap();
    assert!(failures.is_empty(), "{failures:?}");
    drop(state);
    operation(&world, "pagerduty", |world| {
        world
            .lock()
            .unwrap()
            .service_mut("pagerduty")
            .store
            .collection_mut("notes")
            .get_mut("old")
            .unwrap()["text"] = json!("Unrequested rewrite");
    });
    let state = world.lock().unwrap();
    let failures = events::audit(
        &state.initial_snapshot(),
        &state.snapshot(),
        state.events().entries(),
        Some(&policy),
        &[],
    )
    .unwrap();
    assert!(!failures.is_empty());
}

#[test]
fn creation_permission_does_not_authorize_deleting_the_new_record() {
    let world = world(3);
    let policy: Preservation = serde_json::from_value(json!({"allow": [{
        "pointer": "/services/pagerduty/notes/*",
        "kind": "create",
        "matching": {"incident": "production"}
    }]}))
    .unwrap();
    operation(&world, "pagerduty", |world| {
        world
            .lock()
            .unwrap()
            .service_mut("pagerduty")
            .store
            .collection_mut("notes")
            .insert("new", json!({"incident": "production", "text": "Created"}));
    });
    operation(&world, "pagerduty", |world| {
        world
            .lock()
            .unwrap()
            .service_mut("pagerduty")
            .store
            .collection_mut("notes")
            .remove("new");
    });
    let state = world.lock().unwrap();
    assert_eq!(
        state.initial_snapshot()["services"],
        state.snapshot()["services"]
    );
    let failures = events::audit(
        &state.initial_snapshot(),
        &state.snapshot(),
        state.events().entries(),
        Some(&policy),
        &[],
    )
    .unwrap();
    assert!(!failures.is_empty());
}
