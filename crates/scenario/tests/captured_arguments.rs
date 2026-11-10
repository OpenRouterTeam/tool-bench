//! Captured (runtime) oracle arguments: declared constraints, static compatibility with the
//! locked tool schemas, and fail-closed runtime checks in both oracle implementations.

use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use catalog::Catalog;
use scenario::{
    Scenario,
    authoring::CAPTURED_GAP,
    coverage::inventory,
    discover,
    oracle_tools::OracleTools,
    taskgen::{Conditions, Images, write_task_with_conditions},
};
use serde_json::{Value, json};

const CAPTURE_SCENARIOS: [&str; 13] = [
    "business/multi/zendesk-refund-via-stripe",
    "business/shopify/relaunch-tote",
    "cloud/mongodb/refund-order",
    "cloud/okta/contractor-self-service-reset",
    "code/argocd/restart-indexer-worker",
    "code/argocd/roll-back-billing-worker",
    "code/terraform/scale-checkout-staging",
    "code/terraform/unlock-orders-prod",
    "code/vercel/rollback-storefront-production",
    "code/vercel/rotate-checkout-stripe-key",
    "ops/grafana/silence-replica-maintenance",
    "ops/multi/datadog-latency-argocd-rollback",
    "ops/multi/pagerduty-sentry-github-issue",
];

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn catalog() -> Catalog {
    Catalog::load(&root().join("catalog/services.toml")).unwrap()
}

fn load(id: &str) -> Scenario {
    Scenario::load(&root().join("scenarios").join(id)).unwrap()
}

fn row(scenario: &Scenario, tools: &OracleTools) -> Value {
    let calls = scenario
        .validate(&root().join("scenarios"), &catalog(), tools)
        .unwrap();
    let inventory = inventory(std::slice::from_ref(scenario), vec![calls], tools).unwrap();
    serde_json::to_value(&inventory.scenarios[0]).unwrap()
}

#[test]
fn every_scored_capture_is_declared_statically_compatible_and_runtime_checked() {
    let catalog = catalog();
    let tools = OracleTools::load(&root(), &catalog).unwrap();
    let scenarios = discover(&root().join("scenarios")).unwrap();
    let calls = scenarios
        .iter()
        .map(|scenario| scenario.validate(&root().join("scenarios"), &catalog, &tools))
        .collect::<anyhow::Result<Vec<_>>>()
        .unwrap();
    let inventory = serde_json::to_value(inventory(&scenarios, calls, &tools).unwrap()).unwrap();
    let mut validated = Vec::new();
    for row in inventory["scenarios"].as_array().unwrap() {
        let gaps = row["source_gaps"].as_array().unwrap();
        assert!(
            !gaps.contains(&json!(CAPTURED_GAP)),
            "{} still defers captured arguments",
            row["id"]
        );
        if row["authoring"]["captured_arguments"] == "validated" {
            validated.push(row["id"].as_str().unwrap().to_owned());
            for call in row["reference_calls"].as_array().unwrap() {
                if call["arguments"] == "partially_deferred" {
                    assert_eq!(call["captured"]["runtime_locked_schema"], true);
                    assert_eq!(call["captured"]["open"], json!([]));
                    for used in call["captured"]["uses"].as_array().unwrap() {
                        assert_ne!(used["static_check"], "undeclared", "{}", row["id"]);
                    }
                }
            }
        } else {
            assert_eq!(row["authoring"]["captured_arguments"], "none");
        }
    }
    assert_eq!(validated, CAPTURE_SCENARIOS);
    assert_eq!(
        inventory["authoring"]["scored_captured_argument_calls"],
        inventory["authoring"]["scored_captured_argument_calls_validated"]
    );
    assert_eq!(inventory["authoring"]["scored_captured_argument_calls"], 18);
}

#[test]
fn a_capture_whose_declared_type_cannot_satisfy_the_consuming_schema_fails_authoring() {
    let tools = OracleTools::load(&root(), &catalog()).unwrap();
    let mut scenario = load("code/terraform/scale-checkout-staging");
    let capture = scenario
        .solution
        .calls
        .iter_mut()
        .flat_map(|call| &mut call.capture)
        .find(|capture| capture.name == "review_run")
        .unwrap();
    capture.constraint = Some(json!({"type": "integer", "minimum": 1}));
    let error = scenario
        .validate(&root().join("scenarios"), &catalog(), &tools)
        .unwrap_err();
    let message = format!("{error:#}");
    assert!(
        message.contains("binding review_run produces integer")
            && message.contains("terraform.get_run_details argument /run_id requires string"),
        "{message}"
    );

    // Interpolating a structured value into text is a shape error, not a string.
    let mut scenario = load("ops/multi/pagerduty-sentry-github-issue");
    let capture = scenario
        .solution
        .calls
        .iter_mut()
        .flat_map(|call| &mut call.capture)
        .find(|capture| capture.name == "issue_number")
        .unwrap();
    capture.constraint = Some(json!({"type": "object"}));
    let message = format!(
        "{:#}",
        scenario
            .validate(&root().join("scenarios"), &catalog(), &tools)
            .unwrap_err()
    );
    assert!(
        message.contains("cannot be interpolated into text at /note"),
        "{message}"
    );
}

