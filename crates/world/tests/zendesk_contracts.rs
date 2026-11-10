use std::path::Path;

use axum::{
    Router,
    body::{Body, to_bytes},
    http::Request,
};
use mock_core::{
    dataset::Dataset,
    grading::{Grade, GradeInput, Outcome, grade},
    serve::service_app,
    world::{SharedWorld, WorldState, shared},
};
use scenario::{Scenario, solution::Bindings};
use serde_json::{Value, json};
use tower::ServiceExt;

const DESCRIPTION: &str = "Elena Petrova (Fjord Outfitters) called in: since this morning's app update, the barcode scanners in their Bergen warehouse have stopped syncing inventory counts.";
const ESCALATION: &str = "Escalated to on-call engineering under incident INC-4471. The 502s are blocking patient check-ins at Northwind Health.";
const REPLY: &str = "Marcus, billing has issued the refund for the duplicate October charge. Reference RF-88213; it should reach your card within 5-7 business days.";

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
            .join("../../scenarios/business/zendesk")
            .join(slug);
        let scenario = Scenario::load(&directory).unwrap();
        let mut dataset = Dataset::load(&directory).unwrap();
        prepare(&mut dataset);
        let world = shared(WorldState::from_dataset(dataset).unwrap());
        let app = service_app(world.clone(), &mock_zendesk::Zendesk);
        Self {
            scenario,
            world,
            app,
        }
    }

    async fn call(&self, tool: &str, arguments: Value) -> Value {
        let request = Request::post("/_mcp/tools/call")
            .header("content-type", "application/json")
            .body(Body::from(
                json!({"name": tool, "arguments": arguments}).to_string(),
            ))
            .unwrap();
        let response = self.app.clone().oneshot(request).await.unwrap();
        assert!(response.status().is_success());
        let result: Value =
            serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap())
                .unwrap();
        let text = result["content"][0]["text"].as_str().unwrap();
        assert!(!text.starts_with("Error:"), "{tool}: {text}");
        result
    }

    async fn update(&self, ticket_id: u64, fields: Value) {
        let mut arguments = fields;
        arguments["ticket_id"] = json!(ticket_id);
        self.call("update_ticket", arguments).await;
    }

    async fn comment(&self, ticket_id: u64, public: bool, comment: &str) {
        self.call(
            "create_ticket_comment",
            json!({"ticket_id": ticket_id, "public": public, "comment": comment}),
        )
        .await;
    }

    async fn create(&self, extra: Value) -> u64 {
        let mut arguments = json!({
            "subject": "Barcode scanners not syncing after update",
            "description": DESCRIPTION,
            "requester_id": 9010301,
            "priority": "high",
            "type": "incident",
        });
        arguments
            .as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        let result = self.call("create_ticket", arguments).await;
        let ticket: Value =
            serde_json::from_str(result["content"][0]["text"].as_str().unwrap()).unwrap();
        ticket["ticket"]["id"].as_u64().unwrap()
    }

    async fn escalate(&self) {
        self.update(
            6230,
            json!({"priority": "urgent", "tags": ["api", "patient_portal", "escalated"]}),
        )
        .await;
        self.comment(6230, false, ESCALATION).await;
    }

    async fn refund(&self) {
        self.comment(4217, true, REPLY).await;
        self.update(4217, json!({"status": "solved"})).await;
    }

    async fn oracle(&self) {
        let mut bindings = Bindings::new();
        for call in &self.scenario.solution.calls {
            assert_eq!(call.endpoint, "zendesk");
            let result = self
                .call(&call.tool, call.resolve_arguments(&bindings).unwrap())
                .await;
            call.capture_result(&result, &mut bindings).unwrap();
        }
    }

    fn assess(&self) -> Grade {
        let world = self.world.lock().unwrap();
        grade(&GradeInput {
            expect: &self.scenario.expect,
            state: world.snapshot(),
            journal: world.journal.entries(),
            answer: None,
            tools_ips: vec!["unknown".into()],
            context_overflow: false,
            startup: &[],
            initial_state: Some(&world.initial_snapshot()),
            events: Some(world.events().entries()),
        })
    }

    fn snapshot(&self) -> Value {
        self.world.lock().unwrap().snapshot()
    }
}

