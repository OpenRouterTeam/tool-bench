//! Per-arm summaries and paired comparisons for runs made through stock Harbor.
//!
//! One run directory is one arm. Stock Harbor publishes no runner receipt and no realized
//! manifest, so these trials can never satisfy the attested comparison in the crate root
//! (`bench-report --require-attested`). This module reports them anyway and says how far
//! each result was checked: see [`Assurance`].
//!
//! Nothing here turns a missing assessment into a model failure. A pass rate is passes
//! over assessed trials (pass, fail, context overflow, integrity violation); unresolved
//! trials and trials with no assessment are counted beside it, never inside it.

use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

use anyhow::bail;
use mock_core::grading::EVALUATOR_VERSION;
use serde::Serialize;

mod markdown;
mod trial;

pub use markdown::explanation as explanation_markdown;
pub use trial::{Assurance, HostException, Status, TrialRecord, Usage};

/// Arms whose excluded shares differ by at least this much get a warning: exclusions that
/// fall unevenly on tasks bias a pass rate computed over what is left.
const EXCLUSION_GAP: f64 = 0.05;

/// Checks that are about the model's work. `contract_ok`, `environment_ok` and
/// `runner_admission_ok` describe the benchmark, and a trial failing those is not assessed.
const BENCHMARK_CHECKS: [&str; 3] = ["contract_ok", "environment_ok", "runner_admission_ok"];

#[derive(Debug, Clone, PartialEq)]
pub struct Arm {
    /// `A`, `B`, … in the order the run directories were given.
    pub label: String,
    pub directory: String,
    pub trials: Vec<TrialRecord>,
    /// Symbolic links met during discovery. They are never followed.
    pub skipped_links: Vec<String>,
}

