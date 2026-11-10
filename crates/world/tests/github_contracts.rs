mod support;

use axum::http::StatusCode;
use mock_core::{
    dataset::Dataset,
    grading::{Grade, Outcome},
};
use serde_json::{Value, json};
use support::ContractTrial as Trial;

const TITLE: &str = "Login page returns 500";
const BODY: &str = "The login page has returned HTTP 500 since deploy 412.";
const PULL: &str = "/api/v3/repos/octo-org/web/pulls/57";
const ISSUE: &str = "/api/v3/repos/octo-org/payments-api/issues/12";
const HEAD: &str = "6ea035e584d56c79cc252b6a34973828261a108b";
const BASE: &str = "ec9101404d2eae359a8f7a7bd61afcfc080f77b1";

impl Trial {
    fn new(slug: &str) -> Self {
        Self::with_dataset(slug, |_| {})
    }

    fn with_dataset(slug: &str, prepare: impl FnOnce(&mut Dataset)) -> Self {
        Self::load_with_dataset(
            &format!("code/github/{slug}"),
            &mock_github::GitHub,
            &[],
            prepare,
        )
    }

    async fn create(&self, body: &str) -> u64 {
        self.call(
            "POST",
            "/api/v3/repos/octo-org/app/issues",
            json!({"title":TITLE,"body":body}),
        )
        .await["number"]
            .as_u64()
            .unwrap()
    }

    async fn triage(&self) {
        self.call(
            "PATCH",
            ISSUE,
            json!({"labels":["bug","customer-reported","sev-1"],"assignees":["sam"]}),
        )
        .await;
    }

    async fn approve(&self, body: Option<&str>, number: u64) -> u64 {
        let path = format!("/api/v3/repos/octo-org/web/pulls/{number}");
        let pull = self.call("GET", &path, Value::Null).await;
        let mut input = json!({"pullRequestId":pull["node_id"],"event":"APPROVE"});
        if let Some(body) = body {
            input["body"] = json!(body);
        }
        self.call("POST", "/api/graphql", json!({
            "query":"mutation($input: AddPullRequestReviewInput!) { addPullRequestReview(input:$input) { pullRequestReview { id state } } }",
            "variables":{"input":input}
        })).await;
        self.call("GET", &format!("{path}/reviews"), Value::Null)
            .await
            .as_array()
            .unwrap()
            .iter()
            .find(|review| review["user"]["login"] == "mona")
            .unwrap()["id"]
            .as_u64()
            .unwrap()
    }

    async fn merge(&self, extra: Value) -> Value {
        let mut body = json!({"merge_method":"squash"});
        body.as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        self.call("PUT", &format!("{PULL}/merge"), body).await
    }
}

fn pass(result: &Grade) {
    assert_eq!(result.outcome, Outcome::Pass, "{result:?}");
    assert!(result.preservation_checked && result.history_checked);
}

fn fail(result: &Grade) {
    assert_eq!(result.outcome, Outcome::Fail, "{result:?}");
    assert!(result.environment_ok && result.contract_ok && result.integrity_ok);
}

fn unresolved(result: &Grade) {
    assert_eq!(result.outcome, Outcome::Unresolved, "{result:?}");
    assert!(result.environment_ok && result.adjudication_needed);
}

#[tokio::test]
async fn reviewed_github_workflows_require_outcomes_and_authoritative_preservation() {
    for slug in [
        "file-login-bug",
        "triage-duplicate-charge",
        "approve-merge-dependency-bump",
    ] {
        let trial = Trial::new(slug);
        fail(&trial.assess());
        match slug {
            "file-login-bug" => {
                trial.create(BODY).await;
            }
            "triage-duplicate-charge" => trial.triage().await,
            _ => {
                trial.approve(None, 57).await;
                trial.merge(json!({})).await;
            }
        }
        pass(&trial.assess());
    }
}

#[tokio::test]
async fn issue_creation_accepts_shifted_ids_and_follow_up_description_completion() {
    let trial = Trial::with_dataset("file-login-bug", |dataset| {
        let issues = dataset
            .services
            .get_mut("github")
            .unwrap()
            .collection_mut("issues");
        let mut other = issues.get("octo-org/app#3").unwrap().clone();
        other["id"] = json!(2090);
        other["number"] = json!(90);
        other["title"] = json!("Unrelated accessibility follow-up");
        other["node_id"] = json!("I_shifted_fixture");
        issues.insert("octo-org/app#90", other);
    });
    let number = trial.create("").await;
    assert_eq!(number, 91);
    trial
        .call(
            "PATCH",
            &format!("/api/v3/repos/octo-org/app/issues/{number}"),
            json!({"body":"Observed after deploy 412."}),
        )
        .await;
    pass(&trial.assess());
}

