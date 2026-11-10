mod support;

use mock_core::{dataset::Dataset, grading::Outcome};
use serde_json::{Value, json};
use support::ContractTrial;

const COVER: &str = "ops/pagerduty/cover-shift-override";
const HANDOFF: &str = "ops/pagerduty/reassign-replication-lag";
const RESOLVE: &str = "ops/pagerduty/resolve-prod-checkout";
const REPLICA: &str = "Q3K8ORDBLAG2RX";
const CHECKOUT: &str = "Q7CHKAP15PCT1D";
const HANDOFF_NOTE: &str = "Handing off to DBRE: replica-2 disk is at 100%, needs a volume resize.";
const RESOLVE_NOTE: &str = "Resolved by rolling back release 2026.10.29.3.";

fn prepared(id: &str, prepare: impl FnOnce(&mut Dataset)) -> ContractTrial {
    let actor = if id == RESOLVE {
        "lena.weber@tidewater.io"
    } else {
        "alex.kim@tidewater.io"
    };
    ContractTrial::load_with_dataset(
        id,
        &mock_pagerduty::Pagerduty,
        &[
            ("authorization", "Token token=local-fixture-credential"),
            ("from", actor),
        ],
        prepare,
    )
}

fn assessment(trial: &ContractTrial, expected: Outcome) {
    let result = trial.assess();
    assert_eq!(result.outcome, expected, "{result:?}");
}

async fn update(trial: &ContractTrial, incident: &str, attributes: Value) {
    let mut input = json!({"id":incident,"type":"incident_reference"});
    for (field, value) in attributes.as_object().unwrap() {
        input[field] = value.clone();
    }
    trial
        .call("PUT", "/incidents", json!({"incidents":[input]}))
        .await;
}

async fn note(trial: &ContractTrial, incident: &str, content: &str) -> String {
    trial
        .call(
            "POST",
            &format!("/incidents/{incident}/notes"),
            json!({"note":{"content":content}}),
        )
        .await["note"]["id"]
        .as_str()
        .unwrap()
        .to_owned()
}

async fn handoff(trial: &ContractTrial, content: &str) {
    update(
        trial,
        REPLICA,
        json!({"assignments":[{"assignee":{"id":"PDNGYN2","type":"user_reference"}}]}),
    )
    .await;
    note(trial, REPLICA, content).await;
}

async fn resolve(trial: &ContractTrial, content: &str) {
    note(trial, CHECKOUT, content).await;
    update(trial, CHECKOUT, json!({"status":"resolved"})).await;
}

fn override_input(start: &str, end: &str) -> Value {
    json!({"start":start,"end":end,"user":{"id":"PJ0RLEE","type":"user_reference"}})
}

async fn overrides(trial: &ContractTrial, schedule: &str, inputs: Value) -> Value {
    trial
        .call(
            "POST",
            &format!("/schedules/{schedule}/overrides"),
            json!({"overrides":inputs}),
        )
        .await
}

async fn cover(trial: &ContractTrial) {
    overrides(
        trial,
        "PSPLTPR",
        json!([override_input(
            "2026-10-31T09:00:00Z",
            "2026-10-31T17:00:00Z"
        )]),
    )
    .await;
}

#[tokio::test]
async fn the_three_requested_workflows_pass_without_prescribing_discovery_calls() {
    let trial = prepared(COVER, |_| {});
    cover(&trial).await;
    assessment(&trial, Outcome::Pass);
    let trial = prepared(HANDOFF, |_| {});
    handoff(&trial, HANDOFF_NOTE).await;
    assessment(&trial, Outcome::Pass);
    let trial = prepared(RESOLVE, |_| {});
    resolve(&trial, RESOLVE_NOTE).await;
    assessment(&trial, Outcome::Pass);
}

#[tokio::test]
async fn adjacent_shift_segments_are_equivalent_to_one_override() {
    let trial = prepared(COVER, |_| {});
    overrides(
        &trial,
        "PSPLTPR",
        json!([
            override_input("2026-10-31T09:00:00Z", "2026-10-31T13:00:00Z"),
            override_input("2026-10-31T13:00:00Z", "2026-10-31T17:00:00Z")
        ]),
    )
    .await;
    assessment(&trial, Outcome::Pass);
}

#[tokio::test]
async fn a_correct_shift_does_not_authorize_an_extra_payments_override() {
    let trial = prepared(COVER, |_| {});
    cover(&trial).await;
    overrides(
        &trial,
        "PSPAYPR",
        json!([override_input(
            "2026-10-31T09:00:00Z",
            "2026-10-31T17:00:00Z"
        )]),
    )
    .await;
    assessment(&trial, Outcome::Fail);
}

