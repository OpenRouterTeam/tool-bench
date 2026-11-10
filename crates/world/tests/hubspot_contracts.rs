use std::path::Path;

use axum::{
    Router,
    body::{Body, to_bytes},
    http::Request,
};
use mock_core::{
    dataset::Dataset,
    evidence::WorldEvidence,
    grading::{Grade, GradeInput, Outcome, grade},
    serve::service_app,
    world::{SharedWorld, WorldState, shared},
};
use scenario::Scenario;
use serde_json::{Value, json};
use tower::ServiceExt;

const NOTE_TEXT: &str =
    "Call with Bill Lumbergh: security review passed; procurement needs a revised SOW by Nov 14.";

struct Trial {
    scenario: Scenario,
    world: SharedWorld,
    app: Router,
}

impl Trial {
    fn new(slug: &str) -> Self {
        Self::with_dataset(slug, |_| {})
    }

    fn with_dataset(slug: &str, prepare: impl FnOnce(&mut Dataset)) -> Self {
        let directory = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../scenarios/business/hubspot")
            .join(slug);
        let scenario = Scenario::load(&directory).unwrap();
        let mut dataset = Dataset::load(&directory).unwrap();
        prepare(&mut dataset);
        let world = shared(WorldState::from_dataset(dataset).unwrap());
        let app = service_app(world.clone(), &mock_hubspot::Hubspot);
        Self {
            scenario,
            world,
            app,
        }
    }

    async fn request(&self, method: &str, path: &str, body: Value) -> Value {
        let request = Request::builder()
            .method(method)
            .uri(path)
            .header("host", "world:8123")
            .header("authorization", "Bearer synthetic-contract-test")
            .header("content-type", "application/json")
            .body(if body.is_null() {
                Body::empty()
            } else {
                Body::from(body.to_string())
            })
            .unwrap();
        let response = self.app.clone().oneshot(request).await.unwrap();
        let status = response.status();
        let result: Value =
            serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap())
                .unwrap();
        assert!(status.is_success(), "{method} {path}: {status} {result}");
        assert!(result.get("errors").is_none(), "{result}");
        result
    }

    async fn update(&self, object_type: &str, identity: &str, properties: Value) {
        self.request(
            "POST",
            &format!("/crm/v3/objects/{object_type}/batch/update"),
            json!({"inputs": [{"id": identity, "properties": properties}]}),
        )
        .await;
    }

    async fn associate(
        &self,
        from_type: &str,
        from_id: &str,
        to_type: &str,
        to_id: &str,
        association_type: u64,
    ) {
        self.request(
            "POST",
            &format!("/crm/v4/associations/{from_type}/{to_type}/batch/create"),
            json!({"inputs": [{
                "from": {"id": from_id}, "to": {"id": to_id},
                "types": [{"associationCategory": "HUBSPOT_DEFINED", "associationTypeId": association_type}]
            }]}),
        )
        .await;
    }

    async fn note(&self, body: &str, deal: u64, related: bool) -> String {
        let mut associations = json!({"dealIds": [deal]});
        if related {
            associations["contactIds"] = json!([512005]);
            associations["companyIds"] = json!([18234002]);
        }
        let result = self
            .request(
                "POST",
                "/engagements/v1/engagements",
                json!({
                    "engagement": {"type": "NOTE", "timestamp": 1793282400000_i64},
                    "metadata": {"body": body},
                    "associations": associations,
                }),
            )
            .await;
        result["engagement"]["id"].as_u64().unwrap().to_string()
    }

    fn assess(&self) -> Grade {
        let world = self.world.lock().unwrap();
        let initial = world.initial_snapshot();
        grade(&GradeInput {
            expect: &self.scenario.expect,
            state: world.snapshot(),
            journal: world.journal.entries(),
            answer: None,
            tools_ips: vec!["unknown".into()],
            context_overflow: false,
            startup: &[],
            initial_state: Some(&initial),
            events: Some(world.events().entries()),
        })
    }
}

#[tokio::test]
async fn split_deal_updates_preserve_the_actual_initial_amount_instead_of_an_oracle_literal() {
    let trial = Trial::with_dataset("advance-globex-expansion", |dataset| {
        dataset
            .services
            .get_mut("hubspot")
            .unwrap()
            .collection_mut("deals")
            .get_mut("9100101")
            .unwrap()["properties"]["amount"] = json!("18000");
    });
    trial
        .update(
            "0-3",
            "9100101",
            json!({"closedate": "2026-11-20T18:30:00Z"}),
        )
        .await;
    trial
        .update("deals", "9100101", json!({"dealstage": "contractsent"}))
        .await;
    let result = trial.assess();
    assert_eq!(result.reward, 1.0, "{result:?}");
    assert!(result.preservation_checked && result.history_checked);
    let deal = trial
        .request("GET", "/crm/v3/objects/deals/9100101", Value::Null)
        .await;
    assert_eq!(deal["properties"]["amount"], "18000");
}

