mod support;

use mock_core::grading::{CheckKind, Outcome};
use serde_json::Value;
use support::ContractTrial;

const CATERER: &str = "search/brave/caterer-saturday-hours";
const ANSWER: &str = "Phone: +1 503-555-0142\nSaturday hours: 9:00 AM - 2:00 PM";

fn prepared() -> ContractTrial {
    ContractTrial::load_with_dataset(
        CATERER,
        &mock_brave::Brave,
        &[("x-subscription-token", "BSA-contract")],
        |_| {},
    )
}

/// `brave_place_search` reads the same listing data as `brave_local_search`, in one request.
#[tokio::test]
async fn place_search_is_local_listing_data_too() {
    let trial = prepared();
    let found = trial
        .call(
            "GET",
            "/res/v1/local/place_search?q=Juniper+%26+Rye+Catering&location=Portland+Oregon",
            Value::Null,
        )
        .await;
    assert_eq!(
        found["results"][0]["contact"]["telephone"],
        "+1 503-555-0142"
    );
    let grade = trial.assess_with_answer(Some(ANSWER));
    assert_eq!(grade.outcome, Outcome::Pass, "{grade:?}");
}

/// The request asks for listing data; the same answer without a local lookup is not it.
#[tokio::test]
async fn the_answer_without_a_local_lookup_fails_the_required_call() {
    let trial = prepared();
    trial
        .call(
            "GET",
            "/res/v1/web/search?q=Juniper+%26+Rye+Catering+Portland+Oregon",
            Value::Null,
        )
        .await;
    let grade = trial.assess_with_answer(Some(ANSWER));
    assert_eq!(grade.outcome, Outcome::Fail, "{grade:?}");
    assert!(
        grade
            .failures
            .iter()
            .all(|failure| failure.check == CheckKind::RequiredCalls),
        "{grade:?}"
    );
}
