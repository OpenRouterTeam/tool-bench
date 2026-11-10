#![allow(dead_code)]

use std::{fs, path::Path};

use mock_core::{
    dataset::Dataset,
    evidence::WorldEvidence,
    provenance::{ExecutionManifest, TaskContext, canonical_sha256},
    runner::{RunnerAdmission, RunnerPolicy, sha256},
    world::WorldState,
};
use report::Provenance;
use serde_json::{Value, json};

pub fn write_verified(directory: &Path, trial: &str, task: &str, passes: bool) {
    write_fixture(
        directory,
        trial,
        task,
        json!({"version": 2, "audit_history": true, "answer": {"contains": ["Maya"]}}),
        if passes { b"Maya" } else { b"wrong" },
        None,
        None,
    );
}

pub fn write_verified_final_response(directory: &Path, trial: &str, task: &str, passes: bool) {
    write_fixture(
        directory,
        trial,
        task,
        json!({"version": 2, "audit_history": true, "answer": {"source":"final_response","contains":["Maya"]}}),
        if passes { b"Maya" } else { b"wrong" },
        None,
        None,
    );
}

pub fn write_unresolved(directory: &Path, trial: &str, task: &str) {
    write_fixture(
        directory,
        trial,
        task,
        json!({
            "version":2,
            "audit_history":true,
            "records":[{
                "selector":{"collection":"/services/github/issues","key_prefix":"octo-org/app#"},
                "min_count":1,
                "reviewed_text":{"/title":{
                    "requirement":"Review the unfamiliar issue wording against the user request.",
                    "accepted":["A separately reviewed positive issue title"]
                }}
            }]
        }),
        b"Maya",
        None,
        None,
    );
    let assessment: Value = serde_json::from_slice(
        &fs::read(directory.join(trial).join("verifier/assessment.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(assessment["outcome"], "unresolved", "{assessment}");
}

pub fn write_environment_error(directory: &Path, trial: &str, task: &str) {
    write_fixture(
        directory,
        trial,
        task,
        json!({
            "version": 2,
            "audit_history": true,
            "answer": {"record_facts": {
                "requirement": "Read the authored issue fact.",
                "selector": {
                    "collection": "/services/github/issues",
                    "matching": {"number": 1}
                },
                "facts": {"value": {"field": "/unavailable_fact", "kind": "identifier"}}
            }}
        }),
        b"Maya",
        None,
        None,
    );
    let assessment: Value = serde_json::from_slice(
        &fs::read(directory.join(trial).join("verifier/assessment.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(assessment["outcome"], "environment_error", "{assessment}");
}

fn write_fixture(
    directory: &Path,
    trial: &str,
    task: &str,
    definition: Value,
    answer: &[u8],
    review_policy: Option<&mock_core::review::ReviewPolicy>,
    dataset: Option<Dataset>,
) {
    let root = directory.join(trial);
    fs::create_dir_all(root.join("verifier")).unwrap();
    fs::create_dir_all(root.join("artifacts/world")).unwrap();
    let fixture =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/example/github/basic");
    let mut dataset = dataset.unwrap_or_else(|| Dataset::load(&fixture).unwrap());
    dataset.meta.id = task.replace("__", "/");
    dataset.source.scenario = dataset.meta.id.clone();
    dataset.contract = serde_json::from_value(definition).unwrap();
    let contract = dataset.contract.clone();
    let materialization = dataset.materialization.clone();
    let materialization_files: std::collections::BTreeMap<_, _> = materialization
        .as_ref()
        .into_iter()
        .flat_map(|bundle| bundle.files.iter())
        .map(|(name, bytes)| (name.clone(), sha256(bytes)))
        .collect();
    let mut world = WorldState::from_dataset(dataset).unwrap();
    world.seal().unwrap();
    let evidence = WorldEvidence::capture(&world, &[], Ok(vec!["192.0.2.10".into()]));
    let scenario = evidence.scenario.clone();
    let context: TaskContext = serde_json::from_value(json!({
        "schema_version": 1, "scenario": scenario,
        "instruction_sha256": sha256(b"instruction"), "profile_sha256": sha256(b"profile"),
        "framing_sha256": sha256(b"framing"), "environment_files": {"Dockerfile": sha256(b"source")},
        "world_reference": "world:fixture", "tools_reference": "tools:fixture",
        "review_policy_sha256": review_policy.map(|policy|policy.sha256().unwrap()),
        "materialization_files": materialization_files
    })).unwrap();
    let mut images = serde_json::Map::new();
    for (role, identifier) in [("main", "1"), ("world", "2"), ("tools", "3")] {
        images.insert(role.into(), json!({
            "container_id": identifier.repeat(64), "image_id": sha256(role.as_bytes()),
            "reference": format!("{role}:fixture"),
            "filesystem_sha256": sha256(format!("filesystem-{role}").as_bytes()),
            "runtime_config_sha256": sha256(format!("configuration-{role}").as_bytes()),
            "config_components": {"Env": sha256(b"environment")}, "orchestration_project": null,
            "resource_limits": {"memory": 1024, "nano_cpus": 1_000_000_000, "cpu_quota": 0, "cpu_period": 0}
        }));
    }
    let contract_bytes = serde_json::to_vec(&contract).unwrap();
    let source_bytes = serde_json::to_vec(world.source()).unwrap();
    let context_bytes = serde_json::to_vec(&context).unwrap();
    let short_name = trial.rsplit('/').next().unwrap();
    let session = format!("{short_name}__env");
    let manifest: ExecutionManifest = serde_json::from_value(json!({
        "schema_version": 1, "runner_protocol": "tool-bench-runner-v3",
        "verification_protocol": "offline-world-evidence-v5", "scenario": scenario,
        "session_id": session, "task_source_sha256": sha256(&source_bytes),
        "task_context_sha256": sha256(&context_bytes), "context": context,
        "execution_kind": "scripted", "python_version": "3.13.14",
        "agent": {"name": "oracle", "version": "1.0.0", "code_sha256": sha256(b"agent")},
        "harness": {"name": "harbor", "version": "0.23.0", "code_sha256": sha256(b"harness")},
        "agent_timeout_sec": 900.0, "verifier_timeout_sec": 120.0, "images": images
    }))
    .unwrap();
    let manifest_bytes = serde_json::to_vec(&manifest).unwrap();
    let policy: RunnerPolicy = serde_json::from_value(json!({
        "schema_version": 1, "runner_protocol": "tool-bench-runner-v3",
        "verification_protocol": "offline-world-evidence-v5", "harbor_version": "0.23.0",
        "adapter_sha256": "c".repeat(64)
    }))
    .unwrap();
    let policy_bytes = serde_json::to_vec(&policy).unwrap();
    let evidence_bytes = serde_json::to_vec(&evidence).unwrap();
    let admission: RunnerAdmission = serde_json::from_value(json!({
        "schema_version": 1, "runner_protocol": "tool-bench-runner-v3",
        "verification_protocol": "offline-world-evidence-v5", "scenario": scenario,
        "session_id": session, "harbor_version": "0.23.0", "adapter_sha256": policy.adapter_sha256,
        "contract_sha256": sha256(&contract_bytes), "evidence_sha256": sha256(&evidence_bytes),
        "main_artifacts_sha256": sha256(b"snapshot"), "answer_sha256": sha256(answer),
        "manifest_sha256": sha256(&manifest_bytes), "phase": "stopped", "failure": null,
        "main": {"container_id": "1".repeat(64), "status": "exited", "running": false,
            "paused": false, "restarting": false, "started_at": "2026-01-01T00:00:00Z",
            "finished_at": "2026-01-01T00:01:00Z"}, "collection_checks": 3
    }))
    .unwrap();
    let admission_bytes = serde_json::to_vec(&admission).unwrap();
    let mut assessment =
        evidence.assess(&contract, Some(std::str::from_utf8(answer).unwrap()), false);
    assessment.admit_runner(Ok(()));
    let receipt = json!({
        "schema_version": 3, "scenario": scenario, "evaluator_version": evidence.evaluator_version,
        "evidence_schema_version": evidence.schema_version, "contract_sha256": sha256(&contract_bytes),
        "contract_semantic_sha256": canonical_sha256(&contract), "evidence_sha256": sha256(&evidence_bytes),
        "initial_state_sha256": evidence.initial_state_sha256, "world_source": evidence.source,
        "answer_sha256": sha256(answer), "context_overflow": false,
        "task_source_checked": true, "task_source_sha256": sha256(&source_bytes),
        "task_context_sha256": sha256(&context_bytes), "manifest_sha256": sha256(&manifest_bytes),
        "runner_admission_checked": true, "runner_admission_ok": true,
        "runner_policy_sha256": sha256(&policy_bytes), "runner_admission_sha256": sha256(&admission_bytes)
    });
    for (name, bytes) in [
        ("contract.json", contract_bytes),
        ("task-source.json", source_bytes),
        ("task-context.json", context_bytes),
        ("realized-manifest.json", manifest_bytes),
        ("runner-policy.json", policy_bytes),
        ("runner-admission.json", admission_bytes),
        ("assessment.json", serde_json::to_vec(&assessment).unwrap()),
        ("verification.json", serde_json::to_vec(&receipt).unwrap()),
    ] {
        fs::write(root.join("verifier").join(name), bytes).unwrap();
    }
    if let Some(bundle) = materialization {
        for (name, bytes) in bundle.files {
            let destination = root.join("verifier").join(name);
            fs::create_dir_all(destination.parent().unwrap()).unwrap();
            fs::write(destination, bytes).unwrap();
        }
    }
    fs::write(root.join("artifacts/world/evidence.json"), evidence_bytes).unwrap();
    fs::create_dir_all(root.join("artifacts/agent")).unwrap();
    fs::write(root.join("artifacts/agent/answer.md"), answer).unwrap();
    fs::write(
        root.join("result.json"),
        json!({
            "task_name": format!("openrouter/tool-bench__{task}"), "trial_name": short_name,
            "agent_info": {"name": "oracle"}, "exception_info": null,
            "verifier_result": {"rewards": assessment.metrics()}
        })
        .to_string(),
    )
    .unwrap();
}

pub fn write_materialized(directory: &Path, trial: &str, dataset: Dataset) {
    let task = dataset.meta.id.replace('/', "__");
    write_fixture(
        directory,
        trial,
        &task,
        json!({"version":2,"audit_history":true,"answer":{"contains":["Maya"]}}),
        b"Maya",
        None,
        Some(dataset),
    );
}

pub fn write_reviewable(
    directory: &Path,
    trial: &str,
    task: &str,
    policy: &mock_core::review::ReviewPolicy,
    known_failure: bool,
) {
    let mut definition = json!({
        "version":2,
        "audit_history":true,
        "records":[{
            "selector":{"collection":"/services/github/issues","key_prefix":"octo-org/app#"},
            "min_count":1,
            "reviewed_text":{"/title":{
                "requirement":"Review the unfamiliar issue wording against the user request.",
                "accepted":["A separately reviewed positive issue title"]
            }}
        }]
    });
    if known_failure {
        definition["state"] = json!([{
            "pointer":"/services/github/issues/octo-org~1app#1/state",
            "equals":"a known incorrect state"
        }]);
    }
    write_fixture(
        directory,
        trial,
        task,
        definition,
        b"Maya",
        Some(policy),
        None,
    );
}

pub fn write_multifield_reviewable(
    directory: &Path,
    trial: &str,
    task: &str,
    policy: &mock_core::review::ReviewPolicy,
) {
    write_fixture(
        directory,
        trial,
        task,
        json!({
            "version":2,"audit_history":true,
            "records":[{
                "selector":{"collection":"/services/github/issues","key_prefix":"octo-org/app#","matching":{"number":1}},
                "equals":1,
                "reviewed_text":{
                    "/title":{"requirement":"The title must describe the correct issue.","accepted":["A positive title not present in this fixture"]},
                    "/body":{"requirement":"The body must describe the correct reproduction facts.","accepted":["A positive body not present in this fixture"]}
                }
            }]
        }),
        b"Maya",
        Some(policy),
        None,
    );
}

pub fn write_resolved_with_policy(
    directory: &Path,
    trial: &str,
    task: &str,
    policy: &mock_core::review::ReviewPolicy,
) {
    write_fixture(
        directory,
        trial,
        task,
        json!({"version":2,"audit_history":true,"answer":{"contains":["Maya"]}}),
        b"Maya",
        Some(policy),
        None,
    );
}

pub fn write_sentry_answer_reviewable(
    directory: &Path,
    trial: &str,
    task: &str,
    policy: &mock_core::review::ReviewPolicy,
    wrong_key: bool,
) {
    let source = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../scenarios/ops/sentry/create-notification-project");
    let mut dataset = Dataset::load(&source).unwrap();
    let service = dataset.services.get_mut("sentry").unwrap();
    let project = service
        .collection("projects")
        .unwrap()
        .get("checkout-api")
        .unwrap();
    let project_id = project["id"].as_str().unwrap().to_owned();
    let public = service
        .collection("keys")
        .unwrap()
        .iter()
        .find(|(_, key)| key["project"] == "checkout-api" && key["isActive"] == true)
        .unwrap()
        .1["public"]
        .as_str()
        .unwrap()
        .to_owned();
    service.collection_mut("instance").insert(
        "instance",
        json!({"ingestion_origin":"https://ingest.example.net/sentry"}),
    );
    let public = if wrong_key {
        "invented_public"
    } else {
        &public
    };
    let answer = format!(
        "Unfamiliar but potentially correct answer wording: https://{public}@ingest.example.net/sentry/{project_id}"
    );
    let definition = json!({
        "version":2,"audit_history":true,
        "answer":{"sentry_dsn":{
            "project":{"collection":"/services/sentry/projects","matching":{"slug":"checkout-api"}},
            "keys":{"collection":"/services/sentry/keys","matching":{"isActive":true}},
            "ingestion_origin":{
                "selector":{"collection":"/services/sentry/instance","key_prefix":"instance"},
                "field":"/ingestion_origin"
            },
            "content":{"requirement":"Return the actual project DSN without misleading surrounding claims.",
                "accepted":["${dsn}"]}
        }}
    });
    write_fixture(
        directory,
        trial,
        task,
        definition,
        answer.as_bytes(),
        Some(policy),
        Some(dataset),
    );
}

pub fn write_oncall_answer_reviewable(
    directory: &Path,
    trial: &str,
    task: &str,
    policy: &mock_core::review::ReviewPolicy,
    known_failure: bool,
) {
    let source = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../scenarios/ops/grafana/ack-checkout-page-oncall");
    let dataset = Dataset::load(&source).unwrap();
    let mut definition = json!({
        "version":2,"audit_history":true,"answer":dataset.contract.answer
    });
    if known_failure {
        definition["state"] = json!([{"pointer":"/services/grafana/oncall_schedules/SCK1PRIM/name","equals":"Another schedule"}]);
    }
    write_fixture(
        directory,
        trial,
        task,
        definition,
        b"Please page Sam Brennan for Checkout Primary right now.",
        Some(policy),
        Some(dataset),
    );
}

pub fn provenance(task: &str) -> Provenance {
    let root = tempfile::tempdir().unwrap();
    write_verified(root.path(), "trial", task, true);
    report::load_trials(root.path())
        .unwrap()
        .remove(0)
        .provenance
        .unwrap()
}

pub fn result(directory: &Path, trial: &str) -> Value {
    serde_json::from_slice(&fs::read(directory.join(trial).join("result.json")).unwrap()).unwrap()
}
