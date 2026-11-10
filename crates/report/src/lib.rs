//! Paired comparison of two benchmark runs (e.g. router arm A vs arm B) from Harbor trial
//! `result.json` files. Tasks are the pairing unit: each task's mean reward over its
//! attempts in A is compared with the same task in B.

use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, bail};
use serde::Serialize;
use serde_json::Value;

pub mod conditions;
pub mod explain;
mod provenance;
pub use provenance::Provenance;
pub mod release;
pub mod review;

/// Grader outcome codes (`mock_core::grading::Outcome::code`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Pass,
    Fail,
    ContextOverflow,
    IntegrityViolation,
    InvalidContract,
    EnvironmentError,
    Unresolved,
    /// No reward (trial crashed, timed out, or the grader was unreachable).
    Error,
}

impl Outcome {
    fn from_code(code: Option<f64>) -> Self {
        // Codes are small integers carried as JSON numbers.
        match code {
            Some(0.0) => Self::Pass,
            Some(1.0) => Self::Fail,
            Some(2.0) => Self::ContextOverflow,
            Some(3.0) => Self::IntegrityViolation,
            Some(4.0) => Self::InvalidContract,
            Some(5.0) => Self::EnvironmentError,
            Some(6.0) => Self::Unresolved,
            _ => Self::Error,
        }
    }

