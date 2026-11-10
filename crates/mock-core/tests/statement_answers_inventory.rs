//! Repository-wide checks on answer contracts.
//!
//! Every statement-mode contract must accept its own reference answer and
//! treat a missing source as an environment error, and every scenario that
//! this migration deliberately left on keyword predicates is listed here
//! with the reason, so the ledger cannot silently go stale.

mod statement_support;

use std::path::{Path, PathBuf};

use statement_support::Case;

/// Scenarios whose answers cannot yet be bound to authored facts honestly.
const LEFT_ON_KEYWORD_PREDICATES: &[(&str, &str)] = &[(
    "example/aws/missing-bucket-policy",
    "the expected API error code is produced by the service and is not in the authored data",
)];

fn scenarios() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios")
}

/// `(id, [expect.answer] table)` of every scenario that grades an answer.
fn answer_contracts() -> Vec<(String, toml::Value)> {
    let mut found = Vec::new();
    let mut pending = vec![scenarios()];
    while let Some(directory) = pending.pop() {
        let manifest = directory.join("scenario.toml");
        if manifest.is_file() {
            let document: toml::Value =
                toml::from_str(&std::fs::read_to_string(&manifest).unwrap()).unwrap();
            if let Some(answer) = document
                .get("expect")
                .and_then(|expect| expect.get("answer"))
            {
                let id = directory
                    .strip_prefix(scenarios())
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/");
                found.push((id, answer.clone()));
            }
            continue;
        }
        for entry in std::fs::read_dir(&directory).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                pending.push(path);
            }
        }
    }
    found.sort_by(|left, right| left.0.cmp(&right.0));
    found
}

#[test]
fn every_statement_contract_accepts_its_reference_and_needs_its_source() {
    let mut checked = 0;
    for (id, answer) in answer_contracts() {
        if answer.get("record_facts").is_some() {
            Case::load(&id).reference_is_source_bound();
            checked += 1;
        }
    }
    assert!(
        checked >= 45,
        "only {checked} statement contracts were found"
    );
}

#[test]
fn scenarios_left_on_keyword_predicates_are_listed_with_their_reason() {
    let contracts = answer_contracts();
    for (id, reason) in LEFT_ON_KEYWORD_PREDICATES {
        assert!(!reason.is_empty());
        let answer = contracts
            .iter()
            .find(|(candidate, _)| candidate == id)
            .map(|(_, answer)| answer)
            .unwrap_or_else(|| panic!("{id} is listed but grades no answer"));
        let bound = ["record_facts", "source_facts", "oncall", "sentry_dsn"]
            .iter()
            .any(|predicate| answer.get(predicate).is_some());
        assert!(
            !bound,
            "{id} is now source-bound; remove it from the keyword-predicate ledger"
        );
    }
}
