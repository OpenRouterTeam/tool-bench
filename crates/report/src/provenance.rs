use std::{fs, path::Path};

use anyhow::{Context, bail};
use mock_core::{
    evidence::WorldEvidence,
    grading::{Expect, Grade},
    provenance::{DatasetSource, ExecutionManifest, TaskContext, canonical_sha256},
    runner::{RunnerAdmission, RunnerPolicy, sha256},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Provenance {
    pub evaluator_version: u32,
    pub evidence_schema_version: u32,
    pub source: DatasetSource,
    pub initial_state_sha256: String,
    pub runner_adapter_sha256: String,
    pub manifest: ExecutionManifest,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inference: Option<mock_core::inference::InferenceEvidence>,
}

impl Provenance {
    pub(crate) fn release_signature(&self) -> String {
        let mut release = json!({
            "evaluator": self.evaluator_version, "evidence_schema": self.evidence_schema_version,
            "runner": self.runner_adapter_sha256, "harness": self.manifest.harness,
            "python_version": self.manifest.python_version,
            "world_image": self.manifest.images["world"].image_id,
            "tools_image": self.manifest.images["tools"].image_id
        });
        if let Some(policy) = &self.manifest.context.review_policy_sha256 {
            release["review_policy_sha256"] = json!(policy);
        }
        if let Some(inference) = &self.inference {
            release["inference_capture"] = json!({
                "schema": inference.schema_version,
                "code_sha256": inference.adapter.code_sha256,
            });
        }
        canonical_sha256(&release)
    }

    pub(crate) fn compatibility(&self, allow_agent_difference: bool) -> String {
        let images: serde_json::Map<_, _> = self
            .manifest
            .images
            .iter()
            .map(|(role, image)| {
                let mut descriptor = json!({
                    "image_id": if role == "main" { None } else { Some(&image.image_id) },
                    "filesystem_sha256": image.filesystem_sha256,
                    "runtime_config_sha256": image.runtime_config_sha256,
                    "resource_limits": image.resource_limits
                });
                if !image.connector_env_sha256.is_empty() {
                    descriptor["connector_env_sha256"] = json!(image.connector_env_sha256);
                }
                (role.clone(), descriptor)
            })
            .collect();
        canonical_sha256(&json!({
            "evaluator": self.evaluator_version, "evidence_schema": self.evidence_schema_version,
            "source": self.source, "initial": self.initial_state_sha256,
            "runner": self.runner_adapter_sha256, "harness": self.manifest.harness,
            "python_version": self.manifest.python_version,
            "agent": if allow_agent_difference { Value::Null } else { json!(self.manifest.agent) },
            "inference_condition": if allow_agent_difference { Value::Null } else { json!(self.manifest.inference_identity) },
            "context": self.manifest.context, "images": images,
            "agent_timeout_sec": self.manifest.agent_timeout_sec,
            "verifier_timeout_sec": self.manifest.verifier_timeout_sec
        }))
    }
}

pub(crate) fn read(path: &Path) -> anyhow::Result<Vec<u8>> {
    if path.is_symlink() {
        bail!(
            "provenance inputs must not be symbolic links: {}",
            path.display()
        );
    }
    fs::read(path).with_context(|| format!("reading provenance input {}", path.display()))
}

pub(crate) fn load(directory: &Path, result: &Value) -> anyhow::Result<Option<Provenance>> {
    Ok(load_verified(directory, result)?.map(|verified| verified.provenance))
}

pub(crate) struct VerifiedAssessment {
    pub provenance: Provenance,
    pub assessment: Grade,
    pub contract: Expect,
    pub evidence: WorldEvidence,
    pub answer: Option<String>,
    pub bindings: std::collections::BTreeMap<String, String>,
}

pub(crate) fn load_verified(
    directory: &Path,
    result: &Value,
) -> anyhow::Result<Option<VerifiedAssessment>> {
    let verifier = directory.join("verifier");
    if !verifier.join("verification.json").exists() {
        return Ok(None);
    }
    let verification_bytes = read(&verifier.join("verification.json"))?;
    let verification: Value = serde_json::from_slice(&verification_bytes)?;
    if verification["schema_version"] != 3 {
        return Ok(None);
    }
    if verification["task_source_checked"] != true
        || verification["runner_admission_checked"] != true
        || verification["runner_admission_ok"] != true
    {
        bail!("assessment lacks checked source and execution provenance");
    }
    let input = |name: &str, hash: &str| -> anyhow::Result<Vec<u8>> {
        let bytes = read(&verifier.join(name))?;
        if verification[hash].as_str() != Some(sha256(&bytes).as_str()) {
            bail!("assessment provenance byte mismatch for {name}");
        }
        Ok(bytes)
    };
    let contract_bytes = input("contract.json", "contract_sha256")?;
    let source_bytes = input("task-source.json", "task_source_sha256")?;
    let context_bytes = input("task-context.json", "task_context_sha256")?;
    let policy_bytes = input("runner-policy.json", "runner_policy_sha256")?;
    let admission_bytes = input("runner-admission.json", "runner_admission_sha256")?;
    let manifest_bytes = input("realized-manifest.json", "manifest_sha256")?;
    let evidence_bytes = read(&directory.join("artifacts/world/evidence.json"))?;
    if verification["evidence_sha256"].as_str() != Some(sha256(&evidence_bytes).as_str()) {
        bail!("assessment evidence differs from the exact collected world artifact");
    }
    let answer = if verification["answer_sha256"].is_null() {
        None
    } else {
        let bytes = read(&directory.join("artifacts/agent/answer.md"))?;
        if verification["answer_sha256"].as_str() != Some(sha256(&bytes).as_str()) {
            bail!("assessment answer bytes changed after collection");
        }
        Some(bytes)
    };
    let contract: Expect = serde_json::from_slice(&contract_bytes)?;
    let source: DatasetSource = serde_json::from_slice(&source_bytes)?;
    let context: TaskContext = serde_json::from_slice(&context_bytes)?;
    let policy: RunnerPolicy = serde_json::from_slice(&policy_bytes)?;
    let admission: RunnerAdmission = serde_json::from_slice(&admission_bytes)?;
    let manifest: ExecutionManifest = serde_json::from_slice(&manifest_bytes)?;
    let evidence: WorldEvidence = serde_json::from_slice(&evidence_bytes)?;
    if evidence.evaluator_version != mock_core::grading::EVALUATOR_VERSION {
        bail!(
            "unsupported evaluator revision {}; use its matching release for reproduction",
            evidence.evaluator_version
        );
    }
    let scenario = verification["scenario"]
        .as_str()
        .context("verification scenario is missing")?;
    if result["task_name"].as_str().is_none_or(|name| {
        name.rsplit('/')
            .next()
            .unwrap_or(name)
            .trim_start_matches("tool-bench__")
            != scenario.replace('/', "__")
    }) {
        bail!("host task identity does not match the collected scenario");
    }
    source.validate(scenario).map_err(anyhow::Error::msg)?;
    let materialization = mock_core::materialization::bundle_from_directory(
        &verifier,
        &context.materialization_files,
    )
    .map_err(anyhow::Error::msg)?;
    mock_core::materialization::validate_bundle(
        &source,
        &context.materialization_files,
        &materialization,
    )
    .map_err(anyhow::Error::msg)?;
    if source != evidence.source
        || verification["contract_semantic_sha256"] != canonical_sha256(&contract)
        || verification["initial_state_sha256"] != evidence.initial_state_sha256
        || verification["evaluator_version"] != evidence.evaluator_version
        || verification["evidence_schema_version"] != evidence.schema_version
    {
        bail!("assessment source/version metadata is inconsistent");
    }
    admission
        .validate(
            &policy,
            scenario,
            &contract_bytes,
            &evidence_bytes,
            answer.as_deref(),
        )
        .map_err(anyhow::Error::msg)?;
    manifest
        .validate(scenario, &context, &source_bytes, &context_bytes)
        .map_err(anyhow::Error::msg)?;
    if admission.manifest_sha256.as_deref() != Some(sha256(&manifest_bytes).as_str())
        || admission.session_id != manifest.session_id
        || result["trial_name"]
            .as_str()
            .is_none_or(|name| manifest.session_id != format!("{name}__env"))
        || admission
            .main
            .as_ref()
            .is_none_or(|main| main.container_id != manifest.images["main"].container_id)
    {
        bail!("execution manifest is not bound to the collected runner proof");
    }
    if result["agent_info"]["name"].as_str() != Some(manifest.agent.name.as_str()) {
        bail!("host result agent identity does not match its observed execution");
    }
    let inference_bytes = if verification["inference_evidence_sha256"].is_string() {
        let bytes = input("inference-evidence.json", "inference_evidence_sha256")?;
        if bytes != read(&directory.join("artifacts/world/inference-evidence.json"))? {
            bail!("inference evidence differs from the exact collected artifact");
        }
        Some(bytes)
    } else {
        None
    };
    let inference = mock_core::inference::load_bound(
        inference_bytes.as_deref(),
        &policy,
        &admission,
        &manifest,
    )
    .map_err(anyhow::Error::msg)?;
    let native = inference.as_ref().filter(|value| !value.scripted);
    let use_native_final = native.is_some()
        && contract
            .answer
            .as_ref()
            .is_some_and(|answer| answer.source == mock_core::grading::AnswerSource::FinalResponse);
    let answer_text = if let Some(native) = native.filter(|_| use_native_final) {
        native.final_text()
    } else {
        answer.as_deref().map(std::str::from_utf8).transpose()?
    };
    if let Some(inference) = &inference {
        let expected_source = if use_native_final {
            json!("native_final_response")
        } else if answer.is_some() {
            json!("legacy_answer_file")
        } else {
            Value::Null
        };
        if verification["inference_classification"] != json!(inference.classification)
            || verification["answer_source"] != expected_source
            || verification["graded_answer_sha256"]
                != json!(answer_text.map(|text| sha256(text.as_bytes())))
        {
            bail!("inference classification or graded-answer provenance is inconsistent");
        }
    }
    let mut assessed = evidence.assess(
        &contract,
        answer_text,
        verification["context_overflow"] == true,
    );
    if let Some(inference) = &inference {
        inference.apply_to_grade(&mut assessed);
    }
    assessed.admit_runner(Ok(()));
    let saved_bytes = read(&verifier.join("assessment.json"))?;
    let saved: Value = serde_json::from_slice(&saved_bytes)?;
    if canonical_sha256(&assessed) != canonical_sha256(&saved) {
        bail!("saved assessment does not reproduce from its bound inputs");
    }
    let reported = &result["verifier_result"]["rewards"];
    let metrics = assessed.metrics();
    for key in [
        "reward",
        "outcome_code",
        "evaluator_version",
        "contract_version",
        "runner_admission_checked",
        "runner_admission_ok",
        "environment_ok",
        "contract_ok",
    ] {
        if reported[key].as_f64() != Some(metrics[key]) {
            bail!("host reward metrics do not match the verified assessment at {key}");
        }
    }
    let selected_answer = answer_text.map(str::to_owned);
    let materialization_bindings = context.materialization_files.clone();
    let provenance = Provenance {
        evaluator_version: evidence.evaluator_version,
        evidence_schema_version: evidence.schema_version,
        source,
        initial_state_sha256: evidence.initial_state_sha256.clone(),
        runner_adapter_sha256: policy.adapter_sha256,
        manifest,
        inference,
    };
    let mut bindings: std::collections::BTreeMap<String, String> = [
        ("assessment", sha256(&saved_bytes)),
        ("contract", sha256(&contract_bytes)),
        ("evidence", sha256(&evidence_bytes)),
        ("manifest", sha256(&manifest_bytes)),
        ("runner-admission", sha256(&admission_bytes)),
        ("source", sha256(&source_bytes)),
        ("task-context", sha256(&context_bytes)),
        ("verification", sha256(&verification_bytes)),
    ]
    .into_iter()
    .map(|(name, value)| (name.into(), value))
    .collect();
    if let Some(bytes) = &inference_bytes {
        bindings.insert("inference".into(), sha256(bytes));
    }
    bindings.extend(materialization_bindings);
    Ok(Some(VerifiedAssessment {
        provenance,
        assessment: assessed,
        contract,
        evidence,
        answer: selected_answer,
        bindings,
    }))
}
