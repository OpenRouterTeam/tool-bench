//! Replies the way agents write them, graded against each lookup scenario's real answer
//! contract on its authored initial state, as `bench-answer-probe` does.
//!
//! Three corpora live under `tests/fixtures/`, each a directory of files that map a
//! scenario id to replies:
//!
//! - `natural_answers/` and `natural_answers_heldout/`: correct replies in natural
//!   phrasings (`"expect": "pass"`) and replies that are wrong, denied, misattributed or
//!   evasive (`"expect": "reject"`: fail or unresolved, neither earns reward);
//! - `wrong_answers/`: wrong replies written to defeat the reader, with every right value
//!   present but attached to the wrong thing or dismissed. All are `"expect": "reject"`.
//!
//! `expect` is what a careful human grader would decide. The reader is a deterministic
//! heuristic and does not always agree. Where it does not, the entry records the verdict
//! the reader returns today as `"known"` (`"expect": "pass", "known": "unresolved"` for a
//! correct reply it cannot resolve, `"expect": "reject", "known": "pass"` for a wrong
//! reply it passes). An entry without `known` must agree with `expect`. An entry with
//! `known` must return exactly that verdict: a change to the reader that fixes or breaks
//! one fails here until the fixture is updated, so the known limits stay a visible list.
//!
//! An entry may also carry `"edit": {"<json pointer>": <value>}`: authored source values
//! replaced in the initial state before the reply is graded, for a reply that is wrong
//! only once the source has changed.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::Write,
    path::{Path, PathBuf},
};

use mock_core::{
    dataset::Dataset,
    grading::{Expect, GradeInput, Outcome, grade},
    world::WorldState,
};
use serde::Deserialize;
use serde_json::Value;

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

/// A verdict the reader returns for a reply.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Verdict {
    Pass,
    Fail,
    Unresolved,
}

impl Verdict {
    fn of(outcome: Outcome) -> Option<Self> {
        match outcome {
            Outcome::Pass => Some(Self::Pass),
            Outcome::Fail => Some(Self::Fail),
            Outcome::Unresolved => Some(Self::Unresolved),
            _ => None,
        }
    }

    fn agrees(self, expect: Expectation) -> bool {
        (self == Self::Pass) == (expect == Expectation::Pass)
    }

