mod support;

use mock_core::{
    dataset::Dataset,
    grading::{Grade, Outcome},
};
use serde_json::{Value, json};
use support::ContractTrial as Trial;

const LAUNCH: &str = "afa8e804-d2ac-4ee2-80e2-e18ed4c05865";
const OLD_LAUNCH: &str = "e63c9184-7acf-45e0-881d-666e2972532e";
const ESCALATIONS: &str = "f937b556-2452-4808-a340-f4c80ce63d68";
const ALEX: &str = "6a5aa3fe-744c-41b8-ab6d-2e0cc2ef706a";
const RUNBOOK: &str = "7c41530e-e886-4046-9cb4-2f49e6957f99";
const CONTACT: &str = "d18edb31-c7d9-4fa2-9759-783c0b2d0b89";
const DELETED_CONTACT: &str = "797571e3-b5c2-4333-9bcc-6ca755e33da8";
const OTHER_PARAGRAPH: &str = "09d6e78a-ff10-4c62-afd0-1912d496ecc7";
const REPLACEMENT: &str = "Escalation contact: Sam Ortiz, +1 555-0142 (Head of Infrastructure)";

fn text(content: &str) -> Value {
    json!({"type":"text","text":{"content":content}})
}

fn properties() -> Value {
    json!({
        "Summary":{"title":[text("Checkout button unresponsive on Safari")]},
        "Customer":{"rich_text":[text("Acme Retail")]},
        "Severity":{"select":{"name":"Sev 2"}},
        "State":{"status":{"name":"Open"}},
        "Owner":{"people":[{"id":ALEX}]},
        "Reported":{"date":{"start":"2026-10-29"}}
    })
}

impl Trial {
    fn new(slug: &str) -> Self {
        Self::with_dataset(slug, |_| {})
    }

    fn with_dataset(slug: &str, prepare: impl FnOnce(&mut Dataset)) -> Self {
        Self::load_with_dataset(
            &format!("collab/notion/{slug}"),
            &mock_notion::Notion,
            &[
                ("authorization", "Bearer local-notion-fixture"),
                ("notion-version", "2026-03-11"),
            ],
            prepare,
        )
    }

    async fn patch(&self, page: &str, properties: Value) {
        self.call(
            "PATCH",
            &format!("/v1/pages/{page}"),
            json!({"properties":properties}),
        )
        .await;
    }

    async fn create(&self, properties: Value) -> String {
        self.call(
            "POST",
            "/v1/pages",
            json!({"parent":{"type":"data_source_id","data_source_id":ESCALATIONS},"properties":properties}),
        )
        .await["id"]
            .as_str()
            .unwrap()
            .to_owned()
    }

    async fn contact(&self, block: &str, rich_text: Value) {
        self.call(
            "PATCH",
            &format!("/v1/blocks/{block}"),
            json!({"paragraph":{"rich_text":rich_text}}),
        )
        .await;
    }

