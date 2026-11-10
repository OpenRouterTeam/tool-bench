use mock_core::preservation::Preservation;
use serde_json::{Value, json};

fn snapshot(tags: Value) -> Value {
    json!({"services": {"shop": {"customers": {"actual": {"id": 1, "tags": tags}}}}})
}

fn policy() -> Preservation {
    serde_json::from_value(json!({"allow": [{
        "pointer": "/services/shop/customers/actual/tags",
        "add_set_values": ["VIP"], "ascii_case_insensitive": true
    }]}))
    .unwrap()
}

#[test]
fn an_explicit_empty_addition_list_is_reorder_only_not_unrestricted_permission() {
    let policy: Preservation = serde_json::from_value(json!({"allow": [{
        "pointer": "/services/shop/customers/actual/tags",
        "add_set_values": [], "ascii_case_insensitive": true
    }]}))
    .unwrap();
    assert!(
        policy
            .check(&snapshot(json!(["a", "b"])), &snapshot(json!(["B", "A"])))
            .unwrap()
            .is_empty()
    );
    assert!(
        !policy
            .check(
                &snapshot(json!(["a", "b"])),
                &snapshot(json!(["a", "b", "VIP"]))
            )
            .unwrap()
            .is_empty()
    );
}

#[test]
fn additive_sets_preserve_arbitrary_original_members_without_imposing_order_or_case() {
    for (before, after) in [
        (
            json!(["newsletter", "partner"]),
            json!(["vip", "partner", "newsletter"]),
        ),
        (json!(["VIP", "partner"]), json!(["partner", "VIP"])),
        (json!(["partner", "PARTNER"]), json!(["partner", "VIP"])),
        (json!([]), json!(["VIP"])),
        (Value::Null, json!(["VIP"])),
    ] {
        assert!(
            policy()
                .check(&snapshot(before), &snapshot(after))
                .unwrap()
                .is_empty()
        );
    }
    let missing = json!({"services": {"shop": {"customers": {"actual": {"id": 1}}}}});
    assert!(
        policy()
            .check(&missing, &snapshot(json!(["VIP"])))
            .unwrap()
            .is_empty()
    );
}

#[test]
fn removal_unrequested_members_substrings_and_wrong_shapes_are_not_additions() {
    for after in [
        json!(["VIP"]),
        json!(["old", "VIP", "other"]),
        json!(["old", "VIP-candidate"]),
        json!(["old", 42]),
        json!("old,VIP"),
        Value::Null,
    ] {
        assert!(
            !policy()
                .check(&snapshot(json!(["old"])), &snapshot(after))
                .unwrap()
                .is_empty()
        );
    }
}

#[test]
fn a_string_set_rule_cannot_authorize_other_fields_or_new_records() {
    let before = snapshot(json!(["old"]));
    let mut after = snapshot(json!(["old", "VIP"]));
    after["services"]["shop"]["customers"]["actual"]["id"] = json!(2);
    assert!(!policy().check(&before, &after).unwrap().is_empty());
    let missing = json!({"services": {"shop": {"customers": {}}}});
    assert!(
        !policy()
            .check(&missing, &snapshot(json!(["VIP"])))
            .unwrap()
            .is_empty()
    );
}

#[test]
fn wildcard_string_set_rules_resolve_the_actual_record_key_and_keep_their_matcher() {
    let policy: Preservation = serde_json::from_value(json!({"allow": [{
        "pointer": "/services/shop/customers/*/tags", "matching": {"id": 1},
        "add_set_values": ["VIP"]
    }]}))
    .unwrap();
    assert!(
        policy
            .check(&snapshot(json!([])), &snapshot(json!(["VIP"])))
            .unwrap()
            .is_empty()
    );
    let mut after = snapshot(json!(["VIP"]));
    after["services"]["shop"]["customers"]["actual"]["id"] = json!(2);
    assert!(
        !policy
            .check(&snapshot(json!([])), &after)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn malformed_set_rules_fail_validation_instead_of_broadening_permissions() {
    for changed in [
        json!({"kind": "create"}),
        json!({"add_set_values": null}),
        json!({"add_set_values": ["VIP", "vip"]}),
        json!({"add_set_values": [" "]}),
    ] {
        let mut rule = json!({
            "pointer": "/services/shop/customers/actual/tags",
            "add_set_values": ["VIP"], "ascii_case_insensitive": true
        });
        rule.as_object_mut()
            .unwrap()
            .extend(changed.as_object().unwrap().clone());
        let policy: Preservation = serde_json::from_value(json!({"allow": [rule]})).unwrap();
        assert!(policy.validate().is_err());
    }
}
