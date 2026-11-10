use std::path::Path;

use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use mock_core::{
    dataset::Dataset,
    grading::{Grade, GradeInput, Outcome, grade},
    serve::service_app,
    world::{SharedWorld, WorldState, shared},
};
use scenario::Scenario;
use serde_json::{Value, json};
use tower::ServiceExt;

const SUMMARY: &str = "Apple Pay button missing at checkout on iOS 18 Safari";
const DESCRIPTION: &str =
    "The Apple Pay button does not render on the checkout payment step in Safari on iOS 18.";

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
            .join("../../scenarios/collab/jira")
            .join(slug);
        let scenario = Scenario::load(&directory).unwrap();
        let mut dataset = Dataset::load(&directory).unwrap();
        prepare(&mut dataset);
        let world = shared(WorldState::from_dataset(dataset).unwrap());
        let app = service_app(world.clone(), &mock_jira::Jira);
        Self {
            scenario,
            world,
            app,
        }
    }

    async fn request(&self, method: &str, path: &str, body: Value) -> (StatusCode, Value) {
        let request = Request::builder()
            .method(method)
            .uri(path)
            .header("host", "world:8128")
            .header("authorization", "Bearer synthetic-jira-contract")
            .header("content-type", "application/json")
            .body(Body::from(if body.is_null() {
                String::new()
            } else {
                body.to_string()
            }))
            .unwrap();
        let response = self.app.clone().oneshot(request).await.unwrap();
        let status = response.status();
        let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let result = if bytes.is_empty() {
            Value::Null
        } else {
            serde_json::from_slice(&bytes).unwrap()
        };
        (status, result)
    }

    async fn call(&self, method: &str, path: &str, body: Value) -> Value {
        let (status, result) = self.request(method, path, body).await;
        assert!(status.is_success(), "{method} {path}: {status} {result}");
        result
    }

    async fn edit(&self, key: &str, fields: Value) {
        self.call(
            "PUT",
            &format!("/rest/api/2/issue/{key}"),
            json!({"fields": fields}),
        )
        .await;
    }

    async fn create(&self, extra: Value) -> String {
        let mut fields = json!({
            "project": {"key": "WEB"}, "issuetype": {"name": "Bug"},
            "summary": SUMMARY, "description": DESCRIPTION,
            "priority": {"name": "High"}, "assignee": {"name": "alex.kim"},
            "components": [{"name": "Checkout"}], "labels": ["ios"],
        });
        fields
            .as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        let result = self
            .call("POST", "/rest/api/2/issue", json!({"fields": fields}))
            .await;
        result["key"].as_str().unwrap().to_owned()
    }

    async fn transition(&self, key: &str, transition: &str, fields: Value) {
        self.call(
            "POST",
            &format!("/rest/api/2/issue/{key}/transitions"),
            json!({"transition": {"id": transition}, "fields": fields}),
        )
        .await;
    }

    async fn comment(&self, key: &str, body: &str) -> String {
        let result = self
            .call(
                "POST",
                &format!("/rest/api/2/issue/{key}/comment"),
                json!({"body": body}),
            )
            .await;
        result["id"].as_str().unwrap().to_owned()
    }

    async fn assign(&self) {
        self.call(
            "GET",
            "/wiki/rest/api/content/100002?expand=body.storage",
            Value::Null,
        )
        .await;
        self.call(
            "PUT",
            "/rest/api/2/issue/PAY-124/assignee",
            json!({"name": "lena.fischer"}),
        )
        .await;
        self.transition("PAY-124", "11", json!({})).await;
    }

    async fn ship(&self) {
        self.transition("PAY-121", "31", json!({"resolution": {"name": "Done"}}))
            .await;
        self.comment("PAY-121", "Shipped in release 2026.11.").await;
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
        self.world.lock().unwrap().snapshot()["services"]["jira"].clone()
    }
}

fn passed(assessment: &Grade) {
    assert_eq!(assessment.outcome, Outcome::Pass, "{assessment:?}");
    assert!(
        assessment.preservation_checked && assessment.history_checked,
        "{assessment:?}"
    );
}

fn failed(assessment: &Grade) {
    assert_eq!(assessment.outcome, Outcome::Fail, "{assessment:?}");
    assert!(
        assessment.environment_ok && assessment.contract_ok && assessment.integrity_ok,
        "{assessment:?}"
    );
}

#[tokio::test]
async fn all_three_reviewed_plans_have_preservation_and_no_op_controls() {
    for slug in [
        "assign-go-no-go-review",
        "file-apple-pay-bug",
        "ship-refund-retries",
    ] {
        let trial = Trial::new(slug);
        failed(&trial.assess());
        match slug {
            "assign-go-no-go-review" => trial.assign().await,
            "file-apple-pay-bug" => {
                trial.create(json!({})).await;
            }
            _ => trial.ship().await,
        }
        passed(&trial.assess());
    }
}

