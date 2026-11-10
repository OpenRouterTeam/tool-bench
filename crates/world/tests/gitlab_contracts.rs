mod support;

use mock_core::{
    dataset::Dataset,
    grading::{Grade, Outcome},
};
use serde_json::{Value, json};
use support::ContractTrial as Trial;

const CHECKOUT: &str = "/api/v4/projects/northwind%2Fpayments%2Fcheckout-api";
const BILLING: &str = "/api/v4/projects/northwind%2Fbilling%2Fbilling-service/merge_requests/88";
const LEDGER: &str = "/api/v4/projects/northwind%2Fpayments%2Fledger";
const MERGE: &str = "/api/v4/projects/northwind%2Fpayments%2Fledger/merge_requests/41";
const THREAD: &str = "d7e8c986532452a2290d7e7255e2b2cae36e36ce";
const QUESTION: &str =
    "Did you update CHANGELOG.md for the 4.2 bump? Support reads it before every billing release.";
const TITLE: &str = "Add circuit breaker around fraud-score service calls";
const DESCRIPTION: &str = "Fraud-score latency spikes cascaded into checkout timeouts.";
const REPLY: &str = "The changelog entry was added in commit 24f172e5.";

impl Trial {
    fn new(slug: &str) -> Self {
        Self::with_dataset(slug, |_| {})
    }

    fn with_dataset(slug: &str, prepare: impl FnOnce(&mut Dataset)) -> Self {
        Self::load_with_dataset(
            &format!("code/gitlab/{slug}"),
            &mock_gitlab::Gitlab,
            &[],
            prepare,
        )
    }

    async fn create(&self, extra: Value) -> Value {
        let mut body = json!({
            "title":TITLE,"description":DESCRIPTION,"labels":["backend","reliability"],
            "milestone_id":7104,"assignee_ids":[44]
        });
        body.as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        self.call("POST", &format!("{CHECKOUT}/issues"), body).await
    }

    async fn reply(&self, body: &str) -> u64 {
        self.call(
            "POST",
            &format!("{BILLING}/discussions/{THREAD}/notes"),
            json!({"body":body}),
        )
        .await["id"]
            .as_u64()
            .unwrap()
    }

    async fn resolve(&self) {
        self.call(
            "PUT",
            &format!("{BILLING}/discussions/{THREAD}"),
            json!({"resolved":true}),
        )
        .await;
    }

    async fn resolve_note(&self, note: u64) {
        self.call(
            "PUT",
            &format!("{BILLING}/discussions/{THREAD}/notes/{note}"),
            json!({"resolved":true}),
        )
        .await;
    }

    async fn approve(&self) {
        self.call("POST", &format!("{MERGE}/approve"), json!({}))
            .await;
    }

    async fn merge(&self, remove: bool, extra: Value) -> Value {
        let mut body = json!({"should_remove_source_branch":remove});
        body.as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        self.call("PUT", &format!("{MERGE}/merge"), body).await
    }

    async fn complete(&self, slug: &str) {
        match slug {
            "file-circuit-breaker-issue" => {
                self.create(json!({})).await;
            }
            "answer-changelog-review-thread" => {
                self.reply(REPLY).await;
                self.resolve().await;
            }
            _ => {
                self.approve().await;
                self.merge(true, json!({})).await;
            }
        }
    }
}

fn pass(result: &Grade) {
    assert_eq!(result.outcome, Outcome::Pass, "{result:?}");
    assert!(result.preservation_checked && result.history_checked);
}

fn fail(result: &Grade) {
    assert_eq!(result.outcome, Outcome::Fail, "{result:?}");
    assert!(result.contract_ok && result.environment_ok && result.integrity_ok);
}

#[tokio::test]
async fn all_three_gitlab_workflows_require_real_outcomes_and_authoritative_preservation() {
    for slug in [
        "file-circuit-breaker-issue",
        "answer-changelog-review-thread",
        "approve-and-merge-hotfix",
    ] {
        let trial = Trial::new(slug);
        fail(&trial.assess());
        trial.complete(slug).await;
        pass(&trial.assess());
    }
}