#[tokio::test]
async fn creation_rejects_duplicates_wrong_repositories_and_unrequested_assignment() {
    let duplicate = Trial::new("file-login-bug");
    duplicate.create(BODY).await;
    duplicate.create(BODY).await;
    fail(&duplicate.assess());
    let wrong = Trial::new("file-login-bug");
    wrong
        .call(
            "POST",
            "/api/v3/repos/octo-org/payments-api/issues",
            json!({"title":TITLE,"body":BODY}),
        )
        .await;
    fail(&wrong.assess());
    let assignment = Trial::new("file-login-bug");
    assignment
        .call(
            "POST",
            "/api/v3/repos/octo-org/app/issues",
            json!({"title":TITLE,"body":BODY,"assignees":["sam"]}),
        )
        .await;
    fail(&assignment.assess());
}

#[tokio::test]
async fn keyword_presence_does_not_establish_a_truthful_bug_description() {
    let trial = Trial::new("file-login-bug");
    trial
        .create("The login page has NOT returned HTTP 500 since deploy 412.")
        .await;
    unresolved(&trial.assess());
}

#[tokio::test]
async fn filing_a_bug_allows_existing_classification_labels_not_only_the_oracles_empty_set() {
    let trial = Trial::new("file-login-bug");
    trial
        .call(
            "POST",
            "/api/v3/repos/octo-org/app/issues",
            json!({
                "title":TITLE,"body":BODY,"labels":["bug","auth"]
            }),
        )
        .await;
    pass(&trial.assess());
}

#[tokio::test]
async fn old_issue_edits_remain_forbidden_when_restored() {
    let trial = Trial::new("file-login-bug");
    trial.create(BODY).await;
    for title in ["An unauthorized title", "Crash when saving settings"] {
        trial
            .call(
                "PATCH",
                "/api/v3/repos/octo-org/app/issues/1",
                json!({"title":title}),
            )
            .await;
    }
    fail(&trial.assess());
    assert!(!trial.assess().history_ok);
}

#[tokio::test]
async fn additive_triage_accepts_existing_extra_labels_split_operations_and_retries() {
    let trial = Trial::with_dataset("triage-duplicate-charge", |dataset| {
        dataset
            .services
            .get_mut("github")
            .unwrap()
            .collection_mut("issues")
            .get_mut("octo-org/payments-api#12")
            .unwrap()["labels"] = json!(["customer-reported", "billing", "bug"]);
    });
    for _attempt in 0..2 {
        trial
            .call(
                "POST",
                &format!("{ISSUE}/labels"),
                json!({"labels":["sev-1"]}),
            )
            .await;
        trial
            .call(
                "POST",
                &format!("{ISSUE}/assignees"),
                json!({"assignees":["sam"]}),
            )
            .await;
    }
    pass(&trial.assess());
}

#[tokio::test]
async fn triage_rejects_substring_labels_extra_assignees_and_restored_label_removal() {
    for body in [
        json!({"labels":["bug","customer-reported","sev-10"],"assignees":["sam"]}),
        json!({"labels":["bug","customer-reported","sev-1"],"assignees":["sam","lin"]}),
    ] {
        let trial = Trial::new("triage-duplicate-charge");
        trial.call("PATCH", ISSUE, body).await;
        fail(&trial.assess());
    }
    let restored = Trial::new("triage-duplicate-charge");
    restored
        .call(
            "PATCH",
            ISSUE,
            json!({"labels":["sev-1"],"assignees":["sam"]}),
        )
        .await;
    restored.triage().await;
    fail(&restored.assess());
    assert!(!restored.assess().history_ok);
}

#[tokio::test]
async fn invalid_compound_issue_updates_do_not_publish_labels_or_partial_effects() {
    let trial = Trial::new("triage-duplicate-charge");
    let before = trial.state();
    let (status, _) = trial
        .request(
            "PATCH",
            ISSUE,
            json!({"labels":["new-invalid-request-label"],"state":"invalid"}),
        )
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(trial.state(), before);
}

#[tokio::test]
async fn pending_review_then_rest_submission_and_custom_squash_message_are_valid() {
    let trial = Trial::new("approve-merge-dependency-bump");
    let review = trial
        .call("POST", &format!("{PULL}/reviews"), json!({}))
        .await;
    trial
        .call(
            "POST",
            &format!("{PULL}/reviews/{}/events", review["id"]),
            json!({"event":"APPROVE"}),
        )
        .await;
    let merged = trial.merge(json!({"commit_title":"Apply the lodash security update","commit_message":"Bump lodash to 4.17.21."})).await;
    assert_ne!(merged["sha"], HEAD);
    assert_ne!(merged["sha"], BASE);
    pass(&trial.assess());
}

