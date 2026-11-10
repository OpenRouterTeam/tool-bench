use std::path::Path;

use mock_core::{
    dataset::Dataset,
    events::{self, EventKind, OperationEvent, OperationKind},
    grading::{Expect, Grade, GradeInput, Outcome, grade},
    store::Store,
    world::{SharedWorld, WorldState, shared},
};
use serde_json::{Value, json};

const KNOWN: &str = "Shipped in release 2026.11.";
const UNREVIEWED: &str = "The refund retry story did not ship in release 2026.11.";

fn definition() -> Value {
    json!({
        "version": 2,
        "audit_history": true,
        "records": [{
            "selector": {"collection": "/services/pagerduty/notes", "matching": {"incident": "production"}, "created": true},
            "equals": 1,
            "fields": {"/kind": "note"},
            "reviewed_text": {"/body": {
                "requirement": "Say the refund retry story shipped in release 2026.11.",
                "accepted": [KNOWN]
            }}
        }],
        "preservation": {"allow": [{
            "pointer": "/services/pagerduty/notes/*",
            "kind": "create",
            "matching": {"author": "operator"}
        }]}
    })
}

struct Trial {
    world: SharedWorld,
    expect: Expect,
}

fn bound_history_definition() -> Value {
    let mut definition = definition();
    definition["preservation"]["allow"][0]["max_count"] = json!(2);
    definition["records"][0]["reviewed_text"]["/body"] = json!({
        "requirement": "Report shipment and reference the actual source identity without inventing a future identity.",
        "bound": {
            "identities": {"reference": {
                "selector": {"collection": "/services/pagerduty/notes", "matching": {"kind": "refund"}, "created": true},
                "field": "/reference"
            }},
            "examples": ["Shipped in release 2026.11. Reference: ${reference}."]
        }
    });
    definition
}

#[test]
fn later_source_identity_changes_do_not_retroactively_approve_an_earlier_claim() {
    let trial = Trial::new(bound_history_definition());
    for reference in ["re_before", "re_after"] {
        trial.put("source", json!({
            "id":"source", "incident":"finance", "author":"operator", "kind":"refund", "reference":reference
        }));
        if reference == "re_before" {
            trial.set(
                "actual",
                json!("Shipped in release 2026.11. Reference: re_after."),
            );
        }
    }
    let assessment = trial.assess(None);
    assert_eq!(assessment.outcome, Outcome::Unresolved, "{assessment:?}");
    assert!(assessment.environment_ok && assessment.adjudication_needed);
    assert!(
        assessment
            .failures
            .iter()
            .any(|failure| !failure.event_evidence.is_empty())
    );
}

#[test]
fn empty_drafts_do_not_require_source_identities_before_a_claim_is_written() {
    let trial = Trial::new(bound_history_definition());
    trial.set("actual", json!(""));
    trial.put("source", json!({
        "id":"source", "incident":"finance", "author":"operator", "kind":"refund", "reference":"re_actual"
    }));
    trial.set(
        "actual",
        json!("Shipped in release 2026.11. Reference: re_actual."),
    );
    let assessment = trial.assess(None);
    assert_eq!(assessment.outcome, Outcome::Pass, "{assessment:?}");
}

impl Trial {
    fn new(definition: Value) -> Self {
        let expect: Expect = serde_json::from_value(definition).unwrap();
        let directory =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/example/github/basic");
        let mut dataset = Dataset::load(&directory).unwrap();
        dataset.contract = expect.clone();
        dataset
            .services
            .insert("pagerduty".into(), Store::default());
        let world = shared(WorldState::from_dataset(dataset).unwrap());
        Self { world, expect }
    }

    fn put(&self, identity: &str, record: Value) {
        let operation = self
            .world
            .begin_operation(OperationKind::Http, "pagerduty", "tools", "POST", "/notes")
            .unwrap();
        events::sync_scope(Some(operation.context().clone()), || {
            self.world
                .lock()
                .unwrap()
                .service_mut("pagerduty")
                .store
                .collection_mut("notes")
                .insert(identity, record);
        });
        operation.finish(200, true);
    }

    fn set(&self, identity: &str, body: Value) {
        self.put(identity, json!({
            "id": identity, "incident": "production", "author": "operator", "kind": "note", "body": body
        }));
    }

    fn assess(&self, alternate_events: Option<&[OperationEvent]>) -> Grade {
        let world = self.world.lock().unwrap();
        grade(&GradeInput {
            expect: &self.expect,
            state: world.snapshot(),
            journal: world.journal.entries(),
            answer: None,
            tools_ips: vec!["tools".into()],
            context_overflow: false,
            startup: &[],
            initial_state: Some(&world.initial_snapshot()),
            events: Some(alternate_events.unwrap_or(world.events().entries())),
        })
    }
}