#[tokio::test]
async fn creation_follows_shifted_ids_and_allows_staged_classification_and_existing_extra_labels() {
    let trial = Trial::with_dataset("file-circuit-breaker-issue", |dataset| {
        let issues = dataset
            .services
            .get_mut("gitlab")
            .unwrap()
            .collection_mut("issues");
        let mut other = issues
            .get("northwind/payments/checkout-api#57")
            .unwrap()
            .clone();
        other["id"] = json!(49999);
        other["iid"] = json!(99);
        other["title"] = json!("Unrelated deployment cleanup");
        issues.insert("northwind/payments/checkout-api#99", other);
    });
    let created = trial
        .create(json!({"description":"","labels":[],"milestone_id":0,"assignee_ids":[]}))
        .await;
    assert_eq!(created["iid"], 100);
    trial
        .call(
            "PUT",
            &format!("{CHECKOUT}/issues/{}", created["iid"]),
            json!({
                "description":DESCRIPTION,"labels":["reliability","postmortem","backend"],
                "milestone_id":7104,"assignee_ids":[44]
            }),
        )
        .await;
    pass(&trial.assess());
}

#[tokio::test]
async fn description_passes_in_unlisted_wording_and_fails_on_a_service_name_or_another_service() {
    let truthful = Trial::new("file-circuit-breaker-issue");
    truthful
        .create(json!({
            "description":"When fraud-score latency spiked during the outage, checkout requests timed out waiting on it."
        }))
        .await;
    pass(&truthful.assess());
    for description in [
        "fraud-score",
        "Fraud-score errors caused the checkout outage.",
        "Address-validation latency spikes cascaded into checkout timeouts.",
    ] {
        let trial = Trial::new("file-circuit-breaker-issue");
        trial.create(json!({"description":description})).await;
        fail(&trial.assess());
    }
}

#[tokio::test]
async fn classification_uses_exact_labels_and_the_requested_milestone_and_person() {
    for extra in [
        json!({"labels":["backend","reliability-review"]}),
        json!({"milestone_id":7105}),
        json!({"assignee_ids":[45]}),
    ] {
        let trial = Trial::new("file-circuit-breaker-issue");
        trial.create(extra).await;
        fail(&trial.assess());
    }
}

#[tokio::test]
async fn creation_cannot_duplicate_an_issue_or_create_new_label_records() {
    let duplicate = Trial::new("file-circuit-breaker-issue");
    duplicate.create(json!({})).await;
    duplicate.create(json!({})).await;
    fail(&duplicate.assess());
    let labels = Trial::new("file-circuit-breaker-issue");
    labels
        .create(json!({"labels":["backend","reliability","invented-new-label"]}))
        .await;
    fail(&labels.assess());
}

#[tokio::test]
async fn old_issues_are_protected_even_if_their_titles_are_restored() {
    let trial = Trial::new("file-circuit-breaker-issue");
    let original = trial
        .call("GET", &format!("{CHECKOUT}/issues/51"), Value::Null)
        .await;
    trial.create(json!({})).await;
    for title in [json!("Unauthorized replacement"), original["title"].clone()] {
        trial
            .call(
                "PUT",
                &format!("{CHECKOUT}/issues/51"),
                json!({"title":title}),
            )
            .await;
    }
    fail(&trial.assess());
    assert!(!trial.assess().history_ok);
}

