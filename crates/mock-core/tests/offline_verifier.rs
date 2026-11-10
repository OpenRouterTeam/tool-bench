use std::{fs, path::Path, process::Command};

use mock_core::{dataset::Dataset, evidence::WorldEvidence, world::WorldState};
use serde_json::{Value, json};

fn fixture(directory: &Path) -> String {
    fixture_with_answer_source(directory, None)
}

fn fixture_with_answer_source(directory: &Path, answer_source: Option<&str>) -> String {
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/example/github/basic");
    let mut dataset = Dataset::load(&source).unwrap();
    let mut definition =
        json!({"version": 2, "audit_history": true, "answer": {"contains": ["Maya"]}});
    if let Some(source) = answer_source {
        definition["answer"]["source"] = json!(source);
    }
    dataset.contract = serde_json::from_value(definition.clone()).unwrap();
    let mut world = WorldState::from_dataset(dataset).unwrap();
    world.seal().unwrap();
    let evidence = WorldEvidence::capture(&world, &[], Ok(vec!["192.0.2.10".into()]));
    fs::write(
        directory.join("evidence.json"),
        serde_json::to_vec(&evidence).unwrap(),
    )
    .unwrap();
    fs::write(directory.join("contract.json"), definition.to_string()).unwrap();
    fs::write(directory.join("answer.md"), "Maya").unwrap();
    fs::write(
        directory.join("task-source.json"),
        serde_json::to_vec(world.source()).unwrap(),
    )
    .unwrap();
    let source_bytes = fs::read(directory.join("task-source.json")).unwrap();
    let context = json!({
        "schema_version": 1, "scenario": world.meta.id,
        "instruction_sha256": mock_core::runner::sha256(b"instruction"),
        "profile_sha256": mock_core::runner::sha256(b"profile"),
        "framing_sha256": mock_core::runner::sha256(b"framing"),
        "environment_files": {"Dockerfile": mock_core::runner::sha256(b"source")},
        "world_reference": "world:fixture", "tools_reference": "tools:fixture"
    });
    let context_bytes = serde_json::to_vec(&context).unwrap();
    fs::write(directory.join("task-context.json"), &context_bytes).unwrap();
    let mut images = serde_json::Map::new();
    for role in ["main", "world", "tools"] {
        images.insert(role.into(), json!({
            "container_id": match role { "main" => "b".repeat(64), "world" => "c".repeat(64), _ => "d".repeat(64) },
            "image_id": mock_core::runner::sha256(role.as_bytes()), "reference": format!("{role}:fixture"),
            "filesystem_sha256": mock_core::runner::sha256(format!("filesystem-{role}").as_bytes()),
            "runtime_config_sha256": mock_core::runner::sha256(format!("configuration-{role}").as_bytes()),
            "config_components": {"Env": mock_core::runner::sha256(b"environment")}, "orchestration_project": null,
            "resource_limits": {"memory": 1024, "nano_cpus": 1_000_000_000, "cpu_quota": 0, "cpu_period": 0}
        }));
    }
    fs::write(directory.join("realized-manifest.json"), json!({
        "schema_version": 1, "runner_protocol": "tool-bench-runner-v3",
        "verification_protocol": "offline-world-evidence-v5", "scenario": world.meta.id,
        "session_id": "trial__env", "task_source_sha256": mock_core::runner::sha256(&source_bytes),
        "task_context_sha256": mock_core::runner::sha256(&context_bytes), "context": context,
        "execution_kind": "scripted", "python_version": "3.13.14",
        "agent": {"name": "oracle", "version": "1.0.0", "code_sha256": mock_core::runner::sha256(b"agent")},
        "harness": {"name": "harbor", "version": "0.23.0", "code_sha256": mock_core::runner::sha256(b"harness")},
        "agent_timeout_sec": 900.0, "verifier_timeout_sec": 120.0, "images": images
    }).to_string()).unwrap();
    world.meta.id.clone()
}

fn verify(directory: &Path, scenario: &str, answer: bool) -> Value {
    verify_with_options(directory, scenario, answer, false)
}

