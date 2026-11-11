//! Provider-transport evidence for container-side CLI agents.
//!
//! A host-owned provider proxy (`deploy/arm-proxy.ts`, sealed by
//! `deploy/runner/proxy_capture.py`) records every provider exchange of an agent that
//! runs inside the task container. The agent only holds the proxy address and a
//! per-trial placeholder credential, so it cannot author these observations.
//!
//! [`derive`] is the one classification of that evidence. The runner computes it when it
//! seals a capture, and [`check`] recomputes it for `inference::load_bound`, so the
//! verifier, the report and the tally cannot disagree with the recorded exchanges.

use serde::{Deserialize, Serialize};

use crate::inference::{
    FinalizationStatus, InferenceClassification, InferenceEvidence, SamplingValue, TurnKind,
};

pub const PROXY_ADAPTER_NAME: &str = "host-proxy-native-anthropic-messages-v1";
pub const PROXY_AGENT_NAME: &str = "claude-code";
pub const PROXY_SEAM: &str = "host_provider_proxy";
pub const PROXY_WIRE_FORMAT: &str = "anthropic_messages";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExchangeRole {
    /// Part of the conversation that carries the task instruction.
    Main,
    /// Any other provider request by the agent process (side queries, subagents).
    Auxiliary,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExchangeFailure {
    ContextLength,
    RateLimit,
    Overloaded,
    ProviderError,
    TransportError,
    Authentication,
    InvalidRequest,
    MalformedResponse,
    IncompleteStream,
}

impl ExchangeFailure {
    /// A failure the client may retry with the same request.
    pub fn transient(self) -> bool {
        matches!(
            self,
            Self::RateLimit
                | Self::Overloaded
                | Self::ProviderError
                | Self::TransportError
                | Self::IncompleteStream
        )
    }
}

/// One provider request/response pair as seen by the host proxy.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Exchange {
    pub sequence: u32,
    pub role: ExchangeRole,
    /// Main-thread turn this attempt belongs to. `None` for auxiliary requests and for
    /// requests rejected before any inference turn existed.
    pub turn: Option<u32>,
    pub request_sha256: String,
    pub response_sha256: Option<String>,
    pub status: Option<u16>,
    pub streamed: bool,
    pub complete: bool,
    pub latency_ms: Option<u64>,
    pub first_byte_ms: Option<u64>,
    pub response_id: Option<String>,
    pub observed_model: Option<String>,
    pub stop_reason: Option<String>,
    pub failure: Option<ExchangeFailure>,
    /// Uncached prompt tokens as the provider reported them.
    pub input_tokens: Option<u64>,
    pub cache_read_input_tokens: Option<u64>,
    pub cache_creation_input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub cost_usd: Option<f64>,
}

impl Exchange {
    /// Prompt tokens including cache reads and writes; unknown when the provider reported none.
    pub fn prompt_tokens(&self) -> Option<u64> {
        Some(
            self.input_tokens?
                + self.cache_read_input_tokens.unwrap_or(0)
                + self.cache_creation_input_tokens.unwrap_or(0),
        )
    }

    fn kind(&self) -> TurnKind {
        match self.failure {
            Some(ExchangeFailure::ContextLength) => TurnKind::ContextRejected,
            Some(ExchangeFailure::MalformedResponse) => TurnKind::Unparseable,
            Some(_) => TurnKind::ProviderError,
            None => match self.stop_reason.as_deref() {
                Some("tool_use" | "pause_turn") => TurnKind::ToolUse,
                Some("end_turn" | "stop_sequence" | "max_tokens" | "refusal") => {
                    TurnKind::FinalResponse
                }
                _ => TurnKind::Unparseable,
            },
        }
    }
}