#[tokio::test]
async fn primary_and_unlabeled_company_links_are_valid_without_unrequested_contact_edits() {
    for primary in [true, false] {
        let trial = Trial::new("dana-joins-pied-piper");
        trial
            .associate(
                "contacts",
                "512001",
                "companies",
                "18234004",
                if primary { 1 } else { 279 },
            )
            .await;
        trial
            .update(
                "0-1",
                "512001",
                json!({"email": "dana.whitfield@piedpiper.com"}),
            )
            .await;
        let result = trial.assess();
        assert_eq!(result.reward, 1.0, "primary={primary}: {result:?}");
        assert!(result.preservation_checked && result.history_checked);
    }
}

#[tokio::test]
async fn actual_collateral_contact_edits_are_rejected_even_after_restoration() {
    let trial = Trial::new("dana-joins-pied-piper");
    trial
        .update(
            "contacts",
            "512001",
            json!({"email": "dana.whitfield@piedpiper.com"}),
        )
        .await;
    trial
        .associate("companies", "18234004", "contacts", "512001", 280)
        .await;
    for title in ["Chief Executive Officer", "Director of Data Platform"] {
        trial
            .update("contacts", "512001", json!({"jobtitle": title}))
            .await;
    }
    let result = trial.assess();
    assert_eq!(result.reward, 0.0, "{result:?}");
    assert!(result.state_ok && result.records_ok && result.environment_ok);
    assert!(!result.preservation_ok && result.history_checked);
}

#[tokio::test]
async fn an_unrelated_initial_note_does_not_pin_the_new_note_to_a_predicted_id() {
    let trial = Trial::with_dataset("log-call-note-initech", |dataset| {
        dataset
            .services
            .get_mut("hubspot")
            .unwrap()
            .collection_mut("notes")
            .insert(
                "80000000",
                json!({
                    "id": "80000000", "archived": false,
                    "properties": {"hs_note_body": "Unrelated historical account note"},
                    "createdAt": "2026-09-01T12:00:00.000Z",
                    "updatedAt": "2026-09-01T12:00:00.000Z",
                }),
            );
    });
    let identity = trial.note(NOTE_TEXT, 9100104, false).await;
    assert_ne!(identity, "61000005");
    let result = trial.assess();
    assert_eq!(result.reward, 1.0, "{result:?}");
    assert!(result.relations_ok && result.preservation_checked && result.history_checked);
}

#[tokio::test]
async fn exact_note_text_cannot_be_replaced_by_a_contradictory_keyword_match() {
    let trial = Trial::new("log-call-note-initech");
    trial
        .note(
            "This is incorrect: security review passed; procurement needs a revised SOW by Nov 14.",
            9100104,
            false,
        )
        .await;
    let result = trial.assess();
    assert_eq!(result.reward, 0.0, "{result:?}");
    assert!(result.environment_ok && result.contract_ok);
    assert!(!result.records_ok);
}

#[tokio::test]
async fn a_note_can_be_created_refined_and_linked_through_returned_ids() {
    let trial = Trial::new("log-call-note-initech");
    let created = trial
        .request(
            "POST",
            "/crm/v3/objects/notes/batch/create",
            json!({"inputs": [{"properties": {
                "hs_timestamp": "2026-10-29T14:00:00Z", "hs_note_body": ""
            }}]}),
        )
        .await;
    let identity = created["results"][0]["id"].as_str().unwrap();
    trial
        .update("notes", identity, json!({"hs_note_body": NOTE_TEXT}))
        .await;
    trial
        .associate("notes", identity, "deals", "9100104", 214)
        .await;
    let result = trial.assess();
    assert_eq!(result.reward, 1.0, "{result:?}");
    assert!(result.relations_ok && result.preservation_ok && result.history_checked);
}

#[tokio::test]
async fn relevant_optional_contact_and_company_links_must_reference_the_actual_new_note() {
    let valid = Trial::new("log-call-note-initech");
    valid.note(NOTE_TEXT, 9100104, true).await;
    let result = valid.assess();
    assert_eq!(result.reward, 1.0, "{result:?}");
    let wrong_reference = Trial::new("log-call-note-initech");
    wrong_reference.note(NOTE_TEXT, 9100104, false).await;
    wrong_reference
        .associate("contacts", "512005", "notes", "61000003", 201)
        .await;
    let result = wrong_reference.assess();
    assert_eq!(result.reward, 0.0, "{result:?}");
    assert!(result.records_ok && result.preservation_ok && result.environment_ok);
    assert!(!result.relations_ok);
}

