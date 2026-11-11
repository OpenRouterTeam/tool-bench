//! Regression probes for improvement plan item 11: unsupported paraphrases
//! must reach adjudication (Unresolved), not an unjustified Fail.
//!
//! Each test contains:
//! - **wrong controls** that must remain Fail (canonical wrong values),
//! - **correct-but-unrecognized** answers that are factually correct yet
//!   use a notation the reader cannot classify, and should therefore be
//!   Unresolved (needs adjudication) rather than Fail.
//!
//! The original defect: when `affirmed == 0` and no `unreviewed` heuristic fired,
//! the reader falls straight to `Failed` ("answer does not state the source
//! {name}") or `Failed` ("answer states a different value …").  A correct
//! value expressed in an unconvertible unit — compound-rate denominators,
//! unsupported time dimensions, or decimal-vs-percentage notation — triggers
//! neither the `has_number_words` nor the `notes.unlabeled` safety net,
//! because the number IS present as a digit token but with a different
//! magnitude than the target.
//!
//! Wrong-value controls remain alongside the preserved adjudication cases.

mod statement_support;

use mock_core::grading::Outcome;
use statement_support::Case;

// ── 1. Compound-rate unit conversion ───────────────────────────────────
//
// vendor-rate-limit: source = 1,200 requests/min, require_unit = true,
// units = [requests, req, rpm, /min, /minute, per minute, per min, a minute].
//
// "20 requests per second" = 1,200 req/min.  The reader finds token 20
// with reviewed unit "requests", computes 20 ≠ 1,200, marks a unit-conflict
// → Fail.  The rate denominator "per second" is invisible to the number
// reader.  Because the reader cannot verify the conversion, adjudication
// (Unresolved) is the correct conservative verdict.

#[test]
fn compound_rate_conversion_per_second_to_per_minute() {
    let case = Case::load("search/tavily/vendor-rate-limit");
    case.reference_is_source_bound();

    // Wrong controls: genuinely wrong rates or wrong sources.
    case.fails(&[
        "600 requests per minute (https://docs.quillstream.io/api/rate-limits)",
        "900 requests per minute (https://docs.quillstream.io/api/rate-limits)",
    ]);

    // Correct value in contracted unit.
    case.passes(&["1,200 requests per minute (https://docs.quillstream.io/api/rate-limits)"]);

    // 20 req/s = 1,200 req/min — correct value, unconvertible notation.
    let answer = "20 requests per second (https://docs.quillstream.io/api/rate-limits)";
    assert_ne!(
        case.outcome(answer),
        Outcome::Pass,
        "compound rate conversion is not a reviewed form; Pass would be wrong"
    );
    assert_eq!(
        case.outcome(answer),
        Outcome::Unresolved,
        "20 req/s = 1200 req/min is correct; the reader cannot verify the \
         compound-rate conversion, so adjudication is the right outcome"
    );
}

// ── 2. Unsupported time-dimension unit (microseconds) ──────────────────
//
// latency-by-region: source = 412.5 ms, require_unit = true,
// units = [ms, milliseconds, millisecond, msec].
//
// dimension() covers ms ↔ s ↔ min ↔ h.  Microseconds (μs) are absent.
// "412500 microseconds" = 412.5 ms.  The reader sees 412500 with unknown
// unit "microseconds", value ≠ 412.5, no unit match → Fail.

#[test]
fn microseconds_to_milliseconds_dimension_gap() {
    let case = Case::load("search/elasticsearch/latency-by-region");
    case.reference_is_source_bound();

    // Wrong controls: another region's latency, wrong value.
    case.fails(&[
        "Slowest region: us-east-1, average 210 ms for successful /checkout/submit requests on 2026-10-30",
        "Slowest region: eu-west-1, average 999 ms for successful /checkout/submit requests on 2026-10-30",
    ]);

    // Correct in reviewed units.
    case.passes(&[
        "Slowest region: eu-west-1, average 412.5 ms for successful /checkout/submit requests on 2026-10-30",
    ]);

    // Supported dimension conversion (ms → s) passes.
    case.passes(&[
        "Slowest region: eu-west-1, average 0.4125 seconds for successful /checkout/submit requests on 2026-10-30",
    ]);

    // Unsupported dimension: 412500 μs = 412.5 ms.
    let answer = "Slowest region: eu-west-1, average 412500 microseconds for successful /checkout/submit requests on 2026-10-30";
    assert_ne!(
        case.outcome(answer),
        Outcome::Pass,
        "microseconds is not a reviewed unit; Pass would be wrong"
    );
    assert_eq!(
        case.outcome(answer),
        Outcome::Unresolved,
        "412500 μs = 412.5 ms is correct; the reader cannot convert \
         microseconds, so adjudication is the right outcome"
    );
}

// ── 3. Decimal fraction for a percentage ───────────────────────────────
//
// hosting-sla-lookup: uptime = 99.95%, require_unit = true,
// units = ["%", "percent", "pct"].
//
// "0.9995 uptime" is the decimal-fraction equivalent of 99.95%.
// dimension() maps "%" → (ratio, 1/100), but the answer carries no unit
// to trigger conversion.  The reader sees 0.9995 with no unit; 0.9995 ≠
// 99.95 → Fail.

#[test]
fn decimal_fraction_for_percentage() {
    let case = Case::load("search/brave/hosting-sla-lookup");
    case.reference_is_source_bound();

    // Wrong controls: the archived SLA's values.
    case.fails(&["Uptime: 99.9% Service credit: 5% of the monthly fee"]);

    // Correct with reviewed unit.
    case.passes(&["Uptime: 99.95% Service credit: 10% of the monthly fee"]);

    // Decimal fraction: 0.9995 = 99.95%.
    let answer =
        "The Business plan commits to 0.9995 uptime; the credit is 10% of the monthly fee.";
    assert_ne!(
        case.outcome(answer),
        Outcome::Pass,
        "bare decimal without % is not a reviewed form; Pass would be wrong"
    );
    assert_eq!(
        case.outcome(answer),
        Outcome::Unresolved,
        "0.9995 = 99.95%; the reader cannot verify decimal-to-percentage \
         conversion, so adjudication is the right outcome"
    );
}
