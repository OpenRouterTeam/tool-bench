//! Truthful replies keep passing when the authored Datadog source changes.
//!
//! Full timestamps and the staging comparison are optional context: they never decide
//! the verdict, so only replies that state them truthfully are covered here.

mod statement_support;

use serde_json::json;
use statement_support::Case;

const SCENARIO: &str = "ops/datadog/checkout-p95-peak";
const PRODUCTION_TIME: &str =
    "/services/datadog/metrics/checkout.api.latency.p95{env:prod,service:checkout-api}/points/19/0";
const STAGING_PEAK: &str = "/services/datadog/metrics/checkout.api.latency.p95{env:staging,service:checkout-api}/points/15/1";

#[test]
fn truthful_timestamp_precision_and_the_requested_minute_notation_still_pass() {
    let mut case = Case::load(SCENARIO);
    *case.initial.pointer_mut(PRODUCTION_TIME).unwrap() = json!(1793284501);
    case.passes(&[
        "Peak: 1843 ms at 14:35 UTC.",
        "The highest p95 was 1.843 s, reached at 2026-10-29T14:35:01Z.",
        "1843 milliseconds at 14:35:01 UTC",
    ]);
}

#[test]
fn a_truthful_staging_comparison_passes_after_the_staging_peak_changes() {
    let mut case = Case::load(SCENARIO);
    *case.initial.pointer_mut(STAGING_PEAK).unwrap() = json!(2501);
    case.passes(&[
        "Peak: 1843 ms at 14:35 UTC.",
        "1843 ms at 14:35 UTC, not the 2501 ms staging peak.",
        "1843 ms at 14:35 UTC, not the 2.501 s staging peak.",
    ]);
}
