use std::{fs, path::Path, process::Command};

use catalog::Catalog;
use scenario::{
    SolutionCall,
    oracle_results::{self, AllowedApiError, ResultContract},
};
use serde_json::{Value, json};

const COMMAND: &str = "aws ec2 describe-instances";
const NEGATIVE: &str = "aws s3api get-bucket-policy --bucket policy-audit-bucket";
const CODE: &str = "NoSuchBucketPolicy";

fn datadog_result(tool: &str, arguments: Value, text: &str) -> std::process::Output {
    check(
        json!({"jsonrpc":"2.0","id":2,"method":"tools/call",
            "params":{"name":tool,"arguments":arguments}}),
        registry(&[]),
        &json!({"content":[{"type":"text","text":text}]}),
        "7112",
    )
}

fn datadog_schedule() -> Value {
    json!({
        "id":9876543,"scope":["host:pg-prod-replica-2"],"monitorId":7301,
        "start":1793412000,"end":1793419200,"disabled":false,"canceled":null,
        "message":"Rebuild CHG-5012 after error logs","timezone":"UTC","recurrence":null
    })
}

fn datadog_arguments() -> Value {
    json!({
        "scope":"host:pg-prod-replica-2","monitorId":7301,
        "start":1793412000,"end":1793419200,
        "message":"Rebuild CHG-5012 after error logs","timezone":"UTC"
    })
}

#[test]
fn datadog_write_profiles_are_explicit_without_inventing_read_profiles() {
    for tool in ["schedule_downtime", "cancel_downtime", "mute_host"] {
        assert_eq!(
            ResultContract::for_tool("datadog", tool),
            ResultContract::DatadogApiV1
        );
    }
    assert_eq!(
        ResultContract::for_tool("datadog", "get_logs"),
        ResultContract::McpEnvelopeV1
    );
    assert_eq!(
        registry(&[])["profiles"]["7112"]["schedule_downtime"],
        "datadog_api_v1"
    );
}

#[test]
fn datadog_cancel_requires_the_pinned_request_bound_void_completion_message() {
    assert!(
        datadog_result(
            "cancel_downtime",
            json!({"downtimeId":2207731}),
            "Cancelled downtime with ID: 2207731",
        )
        .status
        .success()
    );
    for message in [
        "Cancelled downtime with ID: 2207752",
        "Error cancelling downtime",
        "{\"errors\":[\"permission denied\"]}",
        "",
    ] {
        assert!(
            !datadog_result("cancel_downtime", json!({"downtimeId":2207731}), message,)
                .status
                .success()
        );
    }
    assert!(
        !datadog_result(
            "cancel_downtime",
            json!({"downtimeId":2207731.5}),
            "Cancelled downtime with ID: 2207731.5",
        )
        .status
        .success()
    );
}

#[test]
fn datadog_host_completion_preserves_business_error_words_and_actual_request_fields() {
    let arguments = json!({
        "hostname":"checkout-api-prod-1b","end":1793383200,"message":"Error logs INC-2291"
    });
    let payload = json!({
        "status":"success",
        "message":"Host checkout-api-prod-1b has been muted successfully with message: Error logs INC-2291 until 2026-10-30T18:00:00.000Z"
    });
    assert!(
        datadog_result("mute_host", arguments.clone(), &payload.to_string())
            .status
            .success()
    );
    for field in ["hostname", "message"] {
        let mut changed = arguments.clone();
        changed[field] = json!("different");
        assert!(
            !datadog_result("mute_host", changed, &payload.to_string())
                .status
                .success()
        );
    }
    let mut changed = arguments;
    changed["end"] = json!(1793383199);
    assert!(
        !datadog_result("mute_host", changed, &payload.to_string())
            .status
            .success()
    );
    let payload = json!({"status":"success","message":"Host checkout-api-prod-1b has been muted successfully"});
    for arguments in [
        json!({"hostname":"checkout-api-prod-1b"}),
        json!({"hostname":"checkout-api-prod-1b","message":"","end":0}),
    ] {
        assert!(
            datadog_result("mute_host", arguments, &payload.to_string())
                .status
                .success()
        );
    }
}

#[test]
fn datadog_host_expiry_format_matches_actual_javascript_date_clipping_and_extended_years() {
    for end in [
        -8640000000000_i64,
        -62167219201,
        -62167219200,
        -1,
        1,
        1793383200,
        253402300799,
        253402300800,
        8640000000000,
    ] {
        let actual = Command::new("node")
            .args([
                "-e",
                "process.stdout.write(new Date(Number(process.argv[1])*1000).toISOString())",
                "--",
                &end.to_string(),
            ])
            .output()
            .unwrap();
        assert!(actual.status.success(), "{actual:?}");
        let instant = String::from_utf8(actual.stdout).unwrap();
        let payload = json!({
            "status":"success","message":format!("Host host-1 has been muted successfully until {instant}")
        });
        let checked = datadog_result(
            "mute_host",
            json!({"hostname":"host-1","end":end}),
            &payload.to_string(),
        );
        assert!(
            checked.status.success(),
            "end={end} {}",
            String::from_utf8_lossy(&checked.stderr)
        );
    }
    assert!(
        !datadog_result(
            "mute_host",
            json!({"hostname":"host-1","end":8640000000001_i64}),
            "{\"status\":\"success\",\"message\":\"unqualified time\"}",
        )
        .status
        .success()
    );
}

#[test]
fn datadog_schedule_uses_actual_ids_camel_case_and_semantically_equivalent_scope_lists() {
    let payload = datadog_schedule();
    let text = format!("Scheduled downtime: {}", payload);
    assert!(
        datadog_result("schedule_downtime", datadog_arguments(), &text)
            .status
            .success()
    );
    let mut arguments = datadog_arguments();
    arguments["scope"] = json!("host:pg-prod-replica-2,env:prod");
    let mut payload = payload;
    payload["scope"] = json!(["env:prod", "host:pg-prod-replica-2"]);
    payload["id"] = json!(814923);
    let checked = datadog_result(
        "schedule_downtime",
        arguments,
        &format!("Scheduled downtime: {}", payload),
    );
    assert!(
        checked.status.success(),
        "{}",
        String::from_utf8_lossy(&checked.stderr)
    );
}