fn verify_with_options(
    directory: &Path,
    scenario: &str,
    answer: bool,
    context_overflow: bool,
) -> Value {
    let mut command = Command::new(env!("CARGO_BIN_EXE_bench-verify"));
    command.current_dir(directory).args([
        "--scenario",
        scenario,
        "--contract",
        "contract.json",
        "--evidence",
        "evidence.json",
        "--output",
        "result",
    ]);
    if answer {
        command.args(["--answer", "answer.md"]);
    }
    if context_overflow {
        command.arg("--context-overflow");
    }
    if directory.join("task-source.json").exists() {
        command.args(["--task-source", "task-source.json"]);
    }
    if directory.join("runner-policy.json").exists() {
        command.args([
            "--runner-policy",
            "runner-policy.json",
            "--runner-admission",
            "runner-admission.json",
            "--task-context",
            "task-context.json",
            "--realized-manifest",
            "realized-manifest.json",
        ]);
    }
    if directory.join("inference-evidence.json").exists() {
        command.args(["--inference-evidence", "inference-evidence.json"]);
    }
    let output = command.output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&fs::read(directory.join("result/assessment.json")).unwrap()).unwrap()
}

fn native_fixture(directory: &Path, public_answer: &str, actor_answer: &str) -> String {
    native_fixture_with_answer_source(directory, public_answer, actor_answer, "final_response")
}

fn native_fixture_with_answer_source(
    directory: &Path,
    public_answer: &str,
    actor_answer: &str,
    answer_source: &str,
) -> String {
    use mock_core::runner::sha256;
    let scenario = fixture_with_answer_source(directory, Some(answer_source));
    fs::write(directory.join("answer.md"), actor_answer).unwrap();
    let configured = json!({
        "configured_model": "offline-fixture-model", "configured_provider": "unknown",
        "observed_model": null, "observed_revision": null, "sampling": {}, "budget": {"max_turns": 2},
    });
    let mut manifest: Value =
        serde_json::from_slice(&fs::read(directory.join("realized-manifest.json")).unwrap())
            .unwrap();
    manifest["execution_kind"] = json!("native");
    manifest["agent"]["name"] = json!("computer-1");
    manifest["inference_identity"] = configured.clone();
    let manifest_bytes = serde_json::to_vec(&manifest).unwrap();
    fs::write(directory.join("realized-manifest.json"), &manifest_bytes).unwrap();
    let adapter = json!({
        "name": mock_core::inference::COMPUTER1_ADAPTER_NAME,
        "code_sha256": sha256(b"offline-host-capture-fixture"),
    });
    let evidence = json!({
        "schema_version": 2, "adapter": adapter, "classification": "completed", "finalization": "normal",
        "scenario": scenario, "session_id": "trial__env",
        "turns": [{
            "turn": 0, "kind": "final_response", "input_tokens": 10, "output_tokens": 3,
            "retried": null, "retry_count": null, "cost_usd": null,
        }],
        "final_response": {"text": public_answer, "turn": 0, "finalization": "normal", "resumed": false},
        "cost": {
            "total_input_tokens": 10, "total_output_tokens": 3, "total_cost_usd": null,
            "retry_input_tokens": null, "retry_output_tokens": null, "retry_cost_usd": null,
            "successful_turns": 1, "retried_turns": null, "total_retry_attempts": null,
        },
        "identity": configured, "scripted": false,
    });
    let inference_bytes = serde_json::to_vec(&evidence).unwrap();
    fs::write(directory.join("inference-evidence.json"), &inference_bytes).unwrap();
    fs::write(
        directory.join("runner-policy.json"),
        json!({
            "schema_version": 1, "runner_protocol": "tool-bench-runner-v3",
            "verification_protocol": "offline-world-evidence-v5", "harbor_version": "0.23.0",
            "adapter_sha256": "a".repeat(64), "inference_adapter": adapter,
        })
        .to_string(),
    )
    .unwrap();
    fs::write(directory.join("runner-admission.json"), json!({
        "schema_version": 1, "runner_protocol": "tool-bench-runner-v3",
        "verification_protocol": "offline-world-evidence-v5", "scenario": scenario,
        "session_id": "trial__env", "harbor_version": "0.23.0", "adapter_sha256": "a".repeat(64),
        "contract_sha256": sha256(&fs::read(directory.join("contract.json")).unwrap()),
        "evidence_sha256": sha256(&fs::read(directory.join("evidence.json")).unwrap()),
        "main_artifacts_sha256": sha256(b"snapshot"), "answer_sha256": sha256(actor_answer.as_bytes()),
        "manifest_sha256": sha256(&manifest_bytes), "inference_evidence_sha256": sha256(&inference_bytes),
        "phase": "stopped", "failure": null,
        "main": {"container_id": "b".repeat(64), "status": "exited", "running": false,
            "paused": false, "restarting": false, "started_at": "2026-01-01T00:00:00Z",
            "finished_at": "2026-01-01T00:01:00Z"},
        "collection_checks": 2,
    }).to_string()).unwrap();
    scenario
}

