//! Frozen release manifests and prespecified comparison designs
//! (improvement-plan items 15 and 19).
//!
//! A release freezes, before any result is observed, which tasks belong to
//! which split, each task's family assignment and source/contract identities,
//! the evaluator revision, and the comparison design: repeats, budgets, order,
//! replacement policy and the denominators every reported rate must state.
//! The report's comparison path admits trials against this manifest.
//!
//! A held-out split is an *external sealed input*. A seal flagged
//! `synthetic_fixture` proves only that the admission path works; it is not a
//! held-out task set and supports no generalization claim.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::{
    family::DifficultyFamily,
    provenance::{canonical_sha256, digest_valid},
};

pub const RELEASE_SCHEMA: u32 = 2;
pub const COMPARISON_SCHEMA: u32 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SplitAssignment {
    PublicDevelopment,
    HeldOut,
}

impl SplitAssignment {
    pub fn key(self) -> &'static str {
        match self {
            Self::PublicDevelopment => "public_development",
            Self::HeldOut => "held_out",
        }
    }
}

/// One frozen task. Every identity is the value the trial's own verified
/// provenance must reproduce.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReleaseTask {
    /// Scenario identity, `<group>/<service|multi>/<slug>`.
    pub task_id: String,
    pub split: SplitAssignment,
    pub family: DifficultyFamily,
    /// Canonical hash of the complete reviewed `[scenario.family]` table.
    pub family_sha256: String,
    /// Exact `scenario.toml` bytes (covers the family table and any script).
    pub scenario_sha256: String,
    pub dataset_sha256: String,
    pub contract_sha256: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OrderPolicy {
    /// Tasks run in manifest order within each arm.
    Fixed,
    /// A seeded shuffle recorded with the run.
    DeterministicShuffle,
    /// Arms alternate task by task so drift affects both alike.
    InterleavedArms,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ReplacementPolicy {
    /// Every attempt counts; an unscorable attempt blocks the comparison.
    NoReplacement,
    /// An attempt the evaluator classified as an environment error or that
    /// produced no result may be replaced, up to a per-task limit. Model
    /// failures are never replaceable.
    ReplaceEnvironmentErrors { max_per_task: u32 },
}

/// Budgets fixed for every arm. `None` means "not limited by this release",
/// which the report states instead of implying a matched budget.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Budgets {
    pub agent_timeout_sec_max: Option<u64>,
    pub inference_tokens_max: Option<u64>,
}

/// The unit a reported rate is taken over.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Denominator {
    Attempts,
    Tasks,
    Families,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ComparisonDesign {
    pub schema_version: u32,
    /// Counted attempts per task per arm; fixed before results are seen.
    pub repeats_per_task: u32,
    pub budgets: Budgets,
    pub order_policy: OrderPolicy,
    pub replacement_policy: ReplacementPolicy,
    /// What may differ between arms. Anything else must match.
    pub declared_treatments: Vec<String>,
    /// Every denominator the report must state alongside its rates.
    pub denominators: Vec<Denominator>,
    /// No interim look may stop or extend the run.
    pub sequential_stopping: bool,
}

/// An external/private bundle admitted by hash only.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExternalBundleSeal {
    pub bundle_id: String,
    pub split: SplitAssignment,
    pub content_sha256: String,
    pub admitted_by: String,
    /// A synthetic control, not a reviewed held-out set.
    #[serde(default)]
    pub synthetic_fixture: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Release {
    pub schema_version: u32,
    pub release_id: String,
    pub frozen_at: String,
    pub evaluator_version: u32,
    pub family_schema_version: u32,
    pub comparison: ComparisonDesign,
    #[serde(default)]
    pub external_bundle_seals: Vec<ExternalBundleSeal>,
    pub tasks: Vec<ReleaseTask>,
}

/// What a verified trial states about the task it ran.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrialIdentity<'a> {
    pub task_id: &'a str,
    pub evaluator_version: u32,
    pub scenario_sha256: &'a str,
    pub dataset_sha256: &'a str,
    pub contract_sha256: &'a str,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FamilyCount {
    pub family: DifficultyFamily,
    pub public_development: usize,
    pub held_out: usize,
}