#[test]
fn datadog_schedule_binding_rejects_wrong_intervals_scope_monitor_and_failure_envelopes() {
    for (field, value) in [
        ("start", json!(1793412000.75)),
        ("end", json!(1793419201)),
        ("scope", json!(["host:pg-prod-replica-1"])),
        ("monitorId", json!(7302)),
        ("id", json!("not-a-numeric-id")),
        ("disabled", json!(true)),
        ("recurrence", json!({"type":"days","period":1})),
    ] {
        let mut payload = datadog_schedule();
        payload[field] = value;
        assert!(
            !datadog_result(
                "schedule_downtime",
                datadog_arguments(),
                &format!("Scheduled downtime: {}", payload),
            )
            .status
            .success(),
            "{field}"
        );
    }
    for text in [
        "{\"errors\":[\"bad monitor\"]}",
        "Scheduled downtime: {\"errors\":[\"bad monitor\"]}",
        "Scheduled downtime: not-json",
    ] {
        assert!(
            !datadog_result("schedule_downtime", datadog_arguments(), text)
                .status
                .success()
        );
    }
}

#[test]
fn datadog_recurring_business_negatives_can_complete_but_dropped_until_is_not_certified() {
    let mut arguments = datadog_arguments();
    arguments["recurrence"] = json!({"type":"days","period":1});
    let mut payload = datadog_schedule();
    payload["recurrence"] = json!({"type":"days","period":1,"weekDays":null});
    let text = format!("Scheduled downtime: {}", payload);
    assert!(
        datadog_result("schedule_downtime", arguments.clone(), &text)
            .status
            .success()
    );
    arguments["recurrence"]["until"] = json!(1793505600);
    assert!(
        !datadog_result("schedule_downtime", arguments, &text)
            .status
            .success()
    );
}

#[test]
fn datadog_contradictory_structured_metadata_cannot_override_the_pinned_text_result() {
    let request = json!({"jsonrpc":"2.0","id":2,"method":"tools/call",
        "params":{"name":"cancel_downtime","arguments":{"downtimeId":2207731}}});
    let result = json!({
        "content":[{"type":"text","text":"Cancelled downtime with ID: 2207731"}],
        "structuredContent":{"success":false}
    });
    assert!(
        !check(request, registry(&[]), &result, "7112")
            .status
            .success()
    );
}

fn call(commands: Value) -> SolutionCall {
    serde_json::from_value(json!({
        "endpoint":"aws-api","tool":"call_aws","arguments":{"cli_command":commands}
    }))
    .unwrap()
}

fn registry(calls: &[SolutionCall]) -> Value {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let catalog = Catalog::load(&root.join("catalog/services.toml")).unwrap();
    oracle_results::registry(&catalog, calls).unwrap()
}

fn permission() -> AllowedApiError {
    AllowedApiError {
        command: NEGATIVE.into(),
        status: 404,
        code: CODE.into(),
    }
}

fn result(commands: &[&str]) -> Value {
    json!({
        "isError":false,"content":[{"type":"text","text":"Error: requested log data"}],
        "structuredContent":{"result":commands.iter().map(|command| json!({
            "cli_command":command,"response":{"status_code":200,"error":null,"error_code":null,
                "as_json":"{\"error\":\"ordinary business data\"}"},
            "validation_failures":null,"missing_context_failures":null,"failed_constraints":[]
        })).collect::<Vec<_>>()}
    })
}

fn check(request: Value, contracts: Value, result: &Value, endpoint: &str) -> std::process::Output {
    let directory = tempfile::tempdir().unwrap();
    for (filename, value) in [
        ("request.json", &request),
        ("contracts.json", &contracts),
        ("result.json", result),
    ] {
        fs::write(
            directory.path().join(filename),
            serde_json::to_vec(value).unwrap(),
        )
        .unwrap();
    }
    Command::new("jq")
        .arg("-L")
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("assets"))
        .args([
            "-e",
            "--arg",
            "endpoint",
            endpoint,
            "--slurpfile",
            "request",
        ])
        .arg(directory.path().join("request.json"))
        .args(["--slurpfile", "contracts"])
        .arg(directory.path().join("contracts.json"))
        .arg("-f")
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/oracle-tool-result.jq"))
        .arg(directory.path().join("result.json"))
        .output()
        .unwrap()
}

fn request(commands: Value) -> Value {
    json!({"jsonrpc":"2.0","id":2,"method":"tools/call",
        "params":{"name":"call_aws","arguments":{"cli_command":commands}}})
}

fn grafana_result(tool: &str, arguments: Value, payload: Value) -> std::process::Output {
    let request = json!({"jsonrpc":"2.0","id":2,"method":"tools/call",
        "params":{"name":tool,"arguments":arguments}});
    check(
        request,
        registry(&[]),
        &json!({"content":[{"type":"text","text":payload.to_string()}]}),
        "7114",
    )
}

fn pagerduty_result(tool: &str, arguments: Value, payload: Value) -> std::process::Output {
    let (content, structured) = if tool == "create_schedule_override" {
        (
            payload
                .as_array()
                .unwrap()
                .iter()
                .map(|item| json!({"type":"text","text":item.to_string()}))
                .collect::<Vec<_>>(),
            json!({"result":payload}),
        )
    } else {
        (
            vec![json!({"type":"text","text":payload.to_string()})],
            payload,
        )
    };
    check(
        json!({"jsonrpc":"2.0","id":2,"method":"tools/call",
            "params":{"name":tool,"arguments":arguments}}),
        registry(&[]),
        &json!({"isError":false,"content":content,"structuredContent":structured}),
        "7111",
    )
}

