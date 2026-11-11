use mock_core::clock::{format_rfc3339_nanos, parse_rfc3339_nanos};

#[test]
fn explicit_offsets_and_zero_fraction_spellings_compare_as_the_same_exact_instant() {
    let expected = parse_rfc3339_nanos("2026-11-13T02:00:00Z").unwrap();
    for value in [
        "2026-11-13T02:00:00.000000000Z",
        "2026-11-13T03:00:00+01:00",
        "2026-11-12T20:30:00-05:30",
    ] {
        assert_eq!(parse_rfc3339_nanos(value), Some(expected));
    }
    assert_eq!(
        format_rfc3339_nanos(expected).unwrap(),
        "2026-11-13T02:00:00Z"
    );
}

#[test]
fn nanosecond_differences_are_not_rounded_into_an_exact_requested_window() {
    let start = parse_rfc3339_nanos("2026-11-13T02:00:00Z").unwrap();
    let later = parse_rfc3339_nanos("2026-11-13T02:00:00.000000001Z").unwrap();
    assert_eq!(later - start, 1);
    assert_eq!(
        format_rfc3339_nanos(later).unwrap(),
        "2026-11-13T02:00:00.000000001Z"
    );
}

#[test]
fn invalid_calendar_fields_delimiters_offsets_and_precision_are_not_timestamps() {
    for value in [
        "2026-11-13Q02:00:00Z",
        "2026-11-13T02x00y00Z",
        "2026-02-29T02:00:00Z",
        "2026-13-13T02:00:00Z",
        "2026-11-31T02:00:00Z",
        "2026-11-13T25:00:00Z",
        "2026-11-13T02:60:00Z",
        "2026-11-13T02:00:60Z",
        "2026-11-13T02:00:00+25:00",
        "2026-11-13T02:00:00+01:60",
        "2026-11-13T02:00:00+0100",
        "2026-11-13T02:00:00",
        "2026-11-13T02:00:00.Z",
        "2026-11-13T02:00:00.0000000001Z",
    ] {
        assert!(parse_rfc3339_nanos(value).is_none(), "{value}");
    }
    assert!(parse_rfc3339_nanos("2024-02-29T02:00:00Z").is_some());
}
