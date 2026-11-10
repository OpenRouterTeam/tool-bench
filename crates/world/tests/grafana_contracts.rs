mod support;

use mock_core::{dataset::Dataset, grading::Outcome};
use serde_json::{Value, json};
use support::ContractTrial;

const ACK: &str = "ops/grafana/ack-checkout-page-oncall";
const ANNOTATE: &str = "ops/grafana/annotate-checkout-deploy";
const SILENCE: &str = "ops/grafana/silence-replica-maintenance";
const DEPLOY: i64 = 1_794_492_300_000;
const ONCALL: &str = "sbrennan (s.brennan@octo-corp.com)";

fn prepared(id: &str, prepare: impl FnOnce(&mut Dataset)) -> ContractTrial {
    let authorization = format!("Bearer {}", mock_grafana::TOKEN);
    ContractTrial::load_with_dataset(
        id,
        &mock_grafana::Grafana,
        &[("authorization", &authorization)],
        prepare,
    )
}

fn assessment(trial: &ContractTrial, answer: Option<&str>, expected: Outcome) {
    let result = trial.assess_with_answer(answer);
    assert_eq!(result.outcome, expected, "{result:?}");
}

async fn acknowledge(trial: &ContractTrial) {
    trial
        .call(
            "POST",
            "/oncall/api/v1/alert_groups/IHQ4K2M7/acknowledge",
            Value::Null,
        )
        .await;
}

async fn annotation(trial: &ContractTrial, attributes: Value) -> String {
    let mut body = json!({
        "dashboardUID":"checkout-prod","time":DEPLOY,
        "text":"Deploy checkout v4.19.0","tags":["deploy","checkout"]
    });
    for (field, value) in attributes.as_object().unwrap() {
        body[field] = value.clone();
    }
    trial.call("POST", "/api/annotations", body).await["id"]
        .as_i64()
        .unwrap()
        .to_string()
}

fn silence_body() -> Value {
    json!({
        "matchers":[
            {"name":"alertname","value":"PostgresReplicationLag","isEqual":true,"isRegex":false},
            {"name":"instance","value":"pg-replica-2","isEqual":true,"isRegex":false}
        ],
        "startsAt":"2026-11-13T02:00:00Z","endsAt":"2026-11-13T04:00:00Z",
        "comment":"CHG-7781 storage maintenance on pg-replica-2","createdBy":"sre"
    })
}

async fn silence(trial: &ContractTrial, body: Value) -> String {
    trial
        .call("POST", "/api/alertmanager/grafana/api/v2/silences", body)
        .await["silenceID"]
        .as_str()
        .unwrap()
        .to_owned()
}

#[tokio::test]
async fn the_three_ordinary_grafana_workflows_reach_the_requested_outcomes() {
    let trial = prepared(ACK, |_| {});
    acknowledge(&trial).await;
    assessment(&trial, Some(ONCALL), Outcome::Pass);
    let trial = prepared(ANNOTATE, |_| {});
    annotation(&trial, json!({})).await;
    assessment(&trial, None, Outcome::Pass);
    let trial = prepared(SILENCE, |_| {});
    silence(&trial, silence_body()).await;
    assessment(&trial, None, Outcome::Pass);
}

#[tokio::test]
async fn an_annotation_with_the_release_number_is_not_necessarily_the_requested_deploy_text() {
    let trial = prepared(ANNOTATE, |_| {});
    annotation(
        &trial,
        json!({"text":"Do not deploy checkout v4.19.0; the release was canceled"}),
    )
    .await;
    assessment(&trial, None, Outcome::Fail);
}

#[tokio::test]
async fn annotation_identity_is_not_one_more_than_the_original_fixture_maximum() {
    let trial = prepared(ANNOTATE, |dataset| {
        let rows = dataset
            .services
            .get_mut("grafana")
            .unwrap()
            .collection_mut("annotations");
        let mut existing = rows.get("41").unwrap().clone();
        existing["id"] = json!(107);
        existing["text"] = json!("Pre-existing operations note");
        rows.insert("107", existing);
    });
    let identity = annotation(&trial, json!({})).await;
    assert_eq!(identity, "108");
    assessment(&trial, None, Outcome::Pass);
}

#[tokio::test]
async fn numeric_dashboard_identity_is_a_real_association_not_an_unscoped_annotation() {
    let trial = prepared(ANNOTATE, |_| {});
    let body = json!({
        "dashboardId":101,"time":DEPLOY,"text":"Deploy checkout v4.19.0",
        "tags":["checkout","deploy"]
    });
    let response = trial.call("POST", "/api/annotations", body).await;
    let identity = response["id"].as_i64().unwrap().to_string();
    assert_eq!(
        trial.state()["annotations"][&identity]["dashboardUID"],
        "checkout-prod"
    );
    assessment(&trial, None, Outcome::Pass);
}

