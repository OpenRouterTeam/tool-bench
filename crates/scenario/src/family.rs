//! Task family and difficulty metadata (plan item 12).
//!
//! Each task belongs to exactly one difficulty family. Families describe the
//! *kind* of challenge, not the service or domain. A task may also carry a
//! numeric dependency-depth annotation that reflects information/operation
//! dependencies rather than raw MCP call count.

use serde::{Deserialize, Serialize};

pub const FAMILY_SCHEMA: u32 = 1;

/// The primary difficulty axis a task exercises.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DifficultyFamily {
    EntityAmbiguity,
    ToolAmbiguity,
    Pagination,
    StaleState,
    PermissionRefusal,
    Clarification,
    NoAction,
    RetryIdempotency,
    UntrustedRetrieval,
    DependencyDepth,
    Baseline,
}

impl DifficultyFamily {
    pub fn all() -> &'static [DifficultyFamily] {
        &[
            Self::EntityAmbiguity,
            Self::ToolAmbiguity,
            Self::Pagination,
            Self::StaleState,
            Self::PermissionRefusal,
            Self::Clarification,
            Self::NoAction,
            Self::RetryIdempotency,
            Self::UntrustedRetrieval,
            Self::DependencyDepth,
            Self::Baseline,
        ]
    }

    pub fn key(self) -> &'static str {
        match self {
            Self::EntityAmbiguity => "entity_ambiguity",
            Self::ToolAmbiguity => "tool_ambiguity",
            Self::Pagination => "pagination",
            Self::StaleState => "stale_state",
            Self::PermissionRefusal => "permission_refusal",
            Self::Clarification => "clarification",
            Self::NoAction => "no_action",
            Self::RetryIdempotency => "retry_idempotency",
            Self::UntrustedRetrieval => "untrusted_retrieval",
            Self::DependencyDepth => "dependency_depth",
            Self::Baseline => "baseline",
        }
    }
}

/// Metadata attached to a single task describing its difficulty characteristics.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskFamilyMeta {
    pub schema_version: u32,
    pub family: DifficultyFamily,
    /// Information/operation dependency depth (not reference call count).
    /// 0 = single-step, 1 = one dependency, etc.
    #[serde(default)]
    pub dependency_depth: u32,
    /// Whether a correct outcome may be "no action needed".
    #[serde(default)]
    pub no_action_valid: bool,
    /// Whether a correct outcome may be a clarification request.
    #[serde(default)]
    pub clarification_valid: bool,
    /// Secondary families also exercised (for diagnostics, not scoring splits).
    #[serde(default)]
    pub secondary_families: Vec<DifficultyFamily>,
}

impl TaskFamilyMeta {
    pub fn validate(&self) -> Result<(), String> {
        if self.schema_version != FAMILY_SCHEMA {
            return Err(format!(
                "unsupported family schema version {}",
                self.schema_version
            ));
        }
        if self.secondary_families.contains(&self.family) {
            return Err("primary family must not appear in secondary_families".into());
        }
        let mut seen = std::collections::HashSet::new();
        for f in &self.secondary_families {
            if !seen.insert(f) {
                return Err(format!("duplicate secondary family {:?}", f));
            }
        }
        Ok(())
    }
}

/// Versioned authoring admission requirements for a scored task.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthoringAdmission {
    pub schema_version: u32,
    pub task_id: String,
    pub family: TaskFamilyMeta,
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

    #[test]
    fn all_families_have_distinct_keys() {
        let keys: Vec<_> = DifficultyFamily::all().iter().map(|f| f.key()).collect();
        let unique: std::collections::HashSet<_> = keys.iter().collect();
        assert_eq!(keys.len(), unique.len());
    }

    #[test]
    fn valid_admission_passes() {
        let admission = AuthoringAdmission {
            schema_version: 1,
            task_id: "test/task".into(),
            family: TaskFamilyMeta {
                schema_version: 1,
                family: DifficultyFamily::EntityAmbiguity,
                dependency_depth: 1,
                no_action_valid: false,
                clarification_valid: false,
                secondary_families: vec![],
            },
            support_status: SupportStatus::Scored,
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
        };
        assert!(admission.validate().is_ok());
    }

    #[test]
    fn scored_without_alternatives_fails() {
        let admission = AuthoringAdmission {
            schema_version: 1,
            task_id: "test/task".into(),
            family: TaskFamilyMeta {
                schema_version: 1,
                family: DifficultyFamily::Baseline,
                dependency_depth: 0,
                no_action_valid: false,
                clarification_valid: false,
                secondary_families: vec![],
            },
            support_status: SupportStatus::Scored,
            evaluation_track: EvaluationTrack::McpOnly,
            valid_alternatives: vec![],
            negative_validations: vec![NegativeEvidence {
                description: "Wrong action".into(),
                expected_failure_reason: "fails".into(),
                evidence_sha256: None,
            }],
        };
        assert!(admission.validate().is_err());
    }

    #[test]
    fn unscored_diagnostic_skips_evidence_requirements() {
        let admission = AuthoringAdmission {
            schema_version: 1,
            task_id: "diagnostic/task".into(),
            family: TaskFamilyMeta {
                schema_version: 1,
                family: DifficultyFamily::Clarification,
                dependency_depth: 0,
                no_action_valid: false,
                clarification_valid: true,
                secondary_families: vec![],
            },
            support_status: SupportStatus::UnscoredDiagnostic,
            evaluation_track: EvaluationTrack::McpOnly,
            valid_alternatives: vec![],
            negative_validations: vec![],
        };
        assert!(admission.validate().is_ok());
    }

    #[test]
    fn duplicate_secondary_family_rejected() {
        let meta = TaskFamilyMeta {
            schema_version: 1,
            family: DifficultyFamily::Baseline,
            dependency_depth: 0,
            no_action_valid: false,
            clarification_valid: false,
            secondary_families: vec![DifficultyFamily::Pagination, DifficultyFamily::Pagination],
        };
        assert!(meta.validate().is_err());
    }

    #[test]
    fn primary_in_secondary_rejected() {
        let meta = TaskFamilyMeta {
            schema_version: 1,
            family: DifficultyFamily::Baseline,
            dependency_depth: 0,
            no_action_valid: false,
            clarification_valid: false,
            secondary_families: vec![DifficultyFamily::Baseline],
        };
        assert!(meta.validate().is_err());
    }
}
