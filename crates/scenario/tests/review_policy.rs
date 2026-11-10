use std::{fs, path::Path, process::Command};

use catalog::Catalog;
use mock_core::{provenance::TaskContext, review::ReviewPolicy};
use scenario::{
    Scenario,
    taskgen::{Conditions, Images, write_task_with_conditions},
};
use serde_json::{Value, json};

fn root() -> &'static Path {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
}

fn policy() -> ReviewPolicy {
    serde_json::from_value(json!({
        "schema_version":1,
        "protocol":"tool-bench-content-review-v1",
        "reviewers":{
            "first":{"public_key":"d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a","kind":"scripted_fixture"},
            "second":{"public_key":"3d4017c3e843895a92b70aa74d1b7ebc9c982ccf2ec4968cc0cd55f12af4660c","kind":"scripted_fixture"}
        }
    })).unwrap()
}

#[test]
fn review_policy_is_bound_before_execution_without_entering_the_agent_workspace() {
    let scenario =
        Scenario::load(&root().join("scenarios/collab/notion/update-current-runbook")).unwrap();
    let catalog = Catalog::load(&root().join("catalog/services.toml")).unwrap();
    let images = Images {
        world: "world:fixture".into(),
        tools: "tools:fixture".into(),
    };
    let policy = policy();
    let output = tempfile::tempdir().unwrap();
    write_task_with_conditions(
        &scenario,
        &catalog,
        &images,
        output.path(),
        Conditions {
            review_policy: Some(&policy),
            ..Conditions::default()
        },
    )
    .unwrap();
    let task = output.path().join(scenario.task_name());
    let context: TaskContext =
        serde_json::from_slice(&fs::read(task.join("tests/task-context.json")).unwrap()).unwrap();
    assert_eq!(context.review_policy_sha256, Some(policy.sha256().unwrap()));
    let retained: ReviewPolicy =
        serde_json::from_slice(&fs::read(task.join("tests/review-policy.json")).unwrap()).unwrap();
    assert_eq!(retained, policy);
    assert_eq!(
        fs::read_to_string(task.join("instruction.md")).unwrap(),
        scenario.instruction
    );
    assert!(!task.join("environment/review-policy.json").exists());
    assert!(
        !fs::read_to_string(task.join("environment/Dockerfile"))
            .unwrap()
            .contains("review-policy")
    );
}

#[test]
fn absent_policy_keeps_legacy_context_serialization_and_invalid_policy_cannot_erase_tasks() {
    let scenario =
        Scenario::load(&root().join("scenarios/collab/notion/update-current-runbook")).unwrap();
    let catalog = Catalog::load(&root().join("catalog/services.toml")).unwrap();
    let images = Images {
        world: "world:fixture".into(),
        tools: "tools:fixture".into(),
    };
    let output = tempfile::tempdir().unwrap();
    write_task_with_conditions(
        &scenario,
        &catalog,
        &images,
        output.path(),
        Conditions::default(),
    )
    .unwrap();
    let task = output.path().join(scenario.task_name());
    let bytes = fs::read(task.join("tests/task-context.json")).unwrap();
    let context: Value = serde_json::from_slice(&bytes).unwrap();
    assert!(context.get("review_policy_sha256").is_none());
    assert!(!task.join("tests/review-policy.json").exists());
    let mut invalid = policy();
    invalid.reviewers.remove("second");
    assert!(
        write_task_with_conditions(
            &scenario,
            &catalog,
            &images,
            output.path(),
            Conditions {
                review_policy: Some(&invalid),
                ..Conditions::default()
            },
        )
        .is_err()
    );
    assert_eq!(
        fs::read(task.join("tests/task-context.json")).unwrap(),
        bytes
    );
}

#[test]
fn taskgen_cli_validates_and_materializes_the_declared_policy() {
    let output = tempfile::tempdir().unwrap();
    let path = output.path().join("policy.json");
    let policy = policy();
    fs::write(&path, serde_json::to_vec(&policy).unwrap()).unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_taskgen"))
        .current_dir(root())
        .arg("--out")
        .arg(output.path().join("tasks"))
        .args([
            "--world-image",
            "world:fixture",
            "--tools-image",
            "tools:fixture",
        ])
        .arg("--review-policy")
        .arg(&path)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let task = output
        .path()
        .join("tasks/collab__notion__update-current-runbook");
    let context: TaskContext =
        serde_json::from_slice(&fs::read(task.join("tests/task-context.json")).unwrap()).unwrap();
    assert_eq!(context.review_policy_sha256, Some(policy.sha256().unwrap()));
}
