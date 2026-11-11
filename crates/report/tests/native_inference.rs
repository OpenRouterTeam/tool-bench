use std::{fs, path::Path};

use mock_core::runner::sha256;
use report::{compare_declared, load_trials};
use serde_json::{Value, json};

mod support;

fn read(root: &Path, name: &str) -> Value {
    serde_json::from_slice(&fs::read(root.join(name)).unwrap()).unwrap()
}

fn write(root: &Path, name: &str, value: &Value) {
    fs::write(root.join(name), serde_json::to_vec(value).unwrap()).unwrap();
}

fn native_fixture(root: &Path, model: &str, passes: bool) {
    support::write_verified_final_response(root, "trial", "example__github__basic", passes);
    let trial = root.join("trial");
    let verifier = trial.join("verifier");
    let mut manifest = read(&verifier, "realized-manifest.json");
    let configured = json!({
        "configured_model": model, "configured_provider": "unknown",
        "observed_model": null, "observed_revision": null, "sampling": {}, "budget": {"max_turns": 2},
    });
    manifest["execution_kind"] = json!("native");
    manifest["agent"]["name"] = json!("computer-1");
    manifest["inference_identity"] = configured.clone();
    write(&verifier, "realized-manifest.json", &manifest);
    let adapter = json!({
        "name": mock_core::inference::COMPUTER1_ADAPTER_NAME,
        "code_sha256": sha256(b"offline-capture-fixture"),
    });
    let public = if passes { "Maya" } else { "wrong" };
    let inference = json!({
        "schema_version": 2, "adapter": adapter, "classification": "completed", "finalization": "normal",
        "scenario": manifest["scenario"], "session_id": manifest["session_id"],
        "turns": [{
            "turn": 0, "kind": "final_response", "input_tokens": 10, "output_tokens": 3,
            "retried": null, "retry_count": null, "cost_usd": null,
        }],
        "final_response": {"text": public, "turn": 0, "finalization": "normal", "resumed": false},
        "cost": {
            "total_input_tokens": 10, "total_output_tokens": 3, "total_cost_usd": null,
            "retry_input_tokens": null, "retry_output_tokens": null, "retry_cost_usd": null,
            "successful_turns": 1, "retried_turns": null, "total_retry_attempts": null,
        },
        "identity": configured, "scripted": false,
    });
    write(&verifier, "inference-evidence.json", &inference);
    write(
        &trial,
        "artifacts/world/inference-evidence.json",
        &inference,
    );
    fs::write(
        trial.join("artifacts/agent/answer.md"),
        if passes {
            "actor file is wrong"
        } else {
            "Maya"
        },
    )
    .unwrap();
    let mut policy = read(&verifier, "runner-policy.json");
    policy["inference_adapter"] = adapter;
    write(&verifier, "runner-policy.json", &policy);
    let mut admission = read(&verifier, "runner-admission.json");
    admission["inference_evidence_sha256"] = json!(sha256(
        &fs::read(verifier.join("inference-evidence.json")).unwrap()
    ));
    admission["manifest_sha256"] = json!(sha256(
        &fs::read(verifier.join("realized-manifest.json")).unwrap()
    ));
    admission["answer_sha256"] = json!(sha256(
        &fs::read(trial.join("artifacts/agent/answer.md")).unwrap()
    ));
    write(&verifier, "runner-admission.json", &admission);
    let mut verification = read(&verifier, "verification.json");
    verification["inference_evidence_sha256"] = admission["inference_evidence_sha256"].clone();
    verification["manifest_sha256"] = admission["manifest_sha256"].clone();
    verification["answer_sha256"] = admission["answer_sha256"].clone();
    verification["runner_admission_sha256"] = json!(sha256(
        &fs::read(verifier.join("runner-admission.json")).unwrap()
    ));
    verification["runner_policy_sha256"] = json!(sha256(
        &fs::read(verifier.join("runner-policy.json")).unwrap()
    ));
    verification["inference_classification"] = json!("completed");
    verification["answer_source"] = json!("native_final_response");
    verification["graded_answer_sha256"] = json!(sha256(public.as_bytes()));
    write(&verifier, "verification.json", &verification);
    let mut result = read(&trial, "result.json");
    result["agent_info"]["name"] = json!("computer-1");
    write(&trial, "result.json", &result);
}

#[test]
fn report_reproduces_the_host_final_answer_instead_of_the_actor_file() {
    for passes in [true, false] {
        let directory = tempfile::tempdir().unwrap();
        native_fixture(directory.path(), "offline-fixture-model", passes);
        let trials = load_trials(directory.path()).unwrap();
        assert_eq!(trials.len(), 1);
        assert_eq!(trials[0].reward, if passes { 1.0 } else { 0.0 });
        let captured = trials[0]
            .provenance
            .as_ref()
            .unwrap()
            .inference
            .as_ref()
            .unwrap();
        assert!(!captured.scripted);
        assert_eq!(
            captured.final_text(),
            Some(if passes { "Maya" } else { "wrong" })
        );
    }
}

#[test]
fn native_costs_classifications_and_declared_model_treatment_survive_comparison() {
    let left = tempfile::tempdir().unwrap();
    let right = tempfile::tempdir().unwrap();
    native_fixture(left.path(), "offline-model-a", true);
    native_fixture(right.path(), "offline-model-b", true);
    let left_trials = load_trials(left.path()).unwrap();
    let right_trials = load_trials(right.path()).unwrap();
    assert!(compare_declared(&left_trials, &right_trials, false).is_err());
    let comparison = compare_declared(&left_trials, &right_trials, true).unwrap();
    assert_eq!(comparison.admission_level, "source_bound_host_capture");
    assert_eq!(
        comparison.inference_a[0].identity.configured_model,
        "offline-model-a"
    );
    let encoded = serde_json::to_value(&comparison).unwrap();
    assert_eq!(encoded["inference_a"][0]["classification"], "completed");
    assert!(encoded["inference_a"][0]["cost"]["total_cost_usd"].is_null());
    assert!(encoded["inference_a"][0]["cost"]["total_retry_attempts"].is_null());
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_bench-report"))
        .args(["--trials", left.path().to_str().unwrap(), "--json"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let summary: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(summary["format"], "verified_trial_summary_v1");
    assert_eq!(
        summary["trials"][0]["inference"]["classification"],
        "completed"
    );
    assert!(summary["trials"][0]["inference"]["cost"]["total_cost_usd"].is_null());
}

#[test]
fn native_artifact_and_classification_substitution_fail_report_loading() {
    for mutation in ["artifact", "copy", "classification", "source"] {
        let directory = tempfile::tempdir().unwrap();
        native_fixture(directory.path(), "offline-fixture-model", true);
        let trial = directory.path().join("trial");
        match mutation {
            "artifact" => {
                fs::write(trial.join("artifacts/world/inference-evidence.json"), "{}").unwrap()
            }
            "copy" => fs::write(trial.join("verifier/inference-evidence.json"), "{}").unwrap(),
            _ => {
                let mut verification = read(&trial, "verifier/verification.json");
                if mutation == "classification" {
                    verification["inference_classification"] = json!("policy_refusal");
                } else {
                    verification["answer_source"] = json!("legacy_answer_file");
                }
                write(&trial, "verifier/verification.json", &verification);
            }
        }
        assert!(load_trials(directory.path()).is_err(), "{mutation}");
    }
}
