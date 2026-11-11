use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use mock_core::{
    dataset::Dataset,
    evidence::WorldEvidence,
    materialization::{self, Bundle},
    runner::sha256,
    world::WorldState,
};
use serde_json::{Value, json};

const BASE: &str = "_organizations/synth/base.json";
const TASK: &str = "example/github/materialized";

fn write(root: &Path, name: &str, bytes: impl AsRef<[u8]>) {
    let destination = root.join(name);
    fs::create_dir_all(destination.parent().unwrap()).unwrap();
    fs::write(destination, bytes).unwrap();
}

fn base() -> Value {
    json!({
        "schema_version":1,"id":"synth","revision":1,"description":"Synthetic organization",
        "datasets":{"github":{"issues":[
            {"_key":"repo#2","id":2,"title":"Second","state":"open"},
            {"_key":"repo#1","id":1,"title":"First","state":"open"}
        ]}}
    })
}

fn overlay(task: &str) -> Value {
    json!({
        "schema_version":1,"task_id":task,"base_id":"synth","base_revision":1,
        "inserts":{"github":{"issues":[{"_key":"repo#1","id":1,"title":"First","state":"closed"}]}}
    })
}

fn fixture(root: &Path, task: &str) -> PathBuf {
    write(root, BASE, base().to_string());
    let declaration = format!(
        "[scenario]\nid = {task:?}\nnow = \"2026-11-01T09:00:00Z\"\nseed = \"synth\"\n\
         [materialization]\nschema_version = 1\nbase = {BASE:?}\noverlay = \"overlay.json\"\n\
         [expect]\nversion = 2\naudit_history = true\n[expect.answer]\ncontains = [\"First\"]\n"
    );
    write(root, &format!("{task}/scenario.toml"), declaration);
    write(
        root,
        &format!("{task}/overlay.json"),
        overlay(task).to_string(),
    );
    root.join(task)
}

fn hashes(bundle: &Bundle) -> BTreeMap<String, String> {
    bundle
        .files
        .iter()
        .map(|(name, bytes)| (name.clone(), sha256(bytes)))
        .collect()
}

fn check(dataset: &Dataset, bundle: &Bundle) -> Result<(), String> {
    materialization::validate_bundle(&dataset.source, &hashes(bundle), bundle)
}

#[test]
fn actual_dataset_loader_applies_overlay_without_mutating_the_shared_base() {
    let directory = tempfile::tempdir().unwrap();
    let task = fixture(directory.path(), TASK);
    let authored = fs::read(directory.path().join(BASE)).unwrap();
    let first = Dataset::load(&task).unwrap();
    let second = Dataset::load(&task).unwrap();
    let issues = first.services["github"].collection("issues").unwrap();
    assert_eq!(issues.get("repo#1").unwrap()["state"], "closed");
    assert_eq!(
        issues.iter().map(|(key, _)| key).collect::<Vec<_>>(),
        ["repo#2", "repo#1"]
    );
    assert_eq!(first.source, second.source);
    assert_eq!(
        first.materialization.as_ref().unwrap().files,
        second.materialization.as_ref().unwrap().files
    );
    assert_eq!(fs::read(directory.path().join(BASE)).unwrap(), authored);
    check(&first, first.materialization.as_ref().unwrap()).unwrap();
}

