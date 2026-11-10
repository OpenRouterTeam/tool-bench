//! Controlled exposure through the real consumers: taskgen output, the registration
//! directive in the generated compose file, and the bridge serving the pinned definitions.

use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use catalog::Catalog;
use mcp_tools::{
    exposure::{EXPOSURE_ENV, ExposedBackend, ExposureDirective, definition_sha256},
    front::{Backend, Reply},
    replica::ReplicaBackend,
    snapshot::Snapshot,
};
use mock_core::conditions::{ExposureManifest, ExposureVariant};
use scenario::{
    Scenario,
    exposure::{Exposure, PinnedCatalog, realized_catalog_sha256},
    framing::Framing,
    profile::WorkspaceProfile,
    taskgen::{
        Conditions, Images, Treatments, write_task_with_conditions, write_task_with_treatments,
    },
};
use serde_json::{Value, json};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn catalog() -> Catalog {
    Catalog::load(&root().join("catalog/services.toml")).unwrap()
}

fn images() -> Images {
    Images {
        world: "world:fixture".into(),
        tools: "tools:fixture".into(),
    }
}

fn scenario(id: &str) -> Scenario {
    Scenario::load(&root().join("scenarios").join(id)).unwrap()
}

fn exposure(variant: ExposureVariant, seed: u64, token_budget: Option<usize>) -> Exposure {
    Exposure {
        variant,
        seed,
        token_budget,
        pinned: PinnedCatalog::load(&root(), &catalog()).unwrap(),
    }
}

fn generate(
    id: &str,
    profile: WorkspaceProfile,
    exposure: Option<&Exposure>,
) -> (tempfile::TempDir, PathBuf) {
    let out = tempfile::tempdir().unwrap();
    let scenario = scenario(id);
    write_task_with_treatments(
        &scenario,
        &catalog(),
        &images(),
        out.path(),
        Conditions {
            workspace_profile: profile,
            framing: Framing::Neutral,
            review_policy: None,
            locked_tools: None,
        },
        Treatments {
            exposure,
            awareness: None,
        },
    )
    .unwrap();
    let task = out.path().join(scenario.task_name());
    (out, task)
}

fn tree(directory: &Path) -> BTreeMap<String, Vec<u8>> {
    fn visit(root: &Path, directory: &Path, files: &mut BTreeMap<String, Vec<u8>>) {
        for entry in fs::read_dir(directory).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                visit(root, &path, files);
            } else {
                files.insert(
                    path.strip_prefix(root)
                        .unwrap()
                        .to_str()
                        .unwrap()
                        .to_owned(),
                    fs::read(&path).unwrap(),
                );
            }
        }
    }
    let mut files = BTreeMap::new();
    visit(directory, directory, &mut files);
    files
}

fn registered_servers(task: &Path) -> Vec<String> {
    let text = fs::read_to_string(task.join("task.toml")).unwrap();
    let value: toml::Value = toml::from_str(&text).unwrap();
    value["environment"]["mcp_servers"]
        .as_array()
        .unwrap()
        .iter()
        .map(|server| server["name"].as_str().unwrap().to_owned())
        .collect()
}

/// The directive exactly as the generated compose file hands it to the tools container.
fn directive(task: &Path) -> Option<(String, ExposureDirective)> {
    let compose = fs::read_to_string(task.join("environment/docker-compose.yaml")).unwrap();
    let line = compose
        .lines()
        .find(|line| line.trim_start().starts_with(&format!("{EXPOSURE_ENV}:")))?;
    let quoted = line.split_once(": ").unwrap().1;
    let text: String = serde_json::from_str(quoted).unwrap();
    let directive = ExposureDirective::parse(&text).unwrap();
    Some((text, directive))
}

fn manifest(task: &Path) -> ExposureManifest {
    serde_json::from_slice(&fs::read(task.join("tests/exposure.json")).unwrap()).unwrap()
}

/// What the bridge registers for one endpoint when it serves the authentic pinned snapshot
/// behind the task's directive: `(tool name, served definition hash)` in `tools/list` order.
async fn bridge_listing(directive: &ExposureDirective, endpoint: &str) -> Vec<(String, String)> {
    let snapshot: Snapshot = serde_json::from_slice(
        &fs::read(root().join(format!("servers/snapshots/{endpoint}.json"))).unwrap(),
    )
    .unwrap();
    let backend = ExposedBackend::new(
        ReplicaBackend::new(snapshot),
        directive.endpoint(endpoint).unwrap(),
    );
    backend.open().await.unwrap();
    let Reply::Json(response) = backend
        .request(
            &(),
            json!({"jsonrpc": "2.0", "id": 1, "method": "tools/list", "params": {}}),
        )
        .await
        .unwrap()
    else {
        panic!("tool lists are not streamed")
    };
    assert!(response["error"].is_null(), "{response}");
    response["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|tool| {
            (
                tool["name"].as_str().unwrap().to_owned(),
                definition_sha256(tool),
            )
        })
        .collect()
}

