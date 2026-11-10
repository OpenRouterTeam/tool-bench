use std::{fs, path::Path};

use mock_core::{dataset::Dataset, runner::sha256};
use report::{compare, load_trials};
use serde_json::{Value, json};

mod support;

const TASK: &str = "example/github/materialized";

fn write(root: &Path, name: &str, value: &Value) {
    let destination = root.join(name);
    fs::create_dir_all(destination.parent().unwrap()).unwrap();
    fs::write(destination, serde_json::to_vec(value).unwrap()).unwrap();
}

fn fixture(root: &Path, trials: &Path) {
    write(
        root,
        "base.json",
        &json!({
            "schema_version":1,"id":"synth","revision":1,"description":"Synthetic organization",
            "datasets":{"github":{"issues":[{"id":1,"title":"Maya","state":"open"}]}}
        }),
    );
    write(
        root,
        &format!("{TASK}/overlay.json"),
        &json!({
            "schema_version":1,"task_id":TASK,"base_id":"synth","base_revision":1
        }),
    );
    let path = root.join(TASK);
    fs::write(
        path.join("scenario.toml"),
        format!(
            "[scenario]\nid = {TASK:?}\nnow = \"2026-11-01T09:00:00Z\"\nseed = \"synth\"\n\
         [materialization]\nschema_version = 1\nbase = \"base.json\"\noverlay = \"overlay.json\"\n\
         [expect]\nversion = 2\naudit_history = true\n[expect.answer]\ncontains = [\"Maya\"]\n"
        ),
    )
    .unwrap();
    support::write_materialized(trials, "trial", Dataset::load(&path).unwrap());
}

fn read(root: &Path, name: &str) -> Value {
    serde_json::from_slice(&fs::read(root.join(name)).unwrap()).unwrap()
}

fn rebind_changed_private_file(verifier: &Path, name: &str) {
    let mut context = read(verifier, "task-context.json");
    context["materialization_files"][name] = json!(sha256(&fs::read(verifier.join(name)).unwrap()));
    write(verifier, "task-context.json", &context);
    let context_hash = sha256(&fs::read(verifier.join("task-context.json")).unwrap());
    let mut manifest = read(verifier, "realized-manifest.json");
    manifest["context"] = context;
    manifest["task_context_sha256"] = json!(context_hash);
    write(verifier, "realized-manifest.json", &manifest);
    let manifest_hash = sha256(&fs::read(verifier.join("realized-manifest.json")).unwrap());
    let mut admission = read(verifier, "runner-admission.json");
    admission["manifest_sha256"] = json!(manifest_hash);
    write(verifier, "runner-admission.json", &admission);
    let mut verification = read(verifier, "verification.json");
    verification["task_context_sha256"] = json!(context_hash);
    verification["manifest_sha256"] = json!(manifest_hash);
    verification["runner_admission_sha256"] = json!(sha256(
        &fs::read(verifier.join("runner-admission.json")).unwrap()
    ));
    write(verifier, "verification.json", &verification);
}

#[test]
fn report_reproduces_a_materialized_trial_from_the_private_bundle() {
    let source = tempfile::tempdir().unwrap();
    let trials = tempfile::tempdir().unwrap();
    fixture(source.path(), trials.path());
    let loaded = load_trials(trials.path()).unwrap();
    assert_eq!(loaded.len(), 1);
    let provenance = loaded[0].provenance.as_ref().unwrap();
    assert!(provenance.source.materialization.is_some());
    assert_eq!(provenance.manifest.context.materialization_files.len(), 4);
    assert!(compare(&loaded, &loaded).is_ok());
}

#[test]
fn report_rejects_missing_or_substituted_private_materialization_files() {
    for name in ["base", "overlay", "manifest", "dataset"] {
        let source = tempfile::tempdir().unwrap();
        let trials = tempfile::tempdir().unwrap();
        fixture(source.path(), trials.path());
        let path = trials
            .path()
            .join(format!("trial/verifier/materialization/{name}.json"));
        let original = fs::read(&path).unwrap();
        fs::write(&path, "{}").unwrap();
        assert!(load_trials(trials.path()).is_err(), "{name}");
        fs::remove_file(&path).unwrap();
        assert!(load_trials(trials.path()).is_err(), "{name}");
        fs::write(&path, original).unwrap();
        assert!(load_trials(trials.path()).is_ok(), "{name}");
    }
}

#[test]
fn rehashing_a_wrong_render_cannot_replace_replaying_the_base_and_overlay() {
    let source = tempfile::tempdir().unwrap();
    let trials = tempfile::tempdir().unwrap();
    fixture(source.path(), trials.path());
    let verifier = trials.path().join("trial/verifier");
    let name = "materialization/dataset.json";
    let mut dataset = read(&verifier, name);
    dataset["github"]["issues"]["1"]["title"] = json!("Forged");
    write(&verifier, name, &dataset);
    rebind_changed_private_file(&verifier, name);
    let error = load_trials(trials.path()).unwrap_err().to_string();
    assert!(
        error.contains("differs from its base and overlay"),
        "{error}"
    );
}

#[test]
fn rehashing_a_changed_raw_source_does_not_replace_the_authored_source_identity() {
    let source = tempfile::tempdir().unwrap();
    let trials = tempfile::tempdir().unwrap();
    fixture(source.path(), trials.path());
    let verifier = trials.path().join("trial/verifier");
    let name = "materialization/base.json";
    let mut base = read(&verifier, name);
    base["description"] = json!("Substituted organization");
    write(&verifier, name, &base);
    rebind_changed_private_file(&verifier, name);
    let error = load_trials(trials.path()).unwrap_err().to_string();
    assert!(error.contains("authored source bytes"), "{error}");
}

#[cfg(unix)]
#[test]
fn report_does_not_follow_links_to_materialization_inputs() {
    let source = tempfile::tempdir().unwrap();
    let trials = tempfile::tempdir().unwrap();
    fixture(source.path(), trials.path());
    let verifier = trials.path().join("trial/verifier");
    fs::rename(verifier.join("materialization"), verifier.join("elsewhere")).unwrap();
    std::os::unix::fs::symlink(verifier.join("elsewhere"), verifier.join("materialization"))
        .unwrap();
    assert!(load_trials(trials.path()).is_err());
}