#[test]
fn shared_base_byte_edits_invalidate_every_dependent_task_even_without_a_state_change() {
    let directory = tempfile::tempdir().unwrap();
    let first_path = fixture(directory.path(), TASK);
    let second_path = fixture(directory.path(), "example/github/other");
    let first = Dataset::load(&first_path).unwrap();
    let second = Dataset::load(&second_path).unwrap();
    let mut bytes = fs::read(directory.path().join(BASE)).unwrap();
    bytes.push(b'\n');
    write(directory.path(), BASE, bytes);
    for (path, before) in [(&first_path, &first), (&second_path, &second)] {
        let after = Dataset::load(path).unwrap();
        assert_ne!(before.source.dataset_sha256, after.source.dataset_sha256);
        assert_eq!(
            before
                .source
                .materialization
                .as_ref()
                .unwrap()
                .materialized_sha256,
            after
                .source
                .materialization
                .as_ref()
                .unwrap()
                .materialized_sha256
        );
        assert_ne!(
            before.materialization.as_ref().unwrap().files,
            after.materialization.as_ref().unwrap().files
        );
    }
    let mut changed = base();
    changed["datasets"]["github"]["issues"][0]["title"] = json!("Changed organization");
    write(directory.path(), BASE, changed.to_string());
    assert_ne!(
        first
            .source
            .materialization
            .as_ref()
            .unwrap()
            .materialized_sha256,
        Dataset::load(&first_path)
            .unwrap()
            .source
            .materialization
            .unwrap()
            .materialized_sha256
    );
}

#[test]
fn fresh_worlds_clone_and_reset_independently_of_sources_and_other_trials() {
    let directory = tempfile::tempdir().unwrap();
    let path = fixture(directory.path(), TASK);
    let dataset = Dataset::load(&path).unwrap();
    let before = dataset.materialization.as_ref().unwrap().files.clone();
    let mut first = WorldState::from_dataset(dataset.clone()).unwrap();
    let second = WorldState::from_dataset(dataset).unwrap();
    first
        .service_mut("github")
        .store
        .collection_mut("issues")
        .get_mut("repo#1")
        .unwrap()["title"] = json!("Only this trial");
    assert_ne!(first.snapshot(), second.snapshot());
    first.reset().unwrap();
    assert_eq!(first.snapshot(), second.snapshot());
    assert_eq!(
        Dataset::load(&path).unwrap().materialization.unwrap().files,
        before
    );
}

#[test]
fn record_order_is_part_of_the_materialized_hash_and_pagination_state() {
    let directory = tempfile::tempdir().unwrap();
    let path = fixture(directory.path(), TASK);
    let before = Dataset::load(&path).unwrap();
    let mut changed = base();
    changed["datasets"]["github"]["issues"]
        .as_array_mut()
        .unwrap()
        .reverse();
    write(directory.path(), BASE, changed.to_string());
    let after = Dataset::load(&path).unwrap();
    assert_ne!(
        before.source.materialization.unwrap().materialized_sha256,
        after.source.materialization.unwrap().materialized_sha256
    );
    assert_eq!(
        after.services["github"]
            .collection("issues")
            .unwrap()
            .iter()
            .map(|(key, _)| key)
            .collect::<Vec<_>>(),
        ["repo#1", "repo#2"]
    );
}

#[test]
fn loader_rejects_duplicate_documents_duplicate_json_members_and_precision_loss() {
    for authored in [
        r#"{"schema_version":1,"id":"synth","revision":1,"description":"","datasets":{"github":{"issues":[{"id":1},{"id":1}]}}}"#,
        r#"{"schema_version":1,"id":"synth","revision":1,"description":"","datasets":{"github":{"issues":[{"id":1,"title":"a","title":"b"}]}}}"#,
        r#"{"schema_version":1,"id":"synth","revision":1,"description":"","datasets":{"github":{"issues":[{"id":1,"amount":0.123456789012345678901}]}}}"#,
        r#"{"schema_version":1,"id":"synth","revision":1,"description":"","datasets":{"github":{"issues":[{"title":"No key"}]}}}"#,
    ] {
        let directory = tempfile::tempdir().unwrap();
        let path = fixture(directory.path(), TASK);
        write(directory.path(), BASE, authored);
        assert!(Dataset::load(&path).is_err(), "{authored}");
    }
    let directory = tempfile::tempdir().unwrap();
    let path = fixture(directory.path(), TASK);
    let mut changed = overlay(TASK);
    let document = changed["inserts"]["github"]["issues"][0].clone();
    changed["inserts"]["github"]["issues"]
        .as_array_mut()
        .unwrap()
        .push(document);
    write(&path, "overlay.json", changed.to_string());
    assert!(Dataset::load(&path).is_err());
}

