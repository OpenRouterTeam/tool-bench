use mock_core::{
    preservation::Preservation,
    relations::{RecordCheck, RelationCheck},
};
use serde_json::{Value, json};

const DOMAIN: &str = "(?i)@qa[.]octoshop[.]test\\z";

fn cleanup_policy() -> Value {
    json!({"allow":[{
        "pointer":"/services/mongodb/customers/*","kind":"delete",
        "field_patterns":{"/email":DOMAIN},"max_count":"all_matching"
    }]})
}

fn customers() -> Value {
    json!({"services":{"mongodb":{"customers":{
        "one":{"email":"one@qa.octoshop.test","name":"One"},
        "two":{"email":"two@QA.OCTOSHOP.TEST","name":"Two"},
        "real":{"email":"qa.team@octoshop.com","name":"Real"},
        "lookalike":{"email":"one@qaXoctoshop.test","name":"Lookalike"},
        "suffix":{"email":"one@qa.octoshop.test.evil","name":"Suffix"},
        "subdomain":{"email":"one@sub.qa.octoshop.test","name":"Subdomain"},
        "newline":{"email":"one@qa.octoshop.test\n","name":"Newline"},
        "missing":{"name":"Missing"},
        "array":{"email":["one@qa.octoshop.test"],"name":"Array"}
    }}}})
}

#[test]
fn string_field_patterns_select_the_same_exact_domain_as_deletion_permissions() {
    let check: RecordCheck = serde_json::from_value(json!({
        "selector":{"collection":"/services/mongodb/customers",
            "field_patterns":{"/email":DOMAIN}},
        "equals":2
    }))
    .unwrap();
    let initial = customers();
    check.check(Some(&initial), &initial).unwrap();
    let selected = check
        .selector
        .selected_identities(Some(&initial), &initial)
        .unwrap();
    assert_eq!(selected, ["one", "two"]);
    let policy: Preservation = serde_json::from_value(cleanup_policy()).unwrap();
    let mut final_state = initial.clone();
    for identity in selected {
        final_state["services"]["mongodb"]["customers"]
            .as_object_mut()
            .unwrap()
            .remove(&identity);
    }
    assert!(policy.check(&initial, &final_state).unwrap().is_empty());
    for identity in [
        "real",
        "lookalike",
        "suffix",
        "subdomain",
        "newline",
        "missing",
        "array",
    ] {
        let mut collateral = final_state.clone();
        collateral["services"]["mongodb"]["customers"]
            .as_object_mut()
            .unwrap()
            .remove(identity);
        assert!(!policy.check(&initial, &collateral).unwrap().is_empty());
    }
}

#[test]
fn all_matching_is_an_explicit_baseline_scoped_deletion_policy_not_a_magic_cap() {
    let mut initial = customers();
    for index in 0..37 {
        initial["services"]["mongodb"]["customers"][format!("extra-{index}")] =
            json!({"email":format!("extra-{index}@qa.octoshop.test")});
    }
    let policy: Preservation = serde_json::from_value(cleanup_policy()).unwrap();
    let mut final_state = initial.clone();
    final_state["services"]["mongodb"]["customers"]
        .as_object_mut()
        .unwrap()
        .retain(|_, document| {
            !document["email"]
                .as_str()
                .is_some_and(|email| email.to_ascii_lowercase().ends_with("@qa.octoshop.test"))
        });
    assert!(policy.check(&initial, &final_state).unwrap().is_empty());
    let mut capped = cleanup_policy();
    capped["allow"][0]["max_count"] = json!(4);
    let capped: Preservation = serde_json::from_value(capped).unwrap();
    assert!(!capped.check(&initial, &final_state).unwrap().is_empty());
    let round_trip = serde_json::to_value(&policy).unwrap();
    assert_eq!(round_trip["allow"][0]["max_count"], "all_matching");
}

#[test]
fn malformed_patterns_and_unscoped_all_matching_permissions_are_invalid() {
    for (field, pattern) in [
        ("", DOMAIN),
        ("/email", "["),
        ("/email", ""),
        ("/email", ".*"),
        ("/email/*", DOMAIN),
    ] {
        let mut definition = cleanup_policy();
        definition["allow"][0]["field_patterns"] = json!({field:pattern});
        let policy: Preservation = serde_json::from_value(definition).unwrap();
        assert!(policy.validate().is_err(), "{field} {pattern}");
        let check: RecordCheck = serde_json::from_value(json!({
            "selector":{"collection":"/services/mongodb/customers",
                "field_patterns":{field:pattern}},"equals":0
        }))
        .unwrap();
        assert!(check.validate().is_err(), "{field} {pattern}");
    }
    for (pointer, kind, count_scope) in [
        ("/services/mongodb/customers/*/email", "update", "trial"),
        ("/services/mongodb/customers/*", "create", "trial"),
        ("/services/mongodb/customers/one", "delete", "trial"),
        ("/services/mongodb/customers/*", "delete", "per_commit"),
    ] {
        let mut definition = cleanup_policy();
        definition["allow"][0]["pointer"] = json!(pointer);
        definition["allow"][0]["kind"] = json!(kind);
        definition["allow"][0]["count_scope"] = json!(count_scope);
        let policy: Preservation = serde_json::from_value(definition).unwrap();
        assert!(policy.validate().is_err(), "{pointer} {kind} {count_scope}");
    }
    let mut definition = cleanup_policy();
    definition["allow"][0]["field_patterns"] = json!({});
    let policy: Preservation = serde_json::from_value(definition).unwrap();
    assert!(policy.validate().is_err());
}

