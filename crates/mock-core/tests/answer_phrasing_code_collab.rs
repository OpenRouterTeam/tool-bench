//! How the answer reader treats differently phrased replies to the code and collab lookup
//! scenarios.
//!
//! A correct reply passes however it is phrased. A reply that names another record, swaps
//! two roles, states a wrong value or repeats a value the source no longer holds earns no
//! reward: it fails or is left unresolved.

mod statement_support;

use mock_core::grading::Outcome;
use serde_json::{Value, json};
use statement_support::Case;

fn unrewarded(outcome: Outcome) -> bool {
    matches!(outcome, Outcome::Fail | Outcome::Unresolved)
}

/// Every reply must earn no reward.
#[track_caller]
fn rejects(case: &Case, answers: &[&str]) {
    let rewarded: Vec<String> = answers
        .iter()
        .map(|answer| (answer, case.outcome(answer)))
        .filter(|(_, outcome)| !unrewarded(*outcome))
        .map(|(answer, outcome)| format!("{outcome:?}: {answer}"))
        .collect();
    assert!(
        rewarded.is_empty(),
        "replies that must earn no reward:\n{}",
        rewarded.join("\n")
    );
}

/// The reply must earn no reward once the authored source at `pointer` holds `value`.
#[track_caller]
fn rejects_after(case: &Case, pointer: &str, value: Value, answer: &str) {
    let outcome = case.outcome_after(pointer, value, answer);
    assert!(
        unrewarded(outcome),
        "a reply that must earn no reward after the source changed was graded {outcome:?}: {answer}"
    );
}

#[track_caller]
fn passes_after(case: &Case, pointer: &str, value: Value, answer: &str) {
    assert_eq!(
        case.outcome_after(pointer, value, answer),
        Outcome::Pass,
        "answer: {answer}"
    );
}

// ---- code/argocd/find-degraded-app ----

#[test]
fn argocd_staging_twin_earns_no_reward() {
    let case = Case::load("code/argocd/find-degraded-app");
    rejects(
        &case,
        &[
            "checkout-web-staging is the Degraded production app; its pod cannot pull image tag 2.14.0-rc3 (ImagePullBackOff).",
        ],
    );
}

// ---- code/github/ci-failure-root-cause ----

#[test]
fn github_ci_failure_blamed_on_another_job_earns_no_reward() {
    let case = Case::load("code/github/ci-failure-root-cause");
    rejects(
        &case,
        &[
            "The lint job failed: TestRetryBackoffCapsAtMaxDelay, \"expected delay 30s, got 32s\".",
            "The integration-tests job failed: TestRetryBackoffCapsAtMaxDelay, \"expected delay 30s, got 32s\".",
        ],
    );
}

// ---- code/gitlab/diagnose-failed-pipeline ----

#[test]
fn gitlab_older_pipeline_earns_no_reward() {
    let case = Case::load("code/gitlab/diagnose-failed-pipeline");
    rejects(
        &case,
        &["Pipeline 5511 failed in integration-tests on column refund_reason."],
    );
}

// ---- code/multi/argocd-degraded-gitlab-commit ----

#[test]
fn argocd_gitlab_other_pipeline_earns_no_reward() {
    let case = Case::load("code/multi/argocd-degraded-gitlab-commit");
    rejects(
        &case,
        &["checkout-web (2.14.0-rc3). Pipeline 7716: lint failed, permission_denied."],
    );
}

// ---- code/multi/vercel-deploy-github-pr-lookup ----

#[test]
fn vercel_github_swapped_author_and_approver_earn_no_reward() {
    let case = Case::load("code/multi/vercel-deploy-github-pr-lookup");
    rejects(
        &case,
        &[
            // The approver and the merger swapped.
            "Commit c3a9f12, PR #482. Author: mateoalv. Approved by: riley-p. Merged by: ines-d.",
            // The author and the approver swapped.
            "Commit c3a9f12, PR #482. ines-d wrote it, mateoalv approved it, riley-p merged it.",
        ],
    );
}

#[test]
fn vercel_github_commenter_named_as_approver_earns_no_reward() {
    let case = Case::load("code/multi/vercel-deploy-github-pr-lookup");
    // junpark only commented on the pull request.
    rejects(
        &case,
        &["Commit c3a9f12, PR #482. Author: mateoalv. Approved by: junpark. Merged by: riley-p."],
    );
}

#[test]
fn vercel_github_follows_the_merger() {
    let case = Case::load("code/multi/vercel-deploy-github-pr-lookup");
    let merged_by = "/services/github/pulls/acme-shop~1storefront#482/merged_by";
    rejects_after(&case, merged_by, json!("kofi-m"), &case.reference);
    passes_after(
        &case,
        merged_by,
        json!("kofi-m"),
        "Commit c3a9f12, PR #482, author mateoalv, approver ines-d, merger Kofi Mensah.",
    );
}

