use mock_core::preservation::Preservation;
use serde_json::{Value, json};

fn snapshot(amount: Value) -> Value {
    json!({"services": {"sales": {"opportunities": {"target": {"amount": amount}}}}})
}

fn policy() -> Preservation {
    serde_json::from_value(json!({
        "equivalent_numbers": ["/services/sales/opportunities/target/amount"]
    }))
    .unwrap()
}

#[test]
fn integer_conversion_is_exact_bounded_and_representation_independent() {
    for value in [
        json!(4),
        json!("4"),
        json!("4.000"),
        json!("0.4e1"),
        json!(" +0004 "),
    ] {
        assert_eq!(mock_core::exact_integer(&value), Some(4));
    }
    assert_eq!(mock_core::exact_integer(&json!(i64::MIN)), Some(i64::MIN));
    assert_eq!(mock_core::exact_integer(&json!(i64::MAX)), Some(i64::MAX));
    assert_eq!(
        mock_core::exact_integer(&json!("9007199254740993")),
        Some(9_007_199_254_740_993)
    );
    for value in [
        json!("4.0000000001"),
        json!("1e1000000000"),
        json!("9223372036854775808"),
        json!("NaN"),
        json!(false),
        Value::Null,
    ] {
        assert_eq!(mock_core::exact_integer(&value), None, "{value}");
    }
}

#[test]
fn explicitly_declared_decimal_fields_accept_exact_value_equivalent_representations() {
    for (before, after) in [
        (json!(184000), json!(184000.0)),
        (json!("184000.000"), json!("1.84e5")),
        (json!("-0.00"), json!(0)),
        (json!("0.4200e2"), json!("42")),
        (json!("+00042.00"), json!(42)),
        (json!(".5"), json!("0.50")),
        (json!("42."), json!("42.000")),
        (json!("1e1000000000"), json!("10e999999999")),
    ] {
        assert!(
            policy()
                .check(&snapshot(before), &snapshot(after))
                .unwrap()
                .is_empty()
        );
    }
}

#[test]
fn real_changes_are_not_hidden_by_floating_point_rounding_or_an_epsilon() {
    for (before, after) in [
        (
            json!(9_007_199_254_740_993_u64),
            json!(9_007_199_254_740_992_f64),
        ),
        (json!(u64::MAX), json!(u64::MAX as f64)),
        (json!("1.0000000000000000001"), json!(1.0)),
        (json!("42"), json!("42.000000001")),
        (json!(-42), json!(42)),
        (json!("1e1000"), json!("1e1001")),
        (Value::Null, json!(0)),
        (json!(true), json!(1)),
        (json!("42 USD"), json!(42)),
        (json!("NaN"), json!(0)),
        (json!("1e1e1"), json!(100)),
    ] {
        assert!(
            !policy()
                .check(&snapshot(before), &snapshot(after))
                .unwrap()
                .is_empty()
        );
    }
}

#[test]
fn undeclared_fields_and_identity_types_remain_strict() {
    assert!(
        !Preservation::default()
            .check(&snapshot(json!(42)), &snapshot(json!(42.0)))
            .unwrap()
            .is_empty()
    );
    let before =
        json!({"services": {"sales": {"opportunities": {"target": {"id": 1, "amount": 42}}}}});
    let after =
        json!({"services": {"sales": {"opportunities": {"target": {"id": "1", "amount": 42.0}}}}});
    let failures = policy().check(&before, &after).unwrap();
    assert_eq!(failures.len(), 1);
    assert!(failures[0].pointer.ends_with("/id"));
}

#[test]
fn numeric_equivalence_cannot_authorize_record_creation_or_deletion() {
    let missing = json!({"services": {"sales": {"opportunities": {}}}});
    assert!(
        !policy()
            .check(&missing, &snapshot(json!(42)))
            .unwrap()
            .is_empty()
    );
    assert!(
        !policy()
            .check(&snapshot(json!(42)), &missing)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn malformed_or_duplicate_numeric_field_paths_are_invalid() {
    for fields in [
        vec!["/services/sales/opportunities/target"],
        vec!["/services/sales/opportunities/*/amount"],
        vec![
            "/services/sales/opportunities/target/amount",
            "/services/sales/opportunities/target/amount",
        ],
    ] {
        let policy: Preservation =
            serde_json::from_value(json!({"equivalent_numbers": fields})).unwrap();
        assert!(policy.validate().is_err());
    }
}

#[test]
fn numeric_change_permissions_constrain_actual_values_without_spelling_assumptions() {
    let policy: Preservation = serde_json::from_value(json!({"allow":[{
        "pointer":"/services/sales/opportunities/target/amount",
        "numeric_fields":{"/amount":4}
    }]}))
    .unwrap();
    for value in [json!(4), json!("4.000"), json!("0.4e1"), json!(" +0004 ")] {
        assert!(
            policy
                .check(&snapshot(json!(2)), &snapshot(value))
                .unwrap()
                .is_empty()
        );
    }
    for value in [
        json!(99),
        json!("4.00000001"),
        json!("four"),
        json!(true),
        Value::Null,
    ] {
        assert!(
            !policy
                .check(&snapshot(json!(2)), &snapshot(value))
                .unwrap()
                .is_empty()
        );
    }
    assert!(
        !policy
            .check(
                &snapshot(json!(2)),
                &json!({"services":{"sales":{"opportunities":{}}}}),
            )
            .unwrap()
            .is_empty()
    );
}

#[test]
fn numeric_change_permissions_reject_ambiguous_or_nonliteral_contracts() {
    for fields in [
        json!({"numeric_fields":{"":4}}),
        json!({"numeric_fields":{"/items/*/amount":4}}),
        json!({"numeric_fields":{"/amount":"four"}}),
        json!({"numeric_fields":{"/amount":true}}),
        json!({"numeric_fields":{"/amount":4},"field_patterns":{"/amount":"4"}}),
    ] {
        let mut rule = json!({"pointer":"/services/sales/opportunities/target/amount"});
        rule.as_object_mut()
            .unwrap()
            .extend(fields.as_object().unwrap().clone());
        let policy: Preservation = serde_json::from_value(json!({"allow":[rule]})).unwrap();
        assert!(policy.validate().is_err());
    }
}