#[tokio::test]
async fn fractional_shift_boundaries_are_not_rounded_into_a_correct_window() {
    let trial = prepared(COVER, |_| {});
    let (status, payload) = trial
        .request(
            "POST",
            "/schedules/PSPLTPR/overrides",
            json!({"overrides":[override_input(
                "2026-10-31T09:00:00.9Z",
                "2026-10-31T17:00:00.9Z"
            )]}),
        )
        .await;
    assert_eq!(status.as_u16(), 201);
    assert_eq!(payload[0]["override"]["start"], "2026-10-31T09:00:00.9Z");
    assert_eq!(payload[0]["override"]["end"], "2026-10-31T17:00:00.9Z");
    assessment(&trial, Outcome::Fail);
}

#[tokio::test]
async fn equivalent_offset_shift_instants_are_accepted_without_losing_precision() {
    let trial = prepared(COVER, |_| {});
    overrides(
        &trial,
        "PSPLTPR",
        json!([override_input(
            "2026-10-31T04:00:00.000000000-05:00",
            "2026-10-31T12:00:00-05:00"
        )]),
    )
    .await;
    assessment(&trial, Outcome::Pass);
}

#[tokio::test]
async fn a_handoff_note_must_communicate_the_complete_requested_message() {
    let trial = prepared(HANDOFF, |_| {});
    handoff(&trial, "replica-2").await;
    assert_ne!(trial.assess().outcome, Outcome::Pass);
}

#[tokio::test]
async fn acknowledgement_before_reassignment_does_not_disappear_from_history() {
    let trial = prepared(HANDOFF, |_| {});
    update(&trial, REPLICA, json!({"status":"acknowledged"})).await;
    handoff(&trial, HANDOFF_NOTE).await;
    assert_eq!(trial.state()["incidents"][REPLICA]["status"], "triggered");
    assessment(&trial, Outcome::Fail);
}

#[tokio::test]
async fn a_correct_handoff_does_not_authorize_lower_urgency() {
    let trial = prepared(HANDOFF, |_| {});
    update(&trial, REPLICA, json!({"urgency":"low"})).await;
    handoff(&trial, HANDOFF_NOTE).await;
    assessment(&trial, Outcome::Fail);
}

#[tokio::test]
async fn a_release_token_does_not_establish_truthful_rollback_communication() {
    let trial = prepared(RESOLVE, |_| {});
    resolve(
        &trial,
        "Release 2026.10.29.3 was not rolled back; this incident is still broken.",
    )
    .await;
    assessment(&trial, Outcome::Fail);
}

#[tokio::test]
async fn resolving_production_does_not_authorize_resolving_another_open_incident() {
    let trial = prepared(RESOLVE, |_| {});
    update(&trial, "Q0AP1GW5XX9LMZ", json!({"status":"resolved"})).await;
    resolve(&trial, RESOLVE_NOTE).await;
    assessment(&trial, Outcome::Fail);
}

#[tokio::test]
async fn note_and_requested_action_may_be_completed_in_either_order() {
    let trial = prepared(HANDOFF, |_| {});
    note(&trial, REPLICA, HANDOFF_NOTE).await;
    update(
        &trial,
        REPLICA,
        json!({"assignments":[{"assignee":{"id":"PDNGYN2"}}]}),
    )
    .await;
    assessment(&trial, Outcome::Pass);
    let trial = prepared(RESOLVE, |_| {});
    update(&trial, CHECKOUT, json!({"status":"resolved"})).await;
    note(&trial, CHECKOUT, RESOLVE_NOTE).await;
    assessment(&trial, Outcome::Pass);
}

#[tokio::test]
async fn target_only_acknowledgement_then_resolution_is_not_a_forbidden_extra_step() {
    let trial = prepared(RESOLVE, |_| {});
    update(&trial, CHECKOUT, json!({"status":"acknowledged"})).await;
    resolve(&trial, RESOLVE_NOTE).await;
    assessment(&trial, Outcome::Pass);
}

#[tokio::test]
async fn overlapping_and_repeated_cover_records_are_routing_equivalent_inside_the_exact_window() {
    let trial = prepared(COVER, |_| {});
    cover(&trial).await;
    overrides(
        &trial,
        "PSPLTPR",
        json!([override_input(
            "2026-10-31T12:00:00Z",
            "2026-10-31T15:00:00Z"
        )]),
    )
    .await;
    cover(&trial).await;
    assessment(&trial, Outcome::Pass);
}

