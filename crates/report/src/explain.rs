//! One explanation per trial, built from the same bound evidence the report grades.
//!
//! A failed trial names its failure class, the checks that failed and the evidence each
//! came from, and cost, latency and retry accounting. A measurement nobody observed is
//! `null` and listed under `unknown`; it is never reported as zero.

use std::{collections::BTreeMap, fs, path::Path};

use anyhow::Context;
use mock_core::{
    failure::{FailureClass, classify},
    grading::Grade,
    inference::InferenceEvidence,
    transport::ExchangeRole,
};
use serde::Serialize;
use serde_json::{Value, json};

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FailedCheck {
    pub check: Value,
    pub disposition: Value,
    pub location: String,
    pub reason: String,
    /// Which bound artifact the check was decided from.
    pub evidence_source: &'static str,
    /// World journal sequence numbers supporting the failure.
    pub journal_evidence: Vec<u64>,
    pub event_evidence: Vec<u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Accounting {
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub cost_usd: Option<f64>,
    pub inference_turns: Option<u32>,
    pub retried_turns: Option<u32>,
    pub retry_attempts: Option<u32>,
    pub retry_cost_usd: Option<f64>,
    /// Sum of provider latency over main-thread exchanges.
    pub provider_latency_ms: Option<u64>,
    /// Latency of the exchange that returned the final reply.
    pub final_response_latency_ms: Option<u64>,
    pub wall_clock_ms: Option<u64>,
    pub auxiliary_requests: Option<u32>,
    pub auxiliary_cost_usd: Option<f64>,
    /// Names of the measurements above that were not observed.
    pub unknown: Vec<&'static str>,
    pub scope: &'static str,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Explanation {
    pub task: String,
    pub trial: String,
    /// Whether the assessment was reproduced from its bound inputs.
    pub verified: bool,
    pub unverified_reason: Option<String>,
    pub outcome: Value,
    pub reward: Option<f64>,
    pub failure_class: Option<FailureClass>,
    pub failed_checks: Vec<FailedCheck>,
    pub answer_channel: Value,
    pub inference_classification: Value,
    pub inference_finalization: Value,
    pub capture_faults: Vec<String>,
    /// Provider status, failure and stop reason of the last main-thread exchange.
    pub terminal_exchange: Value,
    pub accounting: Accounting,
    /// Hashes of the artifacts the explanation was derived from.
    pub evidence: BTreeMap<String, String>,
}

fn source(location: &str) -> &'static str {
    match location.split('/').next().unwrap_or_default() {
        "inference" => "inference_evidence",
        "answer" => "answer_channel",
        "contract" => "task_contract",
        "evidence" if location.contains("runner") => "runner_receipt",
        "evidence" => "world_evidence",
        _ => "world_evidence",
    }
}

fn failed_checks(assessment: &Grade) -> Vec<FailedCheck> {
    assessment
        .failures
        .iter()
        .map(|failure| FailedCheck {
            check: json!(failure.check),
            disposition: json!(failure.disposition),
            location: failure.location.clone(),
            reason: failure.reason.clone(),
            evidence_source: source(&failure.location),
            journal_evidence: failure.evidence.clone(),
            event_evidence: failure.event_evidence.clone(),
        })
        .collect()
}

fn accounting(inference: Option<&InferenceEvidence>) -> Accounting {
    let native = inference.filter(|evidence| !evidence.scripted);
    let transport = native.and_then(|evidence| evidence.transport.as_ref());
    let main = || {
        transport
            .into_iter()
            .flat_map(|transport| &transport.exchanges)
            .filter(|exchange| exchange.role == ExchangeRole::Main)
    };
    let provider_latency_ms = transport.and_then(|_| {
        main()
            .map(|exchange| exchange.latency_ms)
            .collect::<Option<Vec<_>>>()
            .map(|values| values.into_iter().sum())
    });
    let final_response_latency_ms = native
        .filter(|evidence| evidence.final_text().is_some())
        .and_then(|_| main().next_back())
        .and_then(|exchange| exchange.latency_ms);
    let mut result = Accounting {
        input_tokens: native.and_then(|evidence| evidence.cost.total_input_tokens),
        output_tokens: native.and_then(|evidence| evidence.cost.total_output_tokens),
        cost_usd: native.and_then(|evidence| evidence.cost.total_cost_usd),
        inference_turns: native.map(|evidence| evidence.turns.len() as u32),
        retried_turns: native.and_then(|evidence| evidence.cost.retried_turns),
        retry_attempts: native.and_then(|evidence| evidence.cost.total_retry_attempts),
        retry_cost_usd: native.and_then(|evidence| evidence.cost.retry_cost_usd),
        provider_latency_ms,
        final_response_latency_ms,
        wall_clock_ms: transport.and_then(|transport| transport.wall_clock_ms),
        auxiliary_requests: transport.map(|transport| transport.auxiliary.requests),
        auxiliary_cost_usd: transport.and_then(|transport| transport.auxiliary.cost_usd),
        unknown: Vec::new(),
        scope: "host-observed provider exchanges; router-internal inference and tool execution time are not measured",
    };
    for (name, known) in [
        ("input_tokens", result.input_tokens.is_some()),
        ("output_tokens", result.output_tokens.is_some()),
        ("cost_usd", result.cost_usd.is_some()),
        ("inference_turns", result.inference_turns.is_some()),
        ("retried_turns", result.retried_turns.is_some()),
        ("retry_attempts", result.retry_attempts.is_some()),
        ("retry_cost_usd", result.retry_cost_usd.is_some()),
        ("provider_latency_ms", result.provider_latency_ms.is_some()),
        (
            "final_response_latency_ms",
            result.final_response_latency_ms.is_some(),
        ),
        ("wall_clock_ms", result.wall_clock_ms.is_some()),
        ("auxiliary_requests", result.auxiliary_requests.is_some()),
        ("auxiliary_cost_usd", result.auxiliary_cost_usd.is_some()),
    ] {
        if !known {
            result.unknown.push(name);
        }
    }
    result
}