// ---- code/terraform/payments-db-upgrade-failure ----

#[test]
fn terraform_plan_counts_pass_in_their_roles() {
    let case = Case::load("code/terraform/payments-db-upgrade-failure");
    case.passes(&[
        "run-Gy4tBHGggKx49nbH failed applying aws_db_instance.payments with InvalidParameterCombination. Plan: 1 to add, 1 to change, 0 to destroy",
    ]);
}

#[test]
fn terraform_swapped_plan_counts_earn_no_reward() {
    let case = Case::load("code/terraform/payments-db-upgrade-failure");
    // One resource is added and one changed; moving either count to another role is wrong.
    rejects(
        &case,
        &[
            "run-Gy4tBHGggKx49nbH failed applying aws_db_instance.payments with InvalidParameterCombination. Plan: 0 to add, 1 to change, 1 to destroy",
            "run-Gy4tBHGggKx49nbH failed applying aws_db_instance.payments with InvalidParameterCombination. Plan: 1 to add, 0 to change, 1 to destroy",
        ],
    );
}

// ---- code/vercel/storefront-production-status ----

#[test]
fn vercel_status_wrong_commit_earns_no_reward() {
    let case = Case::load("code/vercel/storefront-production-status");
    rejects(
        &case,
        &[
            "Production serves 7d41c9e (\"Upgrade to Next.js 15.4 (#487)\"); the latest preview exceeded the function size limit.",
        ],
    );
}

// ---- collab/google/northwind-invoice-lookup ----

#[test]
fn google_invoice_draft_figure_earns_no_reward() {
    let case = Case::load("collab/google/northwind-invoice-lookup");
    rejects(&case, &["PO-48213, $11,900.00, due November 20, 2026."]);
}

// ---- collab/jira/blocked-sprint-item ----

#[test]
fn jira_blocked_swapped_or_future_sprint_issues_earn_no_reward() {
    let case = Case::load("collab/jira/blocked-sprint-item");
    rejects(
        &case,
        &[
            // The blocker and the blocked issue swapped.
            "PAY-117 is blocked by PAY-119; Sam Ortiz is assigned.",
            // The pair in the future sprint.
            "PAY-122 is blocked by PAY-121, assigned to Alex Kim.",
        ],
    );
}

// ---- collab/linear/urgent-bug-owner ----

#[test]
fn linear_closed_or_other_project_bug_earns_no_reward() {
    let case = Case::load("collab/linear/urgent-bug-owner");
    rejects(
        &case,
        &[
            "PAY-104 is owned by Priya Natarajan and is In Progress.",
            "PLAT-233, Sam Ortiz, In Progress.",
        ],
    );
}

// ---- collab/multi/jira-sentry-crash-impact ----

#[test]
fn jira_sentry_swapped_counts_or_lookalike_issue_earn_no_reward() {
    let case = Case::load("collab/multi/jira-sentry-crash-impact");
    rejects(
        &case,
        &[
            // The event and user counts swapped.
            "STOREFRONT-WEB-8D: 1,318 events and 2,172 users, first seen in 5.3.0.",
            "STOREFRONT-WEB-7Q: 2,600 events and 1,904 users, first seen in 5.2.0.",
        ],
    );
}

// ---- collab/multi/slack-pagerduty-dbre-oncall ----

#[test]
fn pagerduty_levels_pass_in_their_roles() {
    let case = Case::load("collab/multi/slack-pagerduty-dbre-oncall");
    case.passes(&[
        "Level 1: Dana Nguyen. Level 2: Sam Okafor.",
        "Dana Nguyen and Sam Okafor.",
    ]);
}

#[test]
fn pagerduty_swapped_levels_earn_no_reward() {
    let case = Case::load("collab/multi/slack-pagerduty-dbre-oncall");
    rejects(&case, &["Level 1: Sam Okafor. Level 2: Dana Nguyen."]);
}

// ---- collab/notion/launch-date-owner ----

#[test]
fn notion_trashed_row_values_earn_no_reward() {
    let case = Case::load("collab/notion/launch-date-owner");
    rejects(
        &case,
        &["Payments v2 GA launches on 2026-11-05 and is owned by Priya Natarajan."],
    );
}

// ---- collab/slack/observability-decision ----

#[test]
fn slack_speculated_vendor_or_other_date_earns_no_reward() {
    let case = Case::load("collab/slack/observability-decision");
    rejects(
        &case,
        &[
            "We chose Datadog; the contract starts 2026-11-15.",
            "We went with Honeycomb; the contract starts 2026-11-15.",
            // The migration-complete date offered as the start date.
            "We chose Grafana Cloud; the contract starts 2027-01-31.",
        ],
    );
}
