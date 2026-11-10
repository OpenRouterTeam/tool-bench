//! Trusted native inference evidence and failure classification.
//!
//! This module captures host-observed, agent-unforgeable evidence about inference
//! transport, final responses, and failure modes. It replaces competing classification
//! paths (empty journal grep, transcript text matching) with structured evidence that
//! produces the same classification locally, in Harbor reporting, and in paired reports.
//!
//! Plan items: 3 (integrity gate), 4 (semantic events), 5 (failure classification),
//! 11 (final-response capture), 15 (manifest binding), 16 (explainable results).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// Structured inference failure classification (plan item 5).
///
/// Each variant represents a distinct failure mode that must not be conflated.
/// The classification is determined from host-observed harness evidence, not
/// agent-controlled files or transcript grep.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InferenceClassification {
    /// Inference completed normally; the model produced a parseable response.
    Completed,
    /// Environment startup failed before any inference occurred.
    EnvironmentStartupFailure,
    /// Tool registration or schema admission failed during setup.
    SchemaAdmissionFailure,
    /// The provider rejected the context on the first turn (too long, invalid format).
    FirstTurnContextRejection,
    /// The provider rejected the context on a later turn (accumulated context too large).
    LaterTurnContextRejection,
    /// A rate limit or transient provider error occurred but the session eventually recovered.
    RecoveredRateLimit,
    /// A rate limit or transient provider error was not recovered; the session failed.
    UnrecoveredRateLimit,
    /// The model produced output that could not be parsed into a valid tool call or response.
    MalformedModelOutput,
    /// Tool execution failed (the tool itself errored, not the model's choice).
    ToolExecutionFailure,
    /// The model produced a policy-correct clarification or refusal.
    PolicyRefusal,
    /// Ordinary task failure: the model completed but did not achieve the objective.
    TaskFailure,
    /// The session was resumed after interruption; evidence carries the resumed flag.
    ResumedSession,
    /// Finalization failed: the harness could not capture a final response.
    FailedFinalization,
    BudgetExhausted,
    NoPublicResponse,
}

impl InferenceClassification {
    /// Whether this classification represents an infrastructure/environment fault
    /// rather than a model behavioral outcome.
    pub fn is_infrastructure_fault(self) -> bool {
        matches!(
            self,
            Self::EnvironmentStartupFailure
                | Self::SchemaAdmissionFailure
                | Self::UnrecoveredRateLimit
                | Self::FailedFinalization
        )
    }

    /// Whether the session produced any model inference at all.
    pub fn had_inference(self) -> bool {
        !matches!(
            self,
            Self::EnvironmentStartupFailure | Self::SchemaAdmissionFailure
        )
    }

    /// Whether a context rejection occurred (first or later turn).
    pub fn is_context_rejection(self) -> bool {
        matches!(
            self,
            Self::FirstTurnContextRejection | Self::LaterTurnContextRejection
        )
    }
}

/// A single inference turn observed by the trusted harness transport.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InferenceTurn {
    /// Monotonic turn index within the session (0-based).
    pub turn: u32,
    /// Whether the turn was a tool-use response or a final assistant reply.
    pub kind: TurnKind,
    /// Provider-reported input tokens for this turn, if available.
    pub input_tokens: Option<u64>,
    /// Provider-reported output tokens for this turn, if available.
    pub output_tokens: Option<u64>,
    /// Whether this turn encountered a transient error that was retried.
    pub retried: Option<bool>,
    /// Number of retry attempts for this turn (0 if no retries).
    pub retry_count: Option<u32>,
    /// Provider-reported cost for this turn in USD, if available. `None` means unknown.
    pub cost_usd: Option<f64>,
    #[serde(default)]
    pub observed_model: Option<String>,
    #[serde(default)]
    pub response_id: Option<String>,
}

/// Distinguishes tool-use turns from final assistant responses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TurnKind {
    /// The model requested one or more tool calls.
    ToolUse,
    /// The model produced a final assistant response (no tool calls).
    FinalResponse,
    /// The model produced output that could not be classified.
    Unparseable,
    /// The provider rejected this turn's context.
    ContextRejected,
    /// A transient provider error on this turn.
    ProviderError,
}

