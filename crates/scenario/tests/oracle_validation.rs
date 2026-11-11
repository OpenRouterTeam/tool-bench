use std::path::Path;

use catalog::Catalog;
use scenario::Scenario;
use scenario::oracle_tools::ArgumentValidation;
use scenario::oracle_tools::OracleTools;
use serde_json::json;

fn fixture(schema: serde_json::Value) -> (tempfile::TempDir, OracleTools) {
    let directory = tempfile::tempdir().unwrap();
    let catalog = Catalog::parse(
        r#"
[[service]]
name = "fixture"
group = "example"
mode = "replica"
world_port = 8100
[[service.endpoint]]
id = "fixture"
mcp_port = 7100
runtime = "replica"
snapshot = "snapshot.json"
"#,
    )
    .unwrap();
    std::fs::write(
        directory.path().join("snapshot.json"),
        serde_json::to_vec(&json!({
            "tools":[{"name":"tool","inputSchema":schema}]
        }))
        .unwrap(),
    )
    .unwrap();
    let tools = OracleTools::load(directory.path(), &catalog).unwrap();
    (directory, tools)
}

fn call(arguments: serde_json::Value, template: bool) -> scenario::SolutionCall {
    scenario::SolutionCall {
        allow_api_errors: Vec::new(),
        endpoint: "fixture".into(),
        tool: "tool".into(),
        arguments,
        template,
        capture: vec![],
        replacements: vec![],
    }
}

fn schema() -> serde_json::Value {
    json!({
        "type":"object","properties":{
            "count":{"$ref":"#/$defs/count"},"body":{"type":"string","minLength":2}
        },
        "required":["count","body"],"additionalProperties":false,
        "$defs":{"count":{"type":"integer","minimum":1}}
    })
}

#[test]
fn local_schema_references_are_checked_and_dynamic_values_are_explicitly_deferred() {
    let (_directory, tools) = fixture(schema());
    let static_call = tools
        .validate(&call(json!({"count":1,"body":"ok"}), false))
        .unwrap();
    assert_eq!(static_call.arguments, ArgumentValidation::StaticValidated);
    assert!(static_call.deferred_paths.is_empty());
    let dynamic = tools
        .validate(&call(json!({"count":1,"body":"${later}"}), true))
        .unwrap();
    assert_eq!(dynamic.arguments, ArgumentValidation::PartiallyDeferred);
    assert_eq!(dynamic.deferred_paths, vec!["/body"]);
    for arguments in [
        json!({"count":0,"body":"${later}"}),
        json!({"count":"wrong","body":"${later}"}),
        json!({"count":1,"body":"${later}","unexpected":"${another}"}),
        json!({"body":"${later}"}),
    ] {
        assert!(tools.validate(&call(arguments, true)).is_err());
    }
}

#[test]
fn whole_captured_arguments_and_value_dependent_roots_are_not_claimed_schema_validated() {
    let (_directory, tools) = fixture(schema());
    let check = tools.validate(&call(json!("${arguments}"), true)).unwrap();
    assert_eq!(check.arguments, ArgumentValidation::PartiallyDeferred);
    assert_eq!(check.deferred_paths, vec![""]);
    assert!(
        tools
            .validate(&call(json!("prefix ${arguments}"), true))
            .is_err()
    );
    let mut composed = schema();
    composed["anyOf"] =
        json!([{"properties":{"count":{"const":1}}},{"properties":{"count":{"const":2}}}]);
    let (_directory, tools) = fixture(composed);
    let check = tools
        .validate(&call(json!({"count":"${count}","body":"ok"}), true))
        .unwrap();
    assert_eq!(check.deferred_paths, vec![""]);
}

#[test]
fn schema_retrieval_never_reads_network_or_operator_files() {
    let file = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(file.path(), r#"{"type":"object"}"#).unwrap();
    for reference in [
        format!("file://{}", file.path().display()),
        "http://127.0.0.1:9/forbidden".into(),
    ] {
        let (_directory, tools) =
            fixture(json!({"type":"object","properties":{},"$ref":reference}));
        assert!(tools.validate(&call(json!({}), false)).is_err());
    }
}

#[test]
fn validation_diagnostics_report_locations_not_argument_values() {
    let (_directory, tools) = fixture(schema());
    let error = tools
        .validate(&call(
            json!({"count":"synthetic-sensitive-value","body":"ok"}),
            false,
        ))
        .unwrap_err()
        .to_string();
    assert!(error.contains("/count"));
    assert!(!error.contains("synthetic-sensitive-value"));
}

#[test]
fn duplicate_snapshot_tools_are_rejected_instead_of_overwriting_a_definition() {
    let directory = tempfile::tempdir().unwrap();
    let catalog = Catalog::parse(
        r#"
[[service]]
name = "fixture"
group = "example"
mode = "replica"
world_port = 8100
[[service.endpoint]]
id = "fixture"
mcp_port = 7100
runtime = "replica"
snapshot = "snapshot.json"
"#,
    )
    .unwrap();
    std::fs::write(
        directory.path().join("snapshot.json"),
        serde_json::to_vec(&json!({
            "tools":[{"name":"same","inputSchema":schema()},{"name":"same","inputSchema":schema()}]
        }))
        .unwrap(),
    )
    .unwrap();
    assert!(OracleTools::load(directory.path(), &catalog).is_err());
}

fn root() -> &'static Path {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
}

fn catalog() -> Catalog {
    Catalog::load(&root().join("catalog/services.toml")).unwrap()
}

fn tools() -> OracleTools {
    OracleTools::load(root(), &catalog()).unwrap()
}

#[test]
fn a_known_endpoint_does_not_make_an_unknown_reference_tool_admissible() {
    let mut scenario =
        Scenario::load(&root().join("scenarios/collab/linear/file-safari-crash-bug")).unwrap();
    scenario.solution.calls[2].tool = "linear_createIssu_typo".into();
    assert!(
        scenario
            .validate(&root().join("scenarios"), &catalog(), &tools())
            .is_err()
    );
}

#[test]
fn statically_wrong_argument_types_and_closed_schema_properties_are_rejected() {
    let mut scenario =
        Scenario::load(&root().join("scenarios/collab/linear/file-safari-crash-bug")).unwrap();
    scenario.solution.calls[2].arguments["priority"] = json!("High");
    assert!(
        scenario
            .validate(&root().join("scenarios"), &catalog(), &tools())
            .is_err()
    );
    let mut scenario =
        Scenario::load(&root().join("scenarios/collab/jira/ship-refund-retries")).unwrap();
    scenario.solution.calls[3].arguments["invented_argument"] = json!(true);
    assert!(
        scenario
            .validate(&root().join("scenarios"), &catalog(), &tools())
            .is_err()
    );
}

#[test]
fn a_capture_does_not_exempt_unrelated_static_arguments_from_validation() {
    let mut scenario =
        Scenario::load(&root().join("scenarios/business/multi/zendesk-refund-via-stripe")).unwrap();
    scenario.solution.calls[5].arguments["public"] = json!("yes");
    assert!(
        scenario
            .validate(&root().join("scenarios"), &catalog(), &tools())
            .is_err()
    );
}
