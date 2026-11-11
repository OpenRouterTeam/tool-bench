use std::{fs, path::Path};

use catalog::Catalog;
use scenario::{
    Scenario, discover,
    oracle_tools::OracleTools,
    taskgen::{Images, write_task},
};

fn root() -> &'static Path {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
}

fn catalog() -> Catalog {
    Catalog::load(&root().join("catalog/services.toml")).unwrap()
}

fn copy_sources(source: &Path, target: &Path) {
    fs::create_dir_all(target).unwrap();
    for entry in fs::read_dir(source).unwrap() {
        let path = entry.unwrap().path();
        let destination = target.join(path.file_name().unwrap());
        if path.is_dir() {
            copy_sources(&path, &destination);
        } else {
            fs::copy(&path, destination).unwrap();
        }
    }
}

fn taskgen(scenarios: &Path, output: &Path, inventory: &Path, check: bool) -> std::process::Output {
    let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_taskgen"));
    command
        .current_dir(root())
        .args([
            "--world-image",
            "w:1",
            "--tools-image",
            "t:1",
            "--scenarios",
        ])
        .arg(scenarios)
        .arg("--out")
        .arg(output)
        .arg("--inventory")
        .arg(inventory);
    if check {
        command.arg("--check");
    }
    command.output().unwrap()
}

#[test]
fn every_scenario_in_the_repo_validates() {
    let scenarios = discover(&root().join("scenarios")).unwrap();
    let tools = OracleTools::load(root(), &catalog()).unwrap();
    assert!(!scenarios.is_empty());
    for s in &scenarios {
        s.validate(&root().join("scenarios"), &catalog(), &tools)
            .unwrap();
    }
}

#[test]
fn templated_final_answers_use_captured_results_and_an_atomic_private_render() {
    let mut scenario =
        Scenario::load(&root().join("scenarios/ops/sentry/create-notification-project")).unwrap();
    scenario.solution.calls = vec![
        serde_json::from_value(serde_json::json!({
            "endpoint":"sentry","tool":"execute_sentry_tool",
            "arguments":{"name":"find_dsns","arguments":{
                "organizationSlug":"lumen-labs","projectSlug":"notification-service"
            }},
            "capture":[{
                "name":"dsn","pointer":"/structuredContent/dsns",
                "select":{"matching":{"name":"Default"},"pointer":"/dsn"}
            }]
        }))
        .unwrap(),
    ];
    scenario.solution.answer = Some("${dsn}".into());
    scenario.solution.answer_template = true;
    scenario.solution.validate_bindings().unwrap();
    let output = tempfile::tempdir().unwrap();
    let images = Images {
        world: "w:1".into(),
        tools: "t:1".into(),
    };
    write_task(&scenario, &catalog(), &images, output.path()).unwrap();
    let task = output
        .path()
        .join("ops__sentry__create-notification-project");
    let script = fs::read_to_string(task.join("solution/solve.sh")).unwrap();
    assert!(script.contains("bound_call 7113"));
    assert!(script.contains("\"$DIR/answer.template.json\" > \"$WORK/answer.json\""));
    assert!(script.contains("> \"$WORK/answer.md\""));
    assert!(script.contains("cp \"$WORK/answer.md\" /app/answer.md"));
    assert!(!script.contains("cp \"$DIR/answer.md\""));
    let template: serde_json::Value =
        serde_json::from_slice(&fs::read(task.join("solution/answer.template.json")).unwrap())
            .unwrap();
    assert_eq!(template["params"]["arguments"], "${dsn}");
    assert!(task.join("solution/render.jq").is_file());
}

#[test]
fn lookup_scenarios_use_preservation_enforcing_contracts() {
    let scenarios = discover(&root().join("scenarios")).unwrap();
    let lookups: Vec<_> = scenarios
        .iter()
        .filter(|scenario| scenario.meta.tags.iter().any(|tag| tag == "lookup"))
        .collect();
    assert_eq!(lookups.len(), 51);
    assert_eq!(
        lookups
            .iter()
            .filter(|scenario| !scenario.meta.id.starts_with("example/"))
            .count(),
        50
    );
    for scenario in lookups {
        assert_eq!(scenario.expect.version, 2, "{}", scenario.meta.id);
        assert!(scenario.expect.audit_history, "{}", scenario.meta.id);
        assert!(
            scenario
                .expect
                .preservation
                .as_ref()
                .is_none_or(|policy| policy.allow.is_empty()),
            "{}",
            scenario.meta.id
        );
    }
}

