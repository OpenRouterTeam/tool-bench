mod support;

use mock_core::{dataset::Dataset, grading::Outcome};
use serde_json::{Value, json};
use support::ContractTrial;

const RENEW: &str = "collab/google/mark-northwind-renewed";
const PLAN: &str = "collab/google/schedule-q1-planning";
const BOOK: &str = "1p91PmGrfYlol_BkGN9Op8j9dWzibNwgg";
const OLD_BOOK: &str = "1_x8cbfTBtuujAmxfx9ZG2B5yGE5HDyhh";
const Q4: &str = "ae2a3372cbeee330eecbe86872";
const PRIYA: &str = "priya.natarajan@octo-corp.example";
const MARCUS: &str = "marcus.webb@octo-corp.example";

fn prepared(id: &str, prepare: impl FnOnce(&mut Dataset)) -> ContractTrial {
    ContractTrial::load_with_dataset(id, &mock_google::Google, &[], prepare)
}

async fn tool(trial: &ContractTrial, name: &str, arguments: Value) -> Value {
    trial
        .call(
            "POST",
            "/_mcp/tools/call",
            json!({"name":name,"arguments":arguments}),
        )
        .await
}

async fn cells(trial: &ContractTrial, book: &str, range: &str, values: Value) {
    tool(
        trial,
        "modify_sheet_values",
        json!({"spreadsheet_id":book,"range_name":range,"values":values}),
    )
    .await;
}

async fn renewed(trial: &ContractTrial) {
    cells(
        trial,
        BOOK,
        "Renewals!C2:D2",
        json!([["2027-11-30", "Renewed"]]),
    )
    .await;
}

async fn meeting(trial: &ContractTrial, changes: Value) -> String {
    let before = trial.state()["calendar_events"].clone();
    let mut arguments = json!({
        "action":"create","summary":"Q1 planning sync",
        "start_time":"2026-11-03T14:00:00","end_time":"2026-11-03T14:45:00",
        "timezone":"America/New_York","attendees":[PRIYA,MARCUS],"add_google_meet":true
    });
    for (field, value) in changes.as_object().unwrap() {
        if value.is_null() {
            arguments.as_object_mut().unwrap().remove(field);
        } else {
            arguments[field] = value.clone();
        }
    }
    tool(trial, "manage_event", arguments).await;
    trial.state()["calendar_events"]
        .as_object()
        .unwrap()
        .keys()
        .find(|identity| before.get(identity.as_str()).is_none())
        .unwrap()
        .clone()
}

fn assessment(trial: &ContractTrial, expected: Outcome) {
    let actual = trial.assess();
    assert_eq!(actual.outcome, expected, "{actual:?}");
}

#[tokio::test]
async fn ordinary_renewal_and_meeting_outcomes_pass() {
    let trial = prepared(RENEW, |_| {});
    renewed(&trial).await;
    assessment(&trial, Outcome::Pass);
    let trial = prepared(PLAN, |_| {});
    meeting(&trial, json!({})).await;
    assessment(&trial, Outcome::Pass);
}

#[tokio::test]
async fn separate_precise_cell_updates_are_equivalent() {
    let trial = prepared(RENEW, |_| {});
    cells(&trial, BOOK, "Renewals!D2", json!([["Renewed"]])).await;
    cells(&trial, BOOK, "Renewals!C2", json!([["2027-11-30"]])).await;
    assessment(&trial, Outcome::Pass);
}

#[tokio::test]
async fn a_preserved_full_target_row_rewrite_is_a_legitimate_plan() {
    let trial = prepared(RENEW, |_| {});
    cells(
        &trial,
        BOOK,
        "Renewals!A2:E2",
        json!([[
            "Northwind Traders",
            "Lena Fischer",
            "2027-11-30",
            "Renewed",
            "Quote received 10/14"
        ]]),
    )
    .await;
    assessment(&trial, Outcome::Pass);
}

#[tokio::test]
async fn changing_the_target_owner_is_not_a_renewal_update() {
    let trial = prepared(RENEW, |_| {});
    renewed(&trial).await;
    cells(&trial, BOOK, "Renewals!B2", json!([["Mallory"]])).await;
    assessment(&trial, Outcome::Fail);
}

