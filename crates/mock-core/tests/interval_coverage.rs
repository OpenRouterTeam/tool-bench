use mock_core::relations::{RecordCheck, RecordCheckError};
use serde_json::{Value, json};

fn definition() -> Value {
    json!({
        "selector":{"collection":"/services/duty/overrides","created":true},
        "min_count":1,
        "interval_coverage":{
            "start_field":"/start","end_field":"/end",
            "start":"2026-10-31T09:00:00Z","end":"2026-10-31T17:00:00Z"
        }
    })
}

fn state(intervals: &[(&str, &str)]) -> Value {
    let mut state = json!({"services":{"duty":{"overrides":{}}}});
    for (index, (start, end)) in intervals.iter().enumerate() {
        state["services"]["duty"]["overrides"][format!("actual-{index}")] =
            json!({"start":start,"end":end});
    }
    state
}

#[test]
fn exact_union_accepts_single_split_reordered_overlapping_and_duplicate_intervals() {
    let check: RecordCheck = serde_json::from_value(definition()).unwrap();
    let initial = state(&[]);
    for intervals in [
        vec![("2026-10-31T09:00:00Z", "2026-10-31T17:00:00Z")],
        vec![
            ("2026-10-31T13:00:00Z", "2026-10-31T17:00:00Z"),
            ("2026-10-31T09:00:00Z", "2026-10-31T13:00:00Z"),
        ],
        vec![
            ("2026-10-31T09:00:00Z", "2026-10-31T17:00:00Z"),
            ("2026-10-31T09:00:00Z", "2026-10-31T17:00:00Z"),
        ],
        vec![
            ("2026-10-31T09:00:00Z", "2026-10-31T14:00:00Z"),
            ("2026-10-31T13:00:00Z", "2026-10-31T17:00:00Z"),
        ],
        vec![
            (
                "2026-10-31T04:00:00-05:00",
                "2026-10-31T08:00:00.000000001-05:00",
            ),
            (
                "2026-10-31T13:00:00.000000001Z",
                "2026-10-31T18:00:00+01:00",
            ),
        ],
    ] {
        check.check(Some(&initial), &state(&intervals)).unwrap();
    }
}

#[test]
fn exact_union_rejects_gaps_extensions_empty_reversed_or_malformed_intervals() {
    let check: RecordCheck = serde_json::from_value(definition()).unwrap();
    let initial = state(&[]);
    for intervals in [
        vec![],
        vec![("2026-10-31T09:00:00.000000001Z", "2026-10-31T17:00:00Z")],
        vec![("2026-10-31T09:00:00Z", "2026-10-31T17:00:00.000000001Z")],
        vec![
            ("2026-10-31T09:00:00Z", "2026-10-31T13:00:00Z"),
            ("2026-10-31T13:00:00.000000001Z", "2026-10-31T17:00:00Z"),
        ],
        vec![("2026-10-31T09:00:00Z", "2026-10-31T16:59:59.999999999Z")],
        vec![
            ("2026-10-31T09:00:00Z", "2026-10-31T17:00:00Z"),
            ("2026-10-31T13:00:00Z", "2026-10-31T13:00:00Z"),
        ],
        vec![
            ("2026-10-31T09:00:00Z", "2026-10-31T17:00:00Z"),
            ("2026-10-31T14:00:00Z", "2026-10-31T13:00:00Z"),
        ],
        vec![("not an instant", "2026-10-31T17:00:00Z")],
    ] {
        assert!(
            matches!(
                check.check(Some(&initial), &state(&intervals)),
                Err(RecordCheckError::Failed(_))
            ),
            "{intervals:?}"
        );
    }
}

#[test]
fn invalid_or_competing_interval_declarations_are_rejected() {
    for (field, value) in [
        ("start", json!("invalid")),
        ("end", json!("2026-10-31T09:00:00Z")),
        ("end", json!("2026-10-31T08:00:00Z")),
        ("start_field", json!("")),
        ("start_field", json!("/nested/*")),
        ("end_field", json!("/start")),
        ("end_field", json!("/start/nested")),
    ] {
        let mut value_definition = definition();
        value_definition["interval_coverage"][field] = value;
        let check: RecordCheck = serde_json::from_value(value_definition).unwrap();
        assert!(check.validate().is_err(), "{field}");
    }
    for attributes in [
        json!({"min_count":0,"max_count":0}),
        json!({"fields":{"/start":"2026-10-31T09:00:00Z"}}),
        json!({"timestamp_fields":{"/start":"2026-10-31T09:00:00Z"}}),
        json!({"numeric_fields":{"/end":1}}),
        json!({"totals":{"/start":1}}),
    ] {
        let mut value_definition = definition();
        for (field, value) in attributes.as_object().unwrap() {
            value_definition[field] = value.clone();
        }
        let check: RecordCheck = serde_json::from_value(value_definition).unwrap();
        assert!(check.validate().is_err(), "{attributes}");
    }
}

#[test]
fn restoring_an_out_of_scope_interval_cannot_erase_its_committed_violation() {
    use mock_core::{
        dataset::Dataset,
        events::{self, OperationKind},
        grading::{Expect, GradeInput, Outcome, grade},
        store::Store,
        world::{WorldState, shared},
    };
    let directory = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../scenarios/example/github/basic");
    let mut dataset = Dataset::load(&directory).unwrap();
    let mut store = Store::default();
    store.collection_mut("overrides");
    dataset.services.insert("duty".into(), store);
    let world = shared(WorldState::from_dataset(dataset).unwrap());
    for start in ["2026-10-31T08:00:00Z", "2026-10-31T09:00:00Z"] {
        let operation = world
            .begin_operation(OperationKind::Http, "duty", "tools", "POST", "/overrides")
            .unwrap();
        events::sync_scope(Some(operation.context().clone()), || {
            world
                .lock()
                .unwrap()
                .service_mut("duty")
                .store
                .collection_mut("overrides")
                .insert(
                    "actual",
                    json!({"start":start,"end":"2026-10-31T17:00:00Z"}),
                );
        });
        operation.finish(200, true);
    }
    let expect: Expect = serde_json::from_value(json!({
        "version":2,"audit_history":true,"records":[definition()],
        "preservation":{"allow":[
            {"pointer":"/services/duty/overrides/actual","kind":"create"},
            {"pointer":"/services/duty/overrides/actual/start"}
        ]}
    }))
    .unwrap();
    let world = world.lock().unwrap();
    let initial = world.initial_snapshot();
    let state = world.snapshot();
    expect.records[0].check(Some(&initial), &state).unwrap();
    let result = grade(&GradeInput {
        expect: &expect,
        state,
        journal: world.journal.entries(),
        answer: None,
        tools_ips: vec!["tools".into()],
        context_overflow: false,
        startup: &[],
        initial_state: Some(&initial),
        events: Some(world.events().entries()),
    });
    assert!(result.preservation_ok, "{result:?}");
    assert_eq!(result.outcome, Outcome::Fail, "{result:?}");
    assert!(
        result
            .failures
            .iter()
            .any(|failure| failure.location.starts_with("records/0/history/")
                && !failure.event_evidence.is_empty())
    );
}