#[test]
fn generated_task_has_harbor_layout_and_all_endpoints() {
    let s = Scenario::load(&root().join("scenarios/code/github/file-login-bug")).unwrap();
    let out = tempfile::tempdir().unwrap();
    let images = Images {
        world: "w:1".into(),
        tools: "t:1".into(),
    };
    write_task(&s, &catalog(), &images, out.path()).unwrap();
    let task = out.path().join("code__github__file-login-bug");
    for f in [
        "task.toml",
        "instruction.md",
        "environment/Dockerfile",
        "environment/docker-compose.yaml",
        "tests/test.sh",
        "tests/expect.json",
        "tests/runner-policy.json",
        "tests/Dockerfile",
        "tests/scenario.txt",
        "solution/solve.sh",
        "solution/response.jq",
        "solution/tool-result.jq",
        "solution/firecrawl-native.jq",
        "solution/result-contracts.json",
        "solution/call-0.json",
    ] {
        assert!(task.join(f).is_file(), "{f}");
    }
    let config_text = fs::read_to_string(task.join("task.toml")).unwrap();
    assert_eq!(
        config_text.matches("[[environment.mcp_servers]]").count(),
        32
    );
    assert!(config_text.contains("url = \"http://tools:7101/mcp\""));
    let config: toml::Value = toml::from_str(&config_text).unwrap();
    assert_eq!(
        config["verifier"]["environment_mode"].as_str(),
        Some("separate")
    );
    assert_eq!(
        config["verifier"]["environment"]["network_mode"].as_str(),
        Some("no-network")
    );
    assert_eq!(
        config["verifier"]["collect"][0]["service"].as_str(),
        Some("world")
    );
    assert_eq!(
        config["metadata"]["verification_protocol"].as_str(),
        Some("offline-world-evidence-v5")
    );
    let collection = config["verifier"]["collect"][0]["command"]
        .as_str()
        .unwrap();
    assert!(collection.contains("-X POST") && collection.contains("/_bench/finalize"));
    assert_eq!(config["artifacts"][1]["service"].as_str(), Some("world"));
    assert_eq!(
        config["metadata"]["runner_protocol"].as_str(),
        Some("tool-bench-runner-v3")
    );
    assert_eq!(
        config["artifacts"][2]["source"].as_str(),
        Some("/exports/runner-admission.json")
    );
    let policy: mock_core::runner::RunnerPolicy =
        serde_json::from_slice(&fs::read(task.join("tests/runner-policy.json")).unwrap()).unwrap();
    assert_eq!(policy.runner_protocol, "tool-bench-runner-v3");
    assert_eq!(policy.harbor_version, "0.23.0");
    assert_eq!(policy.adapter_sha256.len(), 64);
    let compose = fs::read_to_string(task.join("environment/docker-compose.yaml")).unwrap();
    assert!(compose.contains("image: w:1") && compose.contains("image: t:1"));
    assert!(compose.contains("- github.octo-corp.com"));
    assert!(compose.contains("SCENARIO_ID: \"code/github/file-login-bug\""));
    let call: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(task.join("solution/call-0.json")).unwrap())
            .unwrap();
    assert_eq!(call["params"]["name"], "issue_write");
    let test_sh = fs::read_to_string(task.join("tests/test.sh")).unwrap();
    assert!(test_sh.contains("bench-verify") && test_sh.contains("/exports/evidence.json"));
    assert!(!test_sh.contains("curl") && !test_sh.contains("HARBOR_TRIAL_ADMIN_TOKEN"));
    assert!(compose.contains("BENCH_ADMIN_TOKEN: \"${HARBOR_TRIAL_ADMIN_TOKEN:?"));
    assert!(!compose.contains("BENCH_ALLOW_INSECURE_LOCAL"));
    assert!(compose.contains("backend:\n    internal: true"));
    assert!(compose.contains("networks:\n      backend:\n        aliases:"));
    assert!(compose.contains("default: {}\n      backend: {}"));
    let verifier_image = fs::read_to_string(task.join("tests/Dockerfile")).unwrap();
    assert!(verifier_image.contains("FROM w:1\n"));
    assert!(verifier_image.contains("ENTRYPOINT []"));
    assert!(verifier_image.contains("HEALTHCHECK NONE"));
    assert!(!task.join("tests/admin-token").exists());
}

