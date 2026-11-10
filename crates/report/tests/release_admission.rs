//! Frozen release admission in the report's comparison path (plan items 15, 19).

mod support;

use std::{fs, path::Path};

use mock_core::{
    family::DifficultyFamily,
    release::{
        Budgets, COMPARISON_SCHEMA, ComparisonDesign, Denominator, ExternalBundleSeal, OrderPolicy,
        RELEASE_SCHEMA, Release, ReleaseTask, ReplacementPolicy, SplitAssignment,
    },
};
use report::{Outcome, Trial, load_trials, release};
use serde_json::Value;

const TASKS: [(&str, DifficultyFamily); 3] = [
    ("code__github__x", DifficultyFamily::EntityAmbiguity),
    ("code__github__z", DifficultyFamily::EntityAmbiguity),
    ("ops__pagerduty__y", DifficultyFamily::StaleState),
];

fn trial(task: &str, outcome: Outcome) -> Trial {
    Trial {
        task: task.into(),
        reward: f64::from(u8::from(outcome == Outcome::Pass)),
        outcome,
        provenance: matches!(
            outcome,
            Outcome::Pass | Outcome::Fail | Outcome::EnvironmentError
        )
        .then(|| support::provenance(task)),
        review: None,
    }
}

fn frozen(repeats: u32, replacement: ReplacementPolicy) -> Release {
    let digest = format!("sha256:{}", "d".repeat(64));
    let release = Release {
        schema_version: RELEASE_SCHEMA,
        release_id: "fixture-release".into(),
        frozen_at: "2026-11-09T00:00:00Z".into(),
        evaluator_version: mock_core::grading::EVALUATOR_VERSION,
        family_schema_version: mock_core::family::FAMILY_SCHEMA,
        comparison: ComparisonDesign {
            schema_version: COMPARISON_SCHEMA,
            repeats_per_task: repeats,
            budgets: Budgets {
                agent_timeout_sec_max: Some(900),
                inference_tokens_max: None,
            },
            order_policy: OrderPolicy::InterleavedArms,
            replacement_policy: replacement,
            declared_treatments: Vec::new(),
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
            content_sha256: digest.clone(),
            admitted_by: "synthetic fixture".into(),
            synthetic_fixture: true,
        }],
        tasks: TASKS
            .iter()
            .map(|(task, family)| {
                let source = support::provenance(task).source;
                ReleaseTask {
                    task_id: source.scenario,
                    split: SplitAssignment::PublicDevelopment,
                    family: *family,
                    family_sha256: digest.clone(),
                    scenario_sha256: source.scenario_sha256,
                    dataset_sha256: source.dataset_sha256,
                    contract_sha256: source.contract_sha256,
                }
            })
            .collect(),
    };
    release.validate().unwrap();
    release
}

fn arm(outcomes: [[Outcome; 2]; 3]) -> Vec<Trial> {
    TASKS
        .iter()
        .zip(outcomes)
        .flat_map(|((task, _), outcomes)| outcomes.map(|outcome| trial(task, outcome)))
        .collect()
}

const P: Outcome = Outcome::Pass;
const F: Outcome = Outcome::Fail;

fn rejected(a: &[Trial], b: &[Trial], release: &Release) -> String {
    release::compare(a, b, release, false)
        .unwrap_err()
        .to_string()
}