#[tokio::test]
async fn an_unasserted_neighbor_is_still_protected() {
    let trial = prepared(RENEW, |_| {});
    renewed(&trial).await;
    cells(&trial, BOOK, "Renewals!C4", json!([["2040-01-01"]])).await;
    assessment(&trial, Outcome::Fail);
}

#[tokio::test]
async fn restored_neighbor_edits_do_not_erase_history() {
    let trial = prepared(RENEW, |_| {});
    cells(&trial, BOOK, "Renewals!C4", json!([["2040-01-01"]])).await;
    cells(&trial, BOOK, "Renewals!C4", json!([["2027-03-31"]])).await;
    renewed(&trial).await;
    assessment(&trial, Outcome::Fail);
}

#[tokio::test]
async fn archive_tab_changes_are_not_authorized() {
    let trial = prepared(RENEW, |_| {});
    renewed(&trial).await;
    cells(&trial, BOOK, "Archive!D2", json!([["Renewed"]])).await;
    assessment(&trial, Outcome::Fail);
}

#[tokio::test]
async fn even_identical_old_tracker_writes_are_not_requested_metadata_changes() {
    let trial = prepared(RENEW, |_| {});
    cells(&trial, OLD_BOOK, "Renewals!C2", json!([["2025-11-30"]])).await;
    renewed(&trial).await;
    assessment(&trial, Outcome::Fail);
}

#[tokio::test]
async fn the_business_row_is_selected_by_identity_not_its_initial_position() {
    let trial = prepared(RENEW, |dataset| {
        let files = dataset
            .services
            .get_mut("google")
            .unwrap()
            .collection_mut("drive_files");
        let mut book = files.get(BOOK).unwrap().clone();
        book["sheets"][0]["values"]
            .as_array_mut()
            .unwrap()
            .swap(1, 3);
        files.insert(BOOK, book);
    });
    cells(
        &trial,
        BOOK,
        "Renewals!C4:D4",
        json!([["2027-11-30", "Renewed"]]),
    )
    .await;
    assessment(&trial, Outcome::Pass);
}
#[tokio::test]
async fn a_constant_date_formula_is_a_logical_date_not_literal_formula_text() {
    let trial = prepared(RENEW, |_| {});
    cells(
        &trial,
        BOOK,
        "Renewals!C2:D2",
        json!([["=DATE(2027,11,30)", "Renewed"]]),
    )
    .await;
    assessment(&trial, Outcome::Pass);
}

#[tokio::test]
async fn date_rollover_and_decimal_arguments_follow_spreadsheet_rules() {
    for formula in [
        "=DATE(2027,12,0)",
        "=DATE(2027.9,11.9,30.9)",
        "=DATEVALUE(\"2027-11-30\")",
    ] {
        let trial = prepared(RENEW, |_| {});
        cells(
            &trial,
            BOOK,
            "Renewals!C2:D2",
            json!([[formula, "Renewed"]]),
        )
        .await;
        assessment(&trial, Outcome::Pass);
    }
}

#[tokio::test]
async fn clearing_only_the_authorized_cells_before_filling_them_is_safe() {
    let trial = prepared(RENEW, |_| {});
    tool(
        &trial,
        "modify_sheet_values",
        json!({
            "spreadsheet_id":BOOK,"range_name":"Renewals!C2:D2","clear_values":true
        }),
    )
    .await;
    renewed(&trial).await;
    assessment(&trial, Outcome::Pass);
}

#[tokio::test]
async fn an_incorrect_date_claim_is_not_erased_by_restoration() {
    let trial = prepared(RENEW, |_| {});
    cells(&trial, BOOK, "Renewals!C2", json!([["2040-01-01"]])).await;
    renewed(&trial).await;
    assessment(&trial, Outcome::Fail);
}

