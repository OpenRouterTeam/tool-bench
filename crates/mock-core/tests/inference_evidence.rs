//! Integration tests for the inference evidence module.
//!
//! These are SCRIPTED FIXTURE tests that validate the structured evidence types,
//! cross-language JSON compatibility, and fail-closed classification behavior.
//! They cannot certify native-model admission.

use mock_core::inference::{
    AdapterIdentity, CostAccounting, FinalResponse, FinalizationStatus,
    INFERENCE_EVIDENCE_SCHEMA_VERSION, InferenceClassification, InferenceEvidence,
    InferenceIdentity, InferenceTurn, SamplingValue, TurnKind, legacy_classify,
};
use std::collections::BTreeMap;

fn valid_digest() -> String {
    "sha256:".to_owned() + &"a".repeat(64)
}

/// A well-formed evidence struct for testing.
fn reference_evidence() -> InferenceEvidence {
    InferenceEvidence {
        schema_version: INFERENCE_EVIDENCE_SCHEMA_VERSION,
        adapter: AdapterIdentity {
            name: "scripted-fixture".into(),
            code_sha256: valid_digest(),
        },
        classification: InferenceClassification::Completed,
        turns: vec![
            InferenceTurn {
                turn: 0,
                kind: TurnKind::ToolUse,
                input_tokens: Some(500),
                output_tokens: Some(120),
                retried: Some(false),
                retry_count: Some(0),
                cost_usd: Some(0.001),
                observed_model: None,
                response_id: None,
            },
            InferenceTurn {
                turn: 1,
                kind: TurnKind::ToolUse,
                input_tokens: Some(800),
                output_tokens: Some(200),
                retried: Some(true),
                retry_count: Some(2),
                cost_usd: Some(0.005),
                observed_model: None,
                response_id: None,
            },
            InferenceTurn {
                turn: 2,
                kind: TurnKind::FinalResponse,
                input_tokens: Some(1200),
                output_tokens: Some(300),
                retried: Some(false),
                retry_count: Some(0),
                cost_usd: Some(0.003),
                observed_model: None,
                response_id: None,
            },
        ],
        final_response: Some(FinalResponse {
            text: Some("The duplicate charge has been refunded.".into()),
            turn: 2,
            finalization: FinalizationStatus::Normal,
            resumed: false,
        }),
        cost: CostAccounting {
            total_input_tokens: Some(2500),
            total_output_tokens: Some(620),
            total_cost_usd: Some(0.009),
            retry_input_tokens: Some(800),
            retry_output_tokens: Some(200),
            retry_cost_usd: Some(0.003),
            successful_turns: 3,
            retried_turns: Some(1),
            total_retry_attempts: Some(2),
        },
        identity: InferenceIdentity {
            configured_model: "gpt-4o".into(),
            configured_provider: "openai".into(),
            observed_model: Some("gpt-4o-2024-11-20".into()),
            observed_revision: None,
            sampling: BTreeMap::from([
                ("temperature".into(), SamplingValue::Float(0.0)),
                ("max_tokens".into(), SamplingValue::Integer(4096)),
            ]),
            budget: BTreeMap::from([
                ("max_cost_usd".into(), SamplingValue::Float(1.0)),
                ("timeout_sec".into(), SamplingValue::Integer(300)),
            ]),
        },
        scripted: true,
        finalization: FinalizationStatus::Normal,
        scenario: None,
        session_id: None,
    }
}

#[test]
fn reference_evidence_validates() {
    assert!(reference_evidence().validate().is_ok());
}

#[test]
fn evidence_json_roundtrip_preserves_all_fields() {
    let evidence = reference_evidence();
    let json = serde_json::to_string_pretty(&evidence).unwrap();
    let deserialized: InferenceEvidence = serde_json::from_str(&json).unwrap();
    assert_eq!(evidence, deserialized);
}

#[test]
fn python_scripted_stub_deserializes() {
    // Matches the structure produced by ScriptedEvidenceStub.capture() in Python.
    // The exact adapter digest will differ; we use a placeholder.
    let json = serde_json::json!({
            "schema_version": 2,
        "finalization": "no_response",
        "adapter": {
            "name": "scripted-fixture",
            "code_sha256": format!("sha256:{}", "b".repeat(64))
        },
        "classification": "completed",
        "turns": [],
        "final_response": null,
        "cost": {
            "total_input_tokens": null,
            "total_output_tokens": null,
            "total_cost_usd": null,
            "retry_input_tokens": null,
            "retry_output_tokens": null,
            "retry_cost_usd": null,
            "successful_turns": 0,
            "retried_turns": 0,
            "total_retry_attempts": 0
        },
        "identity": {
            "configured_model": "scripted",
            "configured_provider": "scripted",
            "observed_model": null,
            "observed_revision": null,
            "sampling": {},
            "budget": {}
        },
        "scripted": true
    });
    let evidence: InferenceEvidence = serde_json::from_value(json).unwrap();
    assert!(evidence.validate().is_ok());
    assert!(evidence.scripted);
    assert!(!evidence.admits_native());
    assert_eq!(evidence.classification, InferenceClassification::Completed);
}