#[test]
fn admitted_comparisons_state_every_denominator_and_separate_uncertainties() {
    let release = frozen(2, ReplacementPolicy::NoReplacement);
    let a = arm([[F, F], [P, F], [P, P]]);
    let b = arm([[P, P], [P, P], [P, F]]);
    let report = release::compare(&a, &b, &release, false).unwrap();
    assert_eq!(report.release_admission.level, release::FROZEN);
    let admitted = report.release_admission.frozen.as_ref().unwrap();
    assert_eq!(admitted.release_manifest_sha256, release.sha256());
    assert_eq!(admitted.split, SplitAssignment::PublicDevelopment);
    assert_eq!(
        admitted.held_out,
        "synthetic_fixture_seal_only_no_held_out_claim"
    );
    assert!(!admitted.order_policy_verified);
    let d = &admitted.denominators;
    assert_eq!((d.tasks, d.families), (3, 2));
    assert_eq!((d.counted_attempts_a, d.counted_attempts_b), (6, 6));
    assert_eq!((d.replaced_attempts_a, d.replaced_attempts_b), (0, 0));
    assert_eq!(
        (d.attempt_pass_rate_a, d.attempt_pass_rate_b),
        (0.5, 5.0 / 6.0)
    );
    assert!(d.retry_semantics.contains("every attempt counts"));
    // Family rows keep related tasks together.
    assert_eq!(admitted.families.len(), 2);
    let entity = &admitted.families[0];
    assert_eq!(entity.family, DifficultyFamily::EntityAmbiguity);
    assert_eq!((entity.tasks, entity.counted_attempts_a), (2, 4));
    assert_eq!((entity.task_mean_a, entity.task_mean_b), (0.25, 1.0));
    // Three separately labeled intervals around the same point estimate.
    let u = &admitted.uncertainty;
    assert_eq!(u.task_resampling_ci95, report.comparison.ci95);
    let cluster = u.family_cluster_ci95.unwrap();
    for (low, high) in [u.repeated_attempts_ci95, u.task_resampling_ci95, cluster] {
        assert!(low <= report.comparison.mean_diff && report.comparison.mean_diff <= high);
    }
    assert_ne!(u.repeated_attempts_ci95, u.task_resampling_ci95);
    assert!(u.generalization.contains("not unseen held-out families"));
    assert_eq!(report.comparison.paired_tasks, 3);
    let json = serde_json::to_value(&report).unwrap();
    assert_eq!(
        json["release_admission"]["frozen"]["release_id"],
        "fixture-release"
    );
    assert_eq!(json["paired_tasks"], 3, "the comparison stays flattened");
    let markdown = report.markdown();
    assert!(markdown.contains("### Release admission"));
    assert!(markdown.contains("3 tasks in 2 families; counted attempts A 6, B 6"));
    assert!(markdown.contains("| entity_ambiguity | 2 | 4 | 4 | 0.25 | 1.00 | +0.75 |"));
}

#[test]
fn trials_outside_the_release_or_with_changed_identities_are_rejected() {
    let release = frozen(2, ReplacementPolicy::NoReplacement);
    let a = arm([[P, P], [P, P], [P, P]]);

    let mut outside = a.clone();
    outside.push(trial("code__github__unreleased", P));
    assert!(rejected(&outside, &a, &release).contains("is not a task of release"));

    for (change, message) in [
        (
            (|task: &mut ReleaseTask| task.contract_sha256 = format!("sha256:{}", "1".repeat(64)))
                as fn(&mut ReleaseTask),
            "contract mismatch",
        ),
        (
            |task| task.scenario_sha256 = format!("sha256:{}", "2".repeat(64)),
            "family or scenario metadata mismatch",
        ),
        (
            |task| task.dataset_sha256 = format!("sha256:{}", "3".repeat(64)),
            "world data mismatch",
        ),
    ] {
        let mut changed = release.clone();
        change(&mut changed.tasks[0]);
        assert!(rejected(&a, &a, &changed).contains(message), "{message}");
    }

    let mut evaluator = release.clone();
    evaluator.evaluator_version += 1;
    assert!(rejected(&a, &a, &evaluator).contains("evaluator"));

    let mut unverified = a.clone();
    unverified[0].provenance = None;
    assert!(rejected(&unverified, &a, &release).contains("lacks verified provenance"));

    let mut budget = release.clone();
    budget.comparison.budgets.agent_timeout_sec_max = Some(1);
    assert!(rejected(&a, &a, &budget).contains("above the declared 1s"));
    let mut tokens = release.clone();
    tokens.comparison.budgets.inference_tokens_max = Some(1_000_000);
    assert!(rejected(&a, &a, &tokens).contains("cannot yet be verified"));
}

