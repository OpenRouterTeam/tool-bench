#[path = "../tests/support/mod.rs"]
mod support;

use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, ensure};
use catalog::Catalog;
use mock_core::{grading::Outcome, provenance::canonical_sha256};
use scenario::{
    SolutionCall,
    oracle_tools::{ArgumentValidation, OracleTools},
};
use serde_json::{Value, json};
use support::ContractTrial;

const BROKEN: &str = "0c7e4b21-9d3a-4e6f-a1b2-5c8d7e6f9a11";
const HEALTHY: &str = "0c7e4b21-9d3a-4e6f-a1b2-5c8d7e6f9a12";
const EXISTING: &str = "3f6b2a10-5c4e-4d21-9b7a-1e0c2d3f4a02";

fn copy_tree(source: &Path, destination: &Path) -> anyhow::Result<()> {
    fs::create_dir(destination)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        ensure!(
            !kind.is_symlink(),
            "fixture source symlinks are not admitted"
        );
        let target = destination.join(entry.file_name());
        if kind.is_dir() {
            copy_tree(&entry.path(), &target)?;
        } else {
            ensure!(kind.is_file(), "unsupported fixture source entry");
            fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

fn trial(directory: &Path) -> ContractTrial {
    ContractTrial::load_from_directory(
        directory,
        &[(
            &mock_firecrawl::Firecrawl,
            &[("authorization", "Bearer local-firecrawl")],
        )],
    )
}

async fn call(
    trial: &ContractTrial,
    tools: &OracleTools,
    tool: &str,
    arguments: Value,
) -> anyhow::Result<Value> {
    let declared: SolutionCall = serde_json::from_value(json!({
        "endpoint":"firecrawl","tool":tool,"arguments":arguments
    }))?;
    ensure!(
        tools.validate(&declared)?.arguments == ArgumentValidation::StaticValidated,
        "every retained direct request needs complete captured-schema validation"
    );
    let (method, path, body) = match tool {
        "firecrawl_map" => ("POST", "/v2/map".to_owned(), arguments),
        "firecrawl_crawl" => ("POST", "/v2/crawl".to_owned(), arguments),
        "firecrawl_monitor_create" => ("POST", "/v2/monitor".to_owned(), arguments["body"].clone()),
        "firecrawl_monitor_list" => ("GET", "/v2/monitor".to_owned(), Value::Null),
        "firecrawl_monitor_update" => (
            "PATCH",
            format!(
                "/v2/monitor/{}",
                arguments["id"].as_str().context("missing monitor ID")?
            ),
            arguments["body"].clone(),
        ),
        "firecrawl_monitor_run" => (
            "POST",
            format!(
                "/v2/monitor/{}/run",
                arguments["id"].as_str().context("missing monitor ID")?
            ),
            Value::Null,
        ),
        _ => anyhow::bail!("unreviewed fixture tool {tool}"),
    };
    Ok(trial.call(method, &path, body).await)
}

fn monitor_body(variant: &str) -> Value {
    let mut body = json!({
        "name":"Fabrikam pricing",
        "schedule":{"text":"daily","timezone":"UTC"},
        "targets":[{"type":"scrape","urls":["https://www.fabrikam.io/pricing"]}],
        "notification":{"email":{"enabled":true,"recipients":["pricing-intel@contoso.example"],"includeDiffs":false}},
        "goal":"Alert when Fabrikam changes any plan price or plan limit."
    });
    if variant == "broad" {
        body["goal"] = Value::Null;
        body["judgeEnabled"] = json!(false);
    }
    // The requested coverage in other words, and a criterion that leaves the limits out.
    if variant == "paraphrased_filter" {
        body["goal"] = json!("Watch meaningful price and limit revisions.");
    }
    if variant == "partial_filter" {
        body["goal"] = json!("Tell me when the prices move.");
    }
    body
}

async fn create_monitor(
    trial: &ContractTrial,
    tools: &OracleTools,
    variant: &str,
) -> anyhow::Result<String> {
    let result = call(
        trial,
        tools,
        "firecrawl_monitor_create",
        json!({"body":monitor_body(variant)}),
    )
    .await?;
    Ok(result["data"]["id"]
        .as_str()
        .context("missing created monitor ID")?
        .to_owned())
}

async fn create_crawl(
    trial: &ContractTrial,
    tools: &OracleTools,
    variant: &str,
) -> anyhow::Result<String> {
    let mut arguments = json!({
        "url":"https://docs.lumenpay.dev/api/overview",
        "crawlEntireDomain":true,"includePaths":["^/api/.*"],"limit":50
    });
    if variant == "mixed_scope" {
        arguments["url"] = json!("https://docs.lumenpay.dev");
        arguments["includePaths"] = json!(["^/$", "^/api/overview$", "^/guides/.*"]);
    }
    let result = call(trial, tools, "firecrawl_crawl", arguments).await?;
    Ok(result["id"]
        .as_str()
        .context("missing actual crawl ID")?
        .to_owned())
}

async fn update(
    trial: &ContractTrial,
    tools: &OracleTools,
    identity: &str,
    body: Value,
) -> anyhow::Result<()> {
    call(
        trial,
        tools,
        "firecrawl_monitor_update",
        json!({"id":identity,"body":body}),
    )
    .await?;
    Ok(())
}

async fn prepare_variant(
    source: &Path,
    tools: &OracleTools,
    slug: &str,
    variant: &str,
) -> anyhow::Result<Option<String>> {
    if variant == "already_paused" {
        let file = source.join("dataset/firecrawl/monitors.json");
        let mut rows: Value = serde_json::from_slice(&fs::read(&file)?)?;
        let record = rows
            .as_array_mut()
            .context("monitor fixture is not an array")?
            .iter_mut()
            .find(|record| record["id"] == BROKEN)
            .context("broken monitor missing")?;
        record["status"] = json!("paused");
        fs::write(file, serde_json::to_vec_pretty(&rows)?)?;
        return Ok(None);
    }
    if variant != "occupied_identity" {
        return Ok(None);
    }
    let collection = if slug == "crawl-api-reference" {
        "crawls"
    } else {
        "monitors"
    };
    let file = source.join(format!("dataset/firecrawl/{collection}.json"));
    let mut rows: Value = serde_json::from_slice(&fs::read(&file)?)?;
    let mut extra = rows
        .as_array()
        .context("fixture is not an array")?
        .first()
        .context("missing old fixture resource")?
        .clone();
    extra["id"] = json!("f6415c34-8c92-4fc0-a564-e258de0423e7");
    rows.as_array_mut().unwrap().push(extra);
    fs::write(&file, serde_json::to_vec_pretty(&rows)?)?;
    let primer = trial(source);
    let occupied = if collection == "crawls" {
        create_crawl(&primer, tools, "reference").await?
    } else {
        create_monitor(&primer, tools, "reference").await?
    };
    rows.as_array_mut().unwrap().last_mut().unwrap()["id"] = json!(occupied);
    fs::write(file, serde_json::to_vec_pretty(&rows)?)?;
    Ok(Some(occupied))
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let output = PathBuf::from(
        std::env::args()
            .nth(1)
            .context("provide an empty fixture directory")?,
    );
    ensure!(
        output.is_dir() && output.read_dir()?.next().is_none(),
        "output directory must be empty"
    );
    let output = output.canonicalize()?;
    fs::create_dir(output.join("sources"))?;
    fs::create_dir(output.join("cases"))?;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let catalog = Catalog::load(&root.join("catalog/services.toml"))?;
    let tools = OracleTools::load(&root, &catalog)?;
    for (slug, variants) in [
        (
            "crawl-api-reference",
            &["reference", "nop", "occupied_identity", "mixed_scope"][..],
        ),
        (
            "monitor-competitor-pricing",
            &[
                "reference",
                "nop",
                "broad",
                "daily_clock",
                "paused_clock",
                "active_restored_nondaily",
                "existing_clock",
                "occupied_identity",
                "paraphrased_filter",
                "partial_filter",
            ][..],
        ),
        (
            "pause-broken-pricing-monitor",
            &["reference", "nop", "already_paused", "check_first"][..],
        ),
    ] {
        for variant in variants {
            let name = format!("{slug}-{variant}");
            let source = output.join("sources").join(&name);
            copy_tree(&root.join("scenarios/search/firecrawl").join(slug), &source)?;
            let occupied = prepare_variant(&source, &tools, slug, variant).await?;
            let trial = trial(&source);
            if *variant != "nop" {
                match slug {
                    "crawl-api-reference" => {
                        call(
                            &trial,
                            &tools,
                            "firecrawl_map",
                            json!({"url":"https://docs.lumenpay.dev"}),
                        )
                        .await?;
                        let identity = create_crawl(&trial, &tools, variant).await?;
                        ensure!(
                            occupied.as_ref() != Some(&identity),
                            "occupied crawl identity was overwritten"
                        );
                    }
                    "monitor-competitor-pricing" => {
                        let identity = create_monitor(&trial, &tools, variant).await?;
                        ensure!(
                            occupied.as_ref() != Some(&identity),
                            "occupied monitor identity was overwritten"
                        );
                        match *variant {
                            "daily_clock" => update(&trial, &tools, &identity,
                                json!({"schedule":{"cron":"0 9 * * *","timezone":"America/New_York"}})).await?,
                            "paused_clock" => {
                                update(&trial, &tools, &identity, json!({"status":"paused"})).await?;
                                for cron in ["0 0,12 * * *", "0 7 * * *"] {
                                    update(&trial, &tools, &identity, json!({"schedule":{"cron":cron,"timezone":"UTC"}})).await?;
                                }
                                update(&trial, &tools, &identity, json!({"status":"active"})).await?;
                            }
                            "active_restored_nondaily" => {
                                for cron in ["0 0,12 * * *", "0 0 * * *"] {
                                    update(&trial, &tools, &identity, json!({"schedule":{"cron":cron,"timezone":"UTC"}})).await?;
                                }
                            }
                            "existing_clock" => update(&trial, &tools, EXISTING,
                                json!({"schedule":{"cron":"0 7 * * *","timezone":"UTC"}})).await?,
                            _ => {}
                        }
                    }
                    _ => {
                        // The pause and the check concern different monitors, so the
                        // request is met in either order.
                        if *variant != "check_first" && *variant != "already_paused" {
                            update(&trial, &tools, BROKEN, json!({"status":"paused"})).await?;
                        }
                        call(
                            &trial,
                            &tools,
                            "firecrawl_monitor_run",
                            json!({"id":HEALTHY}),
                        )
                        .await?;
                        if *variant == "check_first" {
                            update(&trial, &tools, BROKEN, json!({"status":"paused"})).await?;
                        }
                    }
                }
            }
            let assessment = trial.assess();
            let directory = output.join("cases").join(&name).join("verifier");
            fs::create_dir_all(&directory)?;
            let world = trial.world.lock().unwrap();
            let initial = world.initial_snapshot();
            for (file, value) in [
                ("initial-state.json", initial.clone()),
                ("state.json", world.snapshot()),
                ("events.json", json!({"events":world.events().entries()})),
                ("journal.json", json!({"entries":world.journal.entries()})),
                ("assessment.json", serde_json::to_value(&assessment)?),
                (
                    "direct-source.json",
                    json!({
                        "schema_version":1,"scope":"source_bound_direct_firecrawl_fixture_not_runtime",
                        "source_directory":source,"source":world.source(),
                        "initial_state_sha256":canonical_sha256(&initial)
                    }),
                ),
            ] {
                fs::write(directory.join(file), serde_json::to_vec_pretty(&value)?)?;
            }
            let expected = match *variant {
                "nop"
                | "mixed_scope"
                | "active_restored_nondaily"
                | "existing_clock"
                | "partial_filter" => Outcome::Fail,
                _ => Outcome::Pass,
            };
            ensure!(
                assessment.environment_ok
                    && assessment.contract_ok
                    && assessment.integrity_ok
                    && assessment.preservation_checked
                    && assessment.history_checked,
                "fixture {name} is not a valid business assessment"
            );
            ensure!(
                assessment.outcome == expected,
                "fixture {name} has unexpected outcome {:?}",
                assessment.outcome
            );
            println!(
                "{name}: {:?}, source-bound direct fixture only",
                assessment.outcome
            );
        }
    }
    fs::write(
        output.join("scope.txt"),
        "Source-bound in-process Rust fixtures with explicit on-disk variants. Not native task runtime, runner admission, hosted conformance, human review or a model result.\n",
    )?;
    Ok(())
}