fn pagerduty_incident() -> Value {
    json!({"id":"actual-incident","incident_number":4812,"title":"Error: ordinary incident text",
        "status":"triggered","service":{"id":"actual-service"},
        "created_at":"2026-10-30T13:12:00Z","updated_at":"2026-10-30T14:00:00Z",
        "assignments":[{"at":"2026-10-30T14:00:00Z","assignee":{"id":"actual-user"}}]})
}

fn pagerduty_override() -> Value {
    json!({"status":201,"override":{"id":"actual-new-override",
        "start":"2026-10-31T09:00:00Z","end":"2026-10-31T17:00:00Z",
        "user":{"id":"actual-user","type":"user_reference"}}})
}

fn pagerduty_override_arguments() -> Value {
    json!({"schedule_id":"actual-schedule","override_request":{"overrides":[{
        "start":"2026-10-31T04:00:00-05:00","end":"2026-10-31T18:00:00+01:00",
        "user_id":"actual-user"}]}})
}

#[test]
fn pagerduty_override_statuses_and_every_suboperation_bind_to_actual_requests() {
    assert_eq!(
        ResultContract::for_tool("pagerduty", "create_schedule_override"),
        ResultContract::PagerdutyApiV1
    );
    let successful = pagerduty_result(
        "create_schedule_override",
        pagerduty_override_arguments(),
        json!([pagerduty_override()]),
    );
    assert!(successful.status.success(), "{successful:?}");
    for fields in [
        json!({"status":400,"errors":["invalid interval"]}),
        json!({"status":"201"}),
        json!({"errors":["suboperation failed"]}),
        json!({"override":{"id":"","start":"2026-10-31T09:00:00Z","end":"2026-10-31T17:00:00Z","user":{"id":"actual-user"}}}),
        json!({"override":{"id":"other-id","start":"2026-10-31T09:00:00.000000001Z","end":"2026-10-31T17:00:00Z","user":{"id":"actual-user"}}}),
        json!({"override":{"id":"other-id","start":"2026-10-31T09:00:00Z","end":"2026-10-31T17:00:00Z","user":{"id":"wrong-user"}}}),
    ] {
        let mut payload = pagerduty_override();
        for (field, value) in fields.as_object().unwrap() {
            payload[field] = value.clone();
        }
        let checked = pagerduty_result(
            "create_schedule_override",
            pagerduty_override_arguments(),
            json!([payload]),
        );
        assert!(!checked.status.success(), "{fields}: {checked:?}");
        assert!(checked.stdout.is_empty());
    }
    let mut arguments = pagerduty_override_arguments();
    let requested = arguments["override_request"]["overrides"][0].clone();
    arguments["override_request"]["overrides"]
        .as_array_mut()
        .unwrap()
        .push(requested);
    let checked = pagerduty_result(
        "create_schedule_override",
        arguments,
        json!([pagerduty_override(), {"status":400,"errors":["invalid interval"]}]),
    );
    assert!(!checked.status.success(), "{checked:?}");
}

#[test]
fn pagerduty_override_request_binding_is_not_an_undocumented_result_order_requirement() {
    let mut arguments = pagerduty_override_arguments();
    let mut second_request = arguments["override_request"]["overrides"][0].clone();
    second_request["user_id"] = json!("another-user");
    arguments["override_request"]["overrides"]
        .as_array_mut()
        .unwrap()
        .push(second_request);
    let mut second_result = pagerduty_override();
    second_result["override"]["id"] = json!("second-real-id");
    second_result["override"]["user"]["id"] = json!("another-user");
    let checked = pagerduty_result(
        "create_schedule_override",
        arguments,
        json!([second_result, pagerduty_override()]),
    );
    assert!(checked.status.success(), "{checked:?}");
}

#[test]
fn pagerduty_timestamp_binding_compares_calendar_instants_without_rounding_or_process_timezones() {
    for (requested, returned) in [
        ("2024-02-29T23:30:00-05:00", "2024-03-01T04:30:00Z"),
        ("1969-12-31T19:00:00-05:00", "1970-01-01T00:00:00Z"),
        ("2026-10-31T09:00:00", "2026-10-31T09:00:00Z"),
        (
            "2026-10-31T09:00:00.123456789Z",
            "2026-10-31T04:00:00.123456-05:00",
        ),
    ] {
        let mut arguments = pagerduty_override_arguments();
        arguments["override_request"]["overrides"][0]["start"] = json!(requested);
        arguments["override_request"]["overrides"][0]["end"] = json!("2026-10-31T17:00:00Z");
        let mut payload = pagerduty_override();
        payload["override"]["start"] = json!(returned);
        let checked = pagerduty_result("create_schedule_override", arguments, json!([payload]));
        assert!(
            checked.status.success(),
            "{requested} {returned}: {checked:?}"
        );
    }
    for instant in [
        "2026-02-29T09:00:00Z",
        "2026-13-01T09:00:00Z",
        "2026-10-31T24:00:00Z",
        "2026-10-31T09:00:60Z",
        "2026-10-31T09:00:00+24:00",
        "2026-10-31T09:00:00.1234567890Z",
        "2026-10-31T17:00:00Z",
        "2026-10-31T17:00:01Z",
    ] {
        let mut arguments = pagerduty_override_arguments();
        arguments["override_request"]["overrides"][0]["start"] = json!(instant);
        let mut payload = pagerduty_override();
        payload["override"]["start"] = json!(instant);
        let checked = pagerduty_result("create_schedule_override", arguments, json!([payload]));
        assert!(!checked.status.success(), "{instant}: {checked:?}");
    }
}

