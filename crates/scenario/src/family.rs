//! Task family and difficulty metadata (plan item 12).
//!
//! Each task belongs to exactly one difficulty family. Families describe the
//! *kind* of challenge, not the service or domain. A task may also carry a
//! numeric dependency-depth annotation that reflects information/operation
//! dependencies rather than raw MCP call count.

use serde::{Deserialize, Serialize};

pub use mock_core::family::{DifficultyFamily, FAMILY_SCHEMA, ScenarioFamily};

/// Groups whose scenarios are never part of the scored denominator.
pub const UNSCORED_GROUPS: [&str; 2] = ["example", "diagnostic"];

/// Whether a scenario identity belongs to the scored suite.
pub fn scored(id: &str) -> bool {
    !UNSCORED_GROUPS
        .iter()
        .any(|group| id.split('/').next() == Some(group))
}

/// Load-time check of a scenario's `[scenario.family]` table. Every scenario
/// outside `example/` needs a valid reviewed assignment; conversational and
/// no-action families are diagnostic tracks, never scored single-instruction tasks.
pub fn check(meta: &crate::Meta) -> anyhow::Result<()> {
    let Some(family) = &meta.family else {
        anyhow::ensure!(
            meta.id.split('/').next() == Some("example"),
            "{}: a reviewed [scenario.family] table is required",
            meta.id
        );
        return Ok(());
    };
    family
        .validate()
        .map_err(|reason| anyhow::anyhow!("{}: [scenario.family]: {reason}", meta.id))?;
    anyhow::ensure!(
        !(scored(&meta.id)
            && matches!(
                family.primary,
                DifficultyFamily::Clarification | DifficultyFamily::NoAction
            )),
        "{}: clarification and no-action families are unscored diagnostic tracks",
        meta.id
    );
    Ok(())
}

/// Versioned authoring admission requirements for a scored task.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthoringAdmission {
    pub schema_version: u32,
    pub task_id: String,
    pub family: ScenarioFamily,
    pub support_status: SupportStatus,
    pub evaluation_track: EvaluationTrack,
    /// At least one independently valid alternative solution description.
    pub valid_alternatives: Vec<AlternativeEvidence>,
    /// At least one negative-validation case (should fail for stated reason).
    pub negative_validations: Vec<NegativeEvidence>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SupportStatus {
    Scored,
    UnscoredDiagnostic,
    UnscoredExample,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvaluationTrack {
    McpOnly,
    DirectAllowed,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AlternativeEvidence {
    pub description: String,
    /// SHA-256 of the alternative trace or plan, if recorded.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evidence_sha256: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NegativeEvidence {
    pub description: String,
    pub expected_failure_reason: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evidence_sha256: Option<String>,
}

impl AuthoringAdmission {
    pub fn validate(&self) -> Result<(), String> {
        if self.schema_version != 1 {
            return Err(format!(
                "unsupported authoring admission schema {}",
                self.schema_version
            ));
        }
        self.family.validate()?;
        if self.support_status == SupportStatus::Scored {
            if self.valid_alternatives.is_empty() {
                return Err(
                    "scored tasks require at least one independently valid alternative".into(),
                );
            }
            if self.negative_validations.is_empty() {
                return Err("scored tasks require at least one negative-validation case".into());
            }
        }
        for alt in &self.valid_alternatives {
            if alt.description.trim().is_empty() {
                return Err("alternative evidence must have a non-empty description".into());
            }
        }
        for neg in &self.negative_validations {
            if neg.description.trim().is_empty() || neg.expected_failure_reason.trim().is_empty() {
                return Err(
                    "negative evidence must have non-empty description and failure reason".into(),
                );
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn family(primary: DifficultyFamily) -> ScenarioFamily {
        ScenarioFamily {
            schema_version: FAMILY_SCHEMA,
            primary,
            secondary: Vec::new(),
            dependency_depth: 1,
            rationale: "A reviewed fixture rationale for this admission test.".into(),
        }
    }

    fn admission(status: SupportStatus) -> AuthoringAdmission {
        AuthoringAdmission {
            schema_version: 1,
            task_id: "test/task".into(),
            family: family(DifficultyFamily::EntityAmbiguity),
            support_status: status,
            evaluation_track: EvaluationTrack::McpOnly,
            valid_alternatives: vec![AlternativeEvidence {
                description: "Use customer email lookup instead of name".into(),
                evidence_sha256: None,
            }],
            negative_validations: vec![NegativeEvidence {
                description: "Refund wrong customer's charge".into(),
                expected_failure_reason: "state check fails: wrong charge refunded".into(),
                evidence_sha256: None,
            }],
        }
    }

    #[test]
    fn only_example_and_diagnostic_groups_are_unscored() {
        assert!(scored("business/stripe/refund-duplicate-charge"));
        assert!(!scored("example/github/basic"));
        assert!(!scored("diagnostic/stripe/no-action-already-refunded"));
        assert!(scored("diagnostics/stripe/x"));
    }

    #[test]
    fn scored_admission_requires_alternative_and_negative_evidence() {
        admission(SupportStatus::Scored).validate().unwrap();
        let mut missing = admission(SupportStatus::Scored);
        missing.valid_alternatives.clear();
        assert!(missing.validate().is_err());
        let mut missing = admission(SupportStatus::Scored);
        missing.negative_validations.clear();
        assert!(missing.validate().is_err());
        let mut diagnostic = admission(SupportStatus::UnscoredDiagnostic);
        diagnostic.valid_alternatives.clear();
        diagnostic.negative_validations.clear();
        diagnostic.validate().unwrap();
    }

    #[test]
    fn admission_rejects_an_invalid_family() {
        let mut invalid = admission(SupportStatus::Scored);
        invalid.family.secondary = vec![DifficultyFamily::EntityAmbiguity];
        assert!(invalid.validate().is_err());
    }
}
