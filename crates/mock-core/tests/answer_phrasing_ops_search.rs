//! How the answer reader treats differently phrased replies to the ops, search and
//! diagnostic lookup scenarios.
//!
//! A correct reply passes however it is phrased. A reply that denies a fact, names
//! another record, swaps two roles, states a wrong value or repeats a value the source no
//! longer holds earns no reward: it fails or is left unresolved.

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

/// The authored text at `pointer` with `from` replaced by `to`.
fn rewritten(case: &Case, pointer: &str, from: &str, to: &str) -> Value {
    let text = case.initial.pointer(pointer).unwrap().as_str().unwrap();
    assert!(text.contains(from), "{pointer} does not hold {from:?}");
    json!(text.replace(from, to))
}

// ---------- ops/sentry/top-unresolved-checkout ----------

#[test]
fn sentry_top_unresolved_other_issue_earns_no_reward() {
    let case = Case::load("ops/sentry/top-unresolved-checkout");
    rejects(
        &case,
        &["CHECKOUT-API-2R: DisallowedHost: Invalid HTTP_HOST header: '10.4.2.17:8000'"],
    );
}

#[test]
fn sentry_top_unresolved_follows_the_event_counts() {
    let case = Case::load("ops/sentry/top-unresolved-checkout");
    rejects_after(
        &case,
        "/services/sentry/issues/CHECKOUT-API-3C/count",
        json!("5000"),
        &case.reference,
    );
}

// ---------- ops/datadog/checkout-p95-peak ----------

#[test]
fn datadog_peak_passes_in_other_notations() {
    let case = Case::load("ops/datadog/checkout-p95-peak");
    case.passes(&[
        "The highest p95 was 1.843 s, reached at 2026-10-29T14:35:00Z.",
        "Peak: 1843 ms at 14:35 UTC.",
        "| Peak p95 | 1,843 ms |\n| Time (UTC) | 14:35 |",
        "1843 milliseconds at 14:35:00 UTC",
        "1843 ms at 14:35 UTC, not the 2210 ms staging peak.",
    ]);
}

#[test]
fn datadog_peak_denied_or_wrong_values_earn_no_reward() {
    let case = Case::load("ops/datadog/checkout-p95-peak");
    rejects(
        &case,
        &[
            "The peak was not 1843 ms and it was not at 14:35.",
            "2210 ms at 14:15 UTC",
            "1834 ms at 14:35 UTC",
        ],
    );
}

#[test]
fn datadog_peak_follows_the_series() {
    let case = Case::load("ops/datadog/checkout-p95-peak");
    let point = "/services/datadog/metrics/checkout.api.latency.p95{env:prod,service:checkout-api}/points/17/1";
    rejects_after(&case, point, json!(1900), &case.reference);
    passes_after(&case, point, json!(1900), "1900 ms at 14:25 UTC");
}

// ---------- ops/grafana/top-5xx-checkout-pod ----------

#[test]
fn grafana_5xx_staging_pod_earns_no_reward() {
    let case = Case::load("ops/grafana/top-5xx-checkout-pod");
    rejects(&case, &["checkout-5f6d7c8b9-zq4wd"]);
}

#[test]
fn grafana_5xx_follows_the_series_rates() {
    let case = Case::load("ops/grafana/top-5xx-checkout-pod");
    rejects_after(
        &case,
        "/services/grafana/prom_series/prom-prod~117/rate",
        json!(9.5),
        &case.reference,
    );
}

// ---------- ops/kubernetes/crashloop-root-cause ----------

#[test]
fn k8s_crashloop_passes_as_a_log_excerpt_or_a_summary() {
    let case = Case::load("ops/kubernetes/crashloop-root-cause");
    case.passes(&[
        "`ledger-worker-zx4mrndj8b-gdg8j` is crash-looping. Its previous container log ends with:\n```\n2026-11-03T08:33:44.302Z ERROR db: connect ledger-db.payments.svc:5432: pq: password authentication failed for user \"ledger\"\n2026-11-03T08:33:44.302Z FATAL could not initialise ledger store, exiting (exit code 1)\n```",
        "Pod: ledger-worker-zx4mrndj8b-gdg8j\nError: pq: password authentication failed for user \"ledger\"",
        "ledger-worker-zx4mrndj8b-gdg8j cannot authenticate to Postgres: password authentication failed for user \"ledger\".",
        "ledger-worker-zx4mrndj8b-gdg8j never becomes ready: password authentication failed for user \"ledger\".",
    ]);
}