#[test]
fn malformed_or_unportable_constraints_fail_authoring() {
    let tools = OracleTools::load(&root(), &catalog()).unwrap();
    for (constraint, reason) in [
        (json!({"pattern": "^run-"}), "single JSON type"),
        (
            json!({"type": "string", "oneOf": [{}]}),
            "do not support keyword oneOf",
        ),
        (
            json!({"type": "string", "pattern": "(?i)^run-"}),
            "portable regular-expression subset",
        ),
        (
            json!({"type": "string", "minLength": "1"}),
            "malformed minLength",
        ),
    ] {
        let mut scenario = load("code/terraform/scale-checkout-staging");
        scenario
            .solution
            .calls
            .iter_mut()
            .flat_map(|call| &mut call.capture)
            .for_each(|capture| capture.constraint = Some(constraint.clone()));
        let message = format!(
            "{:#}",
            scenario
                .validate(&root().join("scenarios"), &catalog(), &tools)
                .unwrap_err()
        );
        assert!(message.contains(reason), "{message}");
    }
}

#[test]
fn an_undeclared_capture_keeps_the_gap_and_says_why() {
    let tools = OracleTools::load(&root(), &catalog()).unwrap();
    let mut scenario = load("code/terraform/scale-checkout-staging");
    scenario
        .solution
        .calls
        .iter_mut()
        .flat_map(|call| &mut call.capture)
        .for_each(|capture| capture.constraint = None);
    let row = row(&scenario, &tools);
    assert!(
        row["source_gaps"]
            .as_array()
            .unwrap()
            .contains(&json!(CAPTURED_GAP))
    );
    assert_eq!(row["authoring"]["captured_arguments"], "deferred");
    let consuming = row["reference_calls"]
        .as_array()
        .unwrap()
        .iter()
        .find(|call| call["tool"] == "get_run_details")
        .unwrap();
    assert_eq!(
        consuming["captured"]["open"],
        json!(["undeclared_capture_constraint:review_run"])
    );
    assert_eq!(
        consuming["captured"]["uses"][0]["static_check"],
        "undeclared"
    );
}

#[test]
fn the_rust_oracle_refuses_to_bind_a_value_that_violates_its_constraint() {
    let tools = OracleTools::load(&root(), &catalog()).unwrap();
    let scenario = load("code/terraform/scale-checkout-staging");
    let (create, details) = {
        let calls = &scenario.solution.calls;
        (
            calls.iter().find(|call| call.tool == "create_run").unwrap(),
            calls
                .iter()
                .find(|call| call.tool == "get_run_details")
                .unwrap(),
        )
    };
    let result = |id: Value| json!({"content":[{"type":"text","text":json!({"data":{"id":id}}).to_string()}]});
    let mut bindings = BTreeMap::new();
    let error = create
        .capture_result(&result(json!(41)), &mut bindings)
        .unwrap_err();
    assert!(
        error.contains("capture review_run violates its declared constraint"),
        "{error}"
    );
    assert!(bindings.is_empty(), "a rejected value must never be bound");
    assert!(details.resolve_arguments(&bindings).is_err());

    create
        .capture_result(&result(json!("run-CZcmD7eagjhyX0vN")), &mut bindings)
        .unwrap();
    let arguments = details.resolve_arguments(&bindings).unwrap();
    tools.check_resolved(details, &arguments).unwrap();
    let error = tools
        .check_resolved(details, &json!({"run_id": 41}))
        .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("violate the locked input schema"),
        "{error}"
    );
}

