//! Grades candidate replies against a scenario's answer contract on its authored initial
//! state, without running a world. Use it to check how the grader reads realistic prose.
//!
//!   bench-answer-probe SCENARIOS_ROOT CORPUS_JSON
//!
//! `CORPUS_JSON` maps a scenario id to `[{"answer": "...", "expect": "pass" | "reject"}]`,
//! where `reject` means any verdict that earns no reward (fail or unresolved). An entry may
//! also carry:
//!
//! - `"known": "<verdict>"`: the verdict the reader returns today where it disagrees with
//!   `expect`, recorded as a known limit (`"expect": "reject", "known": "pass"`);
//! - `"edit": {"<json pointer>": <value>}`: authored source values replaced in the initial
//!   state before the reply is graded.
//!
//! Prints one JSON line per reply: `agrees` says whether the verdict agrees with `expect`,
//! `recorded` whether it is what the corpus records (it agrees, or it is exactly the
//! `known` verdict). A summary goes to stderr. Exits non-zero when a verdict is not as
//! recorded.
use std::{collections::BTreeMap, error::Error, fs, path::PathBuf};

use mock_core::{
    dataset::Dataset,
    grading::{Expect, GradeInput, Outcome, grade},
    world::WorldState,
};
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Deserialize)]
struct Reply {
    answer: String,
    expect: String,
    #[serde(default)]
    style: String,
    #[serde(default)]
    known: Option<String>,
    #[serde(default)]
    edit: BTreeMap<String, Value>,
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

#[derive(Default)]
struct Summary {
    agree: usize,
    known_misses: usize,
    known_false_passes: usize,
    unrecorded: usize,
}

fn run() -> Result<bool, Box<dyn Error>> {
    let arguments: Vec<_> = std::env::args().skip(1).collect();
    if arguments.len() != 2 {
        return Err("usage: bench-answer-probe SCENARIOS_ROOT CORPUS_JSON".into());
    }
    let root = PathBuf::from(&arguments[0]);
    let corpus: BTreeMap<String, Vec<Reply>> = serde_json::from_slice(&fs::read(&arguments[1])?)?;
    let mut summary = Summary::default();
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
        let authored = WorldState::from_dataset(dataset)?.initial_snapshot();
        for reply in replies {
            let mut initial = authored.clone();
            for (pointer, value) in &reply.edit {
                *initial
                    .pointer_mut(pointer)
                    .ok_or_else(|| format!("{id}: no authored value at {pointer}"))? =
                    value.clone();
            }
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
            let agrees = match reply.expect.as_str() {
                "pass" => graded.outcome == Outcome::Pass,
                "reject" => matches!(graded.outcome, Outcome::Fail | Outcome::Unresolved),
                other => return Err(format!("{id}: unknown expectation {other:?}").into()),
            };
            let recorded = match reply.known.as_deref() {
                None => agrees,
                Some(known) => known == verdict && !agrees,
            };
            match (recorded, agrees, reply.expect.as_str()) {
                (false, _, _) => summary.unrecorded += 1,
                (true, true, _) => summary.agree += 1,
                (true, false, "pass") => summary.known_misses += 1,
                (true, false, _) => summary.known_false_passes += 1,
            }
            println!(
                "{}",
                json!({
                    "scenario": id,
                    "style": reply.style,
                    "expect": reply.expect,
                    "known": reply.known,
                    "verdict": verdict,
                    "agrees": agrees,
                    "recorded": recorded,
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
    eprintln!(
        "{}: {} agree, {} known misses, {} known false passes, {} not as recorded",
        arguments[1],
        summary.agree,
        summary.known_misses,
        summary.known_false_passes,
        summary.unrecorded
    );
    Ok(summary.unrecorded == 0)
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
