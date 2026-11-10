//! `bench-verify` surfaces the sealed host surface audit as its own `surface_audit`
//! status and carries the bound condition manifests forward, without either ever
//! touching the grade.

use std::{fs, path::Path, process::Command};

use mock_core::{dataset::Dataset, evidence::WorldEvidence, runner::sha256, world::WorldState};
use serde_json::{Value, json};

/// The exact record `deploy/runner/surface_audit.py` seals for a capture with one admin
/// route finding (kept in step by `deploy/runner/test_surface_audit.py`).
const PYTHON_SEALED: &str = include_str!("fixtures/sealed-surface-audit.json");

struct Trial {
    directory: tempfile::TempDir,
    scenario: String,
    receipt: Value,
}

fn trial(framing: &str) -> Trial {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/example/github/basic");
    let mut dataset = Dataset::load(&source).unwrap();
    let definition = json!({"version": 2, "audit_history": true, "answer": {"contains": ["Maya"]}});
    dataset.contract = serde_json::from_value(definition.clone()).unwrap();
    let mut world = WorldState::from_dataset(dataset).unwrap();
    world.seal().unwrap();
    let scenario = world.meta.id.clone();
    let evidence = serde_json::to_vec(&WorldEvidence::capture(
        &world,
        &[],
        Ok(vec!["192.0.2.10".into()]),
    ))
    .unwrap();
    let source_bytes = serde_json::to_vec(world.source()).unwrap();
    let rendered = if framing == "neutral" {
        sha256(b"instruction")
    } else {
        sha256(b"prefixed instruction")
    };
    let framing_manifest = json!({"version": 1, "framing": framing,
        "base_instruction_sha256": sha256(b"instruction"), "rendered_instruction_sha256": rendered})
    .to_string();
    let profile_manifest = json!({"version": 2, "id": "minimal", "files_sha256": sha256(b""),
        "files": [], "business_reference_time": null, "information": []})
    .to_string();
    let context = json!({
        "schema_version": 1, "scenario": scenario, "instruction_sha256": rendered,
        "profile_sha256": sha256(profile_manifest.as_bytes()),
        "framing_sha256": sha256(framing_manifest.as_bytes()),
        "environment_files": {"Dockerfile": sha256(b"source")},
        "world_reference": "world:fixture", "tools_reference": "tools:fixture"
    });
    let context_bytes = serde_json::to_vec(&context).unwrap();
    let mut images = serde_json::Map::new();
    for (role, id) in [("main", "b"), ("world", "c"), ("tools", "d")] {
        images.insert(role.into(), json!({
            "container_id": id.repeat(64), "image_id": sha256(role.as_bytes()), "reference": format!("{role}:fixture"),
            "filesystem_sha256": sha256(b"filesystem"), "runtime_config_sha256": sha256(b"configuration"),
            "config_components": {"Env": sha256(b"environment")}, "orchestration_project": null,
            "resource_limits": {"memory": 1024, "nano_cpus": 1_000_000_000, "cpu_quota": 0, "cpu_period": 0}
        }));
    }
    let manifest = json!({
        "schema_version": 1, "runner_protocol": "tool-bench-runner-v3",
        "verification_protocol": "offline-world-evidence-v5", "scenario": scenario,
        "session_id": "trial__env", "task_source_sha256": sha256(&source_bytes),
        "task_context_sha256": sha256(&context_bytes), "context": context,
        "execution_kind": "scripted", "python_version": "3.13.14",
        "agent": {"name": "oracle", "version": "1.0.0", "code_sha256": sha256(b"agent")},
        "harness": {"name": "harbor", "version": "0.23.0", "code_sha256": sha256(b"harness")},
        "agent_timeout_sec": 900.0, "verifier_timeout_sec": 120.0, "images": images
    })
    .to_string();
    let receipt = json!({
        "schema_version": 1, "runner_protocol": "tool-bench-runner-v3",
        "verification_protocol": "offline-world-evidence-v5", "scenario": scenario,
        "session_id": "trial__env", "harbor_version": "0.23.0", "adapter_sha256": "a".repeat(64),
        "contract_sha256": sha256(definition.to_string().as_bytes()), "evidence_sha256": sha256(&evidence),
        "main_artifacts_sha256": sha256(b"snapshot"), "answer_sha256": sha256(b"Maya"),
        "manifest_sha256": sha256(manifest.as_bytes()), "phase": "stopped", "failure": null,
        "main": {"container_id": "b".repeat(64), "status": "exited", "running": false, "paused": false,
            "restarting": false, "started_at": "2026-01-01T00:00:00Z", "finished_at": "2026-01-01T00:01:00Z"},
        "collection_checks": 2
    });
    for (name, bytes) in [
        ("evidence.json", evidence),
        ("contract.json", definition.to_string().into_bytes()),
        ("answer.md", b"Maya".to_vec()),
        ("task-source.json", source_bytes),
        ("task-context.json", context_bytes),
        ("framing.json", framing_manifest.into_bytes()),
        ("profile.json", profile_manifest.into_bytes()),
        ("realized-manifest.json", manifest.into_bytes()),
        (
            "runner-policy.json",
            json!({"schema_version": 1, "runner_protocol": "tool-bench-runner-v3",
            "verification_protocol": "offline-world-evidence-v5", "harbor_version": "0.23.0",
            "adapter_sha256": "a".repeat(64)})
            .to_string()
            .into_bytes(),
        ),
    ] {
        fs::write(root.join(name), bytes).unwrap();
    }
    Trial {
        directory,
        scenario,
        receipt,
    }
}

