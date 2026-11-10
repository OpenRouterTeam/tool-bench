use std::{error::Error, fs, path::PathBuf};

use mock_core::{dataset::Dataset, world::WorldState};
use serde_json::{Value, json};

fn run() -> Result<(), Box<dyn Error>> {
    let arguments: Vec<_> = std::env::args().skip(1).collect();
    if arguments.len() != 2 {
        return Err("usage: bench-answer-forms SCENARIO_DIRECTORY CANDIDATE_ANSWERS_JSON".into());
    }
    let directory = PathBuf::from(&arguments[0]);
    let answers: Vec<String> = serde_json::from_slice(&fs::read(&arguments[1])?)?;
    let dataset = Dataset::load(&directory)?;
    let facts = dataset
        .contract
        .answer
        .as_ref()
        .and_then(|answer| answer.record_facts.as_ref())
        .ok_or("scenario has no record-fact answer contract")?
        .clone();
    let source = dataset.source.clone();
    let initial = WorldState::from_dataset(dataset)?.initial_snapshot();
    let proposals = answers
        .into_iter()
        .map(|answer| match facts.suggest_assertion(&answer, &initial) {
            Ok(template) => {
                json!({"answer":answer,"template":template,"semantic_review":"required"})
            }
            Err(reason) => json!({"answer":answer,"error":reason,"semantic_review":"required"}),
        })
        .collect::<Vec<Value>>();
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "format":"source_bound_clause_proposals_v1",
            "source":source,
            "claim":"Value binding only. These proposals are not semantic approval or validation evidence.",
            "proposals":proposals,
        }))?
    );
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("bench-answer-forms: {error}");
        std::process::exit(1);
    }
}
