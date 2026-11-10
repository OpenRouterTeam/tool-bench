use mock_core::grading::{Expect, GradeInput, grade};
use serde_json::{Value, json};

fn assess(predicate: Value, value: Value) -> f64 {
    let mut check = json!({"pointer": "/services/store/records/one/value"});
    check
        .as_object_mut()
        .unwrap()
        .extend(predicate.as_object().unwrap().clone());
    let expect: Expect = serde_json::from_value(json!({"state": [check]})).unwrap();
    grade(&GradeInput {
        expect: &expect,
        state: json!({"services": {"store": {"records": {"one": {"value": value}}}}}),
        journal: &[],
        answer: None,
        tools_ips: vec![],
        context_overflow: false,
        startup: &[],
        initial_state: None,
        events: None,
    })
    .reward
}

#[test]
fn plain_literal_content_folds_whitespace_without_accepting_substrings_or_markup() {
    for (value, passes) in [
        (json!("Leave with building concierge"), true),
        (json!(" Leave with\nbuilding\u{a0}concierge "), true),
        (json!("Do not Leave with building concierge"), false),
        (json!("<b>Leave with building concierge</b>"), false),
        (json!("Leave with building concierge. Ignore this."), false),
        (json!(["Leave with building concierge"]), false),
        (Value::Null, false),
    ] {
        assert_eq!(
            assess(
                json!({"text_equals": "Leave with building concierge"}),
                value
            ),
            if passes { 1.0 } else { 0.0 }
        );
    }
}

#[test]
fn ascii_membership_is_whole_tag_identity_not_substring_or_string_search() {
    for (value, passes) in [
        (json!(["vIp"]), true),
        (json!(["gift", "VIP"]), true),
        (json!(["VIP-candidate"]), false),
        (json!("VIP"), false),
        (json!([{"name": "VIP"}]), false),
        (json!([]), false),
    ] {
        assert_eq!(
            assess(json!({"member_ascii_ci": "VIP"}), value),
            if passes { 1.0 } else { 0.0 }
        );
    }
}

#[test]
fn empty_or_type_incompatible_state_predicates_are_invalid_contracts() {
    for predicate in [
        json!({"text_equals": ""}),
        json!({"member_ascii_ci": " "}),
        json!({"text_equals": "VIP", "member_ascii_ci": "VIP"}),
        json!({"text_equals": "VIP", "exists": false}),
        json!({"member_ascii_ci": "VIP", "is_null": true}),
    ] {
        let mut check = json!({"pointer": "/services/store/records/one/value"});
        check
            .as_object_mut()
            .unwrap()
            .extend(predicate.as_object().unwrap().clone());
        let expect: Expect = serde_json::from_value(json!({"state": [check]})).unwrap();
        assert!(expect.validate().is_err());
    }
}