    fn key(self) -> &'static str {
        match self {
            Self::Pass => "pass",
            Self::Fail => "fail",
            Self::ContextOverflow => "context_overflow",
            Self::IntegrityViolation => "integrity_violation",
            Self::InvalidContract => "invalid_contract",
            Self::EnvironmentError => "environment_error",
            Self::Unresolved => "unresolved",
            Self::Error => "error",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Trial {
    /// Task directory name, e.g. `code__github__file-login-bug`.
    pub task: String,
    pub reward: f64,
    pub outcome: Outcome,
    pub provenance: Option<Provenance>,
    pub review: Option<review::coordinator::ReviewAudit>,
}

pub fn trial_summary(trials: &[Trial]) -> Value {
    serde_json::json!({
        "schema_version": 1,
        "format": "verified_trial_summary_v1",
        "scope": "Source/execution provenance; not published-image or held-out qualification",
        "trials": trials.iter().map(|trial| {
            serde_json::json!({
                "task": trial.task,
                "reward": trial.reward,
                "outcome": trial.outcome,
                "provenance_checked": trial.provenance.is_some(),
                "inference": trial.provenance.as_ref().and_then(|value| value.inference.as_ref()),
                "surface_audit": conditions::trial_surface_audit(trial),
                "information_condition": trial.provenance.as_ref().and_then(|value| value.condition.as_ref()),
            })
        }).collect::<Vec<_>>(),
    })
}

/// Every trial `result.json` under `root` (job-level `result.json` files are skipped).
pub fn load_trials(root: &Path) -> anyhow::Result<Vec<Trial>> {
    load_trials_inner(root, true)
}

pub(crate) fn load_diagnostic_trials(root: &Path) -> anyhow::Result<Vec<Trial>> {
    load_trials_inner(root, false)
}

fn load_trials_inner(root: &Path, strict_provenance: bool) -> anyhow::Result<Vec<Trial>> {
    let mut files = Vec::new();
    walk(root, &mut files)?;
    files.sort();
    let mut trials = Vec::new();
    for file in files {
        let v: Value = serde_json::from_str(&fs::read_to_string(&file)?)
            .with_context(|| format!("parsing {}", file.display()))?;
        let Some(task_name) = v["task_name"].as_str() else {
            continue;
        };
        let task = task_name
            .rsplit('/')
            .next()
            .unwrap_or(task_name)
            .trim_start_matches("tool-bench__")
            .to_owned();
        let rewards = &v["verifier_result"]["rewards"];
        let outcome = if v["exception_info"].is_null() {
            Outcome::from_code(rewards["outcome_code"].as_f64())
        } else {
            Outcome::Error
        };
        let provenance = if matches!(
            outcome,
            Outcome::Pass
                | Outcome::Fail
                | Outcome::ContextOverflow
                | Outcome::IntegrityViolation
                | Outcome::EnvironmentError
                | Outcome::Unresolved
        ) {
            match provenance::load(file.parent().context("trial directory")?, &v) {
                Ok(value) => value,
                Err(error) if strict_provenance => return Err(error),
                Err(_) => None,
            }
        } else {
            None
        };
        trials.push(Trial {
            task,
            reward: rewards["reward"].as_f64().unwrap_or(0.0),
            outcome,
            provenance,
            review: None,
        });
    }
    Ok(trials)
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) -> anyhow::Result<()> {
    if dir.is_symlink() {
        bail!("trial discovery cannot follow symbolic links");
    }
    let result = dir.join("result.json");
    if result.is_symlink() {
        bail!("trial discovery cannot follow symbolic links");
    }
    if result.is_file() {
        let value: Value = serde_json::from_slice(&fs::read(&result)?)?;
        if value.get("task_name").is_some_and(|name| !name.is_string()) {
            bail!("malformed host task identity in {}", result.display());
        }
        if value["task_name"].is_string() {
            out.push(result);
            return Ok(());
        }
    }
    for entry in fs::read_dir(dir).with_context(|| format!("reading {}", dir.display()))? {
        let path = entry?.path();
        if path.is_symlink() {
            bail!("trial discovery cannot follow symbolic links");
        }
        if path.is_dir()
            && !path.file_name().is_some_and(|name| {
                [
                    "agent",
                    "user-agent",
                    "artifacts",
                    "verifier",
                    "main-snapshot",
                ]
                .iter()
                .any(|reserved| name == *reserved)
            })
        {
            walk(&path, out)?;
        }
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TaskRow {
    pub task: String,
    pub mean_a: f64,
    pub mean_b: f64,
    pub attempts_a: usize,
    pub attempts_b: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct GroupRow {
    pub group: String,
    pub tasks: usize,
    pub mean_a: f64,
    pub mean_b: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Comparison {
    pub admission_level: String,
    pub release_sha256: String,
    pub declared_treatments: Vec<String>,
    pub paired_tasks: usize,
    pub unpaired_tasks: Vec<String>,
    pub mean_a: f64,
    pub mean_b: f64,
    /// mean(B) − mean(A) over paired tasks.
    pub mean_diff: f64,
    /// 95% paired bootstrap interval of `mean_diff` (10,000 resamples, fixed seed).
    pub ci95: (f64, f64),
    pub b_better: usize,
    pub a_better: usize,
    pub ties: usize,
    pub outcomes_a: BTreeMap<&'static str, usize>,
    pub outcomes_b: BTreeMap<&'static str, usize>,
    pub groups: Vec<GroupRow>,
    pub tasks: Vec<TaskRow>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_review: Option<ReviewComparison>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub inference_a: Vec<TrialInference>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub inference_b: Vec<TrialInference>,
    /// Counts of the per-trial `surface_audit` status in each arm.
    pub surface_audit_a: BTreeMap<String, usize>,
    pub surface_audit_b: BTreeMap<String, usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub information_treatment: Option<conditions::InformationTreatment>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TrialInference {
    pub task: String,
    pub session_id: String,
    pub scripted: bool,
    pub classification: mock_core::inference::InferenceClassification,
    pub identity: mock_core::inference::InferenceIdentity,
    pub cost: mock_core::inference::CostAccounting,
}

fn inference_rows(trials: &[Trial]) -> Vec<TrialInference> {
    trials
        .iter()
        .filter_map(|trial| {
            let provenance = trial.provenance.as_ref()?;
            let inference = provenance.inference.as_ref()?;
            Some(TrialInference {
                task: trial.task.clone(),
                session_id: provenance.manifest.session_id.clone(),
                scripted: inference.scripted,
                classification: inference.classification,
                identity: inference.identity.clone(),
                cost: inference.cost.clone(),
            })
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ReviewComparison {
    pub protocol: String,
    pub policy_sha256: String,
    pub reviewer_kind: mock_core::review::ReviewerKind,
    pub implementation_sha256: String,
    pub raw_outcomes_a: BTreeMap<&'static str, usize>,
    pub raw_outcomes_b: BTreeMap<&'static str, usize>,
    pub reviewed_trials_a: usize,
    pub reviewed_trials_b: usize,
    pub trials_a: Vec<review::coordinator::ReviewAudit>,
    pub trials_b: Vec<review::coordinator::ReviewAudit>,
}

fn review_comparison(a: &[Trial], b: &[Trial]) -> anyhow::Result<Option<ReviewComparison>> {
    if !a.iter().chain(b).any(|trial| trial.review.is_some()) {
        return Ok(None);
    }
    let mut condition = None;
    for trial in a.iter().chain(b) {
        let review = trial
            .review
            .as_ref()
            .context("raw and review-assisted trials cannot be mixed")?;
        let provenance = trial
            .provenance
            .as_ref()
            .context("review requires source/execution provenance")?;
        if review.pending() {
            bail!("incomplete content reviews require adjudication before final comparison");
        }
        if review.task != trial.task
            || review.effective_outcome != trial.outcome
            || review.effective_reward != trial.reward
            || provenance.manifest.context.review_policy_sha256.as_ref()
                != Some(&review.policy_sha256)
            || review.session_id != provenance.manifest.session_id
        {
            bail!("review summary does not match its verified trial");
        }
        let value = (
            review.protocol.clone(),
            review.policy_sha256.clone(),
            review.reviewer_kind,
            review.implementation_sha256.clone(),
        );
        if condition
            .as_ref()
            .is_some_and(|expected| expected != &value)
        {
            bail!("different review policies, reviewer kinds or implementations cannot be pooled");
        }
        condition = Some(value);
    }
    let (protocol, policy_sha256, reviewer_kind, implementation_sha256) =
        condition.context("missing review condition")?;
    let records = |trials: &[Trial]| {
        trials
            .iter()
            .map(|trial| trial.review.as_ref().expect("checked review").clone())
            .collect::<Vec<_>>()
    };
    let outcomes = |trials: &[Trial]| {
        let mut values = BTreeMap::new();
        for trial in trials {
            *values
                .entry(
                    trial
                        .review
                        .as_ref()
                        .expect("checked review")
                        .raw_outcome
                        .key(),
                )
                .or_insert(0) += 1;
        }
        values
    };
    Ok(Some(ReviewComparison {
        protocol,
        policy_sha256,
        reviewer_kind,
        implementation_sha256,
        raw_outcomes_a: outcomes(a),
        raw_outcomes_b: outcomes(b),
        reviewed_trials_a: a
            .iter()
            .filter(|trial| {
                trial
                    .review
                    .as_ref()
                    .is_some_and(|review| review.case_sha256.is_some())
            })
            .count(),
        reviewed_trials_b: b
            .iter()
            .filter(|trial| {
                trial
                    .review
                    .as_ref()
                    .is_some_and(|review| review.case_sha256.is_some())
            })
            .count(),
        trials_a: records(a),
        trials_b: records(b),
    }))
}

pub fn compare(a: &[Trial], b: &[Trial]) -> anyhow::Result<Comparison> {
    compare_declared(a, b, false)
}

pub fn compare_declared(
    a: &[Trial],
    b: &[Trial],
    allow_agent_difference: bool,
) -> anyhow::Result<Comparison> {
    compare_treatment(a, b, allow_agent_difference, None)
}

/// [`compare_declared`] that may also declare one information treatment (framing,
/// workspace or exposure). Arms whose information differs in any undeclared way are
/// rejected with the differences listed.
pub fn compare_treatment(
    a: &[Trial],
    b: &[Trial],
    allow_agent_difference: bool,
    information: Option<mock_core::conditions::InformationAxis>,
) -> anyhow::Result<Comparison> {
    let information_treatment = conditions::admit(a, b, information)?;
    let content_review = review_comparison(a, b)?;
    if a.iter()
        .chain(b)
        .any(|trial| trial.outcome == Outcome::Unresolved)
    {
        bail!("unresolved assessments require adjudication before a final paired comparison");
    }
    for trial in a.iter().chain(b) {
        if matches!(
            trial.outcome,
            Outcome::EnvironmentError | Outcome::InvalidContract | Outcome::Error
        ) {
            bail!(
                "invalid or incomplete assessment for {} ({:?}); cannot publish a final paired comparison",
                trial.task,
                trial.outcome
            );
        }
        let expected = if trial.outcome == Outcome::Pass {
            1.0
        } else {
            0.0
        };
        if !trial.reward.is_finite() || trial.reward != expected {
            bail!(
                "inconsistent binary reward/outcome for {}; cannot publish a final paired comparison",
                trial.task
            );
        }
    }
    let by_task = |trials: &[Trial]| {
        let mut m: BTreeMap<String, Vec<f64>> = BTreeMap::new();
        for t in trials {
            m.entry(t.task.clone()).or_default().push(t.reward);
        }
        m
    };
    let (ma, mb) = (by_task(a), by_task(b));
    if !ma.keys().any(|task| mb.contains_key(task)) {
        bail!("no shared tasks; cannot compute a paired comparison");
    }
    if ma.keys().ne(mb.keys()) {
        bail!("task inventories differ; a final score cannot silently use only their intersection");
    }
    let mut release = None;
    for (label, trials) in [("A", a), ("B", b)] {
        let mut expected = BTreeMap::new();
        for trial in trials {
            let provenance = trial.provenance.as_ref().context(
                "verified assessment/execution provenance is required for final comparisons",
            )?;
            let release_signature = provenance.release_signature();
            if release
                .as_ref()
                .is_some_and(|expected| expected != &release_signature)
            {
                bail!(
                    "mixed evaluator/world/tools/runner releases cannot be pooled into one comparison"
                );
            }
            release = Some(release_signature);
            let signature = provenance.compatibility(false);
            if expected
                .insert(&trial.task, signature.clone())
                .is_some_and(|previous| previous != signature)
            {
                bail!(
                    "heterogeneous world/grader/execution conditions within arm {label} for {}",
                    trial.task
                );
            }
        }
    }
    for task in ma.keys() {
        if ma[task].len() != mb[task].len() {
            bail!(
                "repeat counts differ for {task}; an undeclared replacement/repeat policy cannot be inferred"
            );
        }
        let left = a
            .iter()
            .find(|trial| &trial.task == task)
            .expect("grouped task")
            .provenance
            .as_ref()
            .expect("checked provenance");
        let right = b
            .iter()
            .find(|trial| &trial.task == task)
            .expect("grouped task")
            .provenance
            .as_ref()
            .expect("checked provenance");
        if left.compatibility_under(allow_agent_difference, information)
            != right.compatibility_under(allow_agent_difference, information)
        {
            bail!(
                "incompatible world/source/grader/execution conditions for {task}; only explicitly declared agent differences are supported"
            );
        }
    }
    let mean = |v: &[f64]| v.iter().sum::<f64>() / v.len() as f64;
    let tasks: Vec<TaskRow> = ma
        .iter()
        .filter_map(|(task, ra)| {
            mb.get(task).map(|rb| TaskRow {
                task: task.clone(),
                mean_a: mean(ra),
                mean_b: mean(rb),
                attempts_a: ra.len(),
                attempts_b: rb.len(),
            })
        })
        .collect();
    if tasks.is_empty() {
        bail!("no shared tasks; cannot compute a paired comparison");
    }
    let unpaired_tasks = ma
        .keys()
        .chain(mb.keys())
        .filter(|t| !(ma.contains_key(*t) && mb.contains_key(*t)))
        .cloned()
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    let diffs: Vec<f64> = tasks.iter().map(|t| t.mean_b - t.mean_a).collect();
    let task_count = tasks.len() as f64;
    let mean_a = tasks.iter().map(|t| t.mean_a).sum::<f64>() / task_count;
    let mean_b = tasks.iter().map(|t| t.mean_b).sum::<f64>() / task_count;
    let mut groups: BTreeMap<String, Vec<&TaskRow>> = BTreeMap::new();
    for t in &tasks {
        groups
            .entry(t.task.split("__").next().unwrap_or("").to_owned())
            .or_default()
            .push(t);
    }
    let outcomes = |trials: &[Trial]| {
        let mut m = BTreeMap::new();
        for t in trials {
            *m.entry(t.outcome.key()).or_insert(0) += 1;
        }
        m
    };
    let base_release = release.expect("nonempty admitted comparison");
    let release_sha256 = if let Some(review) = &content_review {
        mock_core::provenance::canonical_sha256(&serde_json::json!({
            "base_release":base_release,"review_protocol":review.protocol,
            "review_policy":review.policy_sha256,"reviewer_kind":review.reviewer_kind,
            "review_implementation":review.implementation_sha256
        }))
    } else {
        base_release
    };
    let has_native = a.iter().chain(b).any(|trial| {
        trial
            .provenance
            .as_ref()
            .is_some_and(|provenance| provenance.manifest.execution_kind == "native")
    });
    let admission_level = match (
        has_native,
        content_review.as_ref().map(|review| review.reviewer_kind),
    ) {
        (true, None) => "source_bound_host_capture",
        (true, Some(mock_core::review::ReviewerKind::Human)) => {
            "source_bound_host_capture_with_authorized_human_content_review"
        }
        (true, Some(mock_core::review::ReviewerKind::ScriptedFixture)) => {
            "source_bound_host_capture_with_scripted_review_controls"
        }
        (false, None) => "source_bound_scripted_only",
        (false, Some(mock_core::review::ReviewerKind::Human)) => {
            "source_bound_scripted_with_authorized_human_content_review"
        }
        (false, Some(mock_core::review::ReviewerKind::ScriptedFixture)) => {
            "source_bound_scripted_review_controls_only"
        }
    };
    Ok(Comparison {
        admission_level: admission_level.into(),
        release_sha256,
        declared_treatments: allow_agent_difference
            .then(|| "agent".to_owned())
            .into_iter()
            .chain(information.map(conditions::axis_label))
            .collect(),
        paired_tasks: tasks.len(),
        unpaired_tasks,
        mean_a,
        mean_b,
        mean_diff: mean_b - mean_a,
        ci95: bootstrap_ci(&diffs),
        b_better: diffs.iter().filter(|d| **d > 0.0).count(),
        a_better: diffs.iter().filter(|d| **d < 0.0).count(),
        ties: diffs.iter().filter(|d| **d == 0.0).count(),
        outcomes_a: outcomes(a),
        outcomes_b: outcomes(b),
        groups: groups
            .into_iter()
            .map(|(group, rows)| GroupRow {
                tasks: rows.len(),
                mean_a: rows.iter().map(|r| r.mean_a).sum::<f64>() / rows.len() as f64,
                mean_b: rows.iter().map(|r| r.mean_b).sum::<f64>() / rows.len() as f64,
                group,
            })
            .collect(),
        tasks,
        content_review,
        inference_a: inference_rows(a),
        inference_b: inference_rows(b),
        surface_audit_a: conditions::surface_audit_counts(a),
        surface_audit_b: conditions::surface_audit_counts(b),
        information_treatment,
    })
}

/// Percentile bootstrap of the mean, with a fixed-seed xorshift so reports are reproducible.
fn bootstrap_ci(diffs: &[f64]) -> (f64, f64) {
    if diffs.is_empty() {
        return (0.0, 0.0);
    }
    let mut state: u64 = 0x9E37_79B9_7F4A_7C15;
    let mut next = || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };
    let mut means: Vec<f64> = (0..10_000)
        .map(|_| {
            (0..diffs.len())
                .map(|_| diffs[(next() % diffs.len() as u64) as usize])
                .sum::<f64>()
                / diffs.len() as f64
        })
        .collect();
    means.sort_by(f64::total_cmp);
    (means[249], means[9_749])
}

impl Comparison {
    pub fn markdown(&self) -> String {
        let mut s = String::new();
        s.push_str(&format!("Admission: {}. Source/provenance admission does not certify a published image, held-out release, or production consumer.\n\n", self.admission_level));
        if let Some(review) = &self.content_review {
            s.push_str(&format!(
                "Content review condition: operator-declared {:?}; judgment-assisted effective results are separate from raw deterministic grades. Policy {}. Reviewed trials: A {}, B {}; other trials needed no content review. Scripted fixtures are protocol controls, not human review or model-performance estimates.\n\nRaw outcomes: A {:?}; B {:?}.\n\n",
                review.reviewer_kind,review.policy_sha256,review.reviewed_trials_a,review.reviewed_trials_b,review.raw_outcomes_a,review.raw_outcomes_b
            ));
        }
        if !self.declared_treatments.is_empty() {
            s.push_str(&format!(
                "Declared treatments: {}.\n\n",
                self.declared_treatments.join(", ")
            ));
        }
        s.push_str(&format!(
            "## Paired comparison (B − A)\n\n- Paired tasks: {}\n- Mean reward: A {:.3}, B {:.3}\n- Difference: {:+.3} (95% CI {:+.3} to {:+.3})\n- Tasks where B is better / A is better / tie: {} / {} / {}\n",
            self.paired_tasks, self.mean_a, self.mean_b, self.mean_diff, self.ci95.0, self.ci95.1, self.b_better, self.a_better, self.ties
        ));
        if !self.unpaired_tasks.is_empty() {
            s.push_str(&format!(
                "- Unpaired (in one run only): {}\n",
                self.unpaired_tasks.join(", ")
            ));
        }
        s.push_str("\n### Outcomes (trials)\n\n| Outcome | A | B |\n|---|---|---|\n");
        for key in [
            "pass",
            "fail",
            "context_overflow",
            "integrity_violation",
            "invalid_contract",
            "environment_error",
            "error",
        ] {
            s.push_str(&format!(
                "| {key} | {} | {} |\n",
                self.outcomes_a.get(key).copied().unwrap_or(0),
                self.outcomes_b.get(key).copied().unwrap_or(0)
            ));
        }
        s.push_str("\n### By group\n\n| Group | Tasks | A | B | Δ |\n|---|---|---|---|---|\n");
        for g in &self.groups {
            s.push_str(&format!(
                "| {} | {} | {:.2} | {:.2} | {:+.2} |\n",
                g.group,
                g.tasks,
                g.mean_a,
                g.mean_b,
                g.mean_b - g.mean_a
            ));
        }
        s.push_str("\n### By task\n\n| Task | A | B | Δ |\n|---|---|---|---|\n");
        for t in &self.tasks {
            s.push_str(&format!(
                "| {} | {:.2} | {:.2} | {:+.2} |\n",
                t.task,
                t.mean_a,
                t.mean_b,
                t.mean_b - t.mean_a
            ));
        }
        s
    }
}

#[cfg(test)]
mod outcome_tests {
    use super::Outcome;

    #[test]
    fn preserves_integrity_contract_and_environment_failure_categories() {
        assert_eq!(Outcome::from_code(Some(3.0)), Outcome::IntegrityViolation);
        assert_eq!(Outcome::from_code(Some(4.0)), Outcome::InvalidContract);
        assert_eq!(Outcome::from_code(Some(5.0)), Outcome::EnvironmentError);
    }

    #[test]
    fn fractional_or_nonfinite_outcome_codes_are_not_rounded_into_success() {
        for code in [0.2, 0.8, f64::NAN, f64::INFINITY, -0.2] {
            assert_eq!(Outcome::from_code(Some(code)), Outcome::Error);
        }
    }
}
