//! taskgen [--scenarios scenarios] [--catalog catalog/services.toml] [--out tasks]
//!         [--world-image IMG] [--tools-image IMG] [--check] [--print-images] [--inventory PATH] [--include-examples]
//!         [--exposure VARIANT [--exposure-seed N] [--exposure-token-budget N]]
//!         [--awareness-condition CELL] [--runner-admission optional|required]
//!
//! Validates every scenario and regenerates its Harbor task. The images default to the
//! published, content-tagged references (`scenario::images`). `--check` fails if `--out`
//! differs from what would be generated (CI guard for hand edits and unpublished sources).
//! `--print-images` prints the two references (the defaults unless overridden) and exits.
//! Sources are hashed only for a reference that is not overridden.
//!
//! `--exposure` (relevant_only, context_feasible, organization_pack, full_catalog,
//! randomized_order, confusing_distractors) controls which pinned MCP endpoints and tools
//! each task registers; the default full catalog generates exactly the usual output.
//! `--awareness-condition` (neutral_minimal, neutral_ordinary, evaluation_minimal,
//! evaluation_ordinary) selects the matching framing and workspace profile together.
//! `--runner-admission optional` (the default, and what the committed `tasks/` use) lets a
//! stock Harbor runner score a trial from the sealed world evidence; `required` generates
//! tasks that only the strict local controller (`deploy/run-trial.sh`) can score.

use std::path::{Path, PathBuf};

