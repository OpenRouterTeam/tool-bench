use mock_core::{
    preservation::Preservation,
    quantity::{ParseError, Quantity},
    relations::{RecordCheck, RecordCheckError, RecordSelector},
};
use serde_json::{Value, json};

fn quantity(value: Value) -> Quantity {
    Quantity::parse(&value).unwrap()
}

#[test]
fn exact_memory_quantities_do_not_depend_on_string_spelling() {
    for value in [
        json!("1Gi"),
        json!("1024Mi"),
        json!("1073741824"),
        json!(1073741824),
        json!("1.073741824e9"),
        json!("1073741824000m"),
        json!("+1Gi"),
        json!(" 1Gi "),
        json!(".5Gi"),
        json!("512Mi"),
    ] {
        let expected = if value == ".5Gi" || value == "512Mi" {
            "512Mi"
        } else {
            "1Gi"
        };
        assert_eq!(quantity(value), quantity(json!(expected)));
    }
    assert_ne!(quantity(json!("1G")), quantity(json!("1Gi")));
    assert_ne!(
        quantity(json!("1073741823.999999999")),
        quantity(json!("1Gi"))
    );
    assert_ne!(
        quantity(json!("1073741824.000000001")),
        quantity(json!("1Gi"))
    );
}

#[test]
fn nanounit_rounding_and_binary_saturation_follow_reviewed_quantity_semantics() {
    for (value, expected) in [
        ("3.001n", "4n"),
        ("1.1E-9", "2n"),
        ("0.0000000001", "1n"),
        ("-0.0000000001", "-1n"),
        ("9999999999.9999999991", "10000000000"),
        ("9Ei", "9223372036854775807"),
        ("-9Ei", "-9223372036854775807"),
        ("0.00000000001Gi", "0.010737419"),
        ("-0", "0"),
    ] {
        assert_eq!(quantity(json!(value)), quantity(json!(expected)), "{value}");
    }
    assert!(quantity(json!("12E")) > quantity(json!("9223372036854775807")));
    assert!(quantity(json!("1n")) < quantity(json!("2n")));
    assert!(quantity(json!("-2n")) < quantity(json!("-1n")));
}

#[test]
fn invalid_and_unreviewed_quantity_inputs_are_not_floating_point_approximations() {
    for value in [
        Value::Null,
        json!(true),
        json!([]),
        json!({}),
        json!(""),
        json!("NaN"),
        json!("1gi"),
        json!("1K"),
        json!("1MiB"),
        json!("1e2Gi"),
        json!("1.2.3"),
        json!("1e"),
    ] {
        assert_eq!(Quantity::parse(&value), Err(ParseError::Invalid), "{value}");
    }
    assert_eq!(
        Quantity::parse(&json!("1e1000001")),
        Err(ParseError::Unsupported)
    );
    assert_eq!(
        Quantity::parse(&json!("1".repeat(4097))),
        Err(ParseError::Unsupported)
    );
}

fn check() -> RecordCheck {
    serde_json::from_value(json!({
        "selector":{"collection":"/services/kubernetes/containers","matching":{"name":"target"}},
        "equals":1,"quantity_fields":{"/memory":"1Gi"}
    }))
    .unwrap()
}

#[test]
fn quantity_record_predicates_distinguish_failures_and_adjudication() {
    for (value, expected) in [
        (json!("1024Mi"), "pass"),
        (json!(1073741824), "pass"),
        (json!("1G"), "fail"),
        (json!(true), "fail"),
        (json!("1e1000001"), "review"),
    ] {
        let state = json!({"services":{"kubernetes":{"containers":{"actual":{"name":"target","memory":value}}}}});
        let result = check().check(None, &state);
        match expected {
            "pass" => assert!(result.is_ok(), "{result:?}"),
            "review" => assert!(matches!(
                result,
                Err(RecordCheckError::NeedsAdjudication(_))
            )),
            _ => assert!(matches!(result, Err(RecordCheckError::Failed(_)))),
        }
    }
}

#[test]
fn quantity_predicates_cannot_compete_with_other_field_predicates() {
    for (field, value) in [
        ("fields", json!({"/memory":"1Gi"})),
        ("numeric_fields", json!({"/memory":1})),
        ("json_fields", json!({"/memory":1})),
        ("optional_fields", json!({"/memory":"1Gi"})),
        ("array_lengths", json!({"/memory":1})),
        (
            "reviewed_text",
            json!({"/memory":{"requirement":"memory","accepted":["1Gi"]}}),
        ),
    ] {
        let mut source = serde_json::to_value(check()).unwrap();
        source[field] = value;
        let invalid: RecordCheck = serde_json::from_value(source).unwrap();
        assert!(invalid.validate().is_err(), "{field}");
    }
    let mut invalid = check();
    invalid.equals = Some(0);
    assert!(invalid.validate().is_err());
}

#[test]
fn declared_quantity_equivalence_does_not_authorize_real_changes() {
    let policy: Preservation = serde_json::from_value(json!({
        "equivalent_quantities":["/services/kubernetes/pods/target/memory"]
    }))
    .unwrap();
    let before = json!({"services":{"kubernetes":{"pods":{"target":{"memory":"512Mi"}}}}});
    for (memory, accepted) in [
        ("536870912", true),
        ("0.5Gi", true),
        ("512M", false),
        ("1Gi", false),
    ] {
        let after = json!({"services":{"kubernetes":{"pods":{"target":{"memory":memory}}}}});
        assert_eq!(
            policy.check(&before, &after).unwrap().is_empty(),
            accepted,
            "{memory}"
        );
    }
}

#[test]
fn latest_record_selection_uses_unique_numeric_revisions_not_keys_or_order() {
    let selector: RecordSelector = serde_json::from_value(json!({
        "collection":"/services/kubernetes/releases","matching":{"name":"target"},"latest_by":"/version"
    })).unwrap();
    let state = json!({"services":{"kubernetes":{"releases":{
        "z":{"name":"target","version":2},"a":{"name":"target","version":10},
        "other":{"name":"decoy","version":999}
    }}}});
    assert_eq!(selector.selected_identities(None, &state).unwrap(), ["a"]);
    let mut ambiguous = state.clone();
    ambiguous["services"]["kubernetes"]["releases"]["z"]["version"] = json!(10);
    assert!(matches!(
        selector.selected_identities(None, &ambiguous),
        Err(RecordCheckError::Failed(_))
    ));
    for invalid in [json!("10"), json!(10.5), Value::Null] {
        let mut state = state.clone();
        state["services"]["kubernetes"]["releases"]["a"]["version"] = invalid;
        assert!(selector.selected_identities(None, &state).is_err());
    }
}