#[test]
fn diagnostic_examples_require_explicit_generation_and_never_inflate_the_scored_inventory() {
    let directory = tempfile::tempdir().unwrap();
    let output = directory.path().join("diagnostic");
    let inventory = directory.path().join("inventory.json");
    let result = std::process::Command::new(env!("CARGO_BIN_EXE_taskgen"))
        .current_dir(root())
        .args([
            "--world-image",
            "w:1",
            "--tools-image",
            "t:1",
            "--include-examples",
            "--out",
        ])
        .arg(&output)
        .arg("--inventory")
        .arg(&inventory)
        .output()
        .unwrap();
    assert!(result.status.success(), "{result:?}");
    assert_eq!(fs::read_dir(&output).unwrap().count(), 154);
    assert!(
        output
            .join("diagnostic__stripe__no-action-already-refunded/tests/materialization/base.json")
            .is_file()
    );
    assert!(
        output
            .join("example__aws__missing-bucket-policy/solution/solve.sh")
            .is_file()
    );
    let counts: serde_json::Value = serde_json::from_slice(&fs::read(inventory).unwrap()).unwrap();
    assert_eq!(counts["summary"]["scored_tasks"], 150);
    assert_eq!(counts["summary"]["unscored_examples"], 2);
    assert_eq!(counts["summary"]["unscored_diagnostics"], 2);
    assert_eq!(counts["summary"]["reference_calls"], 462);
    assert_eq!(counts["summary"]["static_argument_schemas"], 444);
    assert_eq!(counts["summary"]["deferred_argument_schemas"], 18);
    assert_eq!(counts["summary"]["scored_declared_unscorable"], 0);
    assert!(
        counts["scenarios"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|row| {
                let id = row["id"].as_str().unwrap();
                id.starts_with("example/") || id.starts_with("diagnostic/")
            })
            .all(|row| row["scored"] == false)
    );
    let normal = directory.path().join("normal");
    let result = taskgen(
        &root().join("scenarios"),
        &normal,
        &directory.path().join("normal.json"),
        false,
    );
    assert!(result.status.success(), "{result:?}");
    assert_eq!(fs::read_dir(normal).unwrap().count(), 150);
}

#[test]
fn explicit_direct_access_tasks_receive_a_declared_backend_route() {
    let authored = tempfile::tempdir().unwrap();
    copy_sources(
        &root().join("scenarios/code/github/file-login-bug"),
        authored.path(),
    );
    let path = authored.path().join("scenario.toml");
    let mut contents: toml::Value = toml::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    contents["expect"].as_table_mut().unwrap().insert(
        "interfaces".into(),
        toml::Value::String("direct_allowed".into()),
    );
    fs::write(path, toml::to_string(&contents).unwrap()).unwrap();
    let scenario = Scenario::load(authored.path()).unwrap();
    let output = tempfile::tempdir().unwrap();
    write_task(
        &scenario,
        &catalog(),
        &Images {
            world: "w:1".into(),
            tools: "t:1".into(),
        },
        output.path(),
    )
    .unwrap();
    let compose = fs::read_to_string(
        output
            .path()
            .join("code__github__file-login-bug/environment/docker-compose.yaml"),
    )
    .unwrap();
    let main = compose.split("  world:\n").next().unwrap();
    assert!(main.contains("networks:\n      default: {}\n      backend: {}"));
    assert!(compose.contains("BENCH_API_CALLER_HOST: \"\""));
    assert!(compose.contains("BENCH_ADMIN_TOKEN: \"${HARBOR_TRIAL_ADMIN_TOKEN:?"));
}