/// The final assistant response captured by the trusted harness transport (plan item 11).
///
/// This is NOT `/app/answer.md` or any agent-editable file. It is the last assistant
/// message observed by the native harness before the session ended.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FinalResponse {
    /// The text content of the final assistant message, if captured.
    pub text: Option<String>,
    /// The turn index that produced this response.
    pub turn: u32,
    /// Whether the session ended normally or was interrupted/timed out.
    pub finalization: FinalizationStatus,
    /// Whether this response came from a resumed session.
    pub resumed: bool,
}

/// How the inference session ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FinalizationStatus {
    /// The model produced a final response and the session ended cleanly.
    Normal,
    /// The session was interrupted (timeout, cancellation).
    Interrupted,
    /// The harness could not capture the final state.
    CaptureFailed,
    /// The session did not produce any final response.
    NoResponse,
}

/// Per-stage cost and token accounting (plan item 16).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CostAccounting {
    /// Total input tokens across all turns.
    pub total_input_tokens: Option<u64>,
    /// Total output tokens across all turns.
    pub total_output_tokens: Option<u64>,
    /// Total cost in USD across all turns. `None` means unavailable, never zero for unknown.
    pub total_cost_usd: Option<f64>,
    /// Tokens/cost consumed by retried requests (subset of totals).
    pub retry_input_tokens: Option<u64>,
    pub retry_output_tokens: Option<u64>,
    pub retry_cost_usd: Option<f64>,
    /// Number of successful inference turns.
    pub successful_turns: u32,
    /// Number of turns that required retries.
    pub retried_turns: Option<u32>,
    /// Number of total retry attempts across all turns.
    pub total_retry_attempts: Option<u32>,
}

/// Identity of the inference provider/model as configured and observed (plan item 15).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InferenceIdentity {
    /// The model identifier as configured for the trial.
    pub configured_model: String,
    /// The provider identifier as configured (e.g. "openai", "anthropic", "harbor").
    pub configured_provider: String,
    /// The model identifier as reported by the provider in responses, if available.
    /// `None` means the provider did not report it, not that it matches configured.
    pub observed_model: Option<String>,
    /// Provider-reported revision/version, if available. `None` means unknown, never invented.
    pub observed_revision: Option<String>,
    /// Sampling parameters as configured (temperature, top_p, max_tokens, etc.).
    pub sampling: BTreeMap<String, SamplingValue>,
    /// Budget identity: max tokens, max cost, timeout as configured.
    pub budget: BTreeMap<String, SamplingValue>,
}

/// A sampling or budget parameter value that can be numeric, boolean, or string.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum SamplingValue {
    Integer(i64),
    Float(f64),
    Bool(bool),
    Text(String),
}

/// The complete inference evidence for one trial (plan items 3/5/11/15/16).
///
/// This is produced by a trusted harness adapter and bound to the runner receipt.
/// Agent-editable trajectories, `/app/answer.md`, and agent logs are NOT used as
/// inference evidence.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InferenceEvidence {
    /// Schema version for this evidence format.
    pub schema_version: u32,
    /// The adapter that produced this evidence.
    pub adapter: AdapterIdentity,
    /// Structured classification of the inference outcome.
    pub classification: InferenceClassification,
    /// Per-turn evidence from the harness transport.
    pub turns: Vec<InferenceTurn>,
    /// The final assistant response, if captured.
    pub final_response: Option<FinalResponse>,
    /// Aggregated cost/token accounting.
    pub cost: CostAccounting,
    /// Inference identity (model, provider, sampling, budget).
    pub identity: InferenceIdentity,
    /// Whether this evidence was produced by a scripted/fixture provider.
    /// Scripted evidence cannot certify native-model admission.
    pub scripted: bool,
    pub finalization: FinalizationStatus,
    #[serde(default)]
    pub scenario: Option<String>,
    #[serde(default)]
    pub session_id: Option<String>,
    /// Provider exchanges recorded by a host proxy, for container-side CLI agents.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transport: Option<crate::transport::TransportEvidence>,
}

/// Current schema version for inference evidence.
pub const INFERENCE_EVIDENCE_SCHEMA_VERSION: u32 = 2;
pub const COMPUTER1_ADAPTER_NAME: &str = "harbor-native-computer1-v1";

