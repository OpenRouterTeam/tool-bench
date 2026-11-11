//! bench-report <run-dir>... [--json] [--explain <task>]
//! bench-report --require-attested <run-a-dir> <run-b-dir> [--json] [--allow-agent-difference]
//!              [--information-treatment framing|workspace|exposure]
//!              [--release manifest.json]
//! bench-report --trials <run-dir> --json
//!
//! Each dir holds Harbor trial results (local `harbor run -o` output, or a run's artifacts
//! downloaded from the run's artifact bucket) and is one arm.
//!
//! The default report reads stock-Harbor trials, which carry no runner receipt: it
//! re-runs each saved assessment from the collected evidence, says which assurance tier
//! each result has, and never counts a missing assessment as a model failure.
//! `--require-attested` is the strict paired comparison: every trial must carry a
//! verified runner receipt and realized manifest, or the comparison is refused.

use std::path::PathBuf;

use anyhow::{Context, bail};
use report::load_trials;

const USAGE: &str = "usage: bench-report <run-dir>... [--json] [--explain <task>]\n       bench-report --require-attested <run-a> <run-b> [--json] [--allow-agent-difference] [--information-treatment AXIS] [--release MANIFEST]\n       bench-report --trials <run-dir> --json";

/// Options that only mean something for the attested comparison.
const ATTESTED_ONLY: [&str; 6] = [
    "--allow-agent-difference",
    "--release",
    "--information-treatment",
    "--review-policy",
    "--review-store",
    "--instructions-root",
];

fn main() -> anyhow::Result<()> {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
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
    let strict = args
        .iter()
        .filter(|argument| *argument == "--require-attested")
        .count();
    if strict > 1 {
        bail!("duplicate comparison option --require-attested");
    }
    if strict == 1 {
        args.retain(|argument| argument != "--require-attested");
        attested(&args)
    } else {
        arms(&args)
    }
}

/// Per-arm report and paired comparisons over stock-Harbor trials.
fn arms(args: &[String]) -> anyhow::Result<()> {
    let mut directories = Vec::new();
    let mut json_output = false;
    let mut explain = None;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--json" if !json_output => json_output = true,
            "--explain" if explain.is_none() => {
                index += 1;
                explain = Some(
                    args.get(index)
                        .context("--explain needs a task or trial name")?,
                );
            }
            option if ATTESTED_ONLY.contains(&option) => bail!(
                "{option} applies only to the attested comparison; add --require-attested\n{USAGE}"
            ),
            option if option.starts_with("--") => {
                bail!("unsupported or repeated option {option}\n{USAGE}")
            }
            directory => directories.push(PathBuf::from(directory)),
        }
        index += 1;
    }
    if directories.is_empty() {
        bail!("{USAGE}");
    }
    let arms = report::arms::load_arms(&directories)?;
    if let Some(task) = explain {
        let explained = report::arms::explain(&arms, task)?;
        if json_output {
            println!("{}", serde_json::to_string_pretty(&explained)?);
        } else {
            print!("{}", report::arms::explanation_markdown(&explained));
        }
        return Ok(());
    }
    let report = report::arms::report(&arms);
    if json_output {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        print!("{}", report.markdown());
    }
    Ok(())
}

/// The strict paired comparison: admits only trials with verified source and execution
/// provenance, and refuses rather than reporting a partial result.
fn attested(args: &[String]) -> anyhow::Result<()> {
    let mut seen = std::collections::BTreeSet::new();
    let a = PathBuf::from(args.first().context(
        "usage: bench-report --require-attested <run-a> <run-b> [--json] [--allow-agent-difference]",
    )?);
    let b = PathBuf::from(args.get(1).context(
        "usage: bench-report --require-attested <run-a> <run-b> [--json] [--allow-agent-difference]",
    )?);
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
