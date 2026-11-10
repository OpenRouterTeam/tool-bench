//! Grade regressions for assertion contracts on code and collab scenarios.
//!
//! Each test verifies that the reviewed affirmative clauses correctly bind
//! source facts, catch unsupported novel predicates, detect role swaps
//! (including mutations that disambiguate equal values), and catch
//! cross-role edge failures.

mod statement_support;

use mock_core::grading::Outcome;
use serde_json::json;
use statement_support::Case;

// ---- code/argocd/find-degraded-app ----

#[test]
fn argocd_assertions_reject_unsupported_novel_predicates() {
    let case = Case::load("code/argocd/find-degraded-app");
    // A predicate about the right values that no assertion covers.
    case.unresolved(&[
        "checkout-web is Degraded on production because the image tag 2.14.0-rc3 does not exist in the registry.",
        "checkout-web is Degraded on production and runs image tag 2.14.0-rc3.",
    ]);
}

#[test]
fn argocd_assertions_reject_staging_twin() {
    let case = Case::load("code/argocd/find-degraded-app");
    case.fails(&[
        "checkout-web-staging is the Degraded production app; its pod cannot pull image tag 2.14.0-rc3 (ImagePullBackOff).",
    ]);
}

// ---- code/github/ci-failure-root-cause ----

#[test]
fn github_ci_assertions_reject_unsupported_novel_predicates() {
    let case = Case::load("code/github/ci-failure-root-cause");
    case.unresolved(&[
        "unit-tests failed (lint was green): TestRetryBackoffCapsAtMaxDelay - expected delay 30s, got 32s",
    ]);
}

#[test]
fn github_ci_assertions_reject_wrong_job() {
    let case = Case::load("code/github/ci-failure-root-cause");
    case.fails(&[
        "The lint job failed: TestRetryBackoffCapsAtMaxDelay, \"expected delay 30s, got 32s\".",
        "The integration-tests job failed: TestRetryBackoffCapsAtMaxDelay, \"expected delay 30s, got 32s\".",
    ]);
}

// ---- code/gitlab/diagnose-failed-pipeline ----

#[test]
fn gitlab_assertions_reject_older_pipeline() {
    let case = Case::load("code/gitlab/diagnose-failed-pipeline");
    case.fails(&["Pipeline 5511 failed in integration-tests on column refund_reason."]);
}

#[test]
fn gitlab_assertions_reject_novel_predicates() {
    let case = Case::load("code/gitlab/diagnose-failed-pipeline");
    case.unresolved(&[
        "Pipeline 5520 failed in the integration tests job on column refund_reason.",
    ]);
}

// ---- code/multi/argocd-degraded-gitlab-commit ----

#[test]
fn argocd_gitlab_assertions_reject_other_pipeline() {
    let case = Case::load("code/multi/argocd-degraded-gitlab-commit");
    case.fails(&["checkout-web (2.14.0-rc3). Pipeline 7716: lint failed, permission_denied."]);
}

#[test]
fn argocd_gitlab_assertions_reject_unsupported_predicates() {
    let case = Case::load("code/multi/argocd-degraded-gitlab-commit");
    case.unresolved(&[
        "checkout-web (2.14.0-rc3). Pipeline 7712: the image publish job failed with permission_denied.",
    ]);
}

// ---- code/multi/vercel-deploy-github-pr-lookup ----

#[test]
fn vercel_github_assertions_reject_role_swaps() {
    let case = Case::load("code/multi/vercel-deploy-github-pr-lookup");
    case.fails(&[
        // Author in approver role.
        "Commit c3a9f12, PR #482. Author: mateoalv. Approved by: riley-p. Merged by: ines-d.",
        // Approver in author role.
        "Commit c3a9f12, PR #482. ines-d wrote it, mateoalv approved it, riley-p merged it.",
    ]);
}

#[test]
fn vercel_github_assertions_reject_junpark_as_approver() {
    let case = Case::load("code/multi/vercel-deploy-github-pr-lookup");
    // junpark only commented; she did not approve.
    case.fails(&[
        "Commit c3a9f12, PR #482. Author: mateoalv. Approved by: junpark. Merged by: riley-p.",
    ]);
}

#[test]
fn vercel_github_mutation_disambiguates_merger() {
    let case = Case::load("code/multi/vercel-deploy-github-pr-lookup");
    let merged_by = "/services/github/pulls/acme-shop~1storefront#482/merged_by";
    // Changing the merger makes the original answer fail.
    assert_eq!(
        case.outcome_after(merged_by, json!("kofi-m"), &case.reference),
        Outcome::Fail
    );
    // A new answer stating the new merger passes.
    assert_eq!(
        case.outcome_after(
            merged_by,
            json!("kofi-m"),
            "Commit c3a9f12, PR #482, author mateoalv, approver ines-d, merger Kofi Mensah."
        ),
        Outcome::Pass
    );
}

// ---- code/terraform/payments-db-upgrade-failure ----

