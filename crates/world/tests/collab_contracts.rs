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
use std::path::Path;
use tower::ServiceExt;

const ALEX: &str = "0bfae6ae-01d0-58d1-a611-7d1973102c45";
const WEB: &str = "6da75b4c-0e7c-5ac7-a415-f09a129582b1";
const PROJECT: &str = "665e6e05-9585-5351-a871-913d9e5b298f";
const BUG: &str = "75dbca3d-6368-502e-8e56-0c82837ea691";
const SAFARI: &str = "b528a42a-7d96-511c-ab34-1b8cb0bf0c8d";
const CYCLE: &str = "627fb4e1-47ac-567a-99cd-3e236709e3b0";
const PROGRESS: &str = "41688e9c-05a3-5fce-be65-9c7049e13037";

struct Trial {
    scenario: Scenario,
    world: SharedWorld,
    app: Router,
    module: String,
}
impl Trial {
    fn new(module: &str, slug: &str) -> Self {
        Self::with_dataset(module, slug, |_| {})
    }
    fn with_dataset(module: &str, slug: &str, prepare: impl FnOnce(&mut Dataset)) -> Self {
        let directory = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../scenarios/collab")
            .join(module)
            .join(slug);
        let scenario = Scenario::load(&directory).unwrap();
        let mut dataset = Dataset::load(&directory).unwrap();
        prepare(&mut dataset);
        let world = shared(WorldState::from_dataset(dataset).unwrap());
        let app = match module {
            "linear" => service_app(world.clone(), &mock_linear::Linear),
            "slack" => service_app(world.clone(), &mock_slack::Slack),
            _ => panic!("unsupported module"),
        };
        Self {
            scenario,
            world,
            app,
            module: module.to_owned(),
        }
    }
    async fn result(&self, name: &str, arguments: Value) -> Value {
        let request = Request::post("/_mcp/tools/call")
            .header("content-type", "application/json")
            .body(Body::from(
                json!({"name":name,"arguments":arguments}).to_string(),
            ))
            .unwrap();
        let response = self.app.clone().oneshot(request).await.unwrap();
        assert!(response.status().is_success());
        serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap()
    }
    async fn call(&self, name: &str, arguments: Value) -> Value {
        let result = self.result(name, arguments).await;
        assert_ne!(result["isError"], true, "{result}");
        result
    }
    async fn oracle(&self) {
        let mut bindings = Bindings::new();
        for call in &self.scenario.solution.calls {
            let result = self
                .call(&call.tool, call.resolve_arguments(&bindings).unwrap())
                .await;
            call.capture_result(&result, &mut bindings).unwrap();
        }
    }
    fn state(&self) -> Value {
        self.world.lock().unwrap().snapshot()["services"][&self.module].clone()
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
    async fn create_issue(&self, extra: Value) -> String {
        let mut arguments = json!({
            "title":"Saved carts tab reloads in a loop on Safari 17","teamId":WEB,
            "description":"On Safari 17.1 (macOS 14), opening the Saved carts tab makes the page reload in a loop. Chrome and Firefox are fine. It started with the 4.12 release candidate.",
            "priority":2,"labelIds":[BUG,SAFARI],"projectId":PROJECT,"assigneeId":ALEX
        });
        arguments
            .as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        let result = self.call("linear_createIssue", arguments).await;
        let decoded: Value =
            serde_json::from_str(result["content"][0]["text"].as_str().unwrap()).unwrap();
        decoded["id"].as_str().unwrap().to_owned()
    }
    async fn update_issue(&self, id: &str, fields: Value) {
        let mut arguments = fields;
        arguments["id"] = json!(id);
        self.call("linear_updateIssue", arguments).await;
    }
    async fn escalate(&self, body: &str) {
        self.update_issue("WEB-51", json!({"priority":1,"assigneeId":ALEX}))
            .await;
        self.call(
            "linear_createComment",
            json!({"issueId":"WEB-51","body":body}),
        )
        .await;
    }
    async fn send(&self, channel: &str, thread: Option<&str>, text: &str) -> Value {
        let mut arguments = json!({"channel_id":channel,"message":text});
        if let Some(thread) = thread {
            arguments["thread_ts"] = json!(thread);
        }
        self.call("slack_send_message", arguments).await
    }
    async fn checklist(&self, columns: Value) -> Value {
        self.call(
            "slack_add_list_record",
            json!({"list_id":"F0PAYV2LST1","columns":columns}),
        )
        .await
    }
}
fn pass(grade: &Grade) {
    assert_eq!(grade.outcome, Outcome::Pass, "{grade:?}");
    assert!(grade.history_checked && grade.preservation_checked);
}
fn fail(grade: &Grade) {
    assert_eq!(grade.outcome, Outcome::Fail, "{grade:?}");
    assert!(grade.environment_ok && grade.contract_ok && grade.integrity_ok);
}

#[tokio::test]
async fn all_six_reference_plans_preserve_business_state_and_reject_no_op() {
    for (module, slug) in [
        ("linear", "escalate-right-safari-issue"),
        ("linear", "file-safari-crash-bug"),
        ("linear", "pull-issue-into-cycle"),
        ("slack", "cutover-thread-summary"),
        ("slack", "launch-checklist-item"),
        ("slack", "release-date-dm-designer"),
    ] {
        let trial = Trial::new(module, slug);
        fail(&trial.assess());
        trial.oracle().await;
        pass(&trial.assess());
    }
}

#[tokio::test]
async fn linear_creation_accepts_shifted_identifiers_and_follow_up_classification() {
    let trial = Trial::with_dataset("linear", "file-safari-crash-bug", |dataset| {
        let issues = dataset
            .services
            .get_mut("linear")
            .unwrap()
            .collection_mut("issues");
        let mut issue = issues.get("WEB-57").unwrap().clone();
        issue["number"] = json!(99);
        issue["identifier"] = json!("WEB-99");
        issue["id"] = json!("da357bcd-8321-4b61-aa4b-9a0b92312b00");
        issues.insert("WEB-99", issue);
    });
    let actual = trial
        .create_issue(json!({"priority":0,"labelIds":[]}))
        .await;
    trial
        .update_issue(&actual, json!({"priority":2,"addedLabelIds":[SAFARI,BUG]}))
        .await;
    pass(&trial.assess());
    assert!(trial.state()["issues"].get("WEB-100").is_some());
}

#[tokio::test]
async fn linear_required_details_pass_in_unlisted_wording_and_are_not_replaced_by_keywords() {
    let trial = Trial::new("linear", "file-safari-crash-bug");
    trial
        .create_issue(json!({
            "description":"On Safari 17.1 (macOS 14), Saved carts loops after 4.12 RC; Chrome and Firefox work normally."
        }))
        .await;
    pass(&trial.assess());
    for description in [
        "Safari",
        "Safari works correctly; Chrome reloads.",
        "On Safari 16.4 (macOS 13), Saved carts loops after 4.12 RC; Chrome and Firefox work normally.",
        "On Safari 17.1 (macOS 14), Saved carts loops after 4.11 RC; Chrome and Firefox work normally.",
    ] {
        let trial = Trial::new("linear", "file-safari-crash-bug");
        trial.create_issue(json!({"description":description})).await;
        fail(&trial.assess());
    }
}

#[tokio::test]
async fn linear_wrong_classification_duplicate_creation_and_restored_other_issue_edits_fail() {
    for extra in [
        json!({"priority":1}),
        json!({"assigneeId":"a4176131-9edb-57a6-9830-0638d2d2fa29"}),
        json!({"labelIds":[BUG]}),
    ] {
        let trial = Trial::new("linear", "file-safari-crash-bug");
        trial.create_issue(extra).await;
        fail(&trial.assess());
    }
    let trial = Trial::new("linear", "file-safari-crash-bug");
    trial.create_issue(json!({})).await;
    trial.create_issue(json!({})).await;
    fail(&trial.assess());
    let trial = Trial::new("linear", "file-safari-crash-bug");
    trial.create_issue(json!({})).await;
    for priority in [1, 3] {
        trial
            .update_issue("WEB-51", json!({"priority":priority}))
            .await;
    }
    fail(&trial.assess());
}

#[tokio::test]
async fn escalation_uses_new_comment_identity_and_complete_customer_claim() {
    let trial = Trial::with_dataset("linear", "escalate-right-safari-issue", |dataset| {
        let comments = dataset
            .services
            .get_mut("linear")
            .unwrap()
            .collection_mut("comments");
        let mut comment = comments.get("WEB-51#1").unwrap().clone();
        comment["id"] = json!("75c77546-dff5-4cd7-84ad-d1e8f6754431");
        comments.insert("WEB-51#2", comment);
    });
    trial
        .escalate("Acme Retail escalated this Safari checkout issue.")
        .await;
    pass(&trial.assess());
    let trial = Trial::new("linear", "escalate-right-safari-issue");
    trial
        .escalate("Escalation received from the Acme Retail team.")
        .await;
    pass(&trial.assess());
    for body in [
        "Acme",
        "Acme Retail",
        "Acme Corp escalated this issue.",
        "A customer escalated this issue.",
    ] {
        let trial = Trial::new("linear", "escalate-right-safari-issue");
        trial.escalate(body).await;
        fail(&trial.assess());
    }
}

#[tokio::test]
async fn escalation_does_not_authorize_other_tickets_or_optional_field_changes() {
    let trial = Trial::new("linear", "escalate-right-safari-issue");
    trial
        .escalate("Acme Retail escalated this Safari checkout issue.")
        .await;
    trial.update_issue("PAY-97", json!({"priority":1})).await;
    fail(&trial.assess());
    let trial = Trial::new("linear", "escalate-right-safari-issue");
    trial
        .escalate("Acme Retail escalated this Safari checkout issue.")
        .await;
    for estimate in [1, 0] {
        trial
            .update_issue("WEB-51", json!({"estimate":estimate}))
            .await;
    }
    fail(&trial.assess());
}

#[tokio::test]
async fn cycle_membership_and_progression_allow_combined_updates_and_retries() {
    let trial = Trial::new("linear", "pull-issue-into-cycle");
    trial
        .update_issue("PAY-121", json!({"cycleId":CYCLE,"stateId":PROGRESS}))
        .await;
    trial
        .update_issue("PAY-121", json!({"stateId":PROGRESS}))
        .await;
    pass(&trial.assess());
    let trial = Trial::new("linear", "pull-issue-into-cycle");
    trial.oracle().await;
    trial.update_issue("PAY-120", json!({"priority":1})).await;
    fail(&trial.assess());
}

#[tokio::test]
async fn invalid_linear_compound_updates_do_not_publish_partial_history() {
    let trial = Trial::new("linear", "pull-issue-into-cycle");
    let before = trial.state();
    let result=trial.result("linear_updateIssue",json!({"id":"PAY-121","stateId":PROGRESS,"cycleId":"00000000-0000-4000-8000-000000000000"})).await;
    assert_eq!(result["isError"], true);
    assert_eq!(trial.state(), before);
}

#[tokio::test]
async fn new_linear_issues_cannot_use_another_teams_cycle_or_workflow_state() {
    for extra in [json!({"stateId":PROGRESS}), json!({"cycleId":CYCLE})] {
        let trial = Trial::new("linear", "file-safari-crash-bug");
        let before = trial.state();
        let mut arguments = json!({"title":"New issue","teamId":WEB});
        arguments
            .as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        let result = trial.result("linear_createIssue", arguments).await;
        assert_eq!(result["isError"], true, "{result}");
        assert_eq!(trial.state(), before);
    }
}

#[tokio::test]
async fn restored_existing_checklist_edits_are_not_hidden_by_a_correct_new_row() {
    let trial = Trial::new("slack", "launch-checklist-item");
    trial.oracle().await;
    for title in ["Unrequested edit", "Load test checkout at 3x peak"] {
        trial.call("slack_update_list_record",json!({
            "list_id":"F0PAYV2LST1","record_id":"Rec0PV2A0001","updated_columns":{"Task":title}
        })).await;
    }
    let assessment = trial.assess();
    fail(&assessment);
    assert!(!assessment.history_ok);
}

#[tokio::test]
async fn opening_the_requested_dm_is_a_derived_effect_not_an_unrelated_channel_write() {
    let trial = Trial::with_dataset("slack", "release-date-dm-designer", |dataset| {
        dataset
            .services
            .get_mut("slack")
            .unwrap()
            .collection_mut("channels")
            .remove("D09KIMDM4X1");
    });
    trial
        .send(
            "U06AKIM3D2Q",
            None,
            "The web 4.12 release is scheduled for 2026-11-10.",
        )
        .await;
    pass(&trial.assess());
}

#[tokio::test]
async fn invalid_new_dm_requests_do_not_create_a_conversation_as_a_partial_effect() {
    for extra in [json!({"message":""}), json!({"thread_ts":"123.000001"})] {
        let trial = Trial::with_dataset("slack", "release-date-dm-designer", |dataset| {
            dataset
                .services
                .get_mut("slack")
                .unwrap()
                .collection_mut("channels")
                .remove("D09KIMDM4X1");
        });
        let before = trial.state();
        let mut arguments = json!({"channel_id":"U06AKIM3D2Q","message":"Hello"});
        arguments
            .as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        let result = trial.result("slack_send_message", arguments).await;
        assert_eq!(result["isError"], true, "{result}");
        assert_eq!(trial.state(), before);
    }
}

#[tokio::test]
async fn slack_thread_summary_requires_the_complete_final_window_and_lead() {
    for text in [
        "Agreed cutover: 2026-11-07 22:00 to 2026-11-08 01:00 Central time. Sam Ortiz leads.",
        "Summary: 2026-11-07, 22:00-01:00 CT; Sam is the cutover lead.",
    ] {
        let trial = Trial::new("slack", "cutover-thread-summary");
        trial
            .send("C05DBMIG2RT", Some("1792600205.318801"), text)
            .await;
        pass(&trial.assess());
    }
    let trial = Trial::new("slack", "cutover-thread-summary");
    trial
        .send(
            "C05DBMIG2RT",
            Some("1792600205.318801"),
            "Final window is 2026-11-07 from 22:00 to 01:00 CT, and Sam is leading.",
        )
        .await;
    pass(&trial.assess());
    for text in [
        "2026-11-07 22:00",
        "Agreed: 2026-11-07 22:00-01:00 CT. Priya leads.",
        "Agreed: Friday 2026-11-06, 20:00-23:00 CT. Sam leads.",
        "The window starts at 22:00 Central on November 7 and ends at 01:00; Sam leads.",
    ] {
        let trial = Trial::new("slack", "cutover-thread-summary");
        trial
            .send("C05DBMIG2RT", Some("1792600205.318801"), text)
            .await;
        fail(&trial.assess());
    }
}

#[tokio::test]
async fn slack_thread_destinations_and_extra_posts_are_protected() {
    for (channel, thread) in [("C05DBMIG2RT", None), ("C01GENERAL0", None)] {
        let trial = Trial::new("slack", "cutover-thread-summary");
        trial
            .send(
                channel,
                thread,
                "Summary: 2026-11-07, 22:00-01:00 CT; Sam is the cutover lead.",
            )
            .await;
        fail(&trial.assess());
    }
    let trial = Trial::new("slack", "cutover-thread-summary");
    trial.oracle().await;
    trial.send("C01GENERAL0", None, "Unrequested post").await;
    fail(&trial.assess());
}

#[tokio::test]
async fn slack_dm_accepts_user_or_conversation_identity_and_any_wording_with_the_date() {
    for destination in ["U06AKIM3D2Q", "D09KIMDM4X1"] {
        let trial = Trial::new("slack", "release-date-dm-designer");
        trial
            .send(
                destination,
                None,
                "The web 4.12 release is scheduled for 2026-11-10.",
            )
            .await;
        pass(&trial.assess());
    }
    let trial = Trial::new("slack", "release-date-dm-designer");
    trial
        .send(
            "U06AKIM3D2Q",
            None,
            "Web 4.12 will be released on 2026-11-10.",
        )
        .await;
    pass(&trial.assess());
    for text in [
        "Web 4.12 will be released on Tuesday, November 10.",
        "Web 4.12 will be released on 2026-11-03.",
        "The web 4.12 release moved because of the payments SDK regression.",
    ] {
        let trial = Trial::new("slack", "release-date-dm-designer");
        trial.send("U06AKIM3D2Q", None, text).await;
        fail(&trial.assess());
    }
}

#[tokio::test]
async fn slack_dm_wrong_people_and_channel_posts_fail_even_with_correct_date() {
    for destination in ["U04ARIVERA9", "C06RELEASE1"] {
        let trial = Trial::new("slack", "release-date-dm-designer");
        trial
            .send(
                destination,
                None,
                "The web 4.12 release is scheduled for 2026-11-10.",
            )
            .await;
        fail(&trial.assess());
    }
}

#[tokio::test]
async fn checklist_requires_a_new_row_and_preserves_existing_rows_and_columns() {
    let trial = Trial::new("slack", "launch-checklist-item");
    trial.oracle().await;
    pass(&trial.assess());
    trial.call("slack_update_list_record",json!({"list_id":"F0PAYV2LST1","record_id":"Rec0PV2A0001","updated_columns":{"Task":"Changed without permission"}})).await;
    fail(&trial.assess());
}

#[tokio::test]
async fn checklist_creation_can_be_refined_and_ids_can_shift() {
    let trial = Trial::with_dataset("slack", "launch-checklist-item", |dataset| {
        dataset
            .services
            .get_mut("slack")
            .unwrap()
            .collection_mut("lists")
            .get_mut("F0PAYV2LST1")
            .unwrap()["next_rec"] = json!(100);
    });
    let before = trial.state();
    trial
        .checklist(json!({"Task":"Rotate webhook signing secrets"}))
        .await;
    let after = trial.state();
    let rows = after["lists"]["F0PAYV2LST1"]["records"].as_array().unwrap();
    let identity = rows.last().unwrap()["id"].as_str().unwrap();
    assert!(
        !before["lists"]["F0PAYV2LST1"]["records"]
            .as_array()
            .unwrap()
            .iter()
            .any(|row| row["id"] == identity)
    );
    trial.call("slack_update_list_record",json!({
        "list_id":"F0PAYV2LST1","record_id":identity,
        "updated_columns":{"Owner":"U03PRIYAN4T","Status":"In progress","Due date":"2026-11-05"}
    })).await;
    pass(&trial.assess());
}

#[tokio::test]
async fn checklist_wrong_values_and_duplicate_rows_do_not_pass() {
    for columns in [
        json!({"Task":"Rotate webhook signing secrets","Owner":"U05SORTIZ6M","Status":"In progress","Due date":"2026-11-05"}),
        json!({"Task":"Rotate webhook signing secrets","Owner":"U03PRIYAN4T","Status":"Done","Due date":"2026-11-05"}),
        json!({"Task":"Rotate webhook signing secrets","Owner":"U03PRIYAN4T","Status":"In progress","Due date":"2026-11-06"}),
    ] {
        let trial = Trial::new("slack", "launch-checklist-item");
        trial.checklist(columns).await;
        fail(&trial.assess());
    }
    let trial = Trial::new("slack", "launch-checklist-item");
    trial.oracle().await;
    trial
        .checklist(json!({"Task":"Other unrequested row"}))
        .await;
    fail(&trial.assess());
}

#[tokio::test]
async fn slack_visible_caller_identity_is_ordinary_not_a_benchmark_marker() {
    let trial = Trial::new("slack", "cutover-thread-summary");
    let profile = trial
        .call("slack_read_user_profile", json!({}))
        .await
        .to_string()
        .to_lowercase();
    assert!(
        !profile.contains("toolbnch") && !profile.contains("toolbench"),
        "{profile}"
    );
}
