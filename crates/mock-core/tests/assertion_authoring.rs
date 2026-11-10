mod statement_support;

use std::{fs, path::Path, process::Command};

use mock_core::grading::Outcome;
use serde_json::{Value, json};
use statement_support::Case;

const SCENARIO: &str = "search/elasticsearch/latency-by-region";

#[test]
fn proposals_bind_values_instead_of_copying_them_into_the_reviewed_clause() {
    let case = Case::load(SCENARIO);
    let facts = case
        .expect
        .answer
        .as_ref()
        .unwrap()
        .record_facts
        .as_ref()
        .unwrap();
    let template = facts
        .suggest_assertion("eu-west-1 averages 0.4125 seconds.", &case.initial)
        .unwrap();
    assert_eq!(template, "${region} averages ${average}.");
    let mut edited = case.initial.clone();
    *edited
        .pointer_mut("/services/elasticsearch/docs/requests-checkout-2026.10~1r0005/duration_ms")
        .unwrap() = json!(500);
    assert_eq!(
        facts
            .suggest_assertion("eu-west-1 averages 422.5 ms.", &edited)
            .unwrap(),
        template,
    );
    assert!(
        facts
            .suggest_assertion("eu-west-1 averages 412.5 ms.", &edited)
            .is_err()
    );
}

#[test]
fn affirmative_forms_cannot_name_undeclared_slots_or_omit_all_source_bindings() {
    for assertion in ["Verified everything.", "The region is ${undeclared}."] {
        let mut case = Case::load(SCENARIO);
        case.expect
            .answer
            .as_mut()
            .unwrap()
            .record_facts
            .as_mut()
            .unwrap()
            .assertions = vec![assertion.into()];
        assert!(case.expect.validate().is_err(), "{assertion}");
    }
}

#[test]
fn the_authoring_executable_returns_proposals_not_semantic_approval() {
    let directory = tempfile::tempdir().unwrap();
    let answers = directory.path().join("answers.json");
    fs::write(
        &answers,
        json!([
            "eu-west-1 averages 0.4125 seconds.",
            "eu-west-1 wins on speed at 412.5 ms.",
            "I do not know."
        ])
        .to_string(),
    )
    .unwrap();
    let scenario = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../scenarios")
        .join(SCENARIO);
    let output = Command::new(env!("CARGO_BIN_EXE_bench-answer-forms"))
        .arg(scenario)
        .arg(answers)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let response: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(response["format"], "source_bound_clause_proposals_v1");
    assert_eq!(response["source"]["scenario"], SCENARIO);
    assert_eq!(
        response["proposals"][0]["template"],
        "${region} averages ${average}."
    );
    assert_eq!(response["proposals"][1]["semantic_review"], "required");
    assert_eq!(
        response["proposals"][1]["template"],
        "${region} wins on speed at ${average}."
    );
    assert!(response["proposals"][2]["error"].is_string());
    assert_eq!(
        Case::load(SCENARIO).outcome("eu-west-1 wins on speed at 412.5 ms."),
        Outcome::Unresolved
    );
}