#[test]
fn pagerduty_result_binding_uses_the_actual_pinned_datetime_normalization() {
    for (requested, returned) in [
        ("2026-10-31T09:00:00.000000001Z", "2026-10-31T09:00:00Z"),
        (
            "2026-10-31T09:00:00.123456789Z",
            "2026-10-31T09:00:00.123456Z",
        ),
        (
            "2026-10-31T09:00:00.999999999Z",
            "2026-10-31T09:00:00.999999Z",
        ),
        (
            "2026-10-31T04:00:00.000000999-05:00",
            "2026-10-31T09:00:00Z",
        ),
        (
            "1969-12-31T23:59:59.999999999Z",
            "1969-12-31T23:59:59.999999Z",
        ),
        (
            "2026-10-31T09:00:00.1234567890123Z",
            "2026-10-31T09:00:00.123456Z",
        ),
    ] {
        let mut arguments = pagerduty_override_arguments();
        arguments["override_request"]["overrides"][0]["start"] = json!(requested);
        let mut payload = pagerduty_override();
        payload["override"]["start"] = json!(returned);
        let checked = pagerduty_result("create_schedule_override", arguments, json!([payload]));
        assert!(
            checked.status.success(),
            "{requested} -> {returned}: {checked:?}"
        );
    }
}

#[test]
fn pagerduty_result_binding_does_not_accept_returned_precision_the_client_never_sent() {
    let mut arguments = pagerduty_override_arguments();
    arguments["override_request"]["overrides"][0]["start"] =
        json!("2026-10-31T09:00:00.123456789Z");
    let mut payload = pagerduty_override();
    payload["override"]["start"] = json!("2026-10-31T09:00:00.123456789Z");
    let checked = pagerduty_result("create_schedule_override", arguments, json!([payload]));
    assert!(!checked.status.success(), "{checked:?}");
}

#[test]
fn pagerduty_intervals_collapsing_during_native_conversion_cannot_be_reported_as_created() {
    let mut arguments = pagerduty_override_arguments();
    arguments["override_request"]["overrides"][0]["start"] =
        json!("2026-10-31T09:00:00.000000001Z");
    arguments["override_request"]["overrides"][0]["end"] = json!("2026-10-31T09:00:00.000000999Z");
    let mut payload = pagerduty_override();
    payload["override"]["start"] = json!("2026-10-31T09:00:00Z");
    payload["override"]["end"] = json!("2026-10-31T09:00:00Z");
    let checked = pagerduty_result("create_schedule_override", arguments, json!([payload]));
    assert!(!checked.status.success(), "{checked:?}");
}

#[test]
fn pagerduty_management_rejects_actionless_empty_wrong_identity_or_missing_effects() {
    let arguments = json!({"manage_request":{
        "incident_ids":["actual-incident"],"assignement":{"id":"actual-user"}
    }});
    let checked = pagerduty_result(
        "manage_incidents",
        arguments.clone(),
        json!({"response":[pagerduty_incident()]}),
    );
    assert!(checked.status.success(), "{checked:?}");
    for response in [
        json!({"response":[]}),
        json!({"response":[{"id":"other-incident"}]}),
        json!({"response":[pagerduty_incident(),pagerduty_incident()]}),
        {
            let mut incident = pagerduty_incident();
            incident["assignments"] = json!([]);
            json!({"response":[incident]})
        },
        {
            let mut incident = pagerduty_incident();
            incident["assignments"][0]["assignee"]["id"] = json!("wrong-user");
            json!({"response":[incident]})
        },
    ] {
        let checked = pagerduty_result("manage_incidents", arguments.clone(), response.clone());
        assert!(!checked.status.success(), "{response}: {checked:?}");
    }
    for manage_request in [
        json!({"incident_ids":["actual-incident"]}),
        json!({"incident_ids":["actual-incident"],"assignment":{"id":"actual-user"}}),
    ] {
        let checked = pagerduty_result(
            "manage_incidents",
            json!({"manage_request":manage_request}),
            json!({"response":[pagerduty_incident()]}),
        );
        assert!(!checked.status.success(), "{checked:?}");
    }
    let checked = pagerduty_result(
        "manage_incidents",
        json!({"manage_request":{"incident_ids":["actual-incident"],"status":"resolved"}}),
        json!({"response":[pagerduty_incident()]}),
    );
    assert!(!checked.status.success(), "{checked:?}");
}

#[test]
fn pagerduty_note_results_bind_the_full_request_without_treating_business_error_text_as_failure() {
    let arguments = json!({"incident_id":"actual-incident","note":"Error: the rollback resolved this incident."});
    let payload = json!({"id":"actual-note","user":{"id":"actual-actor"},
        "created_at":"2026-10-30T14:00:00Z","content":arguments["note"]});
    let checked = pagerduty_result("add_note_to_incident", arguments.clone(), payload.clone());
    assert!(checked.status.success(), "{checked:?}");
    for fields in [
        json!({"content":"other content"}),
        json!({"id":""}),
        json!({"user":null}),
    ] {
        let mut result = payload.clone();
        for (field, value) in fields.as_object().unwrap() {
            result[field] = value.clone();
        }
        let checked = pagerduty_result("add_note_to_incident", arguments.clone(), result);
        assert!(!checked.status.success(), "{checked:?}");
    }
    let checked = check(
        json!({"jsonrpc":"2.0","id":2,"method":"tools/call",
            "params":{"name":"add_note_to_incident","arguments":arguments}}),
        registry(&[]),
        &json!({"content":[{"type":"text","text":payload.to_string()}],
            "structuredContent":{"id":"forged-or-conflicting-note"}}),
        "7111",
    );
    assert!(!checked.status.success(), "{checked:?}");
    assert_eq!(
        ResultContract::for_tool("pagerduty", "list_schedules"),
        ResultContract::McpEnvelopeV1
    );
}