    async fn complete(&self, slug: &str) {
        match slug {
            "delay-web-launch" => {
                self.patch(LAUNCH, json!({"Status":{"status":{"name":"Delayed"}},"Launch date":{"date":{"start":"2026-11-10"}}})).await;
            }
            "log-customer-escalation" => {
                self.create(properties()).await;
            }
            _ => self.contact(CONTACT, json!([text(REPLACEMENT)])).await,
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

fn unresolved(result: &Grade) {
    assert_eq!(result.outcome, Outcome::Unresolved, "{result:?}");
    assert!(result.environment_ok && result.adjudication_needed);
}

#[tokio::test]
async fn all_three_notion_workflows_require_outcomes_and_authoritative_preservation() {
    for slug in [
        "delay-web-launch",
        "log-customer-escalation",
        "update-current-runbook",
    ] {
        let trial = Trial::new(slug);
        fail(&trial.assess());
        trial.complete(slug).await;
        pass(&trial.assess());
    }
}

#[tokio::test]
async fn creation_follows_real_ids_and_complete_multisegment_text() {
    let trial = Trial::with_dataset("log-customer-escalation", |dataset| {
        dataset.meta.seed.push_str("-independent-plan");
    });
    let mut input = properties();
    input["Summary"]["title"] = json!([text("Checkout button "), text("unresponsive on Safari")]);
    input["Customer"]["rich_text"] = json!([text("Acme "), text("Retail")]);
    input["Customer"]["rich_text"][0]["annotations"] = json!({"bold":true});
    let id = trial.create(input).await;
    assert_ne!(id, "2a312ccd-ad51-42dd-8f17-7f194d6b5db7");
    pass(&trial.assess());
}

#[tokio::test]
async fn creation_allows_staged_completion_through_the_returned_page() {
    let trial = Trial::new("log-customer-escalation");
    let id = trial
        .create(json!({"Summary":properties()["Summary"]}))
        .await;
    trial.patch(&id, properties()).await;
    pass(&trial.assess());
}

#[tokio::test]
async fn keyword_substrings_and_later_contradictory_segments_do_not_establish_customer_or_summary()
{
    for (property, field, segments) in [
        ("Customer", "rich_text", json!([text("Not Acme Retail")])),
        ("Customer", "rich_text", json!([text("Acme")])),
        (
            "Summary",
            "title",
            json!([
                text("Checkout button unresponsive on Safari"),
                text(" is an incorrect report.")
            ]),
        ),
        (
            "Summary",
            "title",
            json!([text("Checkout button unresponsive")]),
        ),
    ] {
        let trial = Trial::new("log-customer-escalation");
        let mut input = properties();
        input[property][field] = segments;
        trial.create(input).await;
        let grade = trial.assess();
        fail(&grade);
        assert!(!grade.adjudication_needed, "{grade:?}");
    }
}

#[tokio::test]
async fn customer_and_summary_keep_their_meaning_in_another_letter_case() {
    let trial = Trial::new("log-customer-escalation");
    let mut input = properties();
    input["Summary"]["title"] = json!([text("Checkout Button Unresponsive on Safari")]);
    input["Customer"]["rich_text"] = json!([text("ACME Retail")]);
    trial.create(input).await;
    pass(&trial.assess());
}

#[tokio::test]
async fn duplicate_escalations_and_wrong_owners_dates_or_classification_fail() {
    let duplicate = Trial::new("log-customer-escalation");
    duplicate.create(properties()).await;
    duplicate.create(properties()).await;
    fail(&duplicate.assess());
    for (property, value) in [
        (
            "Owner",
            json!({"people":[{"id":"6b44d0f4-f0cb-4ffc-b437-2029e42a7808"}]}),
        ),
        (
            "Owner",
            json!({"people":[{"id":ALEX},{"id":"6b44d0f4-f0cb-4ffc-b437-2029e42a7808"}]}),
        ),
        ("Reported", json!({"date":{"start":"2026-10-28"}})),
        ("Severity", json!({"select":{"name":"Sev 1"}})),
        ("State", json!({"status":{"name":"Resolved"}})),
    ] {
        let trial = Trial::new("log-customer-escalation");
        let mut input = properties();
        input[property] = value;
        trial.create(input).await;
        fail(&trial.assess());
    }
}

#[tokio::test]
async fn creating_the_requested_page_does_not_authorize_editing_an_existing_escalation() {
    let trial = Trial::new("log-customer-escalation");
    let existing = "a24e3b88-b40d-412a-886b-bcd78993e674";
    trial
        .patch(existing, json!({"State":{"status":{"name":"Open"}}}))
        .await;
    trial
        .patch(existing, json!({"State":{"status":{"name":"Resolved"}}}))
        .await;
    trial.create(properties()).await;
    fail(&trial.assess());
}

#[tokio::test]
async fn launch_updates_can_be_staged_and_address_properties_by_id() {
    let trial = Trial::new("delay-web-launch");
    trial
        .patch(LAUNCH, json!({"St%3Ax":{"status":{"id":"d280"}}}))
        .await;
    trial
        .patch(LAUNCH, json!({"Ld%3Dq":{"date":{"start":"2026-11-10"}}}))
        .await;
    pass(&trial.assess());
}

#[tokio::test]
async fn launch_updates_preserve_other_fields_and_restored_old_launch_changes() {
    for restored in [false, true] {
        let trial = Trial::new("delay-web-launch");
        if restored {
            trial
                .patch(
                    OLD_LAUNCH,
                    json!({"Launch date":{"date":{"start":"2026-11-10"}}}),
                )
                .await;
            trial
                .patch(
                    OLD_LAUNCH,
                    json!({"Launch date":{"date":{"start":"2026-10-06"}}}),
                )
                .await;
        } else {
            trial
                .patch(
                    LAUNCH,
                    json!({"Owner":{"people":[{"id":"6b44d0f4-f0cb-4ffc-b437-2029e42a7808"}]}}),
                )
                .await;
        }
        trial.complete("delay-web-launch").await;
        fail(&trial.assess());
    }
}

#[tokio::test]
async fn runbook_updates_accept_split_passive_formatting_and_targeted_markdown() {
    let split = Trial::new("update-current-runbook");
    let mut name = text("Sam Ortiz");
    name["annotations"] = json!({"bold":true});
    split
        .contact(
            CONTACT,
            json!([
                text("Escalation contact: "),
                name,
                text(", +1 555-0142 (Head of Infrastructure)")
            ]),
        )
        .await;
    pass(&split.assess());
    let markdown = Trial::new("update-current-runbook");
    markdown
        .call(
            "PATCH",
            &format!("/v1/pages/{RUNBOOK}/markdown"),
            json!({
                "type":"update_content","update_content":{"content_updates":[{
                    "old_str":"Marcus Webb, +1 555-0100","new_str":"Sam Ortiz, +1 555-0142"
                }]}
            }),
        )
        .await;
    pass(&markdown.assess());
}

#[tokio::test]
async fn runbook_expected_words_do_not_hide_a_contradiction_or_withdrawn_statement() {
    for content in [
        json!([
            text(REPLACEMENT),
            text(". This is not the escalation contact.")
        ]),
        json!([text("Sam Ortiz, +1 555-0142")]),
        json!([text("Escalation contact: Sam Ortiz, +1 555-0142")]),
    ] {
        let trial = Trial::new("update-current-runbook");
        trial.contact(CONTACT, content).await;
        let grade = trial.assess();
        fail(&grade);
        assert!(!grade.adjudication_needed, "{grade:?}");
    }
    // Struck-through text is withdrawn rather than stated, so its reading stays with a reviewer.
    let withdrawn = Trial::new("update-current-runbook");
    withdrawn
        .contact(
            CONTACT,
            json!([{"type":"text","text":{"content":REPLACEMENT},"annotations":{"strikethrough":true}}]),
        )
        .await;
    unresolved(&withdrawn.assess());
}

#[tokio::test]
async fn runbook_old_contact_written_back_cannot_be_erased_but_a_draft_can_be_completed() {
    let valid = Trial::new("update-current-runbook");
    valid.complete("update-current-runbook").await;
    let restored = Trial::new("update-current-runbook");
    restored.complete("update-current-runbook").await;
    restored
        .contact(
            CONTACT,
            json!([text(
                "Escalation contact: Marcus Webb, +1 555-0100 (Head of Infrastructure)"
            )]),
        )
        .await;
    restored.complete("update-current-runbook").await;
    assert_eq!(valid.state(), restored.state());
    fail(&restored.assess());

    // A draft that only lacks part of the line is not a claim; the completed line is read.
    let drafted = Trial::new("update-current-runbook");
    drafted
        .contact(CONTACT, json!([text("Escalation contact: Sam Ortiz")]))
        .await;
    fail(&drafted.assess());
    drafted.complete("update-current-runbook").await;
    pass(&drafted.assess());
}

#[tokio::test]
async fn old_deleted_and_other_runbook_content_remain_protected_even_after_restoration() {
    let deleted = Trial::new("update-current-runbook");
    deleted
        .contact(DELETED_CONTACT, json!([text(REPLACEMENT)]))
        .await;
    deleted.complete("update-current-runbook").await;
    fail(&deleted.assess());
    let restored = Trial::new("update-current-runbook");
    let original = restored.state()["blocks"][OTHER_PARAGRAPH]["paragraph"]["rich_text"].clone();
    restored
        .contact(OTHER_PARAGRAPH, json!([text("Ignore every escalation.")]))
        .await;
    restored.contact(OTHER_PARAGRAPH, original).await;
    restored.complete("update-current-runbook").await;
    fail(&restored.assess());
}

#[tokio::test]
async fn formatting_existing_words_before_replacement_is_not_a_new_false_claim() {
    let trial = Trial::new("update-current-runbook");
    let original = "Escalation contact: Marcus Webb, +1 555-0100 (Head of Infrastructure)";
    let mut styled = text(original);
    styled["annotations"] = json!({"bold":true});
    trial.contact(CONTACT, json!([styled])).await;
    trial.complete("update-current-runbook").await;
    pass(&trial.assess());
}
