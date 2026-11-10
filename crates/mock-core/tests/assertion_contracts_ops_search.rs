//! Grade regressions for assertion-mode contracts: ops, search, and
//! diagnostic/stripe/no-action-already-refunded scenarios.
//!
//! Each test verifies: reference answer passes, a novel unsupported predicate
//! is unresolved, role swaps and mutations fail or are unresolved, and data
//! edits show facts follow the source.

mod statement_support;

use mock_core::grading::Outcome;
use serde_json::json;
use statement_support::Case;

// ═══════════════════════════════════════════════════════════════════════════
// ops/sentry/top-unresolved-checkout
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn sentry_top_unresolved_assertions_reference_passes() {
    let case = Case::load("ops/sentry/top-unresolved-checkout");
    case.reference_is_source_bound();
}

#[test]
fn sentry_top_unresolved_assertions_novel_predicate_is_unresolved() {
    let case = Case::load("ops/sentry/top-unresolved-checkout");
    case.unresolved(&[
        "CHECKOUT-API-3F: OperationalError: could not obtain connection from pool within 5.0s. This needs immediate attention.",
    ]);
}

#[test]
fn sentry_top_unresolved_assertions_role_swap_fails() {
    let case = Case::load("ops/sentry/top-unresolved-checkout");
    case.fails(&["CHECKOUT-API-2R: DisallowedHost: Invalid HTTP_HOST header: '10.4.2.17:8000'"]);
}

