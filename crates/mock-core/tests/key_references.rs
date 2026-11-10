use mock_core::relations::{RecordCheckError, RelationCheck};
use serde_json::{Value, json};

fn definition() -> Value {
    json!({
        "source":{"collection":"/services/gitlab/notes","created":true},
        "source_field":"/noteable",
        "target":{"collection":"/services/gitlab/issues","created":true},
        "kind":"key_reference"
    })
}

fn relation() -> RelationCheck {
    serde_json::from_value(definition()).unwrap()
}

fn initial() -> Value {
    json!({"services":{"gitlab":{"issues":{
        "northwind/payments/api#58":{"id":30058,"iid":58}
    },"notes":{}}}})
}

fn completed(target_key: &str) -> Value {
    let mut state = initial();
    state["services"]["gitlab"]["issues"][target_key] = json!({"id":30991,"iid":991});
    state["services"]["gitlab"]["notes"] = json!({
        "arbitrary_note_a":{"noteable":target_key},
        "arbitrary_note_b":{"noteable":target_key}
    });
    state
}

#[test]
fn references_follow_actual_new_record_keys_not_database_ids_or_predicted_numbers() {
    for target in ["northwind/payments/api#59", "northwind/payments/api#991"] {
        relation()
            .check(Some(&initial()), &completed(target))
            .unwrap();
    }
}

#[test]
fn numeric_ids_prefixes_other_projects_and_preexisting_keys_are_not_key_references() {
    for reference in [
        json!("northwind/payments/api#58"),
        json!("northwind/payments/api#9910"),
        json!("northwind/mobile/api#991"),
        json!("30991"),
        json!(30991),
        json!("northwind~1payments~1api#991"),
        json!(""),
        Value::Null,
    ] {
        let mut state = completed("northwind/payments/api#991");
        state["services"]["gitlab"]["notes"]["arbitrary_note_a"]["noteable"] = reference;
        assert!(matches!(
            relation().check(Some(&initial()), &state),
            Err(RecordCheckError::Failed(_))
        ));
    }
    let mut state = completed("northwind/payments/api#991");
    state["services"]["gitlab"]["notes"]["arbitrary_note_a"] = json!({});
    assert!(relation().check(Some(&initial()), &state).is_err());
}

#[test]
fn repeated_database_ids_do_not_make_distinct_storage_keys_ambiguous() {
    let mut state = completed("northwind/payments/api#991");
    state["services"]["gitlab"]["issues"]["northwind/payments/api#992"] =
        state["services"]["gitlab"]["issues"]["northwind/payments/api#991"].clone();
    state["services"]["gitlab"]["notes"]["arbitrary_note_b"]["noteable"] =
        json!("northwind/payments/api#992");
    relation().check(Some(&initial()), &state).unwrap();
}

#[test]
fn optional_sources_do_not_force_a_note_but_known_targets_and_initial_evidence_are_required() {
    let mut state = completed("northwind/payments/api#991");
    state["services"]["gitlab"]["notes"] = json!({});
    relation().check(Some(&initial()), &state).unwrap();
    assert!(matches!(
        relation().check(None, &state),
        Err(RecordCheckError::MissingEvidence(_))
    ));
    assert!(relation().check(Some(&initial()), &initial()).is_err());
}

#[test]
fn key_references_cannot_silently_ignore_target_fields_array_identity_or_join_predicates() {
    for (field, value) in [
        ("target_field", json!("/id")),
        (
            "fields",
            json!([{"source_field":"/amount","target_field":"/amount","kind":"equals"}]),
        ),
    ] {
        let mut invalid = definition();
        invalid[field] = value;
        let check: RelationCheck = serde_json::from_value(invalid).unwrap();
        assert!(check.validate().is_err(), "{field}");
    }
    let mut invalid = definition();
    invalid["target"]["identity_field"] = json!("/id");
    let check: RelationCheck = serde_json::from_value(invalid).unwrap();
    assert!(check.validate().is_err());
    let mut invalid = definition();
    invalid["target"]["collection"] = json!("/services/gitlab/issues/a/items");
    invalid["target"]["identity_field"] = json!("/id");
    let check: RelationCheck = serde_json::from_value(invalid).unwrap();
    assert!(check.validate().is_err());
}

#[test]
fn old_field_relations_still_require_an_explicit_nonempty_target_field() {
    for kind in ["equals", "foreign_key", "bijection", "contains_reference"] {
        let mut invalid = definition();
        invalid["kind"] = json!(kind);
        if let Ok(check) = serde_json::from_value::<RelationCheck>(invalid) {
            assert!(check.validate().is_err());
        }
    }
}