#[tokio::test]
async fn skipped_null_cells_preserve_the_existing_logical_date() {
    let trial = prepared(RENEW, |dataset| {
        let files = dataset
            .services
            .get_mut("google")
            .unwrap()
            .collection_mut("drive_files");
        let mut book = files.get(BOOK).unwrap().clone();
        book["sheets"][0]["values"][1][2] = json!("2027-11-30");
        files.insert(BOOK, book);
    });
    cells(
        &trial,
        BOOK,
        "Renewals!C2:D2",
        json!([[null, "Renewed"]]).to_string().into(),
    )
    .await;
    assessment(&trial, Outcome::Pass);
}

#[tokio::test]
async fn invalid_range_errors_can_be_recovered_without_hidden_header_damage() {
    let trial = prepared(RENEW, |_| {});
    let before = trial.state();
    let (_, response) = trial
        .request(
            "POST",
            "/_mcp/tools/call",
            json!({
                "name":"modify_sheet_values","arguments":{
                    "spreadsheet_id":BOOK,"range_name":"Renewals!C0","values":[["2027-11-30"]]
                }
            }),
        )
        .await;
    assert_eq!(response["isError"], true);
    assert_eq!(trial.state(), before);
    renewed(&trial).await;
    assessment(&trial, Outcome::Pass);
}

#[tokio::test]
async fn an_unqualified_formula_is_not_silent_literal_text_or_partial_success() {
    let trial = prepared(RENEW, |_| {});
    let before = trial.state();
    let (_, response) = trial
        .request(
            "POST",
            "/_mcp/tools/call",
            json!({
                "name":"modify_sheet_values","arguments":{
                    "spreadsheet_id":BOOK,"range_name":"Renewals!C2:D2",
                    "values":[["=EOMONTH(DATE(2027,11,1),0)","Renewed"]]
                }
            }),
        )
        .await;
    assert_eq!(response["isError"], true);
    assert_eq!(trial.state(), before);
    assessment(&trial, Outcome::EnvironmentError);
}

#[tokio::test]
async fn locale_appropriate_date_input_is_not_a_literal_iso_requirement() {
    let trial = prepared(RENEW, |_| {});
    cells(
        &trial,
        BOOK,
        "Renewals!C2:D2",
        json!([["11/30/2027", "Renewed"]]),
    )
    .await;
    assessment(&trial, Outcome::Pass);
}

#[tokio::test]
async fn a_numeric_serial_from_the_json_string_route_is_a_valid_date() {
    let trial = prepared(RENEW, |_| {});
    cells(
        &trial,
        BOOK,
        "Renewals!C2:D2",
        json!([[46721, "Renewed"]]).to_string().into(),
    )
    .await;
    assessment(&trial, Outcome::Pass);
}

#[tokio::test]
async fn reordered_guests_and_optional_reminders_are_legitimate() {
    let trial = prepared(PLAN, |_| {});
    meeting(
        &trial,
        json!({
            "attendees":[MARCUS,PRIYA],"reminders":[{"method":"popup","minutes":10}]
        }),
    )
    .await;
    assessment(&trial, Outcome::Pass);
}

#[tokio::test]
async fn equivalent_utc_instants_are_not_wrong_local_time_strings() {
    let trial = prepared(PLAN, |_| {});
    meeting(
        &trial,
        json!({
            "start_time":"2026-11-03T19:00:00Z","end_time":"2026-11-03T19:45:00Z",
            "timezone":"UTC"
        }),
    )
    .await;
    assessment(&trial, Outcome::Pass);
}

#[tokio::test]
async fn a_real_iana_alias_resolves_the_requested_civil_time() {
    let trial = prepared(PLAN, |_| {});
    meeting(&trial, json!({"timezone":"US/Eastern"})).await;
    assessment(&trial, Outcome::Pass);
}

#[tokio::test]
async fn a_fractional_wrong_start_is_not_truncated_into_success() {
    let trial = prepared(PLAN, |_| {});
    meeting(&trial, json!({"start_time":"2026-11-03T14:00:00.001"})).await;
    assessment(&trial, Outcome::Fail);
}

