//! Runs the natural-answer corpus through the real grade path for the answer
//! modes that read typed source rows (`source_facts`) and the current on-call
//! assignees (`oncall`).  The replies were written without sight of the
//! grader: every `pass` reply must pass and every `reject` reply must earn no
//! reward.  The scenarios whose answer exists only after the work is done are
//! graded through a world in `crates/world/tests/natural_answers_stateful.rs`.

use std::{collections::BTreeMap, fs, path::Path};

use mock_core::{
    dataset::Dataset,
    grading::{Expect, GradeInput, Outcome, grade},
    world::WorldState,
};
use serde::Deserialize;

#[derive(Deserialize)]
struct Reply {
    style: String,
    expect: String,
    answer: String,
}

const SCENARIOS: [(&str, &str); 4] = [
    ("search_web.json", "search/exa/enrich-lead-list"),
    ("search_web.json", "search/exa/fleet-charger-research"),
    (
        "search_web.json",
        "search/tavily/cold-chain-market-research",
    ),
    (
        "ops_elasticsearch.json",
        "ops/grafana/ack-checkout-page-oncall",
    ),
];

#[test]
fn corpus_replies_in_ordinary_prose_are_graded_as_labelled() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut disagreements = Vec::new();
    for (file, id) in SCENARIOS {
        let corpus: BTreeMap<String, Vec<Reply>> = serde_json::from_slice(
            &fs::read(root.join("tests/fixtures/natural_answers").join(file)).unwrap(),
        )
        .unwrap();
        let dataset = Dataset::load(&root.join("../../scenarios").join(id)).unwrap();
        let expect = Expect {
            version: 2,
            answer: dataset.contract.answer.clone(),
            ..Expect::default()
        };
        assert!(expect.answer.is_some(), "{id} has no answer contract");
        let initial = WorldState::from_dataset(dataset)
            .unwrap()
            .initial_snapshot();
        let replies = &corpus[id];
        let labelled = |label: &str| replies.iter().filter(|reply| reply.expect == label).count();
        assert_eq!((labelled("pass"), labelled("reject")), (5, 3), "{id}");
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
            let agrees = match reply.expect.as_str() {
                "pass" => graded.outcome == Outcome::Pass && graded.reward == 1.0,
                "reject" => {
                    matches!(graded.outcome, Outcome::Fail | Outcome::Unresolved)
                        && graded.reward == 0.0
                }
                other => panic!("{id}: unknown expectation {other:?}"),
            };
            if !agrees {
                disagreements.push(format!(
                    "{id} [{}] expected {} but got {:?}: {:?}",
                    reply.style, reply.expect, graded.outcome, graded.failures
                ));
            }
        }
    }
    assert!(disagreements.is_empty(), "{}", disagreements.join("\n"));
}