#[test]
fn an_unreviewed_committed_claim_cannot_be_erased_by_restoring_an_approved_final_record() {
    let trial = Trial::new(definition());
    for body in [KNOWN, UNREVIEWED, KNOWN] {
        trial.set("actual", json!(body));
    }
    let assessment = trial.assess(None);
    assert_eq!(assessment.outcome, Outcome::Unresolved, "{assessment:?}");
    assert!(assessment.environment_ok && assessment.adjudication_needed);
    let contradiction_commit = trial.world.lock().unwrap().events().entries().iter()
        .find(|event| matches!(&event.event, EventKind::StateCommitted { records }
            if records.iter().any(|change| change.after.as_ref().is_some_and(|record| record["body"] == UNREVIEWED))))
        .unwrap().seq;
    assert!(
        assessment
            .failures
            .iter()
            .any(|failure| failure.event_evidence.contains(&contradiction_commit)),
        "{assessment:?}"
    );
}

#[test]
fn final_selected_identity_is_checked_even_before_its_fields_match_the_final_selector() {
    let trial = Trial::new(definition());
    trial.put("thread/odd~identity", json!({
        "id": "thread/odd~identity", "incident": "draft", "author": "operator", "kind": "note", "body": UNREVIEWED
    }));
    trial.set("thread/odd~identity", json!(KNOWN));
    let assessment = trial.assess(None);
    assert_eq!(assessment.outcome, Outcome::Unresolved, "{assessment:?}");
    assert!(assessment.environment_ok);
}

#[test]
fn empty_intermediate_construction_does_not_force_one_create_call_or_a_premature_presence_check() {
    for empty in [Value::Null, json!(""), json!(" \n")] {
        let trial = Trial::new(definition());
        trial.set("actual", empty);
        trial.set("actual", json!(KNOWN));
        let assessment = trial.assess(None);
        assert_eq!(assessment.outcome, Outcome::Pass, "{assessment:?}");
    }
}

#[test]
fn harmless_metadata_refinements_and_reviewed_formatting_remain_valid() {
    let mut definition = definition();
    definition["records"][0]["reviewed_text"]["/body"]["format"] = json!("html");
    let trial = Trial::new(definition);
    trial.set(
        "actual",
        json!("<p>Shipped in release <strong>2026.11</strong>.</p>"),
    );
    trial.put(
        "actual",
        json!({
            "id":"actual", "incident":"production", "author":"operator", "kind":"note",
            "body":"<div>Shipped in release 2026.11.</div>", "metadata":{"release":"2026.11"}
        }),
    );
    assert_eq!(trial.assess(None).outcome, Outcome::Pass);
}

#[test]
fn unsupported_past_rendering_is_not_made_safe_by_replacing_the_final_html() {
    let mut definition = definition();
    definition["records"][0]["reviewed_text"]["/body"]["format"] = json!("html");
    let trial = Trial::new(definition);
    trial.set(
        "actual",
        json!("<div hidden>Shipped in release 2026.11.</div>"),
    );
    trial.set("actual", json!("<p>Shipped in release 2026.11.</p>"));
    let assessment = trial.assess(None);
    assert_eq!(assessment.outcome, Outcome::Unresolved, "{assessment:?}");
    assert!(assessment.environment_ok);
}

#[test]
fn literal_text_history_also_requires_review_without_inventing_an_ordinary_model_failure() {
    let mut definition = definition();
    definition["records"][0]
        .as_object_mut()
        .unwrap()
        .remove("reviewed_text");
    definition["records"][0]["html_text"] = json!({"/body": KNOWN});
    let trial = Trial::new(definition);
    trial.set("actual", json!("<p>A temporary draft needs review.</p>"));
    trial.set("actual", json!("<p>Shipped in release 2026.11.</p>"));
    let assessment = trial.assess(None);
    assert_eq!(assessment.outcome, Outcome::Unresolved, "{assessment:?}");
}

#[test]
fn a_proven_final_structural_failure_is_not_concealed_by_unreviewed_past_wording() {
    let trial = Trial::new(definition());
    trial.set("actual", json!(UNREVIEWED));
    trial.put("actual", json!({
        "id":"actual", "incident":"production", "author":"operator", "kind":"task", "body": KNOWN
    }));
    let assessment = trial.assess(None);
    assert_eq!(assessment.outcome, Outcome::Fail, "{assessment:?}");
    assert!(!assessment.adjudication_needed, "{assessment:?}");
}