#[test]
fn native_capture_does_not_replace_a_declared_answer_artifact_with_chat_text() {
    let directory = tempfile::tempdir().unwrap();
    let scenario = native_fixture_with_answer_source(
        directory.path(),
        "Wrong chat response",
        "Maya",
        "legacy_answer_file",
    );
    let assessed = verify(directory.path(), &scenario, true);
    assert_eq!(assessed["reward"], 1.0, "{assessed}");
    let receipt: Value = serde_json::from_slice(
        &fs::read(directory.path().join("result/verification.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(receipt["answer_source"], "legacy_answer_file");
}

#[test]
fn a_legacy_context_flag_cannot_override_structured_native_inference() {
    let directory = tempfile::tempdir().unwrap();
    let scenario = native_fixture(directory.path(), "Wrong public response", "Maya");
    let assessed = verify_with_options(directory.path(), &scenario, true, true);
    assert_eq!(assessed["outcome"], "fail", "{assessed}");
}

#[test]
fn absent_public_reply_is_decided_by_the_declared_answer_channel() {
    for (source, outcome) in [("legacy_answer_file", "pass"), ("final_response", "fail")] {
        let directory = tempfile::tempdir().unwrap();
        let scenario =
            native_fixture_with_answer_source(directory.path(), "unused", "Maya", source);
        let path = directory.path().join("inference-evidence.json");
        let mut inference: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        inference["classification"] = json!("no_public_response");
        inference["finalization"] = json!("no_response");
        inference["final_response"] = Value::Null;
        inference["turns"][0]["kind"] = json!("tool_use");
        let bytes = serde_json::to_vec(&inference).unwrap();
        fs::write(path, &bytes).unwrap();
        let receipt_path = directory.path().join("runner-admission.json");
        let mut receipt: Value = serde_json::from_slice(&fs::read(&receipt_path).unwrap()).unwrap();
        receipt["inference_evidence_sha256"] = json!(mock_core::runner::sha256(&bytes));
        fs::write(receipt_path, receipt.to_string()).unwrap();
        let assessed = verify(directory.path(), &scenario, true);
        assert_eq!(assessed["outcome"], outcome, "{source}: {assessed}");
        assert_eq!(assessed["environment_ok"], true);
    }
}

#[test]
fn native_final_message_not_the_actor_answer_file_is_graded_through_the_real_cli() {
    for (public, actor, reward) in [
        ("Maya", "Incorrect actor file", 1.0),
        ("Wrong response", "Maya", 0.0),
    ] {
        let directory = tempfile::tempdir().unwrap();
        let scenario = native_fixture(directory.path(), public, actor);
        let assessed = verify(directory.path(), &scenario, true);
        assert_eq!(assessed["reward"], reward, "{assessed}");
        assert_eq!(assessed["environment_ok"], true, "{assessed}");
        let receipt: Value = serde_json::from_slice(
            &fs::read(directory.path().join("result/verification.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(receipt["answer_source"], "native_final_response");
        assert_eq!(
            receipt["graded_answer_sha256"],
            mock_core::runner::sha256(public.as_bytes())
        );
        assert!(
            directory
                .path()
                .join("result/inference-evidence.json")
                .is_file()
        );
    }
}

#[test]
fn missing_or_substituted_native_capture_is_an_environment_fault() {
    for mutation in [
        "missing", "bytes", "adapter", "settings", "scripted", "session", "scenario",
    ] {
        let directory = tempfile::tempdir().unwrap();
        let scenario = native_fixture(directory.path(), "Maya", "Maya");
        let path = directory.path().join("inference-evidence.json");
        let mut evidence: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        match mutation {
            "missing" => fs::remove_file(&path).unwrap(),
            "bytes" => fs::write(&path, b"{}").unwrap(),
            _ => {
                match mutation {
                    "adapter" => evidence["adapter"]["name"] = json!("harbor-native-actor"),
                    "settings" => {
                        evidence["identity"]["configured_model"] = json!("different-model")
                    }
                    "scripted" => evidence["scripted"] = json!(true),
                    "session" => evidence["session_id"] = json!("another-trial__env"),
                    "scenario" => evidence["scenario"] = json!("another/scenario/id"),
                    _ => unreachable!(),
                }
                let bytes = serde_json::to_vec(&evidence).unwrap();
                fs::write(&path, &bytes).unwrap();
                let receipt_path = directory.path().join("runner-admission.json");
                let mut receipt: Value =
                    serde_json::from_slice(&fs::read(&receipt_path).unwrap()).unwrap();
                receipt["inference_evidence_sha256"] = json!(mock_core::runner::sha256(&bytes));
                fs::write(receipt_path, receipt.to_string()).unwrap();
            }
        }
        let assessed = verify(directory.path(), &scenario, true);
        assert_eq!(
            assessed["outcome"], "environment_error",
            "{mutation}: {assessed}"
        );
        assert_eq!(assessed["reward"], 0.0);
    }
}

#[test]
fn required_runner_proof_is_bound_to_the_actual_inputs_and_missing_proof_is_not_a_model_failure() {
    let directory = tempfile::tempdir().unwrap();
    let scenario = fixture(directory.path());
    let policy = json!({
        "schema_version": 1,
        "runner_protocol": "tool-bench-runner-v3",
        "verification_protocol": "offline-world-evidence-v5",
        "harbor_version": "0.23.0",
        "adapter_sha256": "a".repeat(64)
    });
    let mut receipt = json!({
        "schema_version": 1,
        "runner_protocol": "tool-bench-runner-v3",
        "verification_protocol": "offline-world-evidence-v5",
        "scenario": scenario,
        "session_id": "trial__env",
        "harbor_version": "0.23.0",
        "adapter_sha256": "a".repeat(64),
        "contract_sha256": mock_core::runner::sha256(&fs::read(directory.path().join("contract.json")).unwrap()),
        "evidence_sha256": mock_core::runner::sha256(&fs::read(directory.path().join("evidence.json")).unwrap()),
        "main_artifacts_sha256": mock_core::runner::sha256(b"snapshot"),
        "answer_sha256": mock_core::runner::sha256(b"Maya"),
        "manifest_sha256": mock_core::runner::sha256(&fs::read(directory.path().join("realized-manifest.json")).unwrap()),
        "phase": "stopped",
        "failure": null,
        "main": {
            "container_id": "b".repeat(64), "status": "exited",
            "running": false, "paused": false, "restarting": false,
            "started_at": "2026-01-01T00:00:00Z", "finished_at": "2026-01-01T00:01:00Z",
        },
        "collection_checks": 2
    });
    fs::write(
        directory.path().join("runner-policy.json"),
        policy.to_string(),
    )
    .unwrap();
    fs::write(
        directory.path().join("runner-admission.json"),
        receipt.to_string(),
    )
    .unwrap();
    let passing = verify(directory.path(), &scenario, true);
    assert_eq!(passing["reward"], 1.0);
    assert_eq!(passing["runner_admission_checked"], true);
    assert_eq!(passing["runner_admission_ok"], true);
    fs::write(directory.path().join("answer.md"), "Maya (forged)").unwrap();
    let forged = verify(directory.path(), &scenario, true);
    assert_eq!(forged["answer_ok"], true);
    assert_eq!(forged["outcome"], "environment_error");
    assert_eq!(forged["runner_admission_ok"], false);
    fs::write(directory.path().join("answer.md"), "Maya").unwrap();
    for modified in [Some("sha256:wrong".to_owned()), None] {
        if let Some(hash) = modified {
            receipt["evidence_sha256"] = json!(hash);
            fs::write(
                directory.path().join("runner-admission.json"),
                receipt.to_string(),
            )
            .unwrap();
        } else {
            fs::remove_file(directory.path().join("runner-admission.json")).unwrap();
        }
        let result = verify(directory.path(), &scenario, true);
        assert_eq!(result["reward"], 0.0);
        assert_eq!(result["outcome"], "environment_error");
        assert_eq!(result["answer_ok"], true);
        assert_eq!(result["runner_admission_checked"], true);
        assert_eq!(result["runner_admission_ok"], false);
        assert!(
            result["failures"]
                .as_array()
                .unwrap()
                .iter()
                .any(|failure| { failure["location"] == "evidence/runner-admission" })
        );
    }
}

#[test]
fn offline_cli_emits_the_same_assessment_and_numeric_metrics() {
    let directory = tempfile::tempdir().unwrap();
    let scenario = fixture(directory.path());
    let assessment = verify(directory.path(), &scenario, true);
    assert_eq!(assessment["reward"], 1.0);
    assert_eq!(assessment["outcome"], "pass");
    let reward: Value =
        serde_json::from_slice(&fs::read(directory.path().join("result/reward.json")).unwrap())
            .unwrap();
    assert!(reward.as_object().unwrap().values().all(Value::is_number));
    for name in [
        "evidence",
        "journal",
        "events",
        "state",
        "initial-state",
        "verification",
    ] {
        assert!(
            directory
                .path()
                .join(format!("result/{name}.json"))
                .is_file()
        );
    }
    let receipt: Value = serde_json::from_slice(
        &fs::read(directory.path().join("result/verification.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(receipt["scenario"], scenario);
    assert_eq!(receipt["answer_source"], "legacy_answer_file");
    assert!(
        receipt["evidence_sha256"]
            .as_str()
            .unwrap()
            .starts_with("sha256:")
    );
}

#[test]
fn absent_answers_are_agent_failures_but_failed_collection_is_an_environment_fault() {
    let directory = tempfile::tempdir().unwrap();
    let scenario = fixture(directory.path());
    assert_eq!(
        verify(directory.path(), &scenario, false)["outcome"],
        "fail"
    );
    fs::remove_file(directory.path().join("answer.md")).unwrap();
    let assessment = verify(directory.path(), &scenario, true);
    assert_eq!(assessment["outcome"], "environment_error");
    assert_eq!(assessment["reward"], 0.0);
}

#[test]
fn malformed_or_missing_world_evidence_fails_closed_with_explanation() {
    let directory = tempfile::tempdir().unwrap();
    let scenario = fixture(directory.path());
    for content in [Some("not json"), None] {
        if let Some(content) = content {
            fs::write(directory.path().join("evidence.json"), content).unwrap();
        } else {
            fs::remove_file(directory.path().join("evidence.json")).unwrap();
        }
        let assessment = verify(directory.path(), &scenario, true);
        assert_eq!(assessment["outcome"], "environment_error");
        assert_eq!(assessment["reward"], 0.0);
        assert!(
            assessment["failures"]
                .as_array()
                .unwrap()
                .iter()
                .any(|failure| {
                    failure["check"] == "environment" && failure["location"] == "evidence"
                })
        );
    }
}

#[test]
fn cross_task_evidence_and_malformed_contracts_cannot_be_scored_successfully() {
    let directory = tempfile::tempdir().unwrap();
    let scenario = fixture(directory.path());
    assert_eq!(
        verify(directory.path(), "other/scenario/id", true)["outcome"],
        "environment_error",
    );
    fs::write(
        directory.path().join("contract.json"),
        r#"{"answer":{"unknown":true}}"#,
    )
    .unwrap();
    let assessment = verify(directory.path(), &scenario, true);
    assert_eq!(assessment["outcome"], "invalid_contract");
    assert_eq!(assessment["reward"], 0.0);
}