/// Identity of the adapter that produced inference evidence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdapterIdentity {
    /// Adapter name (e.g. "harbor-native", "scripted-fixture").
    pub name: String,
    /// Implementation digest for the adapter code.
    pub code_sha256: String,
}

impl InferenceEvidence {
    /// Validate that this evidence is internally consistent and matches its schema.
    pub fn validate(&self) -> Result<(), String> {
        if self.schema_version != INFERENCE_EVIDENCE_SCHEMA_VERSION {
            return Err(format!(
                "unsupported inference evidence schema version {}",
                self.schema_version
            ));
        }
        if self.adapter.name.is_empty()
            || !crate::provenance::digest_valid(&self.adapter.code_sha256)
        {
            return Err("inference evidence adapter identity is incomplete".into());
        }
        if self.identity.configured_model.is_empty()
            || self.identity.configured_provider.is_empty()
            || self
                .identity
                .sampling
                .values()
                .chain(self.identity.budget.values())
                .any(|value| matches!(value, SamplingValue::Float(number) if !number.is_finite()))
        {
            return Err("configured inference identity or settings are invalid".into());
        }

        // Validate turn consistency.
        for (index, turn) in self.turns.iter().enumerate() {
            if turn.turn != index as u32 {
                return Err(format!(
                    "inference turn {index} has non-sequential turn index {}",
                    turn.turn
                ));
            }
            if turn.retried.is_none() != turn.retry_count.is_none() {
                return Err("retry observation must be known or unknown together".into());
            }
            if turn.retried == Some(true) && turn.retry_count == Some(0) {
                return Err(format!(
                    "inference turn {index} is marked retried but has zero retry count"
                ));
            }
            if turn.retried == Some(false) && turn.retry_count.is_some_and(|count| count > 0) {
                return Err(format!(
                    "inference turn {index} has retry count but is not marked retried"
                ));
            }
            if turn
                .cost_usd
                .is_some_and(|cost| !cost.is_finite() || cost < 0.0)
            {
                return Err("inference turn cost is invalid".into());
            }
        }

        // Validate final_response consistency.
        if let Some(ref response) = self.final_response {
            if response.finalization != self.finalization {
                return Err("final response and session finalization disagree".into());
            }
            if response.turn as usize >= self.turns.len() {
                return Err("final response references a turn beyond the captured sequence".into());
            }
            if response.finalization == FinalizationStatus::Normal && response.text.is_none() {
                return Err("normal finalization must have captured response text".into());
            }
            if response.finalization == FinalizationStatus::Normal
                && (self.turns[response.turn as usize].kind != TurnKind::FinalResponse
                    || response.turn as usize != self.turns.len() - 1)
            {
                return Err("normal final response is not bound to a final-response turn".into());
            }
        }

        // Classification/turn consistency.
        match self.classification {
            InferenceClassification::Completed | InferenceClassification::TaskFailure => {
                if !self.scripted
                    && self
                        .final_response
                        .as_ref()
                        .is_none_or(|response| response.finalization != FinalizationStatus::Normal)
                {
                    return Err(
                        "completed/task-failure classification requires a final response for native evidence".into(),
                    );
                }
            }
            InferenceClassification::EnvironmentStartupFailure
            | InferenceClassification::SchemaAdmissionFailure => {
                if !self.turns.is_empty() {
                    return Err(
                        "startup/schema failure classification must have no inference turns".into(),
                    );
                }
            }
            InferenceClassification::FirstTurnContextRejection => {
                if self.turns.len() > 1 {
                    return Err("first-turn context rejection must have at most one turn".into());
                }
                if self.turns.len() != 1 || self.turns[0].kind != TurnKind::ContextRejected {
                    return Err("first-turn context rejection lacks a rejected turn".into());
                }
            }
            InferenceClassification::LaterTurnContextRejection => {
                if self.turns.len() < 2
                    || self
                        .turns
                        .last()
                        .is_none_or(|turn| turn.kind != TurnKind::ContextRejected)
                {
                    return Err("later-turn context rejection lacks its terminal rejection".into());
                }
            }
            InferenceClassification::NoPublicResponse => {
                if self.final_response.is_some()
                    || self.finalization != FinalizationStatus::NoResponse
                {
                    return Err(
                        "no-public-response classification cannot contain a fabricated reply"
                            .into(),
                    );
                }
            }
            InferenceClassification::ResumedSession => {
                if let Some(ref response) = self.final_response
                    && !response.resumed
                {
                    return Err(
                        "resumed-session classification requires resumed flag on final response"
                            .into(),
                    );
                }
            }
            _ => {}
        }

        // Cost consistency: retry subsets must not exceed totals.
        if let (Some(total), Some(retry)) =
            (self.cost.total_input_tokens, self.cost.retry_input_tokens)
            && retry > total
        {
            return Err("retry input tokens exceed total input tokens".into());
        }
        if let (Some(total), Some(retry)) =
            (self.cost.total_output_tokens, self.cost.retry_output_tokens)
            && retry > total
        {
            return Err("retry output tokens exceed total output tokens".into());
        }
        for value in [self.cost.total_cost_usd, self.cost.retry_cost_usd]
            .into_iter()
            .flatten()
        {
            if !value.is_finite() || value < 0.0 {
                return Err("inference aggregate cost is invalid".into());
            }
        }

        // Scripted evidence must not claim native adapter names.
        if self.scripted && self.adapter.name.contains("native") {
            return Err("scripted evidence must not claim a native adapter identity".into());
        }

        Ok(())
    }