use anyhow::{Context, bail};
use catalog::Catalog;
use scenario::{
    discover,
    framing::Framing,
    images,
    oracle_tools::OracleTools,
    profile::WorkspaceProfile,
    taskgen::{Conditions, Images, RunnerAdmissionMode, Treatments, write_task_with_treatments},
};

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let mut scenarios = PathBuf::from("scenarios");
    let mut catalog_path = PathBuf::from("catalog/services.toml");
    let mut out = PathBuf::from("tasks");
    let mut world_image = None;
    let mut tools_image = None;
    let mut print_images = false;
    let mut inventory_path = None;
    let mut check = false;
    let mut include_examples = false;
    let mut workspace_profile = WorkspaceProfile::Minimal;
    let mut framing = Framing::Neutral;
    let mut review_policy_path = None;
    let mut exposure_variant = scenario::exposure::ExposureVariant::FullCatalog;
    let mut exposure_seed = 0u64;
    let mut exposure_token_budget = None;
    let mut awareness: Option<scenario::awareness::AwarenessCell> = None;
    let mut runner_admission = RunnerAdmissionMode::Optional;
    while let Some(arg) = args.next() {
        let mut value = || args.next().with_context(|| format!("{arg} needs a value"));
        match arg.as_str() {
            "--scenarios" => scenarios = PathBuf::from(value()?),
            "--catalog" => catalog_path = PathBuf::from(value()?),
            "--out" => out = PathBuf::from(value()?),
            "--world-image" => world_image = Some(value()?),
            "--tools-image" => tools_image = Some(value()?),
            "--check" => check = true,
            "--include-examples" => include_examples = true,
            "--workspace-profile" => workspace_profile = value()?.parse()?,
            "--framing" => framing = value()?.parse()?,
            "--review-policy" => review_policy_path = Some(PathBuf::from(value()?)),
            "--exposure" => exposure_variant = value()?.parse().map_err(anyhow::Error::msg)?,
            "--exposure-seed" => exposure_seed = value()?.parse()?,
            "--exposure-token-budget" => exposure_token_budget = Some(value()?.parse()?),
            "--awareness-condition" => {
                awareness = Some(value()?.parse().map_err(anyhow::Error::msg)?)
            }
            "--runner-admission" => {
                runner_admission = value()?.parse().map_err(anyhow::Error::msg)?
            }
            "--print-images" => print_images = true,
            "--inventory" => inventory_path = Some(PathBuf::from(value()?)),
            other => bail!("unexpected argument {other:?}"),
        }
    }
    let review_policy: Option<mock_core::review::ReviewPolicy> = review_policy_path
        .map(|path| {
            let policy: mock_core::review::ReviewPolicy =
                serde_json::from_slice(&std::fs::read(&path)?)
                    .with_context(|| format!("parsing review policy {}", path.display()))?;
            policy.validate().map_err(anyhow::Error::msg)?;
            Ok::<_, anyhow::Error>(policy)
        })
        .transpose()?;
    // Hash the image sources only for a reference that wasn't given.
    let root = Path::new(".");
    let images = Images {
        world: world_image.map_or_else(|| images::world(root), Ok)?,
        tools: tools_image.map_or_else(|| images::tools(root), Ok)?,
    };
    if print_images {
        println!("{}\n{}", images.world, images.tools);
        return Ok(());
    }
    let catalog = Catalog::load(&catalog_path)?;
    catalog.validate()?;
    let all = discover(&scenarios)?;
    let tools = OracleTools::load(root, &catalog)?;
    if let Some(cell) = awareness {
        framing = scenario::awareness::framing(cell);
        workspace_profile = scenario::awareness::workspace_profile(cell, workspace_profile)?;
    }
    let exposure = (exposure_variant != scenario::exposure::ExposureVariant::FullCatalog)
        .then(|| {
            Ok::<_, anyhow::Error>(scenario::exposure::Exposure {
                variant: exposure_variant,
                seed: exposure_seed,
                token_budget: exposure_token_budget,
                pinned: scenario::exposure::PinnedCatalog::load(root, &catalog)?,
            })
        })
        .transpose()?;
    let validated = all
        .iter()
        .map(|scenario| scenario.validate(&scenarios, &catalog, &tools))
        .collect::<anyhow::Result<Vec<_>>>()?;
    let inventory = scenario::coverage::inventory(&all, validated, &tools)?;
    let inventory_json = serde_json::to_string_pretty(&inventory)? + "\n";
    if check && let Some(path) = &inventory_path {
        let current =
            std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
        if current != inventory_json {
            bail!(
                "{} is stale; regenerate the source coverage inventory",
                path.display()
            )
        }
    }
    // Example scenarios exercise the mocks but are not benchmark tasks.
    let tasks: Vec<_> = all
        .iter()
        .filter(|s| include_examples || s.scored())
        .collect();

    // The production planner names trials by the first 44 characters of the task name;
    // two tasks that share them fail the whole run.
    let mut prefixes = std::collections::BTreeMap::new();
    for s in &tasks {
        let name = s.task_name();
        let slug: String = name
            .to_lowercase()
            .chars()
            .map(|character| {
                if character.is_ascii_alphanumeric() {
                    character
                } else {
                    '-'
                }
            })
            .take(44)
            .collect();
        if let Some(other) = prefixes.insert(slug, name.clone()) {
            bail!("tasks {other} and {name} share their first 44 characters");
        }
    }

    let target = if check {
        std::env::temp_dir().join(format!("tool-bench-taskgen-check-{}", std::process::id()))
    } else {
        out.clone()
    };
    if target.exists() {
        if !check {
            ensure_generated(&target)?;
        }
        std::fs::remove_dir_all(&target)?;
    }
    std::fs::create_dir_all(&target)?;
    for s in &tasks {
        write_task_with_treatments(
            s,
            &catalog,
            &images,
            &target,
            Conditions {
                workspace_profile,
                framing,
                review_policy: review_policy.as_ref(),
                locked_tools: Some(&tools),
            },
            Treatments {
                exposure: exposure.as_ref(),
                awareness,
                runner_admission,
            },
        )?;
    }
    if check {
        let status = std::process::Command::new("diff")
            .arg("-r")
            .arg(&out)
            .arg(&target)
            .status()?;
        std::fs::remove_dir_all(&target)?;
        if !status.success() {
            bail!("{} is stale; run taskgen", out.display());
        }
        println!("ok: {} tasks up to date", tasks.len());
    } else {
        if let Some(path) = &inventory_path {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?
            }
            std::fs::write(path, &inventory_json)?;
        }
        println!(
            "wrote {} tasks to {} ({} scenarios validated)",
            tasks.len(),
            out.display(),
            all.len()
        );
    }
    Ok(())
}

/// Refuses to wipe a directory that holds anything other than generated task directories.
fn ensure_generated(dir: &Path) -> anyhow::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if !path.join("task.toml").is_file() {
            bail!(
                "{} is not a taskgen output directory ({} is not a task); refusing to delete it",
                dir.display(),
                path.display()
            );
        }
    }
    Ok(())
}