#[tokio::test]
async fn an_existing_display_zone_does_not_invent_native_input_conversion_on_update() {
    let trial = prepared(PLAN, |_| {});
    let identity = meeting(&trial, json!({})).await;
    tool(
        &trial,
        "manage_event",
        json!({
            "action":"update","event_id":identity,"start_time":"2026-11-03T14:00:00.000000999-05:00"
        }),
    )
    .await;
    assessment(&trial, Outcome::Fail);
}

#[tokio::test]
async fn an_extra_guest_is_not_authorized_by_the_two_requested_names() {
    let trial = prepared(PLAN, |_| {});
    meeting(
        &trial,
        json!({"attendees":[PRIYA,MARCUS,"unrelated@contoso.example"]}),
    )
    .await;
    assessment(&trial, Outcome::Fail);
}

#[tokio::test]
async fn mentioning_the_title_inside_a_contradictory_title_is_not_exact() {
    let trial = prepared(PLAN, |_| {});
    meeting(
        &trial,
        json!({"summary":"Cancelled: do not attend Q1 planning sync"}),
    )
    .await;
    assessment(&trial, Outcome::Fail);
}

#[tokio::test]
async fn a_recurring_series_is_not_the_requested_single_meeting() {
    let trial = prepared(PLAN, |_| {});
    meeting(&trial, json!({"recurrence":["RRULE:FREQ=WEEKLY;COUNT=10"]})).await;
    assessment(&trial, Outcome::Fail);
}

#[tokio::test]
async fn a_canceled_new_event_is_not_a_scheduled_meeting() {
    let trial = prepared(PLAN, |_| {});
    let identity = meeting(&trial, json!({})).await;
    tool(
        &trial,
        "manage_event",
        json!({"action":"delete","event_id":identity}),
    )
    .await;
    assessment(&trial, Outcome::Fail);
}

#[tokio::test]
async fn only_preserving_the_old_meetings_start_is_insufficient() {
    let trial = prepared(PLAN, |_| {});
    meeting(&trial, json!({})).await;
    tool(
        &trial,
        "manage_event",
        json!({
            "action":"update","event_id":Q4,"summary":"Unauthorized replacement"
        }),
    )
    .await;
    assessment(&trial, Outcome::Fail);
}

#[tokio::test]
async fn duplicate_meetings_do_not_satisfy_exactly_one_request() {
    let trial = prepared(PLAN, |_| {});
    meeting(&trial, json!({})).await;
    meeting(&trial, json!({})).await;
    assessment(&trial, Outcome::Fail);
}

#[tokio::test]
async fn a_meet_type_label_without_a_video_entry_point_is_not_completion() {
    let trial = prepared(PLAN, |_| {});
    meeting(
        &trial,
        json!({
            "add_google_meet":null,
            "conference_data":{"conferenceSolution":{"key":{"type":"hangoutsMeet"}}}
        }),
    )
    .await;
    assessment(&trial, Outcome::Fail);
}

#[tokio::test]
async fn unqualified_raw_conference_provisioning_declines_before_mutation() {
    let trial = prepared(PLAN, |_| {});
    let before = trial.state();
    let (_, response) = trial
        .request(
            "POST",
            "/_mcp/tools/call",
            json!({
                "name":"manage_event","arguments":{
                    "action":"create","summary":"Q1 planning sync",
                    "start_time":"2026-11-03T14:00:00","end_time":"2026-11-03T14:45:00",
                    "timezone":"America/New_York","attendees":[PRIYA,MARCUS],
                    "conference_data":{"createRequest":{
                        "requestId":"a-reviewed-request-identity",
                        "conferenceSolutionKey":{"type":"hangoutsMeet"}
                    }}
                }
            }),
        )
        .await;
    assert_eq!(response["isError"], true);
    assert_eq!(trial.state(), before);
    assessment(&trial, Outcome::EnvironmentError);
}

#[tokio::test]
async fn disabling_guest_email_does_not_remove_the_organizer_event_or_its_attendees() {
    let trial = prepared(PLAN, |_| {});
    meeting(&trial, json!({"send_updates":"none"})).await;
    assessment(&trial, Outcome::Pass);
}

