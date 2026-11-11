use std::{fs, path::Path, process::Command};

use catalog::Catalog;
use scenario::oracle_results::{self, ResultContract};
use serde_json::{Value, json};

const ACTOR: &str = "ops@octo-corp.example";
const FILE: &str = "actual-report-resource";
const BOOK: &str = "actual-tracker-resource";
const RECIPIENT: &str = "dana.whitfield@contoso.example";

fn envelope(text: &str) -> Value {
    json!({"isError":false,"content":[{"type":"text","text":text}],"structuredContent":{"result":text}})
}

fn check(tool: &str, arguments: Value, result: &Value) -> std::process::Output {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let catalog = Catalog::load(&root.join("catalog/services.toml")).unwrap();
    let directory = tempfile::tempdir().unwrap();
    for (name, value) in [
        (
            "request.json",
            json!({"jsonrpc":"2.0","id":2,"method":"tools/call",
            "params":{"name":tool,"arguments":arguments}}),
        ),
        (
            "contracts.json",
            oracle_results::registry(&catalog, &[]).unwrap(),
        ),
        ("result.json", result.clone()),
    ] {
        fs::write(directory.path().join(name), value.to_string()).unwrap();
    }
    Command::new("jq")
        .arg("-e")
        .arg("-L")
        .arg(root.join("crates/scenario/assets"))
        .args(["--arg", "endpoint", "7130", "--slurpfile", "request"])
        .arg(directory.path().join("request.json"))
        .args(["--slurpfile", "contracts"])
        .arg(directory.path().join("contracts.json"))
        .arg("-f")
        .arg(root.join("crates/scenario/assets/oracle-tool-result.jq"))
        .arg(directory.path().join("result.json"))
        .output()
        .unwrap()
}

fn accepted(tool: &str, arguments: Value, text: &str) -> bool {
    check(tool, arguments, &envelope(text)).status.success()
}

fn grant() -> String {
    format!(
        "Successfully shared 'Error: ordinary business report.pdf'\n\nPermission created:\n  - User: {RECIPIENT} (reader) [id: actual-permission]\n\nView link: https://drive.google.com/file/d/{FILE}/view?usp=drivesdk"
    )
}

fn grant_arguments() -> Value {
    json!({"action":"grant","file_id":FILE,"share_with":RECIPIENT,"role":"reader","share_type":"user"})
}

fn sheet(cells: usize, rows: usize, columns: usize) -> String {
    format!(
        "Successfully updated range 'Renewals!C2:D2' in spreadsheet {BOOK} for {ACTOR}. Updated: {cells} cells, {rows} rows, {columns} columns."
    )
}

fn sheet_arguments() -> Value {
    json!({"spreadsheet_id":BOOK,"range_name":"Renewals!C2:D2","values":[["2027-11-30","Renewed"]]})
}

fn event_url(identity: &str, calendar: &str) -> String {
    let encoded = Command::new("jq")
        .args([
            "-nr",
            "--arg",
            "identity",
            identity,
            "--arg",
            "calendar",
            calendar,
            r#"($identity + " " + $calendar) | @base64"#,
        ])
        .output()
        .unwrap();
    assert!(encoded.status.success());
    format!(
        "https://www.google.com/calendar/event?eid={}",
        String::from_utf8(encoded.stdout)
            .unwrap()
            .trim()
            .trim_end_matches('=')
    )
}

fn event_arguments() -> Value {
    json!({"action":"create","summary":"Error budget planning","add_google_meet":true})
}

fn event() -> String {
    format!(
        "Successfully created event 'Error budget planning' for {ACTOR}. Link: {}\nSaved start: 2026-11-03T14:00:00-05:00 [America/New_York; weekday: Tuesday; ISO weekday: 2]\nSaved end: 2026-11-03T14:45:00-05:00 [America/New_York; weekday: Tuesday; ISO weekday: 2]\nElapsed duration: 45 minutes (2700 seconds) Google Meet: https://meet.google.com/abc-defg-hij",
        event_url("actual-created-event", ACTOR)
    )
}