#[tokio::test]
async fn created_issue_and_comment_id_streams_can_shift() {
    let trial = Trial::with_dataset("file-apple-pay-bug", |dataset| {
        let issues = dataset
            .services
            .get_mut("jira")
            .unwrap()
            .collection_mut("issues");
        let mut issue = issues.get("WEB-55").unwrap().clone();
        issue["key"] = json!("WEB-88");
        issue["id"] = json!("99000");
        issues.insert("WEB-88", issue);
    });
    assert_eq!(trial.create(json!({})).await, "WEB-89");
    passed(&trial.assess());
    let trial = Trial::with_dataset("ship-refund-retries", |dataset| {
        let comments = dataset
            .services
            .get_mut("jira")
            .unwrap()
            .collection_mut("comments");
        let mut comment = comments.get("10043").unwrap().clone();
        comment["id"] = json!("99000");
        comments.insert("99000", comment);
    });
    trial.ship().await;
    assert!(trial.snapshot()["comments"].get("99001").is_some());
    passed(&trial.assess());
}

#[tokio::test]
async fn newly_created_issue_fields_can_be_refined_without_touching_existing_issues() {
    let trial = Trial::new("file-apple-pay-bug");
    let key = trial.create(json!({"priority": {"name": "Medium"}, "assignee": null, "components": [], "labels": []})).await;
    for fields in [
        json!({"priority": {"id": "2"}, "assignee": {"name": "alex.kim@octo-corp.example"}}),
        json!({"components": [{"id": "10010"}], "labels": ["ios"]}),
    ] {
        trial.edit(&key, fields).await;
    }
    passed(&trial.assess());
}

#[tokio::test]
async fn bug_facts_can_live_in_the_summary_or_across_summary_and_description() {
    for fields in [
        json!({"description": null}),
        json!({"summary": "Missing Apple Pay button", "description": "At checkout in Safari on iOS 18, the Apple Pay button is missing."}),
    ] {
        let trial = Trial::new("file-apple-pay-bug");
        trial.create(fields).await;
        passed(&trial.assess());
    }
}

#[tokio::test]
async fn bug_facts_pass_in_unlisted_wording_and_fail_as_keyword_fragments_or_a_wrong_platform() {
    let trial = Trial::new("file-apple-pay-bug");
    trial
        .create(json!({"summary": "Apple Pay unavailable", "description": "Checkout in iOS 18 Safari does not offer the Apple Pay button."}))
        .await;
    passed(&trial.assess());
    for fields in [
        json!({"summary": "Apple Pay", "description": null}),
        json!({"summary": "Apple Pay button missing at checkout", "description": "The button is not shown on the payment step."}),
        json!({"summary": "Apple Pay button missing at checkout on iOS 17 Safari", "description": null}),
        json!({"summary": "Google Pay button missing at checkout on iOS 18 Safari", "description": null}),
    ] {
        let trial = Trial::new("file-apple-pay-bug");
        trial.create(fields).await;
        failed(&trial.assess());
    }
}

#[tokio::test]
async fn the_wrong_assignee_classification_component_or_project_is_a_known_failure() {
    for fields in [
        json!({"assignee": {"name": "alex.kimball"}}),
        json!({"issuetype": {"name": "Task"}}),
        json!({"priority": {"name": "Low"}}),
        json!({"labels": ["ios18"]}),
        json!({"components": [{"name": "Catalog"}]}),
    ] {
        let trial = Trial::new("file-apple-pay-bug");
        trial.create(fields).await;
        failed(&trial.assess());
    }
}

#[tokio::test]
async fn a_second_created_issue_and_restored_existing_issue_changes_are_rejected() {
    let trial = Trial::new("file-apple-pay-bug");
    trial.create(json!({})).await;
    trial.create(json!({})).await;
    failed(&trial.assess());
    let trial = Trial::new("file-apple-pay-bug");
    trial.create(json!({})).await;
    for priority in ["Low", "Medium"] {
        trial
            .edit("WEB-53", json!({"priority": {"name": priority}}))
            .await;
    }
    let assessment = trial.assess();
    failed(&assessment);
    assert!(!assessment.history_ok);
}

#[tokio::test]
async fn assignment_accepts_a_coincident_transition_and_email_identity_alias() {
    let trial = Trial::new("assign-go-no-go-review");
    trial
        .transition(
            "PAY-124",
            "11",
            json!({"assignee": {"name": "lena.fischer@octo-corp.example"}}),
        )
        .await;
    passed(&trial.assess());
}

#[tokio::test]
async fn progression_before_assignment_is_not_repaired_by_later_assignment() {
    let trial = Trial::new("assign-go-no-go-review");
    trial.transition("PAY-124", "11", json!({})).await;
    trial
        .edit("PAY-124", json!({"assignee": {"name": "lena.fischer"}}))
        .await;
    let assessment = trial.assess();
    failed(&assessment);
    assert!(!assessment.ordering_ok);
}

