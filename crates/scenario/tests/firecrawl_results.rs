use std::{fs, path::Path, process::Command};

use catalog::Catalog;
use scenario::oracle_results::{self, ResultContract};
use serde_json::{Value, json};

fn envelope(tool: &str, payload: Value) -> Value {
    let keys = match tool {
        "firecrawl_map" => vec!["links", "success", "error", "warning", "id"],
        "firecrawl_crawl" | "firecrawl_check_crawl_status" => vec![
            "success",
            "error",
            "warning",
            "id",
            "status",
            "completed",
            "total",
            "creditsUsed",
            "expiresAt",
            "next",
            "url",
            "data",
        ],
        "firecrawl_monitor_list" | "firecrawl_monitor_checks" => {
            vec!["success", "error", "data", "pagination"]
        }
        "firecrawl_monitor_check" => vec!["success", "error", "data", "pagination", "next"],
        _ => vec!["success", "error", "data"],
    };
    let structured: serde_json::Map<String, Value> = payload
        .as_object()
        .unwrap()
        .iter()
        .filter(|(key, _)| keys.contains(&key.as_str()))
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect();
    json!({"content":[{"type":"text","text":payload.to_string()}],"structuredContent":structured})
}

fn check(tool: &str, arguments: Value, result: Value) -> std::process::Output {
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
        ("result.json", result),
    ] {
        fs::write(directory.path().join(name), value.to_string()).unwrap();
    }
    Command::new("jq")
        .args(["-e", "-L"])
        .arg(root.join("crates/scenario/assets"))
        .args(["--arg", "endpoint", "7102", "--slurpfile", "request"])
        .arg(directory.path().join("request.json"))
        .args(["--slurpfile", "contracts"])
        .arg(directory.path().join("contracts.json"))
        .arg("-f")
        .arg(root.join("crates/scenario/assets/oracle-tool-result.jq"))
        .arg(directory.path().join("result.json"))
        .output()
        .unwrap()
}

fn accepted(tool: &str, arguments: Value, payload: Value) -> bool {
    check(tool, arguments, envelope(tool, payload))
        .status
        .success()
}

fn crawl(status: &str) -> Value {
    json!({"id":"actual-crawl","status":status,"completed":1,"total":1,
        "data":[{"markdown":"Error budget documentation"}],"next":null})
}

fn monitor() -> Value {
    json!({"id":"actual-monitor","name":"Pricing","status":"active"})
}

fn manual_check(status: &str) -> Value {
    json!({"id":"actual-check","monitorId":"actual-monitor","status":status,"trigger":"manual"})
}

#[test]
fn reviewed_profiles_do_not_certify_unreviewed_firecrawl_operations() {
    for tool in [
        "firecrawl_map",
        "firecrawl_crawl",
        "firecrawl_check_crawl_status",
        "firecrawl_monitor_create",
        "firecrawl_monitor_list",
        "firecrawl_monitor_get",
        "firecrawl_monitor_update",
        "firecrawl_monitor_run",
        "firecrawl_monitor_checks",
        "firecrawl_monitor_check",
    ] {
        assert_eq!(
            ResultContract::for_tool("firecrawl", tool),
            ResultContract::FirecrawlNativeV1
        );
    }
    assert_eq!(
        ResultContract::for_tool("firecrawl", "firecrawl_agent"),
        ResultContract::McpEnvelopeV1
    );
}

#[test]
fn foreground_crawl_requires_terminal_success_without_inventing_an_sdk_success_field() {
    for status in ["completed", "failed", "cancelled", "queued", "scraping"] {
        assert_eq!(
            accepted(
                "firecrawl_crawl",
                json!({"url":"https://docs.example/api/one"}),
                crawl(status)
            ),
            status == "completed",
            "{status}"
        );
    }
    assert!(!accepted(
        "firecrawl_crawl",
        json!({"url":"https://docs.example/api/one"}),
        json!({"success":true,"id":"accepted-only","url":"https://api.example/v2/crawl/accepted-only"})
    ));
}

#[test]
fn status_reads_bind_the_requested_identity_and_preserve_failed_job_observations() {
    for status in ["completed", "failed", "cancelled", "queued", "scraping"] {
        assert!(accepted(
            "firecrawl_check_crawl_status",
            json!({"id":"actual-crawl"}),
            crawl(status)
        ));
    }
    assert!(!accepted(
        "firecrawl_check_crawl_status",
        json!({"id":"another-crawl"}),
        crawl("completed")
    ));
    for (field, value) in [
        ("completed", json!(-1)),
        ("completed", json!(1.5)),
        ("total", json!("1")),
        ("total", json!(0)),
        ("id", json!("")),
        ("data", json!({})),
        (
            "next",
            json!("https://api.example/v2/crawl/actual-crawl?skip=1"),
        ),
    ] {
        let mut payload = crawl("completed");
        payload[field] = value;
        assert!(
            !accepted(
                "firecrawl_check_crawl_status",
                json!({"id":"actual-crawl"}),
                payload
            ),
            "{field}"
        );
    }
}

#[test]
fn native_text_and_declared_structured_projection_must_agree() {
    let arguments = json!({"id":"actual-monitor"});
    let payload = json!({"success":true,"id":"actual-check","data":manual_check("queued")});
    let original = envelope("firecrawl_monitor_run", payload);
    assert!(original["structuredContent"].get("id").is_none());
    assert!(
        check("firecrawl_monitor_run", arguments.clone(), original.clone())
            .status
            .success()
    );
    for field in ["data", "success"] {
        let mut result = original.clone();
        result["structuredContent"][field] = Value::Null;
        assert!(
            !check("firecrawl_monitor_run", arguments.clone(), result)
                .status
                .success()
        );
    }
    let mut result = original;
    result["structuredContent"]["id"] = json!("actual-check");
    assert!(
        !check("firecrawl_monitor_run", arguments, result)
            .status
            .success()
    );
    assert!(!accepted(
        "firecrawl_monitor_run",
        json!({"id":"actual-monitor"}),
        json!({"success":true,"id":"conflicting-check","data":manual_check("queued")})
    ));
}