impl Release {
    pub fn sha256(&self) -> String {
        canonical_sha256(&serde_json::json!({"tool_bench_release": self}))
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.schema_version != RELEASE_SCHEMA
            || self.comparison.schema_version != COMPARISON_SCHEMA
        {
            return Err("unsupported release or comparison-design schema".into());
        }
        if self.family_schema_version != crate::family::FAMILY_SCHEMA {
            return Err("release was frozen for another family schema".into());
        }
        let name = |text: &str| {
            !text.is_empty()
                && text
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
        };
        if !name(&self.release_id) || crate::clock::parse_rfc3339_nanos(&self.frozen_at).is_none() {
            return Err("release identity or freeze time is invalid".into());
        }
        if self.tasks.is_empty() {
            return Err("a release must contain at least one task".into());
        }
        let mut previous: Option<&str> = None;
        for task in &self.tasks {
            if task.task_id.split('/').count() != 3
                || task.task_id.split('/').any(|part| !name(part))
                || previous.is_some_and(|earlier| earlier >= task.task_id.as_str())
            {
                return Err(format!(
                    "release tasks must be distinct sorted scenario identities ({})",
                    task.task_id
                ));
            }
            previous = Some(&task.task_id);
            if [
                &task.family_sha256,
                &task.scenario_sha256,
                &task.dataset_sha256,
                &task.contract_sha256,
            ]
            .iter()
            .any(|digest| !digest_valid(digest))
            {
                return Err(format!("invalid frozen digest for {}", task.task_id));
            }
        }
        let design = &self.comparison;
        if design.repeats_per_task == 0 || design.repeats_per_task > 100 {
            return Err("repeats_per_task must be between 1 and 100".into());
        }
        if design.sequential_stopping {
            return Err("sequential stopping needs a prespecified rule; none is supported".into());
        }
        if let ReplacementPolicy::ReplaceEnvironmentErrors { max_per_task } =
            design.replacement_policy
            && max_per_task == 0
        {
            return Err("a replacement policy must allow at least one replacement".into());
        }
        if design.budgets.agent_timeout_sec_max == Some(0)
            || design.budgets.inference_tokens_max == Some(0)
        {
            return Err("a declared budget must be positive".into());
        }
        let treatments: BTreeSet<_> = design.declared_treatments.iter().collect();
        if treatments.len() != design.declared_treatments.len()
            || design.declared_treatments.iter().any(|t| {
                !matches!(
                    t.as_str(),
                    "agent"
                        | "information:framing"
                        | "information:workspace"
                        | "information:exposure"
                )
            })
            || design
                .declared_treatments
                .iter()
                .filter(|t| t.starts_with("information:"))
                .count()
                > 1
        {
            return Err("release treatments must be distinct: agent and at most one supported information axis".into());
        }
        let denominators: BTreeSet<_> = design.denominators.iter().collect();
        if denominators.len() != design.denominators.len()
            || !denominators.contains(&Denominator::Attempts)
            || !denominators.contains(&Denominator::Tasks)
        {
            return Err("a design must declare distinct attempt and task denominators".into());
        }
        let mut bundles = BTreeSet::new();
        for seal in &self.external_bundle_seals {
            if !name(&seal.bundle_id)
                || seal.admitted_by.trim().is_empty()
                || !digest_valid(&seal.content_sha256)
                || !bundles.insert(&seal.bundle_id)
            {
                return Err(format!("invalid external bundle seal {}", seal.bundle_id));
            }
        }
        if self
            .tasks
            .iter()
            .any(|task| task.split == SplitAssignment::HeldOut)
            && !self
                .external_bundle_seals
                .iter()
                .any(|seal| seal.split == SplitAssignment::HeldOut && !seal.synthetic_fixture)
        {
            return Err(
                "held-out tasks require a non-synthetic sealed bundle; a synthetic fixture is not a held-out set"
                    .into(),
            );
        }
        Ok(())
    }

