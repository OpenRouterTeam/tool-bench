use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const RUNNER_PROTOCOL: &str = "tool-bench-runner-v3";
pub const VERIFICATION_PROTOCOL: &str = "offline-world-evidence-v5";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunnerPolicy {
    pub schema_version: u32,
    pub runner_protocol: String,
    pub verification_protocol: String,
    pub harbor_version: String,
    pub adapter_sha256: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inference_adapter: Option<crate::inference::AdapterIdentity>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunnerAdmission {
    pub schema_version: u32,
    pub runner_protocol: String,
    pub verification_protocol: String,
    pub scenario: String,
    pub session_id: String,
    pub harbor_version: String,
    pub adapter_sha256: String,
    pub contract_sha256: String,
    pub evidence_sha256: Option<String>,
    pub main_artifacts_sha256: Option<String>,
    pub answer_sha256: Option<String>,
    pub manifest_sha256: Option<String>,
    #[serde(default)]
    pub inference_evidence_sha256: Option<String>,
    pub phase: String,
    pub failure: Option<String>,
    pub main: Option<MainState>,
    pub collection_checks: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MainState {
    pub container_id: String,
    pub status: String,
    pub running: bool,
    pub paused: bool,
    pub restarting: bool,
    pub started_at: String,
    pub finished_at: String,
}

fn hexadecimal_digest(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

pub fn sha256(bytes: &[u8]) -> String {
    format!(
        "sha256:{}",
        Sha256::digest(bytes)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    )
}

impl RunnerAdmission {
    pub fn validate(
        &self,
        policy: &RunnerPolicy,
        scenario: &str,
        contract: &[u8],
        evidence: &[u8],
        answer: Option<&[u8]>,
    ) -> Result<(), String> {
        if policy.schema_version != 1
            || policy.runner_protocol != RUNNER_PROTOCOL
            || policy.verification_protocol != VERIFICATION_PROTOCOL
            || policy.harbor_version.is_empty()
            || !hexadecimal_digest(&policy.adapter_sha256)
        {
            return Err("unsupported or malformed runner admission policy".into());
        }
        if self.schema_version != 1
            || self.runner_protocol != policy.runner_protocol
            || self.verification_protocol != policy.verification_protocol
            || self.harbor_version != policy.harbor_version
            || self.adapter_sha256 != policy.adapter_sha256
        {
            return Err("runner admission does not match the approved implementation".into());
        }
        if self.scenario != scenario
            || self.session_id.trim().is_empty()
            || self.contract_sha256 != sha256(contract)
            || self.evidence_sha256.as_deref() != Some(sha256(evidence).as_str())
            || self.answer_sha256 != answer.map(sha256)
            || !self
                .manifest_sha256
                .as_deref()
                .is_some_and(crate::provenance::digest_valid)
            || !self.main_artifacts_sha256.as_deref().is_some_and(|value| {
                value
                    .strip_prefix("sha256:")
                    .is_some_and(hexadecimal_digest)
            })
        {
            return Err("runner admission is not bound to this task and collected evidence".into());
        }
        let main = self
            .main
            .as_ref()
            .ok_or("runner admission has no independent main-container state")?;
        if self.phase != "stopped"
            || self.failure.is_some()
            || self.collection_checks == 0
            || !hexadecimal_digest(&main.container_id)
            || main.running
            || main.paused
            || main.restarting
            || main.started_at.is_empty()
            || main.finished_at.is_empty()
            || main.started_at.starts_with("0001-")
            || main.finished_at.starts_with("0001-")
            || !matches!(main.status.as_str(), "exited" | "dead")
        {
            return Err("runner admission does not prove quiescent agent shutdown".into());
        }
        Ok(())
    }
}