#[tokio::test]
async fn separate_out_of_order_split_writes_need_complete_coverage_only_at_completion() {
    let trial = prepared(COVER, |_| {});
    overrides(
        &trial,
        "PSPLTPR",
        json!([override_input(
            "2026-10-31T13:00:00Z",
            "2026-10-31T17:00:00Z"
        )]),
    )
    .await;
    overrides(
        &trial,
        "PSPLTPR",
        json!([override_input(
            "2026-10-31T09:00:00Z",
            "2026-10-31T13:00:00Z"
        )]),
    )
    .await;
    assessment(&trial, Outcome::Pass);
}

#[tokio::test]
async fn coverage_must_have_no_gap_extension_or_wrong_replacement() {
    for inputs in [
        json!([
            override_input("2026-10-31T09:00:00Z", "2026-10-31T13:00:00Z"),
            override_input("2026-10-31T13:00:01Z", "2026-10-31T17:00:00Z")
        ]),
        json!([override_input(
            "2026-10-31T08:59:59Z",
            "2026-10-31T17:00:00Z"
        )]),
        json!([override_input(
            "2026-10-31T09:00:00Z",
            "2026-10-31T17:00:01Z"
        )]),
        json!([{"start":"2026-10-31T09:00:00Z","end":"2026-10-31T17:00:00Z",
            "user":{"id":"PJ0RALV","type":"user_reference"}}]),
    ] {
        let trial = prepared(COVER, |_| {});
        overrides(&trial, "PSPLTPR", inputs).await;
        assessment(&trial, Outcome::Fail);
    }
}

#[tokio::test]
async fn actual_override_allocation_does_not_depend_on_the_first_child_slot() {
    let trial = prepared(COVER, |dataset| {
        dataset
            .services
            .get_mut("pagerduty")
            .unwrap()
            .collection_mut("overrides")
            .insert(
                "PSPLTPR/10",
                json!({"id":"POLDOV10","start":"2026-10-28T09:00:00Z",
                    "end":"2026-10-28T17:00:00Z","user":{"id":"PA1EXKM","type":"user_reference"}}),
            );
    });
    let before = trial.state()["overrides"]["PSPLTPR/10"].clone();
    cover(&trial).await;
    assert_eq!(trial.state()["overrides"]["PSPLTPR/10"], before);
    assert_eq!(trial.state()["overrides"].as_object().unwrap().len(), 3);
    assessment(&trial, Outcome::Pass);
}

#[tokio::test]
async fn naive_fractional_times_follow_the_declared_utc_policy_without_rounding() {
    for start in ["2026-10-31T09:00:00.9", "2026-10-31 09:00:00.9"] {
        let trial = prepared(COVER, |_| {});
        let result = overrides(
            &trial,
            "PSPLTPR",
            json!([override_input(start, "2026-10-31T17:00:00Z")]),
        )
        .await;
        assert_eq!(result[0]["override"]["start"], "2026-10-31T09:00:00.9Z");
        assessment(&trial, Outcome::Fail);
    }
}

#[tokio::test]
async fn unrepresentable_rest_precision_never_rounds_into_a_committed_success() {
    for start in [
        "2026-10-31T09:00:00.0000000001Z",
        "2026-10-31T09:00:00.0000000000Z",
    ] {
        let trial = prepared(COVER, |_| {});
        let before = trial.state()["overrides"].clone();
        let (status, _) = trial
            .request(
                "POST",
                "/schedules/PSPLTPR/overrides",
                json!({"overrides":[
                    override_input("2026-10-31T09:00:00Z", "2026-10-31T13:00:00Z"),
                    override_input(start, "2026-10-31T17:00:00Z")
                ]}),
            )
            .await;
        assert_eq!(status.as_u16(), 501);
        assert_eq!(trial.state()["overrides"], before);
        assessment(&trial, Outcome::EnvironmentError);
    }
}

#[tokio::test]
async fn subsecond_query_times_are_precise_reads_not_capability_faults() {
    let trial = prepared(COVER, |_| {});
    cover(&trial).await;
    let (status, response) = trial
        .request(
            "GET",
            "/oncalls?schedule_ids%5B%5D=PSPLTPR&since=2026-10-31T09:00:00.5Z&until=2026-10-31T17:00:00Z",
            Value::Null,
        )
        .await;
    assert_eq!(status.as_u16(), 200);
    assert!(!response["oncalls"].as_array().unwrap().is_empty());
    assert!(
        response["oncalls"]
            .as_array()
            .unwrap()
            .iter()
            .all(|row| row["user"]["id"] == "PJ0RLEE")
    );
    assessment(&trial, Outcome::Pass);
}

