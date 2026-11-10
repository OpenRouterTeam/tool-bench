mod statement_support;

use mock_core::grading::Outcome;
use serde_json::json;
use statement_support::Case;

fn contract() -> Case {
    let mut case = Case::load("search/elasticsearch/latency-by-region");
    let facts = case
        .expect
        .answer
        .as_mut()
        .unwrap()
        .record_facts
        .as_mut()
        .unwrap();
    facts.facts.insert(
        "requests".into(),
        serde_json::from_value(json!({
            "field": "/count",
            "kind": "number",
            "optional": true,
            "units": ["requests"],
            "require_unit": true,
        }))
        .unwrap(),
    );
    facts
        .assertions
        .push("The selected region has ${requests}".into());
    case
}

#[test]
fn omitted_optional_facts_do_not_change_the_required_answer() {
    let case = contract();
    case.passes(&["eu-west-1 averages 412.5 ms."]);
    case.fails(&["The selected region has 6 requests."]);
    let facts = case
        .expect
        .answer
        .as_ref()
        .unwrap()
        .record_facts
        .as_ref()
        .unwrap();
    assert_eq!(
        facts
            .suggest_assertion("eu-west-1 averages 412.5 ms.", &case.initial)
            .unwrap(),
        "${region} averages ${average}.",
    );
}

#[test]
fn stated_optional_values_are_context_and_never_decide_the_verdict() {
    let case = contract();
    case.passes(&[
        "eu-west-1 averages 412.5 ms. The selected region has 6 requests.",
        // The request count was not asked for, so a wrong one does not cost the answer.
        "eu-west-1 averages 412.5 ms. The selected region has 5 requests.",
    ]);
}

#[test]
fn optional_sources_are_not_optional_environment_evidence() {
    let mut case = contract();
    case.expect
        .answer
        .as_mut()
        .unwrap()
        .record_facts
        .as_mut()
        .unwrap()
        .facts
        .get_mut("requests")
        .unwrap()
        .field = "/missing_optional_source".into();
    assert_eq!(
        case.outcome("eu-west-1 averages 412.5 ms."),
        Outcome::EnvironmentError,
    );
}

#[test]
fn optional_values_stay_context_when_their_source_changes() {
    let mut case = contract();
    case.initial["services"]["elasticsearch"]["docs"]["additional-request"] = json!({
        "_index": "requests-checkout-2026.10",
        "@timestamp": "2026-10-30T12:00:00Z",
        "region": "eu-west-1",
        "endpoint": "/checkout/submit",
        "status_code": 200,
        "duration_ms": 412.5,
    });
    case.passes(&[
        "eu-west-1 averages 412.5 ms.",
        "eu-west-1 averages 412.5 ms. The selected region has 7 requests.",
    ]);
}

#[test]
fn optional_facts_need_required_facts() {
    let mut case = contract();
    let facts = case
        .expect
        .answer
        .as_mut()
        .unwrap()
        .record_facts
        .as_mut()
        .unwrap();
    for fact in facts.facts.values_mut() {
        fact.optional = true;
    }
    assert!(case.expect.validate().is_err());
}

fn absence_contract() -> Case {
    let mut case = contract();
    case.initial["services"]["notes"] = json!({"incidents": {}});
    let facts = case
        .expect
        .answer
        .as_mut()
        .unwrap()
        .record_facts
        .as_mut()
        .unwrap();
    facts.related.insert(
        "incident".into(),
        serde_json::from_value(json!({
            "collection": "/services/notes/incidents",
            "filter": {"matching": {"/state": "active"}},
            "allow_absence": true,
        }))
        .unwrap(),
    );
    facts.facts.insert(
        "incident".into(),
        serde_json::from_value(json!({
            "record": "incident", "field": "/name", "kind": "text", "optional": true,
        }))
        .unwrap(),
    );
    facts
        .assertions
        .push("Related incident: ${incident}".into());
    case
}

