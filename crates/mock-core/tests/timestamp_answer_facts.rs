mod statement_support;

use mock_core::grading::Outcome;
use serde_json::json;
use statement_support::Case;

const SCENARIO: &str = "diagnostic/stripe/no-action-already-refunded";
const CREATED: &str = "/services/stripe/refunds/re_6AYez5LT1vd0g6zTWhpyAP2b/created";
const ANSWER: &str = "The duplicate charge was already refunded in full on March 5 as refund re_6AYez5LT1vd0g6zTWhpyAP2b.";

fn case(kind: &str) -> Case {
    let mut case = Case::load(SCENARIO);
    let facts = case
        .expect
        .answer
        .as_mut()
        .unwrap()
        .record_facts
        .as_mut()
        .unwrap();
    facts.facts.insert(
        "refund_date".into(),
        serde_json::from_value(json!({
            "record": "refund", "field": "/created", "kind": kind,
        }))
        .unwrap(),
    );
    case
}

fn clock_case() -> Case {
    let mut case = Case::load(SCENARIO);
    let facts = case
        .expect
        .answer
        .as_mut()
        .unwrap()
        .record_facts
        .as_mut()
        .unwrap();
    facts.facts.remove("refund_date");
    facts.facts.insert(
        "when".into(),
        serde_json::from_value(json!({
            "record": "refund", "field": "/created", "kind": "text", "clock": true,
        }))
        .unwrap(),
    );
    case
}

#[test]
fn unix_seconds_bind_prose_and_iso_dates_without_freezing_the_authored_day() {
    let case = case("utc_date");
    case.passes(&[
        ANSWER,
        "The duplicate charge was already refunded in full on 2026-03-05 as refund re_6AYez5LT1vd0g6zTWhpyAP2b.",
    ]);
    let mut changed = case.initial.clone();
    *changed.pointer_mut(CREATED).unwrap() = json!(1772813520);
    assert_ne!(case.grade_with(&changed, ANSWER).outcome, Outcome::Pass);
    assert_eq!(
        case.grade_with(
            &changed,
            "The duplicate charge was already refunded in full on March 6 as refund re_6AYez5LT1vd0g6zTWhpyAP2b.",
        )
        .outcome,
        Outcome::Pass,
    );
}

#[test]
fn timestamp_offsets_are_resolved_before_selecting_the_utc_calendar_day() {
    let mut case = case("utc_date");
    *case.initial.pointer_mut(CREATED).unwrap() = json!("2026-03-06T01:00:00+14:00");
    case.passes(&[ANSWER]);
    case.fails(&[
        "The duplicate charge was already refunded in full on March 6 as refund re_6AYez5LT1vd0g6zTWhpyAP2b.",
    ]);
}

#[test]
fn impossible_dates_and_unsupported_timestamps_are_environment_errors() {
    for value in [
        json!("2026-02-31T12:00:00Z"),
        json!("2026-03-05T99:00:00Z"),
        json!("not a timestamp"),
        json!(1772727120.5),
        json!(1772727120000_i64),
    ] {
        let mut case = case("utc_date");
        *case.initial.pointer_mut(CREATED).unwrap() = value;
        assert_eq!(case.outcome(ANSWER), Outcome::EnvironmentError);
    }
    for value in ["2026-02-31", "February 29, 2026", "April 31, 2026"] {
        let mut case = case("date");
        *case.initial.pointer_mut(CREATED).unwrap() = json!(value);
        assert_eq!(case.outcome(ANSWER), Outcome::EnvironmentError);
    }
}

#[test]
fn valid_leap_dates_are_not_rejected_by_calendar_validation() {
    let mut case = case("date");
    *case.initial.pointer_mut(CREATED).unwrap() = json!("2028-02-29");
    case.passes(&[
        "The duplicate charge was already refunded in full on February 29, 2028 as refund re_6AYez5LT1vd0g6zTWhpyAP2b.",
    ]);
}

#[test]
fn clock_facts_resolve_offsets_before_rendering_utc_minutes() {
    let mut case = clock_case();
    let answer =
        "The duplicate charge was already refunded as refund re_6AYez5LT1vd0g6zTWhpyAP2b at 16:12.";
    case.passes(&[answer]);
    *case.initial.pointer_mut(CREATED).unwrap() = json!("2026-03-06T06:12:00+14:00");
    case.passes(&[answer]);
}

#[test]
fn clock_facts_accept_full_source_bound_precision() {
    let case = clock_case();
    case.passes(&[
        "The duplicate charge was already refunded as refund re_6AYez5LT1vd0g6zTWhpyAP2b at 16:12:00.",
        "The duplicate charge was already refunded as refund re_6AYez5LT1vd0g6zTWhpyAP2b at 2026-03-05T16:12:00Z.",
        "The duplicate charge was already refunded as refund re_6AYez5LT1vd0g6zTWhpyAP2b at 2026-03-05T16:12:00+00:00.",
    ]);
}

#[test]
fn clock_facts_follow_the_source_seconds() {
    let mut case = clock_case();
    *case.initial.pointer_mut(CREATED).unwrap() = json!(1772727121);
    // The minute form is also found inside a time with other seconds (`16:12:00`), so
    // stale seconds pass; see `stale-seconds` in the wrong-answer fixtures.
    case.passes(&[
        "The duplicate charge was already refunded as refund re_6AYez5LT1vd0g6zTWhpyAP2b at 16:12.",
        "The duplicate charge was already refunded as refund re_6AYez5LT1vd0g6zTWhpyAP2b at 16:12:01.",
        "The duplicate charge was already refunded as refund re_6AYez5LT1vd0g6zTWhpyAP2b at 2026-03-05T16:12:01Z.",
    ]);
}

#[test]
fn clock_facts_do_not_accept_an_impossible_date_or_unsupported_epoch() {
    for value in [
        json!("2026-02-31T16:12:00Z"),
        json!("2026-03-05T16:12:99Z"),
        json!(i64::MAX),
    ] {
        let mut case = clock_case();
        *case.initial.pointer_mut(CREATED).unwrap() = value;
        assert_eq!(
            case.outcome("The duplicate charge was already refunded as refund re_6AYez5LT1vd0g6zTWhpyAP2b at 16:12."),
            Outcome::EnvironmentError,
        );
    }
}