#[test]
fn repeats_replacements_and_coverage_must_match_the_prespecified_design() {
    let strict = frozen(2, ReplacementPolicy::NoReplacement);
    let full = arm([[P, P], [P, F], [P, P]]);

    // A selectively repeated task.
    let mut extra = full.clone();
    extra.push(trial(TASKS[0].0, P));
    assert!(rejected(&extra, &full, &strict).contains("3 counted attempt(s)"));
    // A missing attempt and a missing task.
    assert!(rejected(&full[1..], &full, &strict).contains("1 counted attempt(s)"));
    assert!(rejected(&full[..4], &full, &strict).contains("0 counted attempt(s)"));
    // An unscorable attempt under a no-replacement design, even when "replaced".
    let mut replaced = full.clone();
    replaced.push(trial(TASKS[2].0, Outcome::EnvironmentError));
    assert!(rejected(&replaced, &full, &strict).contains("declares no replacement"));

    let lenient = frozen(
        2,
        ReplacementPolicy::ReplaceEnvironmentErrors { max_per_task: 1 },
    );
    let admitted = release::compare(&replaced, &full, &lenient, false).unwrap();
    let frozen = admitted.release_admission.frozen.unwrap();
    assert_eq!(frozen.denominators.replaced_attempts_a, 1);
    assert_eq!(frozen.denominators.counted_attempts_a, 6);
    assert_eq!(frozen.replaced_attempts[0].task, TASKS[2].0);
    assert!(
        frozen
            .denominators
            .retry_semantics
            .contains("model failures always count")
    );
    // An unreplaced environment error leaves a short task; two exceed the limit.
    let mut unreplaced = full.clone();
    unreplaced[5] = trial(TASKS[2].0, Outcome::EnvironmentError);
    assert!(rejected(&unreplaced, &full, &lenient).contains("1 counted attempt(s)"));
    let mut twice = replaced.clone();
    twice.push(trial(TASKS[2].0, Outcome::EnvironmentError));
    assert!(rejected(&twice, &full, &lenient).contains("exceeds the declared 1 replacement"));
    // Model failures and evaluator defects are never replaceable.
    let mut defect = full.clone();
    defect.push(trial(TASKS[0].0, Outcome::InvalidContract));
    assert!(rejected(&defect, &full, &lenient).contains("never a replaceable attempt"));
    let mut undecided = full.clone();
    undecided.push(trial(TASKS[0].0, Outcome::Unresolved));
    assert!(rejected(&undecided, &full, &lenient).contains("adjudication"));
}

#[test]
fn replacement_attempts_need_the_same_verified_sources_and_execution_conditions() {
    let release = frozen(
        2,
        ReplacementPolicy::ReplaceEnvironmentErrors { max_per_task: 1 },
    );
    let full = arm([[P, P], [P, F], [P, P]]);
    let mutate = |change: fn(&mut Trial)| {
        let mut attempts = full.clone();
        let mut replacement = trial(TASKS[2].0, Outcome::EnvironmentError);
        change(&mut replacement);
        attempts.push(replacement);
        attempts
    };
    let unknown = mutate(|attempt| attempt.provenance = None);
    assert!(rejected(&unknown, &full, &release).contains("lacks verified provenance"));
    let resultless = mutate(|attempt| attempt.outcome = Outcome::Error);
    assert!(rejected(&resultless, &full, &release).contains("resultless attempt"));
    let changed_source = mutate(|attempt| {
        attempt.provenance.as_mut().unwrap().source.dataset_sha256 =
            format!("sha256:{}", "1".repeat(64));
    });
    assert!(rejected(&changed_source, &full, &release).contains("world data mismatch"));
    let changed_budget = mutate(|attempt| {
        attempt
            .provenance
            .as_mut()
            .unwrap()
            .manifest
            .agent_timeout_sec = 901.0;
    });
    assert!(rejected(&changed_budget, &full, &release).contains("above the declared"));
    let changed_agent = mutate(|attempt| {
        attempt.provenance.as_mut().unwrap().manifest.agent.name = "another-agent".into();
    });
    assert!(rejected(&changed_agent, &full, &release).contains("heterogeneous"));
    let changed_runner = mutate(|attempt| {
        attempt.provenance.as_mut().unwrap().runner_adapter_sha256 = "2".repeat(64);
    });
    assert!(rejected(&changed_runner, &full, &release).contains("mixed evaluator"));
}