#[test]
fn original_unsatisfied_tasks_reject_no_op_without_an_environment_fault() {
    for slug in [
        "advance-globex-expansion",
        "dana-joins-pied-piper",
        "log-call-note-initech",
    ] {
        let trial = Trial::new(slug);
        let result = trial.assess();
        assert_eq!(result.reward, 0.0, "{slug}: {result:?}");
        assert!(result.environment_ok && result.contract_ok, "{result:?}");
        assert!(result.history_checked && result.preservation_checked);
    }
}

#[tokio::test]
async fn correct_deal_outcomes_do_not_authorize_amount_changes_even_when_restored() {
    for restore in [false, true] {
        let trial = Trial::new("advance-globex-expansion");
        trial
            .update(
                "deals",
                "9100101",
                json!({"dealstage": "contractsent", "closedate": "2026-11-20"}),
            )
            .await;
        trial
            .update("deals", "9100101", json!({"amount": "96001"}))
            .await;
        if restore {
            trial
                .update("deals", "9100101", json!({"amount": "96000"}))
                .await;
        }
        let result = trial.assess();
        assert_eq!(result.reward, 0.0, "restore={restore}: {result:?}");
        assert!(result.state_ok && result.environment_ok);
        assert!(!result.preservation_ok && result.history_checked);
    }
}

#[tokio::test]
async fn incomplete_or_wrong_deal_updates_do_not_satisfy_the_requested_outcome() {
    for target in ["9100101", "9100102"] {
        let trial = Trial::new("advance-globex-expansion");
        trial
            .update("deals", target, json!({"dealstage": "contractsent"}))
            .await;
        let result = trial.assess();
        assert_eq!(result.reward, 0.0, "{result:?}");
        assert!(!result.state_ok && result.environment_ok);
    }
}

#[tokio::test]
async fn relevant_company_text_and_unchanged_properties_allow_harmless_retries() {
    let trial = Trial::new("dana-joins-pied-piper");
    for _ in 0..2 {
        trial
            .update(
                "contacts",
                "512001",
                json!({
                    "email": "dana.whitfield@piedpiper.com",
                    "company": "Pied Piper",
                    "jobtitle": "Director of Data Platform"
                }),
            )
            .await;
        trial
            .associate("companies", "18234004", "contacts", "512001", 2)
            .await;
    }
    let result = trial.assess();
    assert_eq!(result.reward, 1.0, "{result:?}");
}

#[test]
fn a_satisfied_contact_move_does_not_require_redundant_mutations() {
    let trial = Trial::with_dataset("dana-joins-pied-piper", |dataset| {
        let store = dataset.services.get_mut("hubspot").unwrap();
        store.collection_mut("contacts").get_mut("512001").unwrap()["properties"]["email"] =
            json!("dana.whitfield@piedpiper.com");
        store.collection_mut("associations").insert(
            "companies/18234004/contacts/512001",
            json!({
                "fromObjectType": "companies", "fromObjectId": "18234004",
                "toObjectType": "contacts", "toObjectId": "512001",
                "types": [{"associationCategory": "HUBSPOT_DEFINED", "associationTypeId": 280}]
            }),
        );
    });
    let result = trial.assess();
    assert_eq!(result.reward, 1.0, "{result:?}");
    assert!(result.preservation_checked && result.history_checked);
}

#[tokio::test]
async fn wrong_contact_company_and_partial_moves_are_distinguishable_from_success() {
    for variant in ["wrong_contact", "wrong_company", "email_only", "link_only"] {
        let trial = Trial::new("dana-joins-pied-piper");
        if variant != "link_only" {
            trial
                .update(
                    "contacts",
                    if variant == "wrong_contact" {
                        "512002"
                    } else {
                        "512001"
                    },
                    json!({"email": "dana.whitfield@piedpiper.com"}),
                )
                .await;
        }
        if variant != "email_only" {
            trial
                .associate(
                    "contacts",
                    if variant == "wrong_contact" {
                        "512002"
                    } else {
                        "512001"
                    },
                    "companies",
                    if variant == "wrong_company" {
                        "18234005"
                    } else {
                        "18234004"
                    },
                    279,
                )
                .await;
        }
        let result = trial.assess();
        assert_eq!(result.reward, 0.0, "{variant}: {result:?}");
        assert!(result.environment_ok && result.contract_ok);
    }
}

#[tokio::test]
async fn optional_company_updates_cannot_hide_an_unrelated_change_before_correction() {
    let trial = Trial::new("dana-joins-pied-piper");
    trial
        .update(
            "contacts",
            "512001",
            json!({"email": "dana.whitfield@piedpiper.com", "company": "Globex"}),
        )
        .await;
    trial
        .update("contacts", "512001", json!({"company": "Pied Piper"}))
        .await;
    trial
        .associate("contacts", "512001", "companies", "18234004", 279)
        .await;
    let result = trial.assess();
    assert_eq!(result.reward, 0.0, "{result:?}");
    assert!(result.state_ok && result.records_ok && result.environment_ok);
    assert!(!result.preservation_ok);
}

