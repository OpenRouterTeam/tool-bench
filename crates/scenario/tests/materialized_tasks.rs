use std::{
    fs,
    path::{Path, PathBuf},
};

use catalog::Catalog;
use mock_core::{
    dataset::Dataset,
    materialization::{bundle_from_directory, validate_bundle},
    provenance::{DatasetSource, TaskContext},
    runner::sha256,
};
use scenario::{
    Scenario, images,
    oracle_tools::OracleTools,
    taskgen::{Images, write_task},
};
use serde_json::json;

const TASK: &str = "example/github/materialized";
const BASE: &str = "_organizations/synth/base.json";

fn root() -> &'static Path {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
}

fn write(root: &Path, name: &str, content: impl AsRef<[u8]>) {
    let destination = root.join(name);
    fs::create_dir_all(destination.parent().unwrap()).unwrap();
    fs::write(destination, content).unwrap();
}

fn fixture(root: &Path) -> PathBuf {
    write(root, BASE, json!({
        "schema_version":1,"id":"synth","revision":1,"description":"Synthetic shared organization",
        "datasets":{"github":{"issues":[{"_key":"repo#1","id":1,"title":"First","state":"open"}]}}
    }).to_string());
    write(root, &format!("{TASK}/overlay.json"), json!({
        "schema_version":1,"task_id":TASK,"base_id":"synth","base_revision":1,
        "inserts":{"github":{"issues":[{"_key":"repo#1","id":1,"title":"First","state":"closed"}]}}
    }).to_string());
    write(
        root,
        &format!("{TASK}/scenario.toml"),
        format!(
            "[scenario]\nid = {TASK:?}\ntitle = \"Synthetic materialization diagnostic\"\n\
         now = \"2026-11-01T09:00:00Z\"\nseed = \"synth\"\nservices = [\"github\"]\n\
         [materialization]\nschema_version = 1\nbase = {BASE:?}\noverlay = \"overlay.json\"\n\
         [expect]\nversion = 2\naudit_history = true\n[expect.answer]\ncontains = [\"First\"]\n\
         [solution]\nanswer = \"First\"\n"
        ),
    );
    write(
        root,
        &format!("{TASK}/instruction.md"),
        "Find the first issue title and write it to /app/answer.md.\n",
    );
    root.join(TASK)
}

fn copy(source: &Path, target: &Path) {
    fs::create_dir_all(target).unwrap();
    for entry in fs::read_dir(source).unwrap() {
        let path = entry.unwrap().path();
        let destination = target.join(path.file_name().unwrap());
        if path.is_dir() {
            copy(&path, &destination);
        } else {
            fs::copy(path, destination).unwrap();
        }
    }
}

#[test]
fn actual_taskgen_writes_replayable_private_inputs_but_never_into_the_agent_environment() {
    let source = tempfile::tempdir().unwrap();
    let scenario_path = fixture(source.path());
    let scenario = Scenario::load(&scenario_path).unwrap();
    let catalog = Catalog::load(&root().join("catalog/services.toml")).unwrap();
    let tools = OracleTools::load(root(), &catalog).unwrap();
    scenario.validate(source.path(), &catalog, &tools).unwrap();
    let output = tempfile::tempdir().unwrap();
    write_task(
        &scenario,
        &catalog,
        &Images {
            world: "world:fixture".into(),
            tools: "tools:fixture".into(),
        },
        output.path(),
    )
    .unwrap();
    let task = output.path().join("example__github__materialized");
    let context: TaskContext =
        serde_json::from_slice(&fs::read(task.join("tests/task-context.json")).unwrap()).unwrap();
    let source: DatasetSource =
        serde_json::from_slice(&fs::read(task.join("tests/task-source.json")).unwrap()).unwrap();
    assert_eq!(context.materialization_files.len(), 4);
    let bundle =
        bundle_from_directory(&task.join("tests"), &context.materialization_files).unwrap();
    validate_bundle(&source, &context.materialization_files, &bundle).unwrap();
    for (name, bytes) in &bundle.files {
        assert_eq!(context.materialization_files[name], sha256(bytes));
        assert!(!task.join("environment").join(name).exists());
        assert!(!task.join("solution").join(name).exists());
    }
    let dockerfile = fs::read_to_string(task.join("environment/Dockerfile")).unwrap();
    assert!(!dockerfile.contains("materialization"));
    let compose = fs::read_to_string(task.join("environment/docker-compose.yaml")).unwrap();
    assert!(!compose.contains("_organizations") && !compose.contains("overlay.json"));
}

#[test]
fn world_source_loader_replays_the_same_materialization_under_the_image_layout() {
    let source = tempfile::tempdir().unwrap();
    let path = fixture(source.path());
    let before = Dataset::load(&path).unwrap();
    let image = tempfile::tempdir().unwrap();
    let image_root = image.path().join("opt/tool-bench/scenarios");
    copy(source.path(), &image_root);
    let inside = Dataset::load(&image_root.join(TASK)).unwrap();
    assert_eq!(before.source, inside.source);
    assert_eq!(
        before.materialization.unwrap().files,
        inside.materialization.unwrap().files
    );
    let dockerfile = fs::read_to_string(root().join("images/world/Dockerfile")).unwrap();
    assert!(dockerfile.contains("COPY scenarios /opt/tool-bench/scenarios"));
}

#[test]
fn actual_world_image_identity_covers_shared_bases_and_task_overlays() {
    let directory = tempfile::tempdir().unwrap();
    let repository = directory.path();
    fixture(&repository.join("scenarios"));
    for name in [
        ".dockerignore",
        "LICENSE",
        "THIRD_PARTY_NOTICES.md",
        "Cargo.toml",
        "Cargo.lock",
        "crates/source.rs",
        "catalog/services.toml",
        "images/world/Dockerfile",
        "deploy/runner/manifest.py",
    ] {
        write(repository, name, b"synthetic image input\n");
    }
    let original = images::world(repository).unwrap();
    write(repository, &format!("scenarios/{BASE}"), b"changed base\n");
    let changed_base = images::world(repository).unwrap();
    assert_ne!(original, changed_base);
    write(
        repository,
        &format!("scenarios/{TASK}/overlay.json"),
        b"changed overlay\n",
    );
    assert_ne!(changed_base, images::world(repository).unwrap());
}

#[test]
fn standalone_taskgen_omits_materialization_fields_and_private_files() {
    let scenario = Scenario::load(&root().join("scenarios/code/github/file-login-bug")).unwrap();
    let catalog = Catalog::load(&root().join("catalog/services.toml")).unwrap();
    let output = tempfile::tempdir().unwrap();
    write_task(
        &scenario,
        &catalog,
        &Images {
            world: "world:fixture".into(),
            tools: "tools:fixture".into(),
        },
        output.path(),
    )
    .unwrap();
    let task = output.path().join(scenario.task_name());
    let context: serde_json::Value =
        serde_json::from_slice(&fs::read(task.join("tests/task-context.json")).unwrap()).unwrap();
    assert!(context.get("materialization_files").is_none());
    assert!(!task.join("tests/materialization").exists());
}
