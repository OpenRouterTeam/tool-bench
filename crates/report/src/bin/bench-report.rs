//! bench-report <run-a-dir> <run-b-dir> [--json] [--allow-agent-difference]
//!              [--information-treatment framing|workspace|exposure]
//!              [--release manifest.json]
//!
//! Each dir holds Harbor trial results (local `harbor run -o` output, or a run's artifacts
//! downloaded from the run's artifact bucket).

use std::path::PathBuf;

use anyhow::{Context, bail};
use report::load_trials;

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("--trials") {
        if args.len() != 3 || args[2] != "--json" {
            bail!("usage: bench-report --trials <run-dir> --json");
        }
        println!(
            "{}",
            serde_json::to_string_pretty(&report::explain::summary(&PathBuf::from(&args[1]))?)?
        );
        return Ok(());
    }
    let mut seen = std::collections::BTreeSet::new();
    let a = PathBuf::from(
        args.first()
            .context("usage: bench-report <run-a> <run-b> [--json] [--allow-agent-difference]")?,
    );
    let b = PathBuf::from(
        args.get(1)
            .context("usage: bench-report <run-a> <run-b> [--json] [--allow-agent-difference]")?,
    );
    let mut index = 2;
    let mut review_policy = None;
    let mut review_store = None;
    let mut instructions_root = None;
    let mut json_output = false;
    let mut allow_agent_difference = false;
    let mut information = None;
    let mut release = None;
    while index < args.len() {
        let argument = &args[index];
        if !seen.insert(argument) {
            bail!("duplicate comparison option {argument}");
        }
        match argument.as_str() {
            "--json" => json_output = true,
            "--allow-agent-difference" => allow_agent_difference = true,
            "--release" => {
                index += 1;
                release = Some(report::release::load(&PathBuf::from(
                    args.get(index).context("--release needs a manifest path")?,
                ))?);
            }
            "--information-treatment" => {
                index += 1;
                information = Some(
                    args.get(index)
                        .context("--information-treatment needs framing, workspace or exposure")?
                        .parse()
                        .map_err(anyhow::Error::msg)?,
                );
            }
            "--review-policy" | "--review-store" | "--instructions-root" => {
                index += 1;
                let value = PathBuf::from(
                    args.get(index)
                        .with_context(|| format!("{argument} needs a path"))?,
                );
                match argument.as_str() {
                    "--review-policy" => review_policy = Some(value),
                    "--review-store" => review_store = Some(value),
                    _ => instructions_root = Some(value),
                }
            }
            _ => bail!("unsupported comparison option {argument}"),
        }
        index += 1;
    }
    let (left, right) = match (review_policy, review_store, instructions_root) {
        (None, None, None) => (load_trials(&a)?, load_trials(&b)?),
        (Some(policy), Some(store), Some(instructions)) => {
            let policy = report::review::coordinator::read_policy(&policy)?;
            (
                report::review::coordinator::load_trials(&a, &policy, &store, &instructions)?,
                report::review::coordinator::load_trials(&b, &policy, &store, &instructions)?,
            )
        }
        _ => bail!(
            "review reporting requires --review-policy, --review-store and --instructions-root together"
        ),
    };
    let report = match &release {
        Some(release) => report::release::compare_treatment(
            &left,
            &right,
            release,
            allow_agent_difference,
            information,
        )?,
        None => report::release::compare_unfrozen_treatment(
            &left,
            &right,
            allow_agent_difference,
            information,
        )?,
    };
    if json_output {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        print!("{}", report.markdown());
    }
    Ok(())
}
