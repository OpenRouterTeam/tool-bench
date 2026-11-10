use mock_core::preservation::Preservation;
use serde_json::{Value, json};

#[test]
fn append_contracts_require_authoritative_mutation_history() {
    let definition = json!({
        "version":2,"state":[{"pointer":"/services/slack/lists/target","exists":true}],
        "preservation":serde_json::to_value(policy()).unwrap()
    });
    let mut expect: mock_core::grading::Expect = serde_json::from_value(definition).unwrap();
    assert!(expect.validate().is_err());
    expect.audit_history = true;
    expect.validate().unwrap();
}

fn state(rows: Value) -> Value {
    json!({"services":{"slack":{"lists":{"target":{"id":"target","records":rows,"columns":["Task"]}}}}})
}

fn policy() -> Preservation {
    serde_json::from_value(json!({"allow":[{
        "pointer":"/services/slack/lists/target/records",
        "kind":"append","identity_field":"/id","matching":{"created_by":"caller"},"max_count":1
    }]}))
    .unwrap()
}

#[test]
fn appending_one_actual_row_does_not_authorize_existing_rows_or_columns() {
    let initial = state(json!([{"id":"old","text":"preserve"}]));
    let after = state(json!([
        {"id":"old","text":"preserve"},
        {"id":"actual","created_by":"caller","text":"new"}
    ]));
    assert!(policy().check(&initial, &after).unwrap().is_empty());
    for rows in [
        json!([{"id":"old","text":"changed"},{"id":"actual","created_by":"caller"}]),
        json!([{"id":"actual","created_by":"caller"},{"id":"old","text":"preserve"}]),
        json!([{"id":"old","text":"preserve"},{"id":"first","created_by":"caller"},{"id":"second","created_by":"caller"}]),
        json!([{"id":"old","text":"preserve"},{"id":"actual","created_by":"other"}]),
        json!([{"id":"old","text":"preserve"},{"id":"old","created_by":"caller"}]),
    ] {
        assert!(!policy().check(&initial, &state(rows)).unwrap().is_empty());
    }
    let mut columns = after;
    columns["services"]["slack"]["lists"]["target"]["columns"] = json!(["Other"]);
    assert!(!policy().check(&initial, &columns).unwrap().is_empty());
}

#[test]
fn created_row_fields_may_be_refined_without_predicting_an_id_or_offset() {
    let initial = state(json!([{"id":42,"text":"preserve"}]));
    let after = state(
        json!([{"id":42,"text":"preserve"},{"id":"42","created_by":"caller","text":"completed"}]),
    );
    assert!(policy().check(&initial, &after).unwrap().is_empty());
}

#[test]
fn append_rules_require_literal_arrays_explicit_identity_and_bounded_row_matchers() {
    for patch in [
        json!({"pointer":"/services/slack/lists/*/records"}),
        json!({"identity_field":""}),
        json!({"matching":{}}),
        json!({"count_scope":"per_commit"}),
        json!({"max_count":0}),
        json!({"kind":"update"}),
    ] {
        let mut definition = serde_json::to_value(policy()).unwrap();
        definition["allow"][0]
            .as_object_mut()
            .unwrap()
            .extend(patch.as_object().unwrap().clone());
        let policy: Preservation = serde_json::from_value(definition).unwrap();
        assert!(policy.validate().is_err(), "{patch}");
    }
}
