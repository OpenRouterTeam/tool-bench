//! One Harbor trial directory: what the host recorded, what the verifier saved, and how
//! much of that this tool could check again.

use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use mock_core::{
    evidence::WorldEvidence,
    grading::{EVALUATOR_VERSION, Expect, Grade},
    provenance::{DatasetSource, TaskContext, canonical_sha256},
    runner::sha256,
};
use serde::Serialize;
use serde_json::{Value, json};

use crate::{
    Outcome,
    explain::{FailedCheck, failed_checks, saved_failed_checks},
};

/// What a trial contributes to an arm. Only the first four count in a pass rate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Pass,
    Fail,
    ContextOverflow,
    IntegrityViolation,
    /// The evidence cannot decide; needs adjudication. Not a model failure.
    Unresolved,
    /// The verifier wrote `reward.invalid.json`: invalid contract or environment error.
    WithheldReward,
    /// The verifier published a zero reward with an invalid-contract outcome.
    InvalidContract,
    /// The verifier published a zero reward with an environment-error outcome.
    EnvironmentError,
    /// The verifier started but the host recorded no reward and none was withheld.
    VerifierError,
    /// The agent or environment raised and the host recorded no reward.
    HarnessException,
    /// The trial directory has no usable `result.json`.
    Incomplete,
    /// A reward was recorded, but it does not agree with the collected artifacts.
    ReverificationFailed,
}

impl Status {
    pub const ALL: [Self; 12] = [
        Self::Pass,
        Self::Fail,
        Self::ContextOverflow,
        Self::IntegrityViolation,
        Self::Unresolved,
        Self::WithheldReward,
        Self::InvalidContract,
        Self::EnvironmentError,
        Self::VerifierError,
        Self::HarnessException,
        Self::Incomplete,
        Self::ReverificationFailed,
    ];

    pub fn key(self) -> &'static str {
        match self {
            Self::Pass => "pass",
            Self::Fail => "fail",
            Self::ContextOverflow => "context_overflow",
            Self::IntegrityViolation => "integrity_violation",
            Self::Unresolved => "unresolved",
            Self::WithheldReward => "withheld_reward",
            Self::InvalidContract => "invalid_contract",
            Self::EnvironmentError => "environment_error",
            Self::VerifierError => "verifier_error",
            Self::HarnessException => "harness_exception",
            Self::Incomplete => "incomplete",
            Self::ReverificationFailed => "reverification_failed",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Pass => "pass",
            Self::Fail => "fail",
            Self::ContextOverflow => "context overflow",
            Self::IntegrityViolation => "integrity violation",
            Self::Unresolved => "unresolved (needs adjudication)",
            Self::WithheldReward => "withheld reward (verifier could not assess)",
            Self::InvalidContract => "invalid contract (zero reward published)",
            Self::EnvironmentError => "environment error (zero reward published)",
            Self::VerifierError => "verifier error (no reward file)",
            Self::HarnessException => "agent or environment exception",
            Self::Incomplete => "incomplete (no result.json)",
            Self::ReverificationFailed => "failed re-verification",
        }
    }

    /// Whether the trial counts in the pass rate's denominator.
    pub fn assessed(self) -> bool {
        matches!(
            self,
            Self::Pass | Self::Fail | Self::ContextOverflow | Self::IntegrityViolation
        )
    }
}

/// How far this tool checked a trial's recorded result.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Assurance {
    /// Runner receipt, realized manifest and source bindings verified, and the assessment
    /// reproduced: the tier `--require-attested` demands.
    Attested,
    /// No runner receipt. The saved assessment reproduces from the collected world
    /// evidence, contract and answer with this evaluator, and the host reward matches it.
    Reproduced,
    /// Taken as the verifier recorded it; this tool could not run the assessment again.
    Recorded,
    /// The recorded result disagrees with the collected artifacts.
    Failed,
    /// There is no assessment to check.
    None,
}

impl Assurance {
    pub const ALL: [Self; 5] = [
        Self::Attested,
        Self::Reproduced,
        Self::Recorded,
        Self::Failed,
        Self::None,
    ];

