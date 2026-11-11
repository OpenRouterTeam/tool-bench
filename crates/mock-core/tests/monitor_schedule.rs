use chrono::DateTime;
use mock_core::monitor_schedule::{MonitorSchedule, ScheduleError};
use serde_json::json;

fn instant(source: &str) -> i64 {
    DateTime::parse_from_rfc3339(source).unwrap().timestamp()
}

fn schedule(expression: &str, timezone: &str) -> MonitorSchedule {
    MonitorSchedule::parse(&json!({"cron":expression,"timezone":timezone})).unwrap()
}

#[test]
fn daily_text_forms_preserve_their_actual_local_time() {
    for (text, cron) in [
        ("daily", "0 0 * * *"),
        (" Every day. ", "0 0 * * *"),
        ("daily at 9am", "0 9 * * *"),
        ("daily at 5:30 PM", "30 17 * * *"),
        ("daily at 12am", "0 0 * * *"),
        ("daily at 12pm", "0 12 * * *"),
        ("every 24 hours", "0 */24 * * *"),
    ] {
        let parsed = MonitorSchedule::parse(&json!({"text":text})).unwrap();
        assert_eq!(parsed.normalized(), json!({"cron":cron,"timezone":"UTC"}));
        assert!(parsed.is_daily().unwrap());
    }
}

#[test]
fn text_schedules_are_anchored_and_validate_time_and_divisibility() {
    for text in [
        "every weekday",
        "nightly",
        "daily except weekends",
        "daily at 24:00",
        "daily at 0am",
        "daily at 9:5",
        "every 7 hours",
        "every 7 minutes",
        "every 0 minutes",
        "every 25 hours",
        "hourly at :60",
    ] {
        assert!(
            matches!(
                MonitorSchedule::parse(&json!({"text":text})),
                Err(ScheduleError::Invalid(_))
            ),
            "{text}"
        );
    }
}

#[test]
fn non_daily_text_forms_are_not_duration_guesses() {
    for (text, cron) in [
        ("hourly at :15", "15 * * * *"),
        ("every 6 hours", "0 */6 * * *"),
        ("every 15 minutes starting at :5", "5-59/15 * * * *"),
        ("weekly", "0 0 * * 0"),
    ] {
        let parsed = MonitorSchedule::parse(&json!({"text":text})).unwrap();
        assert_eq!(parsed.normalized()["cron"], cron);
        assert!(!parsed.is_daily().unwrap());
    }
}

#[test]
fn daily_is_semantic_not_one_cron_spelling_or_sampled_gap() {
    for expression in [
        "0 9 * * *",
        "0,0 9,9 1-31 1-12 0-7",
        "0/5 9 * * *",
        "0 9 */1 * */1",
        "0 9 *,1 * *",
    ] {
        assert!(
            schedule(expression, "UTC").is_daily().unwrap(),
            "{expression}"
        );
    }
    for expression in [
        "0 0,12 * * *",
        "0 9 * * 1-5",
        "0 9 1-31 * 1",
        "0,1 9 * * *",
        "0 9 * 1-11 *",
    ] {
        assert!(
            !schedule(expression, "UTC").is_daily().unwrap(),
            "{expression}"
        );
    }
}

#[test]
fn numeric_singleton_steps_do_not_expand_to_the_field_maximum() {
    let parsed = schedule("5/10 9 * * *", "UTC");
    assert!(parsed.is_daily().unwrap());
    assert_eq!(
        parsed.next_after(instant("2026-11-07T09:06:00Z")).unwrap(),
        instant("2026-11-08T09:05:00Z")
    );
}

#[test]
fn day_of_month_and_weekday_intersect() {
    let parsed = schedule("0 9 13 * 5", "UTC");
    assert_eq!(
        parsed.next_after(instant("2026-11-07T00:00:00Z")).unwrap(),
        instant("2026-11-13T09:00:00Z")
    );
    assert_eq!(
        schedule("0 9 1-31 * 1", "UTC")
            .next_after(instant("2026-11-07T00:00:00Z"))
            .unwrap(),
        instant("2026-11-09T09:00:00Z")
    );
}

#[test]
fn sunday_aliases_ranges_and_steps_keep_their_meaning() {
    for expression in ["0 9 * * 0", "0 9 * * 7", "0 9 * * 7/2"] {
        assert_eq!(
            schedule(expression, "UTC")
                .next_after(instant("2026-11-07T00:00:00Z"))
                .unwrap(),
            instant("2026-11-08T09:00:00Z"),
            "{expression}"
        );
    }
    assert_eq!(
        schedule("0 9 * * 6-7", "UTC")
            .next_after(instant("2026-11-02T00:00:00Z"))
            .unwrap(),
        instant("2026-11-07T09:00:00Z")
    );
}

#[test]
fn spring_gaps_skip_missing_wall_times_and_folds_keep_both_instants() {
    let gap = schedule("30 2 * * *", "America/New_York");
    assert_eq!(
        gap.next_after(instant("2027-03-13T12:00:00Z")).unwrap(),
        instant("2027-03-15T06:30:00Z")
    );
    let fold = schedule("30 1 * * *", "America/New_York");
    let first = fold.next_after(instant("2027-11-07T04:00:00Z")).unwrap();
    assert_eq!(first, instant("2027-11-07T05:30:00Z"));
    assert_eq!(
        fold.next_after(first).unwrap(),
        instant("2027-11-07T06:30:00Z")
    );
    assert!(fold.is_daily().unwrap());
}