/// The jq implementation used by generated oracles must agree with the Rust validator.
#[test]
fn the_jq_validator_agrees_with_the_rust_validator_on_the_shared_subset() {
    let cases = json!([
        {"schema": {"type": "string", "pattern": "^re_[A-Za-z0-9_]+$"}, "value": "re_3Nk", "valid": true},
        {"schema": {"type": "string", "pattern": "^re_[A-Za-z0-9_]+$"}, "value": "ch_3Nk", "valid": false},
        {"schema": {"type": "string", "pattern": "^gid://shopify/Product/[0-9]+$"}, "value": "gid://shopify/Product/7", "valid": true},
        {"schema": {"type": "string", "pattern": "^gid://shopify/Product/[0-9]+$"}, "value": "gid://shopify/Product/7?x=1", "valid": false},
        {"schema": {"type": "string", "minLength": 1}, "value": "", "valid": false},
        {"schema": {"type": "string", "maxLength": 2}, "value": "éé", "valid": true},
        {"schema": {"type": "string"}, "value": 7, "valid": false},
        {"schema": {"type": "integer", "minimum": 1}, "value": 7, "valid": true},
        {"schema": {"type": "integer", "minimum": 1}, "value": 7.0, "valid": true},
        {"schema": {"type": "integer", "minimum": 1}, "value": 7.5, "valid": false},
        {"schema": {"type": "integer", "minimum": 1}, "value": 0, "valid": false},
        {"schema": {"type": "number", "maximum": 3}, "value": 3.5, "valid": false},
        {"schema": {"type": ["string", "null"]}, "value": null, "valid": true},
        {"schema": {"type": "boolean"}, "value": "true", "valid": false},
        {"schema": {"enum": ["a", 1]}, "value": 1, "valid": true},
        {"schema": {"enum": ["a", 1]}, "value": "b", "valid": false},
        {"schema": {"const": {"a": [1]}}, "value": {"a": [1]}, "valid": true},
        {"schema": {"const": {"a": [1]}}, "value": {"a": [2]}, "valid": false},
        {"schema": {"type": "object", "required": ["id", "actions"]}, "value": {"id": "r", "actions": {}}, "valid": true},
        {"schema": {"type": "object", "required": ["id", "actions"]}, "value": {"id": "r"}, "valid": false},
        {"schema": {"type": "object", "properties": {"a": {"type": "string"}}, "additionalProperties": false}, "value": {"a": "x"}, "valid": true},
        {"schema": {"type": "object", "properties": {"a": {"type": "string"}}, "additionalProperties": false}, "value": {"a": "x", "b": 1}, "valid": false},
        {"schema": {"type": "object", "additionalProperties": {"type": "integer"}}, "value": {"b": "x"}, "valid": false},
        {"schema": {"type": "object", "properties": {"a": {"type": "string"}}}, "value": {"a": 1}, "valid": false},
        {"schema": {"type": "array", "minItems": 1, "maxItems": 2, "items": {"type": "string"}}, "value": ["a"], "valid": true},
        {"schema": {"type": "array", "minItems": 1, "items": {"type": "string"}}, "value": [], "valid": false},
        {"schema": {"type": "array", "maxItems": 1}, "value": [1, 2], "valid": false},
        {"schema": {"type": "array", "items": {"type": "string"}}, "value": ["a", 2], "valid": false},
        {"schema": {"type": "string", "format": "uri", "description": "annotation only"}, "value": "not a uri", "valid": true},
        {"schema": true, "value": {"anything": 1}, "valid": true},
        {"schema": false, "value": 1, "valid": false}
    ]);
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("cases.json");
    fs::write(&input, cases.to_string()).unwrap();
    let output = Command::new("jq")
        .arg("-c")
        .arg("-L")
        .arg(root().join("crates/scenario/assets"))
        .arg(
            "include \"value-schema\"; map(. as $case | [$case.value | violations($case.schema; \"\")] | length == 0)",
        )
        .arg(&input)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let jq: Vec<bool> = serde_json::from_slice(&output.stdout).unwrap();
    for (case, jq) in cases.as_array().unwrap().iter().zip(jq) {
        assert_eq!(
            scenario::captured::unsupported(&case["schema"]),
            None,
            "{case}"
        );
        let rust = scenario::captured::violations(&case["schema"], &case["value"])
            .unwrap()
            .is_empty();
        assert_eq!(rust, case["valid"].as_bool().unwrap(), "rust: {case}");
        assert_eq!(jq, rust, "jq disagrees: {case}");
    }
}

struct OracleRun {
    success: bool,
    stderr: String,
    requested: Vec<String>,
}

/// Runs the generated oracle against a scripted MCP transport that returns `predecessor`
/// as the previously serving deployment and records every tool request it receives.
fn run_rollback_oracle(scenario: &Scenario, predecessor: Value) -> OracleRun {
    run_rollback_oracle_with(scenario, predecessor, true)
}

