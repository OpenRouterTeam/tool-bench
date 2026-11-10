use mock_core::preservation::Preservation;
use serde_json::{Value, json};

fn initial() -> Value {
    json!({"services": {"github": {"issues": {
        "payments/api#1": {"title": "Duplicate charge", "state": "open", "labels": ["bug"]},
        "payments/api#2": {"title": "Unrelated", "state": "open", "labels": []}
    }}}})
}

fn policy(value: Value) -> Preservation {
    serde_json::from_value(value).unwrap()
}

#[test]
fn default_preserves_all_business_records() {
    let initial = initial();
    let mut changed = initial.clone();
    changed["services"]["github"]["issues"]["payments/api#1"]["state"] = json!("closed");
    let failures = Preservation::default().check(&initial, &changed).unwrap();
    assert_eq!(failures.len(), 1);
    assert_eq!(
        failures[0].pointer,
        "/services/github/issues/payments~1api#1/state"
    );
}

#[test]
fn an_allowed_field_change_does_not_authorize_other_fields_or_records() {
    let policy = policy(json!({"allow": [{
        "pointer": "/services/github/issues/payments~1api#1/state"
    }]}));
    let initial = initial();
    let mut changed = initial.clone();
    changed["services"]["github"]["issues"]["payments/api#1"]["state"] = json!("closed");
    assert!(policy.check(&initial, &changed).unwrap().is_empty());
    changed["services"]["github"]["issues"]["payments/api#1"]["title"] = json!("Changed");
    changed["services"]["github"]["issues"]["payments/api#2"]["state"] = json!("closed");
    assert_eq!(policy.check(&initial, &changed).unwrap().len(), 2);
}

#[test]
fn empty_lazy_collections_do_not_count_as_business_changes() {
    let initial = initial();
    let mut after = initial.clone();
    after["services"]["github"]["comments"] = json!({});
    after["services"]["stripe"] = json!({"refunds": {}});
    after["now"] = json!("2026-11-03T15:00:07Z");
    assert!(
        Preservation::default()
            .check(&initial, &after)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn a_new_matching_record_has_scoped_cardinality() {
    let policy = policy(json!({"allow": [{
        "pointer": "/services/github/comments/*",
        "kind": "create",
        "matching": {"issue": 1},
        "max_count": 1
    }]}));
    let initial = initial();
    let mut after = initial.clone();
    after["services"]["github"]["comments"] = json!({
        "generated-a": {"issue": 1, "body": "Refunded"}
    });
    assert!(policy.check(&initial, &after).unwrap().is_empty());
    after["services"]["github"]["comments"]["generated-b"] =
        json!({"issue": 1, "body": "Duplicated"});
    let failures = policy.check(&initial, &after).unwrap();
    assert_eq!(failures.len(), 1);
    assert!(failures[0].reason.contains("2 records changed"));
}

#[test]
fn generated_record_ids_are_not_part_of_the_contract() {
    let policy = policy(json!({"allow": [{
        "pointer": "/services/github/comments/*",
        "kind": "create",
        "matching": {"issue": 1}
    }]}));
    for record_id in ["generated-a", "unrelated-generator-position"] {
        let initial = initial();
        let mut after = initial.clone();
        after["services"]["github"]["comments"] = json!({
            record_id: {"issue": 1, "body": "Refunded"}
        });
        assert!(policy.check(&initial, &after).unwrap().is_empty());
    }
}

#[test]
fn a_matching_create_rule_cannot_modify_an_existing_record() {
    let policy = policy(json!({"allow": [{
        "pointer": "/services/github/issues/*",
        "kind": "create",
        "matching": {"title": "Duplicate charge"}
    }]}));
    let initial = initial();
    let mut after = initial.clone();
    after["services"]["github"]["issues"]["payments/api#1"]["state"] = json!("closed");
    assert!(!policy.check(&initial, &after).unwrap().is_empty());
}

#[test]
fn deletion_requires_an_explicit_record_rule() {
    let policy = policy(json!({"allow": [{
        "pointer": "/services/github/issues/payments~1api#1",
        "kind": "delete"
    }]}));
    let initial = initial();
    let mut after = initial.clone();
    after["services"]["github"]["issues"]
        .as_object_mut()
        .unwrap()
        .remove("payments/api#1");
    assert!(policy.check(&initial, &after).unwrap().is_empty());
    assert!(
        !Preservation::default()
            .check(&initial, &after)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn arrays_allow_specific_nested_fields_without_authorizing_siblings() {
    let policy = policy(json!({"allow": [{
        "pointer": "/services/kubernetes/deployments/checkout/spec/containers/0/image"
    }]}));
    let initial = json!({"services": {"kubernetes": {"deployments": {
        "checkout": {"spec": {"containers": [{"image": "v1", "name": "web"}]}}
    }}}});
    let mut after = initial.clone();
    after["services"]["kubernetes"]["deployments"]["checkout"]["spec"]["containers"][0]["image"] =
        json!("v2");
    assert!(policy.check(&initial, &after).unwrap().is_empty());
    after["services"]["kubernetes"]["deployments"]["checkout"]["spec"]["containers"][0]["name"] =
        json!("other");
    assert_eq!(policy.check(&initial, &after).unwrap().len(), 1);
}

#[test]
fn broad_or_inert_rules_are_rejected() {
    for value in [
        json!({"allow": [{"pointer": "/services/github"}]}),
        json!({"allow": [{"pointer": "/services/github/issues/payments~1api#1"}]}),
        json!({"allow": [{"pointer": "/services/github/issues/*/state"}]}),
        json!({"allow": [{"pointer": "/services/github/issues/*/state", "matching": {}}]}),
        json!({"allow": [{"pointer": "/services/github/issues/x/state", "max_count": 0}]}),
        json!({"allow": [{"pointer": "/services/github/issues/x~2/state"}]}),
        json!({"allow": [{"pointer": "/services/*/issues/x/state"}]}),
    ] {
        assert!(policy(value.clone()).validate().is_err(), "{value}");
    }
}

#[test]
fn missing_or_malformed_state_is_not_evidence_of_preservation() {
    for snapshot in [json!({}), json!({"services": {"github": []}})] {
        assert!(
            Preservation::default()
                .check(&snapshot, &initial())
                .is_err()
        );
        assert!(
            Preservation::default()
                .check(&initial(), &snapshot)
                .is_err()
        );
    }
}

#[test]
fn creating_an_empty_business_record_is_still_a_change() {
    let initial = initial();
    let mut after = initial.clone();
    after["services"]["github"]["issues"]["empty"] = json!({});
    assert!(
        !Preservation::default()
            .check(&initial, &after)
            .unwrap()
            .is_empty()
    );
}
