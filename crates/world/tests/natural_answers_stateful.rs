//! Grades the natural-answer corpus for the two scenarios whose answer can
//! only be checked after the work is done: the complete contract is graded
//! against a world in which the scenario's work has been carried out, so a
//! `pass` reply earns the reward and a `reject` reply fails on the answer
//! alone.

mod support;

use std::{collections::BTreeMap, fs, path::Path};

use mock_core::grading::{CheckKind, Outcome};
use serde_json::{Value, json};
use support::ContractTrial;

const ONCALL: &str = "ops/grafana/ack-checkout-page-oncall";
const DSN: &str = "ops/sentry/create-notification-project";
/// The DSN the corpus author derived by hand for the project to be created.
const AUTHORED_DSN: &str =
    "https://1a02670783595f20634015aa33b23103@sentry.lumenlabs.dev/4508812006";

struct Reply {
    style: String,
    passes: bool,
    answer: String,
}

fn replies(id: &str) -> Vec<Reply> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../mock-core/tests/fixtures/natural_answers/ops_elasticsearch.json");
    let corpus: BTreeMap<String, Vec<Value>> =
        serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
    let replies: Vec<Reply> = corpus[id]
        .iter()
        .map(|reply| Reply {
            style: reply["style"].as_str().unwrap().to_owned(),
            passes: match reply["expect"].as_str().unwrap() {
                "pass" => true,
                "reject" => false,
                other => panic!("{id}: unknown expectation {other:?}"),
            },
            answer: reply["answer"].as_str().unwrap().to_owned(),
        })
        .collect();
    let passing = replies.iter().filter(|reply| reply.passes).count();
    assert_eq!((passing, replies.len() - passing), (5, 3), "{id}");
    replies
}

fn grade_all(trial: &ContractTrial, id: &str, replies: &[Reply]) {
    let mut disagreements = Vec::new();
    for reply in replies {
        let graded = trial.assess_with_answer(Some(&reply.answer));
        let agrees = if reply.passes {
            graded.outcome == Outcome::Pass && graded.reward == 1.0
        } else {
            // The work is done, so only the answer may be at fault.
            matches!(graded.outcome, Outcome::Fail | Outcome::Unresolved)
                && graded.reward == 0.0
                && !graded.answer_ok
                && graded
                    .failures
                    .iter()
                    .all(|failure| failure.check == CheckKind::Answer)
        };
        if !agrees {
            disagreements.push(format!(
                "{id} [{}] expected {} but got {:?}: {:?}",
                reply.style,
                if reply.passes { "pass" } else { "reject" },
                graded.outcome,
                graded.failures
            ));
        }
    }
    assert!(disagreements.is_empty(), "{}", disagreements.join("\n"));
}

#[tokio::test]
async fn oncall_corpus_replies_are_graded_as_labelled_after_the_page_is_acknowledged() {
    let authorization = format!("Bearer {}", mock_grafana::TOKEN);
    let trial = ContractTrial::load_with_dataset(
        ONCALL,
        &mock_grafana::Grafana,
        &[("authorization", &authorization)],
        |_| {},
    );
    trial
        .call(
            "POST",
            "/oncall/api/v1/alert_groups/IHQ4K2M7/acknowledge",
            Value::Null,
        )
        .await;
    grade_all(&trial, ONCALL, &replies(ONCALL));
}

#[tokio::test]
async fn dsn_corpus_replies_are_graded_as_labelled_against_the_dsn_the_world_mints() {
    let trial = ContractTrial::load_with_dataset(
        DSN,
        &mock_sentry::Sentry,
        &[("authorization", "Bearer sntrys_toolbench")],
        |_| {},
    );
    let project = trial
        .call(
            "POST",
            "/api/0/teams/lumen-labs/payments/projects/",
            json!({"name":"notification-service","platform":"node"}),
        )
        .await;
    let slug = project["slug"].as_str().unwrap();
    let keys = trial
        .call(
            "GET",
            &format!("/api/0/projects/lumen-labs/{slug}/keys/"),
            Value::Null,
        )
        .await;
    let minted = keys
        .as_array()
        .unwrap()
        .iter()
        .find(|key| key["isActive"] == true)
        .unwrap()["dsn"]["public"]
        .as_str()
        .unwrap()
        .to_owned();
    // The corpus was written against a hand-derived DSN; the replies are
    // graded against the one this world actually minted.
    let (authored_id, minted_id) = (
        AUTHORED_DSN.rsplit('/').next().unwrap(),
        minted.rsplit('/').next().unwrap(),
    );
    let mut replies = replies(DSN);
    assert!(
        replies
            .iter()
            .filter(|reply| reply.passes)
            .all(|reply| reply.answer.contains(AUTHORED_DSN))
    );
    for reply in &mut replies {
        reply.answer = reply
            .answer
            .replace(AUTHORED_DSN, &minted)
            .replace(authored_id, minted_id);
    }
    println!("authored {AUTHORED_DSN}\nminted   {minted}");
    grade_all(&trial, DSN, &replies);
}
