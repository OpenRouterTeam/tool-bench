//! Frozen release and comparison design (plan items 15/19).
//!
//! A release freezes which task families are public-development vs genuinely
//! held-out, plus the comparison conditions (repeats, budgets, order,
//! replacement policy). The report rejects unregistered replacements, mutable
//! family assignments, and incompatible conditions.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::family::DifficultyFamily;

pub const RELEASE_SCHEMA: u32 = 1;

/// Whether a task family is used for public development or held out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SplitAssignment {
    PublicDevelopment,
    HeldOut,
}

/// A frozen task entry within a release.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReleaseTask {
    pub task_id: String,
    pub family: DifficultyFamily,
    pub split: SplitAssignment,
    /// SHA-256 of the task's materialized contract at freeze time.
    pub contract_sha256: String,
    /// SHA-256 of the task's materialization manifest (if base+overlay).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub materialization_sha256: Option<String>,
}

/// Comparison conditions frozen at release time.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ComparisonDesign {
    pub schema_version: u32,
    /// Number of repeat attempts per task per arm.
    pub repeats_per_task: u32,
    /// Per-task inference budget in tokens (0 = unlimited).
    pub token_budget: u64,
    /// Task presentation order policy.
    pub order_policy: OrderPolicy,
    /// What happens when a trial fails for infrastructure reasons.
    pub replacement_policy: ReplacementPolicy,
    /// Declared treatments that are allowed to vary between arms.
    pub declared_treatments: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OrderPolicy {
    Fixed,
    DeterministicShuffle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReplacementPolicy {
    NoReplacement,
    ReplaceEnvironmentErrors,
}

/// A frozen release: the complete scored task inventory plus comparison rules.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Release {
    pub schema_version: u32,
    pub release_id: String,
    pub frozen_at: String,
    pub tasks: Vec<ReleaseTask>,
    pub comparison: ComparisonDesign,
    /// SHA-256 of external/private bundle seals admitted to this release.
    #[serde(default)]
    pub external_bundle_seals: Vec<ExternalBundleSeal>,
}

/// An external/private bundle admitted with integrity checks.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExternalBundleSeal {
    pub bundle_id: String,
    pub content_sha256: String,
    pub admitted_by: String,
    /// Clearly synthetic fixture seal for tests, not a real human approval.
    #[serde(default)]
    pub synthetic_fixture: bool,
}

impl Release {
    pub fn sha256(&self) -> String {
        let bytes = serde_json::to_vec(self).expect("release serializes");
        let mut hasher = Sha256::new();
        hasher.update(b"tool-bench-release-v1\0");
        hasher.update(&bytes);
        format!(
            "sha256:{}",
            hasher
                .finalize()
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>()
        )
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.schema_version != RELEASE_SCHEMA {
            return Err(format!(
                "unsupported release schema {}",
                self.schema_version
            ));
        }
        if self.tasks.is_empty() {
            return Err("release must contain at least one task".into());
        }
        let mut ids = BTreeSet::new();
        for task in &self.tasks {
            if !ids.insert(&task.task_id) {
                return Err(format!("duplicate task_id {:?}", task.task_id));
            }
            if !mock_core::provenance::digest_valid(&task.contract_sha256) {
                return Err(format!("invalid contract digest for {}", task.task_id));
            }
            if let Some(ref h) = task.materialization_sha256 {
                if !mock_core::provenance::digest_valid(h) {
                    return Err(format!(
                        "invalid materialization digest for {}",
                        task.task_id
                    ));
                }
            }
        }
        if self.comparison.repeats_per_task == 0 {
            return Err("repeats_per_task must be at least 1".into());
        }
        for seal in &self.external_bundle_seals {
            if !mock_core::provenance::digest_valid(&seal.content_sha256) {
                return Err(format!(
                    "invalid external bundle digest for {}",
                    seal.bundle_id
                ));
            }
        }
        Ok(())
    }

    /// Family-level summary: task counts by family and split.
    pub fn family_summary(&self) -> BTreeMap<String, FamilySummary> {
        let mut result = BTreeMap::new();
        for task in &self.tasks {
            let entry = result
                .entry(task.family.key().to_owned())
                .or_insert_with(|| FamilySummary {
                    family: task.family,
                    public_count: 0,
                    held_out_count: 0,
                });
            match task.split {
                SplitAssignment::PublicDevelopment => entry.public_count += 1,
                SplitAssignment::HeldOut => entry.held_out_count += 1,
            }
        }
        result
    }

    pub fn held_out_task_ids(&self) -> Vec<&str> {
        self.tasks
            .iter()
            .filter(|t| t.split == SplitAssignment::HeldOut)
            .map(|t| t.task_id.as_str())
            .collect()
    }

