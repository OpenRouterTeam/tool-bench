use std::fs;

use report::{Outcome, Trial, compare, compare_declared, load_trials};
use serde_json::json;

mod support;

fn trial() -> Trial {
    Trial {
        task: "example__github__basic".into(),
        reward: 1.0,
        outcome: Outcome::Pass,
        provenance: Some(support::provenance("example__github__basic")),
        review: None,
    }
}

#[test]
fn world_grader_context_images_and_budgets_cannot_change_without_rejection() {
    for (pointer, value) in [
        ("/evaluator_version", json!(999)),
        ("/source/contract_sha256", json!("sha256:changed")),
        ("/source/dataset_sha256", json!("sha256:changed")),
        ("/initial_state_sha256", json!("sha256:changed")),
        ("/runner_adapter_sha256", json!("changed")),
        ("/manifest/images/tools/image_id", json!("sha256:changed")),
        ("/manifest/harness/code_sha256", json!("sha256:changed")),
        ("/manifest/context/profile_sha256", json!("sha256:changed")),
        ("/manifest/context/framing_sha256", json!("sha256:changed")),
        (
            "/manifest/context/instruction_sha256",
            json!("sha256:changed"),
        ),
        ("/manifest/agent_timeout_sec", json!(1800)),
    ] {
        let original = trial();
        let mut changed = original.clone();
        let mut provenance = serde_json::to_value(changed.provenance.as_ref().unwrap()).unwrap();
        *provenance.pointer_mut(pointer).unwrap() = value;
        changed.provenance = Some(serde_json::from_value(provenance).unwrap());
        assert!(compare(&[original], &[changed]).is_err(), "{pointer}");
    }
}

#[test]
fn agent_differences_must_be_declared_and_cannot_hide_other_condition_changes() {
    let original = trial();
    let mut changed = original.clone();
    changed.provenance.as_mut().unwrap().manifest.agent.name = "nop".into();
    assert!(
        compare(
            std::slice::from_ref(&original),
            std::slice::from_ref(&changed)
        )
        .is_err()
    );
    let comparison = compare_declared(
        std::slice::from_ref(&original),
        std::slice::from_ref(&changed),
        true,
    )
    .unwrap();
    assert_eq!(comparison.declared_treatments, ["agent"]);
    changed
        .provenance
        .as_mut()
        .unwrap()
        .manifest
        .harness
        .version = "different".into();
    assert!(compare_declared(&[original], &[changed], true).is_err());
}

#[test]
fn heterogeneous_repeats_task_intersections_and_unbalanced_counts_are_not_inferred() {
    let original = trial();
    let mut changed = original.clone();
    changed.provenance.as_mut().unwrap().manifest.agent.name = "nop".into();
    assert!(
        compare_declared(
            &[original.clone(), changed],
            &[original.clone(), original.clone()],
            true
        )
        .is_err()
    );
    assert!(
        compare(
            &[original.clone(), original.clone()],
            std::slice::from_ref(&original)
        )
        .is_err()
    );
    let mut extra = original.clone();
    extra.task = "other".into();
    assert!(compare(&[original.clone(), extra], &[original]).is_err());
}

#[test]
fn legacy_scalar_results_remain_readable_but_cannot_be_a_verified_final_comparison() {
    let original = Trial {
        provenance: None,
        ..trial()
    };
    assert!(
        compare(
            std::slice::from_ref(&original),
            std::slice::from_ref(&original)
        )
        .is_err()
    );
}

#[test]
fn copied_raw_inputs_reproduce_the_assessment_and_tampering_is_not_trusted() {
    for filename in [
        "contract.json",
        "task-source.json",
        "task-context.json",
        "realized-manifest.json",
        "assessment.json",
    ] {
        let directory = tempfile::tempdir().unwrap();
        support::write_verified(directory.path(), "trial", "example__github__basic", true);
        let trials = load_trials(directory.path()).unwrap();
        assert!(compare(&trials, &trials).is_ok());
        fs::write(directory.path().join("trial/verifier").join(filename), "{}").unwrap();
        assert!(load_trials(directory.path()).is_err(), "{filename}");
    }
}

#[test]
fn agent_writable_nested_results_never_inflate_trial_counts() {
    let directory = tempfile::tempdir().unwrap();
    support::write_verified(directory.path(), "trial", "example__github__basic", true);
    let nested = directory.path().join("trial/agent/fake");
    fs::create_dir_all(&nested).unwrap();
    fs::write(
        nested.join("result.json"),
        support::result(directory.path(), "trial").to_string(),
    )
    .unwrap();
    assert_eq!(load_trials(directory.path()).unwrap().len(), 1);
}