#[test]
fn k8s_crashloop_denied_pod_other_pod_or_other_error_earns_no_reward() {
    let case = Case::load("ops/kubernetes/crashloop-root-cause");
    rejects(
        &case,
        &[
            "The pod is not ledger-worker-zx4mrndj8b-gdg8j; password authentication failed for user \"ledger\".",
            "ledger-worker-c64m9bbw8-v45ss: password authentication failed for user \"ledger\"",
            "ledger-worker-zx4mrndj8b-gdg8j: OOMKilled",
            "ledger-worker-c64m9bbw8-v45ss: panic: invalid config: unknown field \"retryBackoffMs\" in kafka section",
        ],
    );
}

#[test]
fn k8s_crashloop_follows_the_previous_log() {
    let case = Case::load("ops/kubernetes/crashloop-root-cause");
    rejects_after(
        &case,
        "/services/kubernetes/pod_logs/payments~1ledger-worker-zx4mrndj8b-gdg8j~1ledger-worker/previous/2",
        json!(
            "2026-11-03T08:33:44.302Z ERROR db: connect ledger-db.payments.svc:5432: pq: SSL required"
        ),
        &case.reference,
    );
}

// ---------- ops/multi/sentry-release-commit-lookup ----------

#[test]
fn sentry_release_passes_as_prose_or_a_terse_list() {
    let case = Case::load("ops/multi/sentry-release-commit-lookup");
    case.passes(&[
        "Issue STOREFRONT-WEB-7Q first appeared in release storefront@5.2.0. It was introduced by PR #57, authored by Sofia Alvarez.",
        "storefront-web-7q, storefront@5.2.0, pull request 57 by @sofia",
    ]);
}

#[test]
fn sentry_release_denied_issue_other_issue_or_other_author_earns_no_reward() {
    let case = Case::load("ops/multi/sentry-release-commit-lookup");
    rejects(
        &case,
        &[
            "The issue is not STOREFRONT-WEB-7Q; release storefront@5.2.0, PR #57 by Sofia Alvarez.",
            "STOREFRONT-WEB-8D first appeared in storefront@5.3.0; PR #63 by Kenji Watanabe.",
            "STOREFRONT-WEB-7Q first appeared in storefront@5.2.0; PR #57 by Marcus Chen.",
        ],
    );
}

#[test]
fn sentry_release_follows_the_event_counts_and_the_pull_request_author() {
    let case = Case::load("ops/multi/sentry-release-commit-lookup");
    passes_after(
        &case,
        "/services/sentry/issues/STOREFRONT-WEB-8D/count",
        json!("9000"),
        "STOREFRONT-WEB-8D first appeared in storefront@5.3.0; PR #63 by Kenji Watanabe.",
    );
    rejects_after(
        &case,
        "/services/github/pulls/lumen-labs~1storefront#57/user",
        json!("lena"),
        &case.reference,
    );
}

// ---------- ops/pagerduty/payments-oncall-now ----------

#[test]
fn pd_oncall_swapped_levels_or_the_overridden_schedule_earn_no_reward() {
    let case = Case::load("ops/pagerduty/payments-oncall-now");
    rejects(
        &case,
        &[
            "Marcus Chen and Priya Raman.",
            // The scheduled engineer, whom an override replaces.
            "Riya Patel and Marcus Chen.",
        ],
    );
}

// ---------- ops/multi/crashloop-oncall-lookup ----------

#[test]
fn crashloop_oncall_swapped_levels_earn_no_reward() {
    let case = Case::load("ops/multi/crashloop-oncall-lookup");
    rejects(
        &case,
        &[
            "Pod ledger-worker-zx4mrndj8b-gdg8j, error: password authentication failed for user. On call: Marcus Chen, backup Priya Raman.",
        ],
    );
}

// ---------- diagnostic/stripe/no-action-already-refunded ----------

#[test]
fn stripe_refund_fabricated_refund_id_earns_no_reward() {
    let case = Case::load("diagnostic/stripe/no-action-already-refunded");
    rejects(
        &case,
        &["The charge was already refunded as refund re_FAKE123."],
    );
}

// ---------- search/elasticsearch/latency-by-region ----------

#[test]
fn latency_by_region_passes_without_naming_the_ranking() {
    let case = Case::load("search/elasticsearch/latency-by-region");
    case.passes(&["eu-west-1 averages 412.5 ms"]);
}

#[test]
fn latency_by_region_other_region_earns_no_reward() {
    let case = Case::load("search/elasticsearch/latency-by-region");
    rejects(&case, &["ap-southeast-2 averages 412.5 ms"]);
}