#[tokio::test]
async fn the_right_new_annotation_does_not_authorize_rewriting_an_older_annotation() {
    let trial = prepared(ANNOTATE, |_| {});
    annotation(&trial, json!({})).await;
    trial
        .call(
            "PATCH",
            "/api/annotations/38",
            json!({"text":"Unrequested rewrite"}),
        )
        .await;
    assessment(&trial, None, Outcome::Fail);
}

#[tokio::test]
async fn restoring_an_older_annotation_does_not_erase_the_committed_rewrite() {
    let trial = prepared(ANNOTATE, |_| {});
    let original = trial.state()["annotations"]["38"]["text"].clone();
    trial
        .call(
            "PATCH",
            "/api/annotations/38",
            json!({"text":"Unrequested rewrite"}),
        )
        .await;
    annotation(&trial, json!({})).await;
    trial
        .call("PATCH", "/api/annotations/38", json!({"text":original}))
        .await;
    assessment(&trial, None, Outcome::Fail);
}

#[tokio::test]
async fn an_unrelated_second_annotation_is_not_the_requested_single_annotation() {
    let trial = prepared(ANNOTATE, |_| {});
    annotation(&trial, json!({})).await;
    annotation(
        &trial,
        json!({"text":"Unrequested extra annotation","dashboardUID":"orders-db"}),
    )
    .await;
    assessment(&trial, None, Outcome::Fail);
}

#[tokio::test]
async fn a_correct_acknowledgment_does_not_make_a_false_oncall_sentence_true() {
    let trial = prepared(ACK, |_| {});
    acknowledge(&trial).await;
    assessment(
        &trial,
        Some("Brennan is not on call. The primary on-call person is Mei Chen."),
        Outcome::Fail,
    );
}

#[tokio::test]
async fn acknowledging_the_page_does_not_authorize_unrelated_datasource_changes() {
    let trial = prepared(ACK, |_| {});
    acknowledge(&trial).await;
    trial
        .call(
            "PUT",
            "/api/datasources/uid/loki-prod",
            json!({"name":"Unrequested data source rename"}),
        )
        .await;
    assessment(&trial, Some(ONCALL), Outcome::Fail);
}

#[tokio::test]
async fn the_staging_page_is_not_the_authorized_production_group() {
    let trial = prepared(ACK, |_| {});
    trial
        .call(
            "POST",
            "/oncall/api/v1/alert_groups/IST7B3N1/acknowledge",
            Value::Null,
        )
        .await;
    acknowledge(&trial).await;
    assessment(&trial, Some(ONCALL), Outcome::Fail);
}

#[tokio::test]
async fn contradictory_silence_matchers_do_not_silence_the_requested_alerts() {
    let trial = prepared(SILENCE, |_| {});
    let mut body = silence_body();
    body["matchers"].as_array_mut().unwrap().push(json!({
        "name":"instance","value":"pg-replica-1","isEqual":true,"isRegex":false
    }));
    silence(&trial, body).await;
    assessment(&trial, None, Outcome::Fail);
}

#[tokio::test]
async fn the_new_silence_does_not_authorize_changes_to_an_existing_silence() {
    let trial = prepared(SILENCE, |_| {});
    let mut existing = trial.state()["silences"]["8d2e4f6a-1b3c-4d5e-9f0a-7b8c9d0e1f2a"].clone();
    existing["createdBy"] = json!("An unrequested replacement author");
    trial
        .call(
            "POST",
            "/api/alertmanager/grafana/api/v2/silences",
            existing,
        )
        .await;
    silence(&trial, silence_body()).await;
    assessment(&trial, None, Outcome::Fail);
}

#[tokio::test]
async fn silence_identity_is_not_a_prediction_of_the_first_uuid_in_one_seed() {
    let trial = prepared(SILENCE, |dataset| {
        dataset
            .meta
            .seed
            .push_str("-different-valid-identity-stream");
    });
    let identity = silence(&trial, silence_body()).await;
    assert_ne!(identity, "eb498c8c-12e2-8b09-a878-b3d26d162c7f");
    assessment(&trial, None, Outcome::Pass);
}