    fn label(self) -> &'static str {
        match self {
            Self::Pass => "pass",
            Self::Fail => "fail",
            Self::Unresolved => "unresolved",
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Reply {
    style: String,
    expect: Expectation,
    /// The verdict the reader returns today where it disagrees with `expect`.
    #[serde(default)]
    known: Option<Verdict>,
    /// Authored source values replaced before grading, by JSON pointer.
    #[serde(default)]
    edit: BTreeMap<String, Value>,
    answer: String,
}

fn scenarios() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios")
}

/// The replies of every file in `tests/fixtures/<directory>`, by scenario. A scenario in
/// `OTHER_MODES` is listed without its replies: they belong to another answer mode.
fn corpus(directory: &str) -> BTreeMap<String, Vec<Reply>> {
    let directory = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(directory);
    let mut files: Vec<PathBuf> = fs::read_dir(&directory)
        .unwrap_or_else(|error| panic!("{}: {error}", directory.display()))
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "json")
        })
        .collect();
    files.sort();
    assert!(
        !files.is_empty(),
        "{} holds no corpus file",
        directory.display()
    );
    let mut corpus: BTreeMap<String, Vec<Reply>> = BTreeMap::new();
    for file in files {
        let name = file.display();
        let text = fs::read_to_string(&file).unwrap_or_else(|error| panic!("{name}: {error}"));
        let scenarios: BTreeMap<String, Value> =
            serde_json::from_str(&text).unwrap_or_else(|error| panic!("{name}: {error}"));
        for (id, replies) in scenarios {
            let replies: Vec<Reply> = if OTHER_MODES.contains(&id.as_str()) {
                Vec::new()
            } else {
                serde_json::from_value(replies)
                    .unwrap_or_else(|error| panic!("{name}: {id}: {error}"))
            };
            corpus.entry(id).or_default().extend(replies);
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

/// How one corpus directory grades.
#[derive(Default)]
struct Tally {
    /// Replies whose verdict agrees with `expect`.
    agree: usize,
    /// Correct replies recorded as earning no reward.
    known_misses: usize,
    /// Wrong replies recorded as passing.
    known_false_passes: usize,
    /// Replies whose verdict is not what the fixture records.
    problems: Vec<String>,
}

/// Grades every `record_facts` reply of a corpus directory through the real grade path.
fn graded(directory: &str) -> Tally {
    let root = scenarios();
    let mut tally = Tally::default();
    for (id, replies) in corpus(directory) {
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
        let authored = WorldState::from_dataset(dataset)
            .unwrap_or_else(|error| panic!("{id}: {error}"))
            .initial_snapshot();
        let mut seen = BTreeSet::new();
        for reply in replies {
            let entry = format!("{directory}: {id} [{}]", reply.style);
            let edits = serde_json::to_string(&reply.edit).unwrap();
            assert!(
                seen.insert((reply.answer.clone(), edits)),
                "{entry}: the same reply appears twice: {}",
                reply.answer
            );
            let mut initial = authored.clone();
            for (pointer, value) in &reply.edit {
                *initial
                    .pointer_mut(pointer)
                    .unwrap_or_else(|| panic!("{entry}: no authored value at {pointer}")) =
                    value.clone();
            }
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
            let verdict = Verdict::of(grade.outcome);
            let agrees = verdict.is_some_and(|verdict| verdict.agrees(reply.expect));
            let problem = match (reply.known, verdict) {
                (None, _) if agrees => {
                    tally.agree += 1;
                    continue;
                }
                (Some(known), Some(verdict)) if known == verdict && !agrees => {
                    match reply.expect {
                        Expectation::Pass => tally.known_misses += 1,
                        Expectation::Reject => tally.known_false_passes += 1,
                    }
                    continue;
                }
                (None, Some(verdict)) => format!(
                    "expected {:?}, graded {:?}. Either the reader regressed on this reply, \
                     or this is a limit you accept: then record it in the fixture entry as \
                     \"known\": \"{}\"",
                    reply.expect,
                    grade.outcome,
                    verdict.label()
                ),
                (None, None) => format!("expected {:?}, graded {:?}", reply.expect, grade.outcome),
                (Some(known), _) if known.agrees(reply.expect) => format!(
                    "\"known\": \"{}\" does not disagree with \"expect\"; remove it",
                    known.label()
                ),
                (Some(known), _) if agrees => format!(
                    "recorded as a known {:?} but now graded {:?}, as expected. The reader no \
                     longer misreads this reply: remove \"known\" from the fixture entry",
                    known, grade.outcome
                ),
                (Some(known), _) => format!(
                    "recorded as a known {:?} but graded {:?}: update \"known\" in the fixture \
                     entry",
                    known, grade.outcome
                ),
            };
            let reasons: Vec<&str> = grade
                .failures
                .iter()
                .map(|failure| failure.reason.as_str())
                .collect();
            tally.problems.push(format!(
                "{entry}: {problem}\n  reply: {}\n  reasons: {reasons:?}",
                reply.answer
            ));
        }
    }
    tally
}

/// Grades a corpus directory, prints its one-line summary and fails on any reply whose
/// verdict is not what its fixture entry records.
fn check(directory: &str) {
    let tally = graded(directory);
    let total = tally.agree + tally.known_misses + tally.known_false_passes + tally.problems.len();
    assert!(total > 0, "{directory} holds no record_facts replies");
    // Written past the test harness's capture, so the counts show on every run.
    let _ = writeln!(
        std::io::stderr().lock(),
        "{directory}: {total} replies, {} agree, {} known misses, {} known false passes",
        tally.agree,
        tally.known_misses,
        tally.known_false_passes
    );
    assert!(
        tally.problems.is_empty(),
        "{} of {total} replies in {directory} are not graded as the fixtures record:\n\n{}",
        tally.problems.len(),
        tally.problems.join("\n\n")
    );
}

#[test]
fn natural_replies_earn_the_recorded_verdict() {
    check("natural_answers");
}

#[test]
fn held_out_replies_earn_the_recorded_verdict() {
    check("natural_answers_heldout");
}

#[test]
fn adversarial_wrong_replies_earn_the_recorded_verdict() {
    let wrong: Vec<String> = corpus("wrong_answers")
        .into_iter()
        .flat_map(|(id, replies)| {
            replies
                .into_iter()
                .filter(|reply| reply.expect != Expectation::Reject)
                .map(move |reply| format!("{id}: {}", reply.answer))
        })
        .collect();
    assert!(
        wrong.is_empty(),
        "wrong_answers holds only wrong replies; these are not marked \"reject\":\n{}",
        wrong.join("\n")
    );
    check("wrong_answers");
}

#[test]
fn every_record_fact_scenario_has_natural_replies() {
    let corpus = corpus("natural_answers");
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