    pub fn key(self) -> &'static str {
        match self {
            Self::Attested => "attested",
            Self::Reproduced => "reproduced",
            Self::Recorded => "recorded",
            Self::Failed => "failed",
            Self::None => "none",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Attested => "attested: runner receipt verified and assessment reproduced",
            Self::Reproduced => {
                "reproduced: no runner receipt; assessment re-run from collected artifacts"
            }
            Self::Recorded => "recorded only: verifier output taken as written, not re-run",
            Self::Failed => "failed: recorded result disagrees with collected artifacts",
            Self::None => "no assessment to check",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct HostException {
    pub kind: String,
    pub message: String,
}

/// Token and cost figures the harness wrote to `result.json`. The verifier never sees
/// them, so they are reported as recorded and are absent when the harness wrote none.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Usage {
    /// Input tokens including cached ones.
    pub input_tokens: Option<u64>,
    pub cache_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub cost_usd: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TrialRecord {
    /// Task directory name, e.g. `code__github__file-login-bug`.
    pub task: String,
    pub trial: String,
    /// Trial directory relative to the run directory.
    pub directory: String,
    pub status: Status,
    pub assurance: Assurance,
    /// Why there is no assessment, or why the assurance is below `reproduced`.
    pub note: Option<String>,
    /// The published reward. Absent when none was published, including when withheld.
    pub reward: Option<f64>,
    /// The reward metrics: from the re-run assessment when there is one, otherwise as
    /// the verifier or host recorded them.
    pub metrics: BTreeMap<String, f64>,
    pub failed_checks: Vec<FailedCheck>,
    /// An exception the host recorded. An agent timeout is recorded and still graded.
    pub exception: Option<HostException>,
    pub agent: String,
    pub agent_version: Option<String>,
    pub model: Option<String>,
    pub evaluator_version: Option<u32>,
    /// Canonical hash of the contract the verifier graded against.
    pub contract_sha256: Option<String>,
    /// Scenario and dataset identity the world reported loading.
    pub scenario_sha256: Option<String>,
    pub dataset_sha256: Option<String>,
    /// Harbor's checksum of the task directory the trial ran.
    pub task_checksum: Option<String>,
    pub answer_collected: bool,
    pub usage: Usage,
    /// Wall-clock seconds of the agent phase, from the host timestamps.
    pub agent_seconds: Option<f64>,
    #[serde(skip)]
    pub(crate) path: PathBuf,
}

impl TrialRecord {
    pub fn metric(&self, name: &str) -> Option<f64> {
        self.metrics.get(name).copied()
    }
}

const RESERVED: [&str; 6] = [
    "agent",
    "user-agent",
    "artifacts",
    "verifier",
    "main-snapshot",
    "steps",
];

fn json_file(path: &Path) -> Option<Value> {
    if path.is_symlink() {
        return None;
    }
    serde_json::from_slice(&fs::read(path).ok()?).ok()
}

/// A Harbor trial writes `config.json` (with `trial_name`) when it starts and
/// `result.json` (with `task_name`) when it ends. A job directory has neither key.
fn looks_like_trial(directory: &Path) -> bool {
    json_file(&directory.join("result.json")).is_some_and(|value| value["task_name"].is_string())
        || json_file(&directory.join("config.json"))
            .is_some_and(|value| value["trial_name"].is_string() && value["task"].is_object())
}

/// Finds trial directories at any depth. A directory that looks like a trial and has a
/// `verifier/` directory is one; without `verifier/` it is one only when nothing below
/// it is. Symbolic links are never followed and are returned in `skipped`.
pub(crate) fn discover(
    directory: &Path,
    found: &mut Vec<PathBuf>,
    skipped: &mut Vec<String>,
) -> anyhow::Result<()> {
    if directory.is_symlink() {
        skipped.push(directory.display().to_string());
        return Ok(());
    }
    let trial = looks_like_trial(directory);
    if trial && directory.join("verifier").is_dir() {
        found.push(directory.to_owned());
        return Ok(());
    }
    let before = found.len();
    let mut entries: Vec<PathBuf> = fs::read_dir(directory)
        .map_err(|error| anyhow::anyhow!("reading {}: {error}", directory.display()))?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .collect();
    entries.sort();
    for path in entries {
        if path.is_symlink() {
            skipped.push(path.display().to_string());
        } else if path.is_dir()
            && !path
                .file_name()
                .is_some_and(|name| RESERVED.iter().any(|reserved| name == *reserved))
        {
            discover(&path, found, skipped)?;
        }
    }
    if trial && found.len() == before {
        found.push(directory.to_owned());
    }
    Ok(())
}

fn task_name(name: &str) -> String {
    name.rsplit('/')
        .next()
        .unwrap_or(name)
        .trim_start_matches("tool-bench__")
        .to_owned()
}

/// Seconds since the Unix epoch for an RFC 3339 timestamp as Harbor writes them.
fn timestamp(value: &Value) -> Option<f64> {
    let text = value.as_str()?;
    let (date, clock) = text.split_once('T')?;
    let mut date = date.split('-').map(str::parse::<i64>);
    let (year, month, day) = (date.next()?.ok()?, date.next()?.ok()?, date.next()?.ok()?);
    let (clock, offset) = match clock.strip_suffix('Z') {
        Some(clock) => (clock, 0.0),
        None => match clock.rfind(['+', '-']) {
            Some(index) => {
                let (hours, minutes) = clock[index + 1..].split_once(':')?;
                let seconds =
                    hours.parse::<f64>().ok()? * 3600.0 + minutes.parse::<f64>().ok()? * 60.0;
                let sign = if clock[index..].starts_with('-') {
                    -1.0
                } else {
                    1.0
                };
                (&clock[..index], sign * seconds)
            }
            None => (clock, 0.0),
        },
    };
    let mut clock = clock.split(':');
    let (hour, minute, second) = (
        clock.next()?.parse::<f64>().ok()?,
        clock.next()?.parse::<f64>().ok()?,
        clock.next()?.parse::<f64>().ok()?,
    );
    // Days from the civil date (Howard Hinnant's algorithm).
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let year_of_era = year.rem_euclid(400);
    let day_of_year = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    let days = era * 146_097 + day_of_era - 719_468;
    Some(days as f64 * 86_400.0 + hour * 3600.0 + minute * 60.0 + second - offset)
}

fn numbers(value: &Value) -> Option<BTreeMap<String, f64>> {
    let object = value.as_object().filter(|object| !object.is_empty())?;
    object
        .iter()
        .map(|(name, value)| value.as_f64().map(|value| (name.clone(), value)))
        .collect()
}

fn grade_metrics(grade: &Grade) -> BTreeMap<String, f64> {
    grade
        .metrics()
        .into_iter()
        .map(|(name, value)| (name.to_owned(), value))
        .collect()
}

/// Why an assessment was not reproduced.
enum Refusal {
    /// Nothing contradicts the record, but this tool cannot run it again.
    Recorded(String),
    /// The record contradicts the collected artifacts.
    Failed(String),
}

struct Inputs {
    contract: Vec<u8>,
    evidence: Vec<u8>,
    answer: Option<Vec<u8>>,
    source: Option<Vec<u8>>,
    context: Option<Vec<u8>>,
}

/// Reads one input the verification record binds by hash.
fn bound(directory: &Path, relative: &str, expected: &Value) -> Result<Vec<u8>, Refusal> {
    let path = directory.join(relative);
    if path.is_symlink() {
        return Err(Refusal::Failed(format!("{relative} is a symbolic link")));
    }
    let Some(expected) = expected.as_str() else {
        return Err(Refusal::Recorded(format!(
            "the verifier recorded no hash for {relative}"
        )));
    };
    let bytes = fs::read(&path).map_err(|_| {
        Refusal::Recorded(format!(
            "{relative} was not collected, so the assessment cannot be re-run"
        ))
    })?;
    if sha256(&bytes) != expected {
        return Err(Refusal::Failed(format!(
            "{relative} differs from the bytes the verifier recorded"
        )));
    }
    Ok(bytes)
}

fn inputs(directory: &Path, verification: &Value) -> Result<Inputs, Refusal> {
    Ok(Inputs {
        contract: bound(
            directory,
            "verifier/contract.json",
            &verification["contract_sha256"],
        )?,
        evidence: bound(
            directory,
            "artifacts/world/evidence.json",
            &verification["evidence_sha256"],
        )?,
        answer: if verification["answer_sha256"].is_null() {
            None
        } else {
            Some(bound(
                directory,
                "artifacts/agent/answer.md",
                &verification["answer_sha256"],
            )?)
        },
        source: if verification["task_source_checked"] == true {
            Some(bound(
                directory,
                "verifier/task-source.json",
                &verification["task_source_sha256"],
            )?)
        } else {
            None
        },
        context: if verification["task_context_sha256"].is_null() {
            None
        } else {
            Some(bound(
                directory,
                "verifier/task-context.json",
                &verification["task_context_sha256"],
            )?)
        },
    })
}

/// Runs the assessment again the way `bench-verify --allow-unattested-runner` does when
/// the runner published no receipt: from the sealed world evidence, the contract and the
/// collected answer, with the same source, materialization and scenario checks in the
/// same order. Receipt, manifest and inference evidence are not consulted there either.
fn reproduce(directory: &Path, verification: &Value, inputs: &Inputs) -> Result<Grade, Refusal> {
    let scenario = verification["scenario"]
        .as_str()
        .ok_or_else(|| Refusal::Failed("the verification record names no scenario".into()))?;
    let contract: Expect = serde_json::from_slice(&inputs.contract).map_err(|error| {
        Refusal::Recorded(format!(
            "the contract does not parse with this build: {error}"
        ))
    })?;
    let evidence: WorldEvidence = serde_json::from_slice(&inputs.evidence).map_err(|error| {
        Refusal::Recorded(format!(
            "the world evidence does not parse with this build: {error}"
        ))
    })?;
    let answer = inputs
        .answer
        .as_deref()
        .map(std::str::from_utf8)
        .transpose()
        .map_err(|_| Refusal::Recorded("the collected answer is not UTF-8".into()))?;
    if verification["contract_semantic_sha256"] != canonical_sha256(&contract)
        || verification["world_source"] != json!(evidence.source)
        || verification["initial_state_sha256"] != evidence.initial_state_sha256
        || verification["evidence_schema_version"] != evidence.schema_version
    {
        return Err(Refusal::Failed(
            "the verification record's contract, source or version fields do not describe the collected inputs".into(),
        ));
    }
    let mut assessment =
        evidence.assess(&contract, answer, verification["context_overflow"] == true);
    if let Some(bytes) = &inputs.source {
        let check = (|| {
            let expected: DatasetSource =
                serde_json::from_slice(bytes).map_err(|error| error.to_string())?;
            expected.validate(scenario)?;
            if expected != evidence.source {
                return Err(
                    "world scenario/data/contract sources do not match the materialized task"
                        .to_owned(),
                );
            }
            Ok(())
        })();
        if let Err(reason) = check {
            assessment.invalidate_environment("evidence/task-source", reason);
        }
    }
    let materialization = (|| {
        let context: TaskContext = match &inputs.context {
            Some(bytes) => serde_json::from_slice(bytes).map_err(|error| error.to_string())?,
            None if evidence.source.materialization.is_none() => return Ok(()),
            None => return Err("materialized evidence requires its private task context".into()),
        };
        let bundle = mock_core::materialization::bundle_from_directory(
            &directory.join("verifier"),
            &context.materialization_files,
        )?;
        mock_core::materialization::validate_bundle(
            &evidence.source,
            &context.materialization_files,
            &bundle,
        )
    })();
    if let Err(reason) = materialization {
        assessment.invalidate_environment("evidence/materialization", reason);
    }
    if evidence.scenario != scenario {
        assessment.invalidate_environment(
            "evidence/scenario",
            "world evidence does not belong to the requested task",
        );
    }
    Ok(assessment)
}

struct Checked {
    assurance: Assurance,
    note: Option<String>,
    metrics: BTreeMap<String, f64>,
    failed_checks: Vec<FailedCheck>,
}

/// Checks a recorded assessment as far as this build can. `recorded` is the reward the
/// host (or, for a withheld reward, the verifier) wrote; `reward_file` is the verifier
/// file that must carry the same numbers.
fn check(
    directory: &Path,
    result: &Value,
    task: &str,
    recorded: &BTreeMap<String, f64>,
    reward_file: &str,
) -> Checked {
    let verifier = directory.join("verifier");
    let saved = json_file(&verifier.join("assessment.json"));
    let saved_checks = saved.as_ref().map(saved_failed_checks).unwrap_or_default();
    let fallback = |assurance, note: String| Checked {
        assurance,
        note: Some(note),
        metrics: recorded.clone(),
        failed_checks: saved_checks.clone(),
    };
    let Some(verification) = json_file(&verifier.join("verification.json")) else {
        return fallback(
            Assurance::Recorded,
            "no verifier/verification.json was collected; the recorded reward is all there is"
                .into(),
        );
    };
    if verification["schema_version"] != 3 {
        return fallback(
            Assurance::Recorded,
            format!(
                "verification record schema {} is not one this build reads",
                verification["schema_version"]
            ),
        );
    }
    // Harbor reads the host reward from this file, so the two can only differ if one was
    // edited afterwards. A trimmed download may lack it; the re-run below still binds
    // the host reward to the evidence.
    if json_file(&verifier.join(reward_file))
        .as_ref()
        .is_some_and(|written| numbers(written).as_ref() != Some(recorded))
    {
        return fallback(
            Assurance::Failed,
            format!("the recorded reward differs from verifier/{reward_file}"),
        );
    }
    if verification["scenario"]
        .as_str()
        .is_none_or(|scenario| scenario.replace('/', "__") != task)
    {
        return fallback(
            Assurance::Failed,
            "the host task identity does not match the scenario the verifier graded".into(),
        );
    }
    let attested =
        verification["runner_attested"] == true || verification["runner_admission_checked"] == true;
    let inputs = match inputs(directory, &verification) {
        Ok(inputs) => inputs,
        Err(Refusal::Recorded(note)) => return fallback(Assurance::Recorded, note),
        Err(Refusal::Failed(note)) => return fallback(Assurance::Failed, note),
    };
    if verification["evaluator_version"] != EVALUATOR_VERSION {
        return fallback(
            Assurance::Recorded,
            format!(
                "graded by evaluator {}; this build is evaluator {EVALUATOR_VERSION} and cannot re-run it (build bench-report from the matching revision to reproduce)",
                verification["evaluator_version"]
            ),
        );
    }
    let (assurance, grade) = if attested {
        match crate::provenance::load_verified(directory, result) {
            Ok(Some(verified)) => (Assurance::Attested, verified.assessment),
            Ok(None) => {
                return fallback(
                    Assurance::Recorded,
                    "no provenance-checked assessment was collected".into(),
                );
            }
            Err(error) => return fallback(Assurance::Failed, format!("{error:#}")),
        }
    } else {
        // Evidence that parses but breaks an evaluator invariant must cost one trial, not
        // the whole report.
        let rerun = std::panic::catch_unwind(|| reproduce(directory, &verification, &inputs))
            .unwrap_or_else(|_| {
                Err(Refusal::Failed(
                    "re-running the assessment panicked on the collected inputs".into(),
                ))
            });
        let grade = match rerun {
            Ok(grade) => grade,
            Err(Refusal::Recorded(note)) => return fallback(Assurance::Recorded, note),
            Err(Refusal::Failed(note)) => return fallback(Assurance::Failed, note),
        };
        let Some(saved) = &saved else {
            return fallback(
                Assurance::Failed,
                "verifier/assessment.json is missing or unreadable".into(),
            );
        };
        if canonical_sha256(&grade) != canonical_sha256(saved) {
            return fallback(
                Assurance::Failed,
                format!(
                    "the saved assessment does not reproduce from the collected evidence, contract and answer (saved outcome {}, re-run outcome {})",
                    saved["outcome"],
                    json!(grade.outcome)
                ),
            );
        }
        (Assurance::Reproduced, grade)
    };
    let metrics = grade_metrics(&grade);
    if &metrics != recorded {
        return fallback(
            Assurance::Failed,
            "the recorded reward metrics differ from the re-run assessment".into(),
        );
    }
    Checked {
        assurance,
        note: None,
        metrics,
        failed_checks: failed_checks(&grade),
    }
}

const VERIFIER_EXCEPTIONS: [&str; 6] = [
    "RewardFileNotFoundError",
    "RewardFileEmptyError",
    "VerifierOutputParseError",
    "VerifierTimeoutError",
    "AddTestsDirError",
    "DownloadVerifierDirError",
];

fn first_line(text: &str) -> String {
    let line = text.lines().next().unwrap_or_default();
    match line.char_indices().nth(240) {
        Some((index, _)) => format!("{}…", &line[..index]),
        None => line.to_owned(),
    }
}

fn withheld_reason(checks: &[FailedCheck], outcome: &Value) -> String {
    let outcome = outcome.as_str().unwrap_or("unknown outcome");
    match checks.first() {
        Some(failure) => format!(
            "{outcome} at {}: {}",
            failure.location,
            first_line(&failure.reason)
        ),
        None => outcome.to_owned(),
    }
}

/// Loads one trial directory. It never fails: a trial that cannot be read is reported as
/// one, with the reason, rather than dropped from its arm.
pub(crate) fn load(root: &Path, directory: &Path) -> TrialRecord {
    let result =
        json_file(&directory.join("result.json")).filter(|value| value["task_name"].is_string());
    let config = json_file(&directory.join("config.json")).unwrap_or(Value::Null);
    let lock = json_file(&directory.join("lock.json")).unwrap_or(Value::Null);
    let verifier = directory.join("verifier");
    let verification = json_file(&verifier.join("verification.json")).unwrap_or(Value::Null);
    let saved = json_file(&verifier.join("assessment.json")).unwrap_or(Value::Null);
    let empty = Value::Null;
    let host = result.as_ref().unwrap_or(&empty);

    let task = host["task_name"]
        .as_str()
        .or_else(|| config["task"]["name"].as_str())
        .or_else(|| config["task"]["path"].as_str())
        .or_else(|| lock["task"]["name"].as_str())
        .map(task_name)
        .unwrap_or_else(|| "unknown".into());
    let trial = host["trial_name"]
        .as_str()
        .or_else(|| config["trial_name"].as_str())
        .map(str::to_owned)
        .unwrap_or_else(|| {
            directory
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default()
        });
    let exception = host["exception_info"].as_object().map(|_| HostException {
        kind: host["exception_info"]["exception_type"]
            .as_str()
            .unwrap_or("unknown")
            .to_owned(),
        message: first_line(
            host["exception_info"]["exception_message"]
                .as_str()
                .unwrap_or_default(),
        ),
    });
    let recorded = numbers(&host["verifier_result"]["rewards"]);
    let withheld = json_file(&verifier.join("reward.invalid.json"))
        .as_ref()
        .and_then(numbers);

    let (status, checked) = match (&result, recorded, withheld) {
        (None, _, _) => (
            Status::Incomplete,
            Checked {
                assurance: Assurance::None,
                note: Some(
                    if directory.join("result.json").exists() {
                        "result.json is not a readable Harbor trial result"
                    } else {
                        "the trial did not finish or its result was not collected"
                    }
                    .into(),
                ),
                metrics: BTreeMap::new(),
                failed_checks: Vec::new(),
            },
        ),
        (Some(result), Some(recorded), _) => {
            let mut checked = check(directory, result, &task, &recorded, "reward.json");
            let outcome = Outcome::from_code(checked.metrics.get("outcome_code").copied());
            let reward = checked.metrics.get("reward").copied();
            let status = if checked.assurance == Assurance::Failed {
                Status::ReverificationFailed
            } else if reward != Some(if outcome == Outcome::Pass { 1.0 } else { 0.0 }) {
                checked.assurance = Assurance::Failed;
                checked.note = Some("the recorded reward and outcome code disagree".into());
                Status::ReverificationFailed
            } else {
                match outcome {
                    Outcome::Pass => Status::Pass,
                    Outcome::Fail => Status::Fail,
                    Outcome::ContextOverflow => Status::ContextOverflow,
                    Outcome::IntegrityViolation => Status::IntegrityViolation,
                    Outcome::Unresolved => Status::Unresolved,
                    Outcome::InvalidContract => Status::InvalidContract,
                    Outcome::EnvironmentError => Status::EnvironmentError,
                    Outcome::Error => {
                        checked.assurance = Assurance::Failed;
                        checked.note =
                            Some("the recorded reward has no recognised outcome code".into());
                        Status::ReverificationFailed
                    }
                }
            };
            if matches!(status, Status::InvalidContract | Status::EnvironmentError)
                && checked.note.is_none()
            {
                checked.note = Some(withheld_reason(&checked.failed_checks, &saved["outcome"]));
            }
            (status, checked)
        }
        (Some(result), None, Some(withheld)) => {
            let mut checked = check(directory, result, &task, &withheld, "reward.invalid.json");
            let reason = withheld_reason(&checked.failed_checks, &saved["outcome"]);
            checked.note = Some(match checked.note.take() {
                Some(note) => format!("{reason} ({note})"),
                None => reason,
            });
            (Status::WithheldReward, checked)
        }
        (Some(_), None, None) => {
            let verifier_ran = host["verifier"]["started_at"].is_string()
                || exception.as_ref().is_some_and(|exception| {
                    VERIFIER_EXCEPTIONS.contains(&exception.kind.as_str())
                });
            let note = match &exception {
                Some(exception) => format!("{}: {}", exception.kind, exception.message),
                None => "the host recorded neither a reward nor an exception".into(),
            };
            (
                if verifier_ran {
                    Status::VerifierError
                } else {
                    Status::HarnessException
                },
                Checked {
                    assurance: Assurance::None,
                    note: Some(note),
                    metrics: BTreeMap::new(),
                    failed_checks: saved_failed_checks(&saved),
                },
            )
        }
    };

    let text = |value: &Value| value.as_str().map(str::to_owned);
    let model = match (
        host["agent_info"]["model_info"]["provider"].as_str(),
        host["agent_info"]["model_info"]["name"].as_str(),
    ) {
        (Some(provider), Some(name)) => Some(format!("{provider}/{name}")),
        (None, Some(name)) => Some(name.to_owned()),
        _ => text(&host["config"]["agent"]["model_name"])
            .or_else(|| text(&lock["agent"]["model_name"])),
    };
    let tokens = |name: &str| host["agent_result"][name].as_u64();
    TrialRecord {
        directory: directory
            .strip_prefix(root)
            .unwrap_or(directory)
            .display()
            .to_string(),
        // A withheld reward was never published; its zero is not a score.
        reward: checked
            .metrics
            .get("reward")
            .copied()
            .filter(|_| status != Status::WithheldReward),
        agent: text(&host["agent_info"]["name"])
            .or_else(|| text(&lock["agent"]["name"]))
            .unwrap_or_else(|| "unknown".into()),
        agent_version: text(&host["agent_info"]["version"]),
        model,
        evaluator_version: verification["evaluator_version"]
            .as_u64()
            .or_else(|| {
                checked
                    .metrics
                    .get("evaluator_version")
                    .map(|value| *value as u64)
            })
            .and_then(|value| u32::try_from(value).ok()),
        contract_sha256: text(&verification["contract_semantic_sha256"]),
        scenario_sha256: text(&verification["world_source"]["scenario_sha256"]),
        dataset_sha256: text(&verification["world_source"]["dataset_sha256"]),
        task_checksum: text(&host["task_checksum"]),
        answer_collected: directory.join("artifacts/agent/answer.md").is_file(),
        usage: Usage {
            input_tokens: tokens("n_input_tokens"),
            cache_tokens: tokens("n_cache_tokens"),
            output_tokens: tokens("n_output_tokens"),
            cost_usd: host["agent_result"]["cost_usd"].as_f64(),
        },
        agent_seconds: timestamp(&host["agent_execution"]["finished_at"])
            .zip(timestamp(&host["agent_execution"]["started_at"]))
            .map(|(finished, started)| finished - started)
            .filter(|seconds| *seconds >= 0.0),
        task,
        trial,
        status,
        assurance: checked.assurance,
        note: checked.note,
        metrics: checked.metrics,
        failed_checks: checked.failed_checks,
        exception,
        path: directory.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::timestamp;

    #[test]
    fn harbor_timestamps_give_phase_durations() {
        let started = timestamp(&json!("2026-11-10T19:05:53.777052Z")).unwrap();
        let finished = timestamp(&json!("2026-11-10T19:05:55.533088Z")).unwrap();
        assert!((finished - started - 1.756036).abs() < 1e-6);
        assert_eq!(timestamp(&json!("1970-01-01T00:00:00Z")), Some(0.0));
        assert_eq!(
            timestamp(&json!("2000-03-01T00:00:00+00:00")),
            Some(951_868_800.0)
        );
        assert_eq!(
            timestamp(&json!("2026-11-10T13:05:46.148111-06:00")),
            timestamp(&json!("2026-11-10T19:05:46.148111Z"))
        );
        assert_eq!(timestamp(&json!(null)), None);
        assert_eq!(timestamp(&json!("yesterday")), None);
    }
}
