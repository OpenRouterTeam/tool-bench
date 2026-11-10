//! Grades candidate replies against a scenario's answer contract on its authored initial
//! state, without running a world. Use it to check how the grader reads realistic prose.
//!
//!   bench-answer-probe SCENARIOS_ROOT CORPUS_JSON
//!
//! `CORPUS_JSON` maps a scenario id to `[{"answer": "...", "expect": "pass" | "reject"}]`,
//! where `reject` means any verdict that earns no reward (fail or unresolved). Prints one
//! JSON line per reply and exits non-zero when a verdict disagrees with `expect`.
use std::{collections::BTreeMap, error::Error, fs, path::PathBuf};

use mock_core::{
    dataset::Dataset,
    grading::{Expect, GradeInput, Outcome, grade},
    world::WorldState,
};
use serde::Deserialize;
use serde_json::json;

#[derive(Deserialize)]
struct Reply {
    answer: String,
    expect: String,
    #[serde(default)]
    style: String,
}

fn label(outcome: Outcome) -> &'static str {
    match outcome {
        Outcome::Pass => "pass",
        Outcome::Fail => "fail",
        Outcome::Unresolved => "unresolved",
        Outcome::ContextOverflow => "context_overflow",
        Outcome::IntegrityViolation => "integrity_violation",
        Outcome::InvalidContract => "invalid_contract",
        Outcome::EnvironmentError => "environment_error",
    }
}

fn run() -> Result<bool, Box<dyn Error>> {
    let arguments: Vec<_> = std::env::args().skip(1).collect();
    if arguments.len() != 2 {
        return Err("usage: bench-answer-probe SCENARIOS_ROOT CORPUS_JSON".into());
    }
    let root = PathBuf::from(&arguments[0]);
    let corpus: BTreeMap<String, Vec<Reply>> = serde_json::from_slice(&fs::read(&arguments[1])?)?;
    let mut agreed = true;
    for (id, replies) in corpus {
        let dataset = Dataset::load(&root.join(&id))?;
        let answer = dataset
            .contract
            .answer
            .clone()
            .ok_or_else(|| format!("{id} has no answer contract"))?;
        let expect = Expect {
            version: 2,
            answer: Some(answer),
            ..Expect::default()
        };
        let initial = WorldState::from_dataset(dataset)?.initial_snapshot();
        for reply in replies {
            let graded = grade(&GradeInput {
                expect: &expect,
                state: initial.clone(),
                initial_state: Some(&initial),
                journal: &[],
                events: Some(&[]),
                answer: Some(&reply.answer),
                tools_ips: vec![],
                context_overflow: false,
                startup: &[],
            });
            let verdict = label(graded.outcome);
            let matches = match reply.expect.as_str() {
                "pass" => graded.outcome == Outcome::Pass,
                "reject" => matches!(graded.outcome, Outcome::Fail | Outcome::Unresolved),
                other => return Err(format!("{id}: unknown expectation {other:?}").into()),
            };
            agreed &= matches;
            println!(
                "{}",
                json!({
                    "scenario": id,
                    "style": reply.style,
                    "expect": reply.expect,
                    "verdict": verdict,
                    "agrees": matches,
                    "answer": reply.answer,
                    "reasons": graded
                        .failures
                        .iter()
                        .map(|failure| format!("{}: {}", failure.location, failure.reason))
                        .collect::<Vec<_>>(),
                })
            );
        }
    }
    Ok(agreed)
}

fn main() {
    match run() {
        Ok(true) => {}
        Ok(false) => std::process::exit(1),
        Err(error) => {
            eprintln!("bench-answer-probe: {error}");
            std::process::exit(2);
        }
    }
}