#[test]
fn recovered_rate_limit_with_retry_evidence() {
    let mut evidence = reference_evidence();
    evidence.classification = InferenceClassification::RecoveredRateLimit;
    assert!(evidence.validate().is_ok());
    assert!(!evidence.classification.is_infrastructure_fault());
    assert!(evidence.classification.had_inference());
}

#[test]
fn unrecovered_rate_limit_is_infrastructure_fault() {
    assert!(InferenceClassification::UnrecoveredRateLimit.is_infrastructure_fault());
}

#[test]
fn context_rejection_first_vs_later_turn() {
    // First-turn: at most 1 turn allowed.
    let mut evidence = reference_evidence();
    evidence.classification = InferenceClassification::FirstTurnContextRejection;
    evidence.turns = vec![InferenceTurn {
        turn: 0,
        kind: TurnKind::ContextRejected,
        input_tokens: None,
        output_tokens: None,
        retried: Some(false),
        retry_count: Some(0),
        cost_usd: None,
        observed_model: None,
        response_id: None,
    }];
    evidence.final_response = None;
    evidence.finalization = FinalizationStatus::NoResponse;
    assert!(evidence.validate().is_ok());

    // Later-turn: multiple turns allowed.
    let mut evidence = reference_evidence();
    evidence.classification = InferenceClassification::LaterTurnContextRejection;
    evidence.final_response = None;
    evidence.finalization = FinalizationStatus::NoResponse;
    evidence.turns.last_mut().unwrap().kind = TurnKind::ContextRejected;
    assert!(evidence.validate().is_ok());
}

#[test]
fn failed_finalization_classification() {
    let mut evidence = reference_evidence();
    evidence.classification = InferenceClassification::FailedFinalization;
    evidence.finalization = FinalizationStatus::CaptureFailed;
    evidence.final_response = Some(FinalResponse {
        text: None,
        turn: 2,
        finalization: FinalizationStatus::CaptureFailed,
        resumed: false,
    });
    assert!(evidence.validate().is_ok());
    assert!(evidence.classification.is_infrastructure_fault());
}

#[test]
fn resumed_session_requires_flag() {
    let mut evidence = reference_evidence();
    evidence.classification = InferenceClassification::ResumedSession;
    evidence.final_response.as_mut().unwrap().resumed = true;
    assert!(evidence.validate().is_ok());

    evidence.final_response.as_mut().unwrap().resumed = false;
    assert!(evidence.validate().is_err());
}

#[test]
fn legacy_classify_agrees_with_python() {
    assert_eq!(
        legacy_classify(true, 0),
        InferenceClassification::FirstTurnContextRejection
    );
    assert_eq!(
        legacy_classify(true, 5),
        InferenceClassification::LaterTurnContextRejection
    );
    assert_eq!(
        legacy_classify(false, 0),
        InferenceClassification::Completed
    );
    assert_eq!(
        legacy_classify(false, 10),
        InferenceClassification::Completed
    );
}

#[test]
fn scripted_stub_matches_python_structure() {
    let stub = InferenceEvidence::scripted_stub(valid_digest());
    assert!(stub.validate().is_ok());
    assert!(stub.scripted);
    assert!(!stub.admits_native());
    assert_eq!(stub.classification, InferenceClassification::Completed);
    assert!(stub.turns.is_empty());
    assert!(stub.final_response.is_none());
    assert_eq!(stub.identity.configured_model, "scripted");
}

#[test]
fn cost_unknown_is_none_not_zero() {
    // Item 15/16: unavailable model/provider cost stays None, never zero.
    let stub = InferenceEvidence::scripted_stub(valid_digest());
    assert!(stub.cost.total_cost_usd.is_none());
    assert!(stub.cost.total_input_tokens.is_none());
    assert!(stub.cost.total_output_tokens.is_none());
}

#[test]
fn sampling_value_variants() {
    let mut evidence = reference_evidence();
    evidence
        .identity
        .sampling
        .insert("stream".into(), SamplingValue::Bool(true));
    evidence
        .identity
        .sampling
        .insert("stop".into(), SamplingValue::Text("END".into()));
    let json = serde_json::to_string(&evidence).unwrap();
    let deserialized: InferenceEvidence = serde_json::from_str(&json).unwrap();
    assert_eq!(
        deserialized.identity.sampling.get("stream"),
        Some(&SamplingValue::Bool(true))
    );
    assert_eq!(
        deserialized.identity.sampling.get("stop"),
        Some(&SamplingValue::Text("END".into()))
    );
}