#[test]
fn incomplete_history_is_an_environment_fault_not_semantic_adjudication() {
    let trial = Trial::new(definition());
    trial.set("actual", json!(UNREVIEWED));
    trial.set("actual", json!(KNOWN));
    let mut events = trial.world.lock().unwrap().events().entries().to_vec();
    let final_commit = events
        .iter()
        .rposition(|event| matches!(event.event, EventKind::StateCommitted { .. }))
        .unwrap();
    events.remove(final_commit);
    let assessment = trial.assess(Some(&events));
    assert_eq!(
        assessment.outcome,
        Outcome::EnvironmentError,
        "{assessment:?}"
    );
    assert!(!assessment.adjudication_needed, "{assessment:?}");
}

#[test]
fn historical_review_covers_complete_text_projections_not_just_the_final_summary() {
    let mut definition = definition();
    definition["records"][0]
        .as_object_mut()
        .unwrap()
        .remove("reviewed_text");
    definition["records"][0]["reviewed_fields"] = json!({
        "requirement": "Summary and body must truthfully report shipment in release 2026.11.",
        "accepted": [{"/summary": "Refund webhook retries shipped", "/body": KNOWN}]
    });
    let trial = Trial::new(definition);
    for body in [UNREVIEWED, KNOWN] {
        trial.put(
            "actual",
            json!({
                "id":"actual", "incident":"production", "author":"operator", "kind":"note",
                "summary":"Refund webhook retries shipped", "body":body
            }),
        );
    }
    let assessment = trial.assess(None);
    assert_eq!(assessment.outcome, Outcome::Unresolved, "{assessment:?}");
    assert!(assessment.environment_ok);
}

#[test]
fn empty_supported_html_is_construction_not_an_earlier_false_claim() {
    let mut definition = definition();
    definition["records"][0]["reviewed_text"]["/body"]["format"] = json!("html");
    let trial = Trial::new(definition);
    trial.set("actual", json!("<p> </p>"));
    trial.set("actual", json!("<p>Shipped in release 2026.11.</p>"));
    assert_eq!(trial.assess(None).outcome, Outcome::Pass);
}

#[test]
fn an_unsupported_intermediate_text_type_requires_review_not_an_invented_model_failure() {
    let trial = Trial::new(definition());
    trial.set("actual", json!(42));
    trial.set("actual", json!(KNOWN));
    let assessment = trial.assess(None);
    assert_eq!(assessment.outcome, Outcome::Unresolved, "{assessment:?}");
    assert!(assessment.environment_ok);
}

#[test]
fn literal_business_state_text_also_cannot_hide_a_replaced_committed_claim() {
    let mut definition = definition();
    definition["records"][0]
        .as_object_mut()
        .unwrap()
        .remove("reviewed_text");
    definition["state"] = json!([{
        "pointer": "/services/pagerduty/notes/actual/body", "text_equals": KNOWN
    }]);
    let trial = Trial::new(definition);
    for body in [KNOWN, UNREVIEWED, KNOWN] {
        trial.set("actual", json!(body));
    }
    let assessment = trial.assess(None);
    assert_eq!(assessment.outcome, Outcome::Unresolved, "{assessment:?}");
    assert!(
        assessment
            .failures
            .iter()
            .any(|failure| !failure.event_evidence.is_empty())
    );
}

#[test]
fn final_literal_state_mismatches_remain_known_failures() {
    let mut definition = definition();
    definition["records"][0]
        .as_object_mut()
        .unwrap()
        .remove("reviewed_text");
    definition["state"] = json!([{
        "pointer": "/services/pagerduty/notes/actual/body", "text_equals": KNOWN
    }]);
    let trial = Trial::new(definition);
    trial.set("actual", json!(UNREVIEWED));
    let assessment = trial.assess(None);
    assert_eq!(assessment.outcome, Outcome::Fail, "{assessment:?}");
    assert!(!assessment.adjudication_needed);
}

#[test]
fn nested_array_content_is_joined_by_actual_identity_across_root_record_commits() {
    let mut definition = definition();
    definition["records"][0]["selector"]["collection"] =
        json!("/services/pagerduty/notes/root/children");
    definition["records"][0]["selector"]["identity_field"] = json!("/id");
    let trial = Trial::new(definition);
    for body in [KNOWN, UNREVIEWED, KNOWN] {
        trial.put("root", json!({
            "author":"operator",
            "children":[{
                "id":"actual/child~identity", "incident":"production", "kind":"note", "body":body
            }]
        }));
    }
    let assessment = trial.assess(None);
    assert_eq!(assessment.outcome, Outcome::Unresolved, "{assessment:?}");
    assert!(assessment.environment_ok);
    assert!(
        assessment
            .failures
            .iter()
            .any(|failure| !failure.event_evidence.is_empty())
    );
}