#[test]
fn mismatched_task_base_revision_and_unsupported_versions_fail_closed() {
    for (field, value) in [
        ("schema_version", json!(2)),
        ("task_id", json!("example/github/wrong")),
        ("base_id", json!("another")),
        ("base_revision", json!(2)),
    ] {
        let directory = tempfile::tempdir().unwrap();
        let path = fixture(directory.path(), TASK);
        let mut changed = overlay(TASK);
        changed[field] = value;
        write(&path, "overlay.json", changed.to_string());
        assert!(Dataset::load(&path).is_err(), "{field}");
    }
    for (field, value) in [
        ("schema_version", json!(2)),
        ("revision", json!(0)),
        ("id", json!("")),
    ] {
        let directory = tempfile::tempdir().unwrap();
        let path = fixture(directory.path(), TASK);
        let mut changed = base();
        changed[field] = value;
        write(directory.path(), BASE, changed.to_string());
        assert!(Dataset::load(&path).is_err(), "{field}");
    }
}

#[test]
fn traversal_absolute_and_ambiguous_source_paths_are_rejected() {
    for name in [
        "../base.json",
        "/tmp/base.json",
        "a//b.json",
        "./base.json",
        "a\\b.json",
        "a/../b.json",
        "C:/base.json",
    ] {
        for field in ["base", "overlay"] {
            let directory = tempfile::tempdir().unwrap();
            let path = fixture(directory.path(), TASK);
            let mut definition: toml::Value =
                toml::from_str(&fs::read_to_string(path.join("scenario.toml")).unwrap()).unwrap();
            definition["materialization"][field] = toml::Value::String(name.into());
            write(
                &path,
                "scenario.toml",
                toml::to_string(&definition).unwrap(),
            );
            assert!(Dataset::load(&path).is_err(), "{field}: {name}");
        }
    }
}

#[cfg(unix)]
#[test]
fn base_overlay_and_intermediate_symlinks_are_never_followed() {
    use std::os::unix::fs::symlink;
    for target in [
        BASE,
        "example/github/materialized/overlay.json",
        "_organizations/synth",
    ] {
        let directory = tempfile::tempdir().unwrap();
        let path = fixture(directory.path(), TASK);
        let original = directory.path().join(target);
        let moved = directory.path().join("moved");
        fs::rename(&original, &moved).unwrap();
        symlink(&moved, &original).unwrap();
        assert!(Dataset::load(&path).is_err(), "{target}");
    }
}

#[test]
fn standalone_data_and_legacy_contracts_cannot_be_mixed_with_materialization() {
    let directory = tempfile::tempdir().unwrap();
    let path = fixture(directory.path(), TASK);
    fs::create_dir(path.join("dataset")).unwrap();
    assert!(Dataset::load(&path).is_err());
    fs::remove_dir(path.join("dataset")).unwrap();
    let definition = fs::read_to_string(path.join("scenario.toml")).unwrap();
    write(
        &path,
        "scenario.toml",
        definition.replace("version = 2", "version = 1"),
    );
    assert!(Dataset::load(&path).is_err());
}

#[test]
fn bundle_verification_replays_authored_sources_and_rejects_rehashed_wrong_renders() {
    let directory = tempfile::tempdir().unwrap();
    let path = fixture(directory.path(), TASK);
    let dataset = Dataset::load(&path).unwrap();
    let original = dataset.materialization.as_ref().unwrap();
    for name in original.files.keys() {
        let mut missing = original.clone();
        missing.files.remove(name);
        assert!(check(&dataset, &missing).is_err(), "{name}");
        let mut changed = original.clone();
        changed.files.insert(name.clone(), b"{}".to_vec());
        assert!(check(&dataset, &changed).is_err(), "{name}");
    }
    let mut forged = original.clone();
    let mut state: Value =
        serde_json::from_slice(&forged.files["materialization/dataset.json"]).unwrap();
    state["github"]["issues"]["repo#1"]["state"] = json!("open");
    forged.files.insert(
        "materialization/dataset.json".into(),
        serde_json::to_vec(&state).unwrap(),
    );
    assert!(
        check(&dataset, &forged)
            .unwrap_err()
            .contains("differs from its base and overlay")
    );
}

