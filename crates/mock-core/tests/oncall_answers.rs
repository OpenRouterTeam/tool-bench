use mock_core::grading::{Expect, GradeInput, Outcome, grade};
use serde_json::{Value, json};

fn state() -> Value {
    json!({"services":{"grafana":{
        "oncall_schedules":{"primary":{
            "id":"primary","name":"Checkout Primary","on_call_now":["sam"]
        }},
        "oncall_users":{
            "sam":{"id":"sam","name":"Sam Brennan","username":"sbrennan","email":"s.brennan@example.test","role":"user"},
            "mei":{"id":"mei","name":"Mei Chen","username":"mchen","email":"m.chen@example.test","role":"user"}
        }
    }}})
}

fn expect() -> Expect {
    serde_json::from_value(json!({
        "version":2,"answer":{"oncall":{
            "schedule":{"collection":"/services/grafana/oncall_schedules","matching":{"name":"Checkout Primary"}},
            "directory":"/services/grafana/oncall_users",
            "allow_unambiguous_surnames":true,
            "requirement":"Identify exactly the current primary assignees without contradicting their duty."
        }}
    })).unwrap()
}

fn outcome(source: &Value, answer: Option<&str>, expectation: &Expect) -> Outcome {
    grade(&GradeInput {
        expect: expectation,
        state: source.clone(),
        initial_state: Some(source),
        journal: &[],
        events: Some(&[]),
        answer,
        tools_ips: vec![],
        context_overflow: false,
        startup: &[],
    })
    .outcome
}

#[test]
fn current_identity_forms_and_structured_native_user_results_are_source_bound() {
    for answer in [
        "sbrennan",
        "Sam Brennan",
        "Brennan",
        "s.brennan@example.test",
        "sbrennan (s.brennan@example.test)",
        "On call: SAM BRENNAN",
        r#"[{"id":"sam","username":"sbrennan","email":"s.brennan@example.test","role":"user"}]"#,
        r#"{"scheduleId":"primary","scheduleName":"Checkout Primary","users":[{"username":"sbrennan"}]}"#,
        // Metadata the directory does not hold is context, not a claim.
        r#"[{"id":"sam","shift":"OCKP0001"}]"#,
        r#"{"scheduleId":"primary","users":[{"id":"sam"}],"checkedAt":"15:30 UTC"}"#,
        r#"["Sam Brennan"]"#,
    ] {
        assert_eq!(
            outcome(&state(), Some(answer), &expect()),
            Outcome::Pass,
            "{answer}"
        );
    }
}

#[test]
fn ordinary_prose_that_names_the_assignee_passes() {
    for answer in [
        "Please invite sbrennan into the incident room.",
        "I acknowledged the alert group, and Sam Brennan is the person on call right now.",
        "**On call now (Checkout Primary): Sam Brennan**\n\n- Email: s.brennan@example.test\n- Nothing was resolved.",
        "The person on call there right now is Sam Brennan (not resolved yet: the staging group).",
        "It looks like Sam Brennan is on call at the moment.",
        "Sam Brennan is on call. Mei Chen is not on the primary schedule.",
        "Sam Brennan, not Mei Chen.",
        "The rotation is Sam Brennan and Mei Chen; Sam Brennan is on call right now.",
        "@sbrennan (Sam Brennan) is currently on call for Checkout Primary.",
        "On-call (Checkout Primary): Sam Brennan <s.brennan@example.test>",
        "I don't see anyone other than Sam Brennan on the Checkout Primary schedule right now.",
        "Nobody else is on call; it's just Sam Brennan.",
        "| Schedule | On call now |\n|---|---|\n| Checkout Primary | Sam Brennan |",
        "```json\n{\"users\": [{\"id\": \"sam\", \"username\": \"sbrennan\"}]}\n```",
        r#"{"on_call": "Sam Brennan", "schedule": "Checkout Primary"}"#,
    ] {
        assert_eq!(
            outcome(&state(), Some(answer), &expect()),
            Outcome::Pass,
            "{answer}"
        );
    }
}

#[test]
fn people_on_another_schedule_are_context_only_where_they_really_are() {
    let mut source = state();
    source["services"]["grafana"]["oncall_schedules"]["secondary"] = json!({
        "id":"secondary","name":"Checkout Secondary","on_call_now":["mei"]
    });
    assert_eq!(
        outcome(
            &source,
            Some("Mei Chen covers the secondary schedule. Sam Brennan is on call for the primary."),
            &expect()
        ),
        Outcome::Pass
    );
    // Without that schedule the same sentence presents Mei Chen first.
    assert_eq!(
        outcome(
            &state(),
            Some("Mei Chen covers the secondary schedule. Sam Brennan is on call for the primary."),
            &expect()
        ),
        Outcome::Fail
    );
}