#[test]
fn selected_replica_tools_have_explicit_profiles_without_claiming_sdk_or_read_coverage() {
    for tool in [
        "manage_event",
        "modify_sheet_values",
        "manage_drive_access",
        "get_drive_file_permissions",
        "get_drive_shareable_link",
    ] {
        assert_eq!(
            ResultContract::for_tool("google-workspace", tool),
            ResultContract::GoogleWorkspaceReplicaV1
        );
    }
    for tool in [
        "search_gmail_messages",
        "get_gmail_message_content",
        "run_script_function",
    ] {
        assert_eq!(
            ResultContract::for_tool("google-workspace", tool),
            ResultContract::McpEnvelopeV1
        );
    }
    assert_eq!(
        ResultContract::for_tool("other", "manage_event"),
        ResultContract::McpEnvelopeV1
    );
}

#[test]
fn real_replica_confirmation_shapes_are_not_fabricated_sdk_json_objects() {
    for (tool, arguments, text) in [
        ("manage_drive_access", grant_arguments(), grant()),
        ("modify_sheet_values", sheet_arguments(), sheet(2, 1, 2)),
        ("manage_event", event_arguments(), event()),
    ] {
        let result = check(tool, arguments, &envelope(&text));
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
    }
}

#[test]
fn envelopes_must_agree_and_actual_business_error_words_are_preserved() {
    let success = envelope(&sheet(2, 1, 2));
    for (pointer, value) in [
        ("/structuredContent/result", json!("different")),
        ("/structuredContent/result", json!({"updatedCells":2})),
        ("/content", json!([])),
        ("/content", json!([{"type":"image","data":"not text"}])),
        ("/isError", json!(true)),
        ("/isError", json!("false")),
    ] {
        let mut malformed = success.clone();
        *malformed.pointer_mut(pointer).unwrap() = value;
        assert!(
            !check("modify_sheet_values", sheet_arguments(), &malformed)
                .status
                .success(),
            "{pointer}"
        );
    }
    let mut without_flag = success;
    without_flag.as_object_mut().unwrap().remove("isError");
    assert!(
        check("modify_sheet_values", sheet_arguments(), &without_flag)
            .status
            .success()
    );
}

#[test]
fn sheet_counts_and_printed_resource_range_are_bound_to_the_requested_nonnull_cells() {
    assert!(accepted(
        "modify_sheet_values",
        sheet_arguments(),
        &sheet(2, 1, 2)
    ));
    for text in [
        sheet(0, 0, 0),
        sheet(3, 1, 2),
        sheet(2, 2, 2),
        sheet(2, 1, 1),
        sheet(2, 1, 2).replace(BOOK, "wrong-book"),
        sheet(2, 1, 2).replace("C2:D2", "C3:D3"),
    ] {
        assert!(
            !accepted("modify_sheet_values", sheet_arguments(), &text),
            "{text}"
        );
    }
}

#[test]
fn sheet_null_skip_empty_rows_and_json_string_routes_keep_their_actual_counts() {
    for values in [
        json!([["2027-11-30", null], [], [null, "Renewed"]]),
        json!("[[\"2027-11-30\",null],[],[null,\"Renewed\"]]"),
    ] {
        let mut arguments = sheet_arguments();
        arguments["values"] = values;
        assert!(accepted("modify_sheet_values", arguments, &sheet(2, 2, 2)));
    }
    let mut arguments = sheet_arguments();
    arguments["values"] = json!([[null, null]]);
    assert!(accepted("modify_sheet_values", arguments, &sheet(0, 0, 0)));
}

#[test]
fn sheet_clear_confirms_the_requested_resource_without_demanding_an_update_count() {
    let text =
        format!("Successfully cleared range 'Renewals!C2:D2' in spreadsheet {BOOK} for {ACTOR}.");
    assert!(accepted(
        "modify_sheet_values",
        json!({"spreadsheet_id":BOOK,"range_name":"Renewals!C2:D2","clear_values":true}),
        &text
    ));
    assert!(accepted(
        "modify_sheet_values",
        json!({"spreadsheet_id":BOOK,"range_name":"C2:D2","clear_values":true}),
        &text
    ));
    assert!(!accepted(
        "modify_sheet_values",
        json!({"spreadsheet_id":BOOK,"range_name":"C3:D3","clear_values":true}),
        &text
    ));
}

