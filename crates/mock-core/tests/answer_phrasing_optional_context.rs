//! How the answer reader treats the optional detail a contract declares next to its
//! required facts: a pod name, a run id, a commit, a draft amount, a customer name.
//!
//! A reply passes when it states that detail correctly and when it leaves it out.
//!
//! Optional facts are context that never decides the verdict (`statements::check`), so a
//! reply that states a fabricated detail, or one the source no longer holds, next to
//! correct required facts passes too. Those replies are recorded as known false passes
//! (`fabricated-optional-detail`, `stale-optional-detail`) in
//! `fixtures/wrong_answers/from_unit_tests.json`, which `natural_answers.rs` grades.

mod statement_support;

use mock_core::grading::Outcome;
use serde_json::{Value, json};
use statement_support::Case;

#[track_caller]
fn passes_after(case: &Case, pointer: &str, value: Value, answer: &str) {
    assert_eq!(
        case.outcome_after(pointer, value, answer),
        Outcome::Pass,
        "answer: {answer}"
    );
}

// ---- business/stripe/customer-mrr ----

#[test]
fn stripe_mrr_follows_a_renamed_customer() {
    let case = Case::load("business/stripe/customer-mrr");
    let name = "/services/stripe/customers/cus_mxxxdqMeoA1GEc/name";
    passes_after(
        &case,
        name,
        json!("Harbor & Pine Roasters"),
        "Harbor & Pine Roasters has a monthly recurring revenue of $439.",
    );
}

// ---- code/argocd/find-degraded-app ----

#[test]
fn argocd_reply_passes_with_or_without_the_pod_name() {
    let case = Case::load("code/argocd/find-degraded-app");
    case.passes(&[
        "Application: checkout-web. Tag: 2.14.0-rc3. Root cause: ImagePullBackOff. Pod checkout-web-7f8b9c6d5-q2x7m.",
        "Application: checkout-web. Tag: 2.14.0-rc3. Root cause: ImagePullBackOff.",
    ]);
}

// ---- code/github/ci-failure-root-cause ----

#[test]
fn github_ci_reply_passes_with_the_run_id_or_test_location() {
    let case = Case::load("code/github/ci-failure-root-cause");
    case.passes(&[
        "unit-tests failed: TestRetryBackoffCapsAtMaxDelay, \"expected delay 30s, got 32s\". Run 9005.",
        "unit-tests failed: TestRetryBackoffCapsAtMaxDelay, \"expected delay 30s, got 32s\". At backoff_test.go:17.",
    ]);
}

// ---- code/multi/argocd-degraded-gitlab-commit ----

#[test]
fn argocd_gitlab_reply_passes_without_the_commit() {
    let case = Case::load("code/multi/argocd-degraded-gitlab-commit");
    case.passes(&[
        "App: checkout-web, failing on image tag 2.14.0-rc3. Pipeline 7712: image-publish failed, permission_denied.",
    ]);
}

const SHORT_COMMIT_REPLY: &str = "App: checkout-web, failing on image tag 2.14.0-rc3. Pipeline 7712: image-publish failed, permission_denied. Commit a23f2a7.";

/// The case with the deployed revision replaced in both the Argo CD application and the
/// GitLab commit it joins to.
fn argocd_gitlab_with_another_revision() -> Case {
    let mut case = Case::load("code/multi/argocd-degraded-gitlab-commit");
    let old_revision = "a23f2a7b21f788c07d95ceaeb529e54b7ae3dbcd";
    let new_revision = "b84a6d9c21f788c07d95ceaeb529e54b7ae3dbcd";
    for application in case.initial["services"]["argocd"]["applications"]
        .as_object_mut()
        .unwrap()
        .values_mut()
    {
        if application
            .pointer("/status/sync/revision")
            .and_then(Value::as_str)
            == Some(old_revision)
        {
            *application.pointer_mut("/status/sync/revision").unwrap() = json!(new_revision);
        }
    }
    for commit in case.initial["services"]["gitlab"]["commits"]
        .as_object_mut()
        .unwrap()
        .values_mut()
    {
        if commit["id"].as_str() == Some(old_revision) {
            commit["id"] = json!(new_revision);
        }
    }
    case
}