#[test]
fn grafana_generic_http_errors_are_not_successes_just_because_mcp_has_no_error_flag() {
    assert_eq!(
        ResultContract::for_tool("grafana", "grafana_api_request"),
        ResultContract::GrafanaApiV1
    );
    let arguments = json!({"endpoint":"/api/annotations","method":"POST","body":"{}"});
    for payload in [
        json!({"status":400,"headers":{},"data":{"message":"bad request data"}}),
        json!({"status":503,"data":{"message":"Unavailable"}}),
        json!({"status":"200","data":{}}),
        json!({"status":200}),
        json!({"error":"Native request failed"}),
    ] {
        let checked = grafana_result("grafana_api_request", arguments.clone(), payload.clone());
        assert!(!checked.status.success(), "{payload}: {checked:?}");
        assert!(checked.stdout.is_empty());
    }
    let checked = grafana_result(
        "grafana_api_request",
        arguments,
        json!({"status":200,"headers":{},"data":{"error":"ordinary deployment log text"}}),
    );
    assert!(checked.status.success(), "{checked:?}");
}

#[test]
fn grafana_annotations_use_the_pinned_capitalized_payload_and_created_identity() {
    for payload in [
        json!({"Payload":{"id":107,"message":"Annotation added"}}),
        json!({"Payload":{"id":108,"message":"Annotation added","text":"Error: ordinary business content"}}),
    ] {
        let checked = grafana_result("create_annotation", json!({}), payload.clone());
        assert!(checked.status.success(), "{payload}: {checked:?}");
    }
    for payload in [
        json!({"id":107,"message":"Annotation added"}),
        json!({"payload":{"id":107,"message":"Annotation added"}}),
        json!({"Payload":{"id":0,"message":"Annotation added"}}),
        json!({"Payload":{"id":1.5,"message":"Annotation added"}}),
        json!({"Payload":{"id":107,"message":"Annotation rejected"}}),
    ] {
        let checked = grafana_result("create_annotation", json!({}), payload.clone());
        assert!(!checked.status.success(), "{payload}: {checked:?}");
        assert!(checked.stdout.is_empty());
    }
    let checked = grafana_result(
        "create_annotation",
        json!({"format":"graphite"}),
        json!({"Payload":{"id":7,"message":"Graphite annotation added"}}),
    );
    assert!(checked.status.success(), "{checked:?}");
    let checked = grafana_result("get_annotations", json!({}), json!({"Payload":[]}));
    assert!(checked.status.success(), "{checked:?}");
}

#[test]
fn grafana_committed_acknowledgment_with_failed_readback_remains_a_successful_write() {
    let arguments = json!({"alertGroupId":"production","state":" Acknowledged "});
    for payload in [
        json!({"alertGroupId":"production","action":"acknowledge","updated":true,"state":"acknowledged"}),
        json!({"alertGroupId":"production","action":"acknowledge","updated":true,
            "stateWarning":"the acknowledge succeeded but the resulting state could not be read back"}),
    ] {
        let checked = grafana_result("update_alert_group", arguments.clone(), payload.clone());
        assert!(checked.status.success(), "{payload}: {checked:?}");
    }
    for payload in [
        json!({"alertGroupId":"production","action":"acknowledge","updated":false,"state":"acknowledged"}),
        json!({"alertGroupId":"staging","action":"acknowledge","updated":true,"state":"acknowledged"}),
        json!({"alertGroupId":"production","action":"resolve","updated":true,"state":"resolved"}),
        json!({"alertGroupId":"production","action":"acknowledge","updated":true}),
        json!({"alertGroupId":"production","action":"acknowledge","updated":true,"state":"acknowledged","stateWarning":"contradictory envelope"}),
    ] {
        let checked = grafana_result("update_alert_group", arguments.clone(), payload.clone());
        assert!(!checked.status.success(), "{payload}: {checked:?}");
        assert!(checked.stdout.is_empty());
    }
}

#[test]
fn grafana_oncall_tool_success_is_not_proof_that_every_directory_lookup_succeeded() {
    let arguments = json!({"scheduleId":"primary"});
    for users in [json!([]), json!([{"id":"sam","username":"sbrennan"}])] {
        let checked = grafana_result(
            "get_current_oncall_users",
            arguments.clone(),
            json!({"scheduleId":"primary","scheduleName":"Checkout Primary","users":users}),
        );
        assert!(checked.status.success(), "{checked:?}");
    }
    let checked = grafana_result(
        "get_current_oncall_users",
        arguments,
        json!({"scheduleId":"secondary","scheduleName":"Checkout Secondary","users":[]}),
    );
    assert!(!checked.status.success(), "{checked:?}");
}

#[test]
fn grafana_silence_results_bind_real_ids_without_prescribing_a_seed_or_update_id() {
    for payload in [
        json!({"silenceID":"actually-created"}),
        json!({"silenceID":"replacement-id"}),
    ] {
        let checked = grafana_result(
            "alerting_silences_write",
            json!({"operation":"update","silence_id":"previous-id"}),
            payload,
        );
        assert!(checked.status.success(), "{checked:?}");
    }
    for payload in [json!({"silenceID":""}), json!({"id":"unbound"}), json!({})] {
        let checked = grafana_result(
            "alerting_silences_write",
            json!({"operation":"create"}),
            payload,
        );
        assert!(!checked.status.success(), "{checked:?}");
    }
    let row = json!({
        "id":"actual","startsAt":"2026-11-13T02:00:00Z","endsAt":"2026-11-13T04:00:00Z",
        "matchers":[{"name":"instance","value":"pg-replica-2","isRegex":false}],
        "status":{"state":"pending"},"comment":"Error budget maintenance"
    });
    let checked = grafana_result(
        "alerting_silences_read",
        json!({"operation":"get","silence_id":"actual"}),
        row.clone(),
    );
    assert!(checked.status.success(), "{checked:?}");
    let checked = grafana_result(
        "alerting_silences_read",
        json!({"operation":"get","silence_id":"someone-else"}),
        row,
    );
    assert!(!checked.status.success(), "{checked:?}");
}