fn passed(result: &Grade) {
    assert_eq!(result.outcome, Outcome::Pass, "{result:?}");
    assert_eq!(result.reward, 1.0);
    assert!(
        result.preservation_checked && result.history_checked,
        "{result:?}"
    );
}

fn failed(result: &Grade) {
    assert_eq!(result.outcome, Outcome::Fail, "{result:?}");
    assert_eq!(result.reward, 0.0);
    assert!(
        result.environment_ok && result.contract_ok && result.integrity_ok,
        "{result:?}"
    );
}

fn unresolved(result: &Grade) {
    assert_eq!(result.outcome, Outcome::Unresolved, "{result:?}");
    assert!(
        result.adjudication_needed && result.environment_ok,
        "{result:?}"
    );
}

#[tokio::test]
async fn all_three_reference_workflows_have_checked_preservation_and_no_op_controls() {
    for slug in [
        "create-ticket-for-caller",
        "escalate-outage-ticket",
        "reply-refund-and-solve",
    ] {
        let trial = Trial::new(slug);
        failed(&trial.assess());
        trial.oracle().await;
        passed(&trial.assess());
    }
}

#[tokio::test]
async fn new_ticket_identity_can_shift_and_is_bound_to_its_comment_and_audits() {
    let trial = Trial::with_dataset("create-ticket-for-caller", |dataset| {
        let tickets = dataset
            .services
            .get_mut("zendesk")
            .unwrap()
            .collection_mut("tickets");
        let mut ticket = tickets.get("5108").unwrap().clone();
        ticket["id"] = json!(8100);
        tickets.insert("8100", ticket);
    });
    let actual = trial.create(json!({})).await;
    assert_eq!(actual, 8101);
    passed(&trial.assess());
    let snapshot = trial.snapshot();
    let comment = &snapshot["services"]["zendesk"]["comments"][format!("{actual}/1")];
    assert_eq!(comment["ticket_id"], actual);
    let audit = &snapshot["services"]["zendesk"]["audits"]
        [comment["audit_id"].as_u64().unwrap().to_string()];
    assert_eq!(audit["ticket_id"], actual);
}

#[tokio::test]
async fn a_new_ticket_can_be_refined_and_harmlessly_retried_without_matching_the_oracle_plan() {
    let trial = Trial::new("create-ticket-for-caller");
    let actual = trial
        .create(json!({"priority": "normal", "type": "question", "subject": "Scanner incident"}))
        .await;
    for fields in [
        json!({"subject": "Barcode scanners not syncing after update"}),
        json!({"priority": "high"}),
        json!({"type": "incident"}),
        json!({"priority": "high"}),
    ] {
        trial.update(actual, fields).await;
    }
    passed(&trial.assess());
}

#[tokio::test]
async fn creating_for_the_wrong_requester_or_creating_duplicate_tickets_is_not_success() {
    for requester_id in [9010001, 9010302, 9010303] {
        let trial = Trial::new("create-ticket-for-caller");
        trial.create(json!({"requester_id": requester_id})).await;
        failed(&trial.assess());
    }
    let trial = Trial::new("create-ticket-for-caller");
    trial.create(json!({})).await;
    trial.create(json!({})).await;
    failed(&trial.assess());
}

#[tokio::test]
async fn a_description_needs_the_site_the_scanners_the_sync_failure_and_the_update() {
    let trial = Trial::new("create-ticket-for-caller");
    trial
        .create(
            json!({"description": "Bergen scanners stopped syncing counts after today's update."}),
        )
        .await;
    passed(&trial.assess());
    for description in [
        "Bergen",
        "The barcode scanners in the warehouse stopped syncing inventory counts after this morning's app update.",
        "The Stavanger scanners stopped syncing inventory counts after this morning's app update.",
        "Elena reports a problem at the Bergen warehouse since this morning's app update.",
    ] {
        let trial = Trial::new("create-ticket-for-caller");
        trial.create(json!({"description": description})).await;
        failed(&trial.assess());
    }
}

