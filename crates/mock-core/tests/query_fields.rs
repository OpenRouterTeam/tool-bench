use mock_core::{
    grading::{Expect, GradeInput, Outcome, grade},
    journal::JournalEntry,
};
use serde_json::json;

fn assess(query: &str, forbidden: bool) -> mock_core::grading::Grade {
    let matcher = json!({
        "service":"argocd","method":"POST","path":"/resource/actions",
        "query_fields":{"resourceName":"search-indexer-worker","namespace":"search"},
        "body":"restart"
    });
    let mut contract = json!({"version":2,"answer":{"contains":["ready"]}});
    contract[if forbidden {
        "forbidden_calls"
    } else {
        "required_calls"
    }] = json!([matcher]);
    let expect: Expect = serde_json::from_value(contract).unwrap();
    let entry: JournalEntry = serde_json::from_value(json!({
        "seq":0,"time":"2026-10-29T14:05:00Z","service":"argocd",
        "source":"192.0.2.10","method":"POST","path":"/resource/actions",
        "query":query,"request_body":"restart","status":200,"response_body":{}
    }))
    .unwrap();
    let initial = json!({"services":{}});
    grade(&GradeInput {
        expect: &expect,
        state: initial.clone(),
        journal: &[entry],
        answer: Some("ready"),
        tools_ips: vec!["192.0.2.10".into()],
        context_overflow: false,
        startup: &[],
        initial_state: Some(&initial),
        events: None,
    })
}

#[test]
fn decoded_parameter_identity_accepts_order_encoding_and_unrelated_parameters() {
    for query in [
        "namespace=search&resourceName=search-indexer-worker",
        "resourceName=search%2Dindexer%2Dworker&namespace=%73earch",
        "extra=value&namespace=search&resourceName=search-indexer-worker&version=v1",
        "namespace=search&namespace=search&resourceName=search-indexer-worker",
    ] {
        let assessment = assess(query, false);
        assert_eq!(assessment.outcome, Outcome::Pass, "{query}: {assessment:?}");
    }
}

#[test]
fn prefixes_substrings_wrong_namespaces_and_conflicting_duplicate_values_do_not_bind_identity() {
    for query in [
        "namespace=search&resourceName=search-indexer-worker-shadow",
        "namespace=search-staging&resourceName=search-indexer-worker",
        "namespace=search&other=resourceName%3Dsearch-indexer-worker",
        "namespace=search&resourceName=wrong&resourceName=search-indexer-worker",
        "namespace=search&resourceName=search-indexer-worker&resourceName=wrong",
        "resourceName=search-indexer-worker",
        "",
    ] {
        let assessment = assess(query, false);
        assert_eq!(assessment.outcome, Outcome::Fail, "{query}: {assessment:?}");
        assert!(!assessment.required_calls_ok);
        assert!(assessment.environment_ok && assessment.contract_ok);
    }
}

#[test]
fn forbidden_parameters_use_the_same_decoded_identity_not_query_substrings() {
    assert_eq!(
        assess(
            "namespace=search&resourceName=search%2Dindexer-worker",
            true
        )
        .outcome,
        Outcome::Fail
    );
    assert_eq!(
        assess(
            "namespace=search&resourceName=search-indexer-worker-shadow",
            true
        )
        .outcome,
        Outcome::Pass
    );
}

#[test]
fn empty_parameter_names_are_invalid_contracts() {
    let contract: Expect = serde_json::from_value(json!({
        "version":2,"answer":{"contains":["ready"]},
        "required_calls":[{"service":"argocd","query_fields":{"":"value"}}]
    }))
    .unwrap();
    assert!(contract.validate().is_err());
}