#[test]
fn published_image_tags_follow_their_sources() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    for (file, body) in [
        (".dockerignore", "target\n"),
        ("LICENSE", "repository licence"),
        ("THIRD_PARTY_NOTICES.md", "upstream notices"),
        ("Cargo.toml", "a"),
        ("Cargo.lock", "b"),
        ("crates/x.rs", "c"),
        ("scenarios/s.toml", "d"),
        ("images/world/Dockerfile", "e"),
        ("images/tools/Dockerfile", "f"),
        ("catalog/services.toml", "g"),
        ("servers/n.json", "h"),
        ("deploy/runner/admission.py", "i"),
    ] {
        fs::create_dir_all(root.join(file).parent().unwrap()).unwrap();
        fs::write(root.join(file), body).unwrap();
    }
    let before = scenario::images::published(root).unwrap();
    assert!(
        before
            .world
            .starts_with("us-docker.pkg.dev/openrouter-ci/harbor/tool-bench-world:")
    );
    assert!(
        before
            .tools
            .starts_with("us-docker.pkg.dev/openrouter-ci/harbor/tool-bench-tools:")
    );

    fs::create_dir_all(root.join("crates/target")).unwrap();
    fs::write(root.join("crates/target/junk"), "ignored").unwrap();
    fs::create_dir_all(root.join("deploy/runner/__pycache__")).unwrap();
    fs::write(
        root.join("deploy/runner/__pycache__/ignored.pyc"),
        "ignored",
    )
    .unwrap();
    assert_eq!(
        scenario::images::published(root).unwrap().world,
        before.world
    );

    fs::write(root.join("scenarios/s.toml"), "changed").unwrap();
    let after = scenario::images::published(root).unwrap();
    assert_ne!(after.world, before.world);
    assert_eq!(after.tools, before.tools);
    fs::write(root.join("catalog/services.toml"), "header policy changed").unwrap();
    let policy_changed = scenario::images::published(root).unwrap();
    assert_ne!(policy_changed.world, after.world);
    assert_ne!(policy_changed.tools, after.tools);
    fs::write(root.join("deploy/runner/admission.py"), "new runner").unwrap();
    let runner_changed = scenario::images::published(root).unwrap();
    assert_ne!(runner_changed.world, policy_changed.world);
    assert_eq!(runner_changed.tools, policy_changed.tools);
    fs::write(
        root.join("THIRD_PARTY_NOTICES.md"),
        "updated upstream notices",
    )
    .unwrap();
    let notices_changed = scenario::images::published(root).unwrap();
    assert_ne!(notices_changed.world, runner_changed.world);
    assert_ne!(notices_changed.tools, runner_changed.tools);
}

#[test]
fn validation_rejects_answers_without_the_answer_path_and_bad_endpoints() {
    let dir = tempfile::tempdir().unwrap();
    let id = "code/github/bad";
    let d = dir.path().join(id);
    fs::create_dir_all(&d).unwrap();
    fs::write(d.join("instruction.md"), "Tell me the count.").unwrap();
    fs::write(
        d.join("scenario.toml"),
        format!("[scenario]\nid = \"{id}\"\ntitle = \"t\"\nnow = \"2026-01-01T00:00:00Z\"\nseed = \"s\"\nservices = [\"github\"]\n\n[scenario.family]\nschema_version = 1\nprimary = \"baseline\"\ndependency_depth = 0\nrationale = \"A synthetic validation fixture with one direct count.\"\n\n[expect.answer]\ncontains = [\"3\"]\n"),
    )
    .unwrap();
    let s = Scenario::load(&d).unwrap();
    let tools = OracleTools::load(root(), &catalog()).unwrap();
    let err = s
        .validate(dir.path(), &catalog(), &tools)
        .unwrap_err()
        .to_string();
    assert!(err.contains("/app/answer.md"), "{err}");

    fs::write(d.join("instruction.md"), "Write it to /app/answer.md.").unwrap();
    fs::write(
        d.join("scenario.toml"),
        format!("[scenario]\nid = \"{id}\"\ntitle = \"t\"\nnow = \"2026-01-01T00:00:00Z\"\nseed = \"s\"\nservices = [\"github\"]\n\n[scenario.family]\nschema_version = 1\nprimary = \"baseline\"\ndependency_depth = 0\nrationale = \"A synthetic validation fixture with one direct count.\"\n\n[expect.answer]\ncontains = [\"3\"]\n\n[[solution.call]]\nendpoint = \"nope\"\ntool = \"x\"\n"),
    )
    .unwrap();
    let err = Scenario::load(&d)
        .unwrap()
        .validate(
            dir.path(),
            &catalog(),
            &OracleTools::load(root(), &catalog()).unwrap(),
        )
        .unwrap_err()
        .to_string();
    assert!(err.contains("unknown endpoint nope"), "{err}");
}

