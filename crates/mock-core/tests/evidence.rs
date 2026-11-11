use std::path::Path;

use mock_core::{
    dataset::Dataset,
    evidence::WorldEvidence,
    grading::{Expect, Outcome},
    world::WorldState,
};
use serde_json::json;

fn evidence() -> WorldEvidence {
    let directory =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/example/github/basic");
    let mut dataset = Dataset::load(&directory).unwrap();
    dataset.contract = contract();
    let mut world = WorldState::from_dataset(dataset).unwrap();
    world.seal().unwrap();
    WorldEvidence::capture(&world, &[], Ok(vec!["192.0.2.10".into()]))
}

fn contract() -> Expect {
    serde_json::from_value(json!({
        "version": 2,
        "audit_history": true,
        "answer": {"contains": ["Maya"]}
    }))
    .unwrap()
}

#[test]
fn exported_evidence_roundtrips_and_can_be_graded_without_a_running_world() {
    let captured = evidence();
    let encoded = serde_json::to_vec(&captured).unwrap();
    let decoded: WorldEvidence = serde_json::from_slice(&encoded).unwrap();
    let grade = decoded.assess(&contract(), Some("Maya"), false);
    assert_eq!(grade.reward, 1.0, "{grade:?}");
    assert!(grade.history_checked);
}

#[test]
fn incompatible_versions_and_cross_scenario_snapshots_fail_closed() {
    for changed in ["schema", "evaluator", "initial", "final"] {
        let mut evidence = evidence();
        match changed {
            "schema" => evidence.schema_version += 1,
            "evaluator" => evidence.evaluator_version += 1,
            "initial" => evidence.initial_state["scenario"] = json!("other/scenario/id"),
            "final" => evidence.state["scenario"] = json!("other/scenario/id"),
            _ => unreachable!(),
        }
        let grade = evidence.assess(&contract(), Some("Maya"), false);
        assert_eq!(grade.outcome, Outcome::EnvironmentError, "{changed}");
        assert_eq!(grade.reward, 0.0);
    }
}

#[test]
fn unresolved_or_empty_origin_evidence_is_not_a_successful_mcp_trial() {
    for error in [None, Some("lookup failed".to_owned())] {
        let mut evidence = evidence();
        evidence.tools_ips.clear();
        evidence.origin_error = error;
        let grade = evidence.assess(&contract(), Some("Maya"), false);
        assert_eq!(grade.outcome, Outcome::EnvironmentError);
        assert_eq!(grade.reward, 0.0);
    }
}

#[test]
fn captured_debug_answers_are_not_implicitly_used_as_the_user_reply() {
    let mut evidence = evidence();
    evidence.answer = Some("Maya".into());
    let grade = evidence.assess(&contract(), None, false);
    assert_eq!(grade.reward, 0.0);
    assert!(!grade.answer_ok);
}