    /// Classify this evidence for the report, producing a normalized classification
    /// that agrees between local and Harbor reporting (plan item 5/16).
    pub fn normalized_classification(&self) -> InferenceClassification {
        self.classification
    }

    /// An unbound document cannot certify host provenance.
    pub fn admits_native(&self) -> bool {
        false
    }

    pub fn final_text(&self) -> Option<&str> {
        self.final_response
            .as_ref()
            .filter(|response| {
                response.finalization == FinalizationStatus::Normal && !response.resumed
            })
            .and_then(|response| response.text.as_deref())
    }

    pub fn apply_to_grade(&self, assessment: &mut crate::grading::Grade) {
        use crate::grading::{CheckDisposition, CheckFailure, CheckKind, Outcome};
        if self.scripted {
            return;
        }
        if self.classification == InferenceClassification::NoPublicResponse {
            return;
        }
        if self.classification.is_infrastructure_fault() {
            assessment.invalidate_environment(
                "inference/transport",
                format!("host inference ended as {:?}", self.classification),
            );
        } else if self.classification.is_context_rejection() || self.final_text().is_none() {
            assessment.reward = 0.0;
            assessment.answer_ok = false;
            if assessment.contract_ok && assessment.environment_ok && assessment.integrity_ok {
                assessment.outcome = if self.classification.is_context_rejection() {
                    Outcome::ContextOverflow
                } else {
                    Outcome::Fail
                };
            }
            assessment.failures.push(CheckFailure {
                check: CheckKind::Answer,
                disposition: CheckDisposition::Failed,
                location: "inference/completion".into(),
                reason: format!("host inference ended as {:?}", self.classification),
                evidence: Vec::new(),
                event_evidence: Vec::new(),
            });
        }
    }

    /// Build a stub for scripted/fixture runs that cannot certify native admission.
    pub fn scripted_stub(adapter_sha256: String) -> Self {
        Self {
            schema_version: INFERENCE_EVIDENCE_SCHEMA_VERSION,
            adapter: AdapterIdentity {
                name: "scripted-fixture".into(),
                code_sha256: adapter_sha256,
            },
            classification: InferenceClassification::Completed,
            turns: Vec::new(),
            final_response: None,
            cost: CostAccounting::default(),
            identity: InferenceIdentity {
                configured_model: "scripted".into(),
                configured_provider: "scripted".into(),
                observed_model: None,
                observed_revision: None,
                sampling: BTreeMap::new(),
                budget: BTreeMap::new(),
            },
            scripted: true,
            finalization: FinalizationStatus::NoResponse,
            scenario: None,
            session_id: None,
            transport: None,
        }
    }
}

