//! Reviewed difficulty-family metadata (improvement-plan item 12).
//!
//! A family names the *kind* of difficulty a task's data and contract
//! exercise. It is authored in `[scenario.family]`, never derived from a
//! service name, group or tag. Dependency depth counts information/operation
//! dependencies, not reference tool calls.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

pub const FAMILY_SCHEMA: u32 = 1;
/// Longest dependency chain an authored scenario may claim.
pub const MAX_DEPENDENCY_DEPTH: u32 = 6;
const MAX_RATIONALE_CHARS: usize = 240;

/// The primary difficulty axis a task exercises.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DifficultyFamily {
    EntityAmbiguity,
    ToolAmbiguity,
    Pagination,
    StaleState,
    Reconciliation,
    CrossSystemIdentity,
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
            Self::Reconciliation,
            Self::CrossSystemIdentity,
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
            Self::Reconciliation => "reconciliation",
            Self::CrossSystemIdentity => "cross_system_identity",
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

/// The authored `[scenario.family]` table.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScenarioFamily {
    pub schema_version: u32,
    pub primary: DifficultyFamily,
    /// Other axes the data also exercises (diagnostic, never a scoring split).
    #[serde(default)]
    pub secondary: Vec<DifficultyFamily>,
    /// Longest chain of information/operation dependencies (0 = every
    /// identifier is supplied by the request). Not a reference call count.
    pub dependency_depth: u32,
    /// The concrete data or contract fact that justifies the primary family.
    pub rationale: String,
}

impl ScenarioFamily {
    pub fn validate(&self) -> Result<(), String> {
        if self.schema_version != FAMILY_SCHEMA {
            return Err(format!(
                "unsupported family schema version {}",
                self.schema_version
            ));
        }
        if self.dependency_depth > MAX_DEPENDENCY_DEPTH {
            return Err(format!(
                "dependency_depth must be at most {MAX_DEPENDENCY_DEPTH}"
            ));
        }
        let mut seen = BTreeSet::new();
        for family in &self.secondary {
            if *family == self.primary {
                return Err("the primary family must not repeat as a secondary axis".into());
            }
            if *family == DifficultyFamily::Baseline {
                return Err("baseline is not a secondary difficulty axis".into());
            }
            if !seen.insert(family) {
                return Err(format!("duplicate secondary family {}", family.key()));
            }
        }
        if self.primary == DifficultyFamily::Baseline && !self.secondary.is_empty() {
            return Err("a baseline task cannot also claim secondary difficulty axes".into());
        }
        if self.primary == DifficultyFamily::DependencyDepth && self.dependency_depth < 3 {
            return Err(
                "a dependency_depth primary family needs at least three dependent steps".into(),
            );
        }
        let rationale = self.rationale.trim();
        if rationale.len() < 20
            || self.rationale.chars().count() > MAX_RATIONALE_CHARS
            || self.rationale.chars().any(char::is_control)
        {
            return Err(format!(
                "family rationale must be one reviewed sentence of 20 to {MAX_RATIONALE_CHARS} characters"
            ));
        }
        Ok(())
    }

    /// Canonical identity of this assignment, frozen into release manifests.
    pub fn sha256(&self) -> String {
        crate::provenance::canonical_sha256(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn family() -> ScenarioFamily {
        ScenarioFamily {
            schema_version: FAMILY_SCHEMA,
            primary: DifficultyFamily::EntityAmbiguity,
            secondary: vec![DifficultyFamily::StaleState],
            dependency_depth: 2,
            rationale: "Two same-amount charges two minutes apart share one order.".into(),
        }
    }

    #[test]
    fn family_keys_are_distinct_and_match_their_serialized_names() {
        let keys: BTreeSet<_> = DifficultyFamily::all().iter().map(|f| f.key()).collect();
        assert_eq!(keys.len(), DifficultyFamily::all().len());
        for family in DifficultyFamily::all() {
            assert_eq!(
                serde_json::to_value(family).unwrap(),
                serde_json::json!(family.key())
            );
        }
    }

    #[test]
    fn rejects_inconsistent_assignments() {
        family().validate().unwrap();
        for change in [
            (|f: &mut ScenarioFamily| f.schema_version = 2) as fn(&mut ScenarioFamily),
            |f| f.secondary = vec![DifficultyFamily::EntityAmbiguity],
            |f| f.secondary = vec![DifficultyFamily::Pagination, DifficultyFamily::Pagination],
            |f| f.secondary = vec![DifficultyFamily::Baseline],
            |f| f.dependency_depth = MAX_DEPENDENCY_DEPTH + 1,
            |f| f.rationale = "too short".into(),
            |f| f.rationale = "line one of the rationale\nline two of it".into(),
            |f| {
                f.primary = DifficultyFamily::DependencyDepth;
                f.dependency_depth = 2
            },
            |f| f.primary = DifficultyFamily::Baseline,
        ] {
            let mut changed = family();
            change(&mut changed);
            assert!(changed.validate().is_err(), "{changed:?}");
        }
    }

    #[test]
    fn unknown_families_and_fields_do_not_parse() {
        assert!(
            serde_json::from_value::<ScenarioFamily>(serde_json::json!({
                "schema_version": 1, "primary": "stripe", "dependency_depth": 1,
                "rationale": "A service name is not a difficulty family."
            }))
            .is_err()
        );
        assert!(
            serde_json::from_value::<ScenarioFamily>(serde_json::json!({
                "schema_version": 1, "primary": "baseline", "dependency_depth": 1,
                "rationale": "An unknown field must not be silently ignored.", "depth": 2
            }))
            .is_err()
        );
    }

    #[test]
    fn the_assignment_hash_changes_with_any_reviewed_field() {
        let mut changed = family();
        changed.dependency_depth = 3;
        assert_ne!(family().sha256(), changed.sha256());
    }
}