#[tokio::test]
async fn existing_ticket_changes_are_rejected_even_when_restored_after_creation() {
    let trial = Trial::new("create-ticket-for-caller");
    trial.create(json!({})).await;
    for priority in ["urgent", "low"] {
        trial.update(5108, json!({"priority": priority})).await;
    }
    let result = trial.assess();
    failed(&result);
    assert!(!result.history_ok);
}

#[tokio::test]
async fn an_extra_comment_or_temporary_resolution_is_not_authorized_ticket_creation() {
    let trial = Trial::new("create-ticket-for-caller");
    let actual = trial.create(json!({})).await;
    trial
        .comment(actual, false, "An unrelated internal note.")
        .await;
    failed(&trial.assess());
    let trial = Trial::new("create-ticket-for-caller");
    let actual = trial.create(json!({})).await;
    for status in ["solved", "open"] {
        trial.update(actual, json!({"status": status})).await;
    }
    failed(&trial.assess());
}

#[tokio::test]
async fn escalation_allows_split_updates_tag_reordering_and_harmless_retries() {
    let trial = Trial::new("escalate-outage-ticket");
    trial.comment(6230, false, "Escalated to **on-call engineering** under incident INC-4471. The 502s are blocking patient check-ins at Northwind Health.").await;
    trial
        .update(
            6230,
            json!({"tags": ["escalated", "patient_portal", "api", "api"]}),
        )
        .await;
    for _attempt in 0..2 {
        trial.update(6230, json!({"priority": "urgent"})).await;
    }
    passed(&trial.assess());
}

#[tokio::test]
async fn escalation_cannot_remove_or_invent_tags_even_if_the_original_set_is_restored() {
    for tags in [
        json!(["escalated"]),
        json!(["api", "patient_portal", "escalated", "billing"]),
    ] {
        let trial = Trial::new("escalate-outage-ticket");
        trial.escalate().await;
        trial.update(6230, json!({"tags": tags})).await;
        trial
            .update(
                6230,
                json!({"tags": ["api", "patient_portal", "escalated"]}),
            )
            .await;
        let result = trial.assess();
        failed(&result);
        assert!(!result.history_ok);
    }
}

#[tokio::test]
async fn escalation_does_not_authorize_reassignment_or_a_different_customer_audit() {
    let trial = Trial::new("escalate-outage-ticket");
    trial.escalate().await;
    trial.update(6230, json!({"assignee_id": 9010001})).await;
    failed(&trial.assess());
    let trial = Trial::new("escalate-outage-ticket");
    trial.escalate().await;
    trial.update(6233, json!({"priority": "normal"})).await;
    failed(&trial.assess());
}

#[tokio::test]
async fn an_escalation_note_needs_the_escalation_to_on_call_and_the_incident_reference() {
    for (note, truthful) in [
        (
            "I escalated this outage to the engineering on-call, incident INC-4471.",
            true,
        ),
        ("INC-4471", false),
        ("Escalated to on-call engineering.", false),
        (
            "Escalated to on-call engineering under incident INC-4417.",
            false,
        ),
    ] {
        let trial = Trial::new("escalate-outage-ticket");
        trial
            .update(
                6230,
                json!({"priority": "urgent", "tags": ["api", "patient_portal", "escalated"]}),
            )
            .await;
        trial.comment(6230, false, note).await;
        if truthful {
            passed(&trial.assess());
        } else {
            failed(&trial.assess());
        }
    }
}

#[tokio::test]
async fn escalation_must_be_a_new_private_note_on_the_correct_ticket() {
    for (ticket_id, public) in [(6230, true), (6225, false), (6232, false), (6233, false)] {
        let trial = Trial::new("escalate-outage-ticket");
        trial
            .update(
                6230,
                json!({"priority": "urgent", "tags": ["api", "patient_portal", "escalated"]}),
            )
            .await;
        trial.comment(ticket_id, public, ESCALATION).await;
        failed(&trial.assess());
    }
}