// Fails at present: `wins on speed` is not one of the contract's `contradictions`, and
// the reader checks no other predicate about a correctly named value.
#[test]
fn latency_by_region_slowest_region_called_the_fastest_earns_no_reward() {
    let case = Case::load("search/elasticsearch/latency-by-region");
    rejects(&case, &["eu-west-1 wins on speed at 412.5 ms."]);
}

// ---------- search/brave/caterer-saturday-hours ----------

#[test]
fn caterer_other_phone_number_earns_no_reward() {
    let case = Case::load("search/brave/caterer-saturday-hours");
    rejects(&case, &["Phone: 207-555-0198. Saturday: 09:00-14:00."]);
}

// ---------- search/brave/competitor-funding-news ----------

#[test]
fn funding_wrong_round_earns_no_reward() {
    let case = Case::load("search/brave/competitor-funding-news");
    rejects(&case, &["$45M Series A led by Halcyon Ventures."]);
}

// ---------- search/brave/hosting-sla-lookup ----------

#[test]
fn sla_follows_the_published_table() {
    let case = Case::load("search/brave/hosting-sla-lookup");
    let content = "/services/brave/web/https:~1~1www.ironbarkhosting.com~1legal~1sla/content";
    let revised = rewritten(
        &case,
        content,
        "| Business | 99.95% | 10% of the monthly fee below 99.95%",
        "| Business | 99.97% | 15% of the monthly fee below 99.97%",
    );
    passes_after(
        &case,
        content,
        revised,
        "99.97% monthly uptime, 15% service credit.",
    );
}

#[test]
fn sla_denied_or_archived_values_earn_no_reward() {
    let case = Case::load("search/brave/hosting-sla-lookup");
    rejects(
        &case,
        &[
            "It is not 99.95% and the credit is not 10%.",
            "99.9% monthly uptime with a 5% service credit.",
        ],
    );
}

// ---------- search/brave/keynote-recording ----------

#[test]
fn keynote_follows_the_video_duration() {
    let case = Case::load("search/brave/keynote-recording");
    passes_after(
        &case,
        "/services/brave/videos/https:~1~1www.youtube.com~1watch?v=Zt4mQeL1x9c/duration",
        json!("51:03"),
        "Marta Lindqvist, 51:03, https://www.youtube.com/watch?v=Zt4mQeL1x9c",
    );
}

#[test]
fn keynote_denied_speaker_or_other_recordings_earn_no_reward() {
    let case = Case::load("search/brave/keynote-recording");
    rejects(
        &case,
        &[
            "The speaker is not Marta Lindqvist; 47:12, https://www.youtube.com/watch?v=Zt4mQeL1x9c",
            "Tom Ekberg, 39:55, https://vimeo.com/987654321",
            // The highlights cut of the same keynote.
            "Marta Lindqvist, 12:05, https://www.youtube.com/watch?v=r8Kq2PgN7aE",
        ],
    );
}

// ---------- search/elasticsearch/checkout-errors-last-hour ----------

#[test]
fn checkout_errors_other_service_earns_no_reward() {
    let case = Case::load("search/elasticsearch/checkout-errors-last-hour");
    rejects(&case, &["cart-api: 7 errors."]);
}

// ---------- search/elasticsearch/deleted-api-key-audit ----------

#[test]
fn api_key_audit_other_user_earns_no_reward() {
    let case = Case::load("search/elasticsearch/deleted-api-key-audit");
    rejects(&case, &["j.doe deleted the key at 14:05 UTC."]);
}

// ---------- search/elasticsearch/duplicate-charge-lookup ----------

#[test]
fn duplicate_charge_wrong_amount_earns_no_reward() {
    let case = Case::load("search/elasticsearch/duplicate-charge-lookup");
    rejects(&case, &["ORD-109002 and ORD-109003, both $96.40."]);
}

// ---------- search/exa/find-vp-engineering-profile ----------

#[test]
fn vp_profile_former_director_earns_no_reward() {
    let case = Case::load("search/exa/find-vp-engineering-profile");
    rejects(&case, &["https://www.linkedin.com/in/priya-raman-4b21a9"]);
}

// ---------- search/exa/sdk-timeout-default ----------

#[test]
fn sdk_timeout_previous_major_version_option_earns_no_reward() {
    let case = Case::load("search/exa/sdk-timeout-default");
    rejects(&case, &["request_timeout, default 60 seconds"]);
}

// ---------- search/firecrawl/northwind-team-pricing ----------