#[tokio::test]
async fn thread_reply_and_individual_note_resolution_follow_actual_shifted_identities() {
    let trial = Trial::with_dataset("answer-changelog-review-thread", |dataset| {
        let notes = dataset
            .services
            .get_mut("gitlab")
            .unwrap()
            .collection_mut("notes");
        let mut question = notes.remove("81503").unwrap();
        question["id"] = json!(999999);
        notes.insert("999999", question);
    });
    let reply = trial.reply(REPLY).await;
    let thread = trial
        .call(
            "GET",
            &format!("{BILLING}/discussions/{THREAD}"),
            Value::Null,
        )
        .await;
    let question = thread["notes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|note| note["body"] == QUESTION)
        .unwrap()["id"]
        .as_u64()
        .unwrap();
    assert_eq!(question, 999999);
    trial.resolve_note(reply).await;
    trial.resolve_note(question).await;
    pass(&trial.assess());
}

#[tokio::test]
async fn reply_passes_in_unlisted_wording_and_fails_on_a_commit_token_or_another_commit() {
    let truthful = Trial::new("answer-changelog-review-thread");
    truthful
        .reply("Yes, CHANGELOG.md was updated in 24f172e5.")
        .await;
    truthful.resolve().await;
    pass(&truthful.assess());
    for body in [
        "24f172e5",
        "Yes, the changelog entry has been added.",
        "The changelog entry was added in commit 1d91daaa.",
    ] {
        let trial = Trial::new("answer-changelog-review-thread");
        trial.reply(body).await;
        trial.resolve().await;
        fail(&trial.assess());
    }
}

#[tokio::test]
async fn resolving_before_replying_is_not_repaired_by_a_later_reply() {
    let trial = Trial::new("answer-changelog-review-thread");
    trial.resolve().await;
    trial.reply(REPLY).await;
    fail(&trial.assess());
    assert!(trial.assess().records_ok);
    assert!(!trial.assess().ordering_ok);
}

#[tokio::test]
async fn replies_on_another_thread_cannot_satisfy_the_requested_thread() {
    let trial = Trial::new("answer-changelog-review-thread");
    trial
        .call(
            "POST",
            &format!("{BILLING}/discussions/f16d76f16c9f60a92e4e7dbd070831df127ddb78/notes"),
            json!({"body":REPLY}),
        )
        .await;
    trial.resolve().await;
    fail(&trial.assess());
}

#[tokio::test]
async fn a_reworded_reply_is_judged_by_its_final_text() {
    const DRAFT: &str = "Yes, the changelog entry has been added.";
    let trial = Trial::new("answer-changelog-review-thread");
    let reply = trial.reply(REPLY).await;
    trial.resolve().await;
    let completed = trial.state();
    trial
        .call(
            "PUT",
            &format!("{BILLING}/notes/{reply}"),
            json!({"body":DRAFT}),
        )
        .await;
    // Left without the commit, the reply no longer says what was asked.
    fail(&trial.assess());
    trial
        .call(
            "PUT",
            &format!("{BILLING}/notes/{reply}"),
            json!({"body":REPLY}),
        )
        .await;
    assert_eq!(trial.state(), completed);
    // An interim wording that lacked the commit is a draft, not a committed wrong claim.
    pass(&trial.assess());
}

#[tokio::test]
async fn replies_to_resolved_threads_inherit_resolution_without_restamping_old_notes() {
    let trial = Trial::with_dataset("answer-changelog-review-thread", |dataset| {
        let question = dataset
            .services
            .get_mut("gitlab")
            .unwrap()
            .collection_mut("notes")
            .get_mut("81503")
            .unwrap();
        question["resolved"] = json!(true);
        question["resolved_by"] = json!("dana.kowalski");
        question["resolved_at"] = json!("2026-04-01T14:00:00.000Z");
    });
    let original = trial.state()["notes"]["81503"].clone();
    let reply = trial.reply(REPLY).await;
    let created = &trial.state()["notes"][reply.to_string()];
    assert_eq!(created["resolved"], true);
    assert_eq!(created["resolved_by"], "sam.okafor");
    trial.resolve().await;
    trial.resolve_note(81503).await;
    assert_eq!(trial.state()["notes"]["81503"], original);
    pass(&trial.assess());
}

#[tokio::test]
async fn system_notes_are_not_editable_through_either_body_route() {
    let trial = Trial::new("answer-changelog-review-thread");
    let before = trial.state();
    for path in [
        format!("{BILLING}/notes/81501"),
        format!("{BILLING}/discussions/c21d4f11c7f3eb397335aeaa137af1a108027c10/notes/81501"),
    ] {
        let (status, _) = trial
            .request("PUT", &path, json!({"body":"Forged platform history"}))
            .await;
        assert!(status.is_client_error(), "{path}: {status}");
        assert_eq!(trial.state(), before);
    }
}

#[tokio::test]
async fn combined_note_body_and_resolution_updates_are_rejected_without_partial_changes() {
    let trial = Trial::new("answer-changelog-review-thread");
    let before = trial.state();
    let (status, _) = trial
        .request(
            "PUT",
            &format!("{BILLING}/discussions/{THREAD}/notes/81503"),
            json!({
                "body":"An unintended question replacement","resolved":true
            }),
        )
        .await;
    assert!(status.is_client_error());
    assert_eq!(trial.state(), before);
}

#[tokio::test]
async fn merge_keeps_other_approvals_and_accepts_separate_source_branch_deletion() {
    let trial = Trial::with_dataset("approve-and-merge-hotfix", |dataset| {
        dataset
            .services
            .get_mut("gitlab")
            .unwrap()
            .collection_mut("merge_requests")
            .get_mut("northwind/payments/ledger!41")
            .unwrap()["approved_by"] = json!(["mei.chen"]);
    });
    trial.approve().await;
    trial.merge(false, json!({})).await;
    trial
        .call(
            "DELETE",
            &format!("{LEDGER}/repository/branches/hotfix%2Fdouble-post-refunds"),
            Value::Null,
        )
        .await;
    pass(&trial.assess());
    assert_eq!(
        trial.state()["issues"]["northwind/payments/ledger#118"]["state"],
        "closed"
    );
}

#[tokio::test]
async fn custom_merge_messages_do_not_require_a_predicted_commit_hash() {
    let trial = Trial::new("approve-and-merge-hotfix");
    trial.approve().await;
    let merged = trial
        .merge(
            true,
            json!({"merge_commit_message":"Apply the reviewed refund retry hotfix"}),
        )
        .await;
    let state = trial.state();
    assert_eq!(
        state["branches"]["northwind/payments/ledger:main"]["commit"],
        merged["merge_commit_sha"]
    );
    pass(&trial.assess());
}

#[tokio::test]
async fn merge_requires_real_source_removal_and_protects_other_merge_requests() {
    let retained = Trial::new("approve-and-merge-hotfix");
    retained.approve().await;
    retained.merge(false, json!({})).await;
    fail(&retained.assess());
    let other = Trial::new("approve-and-merge-hotfix");
    let path = format!("{LEDGER}/merge_requests/42");
    let original = other.call("GET", &path, Value::Null).await;
    other.approve().await;
    other.merge(true, json!({})).await;
    for title in [json!("Unrequested replacement"), original["title"].clone()] {
        other.call("PUT", &path, json!({"title":title})).await;
    }
    fail(&other.assess());
    assert!(!other.assess().history_ok);
}

#[tokio::test]
async fn reopening_the_automatically_closed_issue_is_not_an_authorized_merge_effect() {
    let trial = Trial::new("approve-and-merge-hotfix");
    trial.approve().await;
    trial.merge(true, json!({})).await;
    trial
        .call(
            "PUT",
            &format!("{LEDGER}/issues/118"),
            json!({"state_event":"reopen"}),
        )
        .await;
    fail(&trial.assess());
}

#[tokio::test]
async fn the_linked_issue_cannot_be_closed_before_the_fix_is_merged() {
    let trial = Trial::new("approve-and-merge-hotfix");
    trial
        .call(
            "PUT",
            &format!("{LEDGER}/issues/118"),
            json!({"state_event":"close"}),
        )
        .await;
    trial.approve().await;
    trial.merge(true, json!({})).await;
    let result = trial.assess();
    fail(&result);
    assert!(result.state_ok && result.records_ok && result.relations_ok);
    assert!(!result.ordering_ok);
}

#[tokio::test]
async fn squash_merging_materializes_real_commits_instead_of_manufacturing_a_sha() {
    let trial = Trial::new("approve-and-merge-hotfix");
    trial.approve().await;
    let merged = trial.merge(true, json!({"squash":true})).await;
    let state = trial.state();
    let squash = merged["squash_commit_sha"].as_str().unwrap();
    let merge = merged["merge_commit_sha"].as_str().unwrap();
    assert_ne!(squash, merge);
    assert!(state["commits"][format!("northwind/payments/ledger@{squash}")].is_object());
    assert_eq!(
        state["commits"][format!("northwind/payments/ledger@{merge}")]["parent_ids"][1],
        squash
    );
    trial
        .call(
            "GET",
            &format!("{LEDGER}/repository/commits/{squash}"),
            Value::Null,
        )
        .await;
    pass(&trial.assess());
}

#[tokio::test]
async fn missing_source_branches_cannot_produce_a_successful_merge_or_partial_effects() {
    let trial = Trial::new("approve-and-merge-hotfix");
    trial.approve().await;
    trial
        .call(
            "DELETE",
            &format!("{LEDGER}/repository/branches/hotfix%2Fdouble-post-refunds"),
            Value::Null,
        )
        .await;
    let before = trial.state();
    let (status, _) = trial
        .request(
            "PUT",
            &format!("{MERGE}/merge"),
            json!({"should_remove_source_branch":true}),
        )
        .await;
    assert!(status.is_client_error(), "{status}");
    assert_eq!(trial.state(), before);
}
