use std::{fs, path::Path, process::Command};

use serde_json::{Value, json};

fn run(endpoint: &str, request: Value) -> std::process::Output {
    let directory = tempfile::tempdir().unwrap();
    fs::write(directory.path().join("request.json"), request.to_string()).unwrap();
    fs::write(
        directory.path().join("control.json"),
        r#"{"expected_reward":0}"#,
    )
    .unwrap();
    Command::new(env!("CARGO_BIN_EXE_programcheck"))
        .current_dir(Path::new(env!("CARGO_MANIFEST_DIR")).join("../.."))
        .arg(endpoint)
        .arg(directory.path())
        .output()
        .unwrap()
}

fn request(arguments: Value) -> Value {
    json!({"jsonrpc":"2.0","id":2,"method":"tools/call",
        "params":{"name":"cancel_downtime","arguments":arguments}})
}

#[test]
fn real_locked_schemas_are_checked_without_claiming_dispatch_or_semantics() {
    let result = run("datadog", request(json!({"downtimeId":2207731})));
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let report: Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(report["validated_request_files"], 1);
    assert_eq!(
        report["requests"][0]["validation"]["tool"],
        "cancel_downtime"
    );
    assert_eq!(
        report["requests"][0]["validation"]["result_contract"],
        "datadog_api_v1"
    );
    assert_eq!(report["limits"].as_array().unwrap().len(), 4);
}

#[test]
fn malformed_unknown_and_render_dependent_arguments_are_not_certified() {
    for (endpoint, request) in [
        ("datadog", request(json!({"downtimeId":"not-a-number"}))),
        (
            "datadog",
            request(json!({"downtimeId":"${created_identity}"})),
        ),
        ("datadog", request(json!(null))),
        ("not-an-endpoint", request(json!({"downtimeId":2207731}))),
    ] {
        let result = run(endpoint, request);
        assert!(!result.status.success());
        assert!(result.stdout.is_empty());
    }
}

#[test]
fn an_empty_program_or_missing_context_does_not_pass_vacuously() {
    for request in [
        json!({"expected_reward":1}),
        json!({"method":"tools/call","params":{"name":"cancel_downtime","arguments":{"downtimeId":1}}}),
        json!({"jsonrpc":"2.0","method":"tools/call","params":{"name":"cancel_downtime","arguments":{"downtimeId":1}}}),
    ] {
        assert!(!run("datadog", request).status.success());
    }
}

#[test]
fn retained_datadog_program_files_pass_the_same_locked_schema_interface_as_task_generation() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    for (script, expected_programs) in [
        ("test-datadog-contracts.sh", 42),
        ("test-datadog-tool-errors.sh", 8),
        ("test-datadog-surfaces.sh", 1),
    ] {
        let directory = tempfile::tempdir().unwrap();
        let generated = Command::new("sh")
            .current_dir(&root)
            .arg(root.join("deploy").join(script))
            .env("CONTROL_OUT", directory.path())
            .env("GENERATE_ONLY", "1")
            .env("TASKGEN_BINARY", env!("CARGO_BIN_EXE_taskgen"))
            .env("WORLD_IMAGE", "world:unexecuted-static-control")
            .env("TOOLS_IMAGE", "tools:unexecuted-static-control")
            .output()
            .unwrap();
        assert!(
            generated.status.success(),
            "{script}: {}",
            String::from_utf8_lossy(&generated.stderr)
        );
        let mut programs = fs::read_dir(directory.path())
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .filter(|path| {
                path.is_dir()
                    && path
                        .file_name()
                        .unwrap()
                        .to_string_lossy()
                        .starts_with("program-")
            })
            .collect::<Vec<_>>();
        programs.sort();
        assert_eq!(programs.len(), expected_programs, "{script}");
        if expected_programs == 42 {
            let mut outcomes = [0_usize; 2];
            for program in &programs {
                let control: Value =
                    serde_json::from_slice(&fs::read(program.join("control.json")).unwrap())
                        .unwrap();
                outcomes[control["expected_reward"].as_u64().unwrap() as usize] += 1;
            }
            assert_eq!(outcomes, [24, 18]);
        }
        let checked = Command::new(env!("CARGO_BIN_EXE_programcheck"))
            .current_dir(&root)
            .arg("datadog")
            .args(&programs)
            .output()
            .unwrap();
        assert!(
            checked.status.success(),
            "{script}: {}",
            String::from_utf8_lossy(&checked.stderr)
        );
        let report: Value = serde_json::from_slice(&checked.stdout).unwrap();
        assert!(report["validated_request_files"].as_u64().unwrap() >= expected_programs as u64);
    }
}