#[tokio::test]
async fn assignment_does_not_authorize_an_archived_owner_collateral_priority_or_page_edits() {
    let trial = Trial::new("assign-go-no-go-review");
    trial
        .transition(
            "PAY-124",
            "11",
            json!({"assignee": {"name": "marcus.webb"}}),
        )
        .await;
    failed(&trial.assess());
    let trial = Trial::new("assign-go-no-go-review");
    trial.assign().await;
    for priority in ["Low", "High"] {
        trial
            .edit("PAY-124", json!({"priority": {"name": priority}}))
            .await;
    }
    failed(&trial.assess());
    let trial = Trial::new("assign-go-no-go-review");
    trial.assign().await;
    trial
        .call(
            "PUT",
            "/wiki/rest/api/content/100002",
            json!({
                "type": "page", "title": "Payments v2 launch plan", "version": {"number": 5},
                "body": {"storage": {"value": "Changed plan", "representation": "storage"}}
            }),
        )
        .await;
    failed(&trial.assess());
}

#[tokio::test]
async fn already_satisfied_assignment_does_not_force_a_redundant_write_or_read_pattern() {
    let trial = Trial::with_dataset("assign-go-no-go-review", |dataset| {
        let issue = dataset
            .services
            .get_mut("jira")
            .unwrap()
            .collection_mut("issues")
            .get_mut("PAY-124")
            .unwrap();
        issue["fields"]["assignee"] = json!("lena.fischer");
        issue["fields"]["status"] = json!("In Progress");
    });
    passed(&trial.assess());
}

#[tokio::test]
async fn shipment_comment_can_precede_or_share_the_transition() {
    let trial = Trial::new("ship-refund-retries");
    trial
        .comment(
            "PAY-121",
            "The refund webhook retry story shipped in release 2026.11.",
        )
        .await;
    trial.transition("PAY-121", "31", json!({})).await;
    passed(&trial.assess());
    let trial = Trial::new("ship-refund-retries");
    trial
        .call(
            "POST",
            "/rest/api/2/issue/PAY-121/transitions",
            json!({
                "transition": {"id": "31"},
                "update": {"comment": [{"add": {"body": "Shipped in release 2026.11."}}]}
            }),
        )
        .await;
    passed(&trial.assess());
}

#[tokio::test]
async fn shipment_requires_a_new_comment_on_the_correct_issue_and_correct_resolution() {
    for (key, resolution) in [("PAY-122", "Done"), ("PAY-121", "Won't Do")] {
        let trial = Trial::new("ship-refund-retries");
        trial
            .transition("PAY-121", "31", json!({"resolution": {"name": resolution}}))
            .await;
        trial.comment(key, "Shipped in release 2026.11.").await;
        failed(&trial.assess());
    }
    let trial = Trial::with_dataset("ship-refund-retries", |dataset| {
        dataset
            .services
            .get_mut("jira")
            .unwrap()
            .collection_mut("comments")
            .get_mut("10042")
            .unwrap()["body"] = json!("Shipped in release 2026.11.");
    });
    trial.transition("PAY-121", "31", json!({})).await;
    failed(&trial.assess());
}

#[tokio::test]
async fn shipment_comment_passes_in_unlisted_wording_and_fails_without_the_release_or_the_claim() {
    let trial = Trial::new("ship-refund-retries");
    trial.transition("PAY-121", "31", json!({})).await;
    trial
        .comment(
            "PAY-121",
            "Refund webhook retries went live in release 2026.11.",
        )
        .await;
    passed(&trial.assess());
    for comment in [
        "2026.11",
        "Shipped in release 2026.10.",
        "Refund webhook retries went live with the November 2026 release.",
    ] {
        let trial = Trial::new("ship-refund-retries");
        trial.transition("PAY-121", "31", json!({})).await;
        trial.comment("PAY-121", comment).await;
        failed(&trial.assess());
    }
}

#[tokio::test]
async fn shipment_cannot_hide_restored_description_changes_or_extra_comments() {
    let trial = Trial::new("ship-refund-retries");
    let original = trial.snapshot()["issues"]["PAY-121"]["fields"]["description"].clone();
    trial.ship().await;
    trial
        .edit(
            "PAY-121",
            json!({"description": "Changed without permission"}),
        )
        .await;
    trial
        .edit("PAY-121", json!({"description": original}))
        .await;
    let assessment = trial.assess();
    failed(&assessment);
    assert!(!assessment.history_ok);
    let trial = Trial::new("ship-refund-retries");
    trial.ship().await;
    trial
        .comment("PAY-121", "Shipped in release 2026.11.")
        .await;
    failed(&trial.assess());
}

