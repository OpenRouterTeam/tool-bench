//! Frozen-release admission for paired comparisons (improvement-plan items
//! 15 and 19).
//!
//! `compare` admits both arms against a release manifest *before* pairing:
//! every trial must be a task of the release with the frozen contract,
//! scenario (family) and dataset identities and the frozen evaluator; attempt
//! counts must equal the prespecified repeats; replacements must follow the
//! declared policy. Rates are then reported per declared denominator with the
//! uncertainty of each stated separately. A comparison run without a manifest
//! is labeled as an unfrozen legacy result, never as a release score.

use std::{collections::BTreeMap, path::Path};

use anyhow::{Context, bail};
use mock_core::{
    family::DifficultyFamily,
    release::{Release, ReleaseTask, ReplacementPolicy, SplitAssignment, TrialIdentity},
};
use serde::Serialize;

use crate::{Comparison, Outcome, Trial};

pub const FROZEN: &str = "frozen_release";
pub const UNFROZEN: &str = "unfrozen_legacy_not_a_release_result";

/// Reads and validates a release manifest. Unknown fields fail.
pub fn load(path: &Path) -> anyhow::Result<Release> {
    let release: Release = serde_json::from_slice(&crate::provenance::read(path)?)
        .with_context(|| format!("parsing release manifest {}", path.display()))?;
    release.validate().map_err(anyhow::Error::msg)?;
    Ok(release)
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ReleaseComparison {
    pub release_admission: ReleaseAdmission,
    #[serde(flatten)]
    pub comparison: Comparison,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ReleaseAdmission {
    /// [`FROZEN`] or [`UNFROZEN`].
    pub level: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frozen: Option<FrozenAdmission>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FrozenAdmission {
    pub release_id: String,
    pub release_manifest_sha256: String,
    pub frozen_at: String,
    pub evaluator_version: u32,
    pub split: SplitAssignment,
    /// What, if anything, this release can say about held-out families.
    pub held_out: &'static str,
    pub design: mock_core::release::ComparisonDesign,
    /// Order is declared by the design; trial evidence cannot confirm it.
    pub order_policy_verified: bool,
    pub denominators: Denominators,
    pub families: Vec<FamilyRow>,
    pub uncertainty: Uncertainty,
    /// Every excluded attempt, so admission failures stay visible.
    pub replaced_attempts: Vec<ReplacedAttempt>,
}

/// The unit counts behind every reported rate.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Denominators {
    pub tasks: usize,
    pub families: usize,
    pub counted_attempts_a: usize,
    pub counted_attempts_b: usize,
    pub replaced_attempts_a: usize,
    pub replaced_attempts_b: usize,
    /// Attempt-level pass rate: passes / counted attempts.
    pub attempt_pass_rate_a: f64,
    pub attempt_pass_rate_b: f64,
    pub retry_semantics: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FamilyRow {
    pub family: DifficultyFamily,
    pub tasks: usize,
    pub counted_attempts_a: usize,
    pub counted_attempts_b: usize,
    /// Mean over this family's tasks of each task's mean reward.
    pub task_mean_a: f64,
    pub task_mean_b: f64,
}

/// Three different questions, never merged into one interval.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Uncertainty {
    /// Would repeating the same attempts on this fixed suite change the
    /// difference? Resamples attempts within each task; tasks stay fixed.
    pub repeated_attempts_ci95: (f64, f64),
    /// Resamples tasks (the comparison's `ci95`): other tasks like these.
    pub task_resampling_ci95: (f64, f64),
    /// Resamples whole families: tasks of one family are not independent
    /// evidence. Absent with fewer than two families.
    pub family_cluster_ci95: Option<(f64, f64)>,
    pub generalization: &'static str,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ReplacedAttempt {
    pub arm: &'static str,
    pub task: String,
    pub outcome: Outcome,
}

fn task_name(task: &ReleaseTask) -> String {
    task.task_id.replace('/', "__")
}

fn scorable(outcome: Outcome) -> bool {
    matches!(
        outcome,
        Outcome::Pass | Outcome::Fail | Outcome::ContextOverflow | Outcome::IntegrityViolation
    )
}

struct Arm<'a> {
    counted: Vec<Trial>,
    replaced: Vec<ReplacedAttempt>,
    rewards: BTreeMap<&'a str, Vec<f64>>,
}

fn admit_arm<'a>(
    label: &'static str,
    trials: &[Trial],
    release: &'a Release,
    names: &BTreeMap<String, &'a ReleaseTask>,
) -> anyhow::Result<Arm<'a>> {
    let design = &release.comparison;
    let mut arm = Arm {
        counted: Vec::new(),
        replaced: Vec::new(),
        rewards: BTreeMap::new(),
    };
    let mut unscorable: BTreeMap<&str, u32> = BTreeMap::new();
    let mut conditions = BTreeMap::new();
    let mut release_signature = None;
    for trial in trials {
        let task = *names.get(&trial.task).with_context(|| {
            format!(
                "arm {label}: {} is not a task of release {}",
                trial.task, release.release_id
            )
        })?;
        match trial.outcome {
            Outcome::Unresolved => bail!(
                "arm {label}: unresolved assessments require adjudication before a release comparison"
            ),
            Outcome::InvalidContract => bail!(
                "arm {label}: {} has an invalid contract; that is an evaluator defect, never a replaceable attempt",
                trial.task
            ),
            Outcome::Error => bail!(
                "arm {label}: {} has no verified assessment; a resultless attempt is not a verified environment-error replacement",
                trial.task
            ),
            _ => {}
        }
        let provenance = trial.provenance.as_ref().with_context(|| {
            format!(
                "arm {label}: {} lacks verified provenance and cannot be admitted to a frozen release, including as a replacement",
                trial.task
            )
        })?;
        release
            .admit(&TrialIdentity {
                task_id: &provenance.source.scenario,
                evaluator_version: provenance.evaluator_version,
                scenario_sha256: &provenance.source.scenario_sha256,
                dataset_sha256: &provenance.source.dataset_sha256,
                contract_sha256: &provenance.source.contract_sha256,
            })
            .map_err(|reason| anyhow::anyhow!("arm {label}: {reason}"))?;
        if provenance.source.scenario != task.task_id {
            bail!("arm {label}: {} ran another scenario's source", trial.task);
        }
        if let Some(limit) = design.budgets.agent_timeout_sec_max
            && provenance.manifest.agent_timeout_sec > limit as f64
        {
            bail!(
                "arm {label}: {} ran with a {}s agent budget, above the declared {limit}s",
                trial.task,
                provenance.manifest.agent_timeout_sec
            );
        }
        let signature = provenance.release_signature();
        if release_signature
            .replace(signature.clone())
            .is_some_and(|previous| previous != signature)
        {
            bail!(
                "arm {label}: mixed evaluator/world/tools/runner releases, including replaced attempts"
            );
        }
        let signature = provenance.compatibility(false);
        if conditions
            .insert(&trial.task, signature.clone())
            .is_some_and(|previous| previous != signature)
        {
            bail!(
                "arm {label}: heterogeneous world/grader/execution conditions for {}, including replaced attempts",
                trial.task
            );
        }
        match trial.outcome {
            Outcome::EnvironmentError => {
                let ReplacementPolicy::ReplaceEnvironmentErrors { max_per_task } =
                    design.replacement_policy
                else {
                    bail!(
                        "arm {label}: {} has an unscorable attempt and release {} declares no replacement",
                        trial.task,
                        release.release_id
                    );
                };
                let used = unscorable.entry(task.task_id.as_str()).or_default();
                *used += 1;
                if *used > max_per_task {
                    bail!(
                        "arm {label}: {} exceeds the declared {max_per_task} replacement(s) per task",
                        trial.task
                    );
                }
                arm.replaced.push(ReplacedAttempt {
                    arm: label,
                    task: trial.task.clone(),
                    outcome: trial.outcome,
                });
            }
            _ => {
                debug_assert!(scorable(trial.outcome));
                arm.rewards
                    .entry(task.task_id.as_str())
                    .or_default()
                    .push(trial.reward);
                arm.counted.push(trial.clone());
            }
        }
    }
    Ok(arm)
}

