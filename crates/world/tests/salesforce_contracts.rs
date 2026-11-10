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
use scenario::Scenario;
use serde_json::{Value, json};
use tower::ServiceExt;

const CASE: &str = "5005g00000spz8MAAQ";
const OPPORTUNITY: &str = "0065g000006FoSrAAK";
const CONTACT: &str = "0035g00000p3z7WAAQ";
const ACCOUNT: &str = "0015g000008HFvFAAW";
const NOTE: &str = "Discussed renewal pricing. Dana wants a 3-year quote by May 22.";

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
            .join("../../scenarios/business/salesforce")
            .join(slug);
        let scenario = Scenario::load(&directory).unwrap();
        let mut dataset = Dataset::load(&directory).unwrap();
        prepare(&mut dataset);
        let world = shared(WorldState::from_dataset(dataset).unwrap());
        let app = service_app(world.clone(), &mock_salesforce::Salesforce);
        Self {
            scenario,
            world,
            app,
        }
    }

    async fn request(&self, method: &str, path: &str, body: Value) -> Value {
        let request = Request::builder()
            .method(method)
            .uri(format!("/services/data/v60.0/{path}"))
            .header("host", "world:8122")
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
        let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let result = if bytes.is_empty() {
            Value::Null
        } else {
            serde_json::from_slice(&bytes).unwrap()
        };
        assert!(status.is_success(), "{method} {path}: {status} {result}");
        result
    }

    async fn update(&self, object: &str, identity: &str, fields: Value) {
        self.request("PATCH", &format!("sobjects/{object}/{identity}"), fields)
            .await;
    }

    async fn close_case(&self) {
        self.update(
            "Case",
            CASE,
            json!({"Status": "Closed", "Reason": "Performance"}),
        )
        .await;
    }

    async fn win(&self) {
        self.update(
            "Opportunity",
            OPPORTUNITY,
            json!({"StageName": "Closed Won", "CloseDate": "2026-05-12"}),
        )
        .await;
    }

    async fn call(&self, extra: Value) -> String {
        let mut fields = json!({
            "WhoId": CONTACT, "Subject": "Renewal pricing call",
            "Status": "Completed", "ActivityDate": "2026-05-12",
            "TaskSubtype": "Call", "Description": NOTE,
        });
        fields
            .as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        let result = self.request("POST", "sobjects/Task", fields).await;
        result["id"].as_str().unwrap().to_owned()
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

#[tokio::test]
async fn closed_case_cannot_hide_changes_to_its_priority_or_other_customers() {
    for (identity, field, values) in [
        (CASE, "Priority", ["Low", "High"]),
        ("5005g00000DXqN3AAL", "Reason", ["Performance", ""]),
    ] {
        let trial = Trial::new("close-resolved-case");
        trial.close_case().await;
        for value in values {
            trial
                .update(
                    "Case",
                    identity,
                    json!({field: if value.is_empty() { Value::Null } else { json!(value) }}),
                )
                .await;
        }
        let result = trial.assess();
        assert_eq!(result.reward, 0.0, "{result:?}");
        assert!(!result.history_ok, "{result:?}");
    }
}

#[tokio::test]
async fn won_opportunity_cannot_hide_restored_amount_changes_or_unrelated_edits() {
    let trial = Trial::new("close-won-kestrel-renewal");
    trial.win().await;
    for amount in [1, 184000] {
        trial
            .update("Opportunity", OPPORTUNITY, json!({"Amount": amount}))
            .await;
    }
    let result = trial.assess();
    assert_eq!(result.reward, 0.0, "{result:?}");
    assert!(!result.history_ok);
}

#[tokio::test]
async fn split_updates_aliases_and_retries_preserve_the_actual_amount() {
    let trial = Trial::with_dataset("close-won-kestrel-renewal", |dataset| {
        dataset
            .services
            .get_mut("salesforce")
            .unwrap()
            .collection_mut("Opportunity")
            .get_mut(OPPORTUNITY)
            .unwrap()["Amount"] = json!(219_500);
    });
    trial
        .update(
            "Opportunity",
            &OPPORTUNITY[..15],
            json!({"CloseDate": "2026-05-12"}),
        )
        .await;
    for _attempt in 0..2 {
        trial
            .update(
                "Opportunity",
                OPPORTUNITY,
                json!({"StageName": "Closed Won"}),
            )
            .await;
    }
    let result = trial.assess();
    assert_eq!(result.reward, 1.0, "{result:?}");
    assert!(result.preservation_checked && result.history_checked);
    let opportunity = &trial.snapshot()["services"]["salesforce"]["Opportunity"][OPPORTUNITY];
    assert_eq!(opportunity["Amount"], 219_500);
    assert_eq!(opportunity["ExpectedRevenue"], 219_500.0);
}

#[tokio::test]
async fn no_action_is_valid_when_the_requested_case_outcome_already_holds() {
    let trial = Trial::with_dataset("close-resolved-case", |dataset| {
        let record = dataset
            .services
            .get_mut("salesforce")
            .unwrap()
            .collection_mut("Case")
            .get_mut(CASE)
            .unwrap();
        record["Status"] = json!("Closed");
        record["Reason"] = json!("Performance");
        record["IsClosed"] = json!(true);
        record["ClosedDate"] = json!("2026-05-10T12:00:00.000+0000");
    });
    let result = trial.assess();
    assert_eq!(result.reward, 1.0, "{result:?}");
    assert!(result.preservation_checked && result.history_checked);
}

#[tokio::test]
async fn actual_created_call_identity_is_used_instead_of_the_predicted_first_id() {
    let trial = Trial::with_dataset("log-call-on-contact", |dataset| {
        let tasks = dataset
            .services
            .get_mut("salesforce")
            .unwrap()
            .collection_mut("Task");
        let mut existing = tasks.values().next().unwrap().clone();
        existing["Id"] = json!("00T8c0000000001EAA");
        tasks.insert("00T8c0000000001EAA", existing);
    });
    let actual = trial.call(json!({})).await;
    assert_ne!(actual, "00T8c0000000001EAA");
    let result = trial.assess();
    assert_eq!(result.reward, 1.0, "{result:?}");
    assert!(result.preservation_checked && result.history_checked);
}

#[tokio::test]
async fn a_meeting_or_wrong_related_account_is_not_the_requested_call() {
    for extra in [
        json!({"TaskSubtype": "Task", "Type": "Meeting"}),
        json!({"WhatId": "0015g00000xkfi9AAA"}),
        json!({"WhoId": "0035g00000my8VHAAY"}),
        json!({"WhoId": "0035g00000MiU60AAF"}),
        json!({"CallDurationInSeconds": 3600}),
        json!({"CallType": "Outbound"}),
    ] {
        let trial = Trial::new("log-call-on-contact");
        trial.call(extra).await;
        assert_eq!(trial.assess().reward, 0.0);
    }
}

#[tokio::test]
async fn explicit_coherent_stage_defaults_and_split_case_updates_are_valid() {
    let trial = Trial::new("close-won-kestrel-renewal");
    trial.update(
        "Opportunity", OPPORTUNITY,
        json!({"StageName": "Closed Won", "Probability": 100, "ForecastCategoryName": "Closed", "CloseDate": "2026-05-12"}),
    ).await;
    assert_eq!(trial.assess().reward, 1.0);
    let trial = Trial::new("close-resolved-case");
    trial
        .update("Case", &CASE[..15], json!({"Reason": "Performance"}))
        .await;
    for _attempt in 0..2 {
        trial
            .update("Case", CASE, json!({"Status": "Closed"}))
            .await;
    }
    let result = trial.assess();
    assert_eq!(result.reward, 1.0, "{result:?}");
    assert_eq!(
        trial.snapshot()["services"]["salesforce"]["Case"][CASE]["ClosedDate"],
        "2026-05-12T15:00:00.000+0000"
    );
}

#[tokio::test]
async fn existing_later_activity_dates_are_preserved_by_a_backdated_call() {
    let trial = Trial::with_dataset("log-call-on-contact", |dataset| {
        let service = dataset.services.get_mut("salesforce").unwrap();
        service.collection_mut("Contact").get_mut(CONTACT).unwrap()["LastActivityDate"] =
            json!("2026-06-01");
        service.collection_mut("Account").get_mut(ACCOUNT).unwrap()["LastActivityDate"] =
            json!("2026-06-02");
    });
    trial.call(json!({})).await;
    let result = trial.assess();
    assert_eq!(result.reward, 1.0, "{result:?}");
    let snapshot = trial.snapshot();
    assert_eq!(
        snapshot["services"]["salesforce"]["Contact"][CONTACT]["LastActivityDate"],
        "2026-06-01"
    );
    assert_eq!(
        snapshot["services"]["salesforce"]["Account"][ACCOUNT]["LastActivityDate"],
        "2026-06-02"
    );
}

#[tokio::test]
async fn already_won_custom_forecasting_is_preserved_without_forcing_stage_defaults() {
    let trial = Trial::with_dataset("close-won-kestrel-renewal", |dataset| {
        let record = dataset
            .services
            .get_mut("salesforce")
            .unwrap()
            .collection_mut("Opportunity")
            .get_mut(OPPORTUNITY)
            .unwrap();
        record["StageName"] = json!("Closed Won");
        record["CloseDate"] = json!("2026-05-12");
        record["IsWon"] = json!(true);
        record["IsClosed"] = json!(true);
        record["Probability"] = json!(80);
        record["ExpectedRevenue"] = json!(147200.0);
        record["ForecastCategoryName"] = json!("Commit");
        record["ForecastCategory"] = json!("Forecast");
    });
    let result = trial.assess();
    assert_eq!(result.reward, 1.0, "{result:?}");
    trial
        .update(
            "Opportunity",
            OPPORTUNITY,
            json!({"CloseDate": "2026-05-12"}),
        )
        .await;
    assert_eq!(trial.assess().reward, 1.0);
    assert_eq!(
        trial.snapshot()["services"]["salesforce"]["Opportunity"][OPPORTUNITY]["Probability"],
        80
    );
}

#[tokio::test]
async fn arbitrary_new_probability_and_forecast_overrides_are_not_requested_side_effects() {
    for extra in [
        json!({"Probability": 80}),
        json!({"ForecastCategoryName": "Omitted"}),
    ] {
        let trial = Trial::new("close-won-kestrel-renewal");
        trial.win().await;
        trial.update("Opportunity", OPPORTUNITY, extra).await;
        let result = trial.assess();
        assert_eq!(result.reward, 0.0, "{result:?}");
    }
}

#[tokio::test]
async fn reusing_an_old_task_or_transiently_creating_on_the_wrong_person_is_not_valid() {
    let trial = Trial::new("log-call-on-contact");
    trial
        .update(
            "Task",
            "00T5g00000MeP3JEAV",
            json!({
                "Subject": "Renewal pricing call", "ActivityDate": "2026-05-12",
                "Description": NOTE, "Type": "Call",
            }),
        )
        .await;
    assert_eq!(trial.assess().reward, 0.0);

    let trial = Trial::new("log-call-on-contact");
    let actual = trial
        .call(json!({"WhoId": "0035g00000my8VHAAY", "Status": "In Progress"}))
        .await;
    trial
        .update(
            "Task",
            &actual,
            json!({"WhoId": CONTACT, "Status": "Completed"}),
        )
        .await;
    let result = trial.assess();
    assert_eq!(result.reward, 0.0);
    assert!(!result.history_ok);
}

#[tokio::test]
async fn contact_only_account_link_and_both_call_representations_are_valid() {
    for extra in [
        json!({}),
        json!({"WhatId": ACCOUNT, "Type": "Call"}),
        json!({"TaskSubtype": "Task", "Type": "Call"}),
        json!({"OwnerId": "0055g00000MZfGEAA1", "Priority": "High"}),
        json!({"Description": "Dana requested a three-year renewal quote by May 22, 2026."}),
    ] {
        let trial = Trial::new("log-call-on-contact");
        trial.call(extra).await;
        let result = trial.assess();
        assert_eq!(result.reward, 1.0, "{result:?}");
        let snapshot = trial.snapshot();
        assert_eq!(
            snapshot["services"]["salesforce"]["Contact"][CONTACT]["LastActivityDate"],
            "2026-05-12"
        );
        assert_eq!(
            snapshot["services"]["salesforce"]["Account"][ACCOUNT]["LastActivityDate"],
            "2026-05-12"
        );
    }
}

#[tokio::test]
async fn unlisted_note_wording_passes_with_the_term_and_the_date_and_fails_without_them() {
    for description in [
        "Dana asked for a three-year quote; she needs it by May 22, 2026.",
        "Renewal pricing call. Follow-up: send Dana a 3 year quote by 5/22.",
    ] {
        let trial = Trial::new("log-call-on-contact");
        trial.call(json!({"Description": description})).await;
        let result = trial.assess();
        assert_eq!(result.reward, 1.0, "{description}: {result:?}");
        assert!(result.preservation_checked && result.history_checked);
    }
    for description in [
        "quote",
        "Dana wants a 1-year quote by June 22.",
        "Dana wants a 3-year quote.",
        "Dana wants a quote by May 22.",
        "Dana wants a 3-year quote by May 29.",
    ] {
        let trial = Trial::new("log-call-on-contact");
        trial.call(json!({"Description": description})).await;
        let result = trial.assess();
        assert_eq!(result.reward, 0.0, "{description}: {result:?}");
        assert_eq!(result.outcome, Outcome::Fail, "{description}: {result:?}");
        assert!(!result.adjudication_needed);
    }
}

#[tokio::test]
async fn equivalent_numeric_serializations_do_not_change_the_preserved_amount() {
    for amount in [json!(184000.0), json!("184000")] {
        let trial = Trial::new("close-won-kestrel-renewal");
        trial
            .update(
                "Opportunity",
                OPPORTUNITY,
                json!({"StageName": "Closed Won", "CloseDate": "2026-05-12", "Amount": amount}),
            )
            .await;
        let result = trial.assess();
        assert_eq!(result.reward, 1.0, "{result:?}");
        assert!(result.preservation_checked && result.history_checked);
    }
}

#[tokio::test]
async fn empty_or_unspecified_auxiliary_type_does_not_erase_a_valid_call_classification() {
    for extra in [
        json!({"Type": ""}),
        json!({"TaskSubtype": null, "Type": "Call"}),
    ] {
        let trial = Trial::new("log-call-on-contact");
        trial.call(extra).await;
        let result = trial.assess();
        assert_eq!(result.reward, 1.0, "{result:?}");
    }
}

#[tokio::test]
async fn missing_note_is_a_known_failure_and_wrong_entities_dominate_text_uncertainty() {
    for extra in [
        json!({"Description": null}),
        json!({"Description": ""}),
        json!({"WhoId": "0035g00000MiU60AAF", "Description": "quote"}),
    ] {
        let trial = Trial::new("log-call-on-contact");
        trial.call(extra).await;
        let result = trial.assess();
        assert_eq!(result.reward, 0.0);
        assert_eq!(result.outcome, Outcome::Fail, "{result:?}");
    }
}

#[tokio::test]
async fn creation_then_refinement_is_valid_but_duplicate_calls_and_restored_old_task_edits_are_not()
{
    let trial = Trial::new("log-call-on-contact");
    let actual = trial
        .call(json!({"Status": "In Progress", "Description": null}))
        .await;
    trial
        .update(
            "Task",
            &actual,
            json!({"Status": "Completed", "Description": NOTE}),
        )
        .await;
    let result = trial.assess();
    assert_eq!(result.reward, 1.0, "{result:?}");
    trial.call(json!({})).await;
    assert_eq!(trial.assess().reward, 0.0);

    let trial = Trial::new("log-call-on-contact");
    trial.call(json!({})).await;
    for subject in ["Unrequested edit", "QBR prep with Dana"] {
        trial
            .update("Task", "00T5g00000MeP3JEAV", json!({"Subject": subject}))
            .await;
    }
    assert_eq!(trial.assess().reward, 0.0);
}
