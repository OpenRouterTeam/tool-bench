//! Assertion-coverage tests for alternative answer forms in ops and search
//! scenarios that were previously blocked by incomplete assertion templates.

mod statement_support;

use mock_core::grading::Outcome;
use serde_json::json;
use statement_support::Case;

// ---------------------------------------------------------------------------
// Ops: datadog/checkout-p95-peak
// ---------------------------------------------------------------------------

#[test]
fn datadog_peak_iso_timestamp_passes() {
    let case = Case::load("ops/datadog/checkout-p95-peak");
    case.passes(&["The highest p95 was 1.843 s, reached at 2026-10-29T14:35:00Z."]);
}

#[test]
fn datadog_peak_other_forms_still_pass() {
    let case = Case::load("ops/datadog/checkout-p95-peak");
    case.passes(&[
        "Peak: 1843 ms at 14:35 UTC.",
        "| Peak p95 | 1,843 ms |\n| Time (UTC) | 14:35 |",
        "1843 milliseconds at 14:35:00 UTC",
        "1843 ms at 14:35 UTC, not the 2210 ms staging peak.",
    ]);
}

#[test]
fn datadog_peak_adversarial_still_fail() {
    let case = Case::load("ops/datadog/checkout-p95-peak");
    case.fails(&[
        "The peak was not 1843 ms and it was not at 14:35.",
        "2210 ms at 14:15 UTC",
        "1834 ms at 14:35 UTC",
    ]);
}

// ---------------------------------------------------------------------------
// Ops: sentry-release-commit-lookup
// ---------------------------------------------------------------------------

#[test]
fn sentry_release_semicolon_form_passes() {
    let case = Case::load("ops/multi/sentry-release-commit-lookup");
    let count = "/services/sentry/issues/STOREFRONT-WEB-8D/count";
    assert_eq!(
        case.outcome_after(
            count,
            json!("9000"),
            "STOREFRONT-WEB-8D first appeared in storefront@5.3.0; PR #63 by Kenji Watanabe."
        ),
        Outcome::Pass
    );
}

#[test]
fn sentry_release_original_forms_still_pass() {
    let case = Case::load("ops/multi/sentry-release-commit-lookup");
    case.passes(&[
        "Issue STOREFRONT-WEB-7Q first appeared in release storefront@5.2.0. It was introduced by PR #57, authored by Sofia Alvarez.",
        "storefront-web-7q, storefront@5.2.0, pull request 57 by @sofia",
    ]);
}

#[test]
fn sentry_release_adversarial_still_fail() {
    let case = Case::load("ops/multi/sentry-release-commit-lookup");
    case.fails(&[
        "The issue is not STOREFRONT-WEB-7Q; release storefront@5.2.0, PR #57 by Sofia Alvarez.",
        "STOREFRONT-WEB-8D first appeared in storefront@5.3.0; PR #63 by Kenji Watanabe.",
    ]);
}

// ---------------------------------------------------------------------------
// Ops: kubernetes/crashloop-root-cause
// ---------------------------------------------------------------------------

#[test]
fn k8s_crashloop_long_log_form_passes() {
    let case = Case::load("ops/kubernetes/crashloop-root-cause");
    case.passes(&[
        "`ledger-worker-zx4mrndj8b-gdg8j` is crash-looping. Its previous container log ends with:\n```\n2026-11-03T08:33:44.302Z ERROR db: connect ledger-db.payments.svc:5432: pq: password authentication failed for user \"ledger\"\n2026-11-03T08:33:44.302Z FATAL could not initialise ledger store, exiting (exit code 1)\n```",
    ]);
}

#[test]
fn k8s_crashloop_short_forms_still_pass() {
    let case = Case::load("ops/kubernetes/crashloop-root-cause");
    case.passes(&[
        "Pod: ledger-worker-zx4mrndj8b-gdg8j\nError: pq: password authentication failed for user \"ledger\"",
        "ledger-worker-zx4mrndj8b-gdg8j cannot authenticate to Postgres: password authentication failed for user \"ledger\".",
        "ledger-worker-zx4mrndj8b-gdg8j never becomes ready: password authentication failed for user \"ledger\".",
    ]);
}

#[test]
fn k8s_crashloop_adversarial_still_fail() {
    let case = Case::load("ops/kubernetes/crashloop-root-cause");
    case.fails(&[
        "The pod is not ledger-worker-zx4mrndj8b-gdg8j; password authentication failed for user \"ledger\".",
        "ledger-worker-c64m9bbw8-v45ss: password authentication failed for user \"ledger\"",
        "ledger-worker-zx4mrndj8b-gdg8j: OOMKilled",
    ]);
}

// ---------------------------------------------------------------------------
// Search: brave/keynote-recording
// ---------------------------------------------------------------------------

#[test]
fn keynote_comma_separated_form_passes() {
    let case = Case::load("search/brave/keynote-recording");
    let duration = "/services/brave/videos/https:~1~1www.youtube.com~1watch?v=Zt4mQeL1x9c/duration";
    assert_eq!(
        case.outcome_after(
            duration,
            json!("51:03"),
            "Marta Lindqvist, 51:03, https://www.youtube.com/watch?v=Zt4mQeL1x9c"
        ),
        Outcome::Pass
    );
}

#[test]
fn keynote_adversarial_still_fail() {
    let case = Case::load("search/brave/keynote-recording");
    case.fails(&[
        "The speaker is not Marta Lindqvist; 47:12, https://www.youtube.com/watch?v=Zt4mQeL1x9c",
        "Tom Ekberg, 39:55, https://vimeo.com/987654321",
    ]);
}

