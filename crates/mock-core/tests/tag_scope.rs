use mock_core::relations::{RecordCheck, RecordCheckError};
use serde_json::{Value, json};

fn initial() -> Value {
    json!({"services":{"datadog":{"hosts":{"replica":{
        "name":"pg-prod-replica-2",
        "tags_by_source":{"agent":["host:pg-prod-replica-2"],"users":["env:prod","role:replica"]}
    }},"downtimes":{}}}})
}

fn check() -> RecordCheck {
    serde_json::from_value(json!({
        "selector":{"collection":"/services/datadog/downtimes","created":true},
        "equals":1,
        "tag_scope":{
            "field":"/scope","equals":{"host":"pg-prod-replica-2"},
            "context":{
                "selector":{"collection":"/services/datadog/hosts","matching":{"name":"pg-prod-replica-2"}},
                "field":"/tags_by_source"
            }
        }
    })).unwrap()
}

fn state(scope: Value) -> Value {
    let mut state = initial();
    state["services"]["datadog"]["downtimes"]["opaque-created-key"] = json!({"scope":scope});
    state
}

#[test]
fn redundant_conjunctions_commas_and_universal_terms_are_semantically_equivalent() {
    for scope in [
        json!(["host:pg-prod-replica-2"]),
        json!(["host:pg-prod-replica-2", "env:prod", "role:replica"]),
        json!(["role:replica, host:pg-prod-replica-2", "host:pg-prod-*"]),
        json!([
            "*",
            "host:*",
            "host:pg-prod-replica-2",
            "host:pg-prod-replica-2"
        ]),
        json!(["env:p*", "host:pg-prod-replica-2"]),
    ] {
        check().check(Some(&initial()), &state(scope)).unwrap();
    }
}

#[test]
fn contradiction_broadening_and_missing_target_tags_are_proven_failures() {
    for scope in [
        json!(["host:pg-prod-replica-2", "host:pg-prod-replica-1"]),
        json!(["host:pg-prod-replica-2", "env:staging"]),
        json!(["host:pg-prod-replica-2", "region:us-east-1"]),
        json!(["host:pg-prod-*"]),
        json!(["host:pg-prod-replica-*", "host:*-2"]),
        json!(["*"]),
        json!(["env:prod"]),
        json!([]),
    ] {
        assert!(matches!(
            check().check(Some(&initial()), &state(scope)),
            Err(RecordCheckError::Failed(_))
        ));
    }
}

#[test]
fn only_the_baseline_context_is_used_to_prove_redundant_restrictions() {
    let mut changed = state(json!(["host:pg-prod-replica-2", "env:staging"]));
    changed["services"]["datadog"]["hosts"]["replica"]["tags_by_source"]["users"] =
        json!(["env:staging"]);
    assert!(matches!(
        check().check(Some(&initial()), &changed),
        Err(RecordCheckError::Failed(_))
    ));
}

#[test]
fn missing_or_ambiguous_context_is_not_mislabeled_as_a_business_failure() {
    assert!(matches!(
        check().check(None, &state(json!(["host:pg-prod-replica-2"]))),
        Err(RecordCheckError::MissingEvidence(_))
    ));
    let mut ambiguous = initial();
    ambiguous["services"]["datadog"]["hosts"]["duplicate"] =
        ambiguous["services"]["datadog"]["hosts"]["replica"].clone();
    assert!(matches!(
        check().check(Some(&ambiguous), &state(json!(["host:pg-prod-replica-2"]))),
        Err(RecordCheckError::MissingEvidence(_))
    ));
}

#[test]
fn unqualified_expressions_require_adjudication_instead_of_guessing_meaning() {
    for scope in [
        json!(["host:pg-prod-replica-2 AND env:prod"]),
        json!(["!host:pg-prod-replica-1"]),
        json!(["host:pg-prod-replica-?"]),
    ] {
        assert!(matches!(
            check().check(Some(&initial()), &state(scope)),
            Err(RecordCheckError::NeedsAdjudication(_))
        ));
    }
}

#[test]
fn scope_and_context_shapes_fail_closed() {
    for scope in [json!("host:pg-prod-replica-2"), json!([1]), Value::Null] {
        assert!(matches!(
            check().check(Some(&initial()), &state(scope)),
            Err(RecordCheckError::Failed(_))
        ));
    }
    let mut malformed = initial();
    malformed["services"]["datadog"]["hosts"]["replica"]["tags_by_source"]["users"] =
        json!("env:prod");
    assert!(matches!(
        check().check(Some(&malformed), &state(json!(["host:pg-prod-replica-2"]))),
        Err(RecordCheckError::MissingEvidence(_))
    ));
}

#[test]
fn scope_literals_do_not_turn_regex_metacharacters_into_wildcards() {
    let mut original = initial();
    original["services"]["datadog"]["hosts"]["replica"]["tags_by_source"]["agent"] =
        json!(["host:db.prod"]);
    let mut goal = check();
    goal.tag_scope
        .as_mut()
        .unwrap()
        .equals
        .insert("host".into(), "db.prod".into());
    goal.check(Some(&original), &state(json!(["host:db.prod"])))
        .unwrap();
    assert!(matches!(
        goal.check(Some(&original), &state(json!(["host:dbXprod"]))),
        Err(RecordCheckError::Failed(_))
    ));
}

#[test]
fn contradictory_or_raw_spelling_constraints_are_invalid_authoring() {
    for (field, predicate) in [
        ("fields", json!({"/scope":["host:pg-prod-replica-2"]})),
        ("array_lengths", json!({"/scope":1})),
        ("fields", json!({"/scope/0":"host:pg-prod-replica-2"})),
        ("numeric_fields", json!({"/scope/0":1})),
    ] {
        let mut source = serde_json::to_value(check()).unwrap();
        source[field] = predicate;
        let goal: RecordCheck = serde_json::from_value(source).unwrap();
        assert!(goal.validate().is_err());
    }
    let mut source = serde_json::to_value(check()).unwrap();
    source["selector"]["matching"] = json!({"scope":["host:pg-prod-replica-2"]});
    let goal: RecordCheck = serde_json::from_value(source).unwrap();
    assert!(goal.validate().is_err());
}