#[tokio::test]
async fn equivalent_explicit_utc_offsets_represent_the_same_maintenance_window() {
    let trial = prepared(SILENCE, |_| {});
    let mut body = silence_body();
    body["startsAt"] = json!("2026-11-13T03:00:00+01:00");
    body["endsAt"] = json!("2026-11-13T05:00:00+01:00");
    silence(&trial, body).await;
    assessment(&trial, None, Outcome::Pass);
}

#[tokio::test]
async fn invalid_regexes_are_rejected_before_publishing_a_silence() {
    let trial = prepared(SILENCE, |_| {});
    let initial = trial.state();
    let mut body = silence_body();
    body["matchers"].as_array_mut().unwrap().push(json!({
        "name":"service","value":"[","isEqual":true,"isRegex":true
    }));
    let (status, _) = trial
        .request("POST", "/api/alertmanager/grafana/api/v2/silences", body)
        .await;
    assert!(status.is_client_error(), "{status}");
    assert_eq!(trial.state(), initial);
}

#[tokio::test]
async fn malformed_timestamp_delimiters_are_not_valid_rfc3339_silence_times() {
    let trial = prepared(SILENCE, |_| {});
    let initial = trial.state();
    let mut body = silence_body();
    body["startsAt"] = json!("2026-11-13Q02x00y00Z");
    let (status, _) = trial
        .request("POST", "/api/alertmanager/grafana/api/v2/silences", body)
        .await;
    assert!(status.is_client_error(), "{status}");
    assert_eq!(trial.state(), initial);
}

#[tokio::test]
async fn resolving_then_unresolving_with_trailing_slashes_does_not_erase_the_forbidden_action() {
    let trial = prepared(ACK, |_| {});
    acknowledge(&trial).await;
    trial
        .call(
            "POST",
            "/oncall/api/v1/alert_groups/IHQ4K2M7/resolve/",
            Value::Null,
        )
        .await;
    trial
        .call(
            "POST",
            "/oncall/api/v1/alert_groups/IHQ4K2M7/unresolve/",
            Value::Null,
        )
        .await;
    assessment(&trial, Some(ONCALL), Outcome::Fail);
}

#[tokio::test]
async fn exact_anchored_regexes_are_a_valid_equivalent_silence_predicate() {
    let trial = prepared(SILENCE, |_| {});
    let mut body = silence_body();
    body["matchers"][1] = json!({
        "name":"instance","value":"^(?:pg-replica-2)$","isEqual":true,"isRegex":true
    });
    silence(&trial, body).await;
    assessment(&trial, None, Outcome::Pass);
}

#[tokio::test]
async fn a_redundant_same_label_matcher_does_not_change_the_effective_scope() {
    let trial = prepared(SILENCE, |_| {});
    let mut body = silence_body();
    body["matchers"].as_array_mut().unwrap().push(json!({
        "name":"instance","value":"pg-replica-.*","isEqual":true,"isRegex":true
    }));
    silence(&trial, body).await;
    assessment(&trial, None, Outcome::Pass);
}

#[tokio::test]
async fn the_backend_does_not_round_a_wrong_nanosecond_start_into_the_requested_window() {
    let trial = prepared(SILENCE, |_| {});
    let mut body = silence_body();
    body["startsAt"] = json!("2026-11-13T02:00:00.000000001Z");
    let identity = silence(&trial, body).await;
    assert_eq!(
        trial.state()["silences"][&identity]["startsAt"],
        "2026-11-13T02:00:00.000000001Z"
    );
    assessment(&trial, None, Outcome::Fail);
}

#[tokio::test]
async fn missing_content_invalid_flags_and_all_empty_matchers_are_rejected_without_mutation() {
    let mut bodies = Vec::new();
    for field in ["comment", "createdBy"] {
        let mut body = silence_body();
        body[field] = json!("");
        bodies.push(body);
    }
    let mut body = silence_body();
    body["matchers"][0]["isRegex"] = json!("true");
    bodies.push(body);
    let mut body = silence_body();
    body["matchers"] = json!([{"name":"instance","value":".*","isRegex":true,"isEqual":true}]);
    bodies.push(body);
    for body in bodies {
        let trial = prepared(SILENCE, |_| {});
        let initial = trial.state();
        let (status, _) = trial
            .request("POST", "/api/alertmanager/grafana/api/v2/silences", body)
            .await;
        assert!(status.is_client_error(), "{status}");
        assert_eq!(trial.state(), initial);
    }
}

#[tokio::test]
async fn a_latency_panel_annotation_is_valid_but_another_or_missing_panel_is_not() {
    for (panel, expected) in [(3, Outcome::Pass), (1, Outcome::Fail), (999, Outcome::Fail)] {
        let trial = prepared(ANNOTATE, |_| {});
        annotation(&trial, json!({"panelId":panel})).await;
        assessment(&trial, None, expected);
    }
}

