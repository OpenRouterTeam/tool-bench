use std::{fs, path::Path, process::Command};

use mock_core::{dataset::Dataset, evidence::WorldEvidence, world::WorldState};
use serde_json::{Value, json};

fn fixture(directory: &Path) -> String {
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/example/github/basic");
    let mut dataset = Dataset::load(&source).unwrap();
    dataset.contract = serde_json::from_value(
        json!({"version": 2, "audit_history": true, "answer": {"contains": ["Maya"]}}),
    )
    .unwrap();
    let mut world = WorldState::from_dataset(dataset).unwrap();
    world.seal().unwrap();
    let evidence = WorldEvidence::capture(&world, &[], Ok(vec!["192.0.2.10".into()]));
    fs::write(
        directory.join("evidence.json"),
        serde_json::to_vec(&evidence).unwrap(),
    )
    .unwrap();
    fs::write(
        directory.join("contract.json"),
        json!({"version": 2, "audit_history": true, "answer": {"contains": ["Maya"]}}).to_string(),
    )
    .unwrap();
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
    let output = command.output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&fs::read(directory.join("result/assessment.json")).unwrap()).unwrap()
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