/// Runs the real verifier and returns `(verification.json, assessment.json, reward.json)`.
fn verify(trial: &Trial, receipt: &Value) -> (Value, Value, Value) {
    let root = trial.directory.path();
    fs::write(root.join("runner-admission.json"), receipt.to_string()).unwrap();
    let _ = fs::remove_dir_all(root.join("result"));
    let output = Command::new(env!("CARGO_BIN_EXE_bench-verify"))
        .current_dir(root)
        .args([
            "--scenario",
            &trial.scenario,
            "--contract",
            "contract.json",
            "--evidence",
            "evidence.json",
        ])
        .args([
            "--output",
            "result",
            "--answer",
            "answer.md",
            "--task-source",
            "task-source.json",
        ])
        .args([
            "--runner-policy",
            "runner-policy.json",
            "--runner-admission",
            "runner-admission.json",
        ])
        .args([
            "--task-context",
            "task-context.json",
            "--realized-manifest",
            "realized-manifest.json",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let read = |name: &str| -> Value {
        serde_json::from_slice(&fs::read(root.join("result").join(name)).unwrap()).unwrap()
    };
    (
        read("verification.json"),
        read("assessment.json"),
        read("reward.json"),
    )
}

#[test]
fn verifier_reports_the_sealed_audit_as_its_own_status_and_never_as_part_of_the_grade() {
    let trial = trial("neutral");
    let (absent, graded, reward) = verify(&trial, &trial.receipt);
    assert_eq!(graded["reward"], 1.0);
    assert_eq!(absent["runner_admission_ok"], true);
    assert_eq!(absent["surface_audit"]["status"], "unqualified");
    assert_eq!(
        absent["surface_audit"]["reason"],
        "no host surface capture was sealed"
    );

    // The record the Python runner seals: an admin route in a workspace file.
    let sealed: Value = serde_json::from_str(PYTHON_SEALED).unwrap();
    let mut receipt = trial.receipt.clone();
    receipt["surface_audit"] = sealed.clone();
    let (found, graded_found, reward_found) = verify(&trial, &receipt);
    assert_eq!(found["surface_audit"]["status"], "findings_present");
    assert_eq!(
        found["surface_audit"]["finding_count"],
        sealed["finding_count"]
    );
    assert_eq!(found["surface_audit"]["date_coherence"], "coherent");
    assert_eq!(
        found["surface_audit"]["report_sha256"],
        sealed["report_sha256"]
    );
    assert!(
        found["surface_audit"]["claim"]
            .as_str()
            .unwrap()
            .contains("does not establish that the model was unaware")
    );

    // A clean capture, and the same capture with one required surface dropped.
    let mut clean = sealed.clone();
    clean["outcome"] = json!("clean");
    clean["finding_count"] = json!(0);
    clean["findings"] = json!([]);
    receipt["surface_audit"] = clean.clone();
    let (passed, graded_clean, reward_clean) = verify(&trial, &receipt);
    assert_eq!(passed["surface_audit"]["status"], "clean");
    let mut uncaptured = clean.clone();
    uncaptured["captured"]
        .as_array_mut()
        .unwrap()
        .retain(|surface| surface != "mcp_tool_definitions");
    receipt["surface_audit"] = uncaptured;
    let (partial, _, _) = verify(&trial, &receipt);
    assert_eq!(partial["surface_audit"]["status"], "unqualified");
    assert_eq!(
        partial["surface_audit"]["missing"],
        json!(["mcp_tool_definitions"])
    );

    // The audit is reported beside the grade and can never move it.
    for (assessment, metrics) in [
        (&graded_found, &reward_found),
        (&graded_clean, &reward_clean),
    ] {
        assert_eq!(assessment, &graded);
        assert_eq!(metrics, &reward);
    }
}

#[test]
fn an_audit_in_a_receipt_that_fails_admission_is_unqualified() {
    let trial = trial("neutral");
    let mut clean: Value = serde_json::from_str(PYTHON_SEALED).unwrap();
    clean["outcome"] = json!("clean");
    clean["finding_count"] = json!(0);
    clean["findings"] = json!([]);
    let mut receipt = trial.receipt.clone();
    receipt["surface_audit"] = clean;
    receipt["phase"] = json!("open");
    let (verification, _, _) = verify(&trial, &receipt);
    assert_eq!(verification["runner_admission_ok"], false);
    assert_eq!(verification["surface_audit"]["status"], "unqualified");

    // An unknown field in the sealed record invalidates the receipt itself.
    let mut receipt = trial.receipt.clone();
    receipt["surface_audit"] = json!({"outcome": "clean", "certified_unaware": true});
    let (verification, _, _) = verify(&trial, &receipt);
    assert_eq!(verification["surface_audit"]["status"], "unqualified");
}

#[test]
fn bound_condition_manifests_are_carried_forward_and_unbound_ones_are_not() {
    let trial = trial("evaluation");
    let (verification, _, _) = verify(&trial, &trial.receipt);
    let condition = &verification["information_condition"];
    assert_eq!(condition["framing"], "evaluation");
    assert_eq!(condition["workspace"], "minimal");
    assert_eq!(condition["exposure"], "full_catalog");
    let root = trial.directory.path();
    for name in ["framing.json", "profile.json"] {
        assert_eq!(
            fs::read(root.join("result").join(name)).unwrap(),
            fs::read(root.join(name)).unwrap()
        );
    }

    // A manifest whose bytes the task context does not bind yields no condition.
    fs::write(root.join("framing.json"), br#"{"framing":"neutral"}"#).unwrap();
    let (verification, assessment, _) = verify(&trial, &trial.receipt);
    assert!(verification["information_condition"].is_null());
    assert!(!root.join("result/framing.json").exists());
    assert_eq!(assessment["reward"], 1.0);
}