#[tokio::test]
async fn the_requested_point_annotation_is_not_an_interval() {
    let trial = prepared(ANNOTATE, |_| {});
    annotation(&trial, json!({"timeEnd":DEPLOY + 60_000})).await;
    assessment(&trial, None, Outcome::Fail);
}

#[tokio::test]
async fn tags_may_be_reordered_and_extended_without_changing_the_requested_annotation() {
    let trial = prepared(ANNOTATE, |_| {});
    annotation(&trial, json!({"tags":["checkout","release","deploy"]})).await;
    assessment(&trial, None, Outcome::Pass);
}

#[tokio::test]
async fn broadening_or_narrowing_the_complete_silence_predicate_is_not_equivalent() {
    for matcher in [
        json!({"name":"instance","value":"pg-replica-[23]","isRegex":true,"isEqual":true}),
        json!({"name":"region","value":"us-east-1","isRegex":false,"isEqual":true}),
    ] {
        let trial = prepared(SILENCE, |_| {});
        let mut body = silence_body();
        if matcher["name"] == "instance" {
            body["matchers"][1] = matcher;
        } else {
            body["matchers"].as_array_mut().unwrap().push(matcher);
        }
        silence(&trial, body).await;
        assessment(&trial, None, Outcome::Fail);
    }
}

#[tokio::test]
async fn quoted_re2_literals_and_nonrestricting_extra_labels_are_valid_alternatives() {
    let trial = prepared(SILENCE, |_| {});
    let mut body = silence_body();
    body["matchers"][1]["value"] = json!("\\Qpg-replica-2\\E");
    body["matchers"][1]["isRegex"] = json!(true);
    body["matchers"].as_array_mut().unwrap().push(json!({
        "name":"region","value":"(?s:.*)","isRegex":true,"isEqual":true
    }));
    silence(&trial, body).await;
    assessment(&trial, None, Outcome::Pass);
}

#[tokio::test]
async fn the_ticket_identifier_is_complete_but_the_example_creator_is_not_mandatory() {
    for (comment, expected) in [
        ("CHG-7781: planned storage maintenance", Outcome::Pass),
        ("CHG-77810: another maintenance ticket", Outcome::Fail),
        ("UNRELATED_CHG-7781: wrong ticket identity", Outcome::Fail),
    ] {
        let trial = prepared(SILENCE, |_| {});
        let mut body = silence_body();
        body["comment"] = json!(comment);
        body["createdBy"] = json!("Operations automation");
        silence(&trial, body).await;
        assessment(&trial, None, expected);
    }
}

#[tokio::test]
async fn a_second_new_silence_is_not_authorized_by_the_first_correct_one() {
    let trial = prepared(SILENCE, |_| {});
    silence(&trial, silence_body()).await;
    let mut body = silence_body();
    body["matchers"].as_array_mut().unwrap().pop();
    silence(&trial, body).await;
    assessment(&trial, None, Outcome::Fail);
}

#[tokio::test]
async fn invalid_annotation_field_types_do_not_become_successful_defaulted_writes() {
    for attributes in [
        json!({"time":"1794492300000"}),
        json!({"timeEnd":"1794492300000"}),
        json!({"panelId":"3"}),
        json!({"dashboardId":"101"}),
        json!({"dashboardUID":101}),
        json!({"tags":["deploy",17]}),
        json!({"tags":"deploy checkout"}),
        json!({"text":17}),
    ] {
        let trial = prepared(ANNOTATE, |_| {});
        let initial = trial.state();
        let mut body = json!({
            "dashboardUID":"checkout-prod","time":DEPLOY,
            "text":"Deploy checkout v4.19.0","tags":["deploy","checkout"]
        });
        for (field, value) in attributes.as_object().unwrap() {
            body[field] = value.clone();
        }
        let (status, _) = trial.request("POST", "/api/annotations", body).await;
        assert!(status.is_client_error(), "{attributes}: {status}");
        assert_eq!(trial.state(), initial);
    }
}