#[test]
fn the_trial_loader_reproduces_environment_errors_before_retaining_provenance() {
    let directory = tempfile::tempdir().unwrap();
    support::write_environment_error(directory.path(), "failed", TASKS[0].0);
    let attempts = load_trials(directory.path()).unwrap();
    assert_eq!(attempts.len(), 1);
    assert_eq!(attempts[0].outcome, Outcome::EnvironmentError);
    assert!(attempts[0].provenance.is_some());

    let assessment = directory.path().join("failed/verifier/assessment.json");
    let mut changed: Value = serde_json::from_slice(&fs::read(&assessment).unwrap()).unwrap();
    changed["failures"] = serde_json::json!([]);
    fs::write(&assessment, serde_json::to_vec(&changed).unwrap()).unwrap();
    let error = load_trials(directory.path()).unwrap_err().to_string();
    assert!(
        error.contains("saved assessment does not reproduce"),
        "{error}"
    );
    let diagnostic = report::explain::summary(directory.path()).unwrap();
    assert_eq!(diagnostic["trials"][0]["provenance_checked"], false);
    let explanation = &diagnostic["trials"][0]["explanation"];
    assert_eq!(explanation["verified"], false);
    assert!(explanation["failure_class"].is_null());
    assert!(
        explanation["unverified_reason"]
            .as_str()
            .unwrap()
            .contains("saved assessment does not reproduce")
    );
}

#[test]
fn replaced_attempts_cannot_cross_the_declared_split() {
    let mut release = frozen(
        2,
        ReplacementPolicy::ReplaceEnvironmentErrors { max_per_task: 1 },
    );
    release.external_bundle_seals[0].synthetic_fixture = false;
    let replacement = trial("ops__pagerduty__heldout", Outcome::EnvironmentError);
    let source = &replacement.provenance.as_ref().unwrap().source;
    let mut task = release.tasks[2].clone();
    task.task_id = source.scenario.clone();
    task.scenario_sha256 = source.scenario_sha256.clone();
    task.dataset_sha256 = source.dataset_sha256.clone();
    task.contract_sha256 = source.contract_sha256.clone();
    task.split = SplitAssignment::HeldOut;
    release.tasks.push(task);
    release
        .tasks
        .sort_by(|left, right| left.task_id.cmp(&right.task_id));
    release.validate().unwrap();
    let full = arm([[P, P], [P, F], [P, P]]);
    let mut mixed = full.clone();
    mixed.push(replacement);
    assert!(rejected(&mixed, &full, &release).contains("compared separately"));
}