#[test]
fn drive_single_grants_bind_the_printed_principal_role_and_file_url() {
    assert!(accepted("manage_drive_access", grant_arguments(), &grant()));
    for text in [
        grant().replace(RECIPIENT, "other@example.test"),
        grant().replace("(reader)", "(writer)"),
        grant().replace(FILE, "wrong-resource"),
        grant().replace("drive.google.com", "drive.google.com.attacker.example"),
        grant().replace("actual-permission", ""),
    ] {
        assert!(
            !accepted("manage_drive_access", grant_arguments(), &text),
            "{text}"
        );
    }
}

#[test]
fn drive_batch_partial_failures_missing_rows_and_substituted_recipients_do_not_pass() {
    let arguments = json!({"action":"grant_batch","file_id":FILE,"recipients":[
        {"email":RECIPIENT,"role":"reader"},{"email":"other@example.test","role":"commenter"}]});
    let text = format!(
        "Batch share results for 'Error budget.pdf'\n\nSummary: 2 succeeded, 0 failed\n\nResults:\n  - User: {RECIPIENT} (reader) [id: first]\n  - User: other@example.test (commenter) [id: second]\n\nView link: https://drive.google.com/file/d/{FILE}/view"
    );
    assert!(accepted("manage_drive_access", arguments.clone(), &text));
    for invalid in [
        text.replace("2 succeeded, 0 failed", "1 succeeded, 1 failed"),
        text.replace("other@example.test", "wrong@example.test"),
        text.replace(
            "\n  - User: other@example.test (commenter) [id: second]",
            "",
        ),
    ] {
        assert!(!accepted(
            "manage_drive_access",
            arguments.clone(),
            &invalid
        ));
    }
}

#[test]
fn existing_permission_corrections_and_revocations_use_returned_not_predicted_identities() {
    let updated = format!(
        "Successfully updated permission on 'Report'\n\nUpdated permission:\n  - User: {RECIPIENT} (reader) [id: actual-existing]"
    );
    assert!(accepted(
        "manage_drive_access",
        json!({"action":"update","file_id":FILE,
        "permission_id":"actual-existing","role":"reader"}),
        &updated
    ));
    assert!(!accepted(
        "manage_drive_access",
        json!({"action":"update","file_id":FILE,
        "permission_id":"predicted","role":"reader"}),
        &updated
    ));
    let revoked = "Successfully removed permission from 'Report'\n\nPermission ID 'actual-existing' has been revoked.";
    assert!(accepted(
        "manage_drive_access",
        json!({"action":"revoke","file_id":FILE,
        "permission_id":"actual-existing"}),
        revoked
    ));
    assert!(!accepted(
        "manage_drive_access",
        json!({"action":"revoke","file_id":FILE,
        "permission_id":"predicted"}),
        revoked
    ));
}

#[test]
fn calendar_creation_binds_summary_and_resolved_calendar_but_not_a_seeded_event_id() {
    assert!(accepted("manage_event", event_arguments(), &event()));
    for text in [
        event().replace("Error budget planning", "Wrong title"),
        event().replace(
            &event_url("actual-created-event", ACTOR),
            &event_url("actual-created-event", "other@example.test"),
        ),
        event().replace(
            "https://meet.google.com/abc-defg-hij",
            "https://wrong.example/video",
        ),
        event().replace("\nSaved end:", "\nMissing end:"),
    ] {
        assert!(
            !accepted("manage_event", event_arguments(), &text),
            "{text}"
        );
    }
    let changed = event().replace(
        &event_url("actual-created-event", ACTOR),
        &event_url("a-different-actual-returned-id", ACTOR),
    );
    assert!(accepted("manage_event", event_arguments(), &changed));
}

#[test]
fn calendar_notifications_are_not_invented_as_printed_delivery_or_completion_fields() {
    for mode in ["all", "externalOnly", "none"] {
        let mut arguments = event_arguments();
        arguments["send_updates"] = json!(mode);
        assert!(accepted("manage_event", arguments, &event()));
    }
}

#[test]
fn a_shared_calendar_without_meet_keeps_the_actual_link_and_optional_fields() {
    let calendar = "team@group.calendar.google.com";
    let text = event()
        .replace(
            &event_url("actual-created-event", ACTOR),
            &event_url("actual-created-event", calendar),
        )
        .replace(" Google Meet: https://meet.google.com/abc-defg-hij", "");
    let mut arguments = event_arguments();
    arguments["calendar_id"] = json!(calendar);
    arguments["add_google_meet"] = json!(false);
    assert!(accepted("manage_event", arguments, &text));
}