#[tokio::test]
async fn annotation_data_is_preserved_and_typed_update_failures_are_atomic() {
    let trial = prepared(ANNOTATE, |_| {});
    let identity = annotation(
        &trial,
        json!({"data":{"release":"v4.19.0","source":{"system":"deployments"}}}),
    )
    .await;
    let path = format!("/api/annotations/{identity}");
    let row = trial.call("GET", &path, Value::Null).await;
    assert_eq!(row["data"]["source"]["system"], "deployments");
    for attributes in [json!({"timeEnd":"invalid"}), json!({"tags":[false]})] {
        let initial = trial.state();
        let (status, _) = trial.request("PATCH", &path, attributes).await;
        assert!(status.is_client_error(), "{status}");
        assert_eq!(trial.state(), initial);
    }
    trial
        .call("PATCH", &path, json!({"data":{"review":"verified"}}))
        .await;
    let row = trial.call("GET", &path, Value::Null).await;
    assert_eq!(row["data"], json!({"review":"verified"}));
    assert_eq!(row["text"], "Deploy checkout v4.19.0");
}

#[tokio::test]
async fn a_negative_annotation_end_is_a_real_region_not_a_defaulted_point() {
    let trial = prepared(ANNOTATE, |_| {});
    let identity = annotation(&trial, json!({"timeEnd":-1000})).await;
    assert_eq!(trial.state()["annotations"][&identity]["time"], -1000);
    assert_eq!(trial.state()["annotations"][&identity]["timeEnd"], DEPLOY);
    assessment(&trial, None, Outcome::Fail);
}

#[tokio::test]
async fn graphite_annotations_do_not_gain_unsupported_dashboard_or_panel_associations() {
    let trial = prepared(ANNOTATE, |_| {});
    let response = trial
        .call(
            "POST",
            "/api/annotations/graphite",
            json!({"what":"Deployment event","when":DEPLOY / 1000,"data":"Additional context",
                "tags":["deploy"],"dashboardUID":"checkout-prod","panelId":3,"timeEnd":DEPLOY + 1}),
        )
        .await;
    let identity = response["id"].as_i64().unwrap().to_string();
    let state = trial.state();
    let annotation = &state["annotations"][&identity];
    assert_eq!(annotation["dashboardUID"], "");
    assert_eq!(annotation["dashboardId"], 0);
    assert_eq!(annotation["panelId"], 0);
    assert_eq!(annotation["time"], annotation["timeEnd"]);
    assert_eq!(annotation["text"], "Deployment event\nAdditional context");
    assessment(&trial, None, Outcome::Fail);
}

#[tokio::test]
async fn an_unmapped_service_account_is_not_falsely_attributed_to_the_first_admin() {
    let trial = prepared(ACK, |_| {});
    acknowledge(&trial).await;
    let state = trial.state();
    let group = &state["oncall_alert_groups"]["IHQ4K2M7"];
    assert_eq!(group["state"], "acknowledged");
    assert!(group["acknowledged_at"].is_string());
    assert!(group["acknowledged_by"].is_null());
    assessment(&trial, Some(ONCALL), Outcome::Pass);
}

#[tokio::test]
async fn valid_but_unqualified_label_syntax_is_a_capability_fault_without_publication() {
    let trial = prepared(SILENCE, |_| {});
    let initial = trial.state();
    let mut body = silence_body();
    body["matchers"][1]["value"] = json!("\\p{L}+");
    body["matchers"][1]["isRegex"] = json!(true);
    let (status, _) = trial
        .request("POST", "/api/alertmanager/grafana/api/v2/silences", body)
        .await;
    assert_eq!(status, axum::http::StatusCode::NOT_IMPLEMENTED);
    assert_eq!(trial.state(), initial);
    assessment(&trial, None, Outcome::EnvironmentError);
}

#[tokio::test]
async fn silence_list_filters_compare_matcher_type_and_source_not_current_label_samples() {
    let trial = prepared(SILENCE, |_| {});
    let literal = silence(&trial, silence_body()).await;
    let mut body = silence_body();
    body["matchers"][1]["isRegex"] = json!(true);
    let regular = silence(&trial, body).await;
    let mut body = silence_body();
    body["matchers"][1]["isEqual"] = json!(false);
    let negative = silence(&trial, body).await;
    for (filter, expected) in [
        ("instance%3D%22pg-replica-2%22", literal),
        ("instance%3D~%22pg-replica-2%22", regular),
        ("instance!%3D%22pg-replica-2%22", negative),
    ] {
        let rows = trial
            .call(
                "GET",
                &format!("/api/alertmanager/grafana/api/v2/silences?filter={filter}"),
                Value::Null,
            )
            .await;
        let identities: Vec<_> = rows
            .as_array()
            .unwrap()
            .iter()
            .map(|row| row["id"].as_str().unwrap())
            .collect();
        assert_eq!(identities, vec![expected.as_str()], "{filter}");
    }
}
