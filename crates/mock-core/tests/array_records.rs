use mock_core::{
    preservation::Preservation,
    relations::{RecordCheck, RecordCheckError},
};
use serde_json::{Value, json};

fn state(variants: Value) -> Value {
    json!({"services": {"shop": {"products": {"target": {"id": 1, "variants": variants}}}}})
}

fn check() -> RecordCheck {
    serde_json::from_value(json!({
        "selector": {
            "collection": "/services/shop/products/target/variants",
            "identity_field": "/id", "matching": {"id": 42}
        },
        "equals": 1, "fields": {"/price": "42.00"}
    }))
    .unwrap()
}

fn policy() -> Preservation {
    serde_json::from_value(json!({"allow": [{
        "pointer": "/services/shop/products/target/variants/*/price",
        "element_matching": {"id": 42}
    }]}))
    .unwrap()
}

#[test]
fn array_outcomes_and_permissions_follow_identity_instead_of_a_fixture_position() {
    for variants in [
        json!([{"id": 42, "price": "48.00"}, {"id": 43, "price": "55.00"}]),
        json!([{"id": 43, "price": "55.00"}, {"id": 42, "price": "48.00"}]),
    ] {
        let before = state(variants);
        let mut after = before.clone();
        let variants = after["services"]["shop"]["products"]["target"]["variants"]
            .as_array_mut()
            .unwrap();
        variants.iter_mut().find(|row| row["id"] == 42).unwrap()["price"] = json!("42.00");
        check().check(Some(&before), &after).unwrap();
        assert!(policy().check(&before, &after).unwrap().is_empty());
    }
}

#[test]
fn element_scope_cannot_authorize_wrong_variants_identity_replacement_or_reordering() {
    let before = state(json!([{"id": 42, "price": "48.00"}, {"id": 43, "price": "55.00"}]));
    for variants in [
        json!([{"id": 42, "price": "42.00"}, {"id": 43, "price": "42.00"}]),
        json!([{"id": 44, "price": "42.00"}, {"id": 43, "price": "55.00"}]),
        json!([{"id": 43, "price": "55.00"}, {"id": 42, "price": "42.00"}]),
        json!([{"id": 42, "price": "42.00"}]),
    ] {
        assert!(
            !policy()
                .check(&before, &state(variants))
                .unwrap()
                .is_empty()
        );
    }
}

#[test]
fn created_array_records_use_initial_identities_not_array_offsets() {
    let check: RecordCheck = serde_json::from_value(json!({
        "selector": {
            "collection": "/services/shop/products/target/variants",
            "identity_field": "/id", "created": true
        },
        "equals": 1, "fields": {"/id": 44}
    }))
    .unwrap();
    let before = state(json!([{"id": 42}, {"id": 43}]));
    check
        .check(
            Some(&before),
            &state(json!([{"id": 43}, {"id": 44}, {"id": 42}])),
        )
        .unwrap();
}

#[test]
fn array_identities_remain_typed_and_duplicates_cannot_be_resolved_by_taking_first() {
    for variants in [
        json!([{"id": "42", "price": "42.00"}]),
        json!([{"id": 42, "price": "42.00"}, {"id": 42, "price": "42.00"}]),
    ] {
        assert!(matches!(
            check().check(None, &state(variants)),
            Err(RecordCheckError::Failed(_))
        ));
    }
}

#[test]
fn array_wildcards_without_a_bounded_element_match_are_invalid() {
    for definition in [
        json!({"allow": [{"pointer": "/services/shop/products/target/variants/*/price"}]}),
        json!({"allow": [{"pointer": "/services/shop/products/target/variants/*", "element_matching": {"id": 42}}]}),
        json!({"allow": [{"pointer": "/services/shop/products/target/variants/0/price", "element_matching": {"id": 42}}]}),
        json!({"allow": [{"pointer": "/services/shop/products/target/variants/*/price", "element_matching": {}}]}),
    ] {
        let policy: Preservation = serde_json::from_value(definition).unwrap();
        assert!(policy.validate().is_err());
    }
}

#[test]
fn array_scoped_string_sets_resolve_each_actual_element_and_do_not_share_a_cached_permission() {
    let policy: Preservation = serde_json::from_value(json!({"allow": [{
        "pointer": "/services/shop/products/target/variants/*/tags",
        "element_matching": {"enabled": true}, "add_set_values": ["VIP"]
    }]}))
    .unwrap();
    let before = state(json!([
        {"id": 42, "enabled": true, "tags": ["old"]},
        {"id": 43, "enabled": true, "tags": ["other"]}
    ]));
    let after = state(json!([
        {"id": 42, "enabled": true, "tags": ["old", "VIP"]},
        {"id": 43, "enabled": true, "tags": ["VIP"]}
    ]));
    assert!(!policy.check(&before, &after).unwrap().is_empty());
}