#[tokio::test]
async fn merge_requires_the_correct_review_and_cannot_modify_another_pull_request() {
    let trial = Trial::new("approve-merge-dependency-bump");
    trial.approve(None, 55).await;
    trial.approve(None, 57).await;
    trial.merge(json!({})).await;
    fail(&trial.assess());
}

#[tokio::test]
async fn later_approval_cannot_repair_a_merge_that_preceded_it() {
    let trial = Trial::with_dataset("approve-merge-dependency-bump", |dataset| {
        dataset
            .services
            .get_mut("github")
            .unwrap()
            .collection_mut("branches")
            .get_mut("octo-org/web:main")
            .unwrap()["required_approvals"] = json!(0);
    });
    trial.merge(json!({})).await;
    trial.approve(None, 57).await;
    fail(&trial.assess());
    assert!(!trial.assess().ordering_ok);
}

#[tokio::test]
async fn merge_result_must_remain_the_actual_main_branch_commit() {
    let trial = Trial::new("approve-merge-dependency-bump");
    trial.approve(None, 57).await;
    trial.merge(json!({})).await;
    trial
        .call(
            "PATCH",
            "/api/v3/repos/octo-org/web/git/refs/heads/main",
            json!({"sha":HEAD,"force":true}),
        )
        .await;
    fail(&trial.assess());
    assert!(!trial.assess().relations_ok);
}

#[tokio::test]
async fn optional_review_text_cannot_hide_a_contradiction_by_restoring_it() {
    let trial = Trial::new("approve-merge-dependency-bump");
    let review = trial.approve(Some("LGTM."), 57).await;
    for body in ["This did not fix lodash; LGTM.", "LGTM."] {
        trial
            .call(
                "PUT",
                &format!("{PULL}/reviews/{review}"),
                json!({"body":body}),
            )
            .await;
    }
    trial.merge(json!({})).await;
    unresolved(&trial.assess());
}

#[tokio::test]
async fn rest_pending_submission_cannot_bypass_self_approval_or_required_comment_body() {
    for event in ["APPROVE", "REQUEST_CHANGES", "COMMENT"] {
        let trial = Trial::with_dataset("approve-merge-dependency-bump", |dataset| {
            dataset
                .services
                .get_mut("github")
                .unwrap()
                .collection_mut("pulls")
                .get_mut("octo-org/web#57")
                .unwrap()["user"] = json!("mona");
        });
        let review = trial
            .call("POST", &format!("{PULL}/reviews"), json!({}))
            .await;
        let before = trial.state();
        let (status, _) = trial
            .request(
                "POST",
                &format!("{PULL}/reviews/{}/events", review["id"]),
                json!({"event":event}),
            )
            .await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{event}");
        assert_eq!(trial.state(), before, "{event}");
    }
}

#[tokio::test]
async fn graphql_pending_submission_rejects_blank_comment_without_committing() {
    let trial = Trial::new("approve-merge-dependency-bump");
    let review = trial
        .call("POST", &format!("{PULL}/reviews"), json!({}))
        .await;
    let before = trial.state();
    let (status, result) = trial.request("POST", "/api/graphql", json!({
        "query":"mutation($input: SubmitPullRequestReviewInput!) { submitPullRequestReview(input:$input) { pullRequestReview { id state } } }",
        "variables":{"input":{"pullRequestReviewId":review["node_id"],"event":"COMMENT"}}
    })).await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        result["errors"]
            .as_array()
            .is_some_and(|errors| !errors.is_empty())
    );
    assert_eq!(trial.state(), before);
}

#[tokio::test]
async fn different_merge_methods_and_reviews_of_other_heads_do_not_satisfy_the_task() {
    let rebase = Trial::new("approve-merge-dependency-bump");
    rebase.approve(None, 57).await;
    rebase.merge(json!({"merge_method":"rebase"})).await;
    fail(&rebase.assess());
    let wrong_head = Trial::new("approve-merge-dependency-bump");
    wrong_head
        .call(
            "POST",
            &format!("{PULL}/reviews"),
            json!({
                "event":"APPROVE","commit_id":"0d2f21da868f4125f15d529dc12b95fee09dd50a"
            }),
        )
        .await;
    wrong_head.merge(json!({})).await;
    fail(&wrong_head.assess());
    assert!(!wrong_head.assess().relations_ok);
}

#[tokio::test]
async fn optional_review_wording_is_not_required_but_unreviewed_claims_need_adjudication() {
    for body in ["LGTM.", "The update looks fine to me."] {
        let trial = Trial::new("approve-merge-dependency-bump");
        trial.approve(Some(body), 57).await;
        trial.merge(json!({})).await;
        if body == "LGTM." {
            pass(&trial.assess());
        } else {
            unresolved(&trial.assess());
        }
    }
}