#[tokio::test]
async fn ordinary_invalid_override_items_still_report_partial_commit_inside_http_success() {
    let trial = prepared(COVER, |_| {});
    let result = overrides(
        &trial,
        "PSPLTPR",
        json!([
            override_input("2026-10-31T09:00:00Z", "2026-10-31T17:00:00Z"),
            override_input("2026-10-31T13:00:00Z", "2026-10-31T13:00:00Z")
        ]),
    )
    .await;
    assert_eq!(result[0]["status"], 201);
    assert_eq!(result[1]["status"], 400);
    assert!(!result[1]["errors"].as_array().unwrap().is_empty());
    assessment(&trial, Outcome::Pass);
}

#[tokio::test]
async fn exact_note_repetition_is_admitted_without_fabricating_retry_lineage() {
    let trial = prepared(HANDOFF, |_| {});
    handoff(&trial, HANDOFF_NOTE).await;
    note(&trial, REPLICA, HANDOFF_NOTE).await;
    assessment(&trial, Outcome::Pass);
    let trial = prepared(RESOLVE, |_| {});
    resolve(&trial, RESOLVE_NOTE).await;
    note(&trial, CHECKOUT, RESOLVE_NOTE).await;
    assessment(&trial, Outcome::Pass);
}

#[tokio::test]
async fn reviewed_resolution_paraphrases_pass_but_unknown_wording_requires_review() {
    let trial = prepared(RESOLVE, |_| {});
    resolve(
        &trial,
        "Rolling back release 2026.10.29.3 resolved this incident.",
    )
    .await;
    assessment(&trial, Outcome::Pass);
    let trial = prepared(RESOLVE, |_| {});
    resolve(&trial, "The production fix looks good after the revert.").await;
    assessment(&trial, Outcome::Unresolved);
}

#[tokio::test]
async fn old_note_rows_do_not_force_the_new_note_to_use_child_one() {
    let trial = prepared(HANDOFF, |dataset| {
        dataset
            .services
            .get_mut("pagerduty")
            .unwrap()
            .collection_mut("notes")
            .insert(
                "Q3K8ORDBLAG2RX/8",
                json!({"id":"POLDNOTE8","content":"Prior investigation",
                    "user":{"id":"PA1EXKM","type":"user_reference"},
                    "created_at":"2026-10-30T13:20:00Z"}),
            );
    });
    handoff(&trial, HANDOFF_NOTE).await;
    assert_eq!(
        trial.state()["notes"]["Q3K8ORDBLAG2RX/8"]["content"],
        "Prior investigation"
    );
    assessment(&trial, Outcome::Pass);
}

#[tokio::test]
async fn wrong_primary_or_an_additional_assignee_is_not_a_complete_handoff() {
    for assignees in [
        json!([{"assignee":{"id":"PS4MOKF"}}]),
        json!([{"assignee":{"id":"PDNGYN2"}},{"assignee":{"id":"PA1EXKM"}}]),
    ] {
        let trial = prepared(HANDOFF, |_| {});
        update(&trial, REPLICA, json!({"assignments":assignees})).await;
        note(&trial, REPLICA, HANDOFF_NOTE).await;
        assessment(&trial, Outcome::Fail);
    }
}

#[tokio::test]
async fn restoring_extra_assignees_or_urgency_does_not_erase_the_unauthorized_commit() {
    for attributes in [
        json!({"assignments":[{"assignee":{"id":"PDNGYN2"}},{"assignee":{"id":"PA1EXKM"}}]}),
        json!({"urgency":"low"}),
    ] {
        let trial = prepared(HANDOFF, |_| {});
        update(&trial, REPLICA, attributes).await;
        update(&trial, REPLICA, json!({"urgency":"high"})).await;
        handoff(&trial, HANDOFF_NOTE).await;
        assessment(&trial, Outcome::Fail);
    }
}

#[tokio::test]
async fn note_only_or_acknowledgement_only_is_not_resolution() {
    let trial = prepared(RESOLVE, |_| {});
    note(&trial, CHECKOUT, RESOLVE_NOTE).await;
    assessment(&trial, Outcome::Fail);
    update(&trial, CHECKOUT, json!({"status":"acknowledged"})).await;
    assessment(&trial, Outcome::Fail);
}

#[tokio::test]
async fn correct_resolution_does_not_authorize_notes_on_an_unrelated_incident() {
    let trial = prepared(RESOLVE, |_| {});
    resolve(&trial, RESOLVE_NOTE).await;
    note(&trial, "Q9CHKWEB5XX3VK", RESOLVE_NOTE).await;
    assessment(&trial, Outcome::Fail);
}
