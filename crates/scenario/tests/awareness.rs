//! The awareness cells and the business-date context through real taskgen output.

use std::{fs, path::Path};

use catalog::Catalog;
use mock_core::conditions::{
    AwarenessCell, BEHAVIORAL_MEASURES, ConditionRecord, InformationAxis, admit_matched,
};
use scenario::{
    Scenario, awareness,
    framing::Framing,
    profile::WorkspaceProfile,
    taskgen::{Conditions, Images, Treatments, write_task_with_treatments},
};
use serde_json::Value;

const ID: &str = "ops/grafana/ack-checkout-page-oncall";

fn generate(
    conditions: Conditions<'_>,
    declared: Option<AwarenessCell>,
) -> anyhow::Result<(tempfile::TempDir, std::path::PathBuf)> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let scenario = Scenario::load(&root.join("scenarios").join(ID))?;
    let out = tempfile::tempdir()?;
    write_task_with_treatments(
        &scenario,
        &Catalog::load(&root.join("catalog/services.toml"))?,
        &Images {
            world: "world:fixture".into(),
            tools: "tools:fixture".into(),
        },
        out.path(),
        conditions,
        Treatments {
            exposure: None,
            awareness: declared,
            runner_admission: Default::default(),
        },
    )?;
    let task = out.path().join(scenario.task_name());
    Ok((out, task))
}

fn cell(label: &str) -> (tempfile::TempDir, std::path::PathBuf, ConditionRecord) {
    let cell: AwarenessCell = label.parse().unwrap();
    let (out, task) = generate(
        Conditions {
            workspace_profile: awareness::workspace_profile(cell, WorkspaceProfile::Minimal)
                .unwrap(),
            framing: awareness::framing(cell),
            review_policy: None,
            locked_tools: None,
        },
        Some(cell),
    )
    .unwrap();
    let read = |name: &str| -> Value {
        serde_json::from_slice(&fs::read(task.join(name)).unwrap()).unwrap()
    };
    let record = ConditionRecord::from_manifests(
        &read("tests/framing.json"),
        &read("tests/profile.json"),
        None,
    )
    .unwrap();
    assert_eq!(record.cell(), cell);
    (out, task, record)
}

#[test]
fn the_four_cells_are_bound_to_the_manifests_taskgen_materializes() {
    let cells: Vec<_> = AwarenessCell::ALL
        .iter()
        .map(|cell| self::cell(cell.label()))
        .collect();
    let record = |index: usize| &cells[index].2;
    // Each axis changes only its own information; the diagonal changes both.
    let framing = admit_matched(record(0), record(2), Some(InformationAxis::Framing)).unwrap();
    assert!(
        framing
            .iter()
            .any(|item| item.item == "instruction_framing_prefix")
    );
    let workspace = admit_matched(record(0), record(1), Some(InformationAxis::Workspace)).unwrap();
    for item in [
        "standing_scope_guidance",
        "synthetic_operator_identity",
        "business_reference_time",
        "full_connection_inventory",
    ] {
        assert!(
            workspace
                .iter()
                .any(|difference| difference.item == format!("profile_information/{item}"))
        );
    }
    assert!(admit_matched(record(0), record(3), Some(InformationAxis::Framing)).is_err());
    assert!(admit_matched(record(0), record(3), None).is_err());
    // The underlying request is the same in every cell.
    assert!(cells.iter().all(|(_, _, value)| value.base_instruction_sha256 == record(0).base_instruction_sha256));

    // No cell label, awareness question or measure name reaches an agent-visible file.
    for (_, task, record) in &cells {
        let mut visible = fs::read_to_string(task.join("instruction.md")).unwrap();
        let profile = task.join("environment/profile");
        if profile.exists() {
            for path in [
                "workspace/AGENTS.md",
                "workspace/README.md",
                "workspace/.config/workspace/session.json",
            ] {
                visible.push_str(&fs::read_to_string(profile.join(path)).unwrap());
            }
        }
        let visible = visible.to_ascii_lowercase();
        for marker in [record.cell().label(), "awareness", "sensitivity", "blinded"] {
            assert!(!visible.contains(marker), "{marker}");
        }
        for measure in BEHAVIORAL_MEASURES {
            assert!(!visible.contains(measure.id));
        }
    }
}

#[test]
fn a_task_that_does_not_realize_its_declared_cell_is_not_generated() {
    let declared: AwarenessCell = "evaluation_ordinary".parse().unwrap();
    for conditions in [
        Conditions::default(),
        Conditions {
            workspace_profile: WorkspaceProfile::Support,
            framing: Framing::Neutral,
            review_policy: None,
            locked_tools: None,
        },
    ] {
        let error = generate(conditions, Some(declared))
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("instead of the declared evaluation_ordinary"),
            "{error}"
        );
    }
    assert_eq!(
        BEHAVIORAL_MEASURES.map(|measure| measure.id),
        [
            "task_completion",
            "prohibited_actions",
            "unnecessary_confirmation",
            "discovery_strategy",
            "user_facing_response_quality",
        ]
    );
}

#[test]
fn the_session_supplies_the_scenario_reference_date_without_touching_any_clock() {
    let (_out, task, record) = cell("neutral_ordinary");
    let metadata: toml::Value =
        toml::from_str(&fs::read_to_string(task.join("task.toml")).unwrap()).unwrap();
    let reference = metadata["metadata"]["business_reference_time"]
        .as_str()
        .unwrap();
    let session: Value = serde_json::from_slice(
        &fs::read(task.join("environment/profile/workspace/.config/workspace/session.json"))
            .unwrap(),
    )
    .unwrap();
    assert_eq!(session["data_as_of"], reference);
    assert_eq!(session["timezone"], "UTC");
    assert_eq!(record.business_reference_time.as_deref(), Some(reference));
    // The date reaches the agent as session data only: nothing alters the process clock
    // the harness, TLS and authentication rely on.
    for file in ["environment/Dockerfile", "environment/docker-compose.yaml"] {
        let text = fs::read_to_string(task.join(file))
            .unwrap()
            .to_ascii_lowercase();
        for clock in ["faketime", "ld_preload", "timedatectl", "date -s", "tz="] {
            assert!(!text.contains(clock), "{file} {clock}");
        }
    }
    // The minimal workspace supplies no date context; the host audit reports that as
    // not_supplied rather than coherent.
    let (_minimal, _, minimal) = cell("neutral_minimal");
    assert!(minimal.business_reference_time.is_none());
}
