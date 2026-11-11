use std::path::Path;

use mock_core::{
    dataset::Dataset,
    events::{self, OperationKind},
    grading::{Expect, GradeInput, Outcome, grade},
    preservation::Preservation,
    store::Store,
    world::{WorldState, shared},
};
use serde_json::{Value, json};

fn policy() -> Value {
    json!({"allow":[{
        "pointer":"/services/duty/notes/*","kind":"create",
        "matching":{"parent":"target"},"max_count":"all_matching",
        "cause":{"service":"duty","methods":["POST"],"path_regex":"^/incidents/target/notes$"}
    }]})
}

#[test]
fn unbounded_creations_still_require_a_record_matcher_and_an_explicit_http_cause() {
    let accepted: Preservation = serde_json::from_value(policy()).unwrap();
    accepted.validate().unwrap();
    for removed in ["matching", "cause"] {
        let mut definition = policy();
        definition["allow"][0]
            .as_object_mut()
            .unwrap()
            .remove(removed);
        let invalid: Preservation = serde_json::from_value(definition).unwrap();
        assert!(invalid.validate().is_err(), "{removed}");
    }
}

fn creation_outcome(path: &str, restore_old: bool) -> Outcome {
    let directory =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/example/github/basic");
    let mut dataset = Dataset::load(&directory).unwrap();
    let mut service = Store::default();
    service
        .collection_mut("notes")
        .insert("old", json!({"parent":"target","content":"original"}));
    dataset.services.insert("duty".into(), service);
    let world = shared(WorldState::from_dataset(dataset).unwrap());
    for content in if restore_old {
        vec!["changed", "original"]
    } else {
        vec!["new"]
    } {
        let operation = world
            .begin_operation(OperationKind::Http, "duty", "tools", "POST", path)
            .unwrap();
        events::sync_scope(Some(operation.context().clone()), || {
            let mut guard = world.lock().unwrap();
            let notes = guard.service_mut("duty").store.collection_mut("notes");
            if restore_old {
                notes.insert("old", json!({"parent":"target","content":content}));
            }
            for identity in ["actual-one", "actual-two"] {
                notes.insert(
                    identity,
                    json!({"parent":"target","content":"requested message"}),
                );
            }
        });
        operation.finish(201, true);
    }
    let expect: Expect = serde_json::from_value(json!({
        "version":2,"audit_history":true,
        "records":[{"selector":{"collection":"/services/duty/notes","created":true},"min_count":1}],
        "preservation":policy()
    }))
    .unwrap();
    let world = world.lock().unwrap();
    grade(&GradeInput {
        expect: &expect,
        state: world.snapshot(),
        journal: world.journal.entries(),
        answer: None,
        tools_ips: vec!["tools".into()],
        context_overflow: false,
        startup: &[],
        initial_state: Some(&world.initial_snapshot()),
        events: Some(world.events().entries()),
    })
    .outcome
}

#[test]
fn repeated_created_records_are_admitted_only_under_their_declared_cause() {
    assert_eq!(
        creation_outcome("/incidents/target/notes", false),
        Outcome::Pass
    );
    assert_eq!(
        creation_outcome("/incidents/other/notes", false),
        Outcome::Fail
    );
}

#[test]
fn cause_scoped_creation_permission_never_authorizes_rewriting_and_restoring_old_records() {
    assert_eq!(
        creation_outcome("/incidents/target/notes", true),
        Outcome::Fail
    );
}

#[test]
fn preservation_array_lengths_apply_to_the_whole_selected_record() {
    let policy: Preservation = serde_json::from_value(json!({"allow":[{
        "pointer":"/services/duty/incidents/target/assignments",
        "matching":{"assignments":[{"assignee":{"id":"new-user"}}]},
        "array_lengths":{"/assignments":1}
    }]}))
    .unwrap();
    let initial = json!({"services":{"duty":{"incidents":{"target":{
        "assignments":[{"assignee":{"id":"old-user"}}]
    }}}}});
    for (assignments, allowed) in [
        (json!([{"assignee":{"id":"new-user"}}]), true),
        (
            json!([{"assignee":{"id":"new-user"}},{"assignee":{"id":"old-user"}}]),
            false,
        ),
        (json!({"0":{"assignee":{"id":"new-user"}}}), false),
    ] {
        let mut state = initial.clone();
        state["services"]["duty"]["incidents"]["target"]["assignments"] = assignments;
        assert_eq!(policy.check(&initial, &state).unwrap().is_empty(), allowed);
    }
    for attributes in [
        json!({"array_lengths":{"":1}}),
        json!({"array_lengths":{"/assignments/*":1}}),
        json!({"numeric_fields":{"/assignments":1}}),
        json!({"field_patterns":{"/assignments":".+"}}),
    ] {
        let mut value = serde_json::to_value(&policy).unwrap();
        for (field, contents) in attributes.as_object().unwrap() {
            value["allow"][0][field] = contents.clone();
        }
        let invalid: Preservation = serde_json::from_value(value).unwrap();
        assert!(invalid.validate().is_err(), "{attributes}");
    }
}