const GRAFANA: &str = "ops/grafana/ack-checkout-page-oncall";

#[test]
fn default_and_explicit_full_catalog_generate_identical_bytes_with_no_exposure_artifacts() {
    let out = tempfile::tempdir().unwrap();
    let scenario = scenario(GRAFANA);
    write_task_with_conditions(
        &scenario,
        &catalog(),
        &images(),
        out.path(),
        Conditions::default(),
    )
    .unwrap();
    let baseline = tree(out.path());
    let full = exposure(ExposureVariant::FullCatalog, 99, Some(1));
    let (explicit, task) = generate(GRAFANA, WorkspaceProfile::Minimal, Some(&full));
    assert_eq!(baseline, tree(explicit.path()));
    assert!(!task.join("tests/exposure.json").exists());
    assert!(directive(&task).is_none());
    let catalog = catalog();
    let all: Vec<String> = catalog
        .services
        .iter()
        .flat_map(|service| service.endpoints.iter().map(|endpoint| endpoint.id.clone()))
        .collect();
    assert_eq!(registered_servers(&task), all);
    let context: Value =
        serde_json::from_slice(&fs::read(task.join("tests/task-context.json")).unwrap()).unwrap();
    assert!(context.get("exposure_sha256").is_none());
    assert!(context["connector_env_sha256"].get(EXPOSURE_ENV).is_none());
}

#[tokio::test]
async fn every_variant_decides_what_the_task_registers_and_what_the_bridge_serves() {
    for (variant, budget) in [
        (ExposureVariant::RelevantOnly, None),
        (ExposureVariant::OrganizationPack, None),
        (ExposureVariant::ContextFeasible, Some(120_000)),
        (ExposureVariant::RandomizedOrder, None),
        (ExposureVariant::ConfusingDistractors, None),
    ] {
        let requested = exposure(variant, 7, budget);
        let (_out, task) = generate(GRAFANA, WorkspaceProfile::Support, Some(&requested));
        let manifest = manifest(&task);
        let (text, directive) = directive(&task).expect("registration directive");
        let label = variant.label();

        // The agent's MCP server list, its workspace inventory and the tools container
        // all carry the same realized endpoint order.
        assert_eq!(registered_servers(&task), manifest.endpoints, "{label}");
        let inventory: Value = serde_json::from_slice(
            &fs::read(
                task.join("environment/profile/workspace/.config/workspace/connections.json"),
            )
            .unwrap(),
        )
        .unwrap();
        let listed: Vec<&str> = inventory["connections"]
            .as_array()
            .unwrap()
            .iter()
            .map(|connection| connection["name"].as_str().unwrap())
            .collect();
        assert_eq!(listed, manifest.endpoints, "{label}");
        let registered: Vec<&str> = directive
            .endpoints
            .iter()
            .map(|endpoint| endpoint.id.as_str())
            .collect();
        assert_eq!(registered, manifest.endpoints, "{label}");

        // The bridge, serving the authentic pinned snapshot behind that directive,
        // registers exactly the manifest's tools, order and definition hashes.
        let mut served = Vec::new();
        for endpoint in &manifest.endpoints {
            for (tool, definition) in bridge_listing(&directive, endpoint).await {
                served.push((endpoint.clone(), tool, definition));
            }
        }
        let recorded: Vec<_> = manifest
            .tools
            .iter()
            .map(|tool| {
                (
                    tool.endpoint.clone(),
                    tool.tool.clone(),
                    tool.definition_sha256.clone(),
                )
            })
            .collect();
        assert_eq!(served, recorded, "{label}");
        assert_eq!(
            realized_catalog_sha256(served.iter().map(|(e, t, d)| (
                e.as_str(),
                t.as_str(),
                d.as_str()
            ))),
            manifest.catalog_sha256,
            "{label}"
        );
        assert!(
            manifest
                .tools
                .iter()
                .enumerate()
                .all(|(index, tool)| tool.position == index)
        );

        // The private context binds both the manifest and the directive the runner
        // must observe on the tools container.
        let context: Value =
            serde_json::from_slice(&fs::read(task.join("tests/task-context.json")).unwrap())
                .unwrap();
        assert_eq!(
            context["exposure_sha256"],
            mock_core::runner::sha256(&fs::read(task.join("tests/exposure.json")).unwrap()),
        );
        assert_eq!(
            context["connector_env_sha256"][EXPOSURE_ENV],
            mock_core::runner::sha256(text.as_bytes())
        );
        assert_eq!(
            manifest.directive_sha256,
            mock_core::runner::sha256(text.as_bytes())
        );

        // Catalog stages: stored and registered are known; later stages are unknown, not zero.
        assert_eq!(
            (
                manifest.metrics.stored.endpoints,
                manifest.metrics.stored.tools
            ),
            (32, 1598)
        );
        assert_eq!(
            manifest.metrics.registered.endpoints,
            manifest.endpoints.len()
        );
        assert_eq!(manifest.metrics.registered.tools, manifest.tools.len());
        let raw: Value =
            serde_json::from_slice(&fs::read(task.join("tests/exposure.json")).unwrap()).unwrap();
        for stage in ["provider_accepted", "model_visible", "loaded"] {
            assert!(raw["metrics"][stage].is_null(), "{label} {stage}");
        }
        assert!(!manifest.information_differences.is_empty(), "{label}");
        assert!(manifest.reference_plan_registered, "{label}");
        assert_eq!(
            manifest.diagnostic,
            variant == ExposureVariant::RelevantOnly
        );

        // Nothing evaluator-owned about the condition reaches an agent-visible file or
        // the directive: no label, seed, manifest or reference-plan marker.
        // (The directive type rejects unknown fields, so it can only name endpoints,
        // tools and hashes; it round-trips to exactly the bytes in the compose file.)
        assert!(!text.contains(label), "{label}");
        assert_eq!(serde_json::to_string(&directive).unwrap(), text, "{label}");
        for (name, bytes) in tree(&task) {
            let agent_visible = name == "instruction.md"
                || name == "environment/Dockerfile"
                || name.starts_with("environment/profile/");
            if agent_visible {
                let text = String::from_utf8_lossy(&bytes).to_ascii_lowercase();
                for marker in [
                    label,
                    "exposure",
                    "distractor",
                    "relevant",
                    "catalog_sha256",
                ] {
                    assert!(!text.contains(marker), "{label}: {name} mentions {marker}");
                }
            }
        }
    }
}

