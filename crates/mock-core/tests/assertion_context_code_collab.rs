//! Mutation tests for source-bound context in code and collab scenarios.
//!
//! Each test verifies that an answer containing stale or wrong values for
//! source-bound optional context does not Pass, while a concise truthful
//! answer (omitting the optional fact or stating it correctly) does Pass.
//! Where the source field can be mutated without breaking join chains, the
//! test mutates the source and checks both stale-reject and updated-pass.

mod statement_support;

use mock_core::grading::Outcome;
use serde_json::json;
use statement_support::Case;

/// Stale context must not pass: either Fail or Unresolved is acceptable.
fn assert_does_not_pass(outcome: Outcome, label: &str) {
    assert_ne!(outcome, Outcome::Pass, "{label}");
}

// ---- code/argocd/find-degraded-app: pod_name context binding ----

#[test]
fn argocd_stale_pod_name_does_not_pass() {
    let case = Case::load("code/argocd/find-degraded-app");
    // Mentioning a fabricated pod name is not stripped by context → uncovered text.
    assert_does_not_pass(
        case.outcome(
            "App: checkout-web. Tag: 2.14.0-rc3. ImagePullBackOff. Pod checkout-web-FAKE-pod.",
        ),
        "fabricated pod name must not pass",
    );
}

#[test]
fn argocd_correct_pod_name_passes() {
    let case = Case::load("code/argocd/find-degraded-app");
    // The real pod name is stripped by context, so it doesn't cause uncovered text.
    case.passes(&[
        "Application: checkout-web. Tag: 2.14.0-rc3. Root cause: ImagePullBackOff. Pod checkout-web-7f8b9c6d5-q2x7m.",
    ]);
}

#[test]
fn argocd_no_pod_name_passes() {
    let case = Case::load("code/argocd/find-degraded-app");
    // Omitting the optional pod name entirely still passes.
    case.passes(&["Application: checkout-web. Tag: 2.14.0-rc3. Root cause: ImagePullBackOff."]);
}

// ---- code/github/ci-failure-root-cause: run_id and test_location bindings ----

#[test]
fn github_ci_stale_run_id_does_not_pass() {
    let case = Case::load("code/github/ci-failure-root-cause");
    assert_does_not_pass(
        case.outcome("unit-tests failed: TestRetryBackoffCapsAtMaxDelay, \"expected delay 30s, got 32s\". Run 9999."),
        "fabricated run id must not pass"
    );
}

#[test]
fn github_ci_correct_run_id_passes() {
    let case = Case::load("code/github/ci-failure-root-cause");
    case.passes(&[
        "unit-tests failed: TestRetryBackoffCapsAtMaxDelay, \"expected delay 30s, got 32s\". Run 9005.",
    ]);
}

#[test]
fn github_ci_stale_test_location_does_not_pass() {
    let case = Case::load("code/github/ci-failure-root-cause");
    assert_does_not_pass(
        case.outcome("unit-tests failed: TestRetryBackoffCapsAtMaxDelay, \"expected delay 30s, got 32s\". At other_test.go:99."),
        "fabricated test location must not pass"
    );
}

#[test]
fn github_ci_correct_test_location_passes() {
    let case = Case::load("code/github/ci-failure-root-cause");
    case.passes(&[
        "unit-tests failed: TestRetryBackoffCapsAtMaxDelay, \"expected delay 30s, got 32s\". At backoff_test.go:17.",
    ]);
}

// ---- code/multi/argocd-degraded-gitlab-commit: commit_sha and scope bindings ----

#[test]
fn argocd_gitlab_stale_commit_sha_does_not_pass() {
    let case = Case::load("code/multi/argocd-degraded-gitlab-commit");
    assert_does_not_pass(
        case.outcome("checkout-web (2.14.0-rc3). Pipeline 7712: image-publish failed, permission_denied. Commit deadbeef."),
        "fabricated commit SHA must not pass"
    );
}

#[test]
fn argocd_gitlab_no_commit_sha_passes() {
    let case = Case::load("code/multi/argocd-degraded-gitlab-commit");
    // Omitting the optional commit SHA entirely still passes.
    case.passes(&[
        "App: checkout-web, failing on image tag 2.14.0-rc3. Pipeline 7712: image-publish failed, permission_denied.",
    ]);
}

