use std::{fs, path::Path};

use report::{Outcome, Trial, compare, load_trials};
use serde_json::json;
mod support;

fn write_trial(dir: &Path, name: &str, task: &str, reward: f64, outcome: f64) {
    let d = dir.join(name);
    fs::create_dir_all(&d).unwrap();
    let result = json!({
        "task_name": format!("openrouter/tool-bench__{task}"),
        "trial_name": name,
        "agent_info": {"name": "claude-code", "model_info": {"name": "anthropic/claude-sonnet-5"}},
        "verifier_result": {"rewards": {"reward": reward, "outcome_code": outcome, "api_calls": 3.0, "mcp_bypass": 0.0}},
        "exception_info": null
    });
    fs::write(d.join("result.json"), result.to_string()).unwrap();
}

#[test]
fn loads_trials_recursively_and_ignores_job_level_results() {
    let dir = tempfile::tempdir().unwrap();
    support::write_verified(dir.path(), "job/a__1", "code__github__x", true);
    fs::write(
        dir.path().join("job/result.json"),
        json!({"id": "job", "stats": {}}).to_string(),
    )
    .unwrap();
    let trials = load_trials(dir.path()).unwrap();
    assert_eq!(trials.len(), 1);
    assert_eq!(trials[0].task, "code__github__x");
    assert_eq!(trials[0].outcome, Outcome::Pass);
}

#[test]
fn empty_or_disjoint_runs_cannot_emit_a_zero_effect_comparison() {
    for variant in ["both_empty", "one_empty", "disjoint"] {
        let first = tempfile::tempdir().unwrap();
        let second = tempfile::tempdir().unwrap();
        if variant == "disjoint" {
            write_trial(first.path(), "first", "code__github__a", 1.0, 0.0);
        }
        if variant != "both_empty" {
            write_trial(second.path(), "second", "ops__pagerduty__b", 1.0, 0.0);
        }
        let output = std::process::Command::new(env!("CARGO_BIN_EXE_bench-report"))
            .arg(first.path())
            .arg(second.path())
            .arg("--json")
            .output()
            .unwrap();
        assert!(
            !output.status.success(),
            "{variant}: {}",
            String::from_utf8_lossy(&output.stdout)
        );
        assert!(output.stdout.is_empty(), "{variant}");
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("no shared tasks"),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[test]
fn unresolved_assessments_are_visible_and_cannot_be_published_as_final_paired_scores() {
    let first = tempfile::tempdir().unwrap();
    let second = tempfile::tempdir().unwrap();
    write_trial(first.path(), "undecided", "code__github__a", 0.0, 6.0);
    write_trial(second.path(), "decided", "code__github__a", 1.0, 0.0);
    let trials = load_trials(first.path()).unwrap();
    assert_eq!(
        serde_json::to_value(trials[0].outcome).unwrap(),
        json!("unresolved")
    );
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_bench-report"))
        .arg(first.path())
        .arg(second.path())
        .arg("--json")
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("require adjudication"),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn checked_unresolved_results_are_reproduced_before_the_comparison_refusal() {
    let directory = tempfile::tempdir().unwrap();
    support::write_unresolved(directory.path(), "trial", "example__github__basic");
    let trials = load_trials(directory.path()).unwrap();
    assert_eq!(trials[0].outcome, report::Outcome::Unresolved);
    assert!(trials[0].provenance.is_some());
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_bench-report"))
        .arg(directory.path())
        .arg(directory.path())
        .arg("--json")
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains("require adjudication"));
}

#[test]
fn pairs_tasks_and_reports_differences_with_a_deterministic_interval() {
    let t = |task: &str, reward: f64, outcome: Outcome| Trial {
        task: task.into(),
        reward,
        outcome,
        provenance: Some(support::provenance(task)),
        review: None,
    };
    let a = vec![
        t("code__github__x", 0.0, Outcome::ContextOverflow),
        t("code__github__x", 0.0, Outcome::ContextOverflow),
        t("ops__pagerduty__y", 1.0, Outcome::Pass),
        t("ops__pagerduty__y", 0.0, Outcome::Fail),
    ];
    let b = vec![
        t("code__github__x", 1.0, Outcome::Pass),
        t("code__github__x", 1.0, Outcome::Pass),
        t("ops__pagerduty__y", 1.0, Outcome::Pass),
        t("ops__pagerduty__y", 1.0, Outcome::Pass),
    ];
    let r = compare(&a, &b).unwrap();
    assert_eq!(r.paired_tasks, 2);
    assert!(r.unpaired_tasks.is_empty());
    assert!((r.mean_a - 0.25).abs() < 1e-9, "{}", r.mean_a);
    assert!((r.mean_b - 1.0).abs() < 1e-9);
    assert!((r.mean_diff - 0.75).abs() < 1e-9);
    assert!(r.ci95.0 <= r.mean_diff && r.mean_diff <= r.ci95.1);
    assert_eq!(r.b_better, 2);
    assert_eq!(r.a_better, 0);
    assert_eq!(r.outcomes_a["context_overflow"], 2);
    assert_eq!(
        compare(&a, &b).unwrap(),
        r,
        "bootstrap is seeded, so reports are reproducible"
    );
    let groups: Vec<&str> = r.groups.iter().map(|g| g.group.as_str()).collect();
    assert_eq!(groups, ["code", "ops"]);
    let md = r.markdown();
    assert!(
        md.contains("| code__github__x | 0.00 | 1.00 | +1.00 |"),
        "{md}"
    );
}

#[test]
fn invalid_environments_contracts_and_missing_assessments_never_become_model_failures() {
    for outcome in [
        Outcome::EnvironmentError,
        Outcome::InvalidContract,
        Outcome::Error,
    ] {
        let first = [Trial {
            task: "task".into(),
            reward: 0.0,
            outcome,
            provenance: None,
            review: None,
        }];
        let second = [Trial {
            task: "task".into(),
            reward: 1.0,
            outcome: Outcome::Pass,
            provenance: None,
            review: None,
        }];
        assert!(compare(&first, &second).is_err(), "{outcome:?}");
    }
}

#[test]
fn malformed_binary_assessments_cannot_produce_a_final_score() {
    for (reward, outcome) in [
        (0.0, Outcome::Pass),
        (1.0, Outcome::Fail),
        (0.5, Outcome::Pass),
        (f64::NAN, Outcome::Fail),
        (f64::INFINITY, Outcome::Pass),
    ] {
        let first = [Trial {
            task: "task".into(),
            reward,
            outcome,
            provenance: None,
            review: None,
        }];
        assert!(compare(&first, &first).is_err(), "{reward} {outcome:?}");
    }
}

#[test]
fn host_reported_trial_exceptions_cannot_be_hidden_by_an_existing_reward() {
    let directory = tempfile::tempdir().unwrap();
    write_trial(directory.path(), "failed", "task", 1.0, 0.0);
    let path = directory.path().join("failed/result.json");
    let mut value = support::result(directory.path(), "failed");
    value["exception_info"] =
        json!({"exception_type": "AdmissionError", "exception_message": "stop failed"});
    fs::write(path, value.to_string()).unwrap();
    let trials = load_trials(directory.path()).unwrap();
    assert_eq!(trials[0].outcome, Outcome::Error);
    assert!(compare(&trials, &trials).is_err());
}