#[test]
fn undeclared_treatments_and_synthetic_held_out_claims_are_rejected() {
    let release = frozen(2, ReplacementPolicy::NoReplacement);
    let a = arm([[P, P], [P, P], [P, P]]);
    assert!(
        release::compare(&a, &a, &release, true)
            .unwrap_err()
            .to_string()
            .contains("does not declare the agent as a treatment")
    );
    let mut declared = release.clone();
    declared.comparison.declared_treatments = vec!["agent".into()];
    let report = release::compare(&a, &a, &declared, false).unwrap();
    assert_eq!(report.comparison.declared_treatments, ["agent"]);

    // Held-out tasks cannot ride on a synthetic seal; a real seal is compared separately.
    let mut synthetic = release.clone();
    synthetic.tasks[2].split = SplitAssignment::HeldOut;
    assert!(rejected(&a, &a, &synthetic).contains("synthetic"));
    synthetic.external_bundle_seals[0].synthetic_fixture = false;
    assert!(rejected(&a, &a, &synthetic).contains("compared separately"));

    let empty: [Trial; 0] = [];
    assert!(rejected(&empty, &empty, &release).contains("not a zero-effect finding"));
}

fn bench_report(arguments: &[&std::ffi::OsStr]) -> std::process::Output {
    std::process::Command::new(env!("CARGO_BIN_EXE_bench-report"))
        .args(arguments)
        .output()
        .unwrap()
}

#[test]
fn the_report_cli_admits_verified_trials_against_a_manifest_file() {
    let directory = tempfile::tempdir().unwrap();
    let (a, b) = (directory.path().join("a"), directory.path().join("b"));
    for (root, passes) in [(&a, false), (&b, true)] {
        support::write_verified(root, "one__1", "example__github__basic", passes);
    }
    let loaded = load_trials(&a).unwrap();
    let source = &loaded[0].provenance.as_ref().unwrap().source;
    let mut manifest = frozen(1, ReplacementPolicy::NoReplacement);
    manifest.tasks = vec![ReleaseTask {
        task_id: source.scenario.clone(),
        scenario_sha256: source.scenario_sha256.clone(),
        dataset_sha256: source.dataset_sha256.clone(),
        contract_sha256: source.contract_sha256.clone(),
        ..manifest.tasks[0].clone()
    }];
    let path = directory.path().join("release.json");
    let write = |release: &Release, path: &Path| {
        fs::write(path, serde_json::to_vec_pretty(release).unwrap()).unwrap()
    };
    write(&manifest, &path);
    let run = |path: &Path| {
        bench_report(&[
            a.as_os_str(),
            b.as_os_str(),
            "--json".as_ref(),
            "--release".as_ref(),
            path.as_os_str(),
        ])
    };
    let output = run(&path);
    assert!(output.status.success(), "{output:?}");
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["release_admission"]["level"], "frozen_release");
    assert_eq!(
        report["release_admission"]["frozen"]["release_manifest_sha256"],
        manifest.sha256()
    );
    assert_eq!(
        report["release_admission"]["frozen"]["denominators"]["tasks"],
        1
    );
    assert_eq!(report["mean_diff"], 1.0);
    assert!(report["release_admission"]["frozen"]["uncertainty"]["family_cluster_ci95"].is_null());

    // The same verified trials against a manifest frozen for another contract.
    let mut other = manifest.clone();
    other.tasks[0].contract_sha256 = format!("sha256:{}", "9".repeat(64));
    let changed = directory.path().join("changed.json");
    write(&other, &changed);
    let output = run(&changed);
    assert!(!output.status.success() && output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains("contract mismatch"));

    // A manifest with an unknown field or an invalid design is not loaded.
    let mut loose: Value = serde_json::to_value(&manifest).unwrap();
    loose["comparison"]["stop_when_significant"] = true.into();
    fs::write(&changed, loose.to_string()).unwrap();
    assert!(!run(&changed).status.success());

    // Without a manifest the result is explicitly not a release result.
    let output = bench_report(&[a.as_os_str(), b.as_os_str(), "--json".as_ref()]);
    assert!(output.status.success(), "{output:?}");
    let legacy: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        legacy["release_admission"]["level"],
        "unfrozen_legacy_not_a_release_result"
    );
    assert!(legacy["release_admission"].get("frozen").is_none());
    let output = bench_report(&[a.as_os_str(), b.as_os_str()]);
    assert!(String::from_utf8_lossy(&output.stdout).contains("not a release result"));
}