#[test]
fn grafana_reviewed_tools_do_not_silently_claim_unreviewed_rule_or_query_result_contracts() {
    for tool in [
        "query_prometheus",
        "alerting_rules_read",
        "list_datasources",
    ] {
        assert_eq!(
            ResultContract::for_tool("grafana", tool),
            ResultContract::McpEnvelopeV1
        );
    }
    let request = json!({"jsonrpc":"2.0","id":2,"method":"tools/call",
        "params":{"name":"create_annotation","arguments":{}}});
    for result in [
        json!({"content":[]}),
        json!({"content":[{"type":"text","text":"not JSON"}]}),
        json!({"content":[{"type":"text","text":"{\"Payload\":{\"id\":1,\"message\":\"Annotation added\"}}"}],
            "structuredContent":{"Payload":{"id":2,"message":"Annotation added"}}}),
    ] {
        let checked = check(request.clone(), registry(&[]), &result, "7114");
        assert!(!checked.status.success(), "{checked:?}");
        assert!(checked.stdout.is_empty());
    }
}

#[test]
fn okta_native_error_shapes_stop_references_without_scoring_business_words() {
    let request = json!({"jsonrpc":"2.0","id":2,"method":"tools/call",
        "params":{"name":"get_user","arguments":{"user_id":"operator"}}});
    let contracts = registry(&[]);
    assert_eq!(
        ResultContract::for_tool("okta", "get_user"),
        ResultContract::OktaApiV1
    );
    for payload in [
        json!({"error":"API request failed"}),
        json!({"exception":"SDK validation rejected input"}),
        json!([{"error":"wrapped native error"}]),
        json!({"ok":false,"error":{"type":"empty_body","message":"no response","tool":"get_user"},
            "status_code":null,"raw":{}}),
        json!({"ok":true,"error":null}),
    ] {
        let result = json!({"content":[{"type":"text","text":payload.to_string()}]});
        let checked = check(request.clone(), contracts.clone(), &result, "7120");
        assert!(!checked.status.success(), "{payload}");
        assert!(checked.stdout.is_empty());
    }
    for payload in [
        json!({"id":"operator","profile":{"error":"customer's reported error","exception":"a business label"}}),
        json!({"eventType":"user.authentication.sso","outcome":{"result":"FAILURE"}}),
        json!({"items":[],"total_fetched":0,"has_more":false}),
    ] {
        let result = json!({"content":[{"type":"text","text":payload.to_string()}],
            "structuredContent":{"result":payload}});
        let checked = check(request.clone(), contracts.clone(), &result, "7120");
        assert!(checked.status.success(), "{checked:?}");
        assert_eq!(
            serde_json::from_slice::<Value>(&checked.stdout).unwrap(),
            result
        );
    }
    let blocks = json!({"content":[
        {"type":"text","text":json!({"id":"one","profile":{"name":"First group"}}).to_string()},
        {"type":"text","text":json!({"id":"two","profile":{"name":"Second group"}}).to_string()}
    ]});
    assert!(
        check(request.clone(), contracts.clone(), &blocks, "7120")
            .status
            .success()
    );
    let empty = json!({"content":[]});
    assert!(
        check(request.clone(), contracts.clone(), &empty, "7120")
            .status
            .success()
    );
    let grouped = json!({
        "content":[{"type":"text","text":json!({"id":"one","profile":{"name":"A group"}}).to_string()}],
        "structuredContent":{"result":[{"id":"one","profile":{"name":"A group"}}]}
    });
    assert!(
        check(request.clone(), contracts.clone(), &grouped, "7120")
            .status
            .success()
    );
    for result in [
        json!({"content":[{"type":"text","text":"not JSON"}]}),
        json!({"content":[{"type":"text","text":"{}"}],"structuredContent":{"result":{"error":"different"}}}),
        json!({"content":[{"type":"image","data":"not a JSON result"}]}),
    ] {
        assert!(
            !check(request.clone(), contracts.clone(), &result, "7120")
                .status
                .success()
        );
    }
}

#[test]
fn okta_accepted_cancellation_or_another_users_message_is_not_reference_completion() {
    let request = json!({"jsonrpc":"2.0","id":2,"method":"tools/call",
        "params":{"name":"deactivate_user","arguments":{"user_id":"selected-user"}}});
    let contracts = registry(&[]);
    for message in [
        "User deactivation cancelled by user.",
        "User another-user deactivated successfully.",
        "Request accepted",
    ] {
        let result =
            json!({"content":[{"type":"text","text":json!({"message":message}).to_string()}]});
        assert!(
            !check(request.clone(), contracts.clone(), &result, "7120")
                .status
                .success()
        );
    }
    let result = json!({"content":[{"type":"text","text":json!({
        "message":"User selected-user deactivated successfully."
    }).to_string()}]});
    assert!(check(request, contracts, &result, "7120").status.success());
}

#[test]
fn argo_gateway_errors_stop_reference_execution_without_keyword_scoring() {
    let request = json!({"jsonrpc":"2.0","id":2,"method":"tools/call",
        "params":{"name":"get_application","arguments":{"applicationName":"worker"}}});
    let contracts = registry(&[]);
    for code in [3, 5, 7, 9, 12, 16] {
        let result = json!({"isError":false,"content":[{"type":"text",
            "text":json!({"code":code,"error":"request failed","message":"request failed"}).to_string()}]});
        let checked = check(request.clone(), contracts.clone(), &result, "7108");
        assert!(!checked.status.success());
        assert!(checked.stdout.is_empty());
        let with_details = json!({"isError":false,"content":[{"type":"text",
            "text":json!({"code":code,"error":"request failed","message":"request failed","details":[]}).to_string()}]});
        assert!(
            !check(request.clone(), contracts.clone(), &with_details, "7108")
                .status
                .success()
        );
    }
    for data in [
        json!({"metadata":{"name":"worker"},"spec":{"error":"ordinary data","code":7,"message":"business context"}}),
        json!({"metadata":{"name":"worker"},"code":7,"error":"ordinary data","message":"business context"}),
        json!([{"error":"ordinary list data","code":5,"message":"diagnostic"}]),
        json!({"items":[]}),
    ] {
        let result = json!({"isError":false,"content":[{"type":"text","text":data.to_string()}]});
        let checked = check(request.clone(), contracts.clone(), &result, "7108");
        assert!(checked.status.success(), "{checked:?}");
    }
    for code in [json!(0), json!("7"), json!(2.5), Value::Null] {
        let malformed = json!({"isError":false,"content":[{"type":"text",
            "text":json!({"code":code,"error":"failure","message":"failure"}).to_string()}]});
        assert!(
            !check(request.clone(), contracts.clone(), &malformed, "7108")
                .status
                .success()
        );
    }
    let malformed =
        json!({"isError":false,"content":[{"type":"text","text":"success without pinned JSON"}]});
    assert!(
        !check(request, contracts, &malformed, "7108")
            .status
            .success()
    );
}