#[tokio::test]
async fn variants_select_and_order_authentic_definitions_as_declared() {
    let grafana = scenario(GRAFANA);
    let reference: Vec<&str> = grafana
        .solution
        .calls
        .iter()
        .map(|call| call.tool.as_str())
        .collect();

    let (_a, task) = generate(
        GRAFANA,
        WorkspaceProfile::Minimal,
        Some(&exposure(ExposureVariant::RelevantOnly, 0, None)),
    );
    let relevant = manifest(&task);
    assert_eq!(relevant.endpoints, ["grafana"]);
    assert!(
        relevant
            .tools
            .iter()
            .all(|tool| reference.contains(&tool.tool.as_str()))
    );
    assert!(
        reference
            .iter()
            .all(|name| relevant.tools.iter().any(|tool| tool.tool == *name))
    );
    assert!(
        relevant
            .limits
            .iter()
            .any(|limit| limit.contains("easier_diagnostic"))
    );
    // A hidden tool is unknown to the bridge, exactly like a tool the server never had.
    let (_, directive_relevant) = directive(&task).unwrap();
    let snapshot: Snapshot =
        serde_json::from_slice(&fs::read(root().join("servers/snapshots/grafana.json")).unwrap())
            .unwrap();
    let hidden = snapshot
        .tools
        .iter()
        .map(|tool| tool["name"].as_str().unwrap())
        .find(|name| !reference.contains(name))
        .unwrap()
        .to_owned();
    let backend = ExposedBackend::new(
        ReplicaBackend::new(snapshot),
        directive_relevant.endpoint("grafana").unwrap(),
    );
    let Reply::Json(refused) = backend
        .request(&(), json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":hidden,"arguments":{}}}))
        .await
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(refused["error"]["code"], -32602);

    let (_b, task) = generate(
        GRAFANA,
        WorkspaceProfile::Minimal,
        Some(&exposure(ExposureVariant::OrganizationPack, 0, None)),
    );
    let pack = manifest(&task);
    assert_eq!(pack.endpoints, ["grafana"]);
    assert_eq!(pack.tools.len(), 79);

    let seeded = |seed| {
        let (_out, task) = generate(
            GRAFANA,
            WorkspaceProfile::Minimal,
            Some(&exposure(ExposureVariant::RandomizedOrder, seed, None)),
        );
        manifest(&task)
    };
    let (first, again, other) = (seeded(1), seeded(1), seeded(2));
    assert_eq!(first, again);
    assert_ne!(first.endpoints, other.endpoints);
    assert_eq!(first.tools.len(), 1598);
    let mut sorted = first.endpoints.clone();
    sorted.sort();
    let mut expected = other.endpoints.clone();
    expected.sort();
    assert_eq!(sorted, expected);

    let (_c, task) = generate(
        GRAFANA,
        WorkspaceProfile::Minimal,
        Some(&exposure(ExposureVariant::ConfusingDistractors, 3, None)),
    );
    let confusing = manifest(&task);
    assert!(confusing.endpoints.contains(&"grafana".to_owned()) && confusing.endpoints.len() >= 3);
    // Distractors are whole authentic endpoints: every definition hash is a pinned one.
    let pinned: std::collections::BTreeSet<String> = confusing
        .endpoints
        .iter()
        .flat_map(|endpoint| {
            let snapshot: Snapshot = serde_json::from_slice(
                &fs::read(root().join(format!("servers/snapshots/{endpoint}.json"))).unwrap(),
            )
            .unwrap();
            snapshot
                .tools
                .iter()
                .map(definition_sha256)
                .collect::<Vec<_>>()
        })
        .collect();
    assert!(
        confusing
            .tools
            .iter()
            .all(|tool| pinned.contains(&tool.definition_sha256))
    );

    let (_d, task) = generate(
        GRAFANA,
        WorkspaceProfile::Minimal,
        Some(&exposure(
            ExposureVariant::ContextFeasible,
            0,
            Some(120_000),
        )),
    );
    let feasible = manifest(&task);
    assert!(feasible.endpoints.contains(&"grafana".to_owned()));
    assert!(feasible.metrics.registered.o200k_estimate <= 120_000);
    assert_eq!(feasible.token_budget, Some(120_000));
    assert!(feasible.budget_basis.unwrap().contains("o200k"));
}

#[test]
fn infeasible_or_underspecified_conditions_fail_closed() {
    let out = tempfile::tempdir().unwrap();
    let attempt = |id: &str, requested: Exposure| {
        write_task_with_treatments(
            &scenario(id),
            &catalog(),
            &images(),
            out.path(),
            Conditions::default(),
            Treatments {
                exposure: Some(&requested),
                awareness: None,
            },
        )
        .map_err(|error| error.to_string())
    };
    assert!(
        attempt(GRAFANA, exposure(ExposureVariant::ContextFeasible, 0, None))
            .unwrap_err()
            .contains("budget")
    );
    assert!(
        attempt(
            GRAFANA,
            exposure(ExposureVariant::ContextFeasible, 0, Some(10))
        )
        .unwrap_err()
        .contains("over the 10 budget")
    );
    assert!(
        attempt(
            "example/github/basic",
            exposure(ExposureVariant::RelevantOnly, 0, None)
        )
        .unwrap_err()
        .contains("reference plan")
    );
}

#[test]
fn exposure_never_changes_the_contract_the_verifier_grades() {
    // Grading reads world state through this contract. It is byte-identical under every
    // exposure, so a plan that used other tools is graded exactly as the reference plan.
    let (_full, full) = generate(GRAFANA, WorkspaceProfile::Minimal, None);
    for variant in [
        ExposureVariant::RelevantOnly,
        ExposureVariant::ConfusingDistractors,
    ] {
        let (_out, task) = generate(
            GRAFANA,
            WorkspaceProfile::Minimal,
            Some(&exposure(variant, 5, None)),
        );
        for name in [
            "tests/expect.json",
            "tests/task-source.json",
            "tests/test.sh",
            "instruction.md",
        ] {
            assert_eq!(
                fs::read(full.join(name)).unwrap(),
                fs::read(task.join(name)).unwrap(),
                "{name}"
            );
        }
        let contract = fs::read_to_string(task.join("tests/expect.json")).unwrap();
        assert!(!contract.contains("exposure") && !contract.contains("reference_plan"));
    }
}
