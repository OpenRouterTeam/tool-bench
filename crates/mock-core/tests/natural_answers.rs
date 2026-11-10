//! Replies the way agents write them, graded against each lookup scenario's real answer
//! contract on its authored initial state, as `bench-answer-probe` does.
//!
//! Each corpus file maps a scenario id to replies. A `pass` reply states the answer in a
//! natural phrasing and must pass. A `reject` reply is wrong, denied, misattributed or
//! evasive and must earn no reward: fail or unresolved.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

use mock_core::{
    dataset::Dataset,
    grading::{Expect, GradeInput, Outcome, grade},
    world::WorldState,
};
use serde::Deserialize;

const CORPUS_FILES: [&str; 4] = [
    "business_cloud.json",
    "code_collab.json",
    "ops_elasticsearch.json",
    "search_web.json",
];

/// Corpus scenarios whose answer contract is not `record_facts`; other tests grade them.
const OTHER_MODES: [&str; 5] = [
    "ops/grafana/ack-checkout-page-oncall",
    "ops/sentry/create-notification-project",
    "search/exa/enrich-lead-list",
    "search/exa/fleet-charger-research",
    "search/tavily/cold-chain-market-research",
];

/// `record_facts` scenarios the corpus has no replies for yet.
const WITHOUT_REPLIES: [&str; 1] = ["diagnostic/stripe/no-action-already-refunded"];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Expectation {
    Pass,
    /// Any verdict that earns no reward: fail or unresolved.
    Reject,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Reply {
    style: String,
    expect: Expectation,
    answer: String,
}

fn scenarios() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios")
}

fn corpus() -> BTreeMap<String, Vec<Reply>> {
    let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/natural_answers");
    let mut corpus = BTreeMap::new();
    for file in CORPUS_FILES {
        let text = fs::read_to_string(directory.join(file))
            .unwrap_or_else(|error| panic!("{file}: {error}"));
        let replies: BTreeMap<String, Vec<Reply>> =
            serde_json::from_str(&text).unwrap_or_else(|error| panic!("{file}: {error}"));
        for (id, replies) in replies {
            assert!(
                corpus.insert(id.clone(), replies).is_none(),
                "{id} appears in more than one corpus file"
            );
        }
    }
    corpus
}

fn subdirectories(directory: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(directory)
        .unwrap_or_else(|error| panic!("{}: {error}", directory.display()))
        .map(|entry| entry.unwrap())
        .filter(|entry| entry.path().is_dir())
        .map(|entry| entry.file_name().into_string().unwrap())
        .collect();
    names.sort();
    names
}

/// Ids of the authored scenarios whose answer contract is `record_facts`.
fn record_fact_scenarios() -> BTreeSet<String> {
    let root = scenarios();
    let mut found = BTreeSet::new();
    for category in subdirectories(&root) {
        for vendor in subdirectories(&root.join(&category)) {
            for name in subdirectories(&root.join(&category).join(&vendor)) {
                let id = format!("{category}/{vendor}/{name}");
                let Ok(text) = fs::read_to_string(root.join(&id).join("scenario.toml")) else {
                    continue;
                };
                let document: toml::Value =
                    toml::from_str(&text).unwrap_or_else(|error| panic!("{id}: {error}"));
                if document
                    .get("expect")
                    .and_then(|expect| expect.get("answer"))
                    .and_then(|answer| answer.get("record_facts"))
                    .is_some()
                {
                    found.insert(id);
                }
            }
        }
    }
    found
}

#[test]
fn natural_replies_earn_the_expected_verdict() {
    let root = scenarios();
    let mut graded = 0;
    let mut disagreements = Vec::new();
    for (id, replies) in corpus() {
        if OTHER_MODES.contains(&id.as_str()) {
            continue;
        }
        let dataset =
            Dataset::load(&root.join(&id)).unwrap_or_else(|error| panic!("{id}: {error}"));
        let answer = dataset
            .contract
            .answer
            .clone()
            .unwrap_or_else(|| panic!("{id} has no answer contract"));
        assert!(
            answer.record_facts.is_some(),
            "{id} is not a record_facts contract; list it in OTHER_MODES"
        );
        let expect = Expect {
            version: 2,
            answer: Some(answer),
            ..Expect::default()
        };
        let initial = WorldState::from_dataset(dataset)
            .unwrap_or_else(|error| panic!("{id}: {error}"))
            .initial_snapshot();
        for reply in replies {
            let grade = grade(&GradeInput {
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
            graded += 1;
            let agrees = match reply.expect {
                Expectation::Pass => grade.outcome == Outcome::Pass,
                Expectation::Reject => {
                    matches!(grade.outcome, Outcome::Fail | Outcome::Unresolved)
                }
            };
            if !agrees {
                let reasons: Vec<&str> = grade
                    .failures
                    .iter()
                    .map(|failure| failure.reason.as_str())
                    .collect();
                disagreements.push(format!(
                    "{id} [{}]: expected {:?}, graded {:?}\n  reply: {}\n  reasons: {reasons:?}",
                    reply.style, reply.expect, grade.outcome, reply.answer
                ));
            }
        }
    }
    assert!(graded > 0, "the corpus holds no record_facts replies");
    assert!(
        disagreements.is_empty(),
        "{} of {graded} natural replies disagree with the corpus:\n\n{}",
        disagreements.len(),
        disagreements.join("\n\n")
    );
}

#[test]
fn every_record_fact_scenario_has_natural_replies() {
    let corpus = corpus();
    let contracts = record_fact_scenarios();
    let mut problems = Vec::new();
    for id in &contracts {
        let exempt = WITHOUT_REPLIES.contains(&id.as_str());
        match corpus.get(id) {
            None if !exempt => problems.push(format!("{id}: no replies in the corpus")),
            Some(_) if exempt => {
                problems.push(format!("{id}: has replies; remove it from WITHOUT_REPLIES"));
            }
            Some(replies) => {
                for expectation in [Expectation::Pass, Expectation::Reject] {
                    if !replies.iter().any(|reply| reply.expect == expectation) {
                        problems.push(format!("{id}: no {expectation:?} reply"));
                    }
                }
            }
            None => {}
        }
    }
    for id in WITHOUT_REPLIES {
        if !contracts.contains(id) {
            problems.push(format!(
                "{id}: WITHOUT_REPLIES names no record_facts scenario"
            ));
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
    let other: BTreeSet<&str> = corpus
        .keys()
        .map(String::as_str)
        .filter(|id| !contracts.contains(*id))
        .collect();
    assert_eq!(
        other,
        BTreeSet::from(OTHER_MODES),
        "OTHER_MODES must list exactly the corpus scenarios without a record_facts contract"
    );
}
