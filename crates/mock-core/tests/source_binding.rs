use std::path::Path;

use mock_core::{
    dataset::Dataset,
    evidence::WorldEvidence,
    grading::{Expect, Outcome},
    world::WorldState,
};
use serde_json::json;

#[test]
fn a_weaker_contract_cannot_turn_the_same_unmodified_world_into_success() {
    let directory = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../scenarios/business/salesforce/close-resolved-case");
    let mut world = WorldState::from_dataset(Dataset::load(&directory).unwrap()).unwrap();
    world.seal().unwrap();
    let captured = WorldEvidence::capture(&world, &[], Ok(vec!["192.0.2.10".into()]));
    let weakened: Expect = serde_json::from_value(json!({
        "state": [{
            "pointer": "/services/salesforce/Case/5005g00000spz8MAAQ/Status",
            "equals": "Working"
        }]
    }))
    .unwrap();
    let result = captured.assess(&weakened, None, false);
    assert!(result.state_ok);
    assert_eq!(result.reward, 0.0);
    assert_eq!(result.outcome, Outcome::EnvironmentError);
    assert!(
        result
            .failures
            .iter()
            .any(|failure| failure.location == "evidence/contract-source")
    );
}

#[test]
fn canonical_contract_fingerprints_ignore_object_order_but_not_types_or_array_order() {
    use mock_core::provenance::canonical_sha256;
    let first: serde_json::Value =
        serde_json::from_str(r#"{"a":1,"b":{"x":"text","y":[1,2]}}"#).unwrap();
    let reordered: serde_json::Value =
        serde_json::from_str(r#"{"b":{"y":[1,2],"x":"text"},"a":1}"#).unwrap();
    assert_eq!(canonical_sha256(&first), canonical_sha256(&reordered));
    for changed in [
        json!({"a": "1", "b": {"x": "text", "y": [1, 2]}}),
        json!({"a": 1, "b": {"x": "text", "y": [2, 1]}}),
    ] {
        assert_ne!(canonical_sha256(&first), canonical_sha256(&changed));
    }
}

#[test]
fn changed_initial_state_or_source_metadata_cannot_be_presented_as_valid_evidence() {
    let directory = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../scenarios/business/salesforce/close-resolved-case");
    let dataset = Dataset::load(&directory).unwrap();
    let expected = dataset.contract.clone();
    let mut world = WorldState::from_dataset(dataset).unwrap();
    world.seal().unwrap();
    let captured = WorldEvidence::capture(&world, &[], Ok(vec!["192.0.2.10".into()]));
    for kind in ["initial", "source"] {
        let mut changed = captured.clone();
        if kind == "initial" {
            changed.initial_state["services"]["salesforce"]["Case"]["5005g00000spz8MAAQ"]["Reason"] =
                json!("forged");
        } else {
            changed.source.dataset_sha256 = "invalid".into();
        }
        assert_eq!(
            changed.assess(&expected, None, false).outcome,
            Outcome::EnvironmentError
        );
    }
}

#[test]
fn source_fingerprints_are_root_independent_and_bind_the_exact_loaded_files() {
    fn copy_tree(source: &Path, destination: &Path) {
        std::fs::create_dir_all(destination).unwrap();
        for entry in std::fs::read_dir(source).unwrap() {
            let path = entry.unwrap().path();
            let target = destination.join(path.file_name().unwrap());
            if path.is_dir() {
                copy_tree(&path, &target);
            } else {
                std::fs::copy(&path, target).unwrap();
            }
        }
    }
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/example/github/basic");
    let target = tempfile::tempdir().unwrap();
    copy_tree(&source, target.path());
    let first = Dataset::load(&source).unwrap();
    let second = Dataset::load(target.path()).unwrap();
    assert_eq!(first.source, second.source);
    let scenario = target.path().join("scenario.toml");
    let mut contents = std::fs::read_to_string(&scenario).unwrap();
    contents.push_str("\n# source revision\n");
    std::fs::write(scenario, contents).unwrap();
    let changed = Dataset::load(target.path()).unwrap();
    assert_ne!(first.source.scenario_sha256, changed.source.scenario_sha256);
    assert_eq!(first.source.contract_sha256, changed.source.contract_sha256);
    let file = target.path().join("dataset/github/issues.json");
    let mut contents = std::fs::read_to_string(&file).unwrap();
    contents.push('\n');
    std::fs::write(file, contents).unwrap();
    assert_ne!(
        first.source.dataset_sha256,
        Dataset::load(target.path()).unwrap().source.dataset_sha256
    );
}

#[test]
fn invalid_contracts_keep_their_distinct_classification_when_source_binding_also_fails() {
    let directory = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../scenarios/business/salesforce/close-resolved-case");
    let dataset = Dataset::load(&directory).unwrap();
    let mut invalid = dataset.contract.clone();
    invalid.version = 99;
    let mut world = WorldState::from_dataset(dataset).unwrap();
    world.seal().unwrap();
    let evidence = WorldEvidence::capture(&world, &[], Ok(vec!["192.0.2.10".into()]));
    let result = evidence.assess(&invalid, None, false);
    assert_eq!(result.outcome, Outcome::InvalidContract);
    assert!(!result.contract_ok && !result.environment_ok);
}
