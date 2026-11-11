use std::{collections::BTreeMap, path::PathBuf};

use anyhow::{Context, bail};
use mock_core::review::{IssueDecision, ReviewCase};
use report::review::{coordinator, prepare};

fn main() -> anyhow::Result<()> {
    let mut arguments = std::env::args().skip(1);
    let command = arguments
        .next()
        .context("usage: bench-review <keygen|export|sign|verify> --flag value ...")?;
    let allowed: &[&str] = match command.as_str() {
        "keygen" => &["--private-key", "--public-key"],
        "export" => &["--trial", "--instruction", "--policy", "--out"],
        "sign" => &[
            "--case",
            "--policy",
            "--reviewer",
            "--private-key",
            "--decisions",
            "--out",
        ],
        "verify" => &[
            "--trial",
            "--instruction",
            "--policy",
            "--review-store",
            "--out",
        ],
        _ => bail!("unsupported content-review command"),
    };
    let mut options = BTreeMap::new();
    while let Some(flag) = arguments.next() {
        if !allowed.contains(&flag.as_str()) || options.contains_key(&flag) {
            bail!("unsupported or repeated review option {flag}");
        }
        let value = arguments
            .next()
            .with_context(|| format!("{flag} requires a value"))?;
        options.insert(flag, value);
    }
    for flag in allowed {
        if !options.contains_key(*flag) {
            bail!("missing review option {flag}");
        }
    }
    let path = |flag: &str| -> PathBuf { PathBuf::from(&options[flag]) };
    match command.as_str() {
        "keygen" => {
            let public = coordinator::generate_key(&path("--private-key"), &path("--public-key"))?;
            println!("{}", serde_json::to_string(&public)?);
        }
        "export" => {
            let trial = path("--trial");
            let output = path("--out");
            coordinator::require_external(&output, &trial)?;
            let policy = coordinator::read_policy(&path("--policy"))?;
            let instruction =
                String::from_utf8(coordinator::read_bytes(&path("--instruction"), 64 * 1024)?)?;
            let prepared = prepare(&trial, &instruction, &policy)?;
            coordinator::write_new(&output, prepared.case())?;
            println!(
                "case_sha256: {}",
                prepared
                    .case()
                    .sha256(&policy)
                    .map_err(anyhow::Error::msg)?
            );
        }
        "sign" => {
            let policy = coordinator::read_policy(&path("--policy"))?;
            let case: ReviewCase = coordinator::read_json(&path("--case"))?;
            let decisions: BTreeMap<String, IssueDecision> =
                coordinator::read_json(&path("--decisions"))?;
            let signed = coordinator::sign(
                &case,
                &policy,
                &options["--reviewer"],
                &path("--private-key"),
                decisions,
            )?;
            coordinator::write_new(&path("--out"), &signed)?;
            println!("signed case: {}", signed.statement.case_sha256);
        }
        _ => {
            let trial = path("--trial");
            let output = path("--out");
            coordinator::require_external(&output, &trial)?;
            let policy = coordinator::read_policy(&path("--policy"))?;
            let instruction =
                String::from_utf8(coordinator::read_bytes(&path("--instruction"), 64 * 1024)?)?;
            let reviewed =
                coordinator::verify(&trial, &instruction, &policy, &path("--review-store"))?;
            coordinator::write_new(&output, &reviewed)?;
            println!(
                "reviewed case: {}; effective outcome: {:?}; content pending: {}",
                reviewed.reviews.case_sha256,
                reviewed.effective.outcome,
                reviewed.effective.adjudication_needed
            );
        }
    }
    Ok(())
}