#[test]
fn argocd_gitlab_short_commit_follows_the_joined_source() {
    Case::load("code/multi/argocd-degraded-gitlab-commit").passes(&[SHORT_COMMIT_REPLY]);
    argocd_gitlab_with_another_revision()
        .passes(&[&SHORT_COMMIT_REPLY.replace("Commit a23f2a7", "Commit b84a6d9")]);
}

const READ_SCOPE_TRACE: &str = "Running with gitlab-runner 18.4.0\n\
     $ docker push ghcr.io/lumen-retail/checkout-web:2.14.0-rc3\n\
     denied: permission_denied: The token provided does not match expected scopes (read:packages). The CI token GHCR_TOKEN expired.\n\
     ERROR: Job failed: command terminated with exit code 1";

#[test]
fn argocd_gitlab_reply_passes_without_the_scope_after_the_trace_changes() {
    let case = Case::load("code/multi/argocd-degraded-gitlab-commit");
    passes_after(
        &case,
        "/services/gitlab/jobs/77123/trace",
        json!(READ_SCOPE_TRACE),
        "checkout-web (2.14.0-rc3). Pipeline 7712: image-publish failed, permission_denied.",
    );
}

// ---- collab/google/northwind-invoice-lookup ----

const REDRAFTED_INVOICE_EMAIL: &str = "Hello Octo Corp team,\n\n\
     Please find invoice INV-20931 for October services attached.\n\n\
     PO number: PO-48213\n\
     Amount due: $12,480.00\n\
     Due date: November 20, 2026\n\n\
     Note: this replaces the draft figure of $10,500.00 we mentioned on our call.\n\n\
     Thank you,\nNorthwind Traders Billing";

#[test]
fn google_invoice_reply_passes_without_the_draft_amount_after_it_changes() {
    let case = Case::load("collab/google/northwind-invoice-lookup");
    passes_after(
        &case,
        "/services/google/gmail_messages/b26a427d1034a022/body_text",
        json!(REDRAFTED_INVOICE_EMAIL),
        "PO-48213, $12,480.00, due November 20, 2026.",
    );
}

// ---- collab/jira/blocked-sprint-item ----

#[test]
fn jira_blocked_reply_passes_without_the_blocker_status_after_it_changes() {
    let case = Case::load("collab/jira/blocked-sprint-item");
    passes_after(
        &case,
        "/services/jira/issues/PAY-117/fields/status",
        json!("Done"),
        "PAY-119 is blocked by PAY-117, assigned to Sam Ortiz.",
    );
}

// ---- collab/multi/jira-sentry-crash-impact ----

#[test]
fn jira_sentry_reply_passes_without_the_last_release_after_it_changes() {
    let case = Case::load("collab/multi/jira-sentry-crash-impact");
    passes_after(
        &case,
        "/services/sentry/issues/STOREFRONT-WEB-8D/lastRelease",
        json!("storefront@5.4.0"),
        "STOREFRONT-WEB-8D: 2,172 events and 1,318 users, first seen in 5.3.0.",
    );
}

// ---- collab/slack/observability-decision ----

const FIVE_YEAR_DECISION: &str = "Decision: we're going with Grafana Cloud. Contract signed today; it starts 2026-11-15 on a 5-year term. Datadog stays until the migration completes on 2027-01-31.";

#[test]
fn slack_reply_passes_without_the_contract_term_after_it_changes() {
    let case = Case::load("collab/slack/observability-decision");
    passes_after(
        &case,
        "/services/slack/messages/C03INFRA9QX~11793131800.099001/text",
        json!(FIVE_YEAR_DECISION),
        "Vendor: Grafana Cloud. Contract start: November 15, 2026.",
    );
}