    pub fn task(&self, task_id: &str) -> Option<&ReleaseTask> {
        self.tasks
            .binary_search_by(|task| task.task_id.as_str().cmp(task_id))
            .ok()
            .map(|index| &self.tasks[index])
    }

    /// Whether a real (non-synthetic) held-out bundle is sealed into this release.
    pub fn has_real_held_out(&self) -> bool {
        self.external_bundle_seals
            .iter()
            .any(|seal| seal.split == SplitAssignment::HeldOut && !seal.synthetic_fixture)
    }

    pub fn family_counts(&self) -> Vec<FamilyCount> {
        let mut counts: BTreeMap<DifficultyFamily, (usize, usize)> = BTreeMap::new();
        for task in &self.tasks {
            let entry = counts.entry(task.family).or_default();
            match task.split {
                SplitAssignment::PublicDevelopment => entry.0 += 1,
                SplitAssignment::HeldOut => entry.1 += 1,
            }
        }
        counts
            .into_iter()
            .map(|(family, (public_development, held_out))| FamilyCount {
                family,
                public_development,
                held_out,
            })
            .collect()
    }

    /// Admits one verified trial identity, or says exactly why it is outside
    /// the frozen release.
    pub fn admit(&self, trial: &TrialIdentity<'_>) -> Result<&ReleaseTask, String> {
        let task = self.task(trial.task_id).ok_or_else(|| {
            format!(
                "{} is not a task of release {}",
                trial.task_id, self.release_id
            )
        })?;
        if trial.evaluator_version != self.evaluator_version {
            return Err(format!(
                "{} was graded by evaluator {}, but release {} froze evaluator {}",
                trial.task_id, trial.evaluator_version, self.release_id, self.evaluator_version
            ));
        }
        if trial.contract_sha256 != task.contract_sha256 {
            return Err(format!(
                "contract mismatch for {}: the trial's task contract is not the frozen one",
                trial.task_id
            ));
        }
        if trial.scenario_sha256 != task.scenario_sha256 {
            return Err(format!(
                "family or scenario metadata mismatch for {}: its authored scenario source (which carries the family assignment) changed after the freeze",
                trial.task_id
            ));
        }
        if trial.dataset_sha256 != task.dataset_sha256 {
            return Err(format!(
                "world data mismatch for {}: the trial's dataset is not the frozen one",
                trial.task_id
            ));
        }
        Ok(task)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn digest(byte: char) -> String {
        format!("sha256:{}", byte.to_string().repeat(64))
    }

    fn task(id: &str, family: DifficultyFamily) -> ReleaseTask {
        ReleaseTask {
            task_id: id.into(),
            split: SplitAssignment::PublicDevelopment,
            family,
            family_sha256: digest('f'),
            scenario_sha256: digest('a'),
            dataset_sha256: digest('b'),
            contract_sha256: digest('c'),
        }
    }

    fn release() -> Release {
        Release {
            schema_version: RELEASE_SCHEMA,
            release_id: "fixture-release-1".into(),
            frozen_at: "2026-11-09T00:00:00Z".into(),
            evaluator_version: 7,
            family_schema_version: crate::family::FAMILY_SCHEMA,
            comparison: ComparisonDesign {
                schema_version: COMPARISON_SCHEMA,
                repeats_per_task: 3,
                budgets: Budgets {
                    agent_timeout_sec_max: Some(900),
                    inference_tokens_max: None,
                },
                order_policy: OrderPolicy::InterleavedArms,
                replacement_policy: ReplacementPolicy::ReplaceEnvironmentErrors { max_per_task: 1 },
                declared_treatments: vec!["agent".into()],
                denominators: vec![
                    Denominator::Attempts,
                    Denominator::Tasks,
                    Denominator::Families,
                ],
                sequential_stopping: false,
            },
            external_bundle_seals: vec![ExternalBundleSeal {
                bundle_id: "held-out-synthetic-fixture".into(),
                split: SplitAssignment::HeldOut,
                content_sha256: digest('d'),
                admitted_by: "synthetic fixture".into(),
                synthetic_fixture: true,
            }],
            tasks: vec![
                task("a/b/one", DifficultyFamily::EntityAmbiguity),
                task("a/b/two", DifficultyFamily::Baseline),
            ],
        }
    }

    fn identity(task_id: &str) -> TrialIdentity<'_> {
        TrialIdentity {
            task_id,
            evaluator_version: 7,
            scenario_sha256: Box::leak(digest('a').into_boxed_str()),
            dataset_sha256: Box::leak(digest('b').into_boxed_str()),
            contract_sha256: Box::leak(digest('c').into_boxed_str()),
        }
    }