/// Admits both arms to `release`, then runs the paired comparison.
pub fn compare(
    a: &[Trial],
    b: &[Trial],
    release: &Release,
    allow_agent_difference: bool,
) -> anyhow::Result<ReleaseComparison> {
    compare_treatment(a, b, release, allow_agent_difference, None)
}

pub fn compare_treatment(
    a: &[Trial],
    b: &[Trial],
    release: &Release,
    allow_agent_difference: bool,
    information: Option<mock_core::conditions::InformationAxis>,
) -> anyhow::Result<ReleaseComparison> {
    release.validate().map_err(anyhow::Error::msg)?;
    let design = &release.comparison;
    let declared_information = design
        .declared_treatments
        .iter()
        .find(|t| t.starts_with("information:"));
    if declared_information.map(String::as_str)
        != information.map(crate::conditions::axis_label).as_deref()
    {
        bail!("requested information treatment differs from the frozen release design");
    }
    let agent_declared = design.declared_treatments.iter().any(|t| t == "agent");
    if allow_agent_difference && !agent_declared {
        bail!(
            "release {} does not declare the agent as a treatment; an undeclared difference cannot be allowed at report time",
            release.release_id
        );
    }
    if design.budgets.inference_tokens_max.is_some() {
        bail!("a declared inference-token budget cannot yet be verified from trial evidence");
    }
    let names: BTreeMap<String, &ReleaseTask> = release
        .tasks
        .iter()
        .map(|task| (task_name(task), task))
        .collect();
    let left = admit_arm("A", a, release, &names)?;
    let right = admit_arm("B", b, release, &names)?;

    let mut splits = a.iter().chain(b).map(|trial| names[&trial.task].split);
    let split = splits
        .next()
        .context("no admitted attempts; an empty comparison is not a zero-effect finding")?;
    if splits.any(|other| other != split) {
        bail!("public-development and held-out tasks must be compared separately");
    }
    if split == SplitAssignment::HeldOut && !release.has_real_held_out() {
        bail!("this release seals only a synthetic held-out fixture; no held-out result exists");
    }
    let expected: Vec<&ReleaseTask> = release
        .tasks
        .iter()
        .filter(|task| task.split == split)
        .collect();
    for (label, arm) in [("A", &left), ("B", &right)] {
        for task in &expected {
            let attempts = arm.rewards.get(task.task_id.as_str()).map_or(0, Vec::len);
            if attempts != design.repeats_per_task as usize {
                bail!(
                    "arm {label}: {} has {attempts} counted attempt(s); release {} prespecified {} per task, so an undeclared repeat, replacement or omission cannot be admitted",
                    task.task_id,
                    release.release_id,
                    design.repeats_per_task
                );
            }
        }
    }

    let comparison =
        crate::compare_treatment(&left.counted, &right.counted, agent_declared, information)?;
    let mut families: BTreeMap<DifficultyFamily, Vec<&ReleaseTask>> = BTreeMap::new();
    for task in &expected {
        families.entry(task.family).or_default().push(task);
    }
    let mean = |values: &[f64]| values.iter().sum::<f64>() / values.len() as f64;
    let task_mean = |arm: &Arm<'_>, task: &ReleaseTask| mean(&arm.rewards[task.task_id.as_str()]);
    let family_rows = families
        .iter()
        .map(|(family, tasks)| FamilyRow {
            family: *family,
            tasks: tasks.len(),
            counted_attempts_a: tasks.len() * design.repeats_per_task as usize,
            counted_attempts_b: tasks.len() * design.repeats_per_task as usize,
            task_mean_a: mean(
                &tasks
                    .iter()
                    .map(|task| task_mean(&left, task))
                    .collect::<Vec<_>>(),
            ),
            task_mean_b: mean(
                &tasks
                    .iter()
                    .map(|task| task_mean(&right, task))
                    .collect::<Vec<_>>(),
            ),
        })
        .collect();
    let pass_rate = |arm: &Arm<'_>| {
        arm.counted
            .iter()
            .filter(|trial| trial.outcome == Outcome::Pass)
            .count() as f64
            / arm.counted.len() as f64
    };
    let paired: Vec<(&[f64], &[f64])> = expected
        .iter()
        .map(|task| {
            (
                left.rewards[task.task_id.as_str()].as_slice(),
                right.rewards[task.task_id.as_str()].as_slice(),
            )
        })
        .collect();
    let clusters: Vec<Vec<f64>> = families
        .values()
        .map(|tasks| {
            tasks
                .iter()
                .map(|task| task_mean(&right, task) - task_mean(&left, task))
                .collect()
        })
        .collect();
    let retry_semantics = match design.replacement_policy {
        ReplacementPolicy::NoReplacement => {
            "every attempt counts; no attempt was replaced or repeated".to_owned()
        }
        ReplacementPolicy::ReplaceEnvironmentErrors { max_per_task } => format!(
            "only provenance-verified evaluator-classified environment errors are replaced, at most {max_per_task} per task; resultless attempts are unqualified and model failures always count"
        ),
    };
    let frozen = FrozenAdmission {
        release_id: release.release_id.clone(),
        release_manifest_sha256: release.sha256(),
        frozen_at: release.frozen_at.clone(),
        evaluator_version: release.evaluator_version,
        split,
        held_out: if release.has_real_held_out() {
            "sealed_external_bundle"
        } else if release.external_bundle_seals.is_empty() {
            "none"
        } else {
            "synthetic_fixture_seal_only_no_held_out_claim"
        },
        design: design.clone(),
        order_policy_verified: false,
        denominators: Denominators {
            tasks: expected.len(),
            families: families.len(),
            counted_attempts_a: left.counted.len(),
            counted_attempts_b: right.counted.len(),
            replaced_attempts_a: left.replaced.len(),
            replaced_attempts_b: right.replaced.len(),
            attempt_pass_rate_a: pass_rate(&left),
            attempt_pass_rate_b: pass_rate(&right),
            retry_semantics,
        },
        families: family_rows,
        uncertainty: Uncertainty {
            repeated_attempts_ci95: attempt_bootstrap(&paired),
            task_resampling_ci95: comparison.ci95,
            family_cluster_ci95: (clusters.len() >= 2).then(|| cluster_bootstrap(&clusters)),
            generalization: if split == SplitAssignment::HeldOut {
                "held-out split of a sealed external bundle; family-cluster interval applies"
            } else {
                "public-development split: these intervals describe this fixed public suite and its families, not unseen held-out families"
            },
        },
        replaced_attempts: left.replaced.into_iter().chain(right.replaced).collect(),
    };
    Ok(ReleaseComparison {
        release_admission: ReleaseAdmission {
            level: FROZEN,
            frozen: Some(frozen),
        },
        comparison,
    })
}