#[tokio::test]
async fn invalid_compound_edits_and_transitions_do_not_publish_partial_comments_or_status() {
    for (method, path, body) in [
        (
            "PUT",
            "/rest/api/2/issue/PAY-121",
            json!({"update": {
                "comment": [{"add": {"body": "Must not be committed"}}],
                "priority": [{"set": {"name": "Invalid priority"}}]
            }}),
        ),
        (
            "POST",
            "/rest/api/2/issue/PAY-121/transitions",
            json!({
                "transition": {"id": "31"}, "update": {
                    "comment": [{"add": {"body": "Must not be committed"}}],
                    "priority": [{"set": {"name": "Invalid priority"}}]
                }
            }),
        ),
    ] {
        let trial = Trial::new("ship-refund-retries");
        let before = trial.snapshot();
        let (status, _) = trial.request(method, path, body).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(trial.snapshot(), before);
    }
}

#[tokio::test]
async fn invalid_creation_updates_cannot_create_an_issue_or_comment_with_a_success_response() {
    let trial = Trial::new("file-apple-pay-bug");
    let before = trial.snapshot();
    let (status, _) = trial.request("POST", "/rest/api/2/issue", json!({
        "fields": {"project": {"key": "WEB"}, "summary": SUMMARY, "issuetype": {"name": "Bug"}},
        "update": {
            "comment": [{"add": {"body": "Must not be committed"}}],
            "priority": [{"set": {"name": "Invalid priority"}}]
        }
    })).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(trial.snapshot(), before);
}

#[tokio::test]
async fn readonly_workflow_and_timestamp_fields_cannot_be_fabricated_by_an_edit() {
    for fields in [
        json!({"status": "Done"}),
        json!({"resolutiondate": "2026-10-29T15:00:00.000+0000"}),
        json!({"created": "2020-01-01T00:00:00.000+0000"}),
        json!({"updated": "2020-01-01T00:00:00.000+0000"}),
        json!({"creator": "marcus.webb"}),
    ] {
        let trial = Trial::new("ship-refund-retries");
        let before = trial.snapshot();
        let (status, _) = trial
            .request(
                "PUT",
                "/rest/api/2/issue/PAY-121",
                json!({"fields": fields}),
            )
            .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{fields}");
        assert_eq!(trial.snapshot(), before);
    }
}

#[tokio::test]
async fn bulk_creation_keeps_successful_items_but_never_publishes_a_failed_items_effects() {
    let trial = Trial::new("file-apple-pay-bug");
    let mut valid = json!({
        "fields": {"project": {"key": "WEB"}, "summary": SUMMARY, "issuetype": {"name": "Bug"}}
    });
    let mut invalid = valid.clone();
    invalid["update"] = json!({
        "comment": [{"add": {"body": "Must not be committed"}}],
        "priority": [{"set": {"name": "Invalid priority"}}]
    });
    valid["fields"]["description"] = json!(DESCRIPTION);
    let before = trial.snapshot();
    let (status, result) = trial
        .request(
            "POST",
            "/rest/api/2/issue/bulk",
            json!({"issueUpdates": [valid, invalid]}),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(result["issues"].as_array().unwrap().len(), 1);
    assert_eq!(result["errors"].as_array().unwrap().len(), 1);
    let after = trial.snapshot();
    assert_eq!(
        after["issues"].as_object().unwrap().len(),
        before["issues"].as_object().unwrap().len() + 1
    );
    assert_eq!(after["comments"], before["comments"]);
}

#[tokio::test]
async fn a_reworded_shipment_comment_is_judged_by_its_final_text() {
    let trial = Trial::new("ship-refund-retries");
    trial.transition("PAY-121", "31", json!({})).await;
    let identity = trial
        .comment("PAY-121", "Shipped in release 2026.11.")
        .await;
    let rewrite = async |body: &str| {
        trial
            .call(
                "PUT",
                &format!("/rest/api/2/issue/PAY-121/comment/{identity}"),
                json!({"body": body}),
            )
            .await;
    };
    // Left without the release, the comment no longer says what was asked.
    rewrite("This story has shipped.").await;
    let assessment = trial.assess();
    failed(&assessment);
    assert!(!assessment.adjudication_needed, "{assessment:?}");
    assert!(
        assessment.failures.iter().any(|failure| failure.check
            == mock_core::grading::CheckKind::Records
            && failure.reason.contains("does not mention")),
        "{assessment:?}"
    );
    rewrite("Shipped in release 2026.11.").await;
    assert_eq!(
        trial.snapshot()["comments"][&identity]["body"],
        "Shipped in release 2026.11."
    );
    // An interim wording that lacked the release is a draft, not a committed wrong claim.
    let assessment = trial.assess();
    passed(&assessment);
    assert!(assessment.history_checked && !assessment.adjudication_needed);
}
