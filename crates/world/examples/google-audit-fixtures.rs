#[path = "../tests/support/mod.rs"]
mod support;

use std::{fs, path::PathBuf};

use anyhow::{Context, ensure};
use catalog::Catalog;
use scenario::{
    SolutionCall,
    oracle_tools::{ArgumentValidation, OracleTools},
};
use serde_json::{Value, json};
use support::ContractTrial;

async fn call(
    trial: &ContractTrial,
    tools: &OracleTools,
    name: &str,
    arguments: Value,
) -> anyhow::Result<()> {
    let declared: SolutionCall = serde_json::from_value(json!({
        "endpoint":"google-workspace","tool":name,"arguments":arguments
    }))?;
    ensure!(
        tools.validate(&declared)?.arguments == ArgumentValidation::StaticValidated,
        "direct source fixtures require completely validated advertised arguments"
    );
    trial
        .call(
            "POST",
            "/_mcp/tools/call",
            json!({"name":name,"arguments":arguments}),
        )
        .await;
    Ok(())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let output = PathBuf::from(
        std::env::args()
            .nth(1)
            .context("provide an empty output directory")?,
    );
    ensure!(
        output.is_dir() && output.read_dir()?.next().is_none(),
        "output must be an empty directory"
    );
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let catalog = Catalog::load(&root.join("catalog/services.toml"))?;
    let tools = OracleTools::load(&root, &catalog)?;
    for (slug, variants) in [
        (
            "mark-northwind-renewed",
            &[
                "reference",
                "nop",
                "serial",
                "wrong_date",
                "restored_neighbor",
            ][..],
        ),
        (
            "schedule-q1-planning",
            &[
                "reference",
                "nop",
                "none",
                "externalOnly",
                "silent_meet",
                "wrong_instant",
                "duplicate",
            ][..],
        ),
        (
            "share-soc2-with-contoso",
            &["reference", "nop", "uppercase", "writer", "wrong_principal"][..],
        ),
    ] {
        for variant in variants {
            let trial = ContractTrial::load_with_dataset(
                &format!("collab/google/{slug}"),
                &mock_google::Google,
                &[],
                |_| {},
            );
            let mut calls = trial.scenario.solution.calls.clone();
            let last = calls.last_mut().context("missing reference mutation")?;
            match *variant {
                "serial" => last.arguments["values"] = json!("[[46721,\"Renewed\"]]"),
                "wrong_date" => last.arguments["values"] = json!([["2040-01-01", "Renewed"]]),
                "none" | "externalOnly" => last.arguments["send_updates"] = json!(variant),
                "silent_meet" => last.arguments["add_google_meet"] = json!(false),
                "wrong_instant" => {
                    last.arguments.as_object_mut().unwrap().remove("timezone");
                    last.arguments["start_time"] = json!("2026-11-03T19:00:00.000000001Z");
                    last.arguments["end_time"] = json!("2026-11-03T19:45:00Z");
                }
                "uppercase" => {
                    last.arguments["share_with"] = json!("DANA.WHITFIELD@CONTOSO.EXAMPLE")
                }
                "writer" => last.arguments["role"] = json!("writer"),
                "wrong_principal" => {
                    last.arguments["share_with"] = json!("dana.whitfield@octo-corp.example")
                }
                _ => {}
            }
            if *variant != "nop" {
                for request in &calls {
                    ensure!(
                        !request.template
                            && request.capture.is_empty()
                            && request.replacements.is_empty(),
                        "fixture generator only supports unbound source references"
                    );
                    call(&trial, &tools, &request.tool, request.arguments.clone()).await?;
                }
            }
            match *variant {
                "restored_neighbor" => {
                    let spreadsheet = &calls.last().unwrap().arguments["spreadsheet_id"];
                    let original = trial.state()["drive_files"][spreadsheet.as_str().unwrap()]
                        ["sheets"][0]["values"][3][2].clone();
                    for value in [json!("2040-01-01"), original] {
                        call(&trial, &tools, "modify_sheet_values", json!({
                            "spreadsheet_id":spreadsheet,"range_name":"Renewals!C4","values":[[value]]
                        })).await?;
                    }
                }
                "silent_meet" => {
                    let created: Vec<_> = {
                        let world = trial.world.lock().unwrap();
                        let initial = world.initial_snapshot();
                        let current = world.snapshot();
                        current["services"]["google"]["calendar_events"]
                            .as_object()
                            .unwrap()
                            .keys()
                            .filter(|identity| {
                                initial["services"]["google"]["calendar_events"]
                                    .get(identity.as_str())
                                    .is_none()
                            })
                            .cloned()
                            .collect()
                    };
                    ensure!(created.len() == 1, "expected one actual created event");
                    call(&trial, &tools, "manage_event", json!({
                        "action":"update","event_id":created[0],"add_google_meet":true,"send_updates":"none"
                    })).await?;
                }
                "duplicate" => {
                    let request = calls.last().unwrap();
                    call(&trial, &tools, &request.tool, request.arguments.clone()).await?;
                }
                _ => {}
            }
            if slug == "share-soc2-with-contoso" && *variant != "nop" {
                let arguments = json!({"file_id":calls.last().unwrap().arguments["file_id"]});
                let before = trial.state();
                for name in ["get_drive_file_permissions", "get_drive_shareable_link"] {
                    call(&trial, &tools, name, arguments.clone()).await?;
                }
                ensure!(
                    trial.state() == before,
                    "permission reads must not mutate business state"
                );
            }
            let assessment = trial.assess();
            ensure!(
                assessment.environment_ok
                    && assessment.contract_ok
                    && assessment.integrity_ok
                    && assessment.contract_version == 2
                    && assessment.preservation_checked
                    && assessment.history_checked
                    && !assessment.adjudication_needed,
                "fixture must be a valid business assessment"
            );
            let expected = if matches!(
                *variant,
                "reference" | "serial" | "none" | "externalOnly" | "silent_meet" | "uppercase"
            ) {
                mock_core::grading::Outcome::Pass
            } else {
                mock_core::grading::Outcome::Fail
            };
            ensure!(
                assessment.outcome == expected,
                "source control {slug}/{variant} has an unexpected business verdict"
            );
            let directory = output.join(format!("{slug}-{variant}")).join("verifier");
            fs::create_dir_all(&directory)?;
            let world = trial.world.lock().unwrap();
            for (name, value) in [
                ("initial-state.json", world.initial_snapshot()),
                ("state.json", world.snapshot()),
                ("events.json", json!({"events":world.events().entries()})),
                ("journal.json", json!({"entries":world.journal.entries()})),
                ("assessment.json", serde_json::to_value(&assessment)?),
            ] {
                fs::write(directory.join(name), serde_json::to_vec_pretty(&value)?)?;
            }
            println!("{slug}/{variant}: {:?}", assessment.outcome);
        }
    }
    fs::write(
        output.join("scope.txt"),
        "Direct in-process Rust workflow fixtures. Not native-client execution, runner admission, a frozen runtime campaign, or a model result.\n",
    )?;
    fs::write(
        output.join("result-contracts.json"),
        serde_json::to_vec_pretty(&scenario::oracle_results::registry(&catalog, &[])?)?,
    )?;
    Ok(())
}