#[test]
fn calendar_updates_bind_actual_ids_even_when_the_summary_is_omitted() {
    let text = event().replace(
        "Successfully created event 'Error budget planning'",
        "Successfully modified event 'Error budget planning' (ID: actual-created-event)",
    );
    for summary in [None, Some("Error budget planning")] {
        let mut arguments =
            json!({"action":"update","event_id":"actual-created-event","add_google_meet":true});
        if let Some(summary) = summary {
            arguments["summary"] = json!(summary);
        }
        assert!(accepted("manage_event", arguments, &text));
    }
    assert!(!accepted(
        "manage_event",
        json!({"action":"update","event_id":"other","add_google_meet":true}),
        &text
    ));
}

#[test]
fn calendar_deletion_and_rsvp_are_acknowledgments_not_provider_or_guest_receipt_proof() {
    let text = format!(
        "Successfully deleted event (ID: actual-event) from calendar 'primary' for {ACTOR}."
    );
    assert!(accepted(
        "manage_event",
        json!({"action":"delete","event_id":"actual-event"}),
        &text
    ));
    assert!(!accepted(
        "manage_event",
        json!({"action":"delete","event_id":"other"}),
        &text
    ));
    let rsvp = format!(
        "Successfully updated RSVP for 'Planning' (ID: actual-event) to 'tentative' for {ACTOR}."
    );
    assert!(accepted(
        "manage_event",
        json!({"action":"rsvp","event_id":"actual-event","response":"tentative"}),
        &rsvp
    ));
}

#[test]
fn permission_read_error_strings_are_not_success_despite_an_ok_mcp_envelope() {
    assert!(!accepted(
        "get_drive_file_permissions",
        json!({"file_id":FILE}),
        "Error getting file permissions: resource not found"
    ));
    let text = format!(
        "File: Error budget report.pdf\nID: {FILE}\nType: application/pdf\nSharing Status:\n  Shared: False\n  Number of permissions: 1\n  Permissions:\n    - User: {ACTOR} (owner) [id: actual-owner]\nURLs:\n  View Link: https://drive.google.com/file/d/{FILE}/view\n❌ This file is NOT shared with 'Anyone with the link'"
    );
    assert!(accepted(
        "get_drive_file_permissions",
        json!({"file_id":FILE}),
        &text
    ));
    assert!(!accepted(
        "get_drive_file_permissions",
        json!({"file_id":"different"}),
        &text
    ));
    assert!(!accepted(
        "get_drive_file_permissions",
        json!({"file_id":FILE}),
        &text.replace("Number of permissions: 1", "Number of permissions: 2")
    ));
}

#[test]
fn calendar_timing_summaries_and_suffixes_cannot_hide_failed_or_inconsistent_completion() {
    for text in [
        event().replace("2700 seconds", "2701 seconds"),
        event().replace(") Google Meet:", ") operation failed Google Meet:"),
        event() + "\nError: provisioning failed",
    ] {
        assert!(!accepted("manage_event", event_arguments(), &text));
    }
}

#[test]
fn replica_blank_optional_defaults_do_not_invent_a_different_actor_calendar_or_role() {
    let mut arguments = event_arguments();
    arguments["calendar_id"] = json!("  ");
    assert!(accepted("manage_event", arguments, &event()));
    let mut arguments = grant_arguments();
    arguments["role"] = json!("");
    arguments["share_type"] = json!("");
    assert!(accepted("manage_drive_access", arguments, &grant()));
}

#[test]
fn shareable_links_require_the_actual_resource_and_not_a_success_looking_url_substitute() {
    let text = format!(
        "File: Error budget.pdf\nID: {FILE}\nType: application/pdf\nShared: False\n\nLinks:\n  View: https://drive.google.com/file/d/{FILE}/view\n  Download: https://drive.google.com/uc?id={FILE}&export=download"
    );
    assert!(accepted(
        "get_drive_shareable_link",
        json!({"file_id":FILE}),
        &text
    ));
    for invalid in [
        text.replace("  View: ", ""),
        text.replace("/file/d/actual-report-resource/", "/file/d/wrong-resource/"),
        text.replace("drive.google.com", "attacker.example"),
    ] {
        assert!(!accepted(
            "get_drive_shareable_link",
            json!({"file_id":FILE}),
            &invalid
        ));
    }
}
