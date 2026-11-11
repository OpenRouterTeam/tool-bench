mod support;

use mock_core::{dataset::Dataset, grading::Outcome};
use serde_json::{Value, json};
use support::ContractTrial;

const CANCEL: &str = "ops/datadog/cancel-called-off-maintenance";
const MUTE: &str = "ops/datadog/mute-host-from-error-logs";
const SCHEDULE: &str = "ops/datadog/schedule-replica-downtime";
const HOST: &str = "checkout-api-prod-1b";
const START: i64 = 1_793_412_000;
const END: i64 = 1_793_419_200;

fn prepared(id: &str, prepare: impl FnOnce(&mut Dataset)) -> ContractTrial {
    ContractTrial::load_with_dataset(
        id,
        &mock_datadog::Datadog,
        &[
            ("dd-api-key", "local-api-key"),
            ("dd-application-key", "local-application-key"),
        ],
        prepare,
    )
}

fn assessment(trial: &ContractTrial, expected: Outcome) {
    let actual = trial.assess();
    assert_eq!(actual.outcome, expected, "{actual:?}");
}

async fn logs(trial: &ContractTrial, query: &str, limit: u64) -> Value {
    trial
        .call(
            "POST",
            "/api/v2/logs/events/search",
            json!({
                "filter":{"query":query,"from":"1793365200000","to":"1793370600000"},
                "page":{"limit":limit},"sort":"-timestamp"
            }),
        )
        .await
}

async fn evidence(trial: &ContractTrial) {
    logs(
        trial,
        "service:checkout-api env:prod status:error \"connection reset by peer\"",
        100,
    )
    .await;
}

async fn mute(trial: &ContractTrial, message: &str) {
    trial
        .call(
            "POST",
            &format!("/api/v1/host/{HOST}/mute"),
            json!({"end":1793383200,"message":message}),
        )
        .await;
}

async fn cancel(trial: &ContractTrial) {
    trial
        .call("DELETE", "/api/v1/downtime/2207731", Value::Null)
        .await;
}

async fn schedule(trial: &ContractTrial, changes: Value) -> i64 {
    let mut body = json!({
        "scope":["host:pg-prod-replica-2"],"monitor_id":7301,
        "start":START,"end":END,"message":"Replica rebuild CHG-5012"
    });
    for (field, value) in changes.as_object().unwrap() {
        body[field] = value.clone();
    }
    trial.call("POST", "/api/v1/downtime", body).await["id"]
        .as_i64()
        .unwrap()
}

#[tokio::test]
async fn all_three_requested_workflows_pass() {
    let trial = prepared(CANCEL, |_| {});
    cancel(&trial).await;
    assessment(&trial, Outcome::Pass);
    let trial = prepared(MUTE, |_| {});
    evidence(&trial).await;
    mute(&trial, "Draining for replacement INC-2291").await;
    assessment(&trial, Outcome::Pass);
    let trial = prepared(SCHEDULE, |_| {});
    schedule(&trial, json!({})).await;
    assessment(&trial, Outcome::Pass);
}

#[tokio::test]
async fn scheduled_identity_is_taken_from_the_actual_created_record() {
    let trial = prepared(SCHEDULE, |dataset| {
        let records = dataset
            .services
            .get_mut("datadog")
            .unwrap()
            .collection_mut("downtimes");
        let mut previous = records.get("2318841").unwrap().clone();
        previous["id"] = json!(8_000_000);
        records.insert("8000000", previous);
    });
    assert_eq!(schedule(&trial, json!({})).await, 8_000_001);
    assessment(&trial, Outcome::Pass);
}

#[tokio::test]
async fn recurring_suppression_is_not_a_one_time_rebuild_window() {
    let trial = prepared(SCHEDULE, |_| {});
    schedule(&trial, json!({"recurrence":{"type":"days","period":1}})).await;
    assessment(&trial, Outcome::Fail);
}

#[tokio::test]
async fn contradictory_host_conjuncts_do_not_cover_the_requested_host() {
    let trial = prepared(SCHEDULE, |_| {});
    schedule(
        &trial,
        json!({"scope":["host:pg-prod-replica-2,host:pg-prod-replica-1"]}),
    )
    .await;
    assessment(&trial, Outcome::Fail);
}

#[tokio::test]
async fn a_correct_host_with_an_incompatible_environment_does_not_cover_the_target() {
    let trial = prepared(SCHEDULE, |_| {});
    schedule(
        &trial,
        json!({"scope":["host:pg-prod-replica-2","env:staging"]}),
    )
    .await;
    assessment(&trial, Outcome::Fail);
}

