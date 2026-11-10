use mock_core::relations::{RecordCheck, RelationCheck};
use serde_json::{Value, json};

fn state() -> Value {
    json!({"services": {"finance": {
        "refunds": {
            "first": {"id": "refund_a", "amount": 18000, "transaction": "balance_x"},
            "second": {"id": "refund_b", "amount": 30000, "transaction": "balance_y"}
        },
        "balances": {
            "different_key": {"id": "balance_y", "amount": -30000},
            "other_key": {"id": "balance_x", "amount": -18000}
        }
    }}})
}

fn check() -> RecordCheck {
    serde_json::from_value(json!({
        "selector": {"collection": "/services/finance/refunds", "created": true},
        "min_count": 1, "max_count": 48000,
        "totals": {"/amount": 48000},
        "integer_bounds": {"/amount": {"min": 1, "max": 48000}}
    }))
    .unwrap()
}

fn initial() -> Value {
    json!({"services": {}})
}

#[test]
fn integer_totals_accept_different_valid_splits_without_reference_call_counts() {
    check().check(Some(&initial()), &state()).unwrap();
    let mut single = state();
    single["services"]["finance"]["refunds"] = json!({"only": {"id": "refund_c", "amount": 48000}});
    check().check(Some(&initial()), &single).unwrap();
}

#[test]
fn wrong_totals_missing_fields_fractional_values_and_canceling_negative_amounts_fail() {
    for amount in [
        json!(30001),
        json!(29999),
        json!(30000.0),
        json!("30000"),
        json!(null),
    ] {
        let mut changed = state();
        changed["services"]["finance"]["refunds"]["second"]["amount"] = amount;
        assert!(check().check(Some(&initial()), &changed).is_err());
    }
    let mut changed = state();
    changed["services"]["finance"]["refunds"]["first"]["amount"] = json!(-1000);
    changed["services"]["finance"]["refunds"]["second"]["amount"] = json!(49000);
    assert!(check().check(Some(&initial()), &changed).is_err());
}

#[test]
fn aggregation_uses_wide_checked_integers_and_not_float_rounding() {
    let aggregate: RecordCheck = serde_json::from_value(json!({
        "selector": {"collection": "/services/finance/refunds", "created": true},
        "equals": 4, "totals": {"/amount": 0}
    }))
    .unwrap();
    let mut changed = state();
    changed["services"]["finance"]["refunds"] = json!({
        "first": {"amount": i64::MAX},
        "second": {"amount": i64::MAX},
        "third": {"amount": -i64::MAX},
        "fourth": {"amount": -i64::MAX}
    });
    aggregate.check(Some(&initial()), &changed).unwrap();
    changed["services"]["finance"]["refunds"]["fourth"]["amount"] = json!(-i64::MAX + 1);
    assert!(aggregate.check(Some(&initial()), &changed).is_err());
}

#[test]
fn inert_conflicting_or_reversed_count_and_integer_bounds_fail_validation() {
    for extra in [
        json!({}),
        json!({"min_count": 0}),
        json!({"equals": 1, "min_count": 1}),
        json!({"min_count": 2, "max_count": 1}),
        json!({"equals": 1, "integer_bounds": {"/amount": {"min": 3, "max": 2}}}),
        json!({"equals": 1, "totals": {"bad": 1}}),
    ] {
        let mut definition =
            json!({"selector": {"collection": "/services/finance/refunds", "created": true}});
        definition
            .as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        let check: RecordCheck = serde_json::from_value(definition).unwrap();
        assert!(check.validate().is_err());
    }
}

fn relation() -> RelationCheck {
    serde_json::from_value(json!({
        "source": {"collection": "/services/finance/refunds", "created": true},
        "source_field": "/transaction",
        "target": {"collection": "/services/finance/balances", "created": true},
        "target_field": "/id", "kind": "bijection"
    }))
    .unwrap()
}

#[test]
fn bijections_match_all_actual_identities_independent_of_order_and_record_keys() {
    relation().check(Some(&initial()), &state()).unwrap();
    for changed in ["duplicate", "missing", "wrong", "type", "null"] {
        let mut state = state();
        match changed {
            "duplicate" => {
                state["services"]["finance"]["refunds"]["second"]["transaction"] =
                    json!("balance_x")
            }
            "missing" => {
                state["services"]["finance"]["balances"]
                    .as_object_mut()
                    .unwrap()
                    .remove("other_key");
            }
            "wrong" => {
                state["services"]["finance"]["refunds"]["first"]["transaction"] = json!("balance_z")
            }
            "type" => state["services"]["finance"]["refunds"]["first"]["transaction"] = json!(1),
            "null" => state["services"]["finance"]["refunds"]["first"]["transaction"] = Value::Null,
            _ => unreachable!(),
        }
        assert!(
            relation().check(Some(&initial()), &state).is_err(),
            "{changed}"
        );
    }
    assert!(relation().check(Some(&initial()), &initial()).is_err());
}

#[test]
fn balanced_totals_cannot_hide_amounts_attached_to_the_wrong_refund() {
    let mut definition = serde_json::to_value(relation()).unwrap();
    definition["fields"] = json!([{
        "source_field": "/amount", "target_field": "/amount", "kind": "negated_integer"
    }]);
    let relation: RelationCheck = serde_json::from_value(definition).unwrap();
    relation.check(Some(&initial()), &state()).unwrap();
    let mut swapped = state();
    swapped["services"]["finance"]["balances"]["different_key"]["amount"] = json!(-18000);
    swapped["services"]["finance"]["balances"]["other_key"]["amount"] = json!(-30000);
    assert!(relation.check(Some(&initial()), &swapped).is_err());
    swapped["services"]["finance"]["balances"]["different_key"]["amount"] = json!("-30000");
    assert!(relation.check(Some(&initial()), &swapped).is_err());
}

#[test]
fn joined_field_predicates_reject_inert_or_duplicate_definitions() {
    for fields in [
        json!([{"source_field": "", "target_field": "/amount", "kind": "equals"}]),
        json!([
            {"source_field": "/amount", "target_field": "/amount", "kind": "equals"},
            {"source_field": "/amount", "target_field": "/amount", "kind": "equals"}
        ]),
    ] {
        let mut definition = serde_json::to_value(relation()).unwrap();
        definition["fields"] = fields;
        let relation: RelationCheck = serde_json::from_value(definition).unwrap();
        assert!(relation.validate().is_err());
    }
}
