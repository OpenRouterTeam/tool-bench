//! Lookup contracts of the collab group, graded through the complete contract: the reads the
//! contract requires are made against the mocks and the written reply is assessed.

mod support;

use mock_core::grading::{CheckKind, Grade, Outcome};
use serde_json::Value;
use support::ContractTrial;

const IMPACT: &str = "collab/multi/jira-sentry-crash-impact";

fn pass(grade: &Grade) {
    assert_eq!(grade.outcome, Outcome::Pass, "{grade:?}");
    assert_eq!(grade.reward, 1.0, "{grade:?}");
}

/// No reward, and only the written reply is at fault.
fn rejected(grade: &Grade) {
    assert!(
        matches!(grade.outcome, Outcome::Fail | Outcome::Unresolved),
        "{grade:?}"
    );
    assert_eq!(grade.reward, 0.0, "{grade:?}");
    assert!(
        grade
            .failures
            .iter()
            .all(|failure| failure.check == CheckKind::Answer),
        "{grade:?}"
    );
}

async fn impact_trial() -> ContractTrial {
    let trial = ContractTrial::load_many_with_dataset(
        IMPACT,
        &[
            (
                &mock_jira::Jira,
                &[("authorization", "Bearer synthetic-jira-contract")],
            ),
            (
                &mock_sentry::Sentry,
                &[("authorization", "Bearer sntrys_toolbench")],
            ),
        ],
        |_| {},
    );
    // The contract requires that the linked issue was looked up in Jira.
    trial
        .call("GET", "/rest/api/2/issue/WEB-56", Value::Null)
        .await;
    trial
}

#[tokio::test]
async fn sentry_impact_counts_are_read_under_their_ordinary_labels() {
    let trial = impact_trial().await;
    for answer in [
        // Sentry's own column names, as the reference answer writes them.
        "Sentry issue: STOREFRONT-WEB-8D. Events: 2,172. Affected users: 1,318. First release: storefront@5.3.0.",
        "Short ID: STOREFRONT-WEB-8D\nEvent count: 2,172\nUser count: 1,318\nFirst release: storefront@5.3.0",
        "Sentry issue STOREFRONT-WEB-8D: 2172 total events, 1318 unique users impacted, first seen in storefront@5.3.0.",
        "STOREFRONT-WEB-8D has 2,172 error events and 1,318 impacted users. It first appeared in release 5.3.0.",
        "STOREFRONT-WEB-8D: 2,172 occurrences hitting 1,318 people; it first showed up in storefront@5.3.0.",
        "STOREFRONT-WEB-8D \u{2014} 2,172 events across 1,318 distinct users, first release storefront@5.3.0.",
    ] {
        pass(&trial.assess_with_answer(Some(answer)));
    }
}

#[tokio::test]
async fn sentry_impact_counts_cannot_stand_in_for_each_other_under_any_label() {
    let trial = impact_trial().await;
    for answer in [
        // The two counts under each other's labels.
        "Short ID: STOREFRONT-WEB-8D\nEvent count: 1,318\nUser count: 2,172\nFirst release: storefront@5.3.0",
        "STOREFRONT-WEB-8D: 1,318 total events and 2,172 unique users; first seen in storefront@5.3.0.",
        // Numbers that say nothing about which count they are.
        "STOREFRONT-WEB-8D, 2,172 / 1,318, storefront@5.3.0.",
        // The look-alike issue, in the newly accepted labels.
        "STOREFRONT-WEB-7Q: 2,600 total events, 1,904 unique users, first release storefront@5.2.0.",
    ] {
        rejected(&trial.assess_with_answer(Some(answer)));
    }
}
