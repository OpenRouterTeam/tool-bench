use std::{fs, path::PathBuf};

use anyhow::{Context, bail, ensure};
use catalog::Catalog;
use scenario::{
    SolutionCall,
    oracle_tools::{ArgumentValidation, OracleTools},
};
use serde_json::{Value, json};

fn has_binding(value: &Value) -> bool {
    match value {
        Value::String(value) => value.contains("${"),
        Value::Array(values) => values.iter().any(has_binding),
        Value::Object(values) => values.values().any(has_binding),
        _ => false,
    }
}

fn main() -> anyhow::Result<()> {
    let mut arguments = std::env::args().skip(1);
    let endpoint = arguments
        .next()
        .context("usage: programcheck ENDPOINT PROGRAM_DIRECTORY...")?;
    let directories = arguments.map(PathBuf::from).collect::<Vec<_>>();
    ensure!(
        !directories.is_empty(),
        "provide at least one program directory"
    );
    let root = std::env::current_dir()?;
    let catalog = Catalog::load(&root.join("catalog/services.toml"))?;
    let tools = OracleTools::load(&root, &catalog)?;
    ensure!(
        tools
            .identities()
            .iter()
            .any(|identity| identity.endpoint == endpoint),
        "unknown catalog endpoint {endpoint}",
    );
    let mut checked = Vec::new();
    for directory in directories {
        ensure!(
            directory.is_dir(),
            "{} is not a program directory",
            directory.display()
        );
        let mut paths = fs::read_dir(&directory)?
            .map(|entry| entry.map(|entry| entry.path()))
            .collect::<std::io::Result<Vec<_>>>()?;
        paths.sort();
        let beginning = checked.len();
        for path in paths {
            if path.extension().is_none_or(|extension| extension != "json") {
                continue;
            }
            let request: Value = serde_json::from_slice(&fs::read(&path)?)
                .with_context(|| format!("parsing {}", path.display()))?;
            if request["method"] != "tools/call" {
                continue;
            }
            ensure!(
                request["jsonrpc"] == "2.0"
                    && request["params"].is_object()
                    && (request["id"].is_number() || request["id"].is_string()),
                "{} lacks its MCP request context",
                path.display()
            );
            let name = request["params"]["name"]
                .as_str()
                .filter(|name| !name.is_empty())
                .with_context(|| format!("{} lacks a tool name", path.display()))?;
            if has_binding(&request["params"]["arguments"]) {
                bail!(
                    "{} contains possible render bindings; runtime argument validation is still required",
                    path.display()
                );
            }
            let call: SolutionCall = serde_json::from_value(json!({
                "endpoint":endpoint,"tool":name,"arguments":request["params"]["arguments"]
            }))?;
            let validation = tools
                .validate(&call)
                .with_context(|| format!("validating {}", path.display()))?;
            ensure!(
                validation.arguments == ArgumentValidation::StaticValidated,
                "{} is not completely statically validated",
                path.display()
            );
            checked.push(json!({"path":path,"validation":validation}));
        }
        ensure!(
            checked.len() > beginning,
            "{} contains no MCP request JSON files",
            directory.display()
        );
    }
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "schema_version":1,
            "scope":"locked_schema_checks_of_retained_static_request_files",
            "limits":[
                "unused_request_files_may_be_included",
                "shell_execution_and_actual_dispatch_are_not_proved",
                "captured_and_runtime_arguments_are_not_validated",
                "operation_support_and_business_semantics_are_not_certified"
            ],
            "validated_request_files":checked.len(),
            "requests":checked
        }))?
    );
    Ok(())
}