fn reject(request: Value, contracts: Value, result: &Value) {
    let output = check(request, contracts, result, "7116");
    assert!(!output.status.success(), "{result}");
    assert!(output.stdout.is_empty(), "{output:?}");
}

#[test]
fn actual_request_commands_bind_complete_ordered_results_without_inspecting_business_error_words() {
    for commands in [json!(COMMAND), json!([COMMAND, COMMAND]), json!([])] {
        let requested: Vec<_> = match &commands {
            Value::String(command) => vec![command.as_str()],
            Value::Array(commands) => commands
                .iter()
                .map(|command| command.as_str().unwrap())
                .collect(),
            _ => unreachable!(),
        };
        let result = result(&requested);
        let output = check(request(commands), registry(&[]), &result, "7116");
        assert!(output.status.success(), "{output:?}");
        assert_eq!(
            serde_json::from_slice::<Value>(&output.stdout).unwrap(),
            result
        );
    }
}

#[test]
fn wrapped_client_service_validation_context_and_constraint_failures_cannot_complete_a_program() {
    for patch in [
        json!({"error":"client validation failed"}),
        json!({"response":{"status_code":400,"error":"MissingParameter","error_code":"MissingParameter"}}),
        json!({"validation_failures":[{"reason":"invalid"}]}),
        json!({"missing_context_failures":[{"reason":"missing"}]}),
        json!({"failed_constraints":["denied"]}),
        json!({"response":null}),
        json!({"response":{"status_code":"200"}}),
        json!({"response":{"status_code":200.5}}),
        json!({"response":{"status_code":200,"error":"failed","error_code":null}}),
        json!({"response":{"status_code":200,"error":null,"error_code":"Unexpected"}}),
        json!({"validation_failures":"malformed"}),
        json!({"cli_command":"aws iam get-user --user-name wrong"}),
    ] {
        let mut result = result(&[COMMAND]);
        result["structuredContent"]["result"][0]
            .as_object_mut()
            .unwrap()
            .extend(patch.as_object().unwrap().clone());
        reject(request(json!(COMMAND)), registry(&[]), &result);
    }
}

#[test]
fn partial_extra_reordered_and_unstructured_results_do_not_supply_missing_execution_evidence() {
    for result in [
        result(&[COMMAND]),
        result(&[COMMAND, "aws iam list-users", COMMAND]),
        result(&["aws iam list-users", COMMAND]),
        json!({"content":[{"type":"text","text":"success"}]}),
        json!({"content":[],"structuredContent":{"result":{}}}),
    ] {
        reject(
            request(json!([COMMAND, "aws iam list-users"])),
            registry(&[]),
            &result,
        );
    }
}

#[test]
fn exact_declared_service_errors_are_evidence_not_blanket_permission_to_ignore_failed_commands() {
    let mut declared = call(json!([COMMAND, NEGATIVE]));
    declared.allow_api_errors = vec![permission()];
    let mut result = result(&[COMMAND, NEGATIVE]);
    result["structuredContent"]["result"][1]["response"] =
        json!({"status_code":404,"error_code":CODE,"error":"Configuration is absent"});
    let output = check(
        request(declared.arguments["cli_command"].clone()),
        registry(&[declared.clone()]),
        &result,
        "7116",
    );
    assert!(output.status.success(), "{output:?}");
    reject(
        request(declared.arguments["cli_command"].clone()),
        registry(&[]),
        &result,
    );
    for (field, value) in [
        ("error_code", json!("NoSuchBucket")),
        ("status_code", json!(403)),
        ("error", Value::Null),
    ] {
        let mut changed = result.clone();
        changed["structuredContent"]["result"][1]["response"][field] = value;
        reject(
            request(declared.arguments["cli_command"].clone()),
            registry(&[declared.clone()]),
            &changed,
        );
    }
    let mut changed = result;
    changed["structuredContent"]["result"][1]["cli_command"] = json!("aws iam list-users");
    reject(
        request(json!([COMMAND, "aws iam list-users"])),
        registry(&[declared]),
        &changed,
    );
}

#[test]
fn error_permissions_are_typed_scoped_static_and_cannot_relabel_client_errors() {
    for permission in [
        AllowedApiError {
            command: COMMAND.into(),
            ..permission()
        },
        AllowedApiError {
            status: 200,
            ..permission()
        },
        AllowedApiError {
            status: 600,
            ..permission()
        },
        AllowedApiError {
            code: "".into(),
            ..permission()
        },
        AllowedApiError {
            command: "${returned_command}".into(),
            ..permission()
        },
    ] {
        let mut call = call(json!(NEGATIVE));
        call.allow_api_errors = vec![permission];
        assert!(oracle_results::validate(&call).is_err());
    }
    let mut call = call(json!(NEGATIVE));
    call.allow_api_errors = vec![permission(), permission()];
    assert!(oracle_results::validate(&call).is_err());
    call.allow_api_errors.truncate(1);
    call.endpoint = "other".into();
    assert!(oracle_results::validate(&call).is_err());
    call.endpoint = "aws-api".into();
    let mut result = result(&[NEGATIVE]);
    result["structuredContent"]["result"][0] =
        json!({"cli_command":NEGATIVE,"error":format!("404 {CODE}")});
    reject(request(json!(NEGATIVE)), registry(&[call]), &result);
}