#[test]
fn a_name_mention_does_not_make_negated_duty_or_the_wrong_schedule_correct() {
    for answer in [
        "Brennan is not on call. The primary on-call person is Mei Chen.",
        "sbrennan is not on call.",
        "Mei Chen is on call right now.",
        "Mei Chen is on call. Sam Brennan is on the rota next week.",
        "I couldn't determine who is on call right now.",
        r#"[{"username":"mchen"}]"#,
        r#"{"scheduleId":"secondary","users":[{"username":"sbrennan"}]}"#,
        r#"[{"id":"sam","email":"m.chen@example.test"}]"#,
        r#"[{"id":"sam"},{"id":"sam"}]"#,
        "[]",
    ] {
        assert_eq!(
            outcome(&state(), Some(answer), &expect()),
            Outcome::Fail,
            "{answer}"
        );
    }
    // An extra assignee joined to the right one, or a statement that is both
    // made and taken back, cannot be decided.
    for answer in [
        "Sam Brennan and Mei Chen are on call.",
        "Sam Brennan is on call. Correction: Sam Brennan is not on call.",
        "I would not page Sam Brennan.",
        "Do not contact Sam Brennan; he isn't on call. Mei Chen is.",
    ] {
        assert_eq!(
            outcome(&state(), Some(answer), &expect()),
            Outcome::Unresolved,
            "{answer}"
        );
    }
}

#[test]
fn changed_current_people_are_not_graded_against_a_canned_brennan_answer() {
    let mut source = state();
    source["services"]["grafana"]["oncall_schedules"]["primary"]["on_call_now"] = json!(["mei"]);
    assert_eq!(outcome(&source, Some("Mei Chen"), &expect()), Outcome::Pass);
    assert_eq!(outcome(&source, Some("sbrennan"), &expect()), Outcome::Fail);
    assert_eq!(
        outcome(&source, Some(r#"[{"id":"sam"}]"#), &expect()),
        Outcome::Fail
    );
    assert_eq!(
        outcome(
            &source,
            Some("Brennan is not on call. The primary on-call person is Mei Chen."),
            &expect()
        ),
        Outcome::Pass
    );
}

#[test]
fn multiple_assignees_require_the_actual_complete_set_without_prescribing_order() {
    let mut source = state();
    source["services"]["grafana"]["oncall_schedules"]["primary"]["on_call_now"] =
        json!(["sam", "mei"]);
    for answer in [
        "Mei Chen, Sam Brennan",
        "sbrennan and mchen",
        r#"[{"id":"mei"},{"id":"sam"}]"#,
    ] {
        assert_eq!(
            outcome(&source, Some(answer), &expect()),
            Outcome::Pass,
            "{answer}"
        );
    }
    assert_eq!(
        outcome(&source, Some(r#"[{"id":"sam"}]"#), &expect()),
        Outcome::Fail
    );
}

#[test]
fn shared_and_partial_names_require_review_not_a_guessed_verdict() {
    let mut source = state();
    source["services"]["grafana"]["oncall_users"]["another"] = json!({
        "id":"another","name":"Taylor Brennan","username":"tbrennan","email":"t.brennan@example.test"
    });
    assert_eq!(
        outcome(&source, Some("Brennan"), &expect()),
        Outcome::Unresolved
    );
    assert_eq!(
        outcome(&source, Some("Sam Brennan"), &expect()),
        Outcome::Pass
    );
    assert_eq!(
        outcome(&source, Some("Sam is on call right now."), &expect()),
        Outcome::Unresolved
    );
}

#[test]
fn an_empty_native_user_list_does_not_override_a_nonempty_authoritative_schedule() {
    let mut source = state();
    assert_eq!(outcome(&source, Some("[]"), &expect()), Outcome::Fail);
    source["services"]["grafana"]["oncall_schedules"]["primary"]["on_call_now"] = json!([]);
    assert_eq!(outcome(&source, Some("[]"), &expect()), Outcome::Pass);
    assert_eq!(
        outcome(
            &source,
            Some("Nobody is on call on Checkout Primary."),
            &expect()
        ),
        Outcome::Pass
    );
    assert_eq!(
        outcome(
            &source,
            Some("The schedule returned no on-call users for the current shift."),
            &expect()
        ),
        Outcome::Pass
    );
    assert_eq!(
        outcome(&source, Some("Sam Brennan is on call."), &expect()),
        Outcome::Fail
    );
    assert_eq!(
        outcome(&source, Some("I checked the schedule."), &expect()),
        Outcome::Unresolved
    );
    assert_eq!(
        outcome(&source, Some(r#"[{"id":"sam"}]"#), &expect()),
        Outcome::Fail
    );
}

#[test]
fn malformed_or_dangling_current_duty_is_missing_evidence_not_a_positive_answer() {
    for duty in [json!(["missing"]), json!(["sam", "sam"]), json!("sam")] {
        let mut source = state();
        source["services"]["grafana"]["oncall_schedules"]["primary"]["on_call_now"] = duty;
        assert_ne!(outcome(&source, Some("sbrennan"), &expect()), Outcome::Pass);
    }
}

#[test]
fn unfamiliar_structured_metadata_does_not_conceal_known_identity_or_cardinality_failures() {
    for answer in [
        r#"{"scheduleId":"secondary","users":[{"id":"sam"}],"extra":"context"}"#,
        r#"{"scheduleId":"primary","users":[{"id":"mei"}],"extra":"context"}"#,
        r#"[{"id":"sam","extra":"context","username":"mchen"}]"#,
        r#"[{"id":"sam","extra":"context"},{"id":"mei"}]"#,
        r#"[{"id":"sam","extra":"context"},{"id":"sam"}]"#,
    ] {
        assert_eq!(
            outcome(&state(), Some(answer), &expect()),
            Outcome::Fail,
            "{answer}"
        );
    }
}