pub fn load_bound(
    bytes: Option<&[u8]>,
    policy: &crate::runner::RunnerPolicy,
    admission: &crate::runner::RunnerAdmission,
    manifest: &crate::provenance::ExecutionManifest,
) -> Result<Option<InferenceEvidence>, String> {
    // A container-side CLI agent is observed at the host provider proxy; every other
    // producer keeps the host-agent capture policy.
    let proxied = manifest.execution_kind == "native"
        && manifest.agent.name == crate::transport::PROXY_AGENT_NAME;
    let selected = if proxied {
        &policy.proxy_inference_adapter
    } else {
        &policy.inference_adapter
    };
    let Some(approved) = selected else {
        return if bytes.is_none()
            && admission.inference_evidence_sha256.is_none()
            && manifest.execution_kind == "scripted"
        {
            Ok(None)
        } else {
            Err("inference evidence lacks an approved capture implementation".into())
        };
    };
    let expected_name = if proxied {
        crate::transport::PROXY_ADAPTER_NAME
    } else {
        COMPUTER1_ADAPTER_NAME
    };
    if approved.name != expected_name || !crate::provenance::digest_valid(&approved.code_sha256) {
        return Err("unsupported inference capture policy".into());
    }
    let bytes = bytes.ok_or("protected inference evidence was not collected")?;
    if admission.inference_evidence_sha256.as_deref() != Some(crate::runner::sha256(bytes).as_str())
    {
        return Err("inference artifact differs from its runner receipt".into());
    }
    let evidence: InferenceEvidence =
        serde_json::from_slice(bytes).map_err(|error| error.to_string())?;
    evidence.validate()?;
    if evidence.scenario.as_deref() != Some(admission.scenario.as_str())
        || evidence.session_id.as_deref() != Some(admission.session_id.as_str())
        || evidence.scenario.as_deref() != Some(manifest.scenario.as_str())
        || evidence.session_id.as_deref() != Some(manifest.session_id.as_str())
    {
        return Err("inference evidence belongs to a different task or session".into());
    }
    if evidence.adapter.code_sha256 != approved.code_sha256 {
        return Err("inference evidence uses an unapproved implementation".into());
    }
    match manifest.execution_kind.as_str() {
        "scripted"
            if evidence.scripted
                && evidence.adapter.name == "scripted-fixture"
                && matches!(manifest.agent.name.as_str(), "oracle" | "nop")
                && evidence.turns.is_empty()
                && evidence.final_response.is_none()
                && evidence.identity.configured_model == "scripted"
                && evidence.identity.configured_provider == "scripted"
                && evidence.transport.is_none() => {}
        "native"
            if !evidence.scripted
                && evidence.adapter == *approved
                && (proxied || manifest.agent.name == "computer-1")
                && evidence.transport.is_some() == proxied =>
        {
            if proxied {
                crate::transport::check(&evidence)?;
            }
            let configured = manifest
                .inference_identity
                .as_ref()
                .ok_or("native manifest lacks configured inference identity")?;
            if configured.configured_model != evidence.identity.configured_model
                || configured.configured_provider != evidence.identity.configured_provider
                || configured.sampling != evidence.identity.sampling
                || configured.budget != evidence.identity.budget
                || configured.observed_model.is_some()
                || configured.observed_revision.is_some()
            {
                return Err(
                    "inference settings differ from the realized execution manifest".into(),
                );
            }
            if matches!(
                evidence.classification,
                InferenceClassification::PolicyRefusal | InferenceClassification::TaskFailure
            ) {
                return Err(
                    "a transport observer cannot certify semantic task or policy judgments".into(),
                );
            }
        }
        _ => return Err("scripted/native inference provenance is inconsistent".into()),
    }
    if manifest.execution_kind == "native" {
        let observed = evidence
            .turns
            .iter()
            .rev()
            .find_map(|turn| turn.observed_model.as_ref());
        if evidence.identity.observed_model.as_ref() != observed
            || evidence.identity.observed_revision.is_some()
        {
            return Err("observed model identity is not supported by its captured turns".into());
        }
    }
    if manifest.execution_kind == "native"
        && (evidence.classification == InferenceClassification::ResumedSession
            || evidence
                .final_response
                .as_ref()
                .is_some_and(|response| response.resumed))
    {
        return Err("resumed native sessions are not supported by this capture condition".into());
    }
    Ok(Some(evidence))
}

