use std::path::Path;

use mock_core::{
    dataset::Dataset,
    evidence::{EVIDENCE_SCHEMA_VERSION, WorldEvidence},
    grading::{EVALUATOR_VERSION, Outcome},
    world::WorldState,
};

#[test]
fn current_evidence_is_versioned_and_prior_revisions_are_not_silently_rescored() {
    let directory =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/example/github/basic");
    let dataset = Dataset::load(&directory).unwrap();
    let contract = dataset.contract.clone();
    let mut world = WorldState::from_dataset(dataset).unwrap();
    world.seal().unwrap();
    let evidence = WorldEvidence::capture(&world, &[], Ok(vec!["192.0.2.10".into()]));
    assert_eq!(EVIDENCE_SCHEMA_VERSION, 7);
    assert_eq!(EVALUATOR_VERSION, 46);
    assert_eq!(evidence.schema_version, EVIDENCE_SCHEMA_VERSION);
    assert_eq!(evidence.evaluator_version, EVALUATOR_VERSION);
    assert!(evidence.assess(&contract, None, false).environment_ok);
    for (schema, evaluator) in [(6, EVALUATOR_VERSION), (7, 45), (6, 45)] {
        let mut historic = evidence.clone();
        historic.schema_version = schema;
        historic.evaluator_version = evaluator;
        let historic: WorldEvidence =
            serde_json::from_slice(&serde_json::to_vec(&historic).unwrap()).unwrap();
        let assessment = historic.assess(&contract, None, false);
        assert_eq!(assessment.outcome, Outcome::EnvironmentError);
        assert!(assessment.failures.iter().any(|failure| {
            failure.location == "evidence/version"
                && failure.reason.contains("revisions are incompatible")
        }));
    }
}