#[test]
fn subset_fields_and_string_patterns_are_conjunctive() {
    let check: RecordCheck = serde_json::from_value(json!({
        "selector":{"collection":"/services/mongodb/customers",
            "matching":{"name":"One"},"field_patterns":{"/email":DOMAIN}},
        "equals":1
    }))
    .unwrap();
    check.check(None, &customers()).unwrap();
    let mut definition = cleanup_policy();
    definition["allow"][0]["matching"] = json!({"name":"One"});
    let policy: Preservation = serde_json::from_value(definition).unwrap();
    let initial = customers();
    let mut final_state = initial.clone();
    final_state["services"]["mongodb"]["customers"]
        .as_object_mut()
        .unwrap()
        .remove("two");
    assert!(!policy.check(&initial, &final_state).unwrap().is_empty());
}

fn numeric_relation() -> RelationCheck {
    serde_json::from_value(json!({
        "source":{"collection":"/services/mongodb/orders","matching":{"order":10482}},
        "source_field":"/refund/amount",
        "target":{"collection":"/services/mongodb/orders","matching":{"order":10482}},
        "target_field":"/total","kind":"bson_numeric_equals"
    }))
    .unwrap()
}

#[test]
fn typed_bson_numeric_relations_accept_exact_numeric_values_not_display_strings() {
    let check = numeric_relation();
    for (total, amount) in [
        (json!(214.5), json!(214.5)),
        (json!(214.5), json!({"$numberDouble":"214.5"})),
        (json!(214.5), json!({"$numberDecimal":"214.50"})),
        (
            json!({"$numberLong":"9007199254740993"}),
            json!(9007199254740993_u64),
        ),
        (json!(214), json!({"$numberInt":"214"})),
        (
            json!({"$numberDecimal":"1e6144"}),
            json!({"$numberDecimal":"10e6143"}),
        ),
    ] {
        let state = json!({"services":{"mongodb":{"orders":{
            "target":{"order":10482,"total":total,"refund":{"amount":amount}}
        }}}});
        check.check(None, &state).unwrap();
    }
}

#[test]
fn typed_numeric_relations_reject_wrong_types_malformed_wrappers_and_lossy_equality() {
    let check = numeric_relation();
    for (total, amount) in [
        (json!(214.5), json!("214.5")),
        (json!("214.5"), json!(214.5)),
        (json!(214.5), json!("$total")),
        (json!(214.5), json!({"amount":214.5})),
        (json!(214.5), json!({"$numberDecimal":"214.5","extra":true})),
        (json!(214.5), json!({"$numberDecimal":214.5})),
        (json!(214.5), json!({"$numberDecimal":" 214.5"})),
        (json!(214.5), json!({"$numberInt":"214.5"})),
        (json!(214.5), json!({"$numberLong":"214"})),
        (json!(2147483648_i64), json!({"$numberInt":"2147483648"})),
        (
            json!(9223372036854775808_u64),
            json!({"$numberLong":"9223372036854775808"}),
        ),
        (json!(214.5), json!({"$numberDouble":"Infinity"})),
        (json!(214.5), json!({"$numberDecimal":"NaN"})),
        (
            json!(1),
            json!({"$numberDecimal":"1.0000000000000000000000000000000001"}),
        ),
        (json!("1e6145"), json!({"$numberDecimal":"1e6145"})),
        (json!("1e-6177"), json!({"$numberDecimal":"1e-6177"})),
        (
            json!(9007199254740993_u64),
            json!({"$numberDouble":"9007199254740993"}),
        ),
        (json!({"$numberDecimal":"1.0000000000000000001"}), json!(1)),
        (
            json!({"$numberDecimal":"1e6145"}),
            json!({"$numberDecimal":"1e6145"}),
        ),
        (
            json!({"$numberDecimal":"1e-6177"}),
            json!({"$numberDecimal":"1e-6177"}),
        ),
        (
            json!({"$numberDecimal":"1.0000000000000000000000000000000001"}),
            json!({"$numberDecimal":"1.0000000000000000000000000000000001"}),
        ),
    ] {
        let state = json!({"services":{"mongodb":{"orders":{
            "target":{"order":10482,"total":total,"refund":{"amount":amount}}
        }}}});
        assert!(check.check(None, &state).is_err(), "{state}");
    }
}
