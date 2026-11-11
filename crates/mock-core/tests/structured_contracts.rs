use mock_core::{
    preservation::Preservation,
    relations::{RecordCheck, RecordCheckError},
};
use serde_json::{Value, json};

fn state(value: Value) -> Value {
    json!({"services":{"configuration":{"values":{"target":{"value":value}}}}})
}

fn json_check() -> RecordCheck {
    serde_json::from_value(json!({
        "selector":{"collection":"/services/configuration/values","key_prefix":"target"},
        "equals":1,
        "json_fields":{"/value":{"enabled":true,"rollout":25,"owner":"payments","fallback":"v1"}}
    }))
    .unwrap()
}

#[test]
fn json_fields_compare_complete_values_without_requiring_one_serialization() {
    for source in [
        r#"{"enabled":true,"rollout":25,"owner":"payments","fallback":"v1"}"#,
        "{ \"fallback\": \"v1\", \"rollout\": 25.0, \"owner\": \"payments\", \"enabled\": true }",
    ] {
        json_check().check(None, &state(json!(source))).unwrap();
    }
    for source in [
        r#"{"enabled":true,"rollout":"25","owner":"payments","fallback":"v1"}"#,
        r#"{"enabled":true,"rollout":25,"owner":"payments","fallback":"v1","extra":true}"#,
        r#"{"enabled":true,"rollout":25,"owner":"payments"}"#,
        r#"{"enabled":true,"rollout":25,"owner":"payments","fallback":"v1"} false"#,
        "not JSON",
    ] {
        assert!(matches!(
            json_check().check(None, &state(json!(source))),
            Err(RecordCheckError::Failed(_))
        ));
    }
}

