mod support;

use mock_core::grading::Outcome;
use serde_json::{Value, json};
use support::ContractTrial;

fn trial() -> ContractTrial {
    ContractTrial::load_with_dataset(
        "ops/pagerduty/cover-shift-override",
        &mock_pagerduty::Pagerduty,
        &[("authorization", "Token token=local-fixture-credential")],
        |_| {},
    )
}

async fn create(trial: &ContractTrial, start: &str, end: &str) -> Value {
    let response = trial
        .call(
            "POST",
            "/schedules/PSPLTPR/overrides",
            json!({"overrides":[{"start":start,"end":end,
            "user":{"id":"PJ0RLEE","type":"user_reference"}}]}),
        )
        .await;
    assert_eq!(response[0]["status"], 201, "{response}");
    response[0]["override"].clone()
}

async fn duty(trial: &ContractTrial, at: &str) -> Vec<String> {
    let response = trial
        .call(
            "GET",
            &format!("/oncalls?schedule_ids%5B%5D=PSPLTPR&since={at}&until={at}"),
            Value::Null,
        )
        .await;
    let users: std::collections::BTreeSet<_> = response["oncalls"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| entry["user"]["id"].as_str().unwrap().to_owned())
        .collect();
    users.into_iter().collect()
}

#[tokio::test]
async fn fractional_split_coverage_is_a_valid_complete_business_outcome() {
    let trial = trial();
    create(
        &trial,
        "2026-10-31T09:00:00Z",
        "2026-10-31T13:00:00.123456789Z",
    )
    .await;
    create(
        &trial,
        "2026-10-31T13:00:00.123456789Z",
        "2026-10-31T17:00:00Z",
    )
    .await;
    let assessment = trial.assess();
    assert_eq!(assessment.outcome, Outcome::Pass, "{assessment:?}");
}

#[tokio::test]
async fn routing_preserves_exact_subsecond_half_open_edges() {
    let trial = trial();
    let record = create(
        &trial,
        "2026-10-31T09:00:00.000000001Z",
        "2026-10-31T09:00:00.000000003Z",
    )
    .await;
    assert_eq!(record["start"], "2026-10-31T09:00:00.000000001Z");
    assert_eq!(record["end"], "2026-10-31T09:00:00.000000003Z");
    for (at, expected) in [
        ("2026-10-31T09:00:00Z", "PA1EXKM"),
        ("2026-10-31T09:00:00.000000001Z", "PJ0RLEE"),
        ("2026-10-31T09:00:00.000000002Z", "PJ0RLEE"),
        ("2026-10-31T09:00:00.000000003Z", "PA1EXKM"),
    ] {
        assert_eq!(duty(&trial, at).await, vec![expected], "{at}");
    }
}