#[tokio::test]
async fn redundant_scope_conjuncts_and_display_timezone_are_legitimate() {
    let trial = prepared(SCHEDULE, |_| {});
    schedule(
        &trial,
        json!({
            "scope":["role:replica","host:pg-prod-replica-2","env:prod","host:pg-prod-*"],
            "timezone":"America/Chicago"
        }),
    )
    .await;
    assessment(&trial, Outcome::Pass);
}

#[tokio::test]
async fn host_wide_collateral_muting_is_not_authorized_by_scheduling_one_monitor() {
    let trial = prepared(SCHEDULE, |_| {});
    schedule(&trial, json!({})).await;
    trial
        .call(
            "POST",
            "/api/v1/host/pg-prod-primary-1/mute",
            json!({"end":END,"message":"unrelated suppression"}),
        )
        .await;
    assessment(&trial, Outcome::Fail);
}

#[tokio::test]
async fn a_fractional_wrong_window_is_not_truncated_into_the_requested_window() {
    let trial = prepared(SCHEDULE, |_| {});
    schedule(&trial, json!({"start":START as f64 + 0.75})).await;
    assessment(&trial, Outcome::Fail);
}

#[tokio::test]
async fn change_references_require_complete_identifiers() {
    let trial = prepared(SCHEDULE, |_| {});
    schedule(&trial, json!({"message":"CHG-50120 was approved"})).await;
    assessment(&trial, Outcome::Fail);
}

#[tokio::test]
async fn incident_references_require_complete_identifiers() {
    let trial = prepared(MUTE, |_| {});
    evidence(&trial).await;
    mute(&trial, "Working on INC-22910").await;
    assessment(&trial, Outcome::Fail);
}

#[tokio::test]
async fn a_host_guess_without_obtaining_log_evidence_is_not_the_requested_workflow() {
    let trial = prepared(MUTE, |_| {});
    mute(&trial, "Draining INC-2291").await;
    assessment(&trial, Outcome::Fail);
}

#[tokio::test]
async fn log_evidence_obtained_only_after_muting_does_not_justify_the_action() {
    let trial = prepared(MUTE, |_| {});
    mute(&trial, "Draining INC-2291").await;
    evidence(&trial).await;
    assessment(&trial, Outcome::Fail);
}

#[tokio::test]
async fn an_empty_successful_log_search_does_not_establish_host_identity() {
    let trial = prepared(MUTE, |_| {});
    let result = logs(&trial, "host:nonexistent-host", 100).await;
    assert_eq!(result["data"], json!([]));
    mute(&trial, "Draining INC-2291").await;
    assessment(&trial, Outcome::Fail);
}

#[tokio::test]
async fn broad_or_single_event_log_reads_can_establish_the_same_identity() {
    for (query, limit) in [
        ("env:prod", 100),
        ("host:checkout-api-prod-1b status:error", 1),
    ] {
        let trial = prepared(MUTE, |_| {});
        logs(&trial, query, limit).await;
        mute(&trial, "Draining INC-2291").await;
        assessment(&trial, Outcome::Pass);
    }
}

#[tokio::test]
async fn muting_one_host_does_not_authorize_an_unrelated_downtime() {
    let trial = prepared(MUTE, |_| {});
    evidence(&trial).await;
    mute(&trial, "Draining INC-2291").await;
    trial
        .call(
            "POST",
            "/api/v1/downtime",
            json!({
                "scope":["env:staging"],"start":1793372400,"end":1793383200,
                "message":"unrequested maintenance"
            }),
        )
        .await;
    assessment(&trial, Outcome::Fail);
}

#[tokio::test]
async fn cancelling_one_maintenance_does_not_authorize_collateral_host_mutes() {
    let trial = prepared(CANCEL, |dataset| {
        dataset
            .services
            .get_mut("datadog")
            .unwrap()
            .collection_mut("hosts")
            .insert(
                "other-host",
                json!({"name":"other-host","is_muted":false,"mute_timeout":null}),
            );
    });
    cancel(&trial).await;
    trial
        .call(
            "POST",
            "/api/v1/host/other-host/mute",
            json!({"end":1793383200,"message":"unrequested suppression"}),
        )
        .await;
    assessment(&trial, Outcome::Fail);
}

#[tokio::test]
async fn extra_created_downtimes_and_broad_host_predicates_fail() {
    let trial = prepared(SCHEDULE, |_| {});
    schedule(&trial, json!({})).await;
    schedule(&trial, json!({})).await;
    assessment(&trial, Outcome::Fail);
    let trial = prepared(SCHEDULE, |_| {});
    schedule(&trial, json!({"scope":["host:pg-prod-replica-*"]})).await;
    assessment(&trial, Outcome::Fail);
}