fn run_rollback_oracle_with(scenario: &Scenario, predecessor: Value, locked: bool) -> OracleRun {
    let catalog = catalog();
    let tools = OracleTools::load(&root(), &catalog).unwrap();
    let output = tempfile::tempdir().unwrap();
    write_task_with_conditions(
        scenario,
        &catalog,
        &Images {
            world: "world:test".into(),
            tools: "tools:test".into(),
        },
        output.path(),
        Conditions {
            locked_tools: locked.then_some(&tools),
            ..Conditions::default()
        },
    )
    .unwrap();
    let bin = output.path().join("bin");
    fs::create_dir(&bin).unwrap();
    let log = output.path().join("requests.log");
    let curl = bin.join("curl");
    fs::write(
        &curl,
        r#"#!/bin/sh
set -eu
inline=""
request=""
while [ "$#" -gt 0 ]; do
  case "$1" in
    -d) inline="$2"; shift 2 ;;
    --data-binary) request="${2#@}"; shift 2 ;;
    *) shift ;;
  esac
done
if [ -n "$inline" ]; then
  if [ "$(printf '%s' "$inline" | jq -r '.method')" = initialize ]; then
    printf 'HTTP/1.1 200 OK\r\nmcp-session-id: fixture-session\r\n\r\n'
  fi
  exit 0
fi
jq -r '.params.name' "$request" >> "$ORACLE_REQUEST_LOG"
jq -n --argjson predecessor "$ORACLE_PREDECESSOR" '{jsonrpc: "2.0", id: 2, result: {content: [
  {type: "text", text: "ok"},
  {type: "text", text: ({id: "dpl_NewRedeployment", lastAliasRequest: {fromDeploymentId: $predecessor}} | tojson)}
]}}'
"#,
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&curl, fs::Permissions::from_mode(0o755)).unwrap();
    }
    let result = Command::new("sh")
        .arg(
            output
                .path()
                .join(scenario.task_name())
                .join("solution/solve.sh"),
        )
        .env(
            "PATH",
            format!("{}:{}", bin.display(), std::env::var("PATH").unwrap()),
        )
        .env("ORACLE_REQUEST_LOG", &log)
        .env("ORACLE_PREDECESSOR", predecessor.to_string())
        .output()
        .unwrap();
    OracleRun {
        success: result.status.success(),
        stderr: String::from_utf8_lossy(&result.stderr).into_owned(),
        requested: fs::read_to_string(&log)
            .unwrap_or_default()
            .lines()
            .map(str::to_owned)
            .collect(),
    }
}

#[test]
fn the_generated_oracle_validates_captured_values_before_the_consuming_call_is_sent() {
    let scenario = load("code/vercel/rollback-storefront-production");

    let valid = run_rollback_oracle(&scenario, json!("dpl_8o704GjcwumkLAaxPwjoxb4V6QDF"));
    assert!(valid.success, "{}", valid.stderr);
    assert_eq!(
        valid.requested,
        [
            "vercel-find-project",
            "vercel-create-deployment",
            "vercel-get-deployment"
        ]
    );

    // The captured value violates its declared constraint: it is never bound or used.
    let rejected = run_rollback_oracle(&scenario, json!("prj_not_a_deployment!"));
    assert!(!rejected.success);
    assert!(
        rejected
            .stderr
            .contains("captured value violates its declared constraint: capture predecessor_id"),
        "{}",
        rejected.stderr
    );
    assert_eq!(rejected.requested, ["vercel-find-project"]);

    // Without a declared constraint the resolved arguments are still checked against the
    // consuming tool's locked input schema, and the call is not sent.
    let mut undeclared = scenario.clone();
    undeclared
        .solution
        .calls
        .iter_mut()
        .flat_map(|call| &mut call.capture)
        .for_each(|capture| capture.constraint = None);
    let rejected = run_rollback_oracle(&undeclared, json!(7));
    assert!(!rejected.success);
    assert!(
        rejected.stderr.contains(
            "resolved captured arguments for vercel-create-deployment violate the locked input schema: /deploymentId: expected string, found number"
        ),
        "{}",
        rejected.stderr
    );
    assert_eq!(rejected.requested, ["vercel-find-project"]);
}

#[test]
fn an_oracle_generated_without_locked_schemas_refuses_to_send_captured_arguments() {
    let scenario = load("code/vercel/rollback-storefront-production");
    let run = run_rollback_oracle_with(&scenario, json!("dpl_8o704GjcwumkLAaxPwjoxb4V6QDF"), false);
    assert!(!run.success);
    assert!(
        run.stderr
            .contains("locked input schema for captured arguments was not generated"),
        "{}",
        run.stderr
    );
    assert_eq!(run.requested, ["vercel-find-project"]);
}