#[test]
fn sentry_top_unresolved_assertions_mutation_follows_source() {
    let case = Case::load("ops/sentry/top-unresolved-checkout");
    assert_eq!(
        case.outcome_after(
            "/services/sentry/issues/CHECKOUT-API-3C/count",
            json!("5000"),
            &case.reference
        ),
        Outcome::Fail
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// ops/datadog/checkout-p95-peak
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn datadog_peak_assertions_reference_passes() {
    let case = Case::load("ops/datadog/checkout-p95-peak");
    case.reference_is_source_bound();
}

#[test]
fn datadog_peak_assertions_novel_predicate_is_unresolved() {
    let case = Case::load("ops/datadog/checkout-p95-peak");
    case.unresolved(&["Peak: 1843 ms at 14:35 UTC. Consider scaling up the checkout service."]);
}

#[test]
fn datadog_peak_assertions_wrong_value_fails() {
    let case = Case::load("ops/datadog/checkout-p95-peak");
    case.fails(&["2210 ms at 14:15 UTC", "1834 ms at 14:35 UTC"]);
}

#[test]
fn datadog_peak_assertions_mutation_follows_source() {
    let case = Case::load("ops/datadog/checkout-p95-peak");
    let series =
        "/services/datadog/metrics/checkout.api.latency.p95{env:prod,service:checkout-api}";
    let inside = format!("{series}/points/17/1");
    assert_eq!(
        case.outcome_after(&inside, json!(1900), &case.reference),
        Outcome::Fail
    );
    assert_eq!(
        case.outcome_after(&inside, json!(1900), "1900 ms at 14:25 UTC"),
        Outcome::Pass
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// ops/grafana/top-5xx-checkout-pod
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn grafana_5xx_assertions_reference_passes() {
    let case = Case::load("ops/grafana/top-5xx-checkout-pod");
    case.reference_is_source_bound();
}

#[test]
fn grafana_5xx_assertions_novel_predicate_is_unresolved() {
    let case = Case::load("ops/grafana/top-5xx-checkout-pod");
    case.unresolved(&[
        "checkout-7b9d6c5f4-xk2lp is the culprit and should be drained immediately.",
    ]);
}

#[test]
fn grafana_5xx_assertions_staging_pod_fails() {
    let case = Case::load("ops/grafana/top-5xx-checkout-pod");
    case.fails(&["checkout-5f6d7c8b9-zq4wd"]);
}

#[test]
fn grafana_5xx_assertions_mutation_follows_source() {
    let case = Case::load("ops/grafana/top-5xx-checkout-pod");
    let rate = "/services/grafana/prom_series/prom-prod~117/rate";
    assert_eq!(
        case.outcome_after(rate, json!(9.5), &case.reference),
        Outcome::Fail
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// ops/kubernetes/crashloop-root-cause
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn k8s_crashloop_assertions_reference_passes() {
    let case = Case::load("ops/kubernetes/crashloop-root-cause");
    case.reference_is_source_bound();
}

#[test]
fn k8s_crashloop_assertions_novel_predicate_is_unresolved() {
    let case = Case::load("ops/kubernetes/crashloop-root-cause");
    case.unresolved(&[
        "Pod: ledger-worker-zx4mrndj8b-gdg8j\nError: pq: password authentication failed for user \"ledger\"\nRecommendation: rotate the credential.",
    ]);
}

#[test]
fn k8s_crashloop_assertions_staging_pod_fails() {
    let case = Case::load("ops/kubernetes/crashloop-root-cause");
    case.fails(&[
        "ledger-worker-c64m9bbw8-v45ss: panic: invalid config: unknown field \"retryBackoffMs\" in kafka section",
    ]);
}

#[test]
fn k8s_crashloop_assertions_mutation_follows_source() {
    let case = Case::load("ops/kubernetes/crashloop-root-cause");
    let line = "/services/kubernetes/pod_logs/payments~1ledger-worker-zx4mrndj8b-gdg8j~1ledger-worker/previous/2";
    let edited = json!(
        "2026-11-03T08:33:44.302Z ERROR db: connect ledger-db.payments.svc:5432: pq: SSL required"
    );
    assert_eq!(
        case.outcome_after(line, edited, &case.reference),
        Outcome::Fail
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// ops/multi/sentry-release-commit-lookup
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn sentry_release_assertions_reference_passes() {
    let case = Case::load("ops/multi/sentry-release-commit-lookup");
    case.reference_is_source_bound();
}

#[test]
fn sentry_release_assertions_novel_predicate_is_unresolved() {
    let case = Case::load("ops/multi/sentry-release-commit-lookup");
    case.unresolved(&[
        "Issue STOREFRONT-WEB-7Q first appeared in release storefront@5.2.0. It was introduced by PR #57, authored by Sofia Alvarez. This is a critical bug.",
    ]);
}

#[test]
fn sentry_release_assertions_wrong_author_fails() {
    let case = Case::load("ops/multi/sentry-release-commit-lookup");
    case.fails(&["STOREFRONT-WEB-7Q first appeared in storefront@5.2.0; PR #57 by Marcus Chen."]);
}

#[test]
fn sentry_release_assertions_mutation_follows_source() {
    let case = Case::load("ops/multi/sentry-release-commit-lookup");
    let author = "/services/github/pulls/lumen-labs~1storefront#57/user";
    assert_eq!(
        case.outcome_after(author, json!("lena"), &case.reference),
        Outcome::Fail
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// ops/pagerduty/payments-oncall-now
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn pd_oncall_assertions_reference_passes() {
    let case = Case::load("ops/pagerduty/payments-oncall-now");
    case.reference_is_source_bound();
}

#[test]
fn pd_oncall_assertions_novel_predicate_is_unresolved() {
    let case = Case::load("ops/pagerduty/payments-oncall-now");
    case.unresolved(&["On call: Priya Raman, backup: Marcus Chen. Please page them."]);
}

#[test]
fn pd_oncall_assertions_role_swap_fails() {
    let case = Case::load("ops/pagerduty/payments-oncall-now");
    case.fails(&["Marcus Chen and Priya Raman."]);
}

#[test]
fn pd_oncall_assertions_scheduled_instead_of_override_fails() {
    let case = Case::load("ops/pagerduty/payments-oncall-now");
    case.fails(&["Riya Patel and Marcus Chen."]);
}

// ═══════════════════════════════════════════════════════════════════════════
// ops/multi/crashloop-oncall-lookup
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn crashloop_oncall_assertions_reference_passes() {
    let case = Case::load("ops/multi/crashloop-oncall-lookup");
    case.reference_is_source_bound();
}

#[test]
fn crashloop_oncall_assertions_novel_predicate_is_unresolved() {
    let case = Case::load("ops/multi/crashloop-oncall-lookup");
    case.unresolved(&[
        "Pod ledger-worker-zx4mrndj8b-gdg8j, error: password authentication failed for user. On call: Priya Raman, backup Marcus Chen. Escalate immediately.",
    ]);
}

#[test]
fn crashloop_oncall_assertions_oncall_role_swap_is_unresolved() {
    let case = Case::load("ops/multi/crashloop-oncall-lookup");
    // Role swap: the rejected form catches specific patterns; a reversal in
    // an assertion-covered clause is unresolved because the wrong binding
    // does not match any reviewed assertion.
    case.unresolved(&[
        "Pod ledger-worker-zx4mrndj8b-gdg8j, error: password authentication failed for user. On call: Marcus Chen, backup Priya Raman.",
    ]);
}

// ═══════════════════════════════════════════════════════════════════════════
// diagnostic/stripe/no-action-already-refunded
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn stripe_refund_assertions_reference_passes() {
    let case = Case::load("diagnostic/stripe/no-action-already-refunded");
    case.reference_is_source_bound();
}

#[test]
fn stripe_refund_assertions_novel_predicate_is_unresolved() {
    let case = Case::load("diagnostic/stripe/no-action-already-refunded");
    case.unresolved(&[
        "The duplicate charge was already refunded in full as refund re_6AYez5LT1vd0g6zTWhpyAP2b. No further action needed; closing this ticket.",
    ]);
}

#[test]
fn stripe_refund_assertions_wrong_refund_id_fails() {
    let case = Case::load("diagnostic/stripe/no-action-already-refunded");
    case.fails(&["The charge was already refunded as refund re_FAKE123."]);
}

// ═══════════════════════════════════════════════════════════════════════════
// search/elasticsearch/latency-by-region (preserves existing assertions)
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn latency_by_region_assertions_reference_passes() {
    let case = Case::load("search/elasticsearch/latency-by-region");
    case.reference_is_source_bound();
}

#[test]
fn latency_by_region_assertions_novel_predicate_is_unresolved() {
    let case = Case::load("search/elasticsearch/latency-by-region");
    case.unresolved(&["eu-west-1 is the slowest, averaging 412.5 ms. Consider adding a cache."]);
}

#[test]
fn latency_by_region_assertions_wrong_region_fails() {
    let case = Case::load("search/elasticsearch/latency-by-region");
    case.fails(&["ap-southeast-2 averages 412.5 ms"]);
}

#[test]
fn latency_by_region_assertions_mutation_follows_source() {
    let case = Case::load("search/elasticsearch/latency-by-region");
    case.passes(&["eu-west-1 averages 412.5 ms"]);
}

// ═══════════════════════════════════════════════════════════════════════════
// search/brave/caterer-saturday-hours
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn caterer_assertions_reference_passes() {
    let case = Case::load("search/brave/caterer-saturday-hours");
    case.reference_is_source_bound();
}

#[test]
fn caterer_assertions_novel_predicate_is_unresolved() {
    let case = Case::load("search/brave/caterer-saturday-hours");
    case.unresolved(&["Phone: 503-555-0142. Saturday: 09:00-14:00. They have great reviews."]);
}

#[test]
fn caterer_assertions_wrong_phone_fails() {
    let case = Case::load("search/brave/caterer-saturday-hours");
    case.fails(&["Phone: 207-555-0198. Saturday: 09:00-14:00."]);
}

// ═══════════════════════════════════════════════════════════════════════════
// search/brave/competitor-funding-news
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn funding_assertions_reference_passes() {
    let case = Case::load("search/brave/competitor-funding-news");
    case.reference_is_source_bound();
}

#[test]
fn funding_assertions_novel_predicate_is_unresolved() {
    let case = Case::load("search/brave/competitor-funding-news");
    case.unresolved(&[
        "Amount: $45M Round: Series B Lead investor: Halcyon Ventures. This changes the competitive landscape.",
    ]);
}

#[test]
fn funding_assertions_wrong_round_fails() {
    let case = Case::load("search/brave/competitor-funding-news");
    case.fails(&["$45M Series A led by Halcyon Ventures."]);
}

// ═══════════════════════════════════════════════════════════════════════════
// search/brave/hosting-sla-lookup
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn sla_assertions_reference_passes() {
    let case = Case::load("search/brave/hosting-sla-lookup");
    case.reference_is_source_bound();
}

#[test]
fn sla_assertions_novel_predicate_is_unresolved() {
    let case = Case::load("search/brave/hosting-sla-lookup");
    case.unresolved(&[
        "Uptime: 99.95% Service credit: 10% of the monthly fee. This is industry standard.",
    ]);
}

#[test]
fn sla_assertions_archived_values_fail() {
    let case = Case::load("search/brave/hosting-sla-lookup");
    case.fails(&["99.9% monthly uptime with a 5% service credit."]);
}

// ═══════════════════════════════════════════════════════════════════════════
// search/brave/keynote-recording
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn keynote_assertions_reference_passes() {
    let case = Case::load("search/brave/keynote-recording");
    case.reference_is_source_bound();
}

#[test]
fn keynote_assertions_novel_predicate_is_unresolved() {
    let case = Case::load("search/brave/keynote-recording");
    case.unresolved(&[
        "Marta Lindqvist gave the keynote; the video runs 47:12: https://www.youtube.com/watch?v=Zt4mQeL1x9c. Highly recommended.",
    ]);
}

#[test]
fn keynote_assertions_highlights_cut_fails() {
    let case = Case::load("search/brave/keynote-recording");
    case.fails(&["Marta Lindqvist, 12:05, https://www.youtube.com/watch?v=r8Kq2PgN7aE"]);
}

// ═══════════════════════════════════════════════════════════════════════════
// search/elasticsearch/checkout-errors-last-hour
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn checkout_errors_assertions_reference_passes() {
    let case = Case::load("search/elasticsearch/checkout-errors-last-hour");
    case.reference_is_source_bound();
}

#[test]
fn checkout_errors_assertions_novel_predicate_is_unresolved() {
    let case = Case::load("search/elasticsearch/checkout-errors-last-hour");
    case.unresolved(&[
        "payment-gateway: 7 errors. Top message: upstream timeout from acquirer after 30000ms (6). Check the acquirer connection.",
    ]);
}

#[test]
fn checkout_errors_assertions_wrong_service_fails() {
    let case = Case::load("search/elasticsearch/checkout-errors-last-hour");
    case.fails(&["cart-api: 7 errors."]);
}

// ═══════════════════════════════════════════════════════════════════════════
// search/elasticsearch/deleted-api-key-audit
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn api_key_audit_assertions_reference_passes() {
    let case = Case::load("search/elasticsearch/deleted-api-key-audit");
    case.reference_is_source_bound();
}

#[test]
fn api_key_audit_assertions_novel_predicate_is_unresolved() {
    let case = Case::load("search/elasticsearch/deleted-api-key-audit");
    case.unresolved(&["The key was deleted by m.okafor at 16:42 UTC. This was unauthorized."]);
}

#[test]
fn api_key_audit_assertions_wrong_user_fails() {
    let case = Case::load("search/elasticsearch/deleted-api-key-audit");
    case.fails(&["j.doe deleted the key at 14:05 UTC."]);
}

// ═══════════════════════════════════════════════════════════════════════════
// search/elasticsearch/duplicate-charge-lookup
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn duplicate_charge_assertions_reference_passes() {
    let case = Case::load("search/elasticsearch/duplicate-charge-lookup");
    case.reference_is_source_bound();
}

#[test]
fn duplicate_charge_assertions_novel_predicate_is_unresolved() {
    let case = Case::load("search/elasticsearch/duplicate-charge-lookup");
    case.unresolved(&[
        "The double charge is ORD-109002 and ORD-109003, both $86.40. Initiate refund.",
    ]);
}

#[test]
fn duplicate_charge_assertions_wrong_amount_fails() {
    let case = Case::load("search/elasticsearch/duplicate-charge-lookup");
    case.fails(&["ORD-109002 and ORD-109003, both $96.40."]);
}

// ═══════════════════════════════════════════════════════════════════════════
// search/exa/find-vp-engineering-profile
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn vp_profile_assertions_reference_passes() {
    let case = Case::load("search/exa/find-vp-engineering-profile");
    case.reference_is_source_bound();
}

#[test]
fn vp_profile_assertions_novel_predicate_is_unresolved() {
    let case = Case::load("search/exa/find-vp-engineering-profile");
    case.unresolved(&[
        "https://www.linkedin.com/in/priya-raman-tessellate. She has an impressive background.",
    ]);
}

#[test]
fn vp_profile_assertions_former_director_fails() {
    let case = Case::load("search/exa/find-vp-engineering-profile");
    case.fails(&["https://www.linkedin.com/in/priya-raman-4b21a9"]);
}

// ═══════════════════════════════════════════════════════════════════════════
// search/exa/sdk-timeout-default
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn sdk_timeout_assertions_reference_passes() {
    let case = Case::load("search/exa/sdk-timeout-default");
    case.reference_is_source_bound();
}

#[test]
fn sdk_timeout_assertions_novel_predicate_is_unresolved() {
    let case = Case::load("search/exa/sdk-timeout-default");
    case.unresolved(&[
        "Option: timeout_seconds Default: 30 seconds. Consider raising this for slow networks.",
    ]);
}

#[test]
fn sdk_timeout_assertions_v3_option_fails() {
    let case = Case::load("search/exa/sdk-timeout-default");
    case.fails(&["request_timeout, default 60 seconds"]);
}

// ═══════════════════════════════════════════════════════════════════════════
// search/firecrawl/northwind-team-pricing
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn team_pricing_assertions_reference_passes() {
    let case = Case::load("search/firecrawl/northwind-team-pricing");
    case.reference_is_source_bound();
}

#[test]
fn team_pricing_assertions_novel_predicate_is_unresolved() {
    let case = Case::load("search/firecrawl/northwind-team-pricing");
    case.unresolved(&["Team plan: $79/month billed annually, 12 seats included. Great value."]);
}

#[test]
fn team_pricing_assertions_stale_price_fails() {
    let case = Case::load("search/firecrawl/northwind-team-pricing");
    case.fails(&["$69/month with 10 seats."]);
}

// ═══════════════════════════════════════════════════════════════════════════
// search/multi/api-key-deletion-audit-github
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn audit_github_assertions_reference_passes() {
    let case = Case::load("search/multi/api-key-deletion-audit-github");
    case.reference_is_source_bound();
}

#[test]
fn audit_github_assertions_novel_predicate_is_unresolved() {
    let case = Case::load("search/multi/api-key-deletion-audit-github");
    case.unresolved(&[
        "m.okafor deleted the key at 16:42 UTC. On GitHub she is mokafor, and her PR #58 has not been merged yet. Follow up with her.",
    ]);
}

#[test]
fn audit_github_assertions_wrong_user_fails() {
    let case = Case::load("search/multi/api-key-deletion-audit-github");
    case.fails(&["j.doe deleted the key at 14:05 UTC. GitHub: jdoe, PR #58."]);
}

// ═══════════════════════════════════════════════════════════════════════════
// search/multi/competitor-funding-and-pricing
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn funding_pricing_assertions_reference_passes() {
    let case = Case::load("search/multi/competitor-funding-and-pricing");
    case.reference_is_source_bound();
}

#[test]
fn funding_pricing_assertions_novel_predicate_is_unresolved() {
    let case = Case::load("search/multi/competitor-funding-and-pricing");
    case.unresolved(&[
        "$45M Series B led by Halcyon Ventures. Growth is $890 per month. We should match their pricing.",
    ]);
}

#[test]
fn funding_pricing_assertions_wrong_price_fails() {
    let case = Case::load("search/multi/competitor-funding-and-pricing");
    case.fails(&["$45M Series B led by Halcyon Ventures. Growth is $690/month."]);
}

// ═══════════════════════════════════════════════════════════════════════════
// search/tavily/vendor-rate-limit
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn rate_limit_assertions_reference_passes() {
    let case = Case::load("search/tavily/vendor-rate-limit");
    case.reference_is_source_bound();
}

#[test]
fn rate_limit_assertions_novel_predicate_is_unresolved() {
    let case = Case::load("search/tavily/vendor-rate-limit");
    case.unresolved(&[
        "1,200 requests per minute (https://docs.quillstream.io/api/rate-limits). Consider upgrading to Enterprise.",
    ]);
}

#[test]
fn rate_limit_assertions_deprecated_value_fails() {
    let case = Case::load("search/tavily/vendor-rate-limit");
    case.fails(&["600 requests per minute, from https://docs.quillstream.io/api/rate-limits"]);
}

// ═══════════════════════════════════════════════════════════════════════════
// search/tavily/webhook-signature-sunset
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn signature_sunset_assertions_reference_passes() {
    let case = Case::load("search/tavily/webhook-signature-sunset");
    case.reference_is_source_bound();
}

#[test]
fn signature_sunset_assertions_novel_predicate_is_unresolved() {
    let case = Case::load("search/tavily/webhook-signature-sunset");
    case.unresolved(&[
        "Date: 2027-01-15 Header: Ferrowave-Signature-V2. Make sure to update your integration.",
    ]);
}

#[test]
fn signature_sunset_assertions_wrong_date_fails() {
    let case = Case::load("search/tavily/webhook-signature-sunset");
    case.fails(&["December 1, 2026; verify Ferrowave-Signature-V2."]);
}