fn inference_fields(inference: Option<&InferenceEvidence>) -> (Value, Value, Vec<String>, Value) {
    let Some(evidence) = inference else {
        return (Value::Null, Value::Null, Vec::new(), Value::Null);
    };
    let transport = evidence.transport.as_ref();
    let terminal = transport
        .and_then(|transport| {
            transport
                .exchanges
                .iter()
                .rev()
                .find(|exchange| exchange.role == ExchangeRole::Main)
        })
        .map_or(Value::Null, |exchange| {
            json!({
                "status": exchange.status, "failure": exchange.failure,
                "stop_reason": exchange.stop_reason, "response_id": exchange.response_id,
                "request_sha256": exchange.request_sha256,
                "response_sha256": exchange.response_sha256,
            })
        });
    (
        json!(evidence.classification),
        json!(evidence.finalization),
        transport.map_or_else(Vec::new, |transport| transport.capture_faults.clone()),
        terminal,
    )
}

fn explain(directory: &Path, result: &Value, task: String) -> Explanation {
    let trial = result["trial_name"].as_str().unwrap_or_default().to_owned();
    let verifier = directory.join("verifier");
    let verification: Value = fs::read(verifier.join("verification.json"))
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or(Value::Null);
    let rewards = &result["verifier_result"]["rewards"];
    match crate::provenance::load_verified(directory, result) {
        Ok(Some(verified)) => {
            let inference = verified.provenance.inference.as_ref();
            let (classification, finalization, capture_faults, terminal_exchange) =
                inference_fields(inference);
            Explanation {
                task,
                trial,
                verified: true,
                unverified_reason: None,
                outcome: json!(verified.assessment.outcome),
                reward: Some(verified.assessment.reward),
                failure_class: Some(classify(&verified.assessment, inference)),
                failed_checks: failed_checks(&verified.assessment),
                answer_channel: verification["answer_source"].clone(),
                inference_classification: classification,
                inference_finalization: finalization,
                capture_faults,
                terminal_exchange,
                accounting: accounting(inference),
                evidence: verified.bindings,
            }
        }
        other => {
            // The saved assessment could not be reproduced. Report what the verifier saved
            // and why it is not trusted; do not derive a classification from it.
            let reason = match other {
                Err(error) => format!("{error:#}"),
                _ if !result["exception_info"].is_null() => {
                    "the trial raised before verification".into()
                }
                _ => "no provenance-checked assessment was collected".into(),
            };
            let saved: Value = fs::read(verifier.join("assessment.json"))
                .ok()
                .and_then(|bytes| serde_json::from_slice(&bytes).ok())
                .unwrap_or(Value::Null);
            let failed_checks = saved["failures"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|failure| {
                    let location = failure["location"].as_str().unwrap_or_default().to_owned();
                    FailedCheck {
                        check: failure["check"].clone(),
                        disposition: failure["disposition"].clone(),
                        evidence_source: source(&location),
                        location,
                        reason: failure["reason"].as_str().unwrap_or_default().to_owned(),
                        journal_evidence: Vec::new(),
                        event_evidence: Vec::new(),
                    }
                })
                .collect();
            Explanation {
                task,
                trial,
                verified: false,
                unverified_reason: Some(reason),
                outcome: saved["outcome"].clone(),
                reward: rewards["reward"].as_f64(),
                failure_class: None,
                failed_checks,
                answer_channel: verification["answer_source"].clone(),
                inference_classification: verification["inference_classification"].clone(),
                inference_finalization: Value::Null,
                capture_faults: Vec::new(),
                terminal_exchange: Value::Null,
                accounting: accounting(None),
                evidence: BTreeMap::new(),
            }
        }
    }
}

/// Explain every trial under `root`, in the order `load_trials` returns them.
pub fn explain_trials(root: &Path) -> anyhow::Result<Vec<Explanation>> {
    let mut files = Vec::new();
    crate::walk(root, &mut files)?;
    files.sort();
    let mut explanations = Vec::new();
    for file in files {
        let result: Value = serde_json::from_str(&fs::read_to_string(&file)?)
            .with_context(|| format!("parsing {}", file.display()))?;
        let Some(task_name) = result["task_name"].as_str() else {
            continue;
        };
        let task = task_name
            .rsplit('/')
            .next()
            .unwrap_or(task_name)
            .trim_start_matches("tool-bench__")
            .to_owned();
        explanations.push(explain(
            file.parent().context("trial directory")?,
            &result,
            task,
        ));
    }
    Ok(explanations)
}

/// The verified trial summary with one explanation attached to each trial.
pub fn summary(root: &Path) -> anyhow::Result<Value> {
    let trials = crate::load_diagnostic_trials(root)?;
    let explanations = explain_trials(root)?;
    anyhow::ensure!(
        trials.len() == explanations.len(),
        "trial explanations do not cover the loaded trials"
    );
    let mut summary = crate::trial_summary(&trials);
    for (trial, explanation) in summary["trials"]
        .as_array_mut()
        .into_iter()
        .flatten()
        .zip(explanations)
    {
        trial["explanation"] = json!(explanation);
    }
    Ok(summary)
}