#[test]
fn standalone_source_serialization_and_digest_remain_unchanged() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/example/github/basic");
    let dataset = Dataset::load(&path).unwrap();
    let source: Value = serde_json::to_value(&dataset.source).unwrap();
    assert!(source.get("materialization").is_none());
    assert!(dataset.materialization.is_none());
    assert_eq!(
        dataset.source.dataset_sha256,
        "sha256:ba65fb44bd157db11382981ba9750806ccc1698c285625935ce288cd0a8f4517"
    );
}

fn verify(root: &Path, path: &Path, include_context: bool) -> Value {
    let mut command = Command::new(env!("CARGO_BIN_EXE_bench-verify"));
    command.current_dir(root).args([
        "--scenario",
        TASK,
        "--contract",
        "contract.json",
        "--evidence",
        "evidence.json",
        "--task-source",
        "task-source.json",
        "--answer",
        "answer.md",
        "--output",
        "result",
    ]);
    if include_context {
        command
            .arg("--task-context")
            .arg(path.join("task-context.json"));
    }
    let output = command.output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&fs::read(root.join("result/assessment.json")).unwrap()).unwrap()
}

#[test]
fn real_offline_verifier_requires_and_copies_the_private_bundle() {
    let directory = tempfile::tempdir().unwrap();
    let path = fixture(directory.path(), TASK);
    let dataset = Dataset::load(&path).unwrap();
    let bundle = dataset.materialization.clone().unwrap();
    let contract = dataset.contract.clone();
    let mut world = WorldState::from_dataset(dataset).unwrap();
    world.seal().unwrap();
    let evidence = WorldEvidence::capture(&world, &[], Ok(vec!["192.0.2.10".into()]));
    for (name, value) in [
        ("evidence.json", serde_json::to_value(evidence).unwrap()),
        ("contract.json", serde_json::to_value(contract).unwrap()),
        (
            "task-source.json",
            serde_json::to_value(world.source()).unwrap(),
        ),
    ] {
        write(directory.path(), name, value.to_string());
    }
    write(directory.path(), "answer.md", "First");
    let private = directory.path().join("private");
    for (name, bytes) in &bundle.files {
        write(&private, name, bytes);
    }
    write(
        &private,
        "task-context.json",
        json!({
            "schema_version":1,"scenario":TASK,"instruction_sha256":sha256(b"instruction"),
            "profile_sha256":sha256(b"profile"),"framing_sha256":sha256(b"framing"),
            "environment_files":{"Dockerfile":sha256(b"environment")},
            "world_reference":"world:fixture","tools_reference":"tools:fixture",
            "materialization_files":hashes(&bundle)
        })
        .to_string(),
    );
    let passed = verify(directory.path(), &private, true);
    assert_eq!(passed["outcome"], "pass", "{passed}");
    for (name, bytes) in &bundle.files {
        assert_eq!(
            fs::read(directory.path().join("result").join(name)).unwrap(),
            *bytes
        );
    }
    assert_eq!(
        verify(directory.path(), &private, false)["outcome"],
        "environment_error"
    );
    write(&private, "materialization/base.json", "{}");
    let failed = verify(directory.path(), &private, true);
    assert_eq!(failed["outcome"], "environment_error");
    assert_eq!(failed["answer_ok"], true);
    assert!(
        failed["failures"]
            .as_array()
            .unwrap()
            .iter()
            .any(|failure| failure["location"] == "evidence/materialization")
    );
}