#[test]
fn image_overrides_skip_hashing_the_sources() {
    let empty = tempfile::tempdir().unwrap();
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_taskgen"))
        .current_dir(empty.path())
        .args([
            "--world-image",
            "w:1",
            "--tools-image",
            "t:1",
            "--print-images",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&output.stdout), "w:1\nt:1\n");
}

#[test]
fn refuses_to_wipe_a_directory_that_is_not_taskgen_output() {
    let out = tempfile::tempdir().unwrap();
    let keep = out.path().join("notes.txt");
    std::fs::write(&keep, "unrelated").unwrap();
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_taskgen"))
        .current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
        .args(["--world-image", "w:1", "--tools-image", "t:1", "--out"])
        .arg(out.path())
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("refusing to delete it"));
    assert_eq!(std::fs::read_to_string(&keep).unwrap(), "unrelated");
}

#[test]
fn invalid_reference_calls_cannot_replace_existing_tasks_or_inventory() {
    let directory = tempfile::tempdir().unwrap();
    let scenarios = directory.path().join("scenarios");
    let authored = scenarios.join("collab/linear/file-safari-crash-bug");
    copy_sources(
        &root().join("scenarios/collab/linear/file-safari-crash-bug"),
        &authored,
    );
    let path = authored.join("scenario.toml");
    let mut document: toml::Value = toml::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    document["solution"]["call"][2]["tool"] =
        toml::Value::String("nonexistent_reference_tool".into());
    fs::write(path, toml::to_string(&document).unwrap()).unwrap();
    let output = directory.path().join("tasks");
    let existing = output.join("existing/task.toml");
    fs::create_dir_all(existing.parent().unwrap()).unwrap();
    fs::write(&existing, "retained task bytes").unwrap();
    let inventory = directory.path().join("inventory.json");
    fs::write(&inventory, "retained inventory bytes").unwrap();

    let result = taskgen(&scenarios, &output, &inventory, false);
    assert!(!result.status.success());
    assert!(
        String::from_utf8_lossy(&result.stderr)
            .contains("unknown reference tool linear.nonexistent_reference_tool")
    );
    assert_eq!(fs::read_to_string(existing).unwrap(), "retained task bytes");
    assert_eq!(
        fs::read_to_string(inventory).unwrap(),
        "retained inventory bytes"
    );
}

#[test]
fn inventory_generation_is_repeatable_and_check_rejects_staleness_without_rewriting_it() {
    let directory = tempfile::tempdir().unwrap();
    let scenarios = directory.path().join("scenarios");
    copy_sources(
        &root().join("scenarios/collab/linear/file-safari-crash-bug"),
        &scenarios.join("collab/linear/file-safari-crash-bug"),
    );
    let output = directory.path().join("tasks");
    let inventory = directory.path().join("coverage/inventory.json");
    let first = taskgen(&scenarios, &output, &inventory, false);
    assert!(
        first.status.success(),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );
    let initial = fs::read(&inventory).unwrap();
    let second = taskgen(&scenarios, &output, &inventory, false);
    assert!(
        second.status.success(),
        "{}",
        String::from_utf8_lossy(&second.stderr)
    );
    assert_eq!(fs::read(&inventory).unwrap(), initial);
    let checked = taskgen(&scenarios, &output, &inventory, true);
    assert!(
        checked.status.success(),
        "{}",
        String::from_utf8_lossy(&checked.stderr)
    );

    fs::write(&inventory, "{}\n").unwrap();
    let rejected = taskgen(&scenarios, &output, &inventory, true);
    assert!(!rejected.status.success());
    assert!(String::from_utf8_lossy(&rejected.stderr).contains("source coverage inventory"));
    assert_eq!(fs::read_to_string(inventory).unwrap(), "{}\n");
    assert!(
        output
            .join("collab__linear__file-safari-crash-bug/task.toml")
            .is_file()
    );
}