    #[test]
    fn admits_only_frozen_identities() {
        let release = release();
        release.validate().unwrap();
        assert_eq!(
            release.admit(&identity("a/b/one")).unwrap().family,
            DifficultyFamily::EntityAmbiguity
        );
        assert!(
            release
                .admit(&identity("a/b/three"))
                .unwrap_err()
                .contains("not a task")
        );
        let other = digest('e');
        for (change, message) in [
            (
                TrialIdentity {
                    evaluator_version: 8,
                    ..identity("a/b/one")
                },
                "evaluator",
            ),
            (
                TrialIdentity {
                    contract_sha256: &other,
                    ..identity("a/b/one")
                },
                "contract mismatch",
            ),
            (
                TrialIdentity {
                    scenario_sha256: &other,
                    ..identity("a/b/one")
                },
                "family or scenario metadata mismatch",
            ),
            (
                TrialIdentity {
                    dataset_sha256: &other,
                    ..identity("a/b/one")
                },
                "world data mismatch",
            ),
        ] {
            assert!(release.admit(&change).unwrap_err().contains(message));
        }
    }

    #[test]
    fn a_synthetic_seal_cannot_back_held_out_tasks() {
        let mut held_out = release();
        held_out.tasks[1].split = SplitAssignment::HeldOut;
        assert!(held_out.validate().unwrap_err().contains("synthetic"));
        assert!(!held_out.has_real_held_out());
        held_out.external_bundle_seals[0].synthetic_fixture = false;
        held_out.validate().unwrap();
        assert!(held_out.has_real_held_out());
    }

    #[test]
    fn release_design_allows_one_explicit_information_axis() {
        for axis in ["framing", "workspace", "exposure"] {
            let mut declared = release();
            declared.comparison.declared_treatments =
                vec!["agent".into(), format!("information:{axis}")];
            declared.validate().unwrap();
        }
        let mut mixed = release();
        mixed.comparison.declared_treatments =
            vec!["information:framing".into(), "information:workspace".into()];
        assert!(mixed.validate().is_err());
    }

    #[test]
    fn rejects_undeclared_or_incoherent_designs() {
        for change in [
            (|r: &mut Release| r.comparison.repeats_per_task = 0) as fn(&mut Release),
            |r| r.comparison.sequential_stopping = true,
            |r| r.comparison.denominators = vec![Denominator::Tasks],
            |r| r.comparison.declared_treatments = vec!["task".into()],
            |r| {
                r.comparison.replacement_policy =
                    ReplacementPolicy::ReplaceEnvironmentErrors { max_per_task: 0 }
            },
            |r| r.comparison.budgets.agent_timeout_sec_max = Some(0),
            |r| r.tasks.swap(0, 1),
            |r| r.tasks[1].task_id = "a/b/one".into(),
            |r| r.tasks[0].contract_sha256 = "sha256:short".into(),
            |r| r.tasks.clear(),
            |r| r.schema_version = 1,
            |r| r.frozen_at = "yesterday".into(),
            |r| r.family_schema_version = 99,
        ] {
            let mut changed = release();
            change(&mut changed);
            assert!(changed.validate().is_err(), "{changed:?}");
        }
    }

    #[test]
    fn the_release_hash_covers_design_and_assignments() {
        let mut design = release();
        design.comparison.repeats_per_task = 5;
        let mut family = release();
        family.tasks[0].family = DifficultyFamily::Pagination;
        assert_ne!(release().sha256(), design.sha256());
        assert_ne!(release().sha256(), family.sha256());
        assert_eq!(release().family_counts().len(), 2);
    }
}