#[test]
fn per_trial_main_image_tags_do_not_hide_real_filesystem_or_runtime_changes() {
    let original = trial();
    let mut changed = original.clone();
    let image = changed
        .provenance
        .as_mut()
        .unwrap()
        .manifest
        .images
        .get_mut("main")
        .unwrap();
    image.image_id = mock_core::runner::sha256(b"same-filesystem-other-project-label");
    image.orchestration_project = Some("new-trial".into());
    assert!(
        compare(
            std::slice::from_ref(&original),
            std::slice::from_ref(&changed)
        )
        .is_ok()
    );
    changed
        .provenance
        .as_mut()
        .unwrap()
        .manifest
        .images
        .get_mut("main")
        .unwrap()
        .filesystem_sha256 = mock_core::runner::sha256(b"different filesystem");
    assert!(
        compare(
            std::slice::from_ref(&original),
            std::slice::from_ref(&changed)
        )
        .is_err()
    );
    let mut changed = original.clone();
    changed
        .provenance
        .as_mut()
        .unwrap()
        .manifest
        .images
        .get_mut("main")
        .unwrap()
        .runtime_config_sha256 = mock_core::runner::sha256(b"different runtime defaults");
    assert!(compare(&[original], &[changed]).is_err());
}

#[test]
fn separate_tasks_cannot_be_used_to_pool_different_evaluator_releases() {
    let original = trial();
    let mut other = original.clone();
    other.task = "other".into();
    other.provenance.as_mut().unwrap().evaluator_version += 1;
    assert!(compare(&[original.clone(), other.clone()], &[original, other]).is_err());
}

#[test]
fn unresolved_results_require_the_same_checked_provenance_as_resolved_results() {
    let directory = tempfile::tempdir().unwrap();
    support::write_unresolved(directory.path(), "trial", "example__github__basic");
    let trials = load_trials(directory.path()).unwrap();
    assert_eq!(trials[0].outcome, Outcome::Unresolved);
    assert!(trials[0].provenance.is_some());
    assert!(compare(&trials, &trials).is_err());
}

#[test]
fn unresolved_disposition_does_not_hide_raw_input_or_saved_assessment_tampering() {
    for filename in [
        "artifacts/world/evidence.json",
        "verifier/contract.json",
        "verifier/assessment.json",
        "verifier/task-source.json",
        "verifier/runner-admission.json",
        "verifier/realized-manifest.json",
    ] {
        let directory = tempfile::tempdir().unwrap();
        support::write_unresolved(directory.path(), "trial", "example__github__basic");
        fs::write(directory.path().join("trial").join(filename), "{}").unwrap();
        assert!(load_trials(directory.path()).is_err(), "{filename}");
    }
}

#[test]
fn review_policies_are_prespecified_release_conditions_not_inferred_treatments() {
    let mut first = trial();
    first
        .provenance
        .as_mut()
        .unwrap()
        .manifest
        .context
        .review_policy_sha256 = Some(mock_core::runner::sha256(b"first policy"));
    let mut second = first.clone();
    second.task = "another task".into();
    second
        .provenance
        .as_mut()
        .unwrap()
        .manifest
        .context
        .review_policy_sha256 = Some(mock_core::runner::sha256(b"second policy"));
    assert!(compare(&[first.clone(), second.clone()], &[first, second]).is_err());
}

#[test]
fn declared_connection_values_require_matching_actual_tools_observations() {
    use mock_core::{
        provenance::{ExecutionManifest, TaskContext},
        runner::sha256,
    };
    let directory = tempfile::tempdir().unwrap();
    support::write_verified(directory.path(), "trial", "example__github__basic", true);
    let verifier = directory.path().join("trial/verifier");
    let source = fs::read(verifier.join("task-source.json")).unwrap();
    let mut context: TaskContext =
        serde_json::from_slice(&fs::read(verifier.join("task-context.json")).unwrap()).unwrap();
    context.connector_env_sha256.insert(
        "GITLAB_API_URL".into(),
        sha256(b"https://gitlab.northwind.internal/api/v4"),
    );
    let context_bytes = serde_json::to_vec(&context).unwrap();
    let mut manifest: ExecutionManifest =
        serde_json::from_slice(&fs::read(verifier.join("realized-manifest.json")).unwrap())
            .unwrap();
    manifest.context = context.clone();
    manifest.task_context_sha256 = sha256(&context_bytes);
    assert!(
        manifest
            .validate("example/github/basic", &context, &source, &context_bytes)
            .is_err()
    );
    manifest
        .images
        .get_mut("tools")
        .unwrap()
        .connector_env_sha256 = context.connector_env_sha256.clone();
    manifest
        .validate("example/github/basic", &context, &source, &context_bytes)
        .unwrap();
    manifest
        .images
        .get_mut("tools")
        .unwrap()
        .connector_env_sha256
        .insert("GITLAB_API_URL".into(), sha256(b"different"));
    assert!(
        manifest
            .validate("example/github/basic", &context, &source, &context_bytes)
            .is_err()
    );
}
