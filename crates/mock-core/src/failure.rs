//! One failure classification for the verifier, the report and the tally.
//!
//! It is computed from the graded assessment and the bound inference evidence only.
//! Journals, transcripts and agent-authored files are never searched for error text.

use serde::Serialize;

use crate::{
    grading::{Grade, Outcome},
    inference::{InferenceClassification, InferenceEvidence},
};

/// Who a result is attributed to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FaultDomain {
    /// The trial passed.
    None,
    /// The agent or model: a scored failure.
    Agent,
    /// Environment, provider or capture: not a score.
    Infrastructure,
    /// The task contract or evaluator.
    Evaluator,
    /// The evidence cannot decide; needs adjudication.
    Unresolved,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FailureClass {
    /// The single label every consumer reports.
    pub label: String,
    pub domain: FaultDomain,
    /// Host inference classification, when native evidence was bound.
    pub inference: Option<InferenceClassification>,
}

fn name(classification: InferenceClassification) -> String {
    serde_json::to_value(classification)
        .ok()
        .and_then(|value| value.as_str().map(str::to_owned))
        .unwrap_or_default()
}

pub fn classify(assessment: &Grade, inference: Option<&InferenceEvidence>) -> FailureClass {
    let native = inference
        .filter(|evidence| !evidence.scripted)
        .map(|evidence| evidence.classification);
    let (label, domain) = match assessment.outcome {
        Outcome::Pass => ("pass".to_owned(), FaultDomain::None),
        Outcome::InvalidContract => ("invalid_contract".to_owned(), FaultDomain::Evaluator),
        Outcome::Unresolved => ("unresolved".to_owned(), FaultDomain::Unresolved),
        Outcome::IntegrityViolation => ("integrity_violation".to_owned(), FaultDomain::Agent),
        Outcome::EnvironmentError => (
            match native {
                Some(classification) if classification.is_infrastructure_fault() => {
                    name(classification)
                }
                _ => "environment_error".to_owned(),
            },
            FaultDomain::Infrastructure,
        ),
        Outcome::ContextOverflow => (
            match native {
                Some(classification) if classification.is_context_rejection() => {
                    name(classification)
                }
                // Frozen historical conditions carry only a boolean flag.
                _ => "context_overflow_legacy_flag".to_owned(),
            },
            FaultDomain::Agent,
        ),
        Outcome::Fail => (
            match native {
                Some(
                    InferenceClassification::Completed
                    | InferenceClassification::RecoveredRateLimit,
                )
                | None => "task_failure".to_owned(),
                Some(classification) => name(classification),
            },
            FaultDomain::Agent,
        ),
    };
    FailureClass {
        label,
        domain,
        inference: native,
    }
}