/// A comparison without a release manifest: still source/provenance checked,
/// but explicitly not a release result.
pub fn compare_unfrozen(
    a: &[Trial],
    b: &[Trial],
    allow_agent_difference: bool,
) -> anyhow::Result<ReleaseComparison> {
    compare_unfrozen_treatment(a, b, allow_agent_difference, None)
}

pub fn compare_unfrozen_treatment(
    a: &[Trial],
    b: &[Trial],
    allow_agent_difference: bool,
    information: Option<mock_core::conditions::InformationAxis>,
) -> anyhow::Result<ReleaseComparison> {
    Ok(ReleaseComparison {
        release_admission: ReleaseAdmission {
            level: UNFROZEN,
            frozen: None,
        },
        comparison: crate::compare_treatment(a, b, allow_agent_difference, information)?,
    })
}

fn generator() -> impl FnMut() -> u64 {
    let mut state: u64 = 0x2545_F491_4F6C_DD1D;
    move || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    }
}

fn interval(mut means: Vec<f64>) -> (f64, f64) {
    means.sort_by(f64::total_cmp);
    (
        means[means.len() * 25 / 1000],
        means[means.len() * 975 / 1000 - 1],
    )
}

/// Resamples attempts within each task and arm; the task set is fixed.
fn attempt_bootstrap(tasks: &[(&[f64], &[f64])]) -> (f64, f64) {
    let mut next = generator();
    let mut resampled = |values: &[f64]| {
        (0..values.len())
            .map(|_| values[(next() % values.len() as u64) as usize])
            .sum::<f64>()
            / values.len() as f64
    };
    interval(
        (0..10_000)
            .map(|_| {
                tasks
                    .iter()
                    .map(|(a, b)| resampled(b) - resampled(a))
                    .sum::<f64>()
                    / tasks.len() as f64
            })
            .collect(),
    )
}