    pub fn public_task_ids(&self) -> Vec<&str> {
        self.tasks
            .iter()
            .filter(|t| t.split == SplitAssignment::PublicDevelopment)
            .map(|t| t.task_id.as_str())
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FamilySummary {
    pub family: DifficultyFamily,
    pub public_count: usize,
    pub held_out_count: usize,
}

/// Validates that a set of trials is compatible with a frozen release.
pub fn validate_trials_against_release(
    release: &Release,
    trial_tasks: &[TrialTaskRecord],
) -> Result<(), String> {
    let release_map: BTreeMap<&str, &ReleaseTask> = release
        .tasks
        .iter()
        .map(|t| (t.task_id.as_str(), t))
        .collect();

    for trial in trial_tasks {
        let release_task = release_map.get(trial.task_id.as_str()).ok_or_else(|| {
            format!(
                "unregistered task {:?} not in release {}",
                trial.task_id, release.release_id
            )
        })?;
        if trial.contract_sha256 != release_task.contract_sha256 {
            return Err(format!(
                "contract mismatch for {}: release expects {}, trial has {}",
                trial.task_id, release_task.contract_sha256, trial.contract_sha256
            ));
        }
        if trial.family != release_task.family {
            return Err(format!(
                "mutable family assignment for {}: release has {:?}, trial has {:?}",
                trial.task_id, release_task.family, trial.family
            ));
        }
    }
    Ok(())
}

/// Minimal trial record for release validation.
#[derive(Debug, Clone)]
pub struct TrialTaskRecord {
    pub task_id: String,
    pub contract_sha256: String,
    pub family: DifficultyFamily,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dummy_sha() -> String {
        format!("sha256:{}", "a".repeat(64))
    }

    fn test_release() -> Release {
        Release {
            schema_version: 1,
            release_id: "test-release-001".into(),
            frozen_at: "2026-11-01T00:00:00Z".into(),
            tasks: vec![
                ReleaseTask {
                    task_id: "task-alpha".into(),
                    family: DifficultyFamily::EntityAmbiguity,
                    split: SplitAssignment::PublicDevelopment,
                    contract_sha256: dummy_sha(),
                    materialization_sha256: None,
                },
                ReleaseTask {
                    task_id: "task-beta".into(),
                    family: DifficultyFamily::NoAction,
                    split: SplitAssignment::HeldOut,
                    contract_sha256: dummy_sha(),
                    materialization_sha256: None,
                },
            ],
            comparison: ComparisonDesign {
                schema_version: 1,
                repeats_per_task: 3,
                token_budget: 0,
                order_policy: OrderPolicy::Fixed,
                replacement_policy: ReplacementPolicy::ReplaceEnvironmentErrors,
                declared_treatments: vec!["agent".into()],
            },
            external_bundle_seals: vec![ExternalBundleSeal {
                bundle_id: "synthetic-fixture-001".into(),
                content_sha256: dummy_sha(),
                admitted_by: "test-fixture".into(),
                synthetic_fixture: true,
            }],
        }
    }

    #[test]
    fn valid_release_passes() {
        assert!(test_release().validate().is_ok());
    }

    #[test]
    fn release_sha256_is_deterministic() {
        let r = test_release();
        assert_eq!(r.sha256(), r.sha256());
    }

    #[test]
    fn family_summary_counts_correctly() {
        let r = test_release();
        let summary = r.family_summary();
        assert_eq!(summary["entity_ambiguity"].public_count, 1);
        assert_eq!(summary["no_action"].held_out_count, 1);
    }

    #[test]
    fn held_out_vs_public_split() {
        let r = test_release();
        assert_eq!(r.held_out_task_ids(), vec!["task-beta"]);
        assert_eq!(r.public_task_ids(), vec!["task-alpha"]);
    }

    #[test]
    fn rejects_unregistered_trial_task() {
        let r = test_release();
        let trials = vec![TrialTaskRecord {
            task_id: "unknown-task".into(),
            contract_sha256: dummy_sha(),
            family: DifficultyFamily::Baseline,
        }];
        assert!(validate_trials_against_release(&r, &trials).is_err());
    }

    #[test]
    fn rejects_contract_mismatch() {
        let r = test_release();
        let trials = vec![TrialTaskRecord {
            task_id: "task-alpha".into(),
            contract_sha256: format!("sha256:{}", "b".repeat(64)),
            family: DifficultyFamily::EntityAmbiguity,
        }];
        assert!(validate_trials_against_release(&r, &trials).is_err());
    }

    #[test]
    fn rejects_mutable_family_assignment() {
        let r = test_release();
        let trials = vec![TrialTaskRecord {
            task_id: "task-alpha".into(),
            contract_sha256: dummy_sha(),
            family: DifficultyFamily::Pagination,
        }];
        assert!(validate_trials_against_release(&r, &trials).is_err());
    }

    #[test]
    fn accepts_valid_trial() {
        let r = test_release();
        let trials = vec![TrialTaskRecord {
            task_id: "task-alpha".into(),
            contract_sha256: dummy_sha(),
            family: DifficultyFamily::EntityAmbiguity,
        }];
        assert!(validate_trials_against_release(&r, &trials).is_ok());
    }

    #[test]
    fn rejects_empty_release() {
        let mut r = test_release();
        r.tasks.clear();
        assert!(r.validate().is_err());
    }

    #[test]
    fn rejects_duplicate_task_ids() {
        let mut r = test_release();
        r.tasks.push(r.tasks[0].clone());
        assert!(r.validate().is_err());
    }

    #[test]
    fn external_seal_synthetic_fixture_flag() {
        let r = test_release();
        assert!(r.external_bundle_seals[0].synthetic_fixture);
    }
}
