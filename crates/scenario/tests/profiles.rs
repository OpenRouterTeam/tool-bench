use std::{fs, path::Path, process::Command};

use catalog::Catalog;
use scenario::{
    Scenario,
    profile::{WorkspaceProfile, materialize},
    taskgen::{Images, write_task_with_profile},
};
use serde_json::Value;

fn root() -> &'static Path {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
}

fn catalog() -> Catalog {
    Catalog::load(&root().join("catalog/services.toml")).unwrap()
}

#[test]
fn profiles_are_deterministic_synthetic_and_do_not_import_private_task_content() {
    let catalog = catalog();
    for profile in [
        WorkspaceProfile::Minimal,
        WorkspaceProfile::Support,
        WorkspaceProfile::Engineering,
    ] {
        let first = materialize(profile, "2026-10-28T15:00:00Z", &catalog).unwrap();
        let again = materialize(profile, "2026-10-28T16:00:00+01:00", &catalog).unwrap();
        assert_eq!(first.files, again.files);
        assert_eq!(first.manifest, again.manifest);
        assert_eq!(first.manifest.files_sha256.len(), 71);
        if profile == WorkspaceProfile::Minimal {
            assert!(first.files.is_empty() && first.manifest.information.is_empty());
            continue;
        }
        assert_eq!(first.files.len(), 7);
        assert_eq!(first.files["workspace/CLAUDE.md"], "@AGENTS.md\n");
        let surface = first.files.values().cloned().collect::<Vec<_>>().join("\n");
        for forbidden in [
            "benchmark",
            "scenario",
            "reward",
            "oracle",
            "_bench",
            "/Users/luke",
            "BENCH_ADMIN_TOKEN",
            "fc-toolbench",
        ] {
            assert!(!surface.contains(forbidden), "{forbidden}");
        }
        let connections: Value =
            serde_json::from_str(&first.files["workspace/.config/workspace/connections.json"])
                .unwrap();
        assert_eq!(connections["connections"].as_array().unwrap().len(), 32);
        assert!(
            connections["connections"]
                .as_array()
                .unwrap()
                .iter()
                .all(|connection| connection.get("env").is_none())
        );
        assert!(surface.contains("application") && surface.contains("2026-10-28T15:00:00Z"));
    }
}

#[test]
fn profile_and_reference_changes_are_visible_in_materialized_hashes() {
    let catalog = catalog();
    let support = materialize(WorkspaceProfile::Support, "2026-10-28T15:00:00Z", &catalog).unwrap();
    let engineering = materialize(
        WorkspaceProfile::Engineering,
        "2026-10-28T15:00:00Z",
        &catalog,
    )
    .unwrap();
    let later = materialize(WorkspaceProfile::Support, "2026-10-29T15:00:00Z", &catalog).unwrap();
    assert_ne!(
        support.manifest.files_sha256,
        engineering.manifest.files_sha256
    );
    assert_ne!(support.manifest.files_sha256, later.manifest.files_sha256);
}

#[test]
fn generated_profiles_leave_instructions_contract_and_catalog_exposure_unchanged() {
    let mut scenario =
        Scenario::load(&root().join("scenarios/business/multi/zendesk-refund-via-stripe")).unwrap();
    scenario.solution.answer = Some("PRIVATE-SOLUTION-CANARY".into());
    let output = tempfile::tempdir().unwrap();
    let images = Images {
        world: "w:1".into(),
        tools: "t:1".into(),
    };
    for profile in [
        WorkspaceProfile::Minimal,
        WorkspaceProfile::Support,
        WorkspaceProfile::Engineering,
    ] {
        let destination = output.path().join(profile.id());
        write_task_with_profile(&scenario, &catalog(), &images, &destination, profile).unwrap();
        let task = destination.join(scenario.task_name());
        assert_eq!(
            fs::read_to_string(task.join("instruction.md")).unwrap(),
            scenario.instruction
        );
        assert_eq!(
            serde_json::from_str::<Value>(
                &fs::read_to_string(task.join("tests/expect.json")).unwrap()
            )
            .unwrap(),
            serde_json::to_value(&scenario.expect).unwrap()
        );
        let config: toml::Value =
            toml::from_str(&fs::read_to_string(task.join("task.toml")).unwrap()).unwrap();
        assert_eq!(
            config["environment"]["mcp_servers"]
                .as_array()
                .unwrap()
                .len(),
            32
        );
        assert_eq!(
            config["metadata"]["workspace_profile"].as_str(),
            Some(profile.id())
        );
        assert!(config.get("workspace_profile").is_none());
        let dockerfile = fs::read_to_string(task.join("environment/Dockerfile")).unwrap();
        assert!(!dockerfile.contains("/tests") && !dockerfile.contains("/solution"));
        if profile == WorkspaceProfile::Minimal {
            assert!(!task.join("environment/profile").exists());
            assert!(!dockerfile.contains("COPY profile"));
        } else {
            let instructions =
                fs::read_to_string(task.join("environment/profile/workspace/AGENTS.md")).unwrap();
            assert!(!instructions.contains("PRIVATE-SOLUTION-CANARY"));
            assert!(!instructions.contains(&scenario.meta.id));
            assert!(dockerfile.contains("COPY profile/workspace/ /app/"));
            assert!(dockerfile.contains("COPY profile/home/ /root/"));
        }
        assert!(task.join("tests/profile.json").is_file());
    }
}

#[test]
fn taskgen_rejects_unknown_profile_names_without_touching_the_output() {
    let output = tempfile::tempdir().unwrap();
    let marker = output.path().join("keep.txt");
    fs::write(&marker, "keep").unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_taskgen"))
        .current_dir(root())
        .args([
            "--workspace-profile",
            "operator-home",
            "--world-image",
            "w:1",
            "--tools-image",
            "t:1",
            "--out",
        ])
        .arg(output.path())
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert_eq!(fs::read_to_string(marker).unwrap(), "keep");
}
