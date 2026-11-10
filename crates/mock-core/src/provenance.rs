use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskContext {
    pub schema_version: u32,
    pub scenario: String,
    pub instruction_sha256: String,
    pub profile_sha256: String,
    pub framing_sha256: String,
    pub environment_files: BTreeMap<String, String>,
    pub world_reference: String,
    pub tools_reference: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub review_policy_sha256: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub connector_env_sha256: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Implementation {
    pub name: String,
    pub version: String,
    pub code_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImageObservation {
    pub container_id: String,
    pub image_id: String,
    pub reference: String,
    pub resource_limits: BTreeMap<String, i64>,
    pub filesystem_sha256: String,
    pub runtime_config_sha256: String,
    pub config_components: BTreeMap<String, String>,
    pub orchestration_project: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub connector_env_sha256: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutionManifest {
    pub schema_version: u32,
    pub runner_protocol: String,
    pub verification_protocol: String,
    pub scenario: String,
    pub session_id: String,
    pub task_source_sha256: String,
    pub task_context_sha256: String,
    pub context: TaskContext,
    pub execution_kind: String,
    pub python_version: String,
    pub agent: Implementation,
    pub harness: Implementation,
    pub agent_timeout_sec: f64,
    pub verifier_timeout_sec: f64,
    pub images: BTreeMap<String, ImageObservation>,
}

impl ExecutionManifest {
    pub fn validate(
        &self,
        scenario: &str,
        context: &TaskContext,
        source: &[u8],
        context_bytes: &[u8],
    ) -> Result<(), String> {
        if self.schema_version != 1
            || self.runner_protocol != crate::runner::RUNNER_PROTOCOL
            || self.verification_protocol != crate::runner::VERIFICATION_PROTOCOL
            || self.scenario != scenario
            || self.session_id.trim().is_empty()
            || self.task_source_sha256 != crate::runner::sha256(source)
            || self.task_context_sha256 != crate::runner::sha256(context_bytes)
            || &self.context != context
            || context.schema_version != 1
            || context.scenario != scenario
            || ![
                &context.instruction_sha256,
                &context.profile_sha256,
                &context.framing_sha256,
            ]
            .into_iter()
            .all(|value| digest_valid(value))
            || context.environment_files.is_empty()
            || context
                .review_policy_sha256
                .as_ref()
                .is_some_and(|value| !digest_valid(value))
            || context
                .environment_files
                .values()
                .any(|value| !digest_valid(value))
        {
            return Err("execution manifest does not match materialized task sources".into());
        }
        if self.execution_kind != "scripted"
            || self.python_version.is_empty()
            || !matches!(self.agent.name.as_str(), "oracle" | "nop")
            || self.harness.name != "harbor"
            || self.harness.version != "0.23.0"
            || [&self.agent, &self.harness]
                .into_iter()
                .any(|value| value.version.is_empty() || !digest_valid(&value.code_sha256))
            || !self.agent_timeout_sec.is_finite()
            || self.agent_timeout_sec <= 0.0
            || !self.verifier_timeout_sec.is_finite()
            || self.verifier_timeout_sec <= 0.0
        {
            return Err("execution producer/runtime is unsupported or unverified".into());
        }
        if self.images.len() != 3 {
            return Err(
                "execution manifest lacks the complete approved container inventory".into(),
            );
        }
        let mut containers = std::collections::BTreeSet::new();
        for role in ["main", "world", "tools"] {
            let image = self
                .images
                .get(role)
                .ok_or("execution image role is missing")?;
            if !containers.insert(&image.container_id)
                || image.container_id.len() != 64
                || !image
                    .container_id
                    .bytes()
                    .all(|byte| byte.is_ascii_hexdigit())
                || !digest_valid(&image.image_id)
                || !digest_valid(&image.filesystem_sha256)
                || !digest_valid(&image.runtime_config_sha256)
                || image.config_components.is_empty()
                || image
                    .config_components
                    .values()
                    .any(|value| !digest_valid(value))
                || image.reference.is_empty()
                || !["memory", "nano_cpus", "cpu_quota", "cpu_period"]
                    .into_iter()
                    .all(|key| image.resource_limits.contains_key(key))
            {
                return Err(
                    "execution image identity or resource observations are incomplete".into(),
                );
            }
        }
        if self.images["world"].reference != context.world_reference
            || self.images["tools"].reference != context.tools_reference
        {
            return Err("observed sidecar image references differ from the declared task".into());
        }
        if context
            .connector_env_sha256
            .values()
            .any(|value| !digest_valid(value))
            || self.images["tools"].connector_env_sha256 != context.connector_env_sha256
            || !self.images["main"].connector_env_sha256.is_empty()
            || !self.images["world"].connector_env_sha256.is_empty()
        {
            return Err(
                "declared connector configuration differs from actual tools-container observations"
                    .into(),
            );
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DatasetSource {
    pub schema_version: u32,
    pub scenario: String,
    pub scenario_sha256: String,
    pub dataset_sha256: String,
    pub contract_sha256: String,
}

pub fn digest_valid(value: &str) -> bool {
    value.strip_prefix("sha256:").is_some_and(|digest| {
        digest.len() == 64
            && digest
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    })
}

pub fn canonical_sha256(value: &impl Serialize) -> String {
    fn canonical(value: Value) -> Value {
        match value {
            Value::Object(values) => {
                let mut entries: Vec<_> = values.into_iter().collect();
                entries.sort_by(|left, right| left.0.cmp(&right.0));
                Value::Object(
                    entries
                        .into_iter()
                        .map(|(key, value)| (key, canonical(value)))
                        .collect::<Map<_, _>>(),
                )
            }
            Value::Array(values) => Value::Array(values.into_iter().map(canonical).collect()),
            value => value,
        }
    }
    let value =
        canonical(serde_json::to_value(value).expect("provenance metadata is JSON serializable"));
    crate::runner::sha256(
        &serde_json::to_vec(&value).expect("canonical metadata is JSON serializable"),
    )
}

impl DatasetSource {
    pub fn validate(&self, scenario: &str) -> Result<(), String> {
        if self.schema_version != 1
            || self.scenario != scenario
            || !digest_valid(&self.scenario_sha256)
            || !digest_valid(&self.dataset_sha256)
            || !digest_valid(&self.contract_sha256)
        {
            return Err("world source provenance is missing, malformed or cross-scenario".into());
        }
        Ok(())
    }
}
