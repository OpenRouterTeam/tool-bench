use std::{fs, path::Path};

use catalog::Catalog;
use scenario::{
    Scenario,
    oracle_tools::OracleTools,
    taskgen::{Images, write_task},
};
use serde_json::Value;

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
fn natural_response_authoring_and_taskgen_do_not_require_an_answer_file_request() {
    let root = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."));
    let catalog = Catalog::load(&root.join("catalog/services.toml")).unwrap();
    let tools = OracleTools::load(root, &catalog).unwrap();
    let temporary = tempfile::tempdir().unwrap();
    let id = "cloud/aws/find-unencrypted-bucket";
    let directory = temporary.path().join(id);
    copy(&root.join("scenarios").join(id), &directory);
    let original = fs::read_to_string(directory.join("scenario.toml")).unwrap();
    assert!(original.contains("[expect.answer]"));
    let contract = original.replace(
        "[expect.answer]",
        "[expect.answer]\nsource = \"final_response\"",
    );
    fs::write(directory.join("scenario.toml"), &contract).unwrap();
    let natural_request = "Our production S3 policy requires SSE-KMS. Which bucket still defaults to SSE-S3? Reply with just its name. Do not change any settings.";
    fs::write(directory.join("instruction.md"), natural_request).unwrap();
    let scenario = Scenario::load(&directory).unwrap();
    scenario
        .validate(temporary.path(), &catalog, &tools)
        .unwrap();
    let generated = tempfile::tempdir().unwrap();
    write_task(
        &scenario,
        &catalog,
        &Images {
            world: "world:fixture".into(),
            tools: "tools:fixture".into(),
        },
        generated.path(),
    )
    .unwrap();
    let task = generated.path().join(scenario.task_name());
    assert_eq!(
        fs::read_to_string(task.join("instruction.md")).unwrap(),
        natural_request
    );
    let script = fs::read_to_string(task.join("tests/test.sh")).unwrap();
    assert!(script.contains("--inference-evidence /exports/inference-evidence.json"));
    let policy: Value =
        serde_json::from_slice(&fs::read(task.join("tests/runner-policy.json")).unwrap()).unwrap();
    assert_eq!(
        policy["inference_adapter"]["name"],
        mock_core::inference::COMPUTER1_ADAPTER_NAME
    );
    assert_eq!(
        policy["inference_adapter"]["code_sha256"]
            .as_str()
            .unwrap()
            .len(),
        71
    );
    let generated_task: toml::Value =
        toml::from_str(&fs::read_to_string(task.join("task.toml")).unwrap()).unwrap();
    assert!(
        generated_task["artifacts"]
            .as_array()
            .unwrap()
            .iter()
            .any(|artifact| artifact["source"].as_str()
                == Some("/exports/inference-evidence.json")
                && artifact["service"].as_str() == Some("world"))
    );
    fs::write(directory.join("scenario.toml"), &original).unwrap();
    assert!(
        Scenario::load(&directory)
            .unwrap()
            .validate(temporary.path(), &catalog, &tools)
            .is_err()
    );
    fs::write(directory.join("scenario.toml"), contract).unwrap();
    fs::write(
        directory.join("instruction.md"),
        "Please write the requested answer to /app/answer.md.",
    )
    .unwrap();
    assert!(
        Scenario::load(&directory)
            .unwrap()
            .validate(temporary.path(), &catalog, &tools)
            .is_err()
    );
}