#[tokio::test]
async fn schedule_and_override_views_preserve_fractional_extents() {
    let trial = trial();
    create(
        &trial,
        "2026-10-31T04:00:00.123456789-05:00",
        "2026-10-31T17:00:00.987654321Z",
    )
    .await;
    let response = trial
        .call(
            "GET",
            "/schedules/PSPLTPR?since=2026-10-31T09:00:00Z&until=2026-10-31T17:00:01Z",
            Value::Null,
        )
        .await;
    let entries = response["schedule"]["final_schedule"]["rendered_schedule_entries"]
        .as_array()
        .unwrap();
    let covering = entries
        .iter()
        .find(|entry| entry["user"]["id"] == "PJ0RLEE")
        .unwrap();
    assert_eq!(covering["start"], "2026-10-31T09:00:00.123456789Z");
    assert_eq!(covering["end"], "2026-10-31T17:00:00.987654321Z");
    let response=trial.call("GET",
        "/schedules/PSPLTPR/overrides?since=2026-10-31T09:00:00.123456788Z&until=2026-10-31T09:00:00.123456790Z",
        Value::Null).await;
    assert_eq!(response["overrides"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn fractional_layer_rotation_edges_are_not_truncated_to_seconds() {
    let trial = ContractTrial::load_with_dataset(
        "ops/pagerduty/cover-shift-override",
        &mock_pagerduty::Pagerduty,
        &[],
        |dataset| {
            let schedules = dataset
                .services
                .get_mut("pagerduty")
                .unwrap()
                .collection_mut("schedules");
            let schedule = schedules.get_mut("PSPLTPR").unwrap();
            schedule["schedule_layers"] = json!([{
                "start":"2026-10-31T09:00:00.123456Z",
                "rotation_virtual_start":"2026-10-31T09:00:00.123456Z",
                "rotation_turn_length_seconds":604800,
                "users":[{"user":{"id":"PA1EXKM"}},{"user":{"id":"PJ0RLEE"}}]
            }]);
        },
    );
    assert_eq!(
        duty(&trial, "2026-11-07T09:00:00.123455999Z").await,
        vec!["PA1EXKM"]
    );
    assert_eq!(
        duty(&trial, "2026-11-07T09:00:00.123456Z").await,
        vec!["PJ0RLEE"]
    );
}

#[tokio::test]
async fn a_fractional_gap_or_extension_is_a_business_failure_not_an_environment_fault() {
    for (middle_end, middle_start) in [
        ("2026-10-31T13:00:00.123456Z", "2026-10-31T13:00:00.123457Z"),
        (
            "2026-10-31T13:00:00.123456788Z",
            "2026-10-31T13:00:00.123456789Z",
        ),
    ] {
        let trial = trial();
        create(&trial, "2026-10-31T09:00:00Z", middle_end).await;
        create(&trial, middle_start, "2026-10-31T17:00:00Z").await;
        let assessment = trial.assess();
        assert!(assessment.environment_ok, "{assessment:?}");
        assert_eq!(assessment.outcome, Outcome::Fail, "{assessment:?}");
    }
    let trial = trial();
    create(
        &trial,
        "2026-10-31T09:00:00Z",
        "2026-10-31T17:00:00.000001Z",
    )
    .await;
    let assessment = trial.assess();
    assert!(assessment.environment_ok, "{assessment:?}");
    assert_eq!(assessment.outcome, Outcome::Fail, "{assessment:?}");
}

#[tokio::test]
async fn overlapping_overrides_do_not_inflate_coverage_beyond_one_hundred_percent() {
    let trial = trial();
    for _ in 0..2 {
        create(
            &trial,
            "2026-10-31T09:00:00.123456Z",
            "2026-10-31T17:00:00.123456Z",
        )
        .await;
    }
    let response=trial.call("GET",
        "/schedules/PSPLTPR?since=2026-10-31T09:00:00.123456Z&until=2026-10-31T17:00:00.123456Z",
        Value::Null).await;
    assert_eq!(
        response["schedule"]["overrides_subschedule"]["rendered_coverage_percentage"],
        100.0
    );
    assert_eq!(
        response["schedule"]["final_schedule"]["rendered_coverage_percentage"],
        100.0
    );
}

#[tokio::test]
async fn incident_and_history_filters_do_not_drop_fractional_query_precision() {
    let trial = ContractTrial::load_with_dataset(
        "ops/pagerduty/cover-shift-override",
        &mock_pagerduty::Pagerduty,
        &[],
        |dataset| {
            let store = dataset.services.get_mut("pagerduty").unwrap();
            for (identity, number, created) in [
                ("PFRAC01", 9001, "2026-10-30T13:00:00.000002Z"),
                ("PFRAC02", 9002, "2026-10-30T13:00:00.000001Z"),
            ] {
                store.collection_mut("incidents").insert(identity,json!({
                    "id":identity,"incident_number":number,"status":"triggered","title":"Precision fixture",
                    "created_at":created,"service":{"id":"PSVORDB","type":"service_reference"}
                }));
                store.collection_mut("log_entries").insert(
                    identity,
                    json!({
                        "id":identity,"type":"trigger_log_entry","created_at":created,
                        "incident":{"id":identity,"type":"incident_reference"},
                        "service":{"id":"PSVORDB","type":"service_reference"}
                    }),
                );
            }
        },
    );
    let response=trial.call("GET",
        "/incidents?since=2026-10-30T13:00:00Z&until=2026-10-30T13:00:00.000003Z&sort_by=created_at:asc",
        Value::Null).await;
    let identities: Vec<_> = response["incidents"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row["id"].as_str().unwrap())
        .collect();
    assert_eq!(identities, vec!["PFRAC02", "PFRAC01"]);
    let response = trial
        .call(
            "GET",
            "/incidents?since=2026-10-30T13:00:00.000001001Z&until=2026-10-30T13:00:00.000002001Z",
            Value::Null,
        )
        .await;
    assert_eq!(response["incidents"].as_array().unwrap().len(), 1);
    assert_eq!(response["incidents"][0]["id"], "PFRAC01");
    let response=trial.call("GET",
        "/log_entries?since=2026-10-30T13:00:00.000001001Z&until=2026-10-30T13:00:00.000002001Z",
        Value::Null).await;
    assert_eq!(response["log_entries"].as_array().unwrap().len(), 1);
    assert_eq!(response["log_entries"][0]["id"], "PFRAC01");
}

#[tokio::test]
async fn oncall_seconds_are_quantized_after_union_not_at_each_endpoint() {
    let trial =
        ContractTrial::load_with_dataset(
            "ops/pagerduty/cover-shift-override",
            &mock_pagerduty::Pagerduty,
            &[],
            |dataset| {
                let policies = dataset
                    .services
                    .get_mut("pagerduty")
                    .unwrap()
                    .collection_mut("escalation_policies");
                let identities: Vec<_> = policies
                    .iter()
                    .map(|(identity, _)| identity.to_owned())
                    .collect();
                for identity in identities {
                    policies.remove(&identity);
                }
                policies.insert("PEPPLAT",json!({
                "id":"PEPPLAT","name":"Platform",
                "escalation_rules":[{"targets":[{"id":"PSPLTPR","type":"schedule_reference"}]}]
            }));
            },
        );
    create(&trial, "2026-10-31T09:00:00.1Z", "2026-10-31T09:00:00.9Z").await;
    create(&trial, "2026-10-31T09:00:01.1Z", "2026-10-31T09:00:01.9Z").await;
    let response = trial
        .call(
            "POST",
            "/analytics/metrics/responders/all",
            json!({
                "filters":{"date_range_start":"2026-10-31T09:00:00Z",
                    "date_range_end":"2026-10-31T09:00:03Z","responder_ids":["PJ0RLEE"]}
            }),
        )
        .await;
    let rows = response["data"].as_array().unwrap();
    assert_eq!(rows.len(), 1, "{response}");
    assert_eq!(rows[0]["total_seconds_on_call"], 1);
    assert_eq!(rows[0]["total_seconds_on_call_level_1"], 1);
}