/// Legacy compatibility: convert a boolean context_overflow + empty journal into
/// the appropriate InferenceClassification for frozen historical conditions.
/// This preserves existing behavior without changing historical assessments.
pub fn legacy_classify(context_overflow: bool, api_calls: usize) -> InferenceClassification {
    if context_overflow && api_calls == 0 {
        InferenceClassification::FirstTurnContextRejection
    } else if context_overflow {
        InferenceClassification::LaterTurnContextRejection
    } else {
        InferenceClassification::Completed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_digest() -> String {
        "sha256:".to_owned() + &"a".repeat(64)
    }

    fn valid_evidence() -> InferenceEvidence {
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
                    input_tokens: Some(100),
                    output_tokens: Some(50),
                    retried: Some(false),
                    retry_count: Some(0),
                    cost_usd: None,
                    observed_model: None,
                    response_id: None,
                },
                InferenceTurn {
                    turn: 1,
                    kind: TurnKind::FinalResponse,
                    input_tokens: Some(200),
                    output_tokens: Some(80),
                    retried: Some(false),
                    retry_count: Some(0),
                    cost_usd: Some(0.003),
                    observed_model: None,
                    response_id: None,
                },
            ],
            final_response: Some(FinalResponse {
                text: Some("The refund has been processed.".into()),
                turn: 1,
                finalization: FinalizationStatus::Normal,
                resumed: false,
            }),
            cost: CostAccounting {
                total_input_tokens: Some(300),
                total_output_tokens: Some(130),
                total_cost_usd: Some(0.003),
                retry_input_tokens: Some(0),
                retry_output_tokens: Some(0),
                retry_cost_usd: Some(0.0),
                successful_turns: 2,
                retried_turns: Some(0),
                total_retry_attempts: Some(0),
            },
            identity: InferenceIdentity {
                configured_model: "scripted".into(),
                configured_provider: "scripted".into(),
                observed_model: None,
                observed_revision: None,
                sampling: BTreeMap::new(),
                budget: BTreeMap::new(),
            },
            scripted: true,
            finalization: FinalizationStatus::Normal,
            scenario: None,
            session_id: None,
            transport: None,
        }
    }

    #[test]
    fn valid_evidence_passes_validation() {
        assert!(valid_evidence().validate().is_ok());
    }

    #[test]
    fn wrong_schema_version_rejected() {
        let mut evidence = valid_evidence();
        evidence.schema_version = 99;
        assert!(evidence.validate().unwrap_err().contains("schema version"));
    }

    #[test]
    fn empty_adapter_name_rejected() {
        let mut evidence = valid_evidence();
        evidence.adapter.name = String::new();
        assert!(
            evidence
                .validate()
                .unwrap_err()
                .contains("adapter identity")
        );
    }

    #[test]
    fn bad_adapter_digest_rejected() {
        let mut evidence = valid_evidence();
        evidence.adapter.code_sha256 = "not-a-digest".into();
        assert!(
            evidence
                .validate()
                .unwrap_err()
                .contains("adapter identity")
        );
    }

    #[test]
    fn non_sequential_turns_rejected() {
        let mut evidence = valid_evidence();
        evidence.turns[1].turn = 5;
        assert!(evidence.validate().unwrap_err().contains("non-sequential"));
    }

    #[test]
    fn retried_without_count_rejected() {
        let mut evidence = valid_evidence();
        evidence.turns[0].retried = Some(true);
        evidence.turns[0].retry_count = Some(0);
        assert!(evidence.validate().unwrap_err().contains("retry count"));
    }

    #[test]
    fn count_without_retried_rejected() {
        let mut evidence = valid_evidence();
        evidence.turns[0].retried = Some(false);
        evidence.turns[0].retry_count = Some(3);
        assert!(evidence.validate().unwrap_err().contains("retry count"));
    }

    #[test]
    fn final_response_beyond_turns_rejected() {
        let mut evidence = valid_evidence();
        evidence.final_response.as_mut().unwrap().turn = 99;
        assert!(evidence.validate().unwrap_err().contains("beyond"));
    }

    #[test]
    fn normal_finalization_without_text_rejected() {
        let mut evidence = valid_evidence();
        evidence.final_response.as_mut().unwrap().text = None;
        assert!(
            evidence
                .validate()
                .unwrap_err()
                .contains("captured response text")
        );
    }

    #[test]
    fn startup_failure_with_turns_rejected() {
        let mut evidence = valid_evidence();
        evidence.classification = InferenceClassification::EnvironmentStartupFailure;
        assert!(
            evidence
                .validate()
                .unwrap_err()
                .contains("no inference turns")
        );
    }

    #[test]
    fn first_turn_rejection_with_many_turns_rejected() {
        let mut evidence = valid_evidence();
        evidence.classification = InferenceClassification::FirstTurnContextRejection;
        assert!(
            evidence
                .validate()
                .unwrap_err()
                .contains("at most one turn")
        );
    }

    #[test]
    fn resumed_session_without_flag_rejected() {
        let mut evidence = valid_evidence();
        evidence.classification = InferenceClassification::ResumedSession;
        evidence.final_response.as_mut().unwrap().resumed = false;
        assert!(evidence.validate().unwrap_err().contains("resumed flag"));
    }

    #[test]
    fn retry_tokens_exceeding_total_rejected() {
        let mut evidence = valid_evidence();
        evidence.cost.retry_input_tokens = Some(999);
        assert!(evidence.validate().unwrap_err().contains("exceed total"));
    }

    #[test]
    fn scripted_with_native_name_rejected() {
        let mut evidence = valid_evidence();
        evidence.scripted = true;
        evidence.adapter.name = "harbor-native-v1".into();
        assert!(
            evidence
                .validate()
                .unwrap_err()
                .contains("scripted evidence must not claim")
        );
    }

    #[test]
    fn scripted_stub_does_not_admit_native() {
        let stub = InferenceEvidence::scripted_stub(valid_digest());
        assert!(stub.validate().is_ok());
        assert!(!stub.admits_native());
        assert!(stub.scripted);
    }

    #[test]
    fn classification_predicates() {
        assert!(InferenceClassification::EnvironmentStartupFailure.is_infrastructure_fault());
        assert!(!InferenceClassification::EnvironmentStartupFailure.had_inference());
        assert!(InferenceClassification::FirstTurnContextRejection.is_context_rejection());
        assert!(InferenceClassification::LaterTurnContextRejection.is_context_rejection());
        assert!(!InferenceClassification::Completed.is_context_rejection());
        assert!(!InferenceClassification::TaskFailure.is_infrastructure_fault());
        assert!(InferenceClassification::TaskFailure.had_inference());
    }

    #[test]
    fn legacy_classify_matches_existing_behavior() {
        // Empty journal + context overflow → first-turn rejection (was ContextOverflow outcome).
        assert_eq!(
            legacy_classify(true, 0),
            InferenceClassification::FirstTurnContextRejection
        );
        // Context overflow with API calls → later-turn rejection.
        assert_eq!(
            legacy_classify(true, 5),
            InferenceClassification::LaterTurnContextRejection
        );
        // No overflow → completed.
        assert_eq!(
            legacy_classify(false, 0),
            InferenceClassification::Completed
        );
    }

    #[test]
    fn evidence_roundtrips_through_json() {
        let evidence = valid_evidence();
        let json = serde_json::to_string(&evidence).unwrap();
        let deserialized: InferenceEvidence = serde_json::from_str(&json).unwrap();
        assert_eq!(evidence, deserialized);
    }

    #[test]
    fn interrupted_finalization_without_text_accepted() {
        let mut evidence = valid_evidence();
        evidence.finalization = FinalizationStatus::Interrupted;
        evidence.final_response = Some(FinalResponse {
            text: None,
            turn: 0,
            finalization: FinalizationStatus::Interrupted,
            resumed: false,
        });
        evidence.turns = vec![InferenceTurn {
            turn: 0,
            kind: TurnKind::ToolUse,
            input_tokens: None,
            output_tokens: None,
            retried: Some(false),
            retry_count: Some(0),
            cost_usd: None,
            observed_model: None,
            response_id: None,
        }];
        assert!(evidence.validate().is_ok());
    }

    #[test]
    fn native_label_alone_does_not_admit_runtime() {
        let mut evidence = valid_evidence();
        evidence.scripted = false;
        evidence.adapter.name = "harbor-native-v1".into();
        assert!(!evidence.admits_native());
    }
}