#[tokio::test]
async fn old_matching_comments_cannot_substitute_for_a_requested_new_note() {
    let trial = Trial::with_dataset("escalate-outage-ticket", |dataset| {
        let comments = dataset
            .services
            .get_mut("zendesk")
            .unwrap()
            .collection_mut("comments");
        let comment = comments.get_mut("6230/2").unwrap();
        comment["body"] = json!(ESCALATION);
        comment["public"] = json!(false);
    });
    trial
        .update(
            6230,
            json!({"priority": "urgent", "tags": ["api", "patient_portal", "escalated"]}),
        )
        .await;
    failed(&trial.assess());
}

#[tokio::test]
async fn public_refund_replies_allow_reviewed_paraphrases_formatting_and_solve_retries() {
    let trial = Trial::new("reply-refund-and-solve");
    trial.comment(4217, true, "Marcus, billing has issued the refund for the duplicate October charge. Reference **RF-88213**; it should reach your card within 5-7 business days.").await;
    for _attempt in 0..2 {
        trial.update(4217, json!({"status": "solved"})).await;
    }
    passed(&trial.assess());
}

#[tokio::test]
async fn a_combined_atomic_public_reply_and_solve_is_a_valid_business_plan() {
    let trial = Trial::new("reply-refund-and-solve");
    trial
        .update(
            4217,
            json!({"comment": {"body": REPLY, "public": true}, "status": "solved"}),
        )
        .await;
    passed(&trial.assess());
}

#[tokio::test]
async fn solving_before_replying_is_not_repaired_by_reopening_and_solving_again() {
    let trial = Trial::new("reply-refund-and-solve");
    trial.update(4217, json!({"status": "solved"})).await;
    trial.update(4217, json!({"status": "open"})).await;
    trial.refund().await;
    let result = trial.assess();
    failed(&result);
    assert!(!result.ordering_ok);
}

#[tokio::test]
async fn a_refund_reply_needs_the_reference_and_the_arrival_estimate() {
    for (reply, truthful) in [
        (
            "Your duplicate October payment has been returned; reference RF-88213. Allow five to seven business days for your card.",
            true,
        ),
        ("RF-88213", false),
        (
            "Marcus, billing has issued the refund for the duplicate October charge. Reference RF-88213.",
            false,
        ),
        (
            "Marcus, billing has issued the refund for the duplicate October charge. It should reach your card within 5-7 business days.",
            false,
        ),
        (
            "Marcus, billing has issued the refund for the duplicate October charge. Reference RF-87002; it should reach your card within 5-7 business days.",
            false,
        ),
    ] {
        let trial = Trial::new("reply-refund-and-solve");
        trial.comment(4217, true, reply).await;
        trial.update(4217, json!({"status": "solved"})).await;
        if truthful {
            passed(&trial.assess());
        } else {
            failed(&trial.assess());
        }
    }
}

#[tokio::test]
async fn a_private_refund_reply_or_a_reply_on_another_ticket_is_a_known_failure() {
    for (ticket_id, public) in [
        (4217, false),
        (4201, true),
        (4219, true),
        (4220, true),
        (4215, true),
    ] {
        let trial = Trial::new("reply-refund-and-solve");
        trial.comment(ticket_id, public, REPLY).await;
        trial.update(4217, json!({"status": "solved"})).await;
        failed(&trial.assess());
    }
}

#[tokio::test]
async fn hidden_html_cannot_pass_using_the_mock_plain_text_extraction() {
    let trial = Trial::new("reply-refund-and-solve");
    trial
        .update(
            4217,
            json!({"comment": {"html_body": format!("<p hidden>{REPLY}</p>"), "public": true}}),
        )
        .await;
    trial.update(4217, json!({"status": "solved"})).await;
    unresolved(&trial.assess());
}

#[tokio::test]
async fn refund_success_does_not_authorize_priority_changes_or_extra_comments() {
    let trial = Trial::new("reply-refund-and-solve");
    trial.refund().await;
    for priority in ["low", "high"] {
        trial.update(4217, json!({"priority": priority})).await;
    }
    let result = trial.assess();
    failed(&result);
    assert!(!result.history_ok);
    let trial = Trial::new("reply-refund-and-solve");
    trial.refund().await;
    trial.comment(4217, true, REPLY).await;
    failed(&trial.assess());
}
