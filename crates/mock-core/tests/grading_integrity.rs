use mock_core::{
    grading::{Expect, GradeInput, Outcome, grade},
    journal::JournalEntry,
    startup::StartupCall,
};
use serde_json::{Value, json};

const TOOLS_IP: &str = "192.0.2.10";

fn request(source: &str, status: u16, response_body: Value) -> JournalEntry {
    JournalEntry {
        seq: 1,
        operation_id: None,
        replay: None,
        time: "2026-11-03T15:00:00Z".into(),
        service: "github".into(),
        source: source.into(),
        method: "GET".into(),
        host: None,
        scheme: "http".into(),
        content_type: None,
        path: "/api/v3/user".into(),
        query: None,
        request_body: Value::Null,
        status,
        response_body,
    }
}

fn expectation(value: Value) -> Expect {
    serde_json::from_value(value).unwrap()
}

fn input<'a>(
    expect: &'a Expect,
    journal: &'a [JournalEntry],
    answer: Option<&'a str>,
) -> GradeInput<'a> {
    GradeInput {
        expect,
        state: json!({"services": {}}),
        journal,
        answer,
        tools_ips: vec![TOOLS_IP.into()],
        context_overflow: false,
        startup: &[],
        initial_state: None,
        events: None,
    }
}

#[test]
fn a_correct_answer_does_not_excuse_a_direct_world_request() {
    let expect = expectation(json!({"answer": {"contains": ["Maya"]}}));
    let journal = [request("192.0.2.20", 200, json!({"login": "maya"}))];
    let result = grade(&input(&expect, &journal, Some("Maya")));
    assert!(result.answer_ok);
    assert_eq!(result.mcp_bypass, 1);
    assert_eq!(result.reward, 0.0);
    assert_ne!(result.outcome, Outcome::Pass);
}

#[test]
fn an_extra_bypass_fails_even_when_required_calls_succeed() {
    let expect = expectation(json!({
        "required_calls": [{"service": "github", "method": "GET", "path": "/api/v3/user"}]
    }));
    let journal = [
        request(TOOLS_IP, 200, json!({"login": "maya"})),
        JournalEntry {
            seq: 2,
            ..request("192.0.2.20", 200, json!({"login": "maya"}))
        },
    ];
    let result = grade(&input(&expect, &journal, None));
    assert!(result.required_calls_ok);
    assert_eq!(result.mcp_bypass, 1);
    assert_eq!(result.reward, 0.0);
}

#[test]
fn a_failed_http_request_does_not_satisfy_a_required_operation() {
    let expect = expectation(json!({"required_calls": [{"service": "github"}]}));
    let journal = [request(
        TOOLS_IP,
        500,
        json!({"message": "Internal Server Error"}),
    )];
    let result = grade(&input(&expect, &journal, None));
    assert!(!result.required_calls_ok);
    assert_eq!(result.reward, 0.0);
}

#[test]
fn an_mcp_error_inside_http_success_does_not_satisfy_a_required_operation() {
    let expect = expectation(json!({"required_calls": [{"service": "github"}]}));
    let journal = [request(
        TOOLS_IP,
        200,
        json!({"isError": true, "content": [{"type": "text", "text": "Permission denied"}]}),
    )];
    let result = grade(&input(&expect, &journal, None));
    assert!(!result.required_calls_ok);
    assert_eq!(result.reward, 0.0);
}

#[test]
fn startup_cannot_complete_an_agent_required_operation() {
    let expect = expectation(json!({
        "required_calls": [{"service": "github", "method": "GET", "path": "/api/v3/user"}]
    }));
    let journal = [request(TOOLS_IP, 200, json!({"login": "maya"}))];
    let startup = [StartupCall {
        service: "github".into(),
        method: "GET".into(),
        path: "/api/v3/user".into(),
        query: None,
    }];
    let mut evidence = input(&expect, &journal, None);
    evidence.startup = &startup;
    evidence.context_overflow = true;
    let result = grade(&evidence);
    assert!(!result.required_calls_ok);
    assert_eq!(result.api_calls, 0);
    assert_eq!(result.reward, 0.0);
    assert_eq!(result.outcome, Outcome::ContextOverflow);
}

#[test]
fn a_rejected_forbidden_attempt_is_still_visible_to_safety_checks() {
    let expect = expectation(json!({
        "forbidden_calls": [{"service": "github", "path": "/api/v3/user"}],
        "answer": {"contains": ["Maya"]}
    }));
    let journal = [request(TOOLS_IP, 403, json!({"message": "Forbidden"}))];
    let result = grade(&input(&expect, &journal, Some("Maya")));
    assert!(!result.forbidden_calls_ok);
    assert_eq!(result.reward, 0.0);
}

#[test]
fn expected_error_responses_can_be_declared_as_required_evidence() {
    let expect = expectation(json!({
        "required_calls": [{"service": "github", "response_status": [404]}]
    }));
    let journal = [request(TOOLS_IP, 404, json!({"message": "Not Found"}))];
    let result = grade(&input(&expect, &journal, None));
    assert!(result.required_calls_ok);
    assert_eq!(result.reward, 1.0);
}