#[tokio::test]
async fn external_only_notifications_do_not_infer_calendar_enrollment_from_email_domains() {
    let trial = prepared(PLAN, |_| {});
    let identity = meeting(&trial, json!({"send_updates":"externalOnly"})).await;
    assessment(&trial, Outcome::Pass);
    let state = trial.state();
    let requests = state["calendar_notification_requests"].as_object().unwrap();
    assert_eq!(requests.len(), 1);
    let request = requests.values().next().unwrap();
    assert_eq!(request["eventId"], identity);
    assert_eq!(request["mode"], "externalOnly");
    assert_eq!(request["delivery"], "unobserved");
    assert_eq!(request["attendees"], json!([MARCUS, PRIYA]));
    assert!(request.get("recipient").is_none());
    assert!(request.get("recipients").is_none());
}

#[tokio::test]
async fn a_safe_create_then_complete_plan_does_not_need_one_reference_shaped_call() {
    let trial = prepared(PLAN, |_| {});
    let identity = meeting(&trial, json!({"attendees":[],"add_google_meet":false})).await;
    tool(
        &trial,
        "manage_event",
        json!({
            "action":"update","event_id":identity,"attendees":[PRIYA,MARCUS],"add_google_meet":true
        }),
    )
    .await;
    assessment(&trial, Outcome::Pass);
}

#[tokio::test]
async fn a_verified_existing_meet_link_can_be_supplied_as_the_event_location() {
    let uri = "https://meet.google.com/abc-defg-hij";
    let trial = prepared(PLAN, |dataset| {
        let events = dataset
            .services
            .get_mut("google")
            .unwrap()
            .collection_mut("calendar_events");
        let mut previous = events.get(Q4).unwrap().clone();
        previous["conferenceData"] = json!({
            "conferenceId":"existing-room","conferenceSolution":{"key":{"type":"hangoutsMeet"}},
            "entryPoints":[{"entryPointType":"video","uri":uri}]
        });
        previous["hangoutLink"] = json!(uri);
        events.insert(Q4, previous);
    });
    meeting(&trial, json!({"add_google_meet":false,"location":uri})).await;
    assessment(&trial, Outcome::Pass);
}

#[tokio::test]
async fn a_silent_meet_update_need_not_resend_the_ready_link_to_every_guest() {
    let trial = prepared(PLAN, |_| {});
    let identity = meeting(&trial, json!({"add_google_meet":false})).await;
    tool(
        &trial,
        "manage_event",
        json!({
            "action":"update","event_id":identity,"add_google_meet":true,"send_updates":"none"
        }),
    )
    .await;
    assessment(&trial, Outcome::Pass);
}

#[tokio::test]
async fn an_invalid_time_update_does_not_leave_an_orphan_conference() {
    let trial = prepared(PLAN, |_| {});
    let identity = meeting(&trial, json!({})).await;
    let before = trial.state();
    let (_, response) = trial
        .request(
            "POST",
            "/_mcp/tools/call",
            json!({
                "name":"manage_event","arguments":{
                    "action":"update","event_id":identity,"end_time":"2026-11-03T18:00:00Z",
                    "add_google_meet":true
                }
            }),
        )
        .await;
    assert_eq!(response["isError"], true);
    assert_eq!(trial.state(), before);
    assessment(&trial, Outcome::Pass);
}

#[tokio::test]
async fn an_allocated_id_from_a_failed_create_does_not_predict_the_actual_event() {
    let trial = prepared(PLAN, |_| {});
    let (_, rejected) = trial.request("POST", "/_mcp/tools/call", json!({
        "name":"manage_event","arguments":{
            "action":"create","summary":"Q1 planning sync","start_time":"2026-11-03T14:00:00",
            "end_time":"2026-11-03T14:45:00","timezone":"America/New_York",
            "add_google_meet":true,"conference_data":{}
        }
    })).await;
    assert_eq!(rejected["isError"], true);
    let identity = meeting(&trial, json!({})).await;
    assert_ne!(identity, "445b671f493e8a06958e4ff0dd");
    assessment(&trial, Outcome::Pass);
}