/// Requests outside the main conversation. Sums are unknown unless every such request
/// reported the measurement.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuxiliaryAccounting {
    pub requests: u32,
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub cost_usd: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TransportEvidence {
    pub seam: String,
    pub wire_format: String,
    pub exchanges: Vec<Exchange>,
    /// Reasons the capture itself is incomplete or ambiguous. Any entry fails the capture.
    pub capture_faults: Vec<String>,
    /// Requests refused by the proxy for lacking the per-trial placeholder credential.
    pub rejected_requests: u32,
    /// Proxied requests that are not inference (for example token counting).
    pub other_requests: u32,
    /// The runner saw the agent phase raise, time out or get cancelled.
    pub interrupted: bool,
    /// The first observed main request already carried assistant turns.
    pub resumed: bool,
    /// First request start to last response end, as measured by the proxy.
    pub wall_clock_ms: Option<u64>,
    /// Main-thread exchanges that reported no usage; their tokens are unknown, not zero.
    pub exchanges_without_usage: u32,
    pub auxiliary: AuxiliaryAccounting,
    /// Host-observed consumer stdout evidence, if the runner captured it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host_stdout: Option<HostStdoutEvidence>,
}

/// Evidence from the host-observed ExecResult.stdout of the CLI agent process.
///
/// The runner wraps `exec_as_agent` and parses the real `type=result` envelope
/// from Claude Code's `--output-format=stream-json` stdout. This is the host's
/// own observation of the public terminal reply, not an agent-editable file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HostStdoutEvidence {
    /// Identifies the capture seam (e.g. "host_stdout_stream_json").
    pub seam: String,
    /// SHA-256 of the terminal result text, if present.
    pub result_sha256: Option<String>,
    /// Faults detected in the host stdout observation.
    pub faults: Vec<String>,
}

/// What the recorded exchanges imply.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Derived {
    pub classification: InferenceClassification,
    pub finalization: FinalizationStatus,
    /// Kind of each main-thread turn, taken from its last attempt.
    pub turns: Vec<TurnKind>,
}

/// Classify a proxy capture. `final_text` says whether the last main exchange returned
/// non-empty public text; `max_turns` is the configured turn budget, if any.
pub fn derive(
    transport: &TransportEvidence,
    final_text: bool,
    max_turns: Option<i64>,
) -> Result<Derived, String> {
    let main: Vec<&Exchange> = transport
        .exchanges
        .iter()
        .filter(|exchange| exchange.role == ExchangeRole::Main)
        .collect();
    let result = |classification, finalization, turns| {
        Ok(Derived {
            classification,
            finalization,
            turns,
        })
    };
    if !transport.capture_faults.is_empty() {
        return result(
            InferenceClassification::FailedFinalization,
            FinalizationStatus::CaptureFailed,
            Vec::new(),
        );
    }
    let Some(terminal) = main.last() else {
        return result(
            InferenceClassification::EnvironmentStartupFailure,
            FinalizationStatus::NoResponse,
            Vec::new(),
        );
    };
    let ever_succeeded = main.iter().any(|exchange| exchange.failure.is_none());
    if !ever_succeeded
        && matches!(
            terminal.failure,
            Some(ExchangeFailure::Authentication | ExchangeFailure::InvalidRequest)
        )
    {
        if main.iter().any(|exchange| exchange.turn.is_some()) {
            return Err("an admission failure cannot carry inference turns".into());
        }
        let classification = if terminal.failure == Some(ExchangeFailure::InvalidRequest) {
            InferenceClassification::SchemaAdmissionFailure
        } else {
            InferenceClassification::EnvironmentStartupFailure
        };
        return result(classification, FinalizationStatus::NoResponse, Vec::new());
    }
    let mut turns: Vec<TurnKind> = Vec::new();
    for (position, exchange) in main.iter().enumerate() {
        let turn = exchange
            .turn
            .ok_or("a main-thread exchange lacks its turn")? as usize;
        let last_attempt = main
            .get(position + 1)
            .is_none_or(|next| next.turn != exchange.turn);
        if turn == turns.len() {
            turns.push(exchange.kind());
        } else if turn + 1 == turns.len() {
            turns[turn] = exchange.kind();
        } else {
            return Err("main-thread turns are not sequential".into());
        }
        if !last_attempt && !exchange.failure.is_some_and(ExchangeFailure::transient) {
            return Err("a retried attempt did not fail transiently".into());
        }
    }
    if transport.resumed {
        return result(
            InferenceClassification::ResumedSession,
            FinalizationStatus::NoResponse,
            turns,
        );
    }
    let retried = main
        .windows(2)
        .any(|pair| pair[0].turn == pair[1].turn && pair[0].turn.is_some());
    let no_response = FinalizationStatus::NoResponse;
    let (classification, finalization) = match terminal.failure {
        Some(ExchangeFailure::ContextLength) if terminal.turn == Some(0) => (
            InferenceClassification::FirstTurnContextRejection,
            no_response,
        ),
        Some(ExchangeFailure::ContextLength) => (
            InferenceClassification::LaterTurnContextRejection,
            no_response,
        ),
        Some(failure) if failure.transient() => {
            (InferenceClassification::UnrecoveredRateLimit, no_response)
        }
        Some(ExchangeFailure::MalformedResponse) => {
            (InferenceClassification::MalformedModelOutput, no_response)
        }
        Some(_) => (
            InferenceClassification::FailedFinalization,
            FinalizationStatus::CaptureFailed,
        ),
        None if transport.interrupted => (
            InferenceClassification::FailedFinalization,
            FinalizationStatus::Interrupted,
        ),
        None => match terminal.stop_reason.as_deref() {
            Some("end_turn" | "stop_sequence") if final_text && retried => (
                InferenceClassification::RecoveredRateLimit,
                FinalizationStatus::Normal,
            ),
            Some("end_turn" | "stop_sequence") if final_text => (
                InferenceClassification::Completed,
                FinalizationStatus::Normal,
            ),
            Some("max_tokens") => (InferenceClassification::BudgetExhausted, no_response),
            Some("tool_use" | "pause_turn")
                if max_turns.is_some_and(|budget| turns.len() as i64 >= budget) =>
            {
                (InferenceClassification::BudgetExhausted, no_response)
            }
            _ => (InferenceClassification::NoPublicResponse, no_response),
        },
    };
    result(classification, finalization, turns)
}