#[test]
fn monitor_writes_bind_actual_child_parent_and_submitted_state() {
    let arguments = json!({"id":"actual-monitor"});
    assert!(accepted(
        "firecrawl_monitor_run",
        arguments.clone(),
        json!({"success":true,"data":manual_check("queued")})
    ));
    assert!(!accepted(
        "firecrawl_monitor_run",
        json!({"id":"other-monitor"}),
        json!({"success":true,"data":manual_check("queued")})
    ));
    assert!(!accepted(
        "firecrawl_monitor_run",
        arguments.clone(),
        json!({"success":true,"data":manual_check("failed")})
    ));
    let mut paused = monitor();
    paused["status"] = json!("paused");
    let arguments = json!({"id":"actual-monitor","body":{"status":"paused"}});
    assert!(accepted(
        "firecrawl_monitor_update",
        arguments.clone(),
        json!({"success":true,"data":paused})
    ));
    assert!(!accepted(
        "firecrawl_monitor_update",
        arguments,
        json!({"success":true,"data":monitor()})
    ));
}

#[test]
fn failed_historical_checks_and_ordinary_business_error_words_are_not_tool_failures() {
    let arguments = json!({"id":"actual-monitor","status":"failed"});
    assert!(accepted(
        "firecrawl_monitor_checks",
        arguments,
        json!({"success":true,"data":[manual_check("failed")]})
    ));
    let mut historical = manual_check("failed");
    historical["pages"] = json!([]);
    assert!(accepted(
        "firecrawl_monitor_check",
        json!({"id":"actual-monitor","checkId":"actual-check"}),
        json!({"success":true,"data":historical})
    ));
    let mut record = monitor();
    record["name"] = json!("Error budget monitoring");
    assert!(accepted(
        "firecrawl_monitor_list",
        json!({}),
        json!({"success":true,"data":[record]})
    ));
    assert!(accepted(
        "firecrawl_map",
        json!({"url":"https://docs.example"}),
        json!({"links":[]})
    ));
}

#[test]
fn malformed_success_and_typed_errors_cannot_authorize_reference_completion() {
    for payload in [
        json!({}),
        json!({"success":true}),
        json!({"success":false,"data":[]}),
        json!({"success":true,"error":"denied","data":[]}),
        json!({"success":true,"data":{}}),
    ] {
        assert!(!accepted("firecrawl_monitor_list", json!({}), payload));
    }
    let mut result = envelope(
        "firecrawl_monitor_get",
        json!({"success":true,"data":monitor()}),
    );
    result["isError"] = json!(true);
    assert!(
        !check(
            "firecrawl_monitor_get",
            json!({"id":"actual-monitor"}),
            result
        )
        .status
        .success()
    );
}

#[test]
fn monitor_creation_honors_advanced_body_precedence_and_native_name_trimming() {
    let payload = json!({"success":true,"data":monitor()});
    for arguments in [
        json!({"name":" Pricing ","page":"https://vendor.example/pricing","goal":"All changes"}),
        json!({"name":"Ignored shorthand","body":{"name":"Pricing"}}),
        json!({"page":"https://vendor.example/pricing","goal":"All changes"}),
    ] {
        assert!(accepted(
            "firecrawl_monitor_create",
            arguments,
            payload.clone()
        ));
    }
    assert!(!accepted(
        "firecrawl_monitor_create",
        json!({"body":{"name":"Another requested name"}}),
        payload
    ));
}

#[test]
fn check_readbacks_bind_parent_and_child_without_requiring_pagination_metadata() {
    let mut record = manual_check("skipped_overlap");
    record["pages"] = json!([]);
    let payload = json!({"success":true,"data":record});
    assert!(accepted(
        "firecrawl_monitor_check",
        json!({"id":"actual-monitor","checkId":"actual-check"}),
        payload.clone()
    ));
    for arguments in [
        json!({"id":"other-monitor","checkId":"actual-check"}),
        json!({"id":"actual-monitor","checkId":"other-check"}),
    ] {
        assert!(!accepted(
            "firecrawl_monitor_check",
            arguments,
            payload.clone()
        ));
    }
    for status in ["unknown", "cancelled", ""] {
        assert!(!accepted(
            "firecrawl_monitor_checks",
            json!({"id":"actual-monitor"}),
            json!({"success":true,"data":[manual_check(status)]})
        ));
    }
}

#[test]
fn monitor_update_checks_explicit_goal_and_boolean_fields_without_banning_business_text() {
    let mut record = monitor();
    record["goal"] = json!("Alert on error budget changes");
    record["judgeEnabled"] = json!(false);
    let arguments = json!({"id":"actual-monitor","body":{
        "goal":" Alert on error budget changes ","judgeEnabled":false
    }});
    assert!(accepted(
        "firecrawl_monitor_update",
        arguments.clone(),
        json!({"success":true,"data":record.clone()})
    ));
    record["judgeEnabled"] = json!(true);
    assert!(!accepted(
        "firecrawl_monitor_update",
        arguments,
        json!({"success":true,"data":record})
    ));
}