#[test]
fn inventory_freshness_tracks_source_changes_that_do_not_change_declaration_counts() {
    let directory = tempfile::tempdir().unwrap();
    let scenarios = directory.path().join("scenarios");
    let authored = scenarios.join("collab/linear/file-safari-crash-bug");
    copy_sources(
        &root().join("scenarios/collab/linear/file-safari-crash-bug"),
        &authored,
    );
    let output = directory.path().join("tasks");
    let inventory = directory.path().join("inventory.json");
    for relative in [
        "scenario.toml",
        "instruction.md",
        "dataset/linear/issues.json",
    ] {
        let generated = taskgen(&scenarios, &output, &inventory, false);
        assert!(
            generated.status.success(),
            "{}",
            String::from_utf8_lossy(&generated.stderr)
        );
        let path = authored.join(relative);
        let original = fs::read_to_string(&path).unwrap();
        fs::write(&path, format!("{original}\n")).unwrap();
        let checked = taskgen(&scenarios, &output, &inventory, true);
        assert!(
            !checked.status.success(),
            "{relative}: changed source was not detected"
        );
        assert!(
            String::from_utf8_lossy(&checked.stderr).contains("source coverage inventory"),
            "{relative}: {}",
            String::from_utf8_lossy(&checked.stderr)
        );
    }
}

#[test]
fn generated_compose_passes_per_scenario_business_time_to_tools_container() {
    let s =
        Scenario::load(&root().join("scenarios/ops/grafana/silence-replica-maintenance")).unwrap();
    assert_eq!(s.meta.now, "2026-11-12T15:30:00Z");
    let out = tempfile::tempdir().unwrap();
    let images = Images {
        world: "w:1".into(),
        tools: "t:1".into(),
    };
    write_task(&s, &catalog(), &images, out.path()).unwrap();
    let task = out.path().join("ops__grafana__silence-replica-maintenance");
    let compose = fs::read_to_string(task.join("environment/docker-compose.yaml")).unwrap();
    assert!(
        compose.contains(&format!("TOOL_BENCH_SCENARIO_NOW: \"{}\"", s.meta.now)),
        "compose must pass TOOL_BENCH_SCENARIO_NOW with the scenario's now value"
    );
    let toml_text = fs::read_to_string(task.join("task.toml")).unwrap();
    let config: toml::Value = toml::from_str(&toml_text).unwrap();
    assert_eq!(
        config["metadata"]["business_reference_time"].as_str(),
        Some("2026-11-12T15:30:00Z"),
        "task.toml metadata must record the scenario's business_reference_time"
    );
    assert!(
        config["metadata"]["clock_mode"].as_str().is_some(),
        "task.toml metadata must include clock_mode"
    );
}

#[test]
fn grafana_scenario_connections_manifest_includes_grafana_account() {
    let s =
        Scenario::load(&root().join("scenarios/ops/grafana/silence-replica-maintenance")).unwrap();
    let out = tempfile::tempdir().unwrap();
    let images = Images {
        world: "w:1".into(),
        tools: "t:1".into(),
    };
    write_task(&s, &catalog(), &images, out.path()).unwrap();
    let task = out.path().join("ops__grafana__silence-replica-maintenance");
    let connections: serde_json::Value =
        serde_json::from_slice(&fs::read(task.join("tests/connections.json")).unwrap()).unwrap();
    assert_eq!(connections["schema_version"], 1);
    let grafana = &connections["accounts"]["grafana"];
    assert_eq!(
        grafana["origin"].as_str(),
        Some("https://grafana.octo-corp.com")
    );
    assert!(
        grafana["environment"]["GRAFANA_URL"]
            .as_str()
            .unwrap()
            .starts_with("https://grafana.octo-corp.com"),
        "connection environment must use the declared origin"
    );
}
