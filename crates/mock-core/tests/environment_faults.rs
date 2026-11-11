use mock_core::grading::{Expect, GradeInput, Outcome, grade};
use serde_json::{Value, json};

fn definition() -> Value {
    json!({
        "state": [{"pointer": "/services/store/products/one/status", "equals": "ACTIVE"}],
        "environment_faults": [{
            "code": "missing_publication", "reason": "No supported channel-publication operation."
        }]
    })
}

#[test]
fn declared_unsupported_setup_cannot_be_a_model_failure_or_a_successful_assessment() {
    let expect: Expect = serde_json::from_value(definition()).unwrap();
    for status in ["ACTIVE", "DRAFT"] {
        let result = grade(&GradeInput {
            expect: &expect,
            state: json!({"services": {"store": {"products": {"one": {"status": status}}}}}),
            journal: &[],
            answer: None,
            tools_ips: vec![],
            context_overflow: false,
            startup: &[],
            initial_state: None,
            events: None,
        });
        assert_eq!(result.reward, 0.0);
        assert_eq!(result.outcome, Outcome::EnvironmentError);
        assert!(result.contract_ok);
        assert!(!result.environment_ok);
        assert_eq!(result.state_ok, status == "ACTIVE");
        assert!(
            result
                .failures
                .iter()
                .any(|failure| failure.location == "environment/declared/missing_publication")
        );
    }
}

#[test]
fn environment_faults_need_meaningful_unique_codes_and_reasons() {
    for faults in [
        json!([{"code": "", "reason": "reason"}]),
        json!([{"code": "bad/code", "reason": "reason"}]),
        json!([{"code": "valid", "reason": " "}]),
        json!([{"code": "same", "reason": "first"}, {"code": "same", "reason": "second"}]),
    ] {
        let mut value = definition();
        value["environment_faults"] = faults;
        let expect: Expect = serde_json::from_value(value).unwrap();
        assert!(expect.validate().is_err());
    }
}