#[test]
fn half_hour_and_two_hour_transitions_are_not_one_hour_assumptions() {
    let half_hour = schedule("45 1 * * *", "Australia/Lord_Howe");
    let first = half_hour
        .next_after(instant("2027-04-03T13:00:00Z"))
        .unwrap();
    assert_eq!(first, instant("2027-04-03T14:45:00Z"));
    assert_eq!(
        half_hour.next_after(first).unwrap(),
        instant("2027-04-03T15:15:00Z")
    );
    let two_hour = schedule("30 1 * * *", "Antarctica/Troll");
    let first = two_hour
        .next_after(instant("2027-10-30T22:00:00Z"))
        .unwrap();
    assert_eq!(first, instant("2027-10-30T23:30:00Z"));
    assert_eq!(
        two_hour.next_after(first).unwrap(),
        instant("2027-10-31T01:30:00Z")
    );
}

#[test]
fn next_run_is_strictly_after_and_uses_the_explicit_reference() {
    let parsed = schedule("0 9 * * *", "utc");
    assert_eq!(
        parsed.next_after(instant("2026-11-07T09:00:00Z")).unwrap(),
        instant("2026-11-08T09:00:00Z")
    );
    assert_eq!(
        parsed
            .next_rfc3339(instant("2026-11-07T08:59:59Z"))
            .unwrap(),
        "2026-11-07T09:00:00.000Z"
    );
    assert_eq!(schedule("*/5 * * * *", "UTC").next_after(-1).unwrap(), 0);
}

#[test]
fn sampled_minimum_is_distinct_from_business_daily_recurrence() {
    let parsed = schedule("0,1 9 * * *", "UTC");
    assert!(parsed.validate_at(instant("2026-11-07T09:00:30Z")).is_ok());
    assert!(matches!(
        parsed.validate_at(instant("2026-11-07T08:59:30Z")),
        Err(ScheduleError::Invalid(_))
    ));
    assert!(!parsed.is_daily().unwrap());
}

#[test]
fn impossible_and_sparse_dates_do_not_gain_an_unbounded_search() {
    assert!(
        schedule("0 0 30 2 *", "UTC")
            .validate_at(instant("2026-11-07T00:00:00Z"))
            .is_err()
    );
    assert!(
        schedule("0 0 29 2 *", "UTC")
            .validate_at(instant("2027-11-07T00:00:00Z"))
            .is_err()
    );
}

#[test]
fn unknown_schema_fields_and_invalid_numeric_fields_are_not_ignored() {
    for value in [
        json!({"text":"daily","cron":"0 0 * * *"}),
        json!({"text":"daily","intervalMinutes":1440}),
        json!({"text":"daily","timezone":null}),
        json!({"text":"daily","timezone":"Mars/Olympus_Mons"}),
        json!({"cron":"60 9 * * *"}),
        json!({"cron":"0 24 * * *"}),
        json!({"cron":"0 9 0 * *"}),
        json!({"cron":"0 9 * 13 *"}),
        json!({"cron":"0 9 * * 8"}),
        json!({"cron":"*/0 * * * *"}),
    ] {
        assert!(
            matches!(
                MonitorSchedule::parse(&value),
                Err(ScheduleError::Invalid(_))
            ),
            "{value}"
        );
    }
}

#[test]
fn unqualified_publisher_quirks_are_not_silently_reinterpreted() {
    for expression in [
        "0,,1 9 * * *",
        "0/1000 9 * * *",
        "0x0 9 * * *",
        "0/1/2 9 * * *",
    ] {
        assert!(
            matches!(
                MonitorSchedule::parse(&json!({"cron":expression})),
                Err(ScheduleError::Unsupported(_))
            ),
            "{expression}"
        );
    }
}

#[test]
fn legacy_fixture_normalization_preserves_specific_authored_daily_times() {
    let parsed = MonitorSchedule::from_fixture(
        &json!({"text":"daily","timezone":"Europe/Berlin","intervalMinutes":1440}),
        Some("2026-10-30T06:00:00Z"),
    )
    .unwrap();
    assert_eq!(
        parsed.normalized(),
        json!({"cron":"0 7 * * *","timezone":"Europe/Berlin"})
    );
    assert!(parsed.is_daily().unwrap());
    assert!(MonitorSchedule::parse(&json!({"text":"daily","intervalMinutes":1440})).is_err());
}

#[test]
fn normalization_does_not_rewrite_an_explicit_fixture_time_from_stale_metadata() {
    let parsed = MonitorSchedule::from_fixture(
        &json!({"text":"daily at 9am","timezone":"UTC","intervalMinutes":1440}),
        Some("2026-10-30T06:00:00Z"),
    )
    .unwrap();
    assert_eq!(parsed.normalized()["cron"], "0 9 * * *");
}