/// Loads every trial under `directory`. An arm with no trials is an error: an empty
/// directory is far more often a wrong path than a run with nothing in it.
pub fn load_arm(label: &str, directory: &Path) -> anyhow::Result<Arm> {
    if !directory.is_dir() {
        bail!("{} is not a directory", directory.display());
    }
    let (mut found, mut skipped_links) = (Vec::new(), Vec::new());
    trial::discover(directory, &mut found, &mut skipped_links)?;
    if found.is_empty() {
        bail!(
            "no Harbor trial directories (result.json beside verifier/) under {}",
            directory.display()
        );
    }
    // Re-running an assessment parses and grades a whole world; do it on every core.
    let workers = std::thread::available_parallelism()
        .map_or(1, usize::from)
        .min(found.len());
    let mut trials: Vec<TrialRecord> = std::thread::scope(|scope| {
        let handles: Vec<_> = found
            .chunks(found.len().div_ceil(workers))
            .map(|paths| {
                scope.spawn(move || {
                    paths
                        .iter()
                        .map(|path| trial::load(directory, path))
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        handles
            .into_iter()
            .flat_map(|handle| handle.join().expect("a trial loader panicked"))
            .collect()
    });
    trials
        .sort_by(|left, right| (&left.task, &left.directory).cmp(&(&right.task, &right.directory)));
    Ok(Arm {
        label: label.to_owned(),
        directory: directory.display().to_string(),
        trials,
        skipped_links,
    })
}

pub fn label(index: usize) -> String {
    match u8::try_from(index) {
        Ok(index) if index < 26 => char::from(b'A' + index).to_string(),
        _ => format!("arm{}", index + 1),
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PassRate {
    pub passed: usize,
    /// Pass, fail, context overflow and integrity violation.
    pub assessed: usize,
    /// `passed / assessed`; absent when nothing was assessed.
    pub rate: Option<f64>,
    /// 95% Wilson score interval. It treats trials as independent, so with repeated
    /// attempts per task it is narrower than the task-level uncertainty.
    pub ci95: Option<(f64, f64)>,
    /// Trials left out of the denominator: unresolved or without an assessment.
    pub excluded: usize,
}

fn wilson(passed: usize, total: usize) -> Option<(f64, f64)> {
    if total == 0 {
        return None;
    }
    let (n, p, z) = (
        total as f64,
        passed as f64 / total as f64,
        1.959_963_984_540_054_f64,
    );
    let denominator = 1.0 + z * z / n;
    let centre = (p + z * z / (2.0 * n)) / denominator;
    let half = z * (p * (1.0 - p) / n + z * z / (4.0 * n * n)).sqrt() / denominator;
    Some(((centre - half).max(0.0), (centre + half).min(1.0)))
}

fn pass_rate<'a>(trials: impl IntoIterator<Item = &'a TrialRecord>) -> PassRate {
    let (mut passed, mut assessed, mut excluded) = (0, 0, 0);
    for trial in trials {
        if trial.status.assessed() {
            assessed += 1;
            passed += usize::from(trial.status == Status::Pass);
        } else {
            excluded += 1;
        }
    }
    PassRate {
        passed,
        assessed,
        rate: (assessed > 0).then(|| passed as f64 / assessed as f64),
        ci95: wilson(passed, assessed),
        excluded,
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct GroupRate {
    pub group: String,
    pub tasks: usize,
    pub trials: usize,
    #[serde(flatten)]
    pub rate: PassRate,
}

fn grouped(trials: &[TrialRecord], key: impl Fn(&TrialRecord) -> String) -> Vec<GroupRate> {
    let mut groups: BTreeMap<String, Vec<&TrialRecord>> = BTreeMap::new();
    for trial in trials {
        groups.entry(key(trial)).or_default().push(trial);
    }
    groups
        .into_iter()
        .map(|(group, trials)| GroupRate {
            group,
            tasks: trials
                .iter()
                .map(|trial| &trial.task)
                .collect::<BTreeSet<_>>()
                .len(),
            trials: trials.len(),
            rate: pass_rate(trials.iter().copied()),
        })
        .collect()
}

/// The `business`, `cloud`, `code`, `collab`, `ops` or `search` prefix of a task name.
pub fn task_group(task: &str) -> &str {
    task.split("__").next().unwrap_or(task)
}

/// `multi-service` for `<group>__multi__<slug>` tasks, `single-service` otherwise.
pub fn task_scope(task: &str) -> &'static str {
    if task.split("__").nth(1) == Some("multi") {
        "multi-service"
    } else {
        "single-service"
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Distribution {
    pub n: usize,
    pub mean: f64,
    pub median: f64,
}

fn distribution(mut values: Vec<f64>) -> Option<Distribution> {
    if values.is_empty() {
        return None;
    }
    values.sort_by(f64::total_cmp);
    let middle = values.len() / 2;
    Some(Distribution {
        n: values.len(),
        mean: values.iter().sum::<f64>() / values.len() as f64,
        median: if values.len() % 2 == 1 {
            values[middle]
        } else {
            (values[middle - 1] + values[middle]) / 2.0
        },
    })
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Count {
    pub name: String,
    pub trials: usize,
}

fn counts(names: impl IntoIterator<Item = String>) -> Vec<Count> {
    let mut counted: BTreeMap<String, usize> = BTreeMap::new();
    for name in names {
        *counted.entry(name).or_default() += 1;
    }
    let mut counted: Vec<Count> = counted
        .into_iter()
        .map(|(name, trials)| Count { name, trials })
        .collect();
    counted.sort_by(|left, right| {
        right
            .trials
            .cmp(&left.trials)
            .then(left.name.cmp(&right.name))
    });
    counted
}

/// Usage and timing as the harness recorded them. The evaluator does not see or check
/// these, and `trials_reporting` says how many trials carried each figure.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct HarnessUsage {
    pub trials_reporting_tokens: usize,
    pub input_tokens: Option<u64>,
    pub cache_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub trials_reporting_cost: usize,
    pub cost_usd: Option<f64>,
    pub agent_seconds: Option<Distribution>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TaskConsistency {
    /// Tasks with at least one assessed trial.
    pub tasks: usize,
    pub passed_every_attempt: usize,
    pub passed_some_attempts: usize,
    pub passed_no_attempt: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Unassessed {
    pub task: String,
    pub trial: String,
    pub status: Status,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ArmSummary {
    pub label: String,
    pub directory: String,
    /// Agent, version and model as the host recorded them, most frequent first.
    pub agents: Vec<Count>,
    pub evaluator_versions: Vec<u32>,
    pub trials: usize,
    pub tasks: usize,
    pub attempts_per_task: (usize, usize),
    pub assurance: BTreeMap<&'static str, usize>,
    pub outcomes: BTreeMap<&'static str, usize>,
    pub pass_rate: PassRate,
    pub task_consistency: TaskConsistency,
    pub groups: Vec<GroupRate>,
    pub scopes: Vec<GroupRate>,
    /// `api_calls` over assessed trials, and over the passed ones alone.
    pub api_calls: Option<Distribution>,
    pub api_calls_passed: Option<Distribution>,
    /// Checks that failed in trials graded fail or integrity violation, most frequent
    /// first. One trial can fail several.
    pub failed_checks: Vec<Count>,
    /// Failed trials whose agent made no world request at all.
    pub failed_without_api_calls: usize,
    pub mcp_bypass_trials: usize,
    /// Host exceptions on trials that were still assessed (an agent timeout is graded).
    pub exceptions_on_assessed_trials: Vec<Count>,
    pub usage: HarnessUsage,
    /// Every trial left out of the pass rate, with the reason.
    pub unassessed: Vec<Unassessed>,
    pub warnings: Vec<String>,
    pub trial_records: Vec<TrialRecord>,
}

fn sum(values: impl Iterator<Item = Option<u64>>) -> Option<u64> {
    values.flatten().reduce(|total, value| total + value)
}

fn by_task(trials: &[TrialRecord]) -> BTreeMap<&str, Vec<&TrialRecord>> {
    let mut tasks: BTreeMap<&str, Vec<&TrialRecord>> = BTreeMap::new();
    for trial in trials {
        tasks.entry(&trial.task).or_default().push(trial);
    }
    tasks
}

pub fn summarize(arm: &Arm) -> ArmSummary {
    let trials = &arm.trials;
    let tasks = by_task(trials);
    let assessed: Vec<&TrialRecord> = trials
        .iter()
        .filter(|trial| trial.status.assessed())
        .collect();
    let failed: Vec<&TrialRecord> = assessed
        .iter()
        .copied()
        .filter(|trial| matches!(trial.status, Status::Fail | Status::IntegrityViolation))
        .collect();
    let api_calls = |trials: &mut dyn Iterator<Item = &&TrialRecord>| {
        distribution(
            trials
                .filter_map(|trial| trial.metric("api_calls"))
                .collect(),
        )
    };
    let mut consistency = TaskConsistency {
        tasks: 0,
        passed_every_attempt: 0,
        passed_some_attempts: 0,
        passed_no_attempt: 0,
    };
    for attempts in tasks.values() {
        let rate = pass_rate(attempts.iter().copied());
        if rate.assessed == 0 {
            continue;
        }
        consistency.tasks += 1;
        if rate.passed == rate.assessed {
            consistency.passed_every_attempt += 1;
        } else if rate.passed == 0 {
            consistency.passed_no_attempt += 1;
        } else {
            consistency.passed_some_attempts += 1;
        }
    }
    let mut evaluator_versions: Vec<u32> = trials
        .iter()
        .filter_map(|trial| trial.evaluator_version)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    evaluator_versions.sort_unstable();
    // A trial without result.json has no recorded version or model to compare.
    let agents = counts(
        trials
            .iter()
            .filter(|trial| trial.status != Status::Incomplete)
            .map(|trial| {
                let mut name = trial.agent.clone();
                if let Some(version) = &trial.agent_version {
                    name.push_str(&format!(" {version}"));
                }
                if let Some(model) = &trial.model {
                    name.push_str(&format!(", model {model}"));
                }
                name
            }),
    );

    let mut warnings = Vec::new();
    let count = |assurance: Assurance| {
        trials
            .iter()
            .filter(|trial| trial.assurance == assurance)
            .count()
    };
    // The reason most of an arm's trials share is usually the whole story.
    let common_reason = |assurance: Assurance| {
        counts(
            trials
                .iter()
                .filter(|trial| trial.assurance == assurance)
                .filter_map(|trial| trial.note.clone()),
        )
        .first()
        .map_or_else(String::new, |reason| {
            format!(
                " Most common reason ({} trial(s)): {}.",
                reason.trials, reason.name
            )
        })
    };
    if count(Assurance::Failed) > 0 {
        warnings.push(format!(
            "{} trial(s) failed re-verification: the recorded result disagrees with the collected artifacts. They are excluded from every rate.{}",
            count(Assurance::Failed),
            common_reason(Assurance::Failed)
        ));
    }
    if count(Assurance::Recorded) > 0 {
        warnings.push(format!(
            "{} trial(s) are recorded only: the verifier's output is taken as written and was not re-run by this tool.{}",
            count(Assurance::Recorded),
            common_reason(Assurance::Recorded)
        ));
    }
    if evaluator_versions.len() > 1 {
        warnings.push(format!(
            "trials were graded by more than one evaluator version ({}); their grades are not comparable with each other.",
            list(&evaluator_versions)
        ));
    }
    if agents.len() > 1 {
        warnings.push(format!(
            "trials ran under {} different agent/model identities.",
            agents.len()
        ));
    }
    let mixed_contracts = tasks
        .values()
        .filter(|attempts| distinct(attempts, |trial| trial.contract_sha256.as_ref()) > 1)
        .count();
    if mixed_contracts > 0 {
        warnings.push(format!(
            "{mixed_contracts} task(s) were graded against more than one contract within this arm."
        ));
    }
    if !arm.skipped_links.is_empty() {
        warnings.push(format!(
            "{} symbolic link(s) were not followed during trial discovery.",
            arm.skipped_links.len()
        ));
    }

    let attempts = tasks.values().map(Vec::len);
    ArmSummary {
        label: arm.label.clone(),
        directory: arm.directory.clone(),
        agents,
        evaluator_versions,
        trials: trials.len(),
        tasks: tasks.len(),
        attempts_per_task: (
            attempts.clone().min().unwrap_or(0),
            attempts.max().unwrap_or(0),
        ),
        assurance: Assurance::ALL
            .into_iter()
            .map(|assurance| (assurance.key(), count(assurance)))
            .collect(),
        outcomes: Status::ALL
            .into_iter()
            .map(|status| {
                (
                    status.key(),
                    trials.iter().filter(|trial| trial.status == status).count(),
                )
            })
            .collect(),
        pass_rate: pass_rate(trials),
        task_consistency: consistency,
        groups: grouped(trials, |trial| task_group(&trial.task).to_owned()),
        scopes: grouped(trials, |trial| task_scope(&trial.task).to_owned()),
        api_calls: api_calls(&mut assessed.iter()),
        api_calls_passed: api_calls(
            &mut assessed.iter().filter(|trial| trial.status == Status::Pass),
        ),
        failed_checks: counts(failed.iter().flat_map(|trial| {
            trial
                .metrics
                .iter()
                .filter(|(name, value)| {
                    name.ends_with("_ok")
                        && **value == 0.0
                        && !BENCHMARK_CHECKS.contains(&name.as_str())
                })
                .map(|(name, _)| name.trim_end_matches("_ok").to_owned())
        })),
        failed_without_api_calls: failed
            .iter()
            .filter(|trial| trial.metric("api_calls") == Some(0.0))
            .count(),
        mcp_bypass_trials: trials
            .iter()
            .filter(|trial| trial.metric("mcp_bypass").is_some_and(|value| value > 0.0))
            .count(),
        exceptions_on_assessed_trials: counts(
            assessed
                .iter()
                .filter_map(|trial| trial.exception.as_ref().map(|value| value.kind.clone())),
        ),
        usage: HarnessUsage {
            trials_reporting_tokens: trials
                .iter()
                .filter(|trial| {
                    trial.usage.input_tokens.is_some() || trial.usage.output_tokens.is_some()
                })
                .count(),
            input_tokens: sum(trials.iter().map(|trial| trial.usage.input_tokens)),
            cache_tokens: sum(trials.iter().map(|trial| trial.usage.cache_tokens)),
            output_tokens: sum(trials.iter().map(|trial| trial.usage.output_tokens)),
            trials_reporting_cost: trials
                .iter()
                .filter(|trial| trial.usage.cost_usd.is_some())
                .count(),
            cost_usd: trials
                .iter()
                .filter_map(|trial| trial.usage.cost_usd)
                .reduce(|total, value| total + value),
            agent_seconds: distribution(
                trials
                    .iter()
                    .filter_map(|trial| trial.agent_seconds)
                    .collect(),
            ),
        },
        unassessed: trials
            .iter()
            .filter(|trial| !trial.status.assessed())
            .map(|trial| Unassessed {
                task: trial.task.clone(),
                trial: trial.trial.clone(),
                status: trial.status,
                reason: trial.note.clone().unwrap_or_else(|| {
                    trial
                        .failed_checks
                        .first()
                        .map(|failure| format!("{}: {}", failure.location, failure.reason))
                        .unwrap_or_default()
                }),
            })
            .collect(),
        warnings,
        trial_records: trials.clone(),
    }
}

fn list<T: std::fmt::Display>(values: &[T]) -> String {
    values
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(", ")
}

fn distinct<'a, T: Ord + 'a>(
    trials: &[&'a TrialRecord],
    key: impl Fn(&'a TrialRecord) -> Option<&'a T>,
) -> usize {
    trials
        .iter()
        .filter_map(|trial| key(trial))
        .collect::<BTreeSet<_>>()
        .len()
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PairedTask {
    pub task: String,
    pub passed_a: usize,
    pub assessed_a: usize,
    pub rate_a: f64,
    pub passed_b: usize,
    pub assessed_b: usize,
    pub rate_b: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PairedComparison {
    pub a: String,
    pub b: String,
    /// Tasks with at least one assessed trial in both arms. Only these are compared.
    pub paired_tasks: usize,
    /// Mean over paired tasks of each task's pass rate.
    pub mean_a: f64,
    pub mean_b: f64,
    /// `mean_b − mean_a`.
    pub mean_diff: f64,
    /// 95% paired bootstrap interval of `mean_diff` over tasks (10,000 resamples, fixed
    /// seed). Absent with fewer than two paired tasks.
    pub ci95: Option<(f64, f64)>,
    pub b_better: usize,
    pub a_better: usize,
    pub ties: usize,
    /// Two-sided exact sign test over tasks where the arms differ.
    pub sign_test_p: Option<f64>,
    /// Paired tasks whose pass rate differs between the arms, largest difference first.
    pub flips: Vec<PairedTask>,
    pub tasks: Vec<PairedTask>,
    pub only_in_a: Vec<String>,
    pub only_in_b: Vec<String>,
    /// Tasks both arms attempted where one arm has no assessed trial.
    pub attempted_but_unpaired: Vec<String>,
    /// Share of each arm's trials left out of its pass rate.
    pub excluded_share_a: f64,
    pub excluded_share_b: f64,
    /// Reasons the two arms' numbers may not be comparable.
    pub warnings: Vec<String>,
}

/// Two-sided exact binomial test of `wins` against `losses` at one half.
fn sign_test(wins: usize, losses: usize) -> Option<f64> {
    let n = wins + losses;
    if n == 0 {
        return None;
    }
    let mut term = (-(n as f64) * std::f64::consts::LN_2).exp();
    let mut tail = 0.0;
    for index in 0..=wins.min(losses) {
        tail += term;
        term *= (n - index) as f64 / (index + 1) as f64;
    }
    Some((2.0 * tail).min(1.0))
}

fn sample(names: &[String]) -> String {
    let shown: Vec<&str> = names.iter().take(5).map(String::as_str).collect();
    if names.len() > shown.len() {
        format!("{}, … ({} in all)", shown.join(", "), names.len())
    } else {
        shown.join(", ")
    }
}

pub fn compare(a: &Arm, b: &Arm) -> PairedComparison {
    let (tasks_a, tasks_b) = (by_task(&a.trials), by_task(&b.trials));
    let names = |tasks: &BTreeMap<&str, Vec<&TrialRecord>>,
                 other: &BTreeMap<&str, Vec<&TrialRecord>>| {
        tasks
            .keys()
            .filter(|task| !other.contains_key(*task))
            .map(|task| (*task).to_owned())
            .collect::<Vec<_>>()
    };
    let (only_in_a, only_in_b) = (names(&tasks_a, &tasks_b), names(&tasks_b, &tasks_a));
    let shared: Vec<&str> = tasks_a
        .keys()
        .filter(|task| tasks_b.contains_key(*task))
        .copied()
        .collect();

    let (mut tasks, mut attempted_but_unpaired) = (Vec::new(), Vec::new());
    for task in &shared {
        let (left, right) = (
            pass_rate(tasks_a[task].iter().copied()),
            pass_rate(tasks_b[task].iter().copied()),
        );
        match (left.rate, right.rate) {
            (Some(rate_a), Some(rate_b)) => tasks.push(PairedTask {
                task: (*task).to_owned(),
                passed_a: left.passed,
                assessed_a: left.assessed,
                rate_a,
                passed_b: right.passed,
                assessed_b: right.assessed,
                rate_b,
            }),
            _ => attempted_but_unpaired.push((*task).to_owned()),
        }
    }
    let diffs: Vec<f64> = tasks.iter().map(|task| task.rate_b - task.rate_a).collect();
    let mean = |values: &mut dyn Iterator<Item = f64>| {
        if tasks.is_empty() {
            0.0
        } else {
            values.sum::<f64>() / tasks.len() as f64
        }
    };
    let (mean_a, mean_b) = (
        mean(&mut tasks.iter().map(|task| task.rate_a)),
        mean(&mut tasks.iter().map(|task| task.rate_b)),
    );
    let (b_better, a_better) = (
        diffs.iter().filter(|diff| **diff > 0.0).count(),
        diffs.iter().filter(|diff| **diff < 0.0).count(),
    );
    let mut flips: Vec<PairedTask> = tasks
        .iter()
        .filter(|task| task.rate_a != task.rate_b)
        .cloned()
        .collect();
    flips.sort_by(|left, right| {
        (right.rate_b - right.rate_a)
            .abs()
            .total_cmp(&(left.rate_b - left.rate_a).abs())
            .then(left.task.cmp(&right.task))
    });

    let mut warnings = Vec::new();
    let versions = |arm: &Arm| {
        arm.trials
            .iter()
            .filter_map(|trial| trial.evaluator_version)
            .collect::<BTreeSet<_>>()
    };
    let (versions_a, versions_b) = (versions(a), versions(b));
    if versions_a != versions_b {
        warnings.push(format!(
            "Evaluator versions differ ({}: {}; {}: {}). The arms were graded by different evaluators, so their pass rates are not comparable.",
            a.label,
            list(&versions_a.into_iter().collect::<Vec<_>>()),
            b.label,
            list(&versions_b.into_iter().collect::<Vec<_>>()),
        ));
    }
    if !only_in_a.is_empty() || !only_in_b.is_empty() {
        let mut parts = Vec::new();
        if !only_in_a.is_empty() {
            parts.push(format!("only in {}: {}", a.label, sample(&only_in_a)));
        }
        if !only_in_b.is_empty() {
            parts.push(format!("only in {}: {}", b.label, sample(&only_in_b)));
        }
        warnings.push(format!(
            "Task sets differ ({}). Each arm's own pass rate covers a different task set; only the paired comparison on the {} shared task(s) is like for like.",
            parts.join("; "),
            shared.len()
        ));
    }
    let differing = |key: fn(&TrialRecord) -> Option<&String>| {
        shared
            .iter()
            .filter(|task| {
                let values = |trials: &[&TrialRecord]| {
                    trials
                        .iter()
                        .filter_map(|trial| key(trial))
                        .cloned()
                        .collect::<BTreeSet<String>>()
                };
                let (left, right) = (values(&tasks_a[*task]), values(&tasks_b[*task]));
                !left.is_empty() && !right.is_empty() && left != right
            })
            .map(|task| (*task).to_owned())
            .collect::<Vec<_>>()
    };
    let contracts = differing(|trial| trial.contract_sha256.as_ref());
    if !contracts.is_empty() {
        warnings.push(format!(
            "Contract hashes differ for {} shared task(s) ({}). Those tasks were graded against different expectations, so their results are not comparable.",
            contracts.len(),
            sample(&contracts)
        ));
    }
    let mut sources = differing(|trial| trial.dataset_sha256.as_ref());
    sources.extend(differing(|trial| trial.scenario_sha256.as_ref()));
    sources.sort();
    sources.dedup();
    if !sources.is_empty() {
        warnings.push(format!(
            "Scenario or dataset hashes differ for {} shared task(s) ({}). The world those tasks ran against was not the same.",
            sources.len(),
            sample(&sources)
        ));
    }
    let definitions: Vec<String> = differing(|trial| trial.task_checksum.as_ref())
        .into_iter()
        .filter(|task| !contracts.contains(task) && !sources.contains(task))
        .collect();
    if !definitions.is_empty() {
        warnings.push(format!(
            "Harbor task checksums differ for {} more shared task(s) ({}). Their instruction, environment, image tags or verifier files changed between the arms.",
            definitions.len(),
            sample(&definitions)
        ));
    }
    let attempts: Vec<String> = shared
        .iter()
        .filter(|task| tasks_a[*task].len() != tasks_b[*task].len())
        .map(|task| (*task).to_owned())
        .collect();
    if !attempts.is_empty() {
        warnings.push(format!(
            "Attempt counts differ for {} shared task(s) ({}). Per-task pass rates are still compared, with unequal precision.",
            attempts.len(),
            sample(&attempts)
        ));
    }
    if !attempted_but_unpaired.is_empty() {
        warnings.push(format!(
            "{} task(s) both arms attempted are not paired because one arm has no assessed trial for them ({}).",
            attempted_but_unpaired.len(),
            sample(&attempted_but_unpaired)
        ));
    }
    let excluded_share = |arm: &Arm| {
        let rate = pass_rate(&arm.trials);
        rate.excluded as f64 / arm.trials.len().max(1) as f64
    };
    let (excluded_share_a, excluded_share_b) = (excluded_share(a), excluded_share(b));
    if (excluded_share_a - excluded_share_b).abs() >= EXCLUSION_GAP {
        warnings.push(format!(
            "{} left {:.1}% of its trials out of its pass rate and {} left {:.1}%. If the excluded trials are not a random sample of tasks, the two pass rates over assessed trials are not like for like; rely on the paired tasks and read the excluded lists.",
            a.label,
            excluded_share_a * 100.0,
            b.label,
            excluded_share_b * 100.0
        ));
    }

    PairedComparison {
        a: a.label.clone(),
        b: b.label.clone(),
        paired_tasks: tasks.len(),
        mean_a,
        mean_b,
        mean_diff: mean_b - mean_a,
        ci95: (diffs.len() > 1).then(|| crate::bootstrap_ci(&diffs)),
        b_better,
        a_better,
        ties: tasks.len() - b_better - a_better,
        sign_test_p: sign_test(b_better, a_better),
        flips,
        tasks,
        only_in_a,
        only_in_b,
        attempted_but_unpaired,
        excluded_share_a,
        excluded_share_b,
        warnings,
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Report {
    pub schema_version: u32,
    pub format: &'static str,
    /// The evaluator this tool was built from; only trials it graded can be re-run.
    pub evaluator_version: u32,
    pub assurance_scope: &'static str,
    pub arms: Vec<ArmSummary>,
    /// One comparison per pair of arms, in the order given.
    pub comparisons: Vec<PairedComparison>,
}

pub const ASSURANCE_SCOPE: &str = "Unattested tier. For a `reproduced` trial this tool checked that the collected contract, world evidence, answer, task source and task context still hash to the values in verification.json, re-ran the assessment from them with its own evaluator, and found the saved assessment.json and the reward in result.json equal to that re-run. Taken on trust, because stock Harbor publishes no runner receipt or realized manifest: that the evidence and answer files are the ones this trial's world and agent produced, that the agent container was stopped before they were collected, the agent and model identity, the images, limits and timeouts actually used, the context-overflow flag passed to the verifier, and what the model was shown. Hashes bind bytes; they are not signatures. This is not the attested tier (--require-attested).";

pub fn report(arms: &[Arm]) -> Report {
    let mut comparisons = Vec::new();
    for (index, a) in arms.iter().enumerate() {
        for b in &arms[index + 1..] {
            comparisons.push(compare(a, b));
        }
    }
    Report {
        schema_version: 1,
        format: "arm_report_v1",
        evaluator_version: EVALUATOR_VERSION,
        assurance_scope: ASSURANCE_SCOPE,
        arms: arms.iter().map(summarize).collect(),
        comparisons,
    }
}

/// Loads each run directory as one arm, labelled `A`, `B`, … in order.
pub fn load_arms(directories: &[std::path::PathBuf]) -> anyhow::Result<Vec<Arm>> {
    directories
        .iter()
        .enumerate()
        .map(|(index, directory)| load_arm(&label(index), directory))
        .collect()
}

/// Why the trials of one task ended as they did.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TrialExplanation {
    pub arm: String,
    /// The trial directory, under the run directory as it was given.
    pub path: String,
    #[serde(flatten)]
    pub trial: TrialRecord,
    /// Every `*_ok` metric that is zero, including the benchmark's own checks.
    pub checks_not_ok: Vec<String>,
    /// The start of the collected answer file, when there is one.
    pub answer_excerpt: Option<String>,
}

/// Every trial whose task or trial name is `query`, or, when none is, contains it.
/// A scenario id (`code/github/x`) names the same task as `code__github__x`.
pub fn explain(arms: &[Arm], query: &str) -> anyhow::Result<Vec<TrialExplanation>> {
    let wanted = query.replace('/', "__");
    let wanted = wanted.trim_start_matches("tool-bench__");
    let select = |matches: &dyn Fn(&TrialRecord) -> bool| {
        arms.iter()
            .flat_map(|arm| arm.trials.iter().map(move |trial| (arm, trial)))
            .filter(|(_, trial)| matches(trial))
            .collect::<Vec<_>>()
    };
    let mut selected = select(&|trial| trial.task == wanted || trial.trial == query);
    if selected.is_empty() {
        selected = select(&|trial| trial.task.contains(wanted) || trial.trial.contains(query));
    }
    if selected.is_empty() {
        bail!("no trial of a task or trial named {query:?} in the given run directories");
    }
    let tasks: BTreeSet<&str> = selected
        .iter()
        .map(|(_, trial)| trial.task.as_str())
        .collect();
    if tasks.len() > 1 {
        bail!(
            "{query:?} matches {} tasks ({}); name one",
            tasks.len(),
            sample(
                &tasks
                    .iter()
                    .map(|task| (*task).to_owned())
                    .collect::<Vec<_>>()
            )
        );
    }
    Ok(selected
        .into_iter()
        .map(|(arm, trial)| TrialExplanation {
            arm: arm.label.clone(),
            path: Path::new(&arm.directory)
                .join(&trial.directory)
                .display()
                .to_string(),
            checks_not_ok: trial
                .metrics
                .iter()
                .filter(|(name, value)| name.ends_with("_ok") && **value == 0.0)
                .map(|(name, _)| name.clone())
                .collect(),
            answer_excerpt: std::fs::read(trial.path.join("artifacts/agent/answer.md"))
                .ok()
                .map(|bytes| {
                    let text = String::from_utf8_lossy(&bytes);
                    match text.char_indices().nth(600) {
                        Some((index, _)) => format!("{}…", &text[..index]),
                        None => text.into_owned(),
                    }
                }),
            trial: trial.clone(),
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::{distribution, label, sign_test, task_group, task_scope, wilson};

    #[test]
    fn wilson_interval_brackets_the_rate_and_stays_in_range() {
        let (low, high) = wilson(8, 10).unwrap();
        assert!((low - 0.4902).abs() < 1e-3 && (high - 0.9433).abs() < 1e-3);
        assert_eq!(wilson(0, 0), None);
        let (low, high) = wilson(5, 5).unwrap();
        assert!(low > 0.5 && low < 1.0 && high == 1.0);
        let (low, high) = wilson(0, 5).unwrap();
        assert!(low == 0.0 && high > 0.0 && high < 0.5);
    }

    #[test]
    fn sign_test_is_exact_and_two_sided() {
        assert_eq!(sign_test(0, 0), None);
        assert!((sign_test(5, 0).unwrap() - 0.0625).abs() < 1e-12);
        assert!((sign_test(0, 5).unwrap() - 0.0625).abs() < 1e-12);
        assert_eq!(sign_test(3, 3), Some(1.0));
        assert!((sign_test(9, 1).unwrap() - 0.021_484_375).abs() < 1e-12);
    }

    #[test]
    fn task_names_give_group_and_scope() {
        assert_eq!(task_group("ops__pagerduty__ack"), "ops");
        assert_eq!(task_scope("ops__pagerduty__ack"), "single-service");
        assert_eq!(task_scope("business__multi__refund"), "multi-service");
        assert_eq!(task_scope("code__github__multi-repo"), "single-service");
        assert_eq!(label(0), "A");
        assert_eq!(label(25), "Z");
        assert_eq!(label(26), "arm27");
    }

    #[test]
    fn median_of_an_even_count_is_the_midpoint() {
        let stats = distribution(vec![4.0, 1.0, 3.0, 2.0]).unwrap();
        assert_eq!((stats.n, stats.mean, stats.median), (4, 2.5, 2.5));
        assert_eq!(distribution(vec![7.0, 1.0, 3.0]).unwrap().median, 3.0);
        assert_eq!(distribution(Vec::new()), None);
    }
}