#[test]
fn profiles_are_endpoint_and_tool_scoped_not_global_key_or_error_word_heuristics() {
    let malformed_vendor_result = json!({
        "content":[{"type":"text","text":"Error: expected business data"}],
        "structuredContent":{"result":[{"error":"not a declared vendor contract on this endpoint"}]}
    });
    let output = check(
        request(json!(COMMAND)),
        registry(&[]),
        &malformed_vendor_result,
        "7101",
    );
    assert!(output.status.success(), "{output:?}");
    let mut other_tool = request(json!(COMMAND));
    other_tool["params"]["name"] = json!("another_tool");
    let output = check(other_tool, registry(&[]), &malformed_vendor_result, "7116");
    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        ResultContract::for_tool("aws-api", "call_aws"),
        ResultContract::AwsApiV1
    );
    assert_eq!(
        ResultContract::for_tool("github", "call_aws"),
        ResultContract::McpEnvelopeV1
    );
}

#[test]
fn missing_or_unknown_context_and_unreviewed_wildcard_or_cli_alias_shapes_fail_closed() {
    for commands in [
        json!(""),
        json!([" "]),
        json!(null),
        json!("aws ec2 describe-instances --region *"),
    ] {
        reject(request(commands), registry(&[]), &result(&[COMMAND]));
    }
    for contracts in [
        json!({}),
        json!({"schema_version":9,"protocol":"oracle-tool-results-v1","profiles":{},"allowed_aws_api_errors":[]}),
        json!({"schema_version":1,"protocol":"oracle-tool-results-v1","profiles":{"7116":{"call_aws":"unknown"}},"allowed_aws_api_errors":[]}),
        json!({"schema_version":1,"protocol":"oracle-tool-results-v1","profiles":{"7116":null},"allowed_aws_api_errors":[]}),
        json!({"schema_version":1,"protocol":"oracle-tool-results-v1","profiles":{"7116":{"call_aws":null}},"allowed_aws_api_errors":[]}),
    ] {
        reject(request(json!(COMMAND)), contracts, &result(&[COMMAND]));
    }
    let mut alias_result = result(&[COMMAND]);
    alias_result["structuredContent"]["result"][0]["response"] =
        json!("CLI stdout without API status");
    reject(request(json!(COMMAND)), registry(&[]), &alias_result);
}

#[test]
fn result_profiles_follow_the_real_catalog_port_and_reject_ambiguous_routing() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut catalog = Catalog::load(&root.join("catalog/services.toml")).unwrap();
    let endpoint = catalog
        .services
        .iter_mut()
        .flat_map(|service| &mut service.endpoints)
        .find(|endpoint| endpoint.id == "aws-api")
        .unwrap();
    endpoint.mcp_port = 7999;
    let contracts = oracle_results::registry(&catalog, &[]).unwrap();
    assert_eq!(contracts["profiles"]["7999"]["call_aws"], "aws_api_v1");
    assert!(contracts["profiles"].get("7116").is_none());
    let output = check(
        request(json!(COMMAND)),
        contracts,
        &result(&[COMMAND]),
        "7999",
    );
    assert!(output.status.success(), "{output:?}");
    let other = catalog
        .services
        .iter_mut()
        .flat_map(|service| &mut service.endpoints)
        .find(|endpoint| endpoint.id != "aws-api")
        .unwrap();
    other.mcp_port = 7999;
    assert!(oracle_results::registry(&catalog, &[]).is_err());
}

#[test]
fn terraform_profiles_bind_native_mutation_text_to_its_actual_request() {
    let contracts = registry(&[]);
    assert_eq!(
        contracts["profiles"]["7109"]["action_run"],
        "terraform_api_v1"
    );
    assert_eq!(
        ResultContract::for_tool("terraform", "get_workspace_details"),
        ResultContract::McpEnvelopeV1,
    );
    for (tool, arguments, success) in [
        (
            "action_run",
            json!({"run_id":"run-selected","run_action":"cancel"}),
            "Run canceled successfully",
        ),
        (
            "force_unlock_workspace",
            json!({"workspace_id":"ws-selected"}),
            "Workspace \"ws-selected\" is now unlocked",
        ),
        (
            "update_workspace_variable",
            json!({"key":"desired_count","variable_id":"var-selected"}),
            "Updated variable desired_count with ID var-selected",
        ),
    ] {
        let request = json!({"jsonrpc":"2.0","id":2,"method":"tools/call",
            "params":{"name":tool,"arguments":arguments}});
        for (text, expected) in [(success, true), ("A different operation succeeded", false)] {
            let result = json!({"content":[{"type":"text","text":text}]});
            let output = check(request.clone(), contracts.clone(), &result, "7109");
            assert_eq!(output.status.success(), expected, "{tool}: {output:?}");
        }
    }
}

#[test]
fn terraform_jsonapi_reads_require_the_actual_run_identity_not_a_successful_transport() {
    let request = json!({"jsonrpc":"2.0","id":2,"method":"tools/call",
        "params":{"name":"get_run_details","arguments":{"run_id":"run-selected"}}});
    for (payload, expected) in [
        (
            json!({"data":{"id":"run-selected","type":"runs","attributes":{"status":"applying"}}}),
            true,
        ),
        (
            json!({"data":{"id":"run-other","type":"runs","attributes":{"status":"applied"}}}),
            false,
        ),
        (
            json!({"data":{"id":"run-selected","type":"workspaces","attributes":{"status":"applied"}}}),
            false,
        ),
        (json!({"errors":[{"detail":"SDK failed"}]}), false),
        (json!({"exception":"serialization failed"}), false),
    ] {
        let result = json!({"content":[{"type":"text","text":payload.to_string()}]});
        let output = check(request.clone(), registry(&[]), &result, "7109");
        assert_eq!(output.status.success(), expected, "{output:?}");
    }
}
