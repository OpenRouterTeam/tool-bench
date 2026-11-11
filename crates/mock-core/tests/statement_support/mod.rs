//! Shared harness for statement-mode answer contracts.
//!
//! Every check goes through `mock_core::grading::grade` with the scenario's
//! real `[expect.answer]` contract and the initial business state loaded from
//! the scenario's authored dataset, exactly as the world loads it.
#![allow(dead_code)]

use std::path::{Path, PathBuf};

use mock_core::{
    dataset::Dataset,
    grading::{Expect, Grade, GradeInput, Outcome, grade},
    world::WorldState,
};
use serde_json::{Value, json};

pub struct Case {
    pub expect: Expect,
    pub initial: Value,
    pub reference: String,
}

fn directory(id: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../scenarios")
        .join(id)
}

impl Case {
    pub fn load(id: &str) -> Self {
        let directory = directory(id);
        let dataset = Dataset::load(&directory).unwrap_or_else(|error| panic!("{id}: {error}"));
        let answer = serde_json::to_value(
            dataset
                .contract
                .answer
                .as_ref()
                .unwrap_or_else(|| panic!("{id} has no answer contract")),
        )
        .unwrap();
        assert_eq!(
            answer["record_facts"]["mode"], "statements",
            "{id} is not a statement-mode contract"
        );
        assert!(
            answer["contains"].as_array().is_none_or(Vec::is_empty) && answer["regex"].is_null(),
            "{id} still carries legacy keyword predicates"
        );
        let expect = serde_json::from_value(json!({"version": 2, "answer": answer})).unwrap();
        let document: toml::Value =
            toml::from_str(&std::fs::read_to_string(directory.join("scenario.toml")).unwrap())
                .unwrap();
        let reference = document["solution"]["answer"]
            .as_str()
            .unwrap_or_else(|| panic!("{id} has no reference answer"))
            .to_owned();
        let initial = WorldState::from_dataset(dataset)
            .unwrap()
            .initial_snapshot();
        Self {
            expect,
            initial,
            reference,
        }
    }

    pub fn grade_with(&self, initial: &Value, answer: &str) -> Grade {
        grade(&GradeInput {
            expect: &self.expect,
            state: initial.clone(),
            initial_state: Some(initial),
            journal: &[],
            events: Some(&[]),
            answer: Some(answer),
            tools_ips: vec![],
            context_overflow: false,
            startup: &[],
        })
    }

    pub fn outcome(&self, answer: &str) -> Outcome {
        let grade = self.grade_with(&self.initial, answer);
        assert!(grade.contract_ok, "{grade:?}");
        grade.outcome
    }

    #[track_caller]
    pub fn all(&self, expected: Outcome, answers: &[&str]) {
        for answer in answers {
            let grade = self.grade_with(&self.initial, answer);
            assert_eq!(
                grade.outcome, expected,
                "answer: {answer}\nfailures: {:?}",
                grade.failures
            );
        }
    }

    #[track_caller]
    pub fn passes(&self, answers: &[&str]) {
        self.all(Outcome::Pass, answers);
    }

    #[track_caller]
    pub fn fails(&self, answers: &[&str]) {
        self.all(Outcome::Fail, answers);
    }

    #[track_caller]
    pub fn unresolved(&self, answers: &[&str]) {
        self.all(Outcome::Unresolved, answers);
    }

    /// The reference answer passes, an empty answer fails, and removing the
    /// authored source collection is an environment error rather than a score.
    #[track_caller]
    pub fn reference_is_source_bound(&self) {
        let reference = self.reference.clone();
        self.passes(&[&reference]);
        self.fails(&["", "I could not find it."]);
        let collection = self
            .expect
            .answer
            .as_ref()
            .unwrap()
            .record_facts
            .as_ref()
            .unwrap()
            .selector
            .collection
            .clone();
        let mut missing = self.initial.clone();
        *missing
            .pointer_mut(&collection)
            .expect("the source collection exists") = json!({});
        assert_eq!(
            self.grade_with(&missing, &reference).outcome,
            Outcome::EnvironmentError,
            "a missing source must be an environment error"
        );
    }

    /// Grades `answer` after editing the authored source at `pointer`.
    pub fn outcome_after(&self, pointer: &str, value: Value, answer: &str) -> Outcome {
        let mut edited = self.initial.clone();
        *edited
            .pointer_mut(pointer)
            .unwrap_or_else(|| panic!("no authored value at {pointer}")) = value;
        self.grade_with(&edited, answer).outcome
    }
}
