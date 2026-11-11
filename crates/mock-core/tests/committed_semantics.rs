use std::path::Path;

use mock_core::{
    dataset::Dataset,
    events::{self, OperationKind},
    grading::{CheckKind, Expect, Grade, GradeInput, Outcome, grade},
    relations::{RecordCheck, RecordCheckError},
    store::Store,
    world::{WorldState, shared},
};
use serde_json::{Value, json};

#[test]
fn exact_timestamp_fields_accept_offsets_without_rounding_near_misses() {
    let check: RecordCheck = serde_json::from_value(json!({
        "selector":{"collection":"/services/ops/silences","created":true},
        "equals":1,"timestamp_fields":{"/startsAt":"2026-11-13T02:00:00Z"}
    }))
    .unwrap();
    let initial = json!({"services":{"ops":{"silences":{}}}});
    check
        .check(
            Some(&initial),
            &json!({"services":{"ops":{"silences":{"actual":{
                "startsAt":"2026-11-13T03:00:00+01:00"
            }}}}}),
        )
        .unwrap();
    assert!(matches!(
        check.check(
            Some(&initial),
            &json!({"services":{"ops":{"silences":{"actual":{
                "startsAt":"2026-11-13T02:00:00.000000001Z"
            }}}}})
        ),
        Err(RecordCheckError::Failed(_))
    ));
}

#[test]
fn timestamp_and_label_fields_reject_zero_counts_and_competing_spellings() {
    for value in [
        json!({"equals":0,"timestamp_fields":{"/start":"2026-11-13T02:00:00Z"}}),
        json!({"equals":0,"label_matchers":{"field":"/matchers","equals":{"instance":"pg-replica-2"}}}),
        json!({"equals":1,"timestamp_fields":{"/start":"invalid"}}),
        json!({"equals":1,"timestamp_fields":{"/start":"2026-11-13T02:00:00Z"},"fields":{"/start":"2026-11-13T02:00:00Z"}}),
        json!({"equals":1,"label_matchers":{"field":"/matchers","equals":{"instance":"pg-replica-2"}},"array_lengths":{"/matchers":1}}),
    ] {
        let mut value = value;
        value["selector"] = json!({"collection":"/services/ops/silences","created":true});
        let check: RecordCheck = serde_json::from_value(value).unwrap();
        assert!(check.validate().is_err());
    }
}

fn committed_silence_versions(versions: &[Value]) -> Grade {
    let directory =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/example/github/basic");
    let mut dataset = Dataset::load(&directory).unwrap();
    let mut service = Store::default();
    service.collection_mut("silences");
    dataset.services.insert("ops".into(), service);
    let world = shared(WorldState::from_dataset(dataset).unwrap());
    for version in versions {
        let operation = world
            .begin_operation(OperationKind::Http, "ops", "tools", "POST", "/silences")
            .unwrap();
        events::sync_scope(Some(operation.context().clone()), || {
            world
                .lock()
                .unwrap()
                .service_mut("ops")
                .store
                .collection_mut("silences")
                .insert("actual", version.clone());
        });
        operation.finish(200, true);
    }
    let expect: Expect = serde_json::from_value(json!({
        "version":2,"audit_history":true,
        "records":[{
            "selector":{"collection":"/services/ops/silences","created":true},
            "equals":1,
            "timestamp_fields":{"/startsAt":"2026-11-13T02:00:00Z","/endsAt":"2026-11-13T04:00:00Z"},
            "label_matchers":{"field":"/matchers","equals":{"instance":"pg-replica-2"}}
        }],
        "preservation":{"allow":[
            {"pointer":"/services/ops/silences/actual","kind":"create"},
            {"pointer":"/services/ops/silences/actual/startsAt","kind":"update"},
            {"pointer":"/services/ops/silences/actual/endsAt","kind":"update"},
            {"pointer":"/services/ops/silences/actual/matchers","kind":"update"}
        ]}
    }))
    .unwrap();
    expect.validate().unwrap();
    let world = world.lock().unwrap();
    expect.records[0]
        .check(Some(&world.initial_snapshot()), &world.snapshot())
        .unwrap();
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
}

fn correct_silence() -> Value {
    json!({
        "startsAt":"2026-11-13T02:00:00Z","endsAt":"2026-11-13T04:00:00Z",
        "matchers":[{"name":"instance","value":"pg-replica-2","isRegex":false,"isEqual":true}]
    })
}

#[test]
fn restoring_scope_or_window_does_not_erase_a_committed_semantic_violation() {
    let mut broad = correct_silence();
    broad["matchers"][0]["value"] = json!("pg-replica-.*");
    broad["matchers"][0]["isRegex"] = json!(true);
    let mut wrong_window = correct_silence();
    wrong_window["startsAt"] = json!("2026-11-13T01:00:00Z");
    for incorrect in [broad, wrong_window] {
        let result = committed_silence_versions(&[incorrect, correct_silence()]);
        assert_eq!(result.outcome, Outcome::Fail, "{result:?}");
        assert!(result.failures.iter().any(|failure| {
            failure.check == CheckKind::Records
                && failure.location == "records/0/history/actual"
                && !failure.event_evidence.is_empty()
        }));
        assert!(
            result
                .failures
                .iter()
                .all(|failure| failure.check != CheckKind::Preservation),
            "{result:?}"
        );
    }
}

#[test]
fn equivalent_committed_predicate_and_time_spellings_are_not_collateral_violations() {
    let mut equivalent = correct_silence();
    equivalent["startsAt"] = json!("2026-11-13T03:00:00+01:00");
    equivalent["endsAt"] = json!("2026-11-13T05:00:00+01:00");
    equivalent["matchers"][0]["value"] = json!("\\Qpg-replica-2\\E");
    equivalent["matchers"][0]["isRegex"] = json!(true);
    let result = committed_silence_versions(&[equivalent, correct_silence()]);
    assert_eq!(result.outcome, Outcome::Pass, "{result:?}");
}