#[test]
fn terraform_assertions_disambiguate_equal_plan_counts() {
    let case = Case::load("code/terraform/payments-db-upgrade-failure");
    // to_add=1 and to_change=1 are equal; swapping them must fail.
    case.fails(&[
        "run-Gy4tBHGggKx49nbH failed applying aws_db_instance.payments with InvalidParameterCombination. Plan: 0 to add, 1 to change, 1 to destroy",
        "run-Gy4tBHGggKx49nbH failed applying aws_db_instance.payments with InvalidParameterCombination. Plan: 1 to add, 0 to change, 1 to destroy",
    ]);
    // Correct counts pass.
    case.passes(&[
        "run-Gy4tBHGggKx49nbH failed applying aws_db_instance.payments with InvalidParameterCombination. Plan: 1 to add, 1 to change, 0 to destroy",
    ]);
}

#[test]
fn terraform_assertions_reject_unsupported_count_forms() {
    let case = Case::load("code/terraform/payments-db-upgrade-failure");
    case.unresolved(&[
        "run-Gy4tBHGggKx49nbH failed applying aws_db_instance.payments with InvalidParameterCombination. The plan would add 1, change 1 and destroy 0 resources.",
    ]);
}

// ---- code/vercel/storefront-production-status ----

#[test]
fn vercel_status_assertions_reject_wrong_commit() {
    let case = Case::load("code/vercel/storefront-production-status");
    case.fails(&[
        "Production serves 7d41c9e (\"Upgrade to Next.js 15.4 (#487)\"); the latest preview exceeded the function size limit.",
    ]);
}

// ---- collab/google/northwind-invoice-lookup ----

#[test]
fn google_invoice_assertions_reject_draft_figure() {
    let case = Case::load("collab/google/northwind-invoice-lookup");
    case.fails(&["PO-48213, $11,900.00, due November 20, 2026."]);
}

// ---- collab/jira/blocked-sprint-item ----

#[test]
fn jira_blocked_assertions_reject_role_swap() {
    let case = Case::load("collab/jira/blocked-sprint-item");
    // Blocker and blocked swapped.
    case.fails(&["PAY-117 is blocked by PAY-119; Sam Ortiz is assigned."]);
}

#[test]
fn jira_blocked_assertions_reject_future_sprint_pair() {
    let case = Case::load("collab/jira/blocked-sprint-item");
    case.fails(&["PAY-122 is blocked by PAY-121, assigned to Alex Kim."]);
}

// ---- collab/linear/urgent-bug-owner ----

#[test]
fn linear_assertions_reject_closed_bug() {
    let case = Case::load("collab/linear/urgent-bug-owner");
    case.fails(&["PAY-104 is owned by Priya Natarajan and is In Progress."]);
}

#[test]
fn linear_assertions_reject_other_project_bug() {
    let case = Case::load("collab/linear/urgent-bug-owner");
    case.fails(&["PLAT-233, Sam Ortiz, In Progress."]);
}

// ---- collab/multi/jira-sentry-crash-impact ----

#[test]
fn jira_sentry_assertions_reject_swapped_counts() {
    let case = Case::load("collab/multi/jira-sentry-crash-impact");
    // Events and users swapped.
    case.fails(&["STOREFRONT-WEB-8D: 1,318 events and 2,172 users, first seen in 5.3.0."]);
}

#[test]
fn jira_sentry_assertions_reject_lookalike_issue() {
    let case = Case::load("collab/multi/jira-sentry-crash-impact");
    case.fails(&["STOREFRONT-WEB-7Q: 2,600 events and 1,904 users, first seen in 5.2.0."]);
}

// ---- collab/multi/slack-pagerduty-dbre-oncall ----

#[test]
fn pagerduty_assertions_reject_role_swap() {
    let case = Case::load("collab/multi/slack-pagerduty-dbre-oncall");
    // Level 1 and Level 2 swapped.
    case.fails(&["Level 1: Sam Okafor. Level 2: Dana Nguyen."]);
}

#[test]
fn pagerduty_assertions_accept_correct_roles() {
    let case = Case::load("collab/multi/slack-pagerduty-dbre-oncall");
    case.passes(&[
        "Level 1: Dana Nguyen. Level 2: Sam Okafor.",
        "Dana Nguyen and Sam Okafor.",
    ]);
}

// ---- collab/notion/launch-date-owner ----

#[test]
fn notion_assertions_reject_trashed_row_values() {
    let case = Case::load("collab/notion/launch-date-owner");
    // The trashed duplicate's date and owner.
    case.fails(&["Payments v2 GA launches on 2026-11-05 and is owned by Priya Natarajan."]);
}

// ---- collab/slack/observability-decision ----

#[test]
fn slack_assertions_reject_speculated_vendors() {
    let case = Case::load("collab/slack/observability-decision");
    case.fails(&[
        "We chose Datadog; the contract starts 2026-11-15.",
        "We went with Honeycomb; the contract starts 2026-11-15.",
    ]);
}

#[test]
fn slack_assertions_reject_swapped_dates() {
    let case = Case::load("collab/slack/observability-decision");
    // The migration-complete date offered as the start date.
    case.fails(&["We chose Grafana Cloud; the contract starts 2027-01-31."]);
}