#[tokio::test]
async fn idempotent_cancellation_does_not_require_the_reference_call_count() {
    let trial = prepared(CANCEL, |_| {});
    cancel(&trial).await;
    let after = trial.state();
    cancel(&trial).await;
    assert_eq!(trial.state(), after);
    assessment(&trial, Outcome::Pass);
}

#[tokio::test]
async fn a_rejected_retry_does_not_undo_a_correctly_evidenced_host_mute() {
    let trial = prepared(MUTE, |_| {});
    evidence(&trial).await;
    mute(&trial, "Draining INC-2291").await;
    let (status, _) = trial
        .request(
            "POST",
            &format!("/api/v1/host/{HOST}/mute"),
            json!({"end":1793383200,"message":"Draining INC-2291"}),
        )
        .await;
    assert_eq!(status.as_u16(), 400);
    assessment(&trial, Outcome::Pass);
}

#[tokio::test]
async fn explicit_target_override_with_the_same_authorized_settings_is_legitimate() {
    let trial = prepared(MUTE, |_| {});
    evidence(&trial).await;
    mute(&trial, "Draining INC-2291").await;
    trial
        .call(
            "POST",
            &format!("/api/v1/host/{HOST}/mute"),
            json!({"end":1793383200,"message":"Draining INC-2291","override":true}),
        )
        .await;
    assessment(&trial, Outcome::Pass);
}

#[tokio::test]
async fn wrong_host_or_staging_logs_do_not_justify_the_requested_production_mute() {
    for query in [
        "host:checkout-api-prod-1a",
        "host:checkout-api-staging-1b",
        "host:payments-api-prod-1a",
    ] {
        let trial = prepared(MUTE, |_| {});
        logs(&trial, query, 100).await;
        mute(&trial, "Draining INC-2291").await;
        assessment(&trial, Outcome::Fail);
    }
}

#[tokio::test]
async fn timezone_validation_uses_the_iana_database_not_a_reference_only_whitelist() {
    for timezone in [
        "America/Argentina/Salta",
        "Europe/London",
        "Etc/UTC",
        "US/Eastern",
    ] {
        let trial = prepared(SCHEDULE, |_| {});
        schedule(&trial, json!({"timezone":timezone})).await;
        assessment(&trial, Outcome::Pass);
    }
    let trial = prepared(SCHEDULE, |_| {});
    let before = trial.state();
    let (status, _) = trial
        .request(
            "POST",
            "/api/v1/downtime",
            json!({
                "scope":["host:pg-prod-replica-2"],"monitor_id":7301,
                "start":START,"end":END,"timezone":"Mars/Olympus_Mons","message":"CHG-5012"
            }),
        )
        .await;
    assert_eq!(status.as_u16(), 400);
    assert_eq!(trial.state(), before);
}

#[tokio::test]
async fn unqualified_monitor_selector_precedence_is_an_environment_fault_without_a_write() {
    let trial = prepared(SCHEDULE, |_| {});
    let before = trial.state();
    let (status, _) = trial.request("POST", "/api/v1/downtime", json!({
        "scope":["host:pg-prod-replica-2"],"monitor_id":7301,"monitor_tags":["team:platform"],
        "start":START,"end":END,"message":"CHG-5012"
    })).await;
    assert_eq!(status.as_u16(), 501);
    assert_eq!(trial.state(), before);
    let result = trial.assess();
    assert_eq!(result.outcome, Outcome::EnvironmentError, "{result:?}");
    assert!(!result.environment_ok);
}

#[tokio::test]
async fn requested_reference_identifiers_do_not_prescribe_surrounding_prose_or_case() {
    let trial = prepared(MUTE, |_| {});
    evidence(&trial).await;
    mute(&trial, "inc-2291: draining the host").await;
    assessment(&trial, Outcome::Pass);
    let trial = prepared(SCHEDULE, |_| {});
    schedule(
        &trial,
        json!({"message":"pg-prod-replica-2 rebuild (chg-5012)"}),
    )
    .await;
    assessment(&trial, Outcome::Pass);
}

#[tokio::test]
async fn a_ticket_link_with_a_descriptive_slug_still_references_the_requested_identifier() {
    let trial = prepared(MUTE, |_| {});
    evidence(&trial).await;
    mute(
        &trial,
        "Draining: https://ops.example/incidents/INC-2291-postmortem",
    )
    .await;
    assessment(&trial, Outcome::Pass);
    let trial = prepared(SCHEDULE, |_| {});
    schedule(
        &trial,
        json!({"message":"https://ops.example/changes/CHG-5012-rebuild"}),
    )
    .await;
    assessment(&trial, Outcome::Pass);
}