#[test]
fn team_pricing_follows_the_pricing_page() {
    let case = Case::load("search/firecrawl/northwind-team-pricing");
    let markdown =
        "/services/firecrawl/pages/https:~1~1www.northwind-analytics.com~1pricing/markdown";
    let revised = rewritten(
        &case,
        markdown,
        "- 12 seats included",
        "- 15 seats included",
    );
    passes_after(
        &case,
        markdown,
        revised,
        "$79/month billed annually, 15 seats.",
    );
}

#[test]
fn team_pricing_denied_or_stale_values_earn_no_reward() {
    let case = Case::load("search/firecrawl/northwind-team-pricing");
    rejects(
        &case,
        &[
            "It is not $79 per month and it does not include 12 seats.",
            "$69/month billed annually with 10 seats.",
            "$69/month with 10 seats.",
        ],
    );
}

// ---------- search/multi/api-key-deletion-audit-github ----------

#[test]
fn audit_github_other_user_earns_no_reward() {
    let case = Case::load("search/multi/api-key-deletion-audit-github");
    rejects(
        &case,
        &["j.doe deleted the key at 14:05 UTC. GitHub: jdoe, PR #58."],
    );
}

// ---------- search/multi/competitor-funding-and-pricing ----------

#[test]
fn funding_pricing_passes_with_the_monthly_billing_aside() {
    let case = Case::load("search/multi/competitor-funding-and-pricing");
    case.passes(&[
        "$45M Series B led by Halcyon Ventures. Growth is $890/month billed annually, or $1,090 month to month.",
    ]);
}

#[test]
fn funding_pricing_follows_the_pricing_page() {
    let case = Case::load("search/multi/competitor-funding-and-pricing");
    let markdown = "/services/firecrawl/pages/https:~1~1www.norvikanalytics.com~1pricing/markdown";
    let repriced = rewritten(&case, markdown, "$890/month", "$920/month");
    passes_after(
        &case,
        markdown,
        repriced,
        "$45M Series B led by Halcyon Ventures. Growth is $920/month.",
    );
}

#[test]
fn funding_pricing_denied_wrong_or_missing_values_earn_no_reward() {
    let case = Case::load("search/multi/competitor-funding-and-pricing");
    rejects(
        &case,
        &[
            "$45M Series B led by Halcyon Ventures. Growth is not $890/month.",
            "$12M Series A led by Fjord Capital. Growth is $890/month.",
            "$45M Series B led by Halcyon Ventures.",
            "$45M Series B led by Halcyon Ventures. Growth is $690/month.",
        ],
    );
}

// ---------- search/tavily/vendor-rate-limit ----------

#[test]
fn rate_limit_follows_the_documented_table() {
    let case = Case::load("search/tavily/vendor-rate-limit");
    let raw = "/services/tavily/pages/https:~1~1docs.quillstream.io~1api~1rate-limits/raw_content";
    let revised = rewritten(
        &case,
        raw,
        "| Business | 1,200 | 200 |",
        "| Business | 1,500 | 200 |",
    );
    passes_after(
        &case,
        raw,
        revised,
        "1,500 requests per minute, from https://docs.quillstream.io/api/rate-limits",
    );
}

#[test]
fn rate_limit_denied_or_deprecated_values_earn_no_reward() {
    let case = Case::load("search/tavily/vendor-rate-limit");
    rejects(
        &case,
        &[
            "It is not 1,200 requests per minute (https://docs.quillstream.io/api/rate-limits).",
            "600 requests per minute, from https://docs.quillstream.io/v2/api/rate-limits",
            "600 requests per minute, from https://docs.quillstream.io/api/rate-limits",
        ],
    );
}

// ---------- search/tavily/webhook-signature-sunset ----------

#[test]
fn signature_sunset_follows_the_documented_date() {
    let case = Case::load("search/tavily/webhook-signature-sunset");
    let raw = "/services/tavily/pages/https:~1~1docs.ferrowave.dev~1guides~1webhooks~1verify-signatures/raw_content";
    let revised = rewritten(&case, raw, "January 15, 2027", "March 2, 2027");
    passes_after(
        &case,
        raw,
        revised,
        "2027-03-02; verify Ferrowave-Signature-V2.",
    );
}

#[test]
fn signature_sunset_denied_or_wrong_date_earns_no_reward() {
    let case = Case::load("search/tavily/webhook-signature-sunset");
    rejects(
        &case,
        &[
            "It is not January 15, 2027, and the header is not Ferrowave-Signature-V2.",
            "December 1, 2026; verify Ferrowave-Signature-V2.",
        ],
    );
}
