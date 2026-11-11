use mock_core::clock::{ClockMode, LogicalClock, validate_evidence};
use serde_json::json;

#[test]
fn fixed_transport_does_not_advance_time_but_legacy_is_explicitly_reproducible() {
    let mut clock = LogicalClock::new("2026-03-14T09:00:00Z", 7).unwrap();
    for _ in 0..1000 {
        clock.transport_step(ClockMode::Fixed).unwrap();
    }
    assert_eq!(clock.now_rfc3339(), "2026-03-14T09:00:00Z");
    clock.transport_step(ClockMode::LegacyTransport).unwrap();
    assert_eq!(clock.now_rfc3339(), "2026-03-14T09:00:07Z");
}

#[test]
fn invalid_clock_steps_and_out_of_range_advances_fail_without_changing_time() {
    assert!(LogicalClock::new("2026-03-14T09:00:00Z", -1).is_err());
    let mut clock = LogicalClock::new("2026-03-14T09:00:00Z", i64::MAX).unwrap();
    let original = clock.now_unix();
    assert!(clock.transport_step(ClockMode::LegacyTransport).is_err());
    assert_eq!(clock.now_unix(), original);
    assert!(clock.advance(-1).is_err());
    assert_eq!(clock.now_unix(), original);
}

#[test]
fn clock_evidence_cannot_hide_drift_or_a_mid_trial_policy_change() {
    let initial = json!({
        "now": "2026-03-14T09:00:00Z",
        "clock": {"mode": "fixed", "reference": "2026-03-14T09:00:00Z", "step_secs": 7}
    });
    assert!(validate_evidence(&initial, &initial).is_ok());
    let mut changed = initial.clone();
    changed["now"] = json!("2026-03-14T09:00:07Z");
    assert!(validate_evidence(&initial, &changed).is_err());
    changed["clock"]["mode"] = json!("legacy_transport");
    assert!(validate_evidence(&initial, &changed).is_err());
    assert!(validate_evidence(&json!({"now": "2026-03-14T09:00:00Z"}), &initial).is_err());
    let mut legacy_initial = initial;
    legacy_initial["clock"]["mode"] = json!("legacy_transport");
    assert!(validate_evidence(&legacy_initial, &changed).is_ok());
    changed["now"] = json!("2026-03-14T09:00:08Z");
    assert!(validate_evidence(&legacy_initial, &changed).is_err());
}
