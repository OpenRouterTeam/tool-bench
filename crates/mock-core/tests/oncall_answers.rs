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
            "empty_answers":["Nobody is on call on Checkout Primary."],
            "content":{
                "requirement":"Identify exactly the current primary assignees without contradicting their duty.",
                "accepted":["${oncall}","On call: ${oncall}","${oncall} is on call."],
                "rejected":["${oncall} is not on call.","${oncall} is not on call. The primary on-call person is Mei Chen."]
            }
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
    ] {
        assert_eq!(
            outcome(&state(), Some(answer), &expect()),
            Outcome::Pass,
            "{answer}"
        );
    }
}

#[test]
fn a_name_mention_does_not_make_negated_duty_or_the_wrong_schedule_correct() {
    for answer in [
        "Brennan is not on call. The primary on-call person is Mei Chen.",
        "sbrennan is not on call.",
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
}

#[test]
fn changed_current_people_are_not_graded_against_a_canned_brennan_answer() {
    let mut source = state();
    source["services"]["grafana"]["oncall_schedules"]["primary"]["on_call_now"] = json!(["mei"]);
    assert_eq!(outcome(&source, Some("Mei Chen"), &expect()), Outcome::Pass);
    assert_eq!(
        outcome(&source, Some("sbrennan"), &expect()),
        Outcome::Unresolved
    );
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
        Outcome::Unresolved
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
fn ambiguous_names_and_unreviewed_wording_require_review_not_a_guessed_verdict() {
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
        outcome(
            &source,
            Some("Please invite sbrennan into the incident room."),
            &expect()
        ),
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
        r#"{"scheduleId":"secondary","users":[{"id":"sam"}],"extra":"unreviewed"}"#,
        r#"{"scheduleId":"primary","users":[{"id":"mei"}],"extra":"unreviewed"}"#,
        r#"[{"id":"sam","unreviewed":"metadata","username":"mchen"}]"#,
        r#"[{"id":"sam","unreviewed":"metadata"},{"id":"mei"}]"#,
        r#"[{"id":"sam","unreviewed":"metadata"},{"id":"sam"}]"#,
    ] {
        assert_eq!(
            outcome(&state(), Some(answer), &expect()),
            Outcome::Fail,
            "{answer}"
        );
    }
    assert_eq!(
        outcome(
            &state(),
            Some(r#"[{"id":"sam","unreviewed":"metadata"}]"#),
            &expect()
        ),
        Outcome::Unresolved
    );
}