/// Resamples whole families, keeping each family's tasks together.
fn cluster_bootstrap(clusters: &[Vec<f64>]) -> (f64, f64) {
    let mut next = generator();
    interval(
        (0..10_000)
            .map(|_| {
                let (mut total, mut count) = (0.0, 0usize);
                for _ in 0..clusters.len() {
                    let cluster = &clusters[(next() % clusters.len() as u64) as usize];
                    total += cluster.iter().sum::<f64>();
                    count += cluster.len();
                }
                total / count as f64
            })
            .collect(),
    )
}

impl ReleaseComparison {
    pub fn markdown(&self) -> String {
        let mut text = self.comparison.markdown();
        text.push_str("\n### Release admission\n\n");
        let Some(frozen) = &self.release_admission.frozen else {
            text.push_str("No release manifest was supplied. This is an unfrozen legacy comparison: splits, families, repeats, budgets and replacements were not prespecified, so it is not a release result.\n");
            return text;
        };
        let d = &frozen.denominators;
        text.push_str(&format!(
            "- Release {} (manifest {}), frozen {}, evaluator {}, split {}\n- Held-out status: {}\n- Design: {} counted attempt(s) per task per arm; order {:?} (declared, not verifiable from trial evidence); {}\n- Denominators: {} tasks in {} families; counted attempts A {}, B {}; replaced attempts A {}, B {}\n- Attempt-level pass rate: A {:.3}, B {:.3}\n- Difference uncertainty (B − A), 95%: repeated attempts on this fixed suite {:+.3} to {:+.3}; task resampling {:+.3} to {:+.3}; family clusters {}\n- {}\n",
            frozen.release_id,
            frozen.release_manifest_sha256,
            frozen.frozen_at,
            frozen.evaluator_version,
            frozen.split.key(),
            frozen.held_out,
            frozen.design.repeats_per_task,
            frozen.design.order_policy,
            d.retry_semantics,
            d.tasks,
            d.families,
            d.counted_attempts_a,
            d.counted_attempts_b,
            d.replaced_attempts_a,
            d.replaced_attempts_b,
            d.attempt_pass_rate_a,
            d.attempt_pass_rate_b,
            frozen.uncertainty.repeated_attempts_ci95.0,
            frozen.uncertainty.repeated_attempts_ci95.1,
            frozen.uncertainty.task_resampling_ci95.0,
            frozen.uncertainty.task_resampling_ci95.1,
            frozen.uncertainty.family_cluster_ci95.map_or_else(
                || "not estimable with fewer than two families".to_owned(),
                |(low, high)| format!("{low:+.3} to {high:+.3}")
            ),
            frozen.uncertainty.generalization,
        ));
        text.push_str(
            "\n| Family | Tasks | Attempts A | Attempts B | A | B | Δ |\n|---|---|---|---|---|---|---|\n",
        );
        for row in &frozen.families {
            text.push_str(&format!(
                "| {} | {} | {} | {} | {:.2} | {:.2} | {:+.2} |\n",
                row.family.key(),
                row.tasks,
                row.counted_attempts_a,
                row.counted_attempts_b,
                row.task_mean_a,
                row.task_mean_b,
                row.task_mean_b - row.task_mean_a
            ));
        }
        for replaced in &frozen.replaced_attempts {
            text.push_str(&format!(
                "- Replaced attempt (arm {}): {} ended {:?}\n",
                replaced.arm, replaced.task, replaced.outcome
            ));
        }
        text
    }
}
