//! Lookup contracts of the collab group, graded through the complete contract: the reads the
//! contract requires are made against the mocks and the written reply is assessed.

mod support;

use mock_core::grading::{CheckKind, Grade, Outcome};
use serde_json::Value;
use support::ContractTrial;

const IMPACT: &str = "collab/multi/jira-sentry-crash-impact";
const ONCALL: &str = "collab/multi/slack-pagerduty-dbre-oncall";

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

fn oncall_trial() -> ContractTrial {
    // PagerDuty first: its API is the one the test reads.
    ContractTrial::load_many_with_dataset(
        ONCALL,
        &[
            (
                &mock_pagerduty::Pagerduty,
                &[("authorization", "Token token=local-fixture-credential")],
            ),
            (&mock_slack::Slack, &[]),
        ],
        |_| {},
    )
}

#[tokio::test]
async fn the_inventory_incident_is_routed_to_the_rotation_the_thread_names() {
    let trial = oncall_trial();
    // The Slack thread says PD #4813 goes to the production DBRE rotation. Following the
    // incident through PagerDuty must lead to the same policy and the same people.
    let incident = trial
        .call("GET", "/incidents/Q1INVDBLAG7QWE", Value::Null)
        .await["incident"]
        .clone();
    assert_eq!(incident["incident_number"], 4813);
    let service = trial
        .call(
            "GET",
            &format!("/services/{}", incident["service"]["id"].as_str().unwrap()),
            Value::Null,
        )
        .await["service"]
        .clone();
    assert_eq!(service["name"], "inventory-db");
    for routed in [&incident, &service] {
        assert_eq!(routed["escalation_policy"]["id"], "PEPDBRE", "{routed}");
        assert_eq!(
            routed["escalation_policy"]["summary"], "Database Reliability",
            "{routed}"
        );
    }
    let oncalls = trial
        .call(
            "GET",
            "/oncalls?escalation_policy_ids%5B%5D=PEPDBRE",
            Value::Null,
        )
        .await["oncalls"]
        .clone();
    let at_level = |level: u64| {
        oncalls
            .as_array()
            .unwrap()
            .iter()
            .find(|oncall| oncall["escalation_level"] == level)
            .unwrap_or_else(|| panic!("no level {level} on call: {oncalls}"))["user"]
            .clone()
    };
    assert_eq!(at_level(1)["summary"], "Dana Nguyen");
    assert_eq!(at_level(2)["summary"], "Sam Okafor");
    // The page went to the person who is first on call for that policy.
    assert_eq!(
        incident["assignments"][0]["assignee"]["id"],
        at_level(1)["id"]
    );

    pass(&trial.assess_with_answer(Some("First on call: Dana Nguyen. Backup: Sam Okafor.")));
    // PD #4813 is assigned to Dana through the Database Reliability policy.
    pass(&trial.assess_with_answer(Some(
        "PD #4813 is on the inventory-db service, which uses the Database Reliability escalation policy. Dana Nguyen is first on call (the incident is assigned to her) and Sam Okafor is the backup at level 2.",
    )));
}

#[tokio::test]
async fn another_rotation_or_swapped_levels_are_still_not_the_answer() {
    let trial = oncall_trial();
    for answer in [
        // The Platform rotation, which does not own this alert.
        "First on call: Alex Kim. Backup: Jordan Lee.",
        // The staging rotation the thread rules out.
        "Sam Okafor is on call for Database Reliability - Staging; that policy has no backup.",
        "First on call: Sam Okafor. Backup: Dana Nguyen.",
    ] {
        rejected(&trial.assess_with_answer(Some(answer)));
    }
}