// ---------------------------------------------------------------------------
// Search: brave/hosting-sla-lookup
// ---------------------------------------------------------------------------

#[test]
fn sla_monthly_uptime_credit_form_passes() {
    let case = Case::load("search/brave/hosting-sla-lookup");
    let content = "/services/brave/web/https:~1~1www.ironbarkhosting.com~1legal~1sla/content";
    let edited_content = {
        let text = case.initial.pointer(content).unwrap().as_str().unwrap();
        let replaced = text.replace(
            "| Business | 99.95% | 10% of the monthly fee below 99.95%",
            "| Business | 99.97% | 15% of the monthly fee below 99.97%",
        );
        json!(replaced)
    };
    assert_eq!(
        case.outcome_after(
            content,
            edited_content,
            "99.97% monthly uptime, 15% service credit."
        ),
        Outcome::Pass
    );
}

#[test]
fn sla_adversarial_still_fail() {
    let case = Case::load("search/brave/hosting-sla-lookup");
    case.fails(&[
        "It is not 99.95% and the credit is not 10%.",
        "99.9% monthly uptime with a 5% service credit.",
    ]);
}

// ---------------------------------------------------------------------------
// Search: firecrawl/northwind-team-pricing
// ---------------------------------------------------------------------------

#[test]
fn team_pricing_billed_annually_form_passes() {
    let case = Case::load("search/firecrawl/northwind-team-pricing");
    let markdown =
        "/services/firecrawl/pages/https:~1~1www.northwind-analytics.com~1pricing/markdown";
    let text = case.initial.pointer(markdown).unwrap().as_str().unwrap();
    let edited = json!(text.replace("- 12 seats included", "- 15 seats included"));
    assert_eq!(
        case.outcome_after(markdown, edited, "$79/month billed annually, 15 seats."),
        Outcome::Pass
    );
}

#[test]
fn team_pricing_adversarial_still_fail() {
    let case = Case::load("search/firecrawl/northwind-team-pricing");
    case.fails(&[
        "It is not $79 per month and it does not include 12 seats.",
        "$69/month billed annually with 10 seats.",
    ]);
}

// ---------------------------------------------------------------------------
// Search: tavily/vendor-rate-limit
// ---------------------------------------------------------------------------

#[test]
fn rate_limit_comma_from_form_passes() {
    let case = Case::load("search/tavily/vendor-rate-limit");
    let raw = "/services/tavily/pages/https:~1~1docs.quillstream.io~1api~1rate-limits/raw_content";
    let text = case.initial.pointer(raw).unwrap().as_str().unwrap();
    let edited = json!(text.replace("| Business | 1,200 | 200 |", "| Business | 1,500 | 200 |"));
    assert_eq!(
        case.outcome_after(
            raw,
            edited,
            "1,500 requests per minute, from https://docs.quillstream.io/api/rate-limits"
        ),
        Outcome::Pass
    );
}

#[test]
fn rate_limit_adversarial_still_fail() {
    let case = Case::load("search/tavily/vendor-rate-limit");
    case.fails(&[
        "It is not 1,200 requests per minute (https://docs.quillstream.io/api/rate-limits).",
        "600 requests per minute, from https://docs.quillstream.io/v2/api/rate-limits",
    ]);
}

// ---------------------------------------------------------------------------
// Search: tavily/webhook-signature-sunset
// ---------------------------------------------------------------------------

#[test]
fn signature_sunset_semicolon_verify_form_passes() {
    let case = Case::load("search/tavily/webhook-signature-sunset");
    let raw = "/services/tavily/pages/https:~1~1docs.ferrowave.dev~1guides~1webhooks~1verify-signatures/raw_content";
    let text = case.initial.pointer(raw).unwrap().as_str().unwrap();
    let edited = json!(text.replace("January 15, 2027", "March 2, 2027"));
    assert_eq!(
        case.outcome_after(raw, edited, "2027-03-02; verify Ferrowave-Signature-V2."),
        Outcome::Pass
    );
}

#[test]
fn signature_sunset_adversarial_still_fail() {
    let case = Case::load("search/tavily/webhook-signature-sunset");
    case.fails(&[
        "It is not January 15, 2027, and the header is not Ferrowave-Signature-V2.",
        "December 1, 2026; verify Ferrowave-Signature-V2.",
    ]);
}

// ---------------------------------------------------------------------------
// Search: multi/competitor-funding-and-pricing
// ---------------------------------------------------------------------------

#[test]
fn multi_funding_monthly_billing_aside_passes() {
    let case = Case::load("search/multi/competitor-funding-and-pricing");
    case.passes(&[
        "$45M Series B led by Halcyon Ventures. Growth is $890/month billed annually, or $1,090 month to month.",
    ]);
}

#[test]
fn multi_funding_outcome_after_repriced_passes() {
    let case = Case::load("search/multi/competitor-funding-and-pricing");
    let markdown = "/services/firecrawl/pages/https:~1~1www.norvikanalytics.com~1pricing/markdown";
    let text = case.initial.pointer(markdown).unwrap().as_str().unwrap();
    let repriced = json!(text.replace("$890/month", "$920/month"));
    assert_eq!(
        case.outcome_after(
            markdown,
            repriced,
            "$45M Series B led by Halcyon Ventures. Growth is $920/month."
        ),
        Outcome::Pass
    );
}

#[test]
fn multi_funding_adversarial_still_fail() {
    let case = Case::load("search/multi/competitor-funding-and-pricing");
    case.fails(&[
        "$45M Series B led by Halcyon Ventures. Growth is not $890/month.",
        "$12M Series A led by Fjord Capital. Growth is $890/month.",
        "$45M Series B led by Halcyon Ventures.",
    ]);
}
