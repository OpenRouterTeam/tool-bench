use mock_core::relations::{RecordCheck, RecordCheckError};
use serde_json::{Value, json};

fn definition() -> Value {
    json!({
        "selector": {"collection": "/services/jira/issues", "created": true},
        "equals": 1,
        "reviewed_fields": {
            "requirement": "Report that the Apple Pay button is missing at checkout on iOS 18 Safari, without contradictory or invented claims.",
            "accepted": [
                {"/summary": "Apple Pay missing on iOS 18 Safari checkout", "/description": ""},
                {"/summary": "Apple Pay missing", "/description": "The button is absent at checkout on iOS 18 Safari."}
            ]
        }
    })
}

fn check() -> RecordCheck {
    serde_json::from_value(definition()).unwrap()
}

fn initial() -> Value {
    json!({"services": {}})
}

fn state(record: Value) -> Value {
    json!({"services": {"jira": {"issues": {"actual": record}}}})
}

#[test]
fn complete_reviewed_content_can_be_distributed_across_fields() {
    for record in [
        json!({"summary": "Apple Pay missing on iOS 18 Safari checkout"}),
        json!({"summary": "Apple Pay missing on iOS 18 Safari checkout", "description": null}),
        json!({"summary": "Apple Pay missing on iOS 18 Safari checkout", "description": " \n"}),
        json!({"summary": " Apple Pay missing ", "description": "The button is absent\nat checkout on iOS 18 Safari."}),
    ] {
        check().check(Some(&initial()), &state(record)).unwrap();
    }
}

#[test]
fn a_reviewed_summary_does_not_hide_contradictory_or_unreviewed_description() {
    for record in [
        json!({"summary": "Apple Pay missing on iOS 18 Safari checkout", "description": "Actually, Apple Pay works correctly."}),
        json!({"summary": "Apple Pay missing", "description": "Checkout payment options do not include Apple Pay on Safari for iOS 18."}),
        json!({"summary": "Apple Pay", "description": ""}),
        json!({"summary": "Apple Pay missing", "description": "The button is absent at checkout on iOS 18 Safari. This is false."}),
        json!({"summary": "Apple Pay works correctly", "description": "The button is absent at checkout on iOS 18 Safari."}),
    ] {
        assert!(matches!(
            check().check(Some(&initial()), &state(record)),
            Err(RecordCheckError::NeedsAdjudication(_))
        ));
    }
}

#[test]
fn all_empty_or_nontext_content_is_a_known_failure_not_a_positive_example() {
    for record in [
        json!({}),
        json!({"summary": null, "description": ""}),
        json!({"summary": " \n", "description": null}),
        json!({"summary": "Apple Pay missing", "description": 42}),
    ] {
        assert!(matches!(
            check().check(Some(&initial()), &state(record)),
            Err(RecordCheckError::Failed(_))
        ));
    }
}

#[test]
fn reviewed_field_examples_require_a_complete_consistent_projection() {
    for patch in [
        json!({"reviewed_fields": {"requirement": "", "accepted": [{"/summary": "text"}]}}),
        json!({"reviewed_fields": {"requirement": "facts", "accepted": []}}),
        json!({"reviewed_fields": {"requirement": "facts", "accepted": [{}]}}),
        json!({"reviewed_fields": {"requirement": "facts", "accepted": [{"/summary": " "}]}}),
        json!({"reviewed_fields": {"requirement": "facts", "accepted": [{"/summary": "text"}, {"/description": "text"}]}}),
        json!({"reviewed_fields": {"requirement": "facts", "accepted": [{"/summary": "text"}, {"/summary": " text\n"}]}}),
        json!({"reviewed_fields": {"requirement": "facts", "accepted": [{"": "text"}]}}),
        json!({"fields": {"/summary": "literal"}}),
        json!({"equals": 0}),
    ] {
        let mut invalid = definition();
        invalid
            .as_object_mut()
            .unwrap()
            .extend(patch.as_object().unwrap().clone());
        let check: RecordCheck = serde_json::from_value(invalid).unwrap();
        assert!(check.validate().is_err(), "{patch}");
    }
}

#[test]
fn known_structural_failures_are_not_hidden_by_unreviewed_content() {
    let mut contract = definition();
    contract["fields"] = json!({"/owner": "Alex"});
    let check: RecordCheck = serde_json::from_value(contract).unwrap();
    assert!(matches!(
        check.check(Some(&initial()), &state(json!({"summary": "unreviewed"}))),
        Err(RecordCheckError::Failed(_))
    ));
}