#[tokio::test]
async fn wrong_deal_duplicate_and_unlinked_notes_fail_for_business_reasons() {
    for variant in ["wrong_deal", "duplicate", "unlinked"] {
        let trial = Trial::new("log-call-note-initech");
        if variant == "unlinked" {
            trial
                .request(
                    "POST",
                    "/crm/v3/objects/notes/batch/create",
                    json!({"inputs": [{"properties": {
                        "hs_timestamp": "2026-10-29T14:00:00Z", "hs_note_body": NOTE_TEXT
                    }}]}),
                )
                .await;
        } else {
            trial
                .note(
                    NOTE_TEXT,
                    if variant == "wrong_deal" {
                        9100105
                    } else {
                        9100104
                    },
                    false,
                )
                .await;
            if variant == "duplicate" {
                trial.note(NOTE_TEXT, 9100104, false).await;
            }
        }
        let result = trial.assess();
        assert_eq!(result.reward, 0.0, "{variant}: {result:?}");
        assert!(!result.records_ok && result.environment_ok && result.contract_ok);
    }
}

#[tokio::test]
async fn an_extra_link_to_the_wrong_contact_is_not_an_authorized_side_effect() {
    let trial = Trial::new("log-call-note-initech");
    let identity = trial.note(NOTE_TEXT, 9100104, false).await;
    trial
        .associate("contacts", "512007", "notes", &identity, 201)
        .await;
    let result = trial.assess();
    assert_eq!(result.reward, 0.0, "{result:?}");
    assert!(result.records_ok && result.relations_ok && result.environment_ok);
    assert!(!result.preservation_ok);
}

#[tokio::test]
async fn equivalent_supported_rich_text_preserves_the_requested_note_content() {
    for body in [
        "<p>Call with <strong>Bill Lumbergh</strong>: security review passed;<br>procurement needs a revised SOW by Nov 14.</p>",
        "<div>Call with Bill Lumbergh: security review passed; procurement needs a revised SOW by Nov&nbsp;14.</div>",
        "<p>Call with Bill Lumbergh: security review passed;</p><p>procurement needs a revised SOW by Nov 14.</p>",
    ] {
        let trial = Trial::new("log-call-note-initech");
        trial.note(body, 9100104, false).await;
        let result = trial.assess();
        assert_eq!(result.reward, 1.0, "{body}: {result:?}");
    }
}

#[tokio::test]
async fn unsupported_rich_text_needs_adjudication_instead_of_becoming_a_model_failure() {
    let trial = Trial::new("log-call-note-initech");
    trial
        .note(
            &format!("<div style=\"display:none\">{NOTE_TEXT}</div>"),
            9100104,
            false,
        )
        .await;
    let result = trial.assess();
    assert_eq!(result.reward, 0.0);
    assert!(result.environment_ok && result.contract_ok && result.preservation_ok);
    let result = serde_json::to_value(result).unwrap();
    assert_eq!(result["outcome"], "unresolved");
    assert_eq!(result["adjudication_needed"], true);
}

#[tokio::test]
async fn rendering_uncertainty_cannot_hide_an_independently_proven_wrong_entity() {
    let trial = Trial::new("log-call-note-initech");
    trial
        .note(
            &format!("<div style=\"display:none\">{NOTE_TEXT}</div>"),
            9100105,
            false,
        )
        .await;
    let result = serde_json::to_value(trial.assess()).unwrap();
    assert_eq!(result["reward"], 0.0);
    assert_eq!(result["outcome"], "fail");
    assert_eq!(result["adjudication_needed"], true);
}

#[tokio::test]
async fn rendering_uncertainty_cannot_hide_interface_bypass_or_invalid_evidence() {
    let trial = Trial::new("log-call-note-initech");
    trial
        .note(
            &format!("<div style=\"display:none\">{NOTE_TEXT}</div>"),
            9100104,
            false,
        )
        .await;
    let mut world = trial.world.lock().unwrap();
    world.seal().unwrap();
    let mut evidence = WorldEvidence::capture(&world, &[], Ok(vec!["unknown".into()]));
    let unresolved = evidence.assess(&trial.scenario.expect, None, false);
    assert_eq!(unresolved.outcome, Outcome::Unresolved);
    evidence.tools_ips = vec!["192.0.2.40".into()];
    let bypassed = evidence.assess(&trial.scenario.expect, None, false);
    assert_eq!(bypassed.outcome, Outcome::IntegrityViolation);
    assert!(bypassed.adjudication_needed);
    evidence.evaluator_version = 0;
    let invalid = evidence.assess(&trial.scenario.expect, None, false);
    assert_eq!(invalid.outcome, Outcome::EnvironmentError);
    assert!(invalid.adjudication_needed);
}
