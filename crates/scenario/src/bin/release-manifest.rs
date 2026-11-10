//! release-manifest --manifest <releases/NAME.json> [--scenarios scenarios] (--check | --freeze)
//! release-manifest --dataset-source <scenario-dir>
//!
//! `--check` fails unless the manifest is exactly what the current scenario
//! sources freeze to. `--freeze` rewrites only the derived task list and
//! revisions, keeping the authored header (identity, comparison design,
//! seals). `--dataset-source` prints what the real loader materializes for
//! one scenario; host tests use it to exercise `Dataset::load`.

use std::path::PathBuf;

use anyhow::{Context, bail};
use scenario::{discover, release_manifest};

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut manifest = None;
    let mut scenarios = PathBuf::from("scenarios");
    let mut mode = None;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--dataset-source" => {
                let directory = PathBuf::from(args.get(index + 1).context("scenario directory")?);
                let dataset = mock_core::dataset::Dataset::load(&directory)?;
                let materialized = dataset
                    .materialization
                    .as_ref()
                    .and_then(|bundle| bundle.files.get("materialization/dataset.json"))
                    .map(|bytes| serde_json::from_slice::<serde_json::Value>(bytes))
                    .transpose()?;
                println!(
                    "{}",
                    serde_json::to_string_pretty(&serde_json::json!({
                        "source": dataset.source,
                        "materialized": materialized,
                        "scripted_user": dataset.scripted_user,
                    }))?
                );
                return Ok(());
            }
            "--manifest" | "--scenarios" => {
                let value = PathBuf::from(args.get(index + 1).context("option needs a path")?);
                if args[index] == "--manifest" {
                    manifest = Some(value)
                } else {
                    scenarios = value
                }
                index += 1;
            }
            "--check" | "--freeze" if mode.is_none() => mode = Some(args[index].clone()),
            other => bail!("unsupported option {other}"),
        }
        index += 1;
    }
    let manifest = manifest.context("--manifest is required")?;
    let all = discover(&scenarios)?;
    let mut release = release_manifest::read(&manifest)?;
    match mode.as_deref() {
        Some("--check") => {
            release_manifest::check(&release, &all)?;
            release_manifest::check_synthetic_seals(&release, &manifest)?;
            println!(
                "ok: release {} ({}) freezes {} tasks",
                release.release_id,
                release.sha256(),
                release.tasks.len()
            );
        }
        Some("--freeze") => {
            release_manifest::refreeze(&mut release, &all)?;
            release_manifest::check_synthetic_seals(&release, &manifest)?;
            std::fs::write(&manifest, release_manifest::encode(&release))?;
            println!(
                "froze {} tasks into {} ({})",
                release.tasks.len(),
                manifest.display(),
                release.sha256()
            );
        }
        _ => bail!("choose --check or --freeze"),
    }
    Ok(())
}