#[test]
fn explicitly_absent_related_records_do_not_require_an_unasked_explanation() {
    let case = absence_contract();
    case.passes(&["eu-west-1 averages 412.5 ms."]);
}

#[test]
fn absent_related_records_do_not_hide_missing_or_ambiguous_source_evidence() {
    let mut case = absence_contract();
    case.initial["services"]["notes"] = json!({});
    assert_eq!(
        case.outcome("eu-west-1 averages 412.5 ms."),
        Outcome::EnvironmentError,
    );
    case.initial["services"]["notes"] = json!({"incidents": {
        "incident-1": {"state": "active"},
    }});
    assert_eq!(
        case.outcome("eu-west-1 averages 412.5 ms."),
        Outcome::EnvironmentError,
    );
    case.initial["services"]["notes"] = json!({"incidents": {
        "incident-1": {"state": "active", "name": "Checkout outage"},
        "incident-2": {"state": "active", "name": "Other outage"},
    }});
    assert_eq!(
        case.outcome("eu-west-1 averages 412.5 ms."),
        Outcome::EnvironmentError,
    );
}

#[test]
fn optional_related_records_may_be_stated_or_omitted() {
    let mut case = absence_contract();
    case.initial["services"]["notes"]["incidents"] = json!({
        "incident-1": {"state": "active", "name": "Checkout outage"},
    });
    case.passes(&["eu-west-1 averages 412.5 ms. Related incident: Checkout outage."]);
    case.initial["services"]["notes"]["incidents"]["incident-1"]["state"] = json!("resolved");
    case.passes(&["eu-west-1 averages 412.5 ms."]);
}

#[test]
fn absent_records_cannot_supply_required_facts_or_mandatory_descendants() {
    let mut case = absence_contract();
    let facts = case
        .expect
        .answer
        .as_mut()
        .unwrap()
        .record_facts
        .as_mut()
        .unwrap();
    facts.facts.get_mut("incident").unwrap().optional = false;
    assert!(case.expect.validate().is_err());

    let mut case = absence_contract();
    let facts = case
        .expect
        .answer
        .as_mut()
        .unwrap()
        .record_facts
        .as_mut()
        .unwrap();
    facts.related.insert(
        "owner".into(),
        serde_json::from_value(json!({
            "from": "incident", "collection": "/services/notes/users",
            "on": {"/id": "/owner"},
        }))
        .unwrap(),
    );
    assert!(case.expect.validate().is_err());
}

#[test]
fn optional_negative_context_cannot_erase_a_denial_of_a_required_fact() {
    let mut case = Case::load("search/tavily/webhook-signature-sunset");
    let pointer = "/services/tavily/pages/https:~1~1docs.ferrowave.dev~1guides~1webhooks~1verify-signatures/raw_content";
    let original = case.initial.pointer(pointer).unwrap().as_str().unwrap();
    let changed = original.replace("Ferrowave-Signature-V2", "X-Ferrowave-Sig");
    *case.initial.pointer_mut(pointer).unwrap() = json!(changed);
    assert_ne!(
        case.outcome("Date: January 15, 2027. Header: X-Ferrowave-Sig, not X-Ferrowave-Sig.",),
        Outcome::Pass,
    );
}

#[test]
fn optional_descendants_still_require_an_observed_source_collection() {
    let mut case = absence_contract();
    let facts = case
        .expect
        .answer
        .as_mut()
        .unwrap()
        .record_facts
        .as_mut()
        .unwrap();
    facts.related.insert(
        "owner".into(),
        serde_json::from_value(json!({
            "from": "incident", "collection": "/services/notes/users",
            "on": {"/id": "/owner"}, "allow_absence": true,
        }))
        .unwrap(),
    );
    assert_eq!(
        case.outcome("eu-west-1 averages 412.5 ms."),
        Outcome::EnvironmentError,
    );
}
