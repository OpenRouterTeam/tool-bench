use mock_core::relations::{RecordCheck, RecordCheckError, RelationCheck};
use serde_json::{Value, json};

fn record(expected: Value) -> RecordCheck {
    serde_json::from_value(json!({
        "selector":{"collection":"/services/crm/records","matching":{"id":"target"}},
        "equals":1,"numeric_fields":{"/amount":expected}
    }))
    .unwrap()
}

fn state(value: Value) -> Value {
    json!({"services":{"crm":{"records":{"target":{"id":"target","amount":value}}}}})
}

#[test]
fn numeric_record_fields_compare_exact_decimal_values_not_serialization_or_substrings() {
    let check = record(json!("176500"));
    for value in [
        json!(176500),
        json!(176500.0),
        json!("176500.00"),
        json!("1.765e5"),
    ] {
        check.check(Some(&state(json!(0))), &state(value)).unwrap();
    }
    for value in [
        json!("176500.01"),
        json!("176499.99"),
        json!("176500 USD"),
        json!("1176500"),
        Value::Null,
        json!(false),
        json!([]),
    ] {
        assert!(matches!(
            check.check(Some(&state(json!(0))), &state(value)),
            Err(RecordCheckError::Failed(_))
        ));
    }
}

#[test]
fn malformed_or_competing_numeric_predicates_are_invalid_contracts() {
    for expected in [
        Value::Null,
        json!(""),
        json!("NaN"),
        json!("infinity"),
        json!({}),
    ] {
        assert!(record(expected).validate().is_err());
    }
    let mut definition = serde_json::to_value(record(json!(12))).unwrap();
    definition["fields"] = json!({"/amount":12});
    assert!(
        serde_json::from_value::<RecordCheck>(definition)
            .unwrap()
            .validate()
            .is_err()
    );
}

#[test]
fn numeric_relationships_bind_actual_source_and_target_values() {
    let relation:RelationCheck=serde_json::from_value(json!({
        "source":{"collection":"/services/hubspot/deals","matching":{"id":"deal"}},
        "source_field":"/amount",
        "target":{"collection":"/services/salesforce/opportunities","matching":{"id":"opportunity"}},
        "target_field":"/amount","kind":"numeric_equals"
    })).unwrap();
    for value in [json!("176500"), json!("176500.00"), json!(176500.0)] {
        let state = json!({"services":{
            "hubspot":{"deals":{"deal":{"id":"deal","amount":value}}},
            "salesforce":{"opportunities":{"opportunity":{"id":"opportunity","amount":176500}}}
        }});
        relation
            .check(Some(&json!({"services":{}})), &state)
            .unwrap();
        let mut wrong = state.clone();
        wrong["services"]["salesforce"]["opportunities"]["opportunity"]["amount"] =
            json!(176500.01);
        assert!(
            relation
                .check(Some(&json!({"services":{}})), &wrong)
                .is_err()
        );
    }
}