#[test]
fn duplicate_json_members_including_escaped_and_nested_aliases_are_not_interoperable_evidence() {
    for source in [
        r#"{"enabled":false,"enabled":true,"rollout":25,"owner":"payments","fallback":"v1"}"#,
        r#"{"\u0065nabled":false,"enabled":true,"rollout":25,"owner":"payments","fallback":"v1"}"#,
    ] {
        assert!(matches!(
            json_check().check(None, &state(json!(source))),
            Err(RecordCheckError::Failed(_))
        ));
    }
    let check: RecordCheck = serde_json::from_value(json!({
        "selector":{"collection":"/services/configuration/values","key_prefix":"target"},
        "equals":1,"json_fields":{"/value":{"nested":{"key":2}}}
    }))
    .unwrap();
    assert!(
        check
            .check(None, &state(json!(r#"{"nested":{"key":1,"key":2}}"#)))
            .is_err()
    );
}

fn sets() -> RecordCheck {
    serde_json::from_value(json!({
        "selector":{"collection":"/services/configuration/routes","key_prefix":"actual-"},
        "min_count":1,"max_count":3,
        "value_sets":{"/pattern":[["all"],["http","https"],["all","http","https"]]}
    }))
    .unwrap()
}

fn routes(patterns: &[&str]) -> Value {
    let records: serde_json::Map<String, Value> = patterns
        .iter()
        .enumerate()
        .map(|(index, pattern)| (format!("actual-{index}"), json!({"pattern":pattern})))
        .collect();
    json!({"services":{"configuration":{"routes":records}}})
}

#[test]
fn record_value_sets_accept_equivalent_complete_plans_not_partial_or_extra_coverage() {
    for patterns in [
        vec!["all"],
        vec!["https", "http"],
        vec!["http", "all", "https"],
    ] {
        sets().check(None, &routes(&patterns)).unwrap();
    }
    for patterns in [
        vec!["http"],
        vec!["https"],
        vec!["all", "other"],
        vec!["http", "https", "https"],
    ] {
        assert!(matches!(
            sets().check(None, &routes(&patterns)),
            Err(RecordCheckError::Failed(_))
        ));
    }
}

#[test]
fn malformed_or_competing_structured_predicates_do_not_enter_the_suite() {
    for choices in [
        json!([]),
        json!([[]]),
        json!([["same", "same"]]),
        json!([["first", "second"], ["second", "first"]]),
    ] {
        let mut definition = serde_json::to_value(sets()).unwrap();
        definition["value_sets"]["/pattern"] = choices;
        assert!(
            serde_json::from_value::<RecordCheck>(definition)
                .unwrap()
                .validate()
                .is_err()
        );
    }
    for competing in ["fields", "numeric_fields", "reviewed_text", "value_sets"] {
        let mut definition = serde_json::to_value(json_check()).unwrap();
        definition[competing] = match competing {
            "reviewed_text" => json!({"/value":{"requirement":"text","accepted":["text"]}}),
            "value_sets" => json!({"/value":[["text"]]}),
            _ => json!({"/value":1}),
        };
        assert!(
            serde_json::from_value::<RecordCheck>(definition)
                .unwrap()
                .validate()
                .is_err()
        );
    }
}

fn projection() -> Preservation {
    serde_json::from_value(json!({
        "json_strings":["/services/configuration/values/target/value"],
        "allow":[
            {"pointer":"/services/configuration/values/target/value/enabled"},
            {"pointer":"/services/configuration/values/target/value/rollout"}
        ]
    }))
    .unwrap()
}

#[test]
fn encoded_json_preservation_is_field_scoped_and_ignores_only_representation_changes() {
    let initial = state(json!(
        r#"{"enabled":false,"rollout":0,"owner":"payments","fallback":"v1","nested":{"number":1}}"#
    ));
    let changed = state(json!(
        r#"{"nested":{"number":1.0},"fallback":"v1","owner":"payments","rollout":25,"enabled":true}"#
    ));
    assert!(projection().check(&initial, &changed).unwrap().is_empty());
    for source in [
        r#"{"enabled":true,"rollout":25,"owner":"other","fallback":"v1","nested":{"number":1}}"#,
        r#"{"enabled":true,"rollout":25,"owner":"payments","nested":{"number":1}}"#,
        r#"{"enabled":true,"rollout":25,"owner":"payments","fallback":"v1","nested":{"number":2}}"#,
        r#"{"enabled":false,"enabled":true,"rollout":25,"owner":"payments","fallback":"v1","nested":{"number":1}}"#,
        "not JSON",
    ] {
        assert!(
            !projection()
                .check(&initial, &state(json!(source)))
                .unwrap()
                .is_empty()
        );
    }
}

#[test]
fn malformed_initial_json_is_missing_environment_evidence_not_an_agent_failure() {
    assert!(
        projection()
            .check(&state(json!("not JSON")), &state(json!("{}")))
            .is_err()
    );
}

#[test]
fn encoded_json_projection_declarations_must_be_literal_distinct_and_nonoverlapping() {
    for pointers in [
        json!(["/services/configuration/values/*/value"]),
        json!([
            "/services/configuration/values/target/value",
            "/services/configuration/values/target/value"
        ]),
        json!([
            "/services/configuration/values/target/value",
            "/services/configuration/values/target/value/nested"
        ]),
        json!(["/services/configuration/values/target"]),
    ] {
        let policy: Preservation =
            serde_json::from_value(json!({"json_strings":pointers})).unwrap();
        assert!(policy.validate().is_err());
    }
}

#[test]
fn decoded_json_cannot_gain_credit_by_rounding_a_different_decimal_to_the_expected_number() {
    for source in [
        r#"{"enabled":true,"rollout":25.000000000000000001,"owner":"payments","fallback":"v1"}"#,
        r#"{"enabled":true,"rollout":24.999999999999999999,"owner":"payments","fallback":"v1"}"#,
    ] {
        assert!(matches!(
            json_check().check(None, &state(json!(source))),
            Err(RecordCheckError::Failed(_))
        ));
    }
}

#[test]
fn malformed_encoded_strings_cannot_alias_a_previously_decoded_json_string() {
    let initial = state(json!(r#""not JSON""#));
    assert!(
        !projection()
            .check(&initial, &state(json!("not JSON")))
            .unwrap()
            .is_empty()
    );
}

#[test]
fn declared_url_host_equivalence_does_not_fold_paths_or_change_stored_values() {
    let check: RecordCheck = serde_json::from_value(json!({
        "selector":{"collection":"/services/configuration/routes","key_prefix":"actual-"},
        "min_count":1,"max_count":2,
        "url_host_fields":["/pattern"],
        "value_sets":{"/pattern":[["api.example.com/v2/*"],["http://api.example.com/v2/*","https://api.example.com/v2/*"]]}
    })).unwrap();
    let state = routes(&["API.EXAMPLE.COM/v2/*"]);
    check.check(None, &state).unwrap();
    assert_eq!(
        state["services"]["configuration"]["routes"]["actual-0"]["pattern"],
        "API.EXAMPLE.COM/v2/*"
    );
    check
        .check(
            None,
            &routes(&[
                "https://API.EXAMPLE.COM/v2/*",
                "http://Api.Example.Com/v2/*",
            ]),
        )
        .unwrap();
    for patterns in [
        vec!["api.example.com/V2/*"],
        vec!["api.example.com/v2/*#fragment"],
        vec!["https://api.example.com/v2/*"],
    ] {
        assert!(check.check(None, &routes(&patterns)).is_err());
    }
}

#[test]
fn creation_matchers_can_compare_url_hosts_without_authorizing_other_paths() {
    let policy: Preservation = serde_json::from_value(json!({
        "allow":[{"pointer":"/services/configuration/routes/*","kind":"create",
            "matching":{"pattern":"api.example.com/v2/*"},"url_host_fields":["/pattern"]}]
    }))
    .unwrap();
    let initial = json!({"services":{}});
    assert!(
        policy
            .check(&initial, &routes(&["API.EXAMPLE.COM/v2/*"]))
            .unwrap()
            .is_empty()
    );
    assert!(
        !policy
            .check(&initial, &routes(&["api.example.com/V2/*"]))
            .unwrap()
            .is_empty()
    );
}

#[test]
fn a_leaf_removal_permission_is_not_a_whole_field_rewrite_or_record_delete() {
    let policy: Preservation = serde_json::from_value(json!({
        "allow":[{"pointer":"/services/configuration/values/target/metadata","kind":"remove_field"}]
    }))
    .unwrap();
    let mut initial = state(json!(1));
    initial["services"]["configuration"]["values"]["target"]["metadata"] =
        json!({"ticket":"original"});
    let removed = state(json!(1));
    assert!(policy.check(&initial, &removed).unwrap().is_empty());
    for metadata in [json!({}), json!({"ticket":"other"}), Value::Null] {
        let mut changed = initial.clone();
        changed["services"]["configuration"]["values"]["target"]["metadata"] = metadata;
        assert!(!policy.check(&initial, &changed).unwrap().is_empty());
    }
    assert!(
        !policy
            .check(&initial, &json!({"services":{}}))
            .unwrap()
            .is_empty()
    );
}

#[test]
fn url_host_and_leaf_removal_declarations_cannot_silently_expand_their_scope() {
    for extra in [
        json!({"url_host_fields":["/pattern"]}),
        json!({"url_host_fields":["/pattern","/pattern"],"value_sets":{"/pattern":[["api.example.com/v2/*"]]}}),
        json!({"url_host_fields":["/pattern"],"value_sets":{"/pattern":[["api.example.com/v2/*"],["API.EXAMPLE.COM/v2/*"]]}}),
        json!({"url_host_fields":["/pattern"],"value_sets":{"/pattern":[[3]]}}),
    ] {
        let mut definition = json!({"selector":{"collection":"/services/configuration/routes","created":true},"equals":1});
        definition
            .as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        assert!(
            serde_json::from_value::<RecordCheck>(definition)
                .unwrap()
                .validate()
                .is_err()
        );
    }
    for definition in [
        json!({"allow":[{"pointer":"/services/configuration/values/target","kind":"remove_field"}]}),
        json!({"allow":[{"pointer":"/services/configuration/values/target/value","url_host_fields":["/pattern"]}]}),
        json!({"allow":[{"pointer":"/services/configuration/routes/*","kind":"create","matching":{"pattern":3},"url_host_fields":["/pattern"]}]}),
    ] {
        assert!(
            serde_json::from_value::<Preservation>(definition)
                .unwrap()
                .validate()
                .is_err()
        );
    }
}

#[test]
fn leaf_restoration_only_accepts_the_actual_initial_value_not_a_matching_subset() {
    let policy:Preservation=serde_json::from_value(json!({
        "allow":[{"pointer":"/services/configuration/values/target/metadata","kind":"restore_field"}]
    })).unwrap();
    let mut initial = state(json!(1));
    initial["services"]["configuration"]["values"]["target"]["metadata"] =
        json!({"ticket":"original"});
    assert!(policy.check(&initial, &initial).unwrap().is_empty());
    for metadata in [
        json!({"ticket":"original","extra":true}),
        json!({"ticket":"other"}),
        Value::Null,
    ] {
        let mut changed = initial.clone();
        changed["services"]["configuration"]["values"]["target"]["metadata"] = metadata;
        assert!(!policy.check(&initial, &changed).unwrap().is_empty());
    }
}