#[test]
fn argocd_gitlab_short_commit_context_tracks_the_joined_source() {
    let mut case = Case::load("code/multi/argocd-degraded-gitlab-commit");
    let answer = "App: checkout-web, failing on image tag 2.14.0-rc3. Pipeline 7712: image-publish failed, permission_denied. Commit a23f2a7.";
    case.passes(&[answer]);
    let old_revision = "a23f2a7b21f788c07d95ceaeb529e54b7ae3dbcd";
    let new_revision = "b84a6d9c21f788c07d95ceaeb529e54b7ae3dbcd";
    for application in case.initial["services"]["argocd"]["applications"]
        .as_object_mut()
        .unwrap()
        .values_mut()
    {
        if application
            .pointer("/status/sync/revision")
            .and_then(serde_json::Value::as_str)
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
    assert_ne!(case.outcome(answer), Outcome::Pass);
    case.passes(&[&answer.replace("Commit a23f2a7", "Commit b84a6d9")]);
}

#[test]
fn argocd_gitlab_scope_mutation_rejects_stale_scope() {
    let case = Case::load("code/multi/argocd-degraded-gitlab-commit");
    let pointer = "/services/gitlab/jobs/77123/trace";
    let new_trace = "Running with gitlab-runner 18.4.0\n\
                     $ docker push ghcr.io/lumen-retail/checkout-web:2.14.0-rc3\n\
                     denied: permission_denied: The token provided does not match expected scopes (read:packages). The CI token GHCR_TOKEN expired.\n\
                     ERROR: Job failed: command terminated with exit code 1";
    assert_does_not_pass(
        case.outcome_after(
            pointer,
            json!(new_trace),
            "checkout-web (2.14.0-rc3). Pipeline 7712: image-publish failed, permission_denied. Scope write:packages."
        ),
        "stale scope write:packages must not pass after mutation to read:packages"
    );
    assert_eq!(
        case.outcome_after(
            pointer,
            json!(new_trace),
            "checkout-web (2.14.0-rc3). Pipeline 7712: image-publish failed, permission_denied."
        ),
        Outcome::Pass,
        "concise answer without optional scope must pass"
    );
}

// ---- collab/google/northwind-invoice-lookup: draft_amount context binding ----

#[test]
fn google_invoice_draft_amount_mutation_rejects_stale() {
    let case = Case::load("collab/google/northwind-invoice-lookup");
    let pointer = "/services/google/gmail_messages/b26a427d1034a022/body_text";
    let new_body = "Hello Octo Corp team,\n\n\
                    Please find invoice INV-20931 for October services attached.\n\n\
                    PO number: PO-48213\n\
                    Amount due: $12,480.00\n\
                    Due date: November 20, 2026\n\n\
                    Note: this replaces the draft figure of $10,500.00 we mentioned on our call.\n\n\
                    Thank you,\nNorthwind Traders Billing";
    assert_does_not_pass(
        case.outcome_after(
            pointer,
            json!(new_body),
            "PO-48213, $12,480.00, due November 20, 2026. This replaces the earlier draft figure of $11,900.00."
        ),
        "stale draft amount $11,900 must not pass after mutation to $10,500"
    );
    assert_eq!(
        case.outcome_after(
            pointer,
            json!(new_body),
            "PO-48213, $12,480.00, due November 20, 2026."
        ),
        Outcome::Pass,
        "concise answer without optional draft amount must pass"
    );
}

// ---- collab/jira/blocked-sprint-item: blocker_status context binding ----

#[test]
fn jira_blocked_blocker_status_mutation_rejects_stale() {
    let case = Case::load("collab/jira/blocked-sprint-item");
    let pointer = "/services/jira/issues/PAY-117/fields/status";
    assert_does_not_pass(
        case.outcome_after(
            pointer,
            json!("Done"),
            "PAY-119 is blocked by PAY-117, assigned to Sam Ortiz. In Progress.",
        ),
        "stale status In Progress must not pass after mutation to Done",
    );
    assert_eq!(
        case.outcome_after(
            pointer,
            json!("Done"),
            "PAY-119 is blocked by PAY-117, assigned to Sam Ortiz."
        ),
        Outcome::Pass,
        "concise answer without optional status must pass"
    );
}

// ---- collab/multi/jira-sentry-crash-impact: last_release context binding ----

#[test]
fn jira_sentry_last_release_mutation_rejects_stale() {
    let case = Case::load("collab/multi/jira-sentry-crash-impact");
    let pointer = "/services/sentry/issues/STOREFRONT-WEB-8D/lastRelease";
    assert_does_not_pass(
        case.outcome_after(
            pointer,
            json!("storefront@5.4.0"),
            "STOREFRONT-WEB-8D: 2,172 events and 1,318 users, first seen in 5.3.0. Last seen in storefront@5.3.1."
        ),
        "stale lastRelease 5.3.1 must not pass after mutation to 5.4.0"
    );
    assert_eq!(
        case.outcome_after(
            pointer,
            json!("storefront@5.4.0"),
            "STOREFRONT-WEB-8D: 2,172 events and 1,318 users, first seen in 5.3.0."
        ),
        Outcome::Pass,
        "concise answer without optional last release must pass"
    );
}

// ---- collab/slack/observability-decision: contract_term context binding ----

#[test]
fn slack_contract_term_mutation_rejects_stale() {
    let case = Case::load("collab/slack/observability-decision");
    let pointer = "/services/slack/messages/C03INFRA9QX~11793131800.099001/text";
    let new_text = "Decision: we're going with Grafana Cloud. Contract signed today; it starts 2026-11-15 on a 5-year term. Datadog stays until the migration completes on 2027-01-31.";
    assert_does_not_pass(
        case.outcome_after(
            pointer,
            json!(new_text),
            "Vendor: Grafana Cloud. Contract start: November 15, 2026. A 3-year term.",
        ),
        "stale 3-year term must not pass after mutation to 5-year",
    );
    assert_eq!(
        case.outcome_after(
            pointer,
            json!(new_text),
            "Vendor: Grafana Cloud. Contract start: November 15, 2026."
        ),
        Outcome::Pass,
        "concise answer without optional contract term must pass"
    );
}
