use mock_core::relations::{RecordCheckError, RelationCheck};
use serde_json::{Value, json};

fn relation() -> RelationCheck {
    serde_json::from_value(json!({
        "source": {"collection": "/services/crm/links", "created": true},
        "source_field": "/note",
        "target": {"collection": "/services/crm/notes", "created": true},
        "target_field": "/id",
        "kind": "foreign_key"
    }))
    .unwrap()
}

fn state() -> Value {
    json!({"services": {"crm": {
        "notes": {"arbitrary_key": {"id": "actual_note"}},
        "links": {
            "contact_link": {"note": "actual_note"},
            "deal_link": {"note": "actual_note"}
        }
    }}})
}

fn initial() -> Value {
    json!({"services": {}})
}

#[test]
fn multiple_records_can_reference_one_actual_identity_without_a_bijection() {
    relation().check(Some(&initial()), &state()).unwrap();
    let mut changed = state();
    changed["services"]["crm"]["notes"]["another_key"] = json!({"id": "other_note"});
    changed["services"]["crm"]["links"]["contact_link"]["note"] = json!("other_note");
    relation().check(Some(&initial()), &changed).unwrap();
}

#[test]
fn dangling_missing_empty_and_differently_typed_references_fail() {
    for identity in [
        json!("old_note"),
        json!(""),
        json!(42),
        json!(42.0),
        Value::Null,
    ] {
        let mut changed = state();
        changed["services"]["crm"]["links"]["contact_link"]["note"] = identity;
        assert!(relation().check(Some(&initial()), &changed).is_err());
    }
    let mut changed = state();
    changed["services"]["crm"]["links"]["contact_link"] = json!({});
    assert!(relation().check(Some(&initial()), &changed).is_err());
    changed["services"]["crm"]["notes"]["arbitrary_key"]["id"] = json!(42);
    changed["services"]["crm"]["links"] = json!({"only": {"note": "42"}});
    assert!(relation().check(Some(&initial()), &changed).is_err());
}

#[test]
fn optional_sources_do_not_force_a_write_but_targets_must_be_known_and_unambiguous() {
    let mut changed = state();
    changed["services"]["crm"]["links"] = json!({});
    relation().check(Some(&initial()), &changed).unwrap();
    changed["services"]["crm"]["notes"] = json!({});
    assert!(relation().check(Some(&initial()), &changed).is_err());
    let mut changed = state();
    changed["services"]["crm"]["notes"]["duplicate_key"] = json!({"id": "actual_note"});
    assert!(relation().check(Some(&initial()), &changed).is_err());
    assert!(matches!(
        relation().check(None, &state()),
        Err(RecordCheckError::MissingEvidence(_))
    ));
}

#[test]
fn foreign_key_checks_cannot_silently_ignore_joined_field_predicates() {
    let mut definition = serde_json::to_value(relation()).unwrap();
    definition["fields"] = json!([{
        "source_field": "/currency", "target_field": "/currency", "kind": "equals"
    }]);
    let check: RelationCheck = serde_json::from_value(definition).unwrap();
    check.validate().unwrap();
    assert!(check.check(Some(&initial()), &state()).is_err());
    let mut complete = state();
    complete["services"]["crm"]["notes"]["arbitrary_key"]["currency"] = json!("USD");
    for link in complete["services"]["crm"]["links"]
        .as_object_mut()
        .unwrap()
        .values_mut()
    {
        link["currency"] = json!("USD");
    }
    check.check(Some(&initial()), &complete).unwrap();
    complete["services"]["crm"]["links"]["deal_link"]["currency"] = json!("EUR");
    assert!(check.check(Some(&initial()), &complete).is_err());
}