fn digest(value: &str) -> bool {
    crate::provenance::digest_valid(value)
}

/// Require a proxy capture document to say exactly what its recorded exchanges imply.
pub fn check(evidence: &InferenceEvidence) -> Result<(), String> {
    let transport = evidence
        .transport
        .as_ref()
        .ok_or("proxy capture lacks its provider exchanges")?;
    if transport.seam != PROXY_SEAM || transport.wire_format != PROXY_WIRE_FORMAT {
        return Err("unsupported provider transport seam".into());
    }
    for (position, exchange) in transport.exchanges.iter().enumerate() {
        if position > 0 && transport.exchanges[position - 1].sequence >= exchange.sequence {
            return Err("provider exchanges are not in capture order".into());
        }
        if !digest(&exchange.request_sha256)
            || exchange
                .response_sha256
                .as_deref()
                .is_some_and(|value| !digest(value))
            || exchange
                .cost_usd
                .is_some_and(|cost| !cost.is_finite() || cost < 0.0)
            || (exchange.role == ExchangeRole::Auxiliary && exchange.turn.is_some())
            || (exchange.failure.is_none() && !exchange.complete)
        {
            return Err("provider exchange identity is malformed".into());
        }
    }
    let final_text = evidence
        .final_response
        .as_ref()
        .and_then(|response| response.text.as_deref())
        .is_some_and(|text| !text.is_empty());
    let max_turns = match evidence.identity.budget.get("max_turns") {
        Some(SamplingValue::Integer(value)) => Some(*value),
        _ => None,
    };
    let derived = derive(transport, final_text, max_turns)?;
    if derived.classification != evidence.classification
        || derived.finalization != evidence.finalization
    {
        return Err("inference classification differs from its recorded provider exchanges".into());
    }
    if evidence.turns.len() != derived.turns.len() {
        return Err("inference turns differ from the recorded provider exchanges".into());
    }
    for (index, turn) in evidence.turns.iter().enumerate() {
        let attempts: Vec<&Exchange> = transport
            .exchanges
            .iter()
            .filter(|exchange| exchange.turn == Some(index as u32))
            .collect();
        let last = attempts
            .last()
            .ok_or("an inference turn has no provider exchange")?;
        let retries = attempts.len() as u32 - 1;
        if turn.kind != derived.turns[index]
            || turn.retry_count != Some(retries)
            || turn.retried != Some(retries > 0)
            || turn.response_id != last.response_id
            || turn.observed_model != last.observed_model
            || turn.input_tokens != last.prompt_tokens()
            || turn.output_tokens != last.output_tokens
            || turn.cost_usd != last.cost_usd
        {
            return Err("an inference turn differs from its provider exchange".into());
        }
    }
    let normal = derived.finalization == FinalizationStatus::Normal;
    match &evidence.final_response {
        Some(response)
            if normal
                && final_text
                && !response.resumed
                && response.turn as usize + 1 == evidence.turns.len() => {}
        None if !normal => {}
        _ => return Err("final response is not the last provider reply".into()),
    }
    // A completed proxy capture REQUIRES the host-observed CLI terminal carrier.
    // Missing or faulted host_stdout with Normal finalization is infrastructure
    // uncertainty, not a valid completed observation.
    if normal {
        let host = transport
            .host_stdout
            .as_ref()
            .ok_or("completed proxy capture lacks its host terminal carrier")?;
        if host.result_sha256.is_none() && final_text {
            return Err("completed proxy capture lacks a host terminal result digest".into());
        }
    }
    // Validate host stdout evidence when present.
    if let Some(host) = &transport.host_stdout {
        if host.seam != "host_stdout_stream_json" {
            return Err("unsupported host stdout capture seam".into());
        }
        if normal && !host.faults.is_empty() {
            return Err(format!(
                "host stdout capture has faults: {}",
                host.faults.join(", ")
            ));
        }
        if let Some(ref result_sha256) = host.result_sha256 {
            if !digest(result_sha256) {
                return Err("host stdout result digest is malformed".into());
            }
            // When both the provider and host stdout observed a final text,
            // verify they agree.
            if let Some(ref response) = evidence.final_response
                && let Some(ref text) = response.text
            {
                let expected = crate::runner::sha256(text.as_bytes());
                if *result_sha256 != expected {
                    return Err(
                        "host stdout result does not match the provider final response".into(),
                    );
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inference::*;
    use std::collections::BTreeMap;

    fn valid_digest() -> String {
        "sha256:".to_owned() + &"a".repeat(64)
    }

    #[test]
    fn provider_failure_is_not_erased_by_an_unsuccessful_cli_terminal_carrier() {
        let mut evidence: InferenceEvidence = serde_json::from_str(include_str!(
            "../tests/fixtures/proxy-capture/first_turn_context_rejection.json"
        ))
        .unwrap();
        evidence.transport.as_mut().unwrap().host_stdout = Some(HostStdoutEvidence {
            seam: "host_stdout_stream_json".into(),
            result_sha256: Some(crate::runner::sha256(b"Context window exceeded")),
            faults: vec![
                "host_stdout_nonzero_exit".into(),
                "host_stdout_result_is_error".into(),
                "host_stdout_result_subtype_not_success".into(),
            ],
        });
        check(&evidence).unwrap();
        evidence.classification = InferenceClassification::Completed;
        assert!(check(&evidence).is_err());
    }

    fn normal_evidence_with_host_stdout(
        host_stdout: Option<HostStdoutEvidence>,
    ) -> InferenceEvidence {
        let final_text = "Maya";
        InferenceEvidence {
            schema_version: INFERENCE_EVIDENCE_SCHEMA_VERSION,
            adapter: AdapterIdentity {
                name: PROXY_ADAPTER_NAME.into(),
                code_sha256: valid_digest(),
            },
            classification: InferenceClassification::Completed,
            turns: vec![InferenceTurn {
                turn: 0,
                kind: TurnKind::FinalResponse,
                input_tokens: Some(100),
                output_tokens: Some(50),
                retried: Some(false),
                retry_count: Some(0),
                cost_usd: None,
                observed_model: None,
                response_id: Some("msg_1".into()),
            }],
            final_response: Some(FinalResponse {
                text: Some(final_text.into()),
                turn: 0,
                finalization: FinalizationStatus::Normal,
                resumed: false,
            }),
            cost: CostAccounting::default(),
            identity: InferenceIdentity {
                configured_model: "test".into(),
                configured_provider: "test".into(),
                observed_model: None,
                observed_revision: None,
                sampling: BTreeMap::new(),
                budget: BTreeMap::new(),
            },
            scripted: false,
            finalization: FinalizationStatus::Normal,
            scenario: None,
            session_id: None,
            transport: Some(TransportEvidence {
                seam: PROXY_SEAM.into(),
                wire_format: PROXY_WIRE_FORMAT.into(),
                exchanges: vec![Exchange {
                    sequence: 0,
                    role: ExchangeRole::Main,
                    turn: Some(0),
                    request_sha256: valid_digest(),
                    response_sha256: Some(valid_digest()),
                    status: Some(200),
                    streamed: true,
                    complete: true,
                    latency_ms: Some(100),
                    first_byte_ms: Some(10),
                    response_id: Some("msg_1".into()),
                    observed_model: None,
                    stop_reason: Some("end_turn".into()),
                    failure: None,
                    input_tokens: Some(100),
                    cache_read_input_tokens: None,
                    cache_creation_input_tokens: None,
                    output_tokens: Some(50),
                    cost_usd: None,
                }],
                capture_faults: Vec::new(),
                rejected_requests: 0,
                other_requests: 0,
                interrupted: false,
                resumed: false,
                wall_clock_ms: Some(100),
                exchanges_without_usage: 0,
                auxiliary: AuxiliaryAccounting::default(),
                host_stdout,
            }),
        }
    }

    #[test]
    fn normal_with_valid_host_stdout_passes() {
        let final_text = "Maya";
        let result_sha256 = crate::runner::sha256(final_text.as_bytes());
        let evidence = normal_evidence_with_host_stdout(Some(HostStdoutEvidence {
            seam: "host_stdout_stream_json".into(),
            result_sha256: Some(result_sha256),
            faults: Vec::new(),
        }));
        assert!(check(&evidence).is_ok());
    }

    #[test]
    fn normal_without_host_stdout_rejected() {
        let evidence = normal_evidence_with_host_stdout(None);
        let err = check(&evidence).unwrap_err();
        assert!(err.contains("host terminal carrier"), "{err}");
    }

    #[test]
    fn normal_with_null_digest_rejected() {
        let evidence = normal_evidence_with_host_stdout(Some(HostStdoutEvidence {
            seam: "host_stdout_stream_json".into(),
            result_sha256: None,
            faults: Vec::new(),
        }));
        let err = check(&evidence).unwrap_err();
        assert!(err.contains("result digest"), "{err}");
    }

    #[test]
    fn normal_with_wrong_digest_rejected() {
        let wrong = crate::runner::sha256(b"Mallory");
        let evidence = normal_evidence_with_host_stdout(Some(HostStdoutEvidence {
            seam: "host_stdout_stream_json".into(),
            result_sha256: Some(wrong),
            faults: Vec::new(),
        }));
        let err = check(&evidence).unwrap_err();
        assert!(err.contains("does not match"), "{err}");
    }

    #[test]
    fn normal_with_faulted_host_stdout_rejected() {
        let final_text = "Maya";
        let result_sha256 = crate::runner::sha256(final_text.as_bytes());
        let evidence = normal_evidence_with_host_stdout(Some(HostStdoutEvidence {
            seam: "host_stdout_stream_json".into(),
            result_sha256: Some(result_sha256),
            faults: vec!["host_stdout_malformed_json".into()],
        }));
        let err = check(&evidence).unwrap_err();
        assert!(err.contains("faults"), "{err}");
    }
}
