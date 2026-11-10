//! The default `bench-report`: per-arm summaries and paired comparisons over stock-Harbor
//! trials, which carry no runner receipt.
//!
//! Two kinds of trial directory are used. `tests/fixtures/stock` holds real trials copied
//! from stock Harbor 0.23.0 runs, and variants of them graded by the real verifier (see
//! its README). The rest are written here from the example scenario with the current
//! evaluator, so they keep reproducing when the evaluator changes.

use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use mock_core::{
    evidence::WorldEvidence,
    grading::{EVALUATOR_VERSION, Expect},
};
use report::arms::{self, Arm, Assurance, Status};
use serde_json::{Value, json};

mod support;

fn read(path: &Path) -> Value {
    serde_json::from_slice(&fs::read(path).unwrap_or_else(|error| panic!("{path:?}: {error}")))
        .unwrap()
}

fn write(path: &Path, value: &Value) {
    fs::write(path, serde_json::to_vec_pretty(value).unwrap()).unwrap();
}

const MODEL: &str = "claude-code 2.1.235, model anthropic/claude-sonnet-5";

/// Rewrites a strict fixture trial into what stock Harbor leaves behind: no runner
/// receipt or realized manifest, graded as `bench-verify --allow-unattested-runner
/// --withhold-invalid-reward` grades, with the host result a model run would have.
fn unattest(root: &Path, trial: &str, context_overflow: bool) {
    let directory = root.join(trial);
    let verifier = directory.join("verifier");
    for name in ["runner-admission.json", "realized-manifest.json"] {
        fs::remove_file(verifier.join(name)).unwrap();
    }
    let contract: Expect = serde_json::from_value(read(&verifier.join("contract.json"))).unwrap();
    let evidence: WorldEvidence =
        serde_json::from_value(read(&directory.join("artifacts/world/evidence.json"))).unwrap();
    let answer = fs::read_to_string(directory.join("artifacts/agent/answer.md")).unwrap();
    let assessment = evidence.assess(&contract, Some(&answer), context_overflow);
    let mut verification = read(&verifier.join("verification.json"));
    verification["runner_attested"] = json!(false);
    verification["runner_admission_checked"] = json!(false);
    verification["manifest_sha256"] = Value::Null;
    verification["runner_admission_sha256"] = Value::Null;
    verification["context_overflow"] = json!(context_overflow);
    write(&verifier.join("verification.json"), &verification);
    write(&verifier.join("assessment.json"), &json!(assessment));
    let mut result = support::result(root, trial);
    result["agent_info"] = json!({
        "name": "claude-code", "version": "2.1.235",
        "model_info": {"name": "claude-sonnet-5", "provider": "anthropic"}
    });
    if assessment.contract_ok && assessment.environment_ok {
        write(&verifier.join("reward.json"), &json!(assessment.metrics()));
        result["verifier_result"] = json!({"rewards": assessment.metrics()});
    } else {
        write(
            &verifier.join("reward.invalid.json"),
            &json!(assessment.metrics()),
        );
        result["verifier_result"] = Value::Null;
        result["exception_info"] = json!({
            "exception_type": "RewardFileNotFoundError",
            "exception_message": "No reward file found at verifier/reward.txt or verifier/reward.json"
        });
    }
    write(&directory.join("result.json"), &result);
}

fn pass(root: &Path, trial: &str, task: &str) {
    support::write_verified(root, trial, task, true);
    unattest(root, trial, false);
}

fn fail(root: &Path, trial: &str, task: &str) {
    support::write_verified(root, trial, task, false);
    unattest(root, trial, false);
}

fn context_overflow(root: &Path, trial: &str, task: &str) {
    support::write_verified(root, trial, task, false);
    unattest(root, trial, true);
}

fn unresolved(root: &Path, trial: &str, task: &str) {
    support::write_unresolved(root, trial, task);
    unattest(root, trial, false);
}

fn withheld(root: &Path, trial: &str, task: &str) {
    support::write_environment_error(root, trial, task);
    unattest(root, trial, false);
}

/// A trial whose host result carries an exception and no reward.
fn errored(root: &Path, trial: &str, task: &str, exception: &str, verifier_started: bool) {
    let directory = root.join(trial);
    fs::create_dir_all(directory.join("verifier")).unwrap();
    write(
        &directory.join("result.json"),
        &json!({
            "task_name": format!("openrouter/tool-bench__{task}"), "trial_name": trial,
            "agent_info": {"name": "claude-code", "version": "2.1.235",
                "model_info": {"name": "claude-sonnet-5", "provider": "anthropic"}},
            "verifier_result": null,
            "verifier": if verifier_started { json!({"started_at": "2026-11-10T19:06:33Z"}) } else { Value::Null },
            "exception_info": {"exception_type": exception, "exception_message": "boom\nsecond line"}
        }),
    );
}

fn arm(label: &str, root: &Path) -> Arm {
    arms::load_arm(label, root).unwrap()
}

fn statuses(arm: &Arm) -> Vec<(&str, Status)> {
    arm.trials
        .iter()
        .map(|trial| (trial.trial.as_str(), trial.status))
        .collect()
}

fn fixtures(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/stock")
        .join(name)
}

/// The tier a committed fixture trial reaches. It was graded by one evaluator revision
/// and only that revision can re-run it; any other reports the verifier's own record.
fn fixture_tier() -> Assurance {
    let graded_by = read(
        &fixtures("oracle/2026-11-10__13-16-13/search__exa__find-vp-engineering__wM2CQWr")
            .join("verifier/verification.json"),
    )["evaluator_version"]
        .as_u64()
        .unwrap();
    if graded_by == u64::from(EVALUATOR_VERSION) {
        Assurance::Reproduced
    } else {
        Assurance::Recorded
    }
}

#[test]
fn real_stock_harbor_trials_reproduce_and_their_arms_are_summarized() {
    let (oracle, nop) = (arm("A", &fixtures("oracle")), arm("B", &fixtures("nop")));
    assert_eq!(
        statuses(&oracle),
        [
            ("business__zendesk__find-invoice__gLH42GH", Status::Pass),
            ("search__exa__find-vp-engineering__wM2CQWr", Status::Pass),
        ]
    );
    assert_eq!(
        statuses(&nop),
        [
            (
                "business__shopify__concierge-not__UyuwoH8",
                Status::HarnessException
            ),
            ("business__zendesk__find-invoice__r26zPNi", Status::Fail),
        ]
    );
    for trial in oracle.trials.iter().chain(&nop.trials[1..]) {
        assert_ne!(
            trial.assurance,
            Assurance::Failed,
            "{}: {:?}. These trials were graded by the evaluator revision in their \
             verification.json. If that is still EVALUATOR_VERSION, grading changed without a \
             version bump; otherwise see tests/fixtures/README.md.",
            trial.trial,
            trial.note
        );
        assert_eq!(trial.assurance, fixture_tier(), "{:?}", trial.note);
    }
    let exception = &nop.trials[0];
    assert_eq!(exception.assurance, Assurance::None);
    assert_eq!(exception.exception.as_ref().unwrap().kind, "RuntimeError");
    assert!(
        exception
            .note
            .as_ref()
            .unwrap()
            .starts_with("RuntimeError: Docker compose")
    );

    let summary = arms::summarize(&oracle);
    assert_eq!((summary.trials, summary.tasks), (2, 2));
    assert_eq!(summary.agents[0].name, "oracle 1.0.0");
    assert_eq!(
        (
            summary.pass_rate.passed,
            summary.pass_rate.assessed,
            summary.pass_rate.excluded
        ),
        (2, 2, 0)
    );
    // The oracle made one request for the search task and two for the Zendesk task.
    let calls = summary.api_calls.unwrap();
    assert_eq!((calls.n, calls.mean, calls.median), (2, 1.5, 1.5));
    let groups: Vec<_> = summary
        .groups
        .iter()
        .map(|group| (group.group.as_str(), group.rate.passed, group.rate.assessed))
        .collect();
    assert_eq!(groups, [("business", 1, 1), ("search", 1, 1)]);
    assert!(summary.usage.agent_seconds.is_some());
    assert_eq!(summary.usage.trials_reporting_tokens, 0);

    let summary = arms::summarize(&nop);
    assert_eq!(summary.outcomes["fail"], 1);
    assert_eq!(summary.outcomes["harness_exception"], 1);
    assert_eq!(
        (
            summary.pass_rate.passed,
            summary.pass_rate.assessed,
            summary.pass_rate.excluded
        ),
        (0, 1, 1)
    );
    assert_eq!(summary.pass_rate.rate, Some(0.0));
    assert_eq!(summary.failed_without_api_calls, 1);
    assert_eq!(summary.failed_checks[0].name, "answer");
    assert_eq!(summary.unassessed.len(), 1);

    let paired = arms::compare(&oracle, &nop);
    assert_eq!(paired.paired_tasks, 1);
    assert_eq!((paired.b_better, paired.a_better, paired.ties), (0, 1, 0));
    assert_eq!(
        paired.flips[0].task,
        "business__zendesk__find-invoice-attachment"
    );
    assert_eq!(
        paired.only_in_a,
        ["search__exa__find-vp-engineering-profile"]
    );
    assert_eq!(paired.only_in_b, ["business__shopify__concierge-note"]);
    assert!(
        paired
            .warnings
            .iter()
            .any(|warning| warning.starts_with("Task sets differ"))
    );
    // Half of the nop arm has no assessment, none of the oracle arm.
    assert!(
        paired.warnings[1]
            .starts_with("A left 0.0% of its trials out of its pass rate and B left 50.0%"),
        "{:?}",
        paired.warnings
    );
    // Same evaluator, same contracts and same task directories: nothing else to warn about.
    assert_eq!(paired.warnings.len(), 2, "{:?}", paired.warnings);
}

#[test]
fn variants_graded_by_the_real_verifier_land_in_their_own_categories() {
    let variants = arm("A", &fixtures("variants"));
    assert_eq!(
        statuses(&variants),
        [
            (
                "business__zendesk__find-invoice__CTXOVER",
                Status::ContextOverflow
            ),
            (
                "business__zendesk__find-invoice__INCOMPL",
                Status::Incomplete
            ),
            (
                "search__exa__find-vp-engineering__UNRESLV",
                Status::Unresolved
            ),
            (
                "search__exa__find-vp-engineering__WITHHLD",
                Status::WithheldReward
            ),
            ("search__exa__find-vp-engineering__WRONGAN", Status::Fail),
        ]
    );
    for trial in &variants.trials {
        let expected = if trial.status == Status::Incomplete {
            Assurance::None
        } else {
            fixture_tier()
        };
        assert_eq!(
            trial.assurance, expected,
            "{}: {:?}",
            trial.trial, trial.note
        );
    }
    let summary = arms::summarize(&variants);
    // Fail and context overflow are assessed; unresolved, withheld and incomplete are not.
    assert_eq!(
        (
            summary.pass_rate.passed,
            summary.pass_rate.assessed,
            summary.pass_rate.excluded
        ),
        (0, 2, 3)
    );
    assert_eq!(summary.agents[0].name, MODEL);
    assert_eq!(
        summary.agents.len(),
        1,
        "an incomplete trial has no identity to differ in"
    );
    assert_eq!(summary.usage.trials_reporting_tokens, 3);
    assert_eq!(summary.usage.input_tokens, Some(3 * 1_843_210));
    assert!((summary.usage.cost_usd.unwrap() - 4.02).abs() < 1e-9);
    let withheld = summary
        .unassessed
        .iter()
        .find(|trial| trial.status == Status::WithheldReward)
        .unwrap();
    assert!(
        withheld
            .reason
            .starts_with("environment_error at evidence/task-source:"),
        "{}",
        withheld.reason
    );

    let explained = arms::explain(&[variants], "search/exa/find-vp-engineering-profile").unwrap();
    assert_eq!(explained.len(), 3);
    let wrong = explained
        .iter()
        .find(|trial| trial.trial.status == Status::Fail)
        .unwrap();
    assert_eq!(wrong.checks_not_ok, ["answer_ok"]);
    let failure = &wrong.trial.failed_checks[0];
    assert_eq!(
        (&failure.check, &failure.disposition),
        (&json!("answer"), &json!("failed"))
    );
    assert_eq!(failure.location, "answer");
    assert!(
        failure.reason.contains("another authored record's value"),
        "{}",
        failure.reason
    );
    assert_eq!(
        wrong.answer_excerpt.as_deref(),
        Some("https://www.linkedin.com/in/priya-raman-4b21a9")
    );
    let text = arms::explanation_markdown(&explained);
    assert!(text.contains("**answer** (failed) at `answer`"), "{text}");
    assert!(
        text.contains("**answer** (unresolved) at `answer`"),
        "{text}"
    );
    assert!(
        text.contains("**environment** (failed) at `evidence/task-source`"),
        "{text}"
    );
}

#[test]
fn the_pass_rate_counts_only_assessed_trials_and_lists_everything_else() {
    let root = tempfile::tempdir().unwrap();
    let root = root.path();
    pass(root, "a__1", "code__github__a");
    pass(root, "a__2", "code__github__a");
    fail(root, "a__3", "code__github__a");
    fail(root, "b__1", "ops__multi__b");
    pass(root, "c__1", "ops__multi__c");
    context_overflow(root, "d__1", "search__exa__d");
    unresolved(root, "e__1", "business__hubspot__e");
    withheld(root, "f__1", "cloud__aws__f");
    errored(
        root,
        "g__1",
        "collab__slack__g",
        "VerifierTimeoutError",
        true,
    );
    errored(
        root,
        "h__1",
        "collab__slack__h",
        "EnvironmentStartTimeoutError",
        false,
    );
    let incomplete = root.join("i__1");
    fs::create_dir_all(incomplete.join("verifier")).unwrap();
    write(
        &incomplete.join("config.json"),
        &json!({"task": {"path": "/tasks/collab__slack__i"}, "trial_name": "i__1"}),
    );

    let loaded = arm("A", root);
    assert_eq!(
        statuses(&loaded),
        [
            ("e__1", Status::Unresolved),
            ("f__1", Status::WithheldReward),
            ("a__1", Status::Pass),
            ("a__2", Status::Pass),
            ("a__3", Status::Fail),
            ("g__1", Status::VerifierError),
            ("h__1", Status::HarnessException),
            ("i__1", Status::Incomplete),
            ("b__1", Status::Fail),
            ("c__1", Status::Pass),
            ("d__1", Status::ContextOverflow),
        ]
    );
    for trial in &loaded.trials {
        let checked = matches!(
            trial.status,
            Status::VerifierError | Status::HarnessException | Status::Incomplete
        );
        assert_eq!(
            trial.assurance,
            if checked {
                Assurance::None
            } else {
                Assurance::Reproduced
            },
            "{}: {:?}",
            trial.trial,
            trial.note
        );
    }
    let summary = arms::summarize(&loaded);
    assert_eq!((summary.trials, summary.tasks), (11, 9));
    assert_eq!(summary.attempts_per_task, (1, 3));
    let rate = &summary.pass_rate;
    assert_eq!((rate.passed, rate.assessed, rate.excluded), (3, 6, 5));
    assert_eq!(rate.rate, Some(0.5));
    let (low, high) = rate.ci95.unwrap();
    assert!(
        low > 0.18 && low < 0.19 && high > 0.81 && high < 0.82,
        "{low} {high}"
    );
    for (outcome, count) in [
        ("pass", 3),
        ("fail", 2),
        ("context_overflow", 1),
        ("integrity_violation", 0),
        ("unresolved", 1),
        ("withheld_reward", 1),
        ("verifier_error", 1),
        ("harness_exception", 1),
        ("incomplete", 1),
        ("reverification_failed", 0),
    ] {
        assert_eq!(summary.outcomes[outcome], count, "{outcome}");
    }
    assert_eq!(summary.assurance["reproduced"], 8);
    assert_eq!(summary.assurance["none"], 3);
    let consistency = &summary.task_consistency;
    assert_eq!(
        (
            consistency.tasks,
            consistency.passed_every_attempt,
            consistency.passed_some_attempts,
            consistency.passed_no_attempt
        ),
        (4, 1, 1, 2)
    );
    let group = |name: &str| {
        let group = summary
            .groups
            .iter()
            .find(|group| group.group == name)
            .unwrap();
        (
            group.tasks,
            group.rate.passed,
            group.rate.assessed,
            group.rate.excluded,
        )
    };
    assert_eq!(group("code"), (1, 2, 3, 0));
    assert_eq!(group("ops"), (2, 1, 2, 0));
    assert_eq!(group("search"), (1, 0, 1, 0));
    assert_eq!(group("business"), (1, 0, 0, 1));
    assert_eq!(group("collab"), (3, 0, 0, 3));
    let scope = |name: &str| {
        let scope = summary
            .scopes
            .iter()
            .find(|scope| scope.group == name)
            .unwrap();
        (scope.tasks, scope.rate.passed, scope.rate.assessed)
    };
    assert_eq!(scope("multi-service"), (2, 1, 2));
    assert_eq!(scope("single-service"), (7, 2, 4));
    // Context overflow is assessed but is not a failed check; the two fails missed the answer.
    assert_eq!(summary.failed_checks.len(), 1);
    assert_eq!(
        (
            summary.failed_checks[0].name.as_str(),
            summary.failed_checks[0].trials
        ),
        ("answer", 2)
    );
    assert_eq!(summary.unassessed.len(), 5);
    let reason = |status: Status| {
        &summary
            .unassessed
            .iter()
            .find(|trial| trial.status == status)
            .unwrap()
            .reason
    };
    assert_eq!(reason(Status::VerifierError), "VerifierTimeoutError: boom");
    assert_eq!(
        reason(Status::HarnessException),
        "EnvironmentStartTimeoutError: boom"
    );
    assert!(reason(Status::WithheldReward).starts_with("environment_error at "));
    assert!(summary.warnings.is_empty(), "{:?}", summary.warnings);

    let markdown = arms::report(&[loaded]).markdown();
    for line in [
        "| **Pass rate (assessed trials)** | **50.0%** |",
        "| Excluded from the pass rate | 5 |",
        "| *assessed (pass-rate denominator)* | *6* |",
        "| withheld reward (verifier could not assess) | 1 |",
        "| verifier error (no reward file) | 1 |",
        "| agent or environment exception | 1 |",
        "| incomplete (no result.json) | 1 |",
        "| code | 66.7% (2/3); 1 task |",
        "| collab | n/a (0/0), 3 excluded; 3 tasks |",
        "| multi-service | 50.0% (1/2); 2 tasks |",
        "| answer | 2 |",
        "- `collab__slack__g` (`g__1`): verifier error (no reward file): VerifierTimeoutError: boom",
    ] {
        assert!(markdown.contains(line), "missing {line:?} in\n{markdown}");
    }
    assert!(!markdown.contains("## Paired comparison"));
}

#[test]
fn two_arms_are_paired_by_task_with_wins_losses_ties_and_flips() {
    let (a, b) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let (a, b) = (a.path(), b.path());
    // x: A fails all three, B passes two of three. y: both pass. z: A passes, B fails.
    for attempt in 1..=3 {
        fail(a, &format!("x__{attempt}"), "code__github__x");
    }
    pass(b, "x__1", "code__github__x");
    pass(b, "x__2", "code__github__x");
    fail(b, "x__3", "code__github__x");
    pass(a, "y__1", "ops__pagerduty__y");
    pass(b, "y__1", "ops__pagerduty__y");
    pass(a, "z__1", "ops__pagerduty__z");
    fail(b, "z__1", "ops__pagerduty__z");
    // w: A's verifier never produced a reward, so w cannot be paired and is not a loss for A.
    errored(a, "w__1", "cloud__aws__w", "RewardFileNotFoundError", true);
    pass(b, "w__1", "cloud__aws__w");

    let (left, right) = (arm("A", a), arm("B", b));
    let paired = arms::compare(&left, &right);
    assert_eq!(paired.paired_tasks, 3);
    assert_eq!((paired.b_better, paired.a_better, paired.ties), (1, 1, 1));
    assert!((paired.mean_a - 2.0 / 3.0).abs() < 1e-9);
    assert!((paired.mean_b - (2.0 / 3.0 + 1.0) / 3.0).abs() < 1e-9);
    assert!((paired.mean_diff - (2.0 / 3.0 - 1.0) / 3.0).abs() < 1e-9);
    let (low, high) = paired.ci95.unwrap();
    assert!(low <= paired.mean_diff && paired.mean_diff <= high);
    assert_eq!(paired.sign_test_p, Some(1.0));
    let flips: Vec<_> = paired
        .flips
        .iter()
        .map(|task| {
            (
                task.task.as_str(),
                task.passed_a,
                task.assessed_a,
                task.passed_b,
                task.assessed_b,
            )
        })
        .collect();
    assert_eq!(
        flips,
        [
            ("ops__pagerduty__z", 1, 1, 0, 1),
            ("code__github__x", 0, 3, 2, 3)
        ]
    );
    assert_eq!(paired.attempted_but_unpaired, ["cloud__aws__w"]);
    assert!(paired.only_in_a.is_empty() && paired.only_in_b.is_empty());
    assert_eq!(paired.warnings.len(), 2, "{:?}", paired.warnings);
    assert!(paired.warnings[0].contains("not paired because one arm has no assessed trial"));
    assert!(
        paired.warnings[1]
            .starts_with("A left 16.7% of its trials out of its pass rate and B left 0.0%")
    );
    assert_eq!(
        arms::compare(&left, &right),
        paired,
        "the bootstrap is seeded"
    );

    let report = arms::report(&[left, right]);
    assert_eq!(report.comparisons.len(), 1);
    let markdown = report.markdown();
    for line in [
        "## Paired comparison: B − A",
        "- Paired tasks (an assessed trial in both arms): 3",
        "- Tasks where B is better / A is better / tie: 1 / 1 / 1",
        "| ops__pagerduty__z | 1/1 (100%) | 0/1 (0%) | -100 |",
        "| code__github__x | 0/3 (0%) | 2/3 (67%) | +67 |",
    ] {
        assert!(markdown.contains(line), "missing {line:?} in\n{markdown}");
    }
    assert!(
        !markdown.contains("| ops__pagerduty__y |"),
        "ties are not flips"
    );
}

#[test]
fn arms_that_differ_in_evaluator_tasks_or_contracts_are_flagged_as_not_comparable() {
    let (a, b) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let (a, b) = (a.path(), b.path());
    pass(a, "same__1", "code__github__same");
    pass(b, "same__1", "code__github__same");
    pass(a, "only-a__1", "code__github__only-a");
    pass(b, "only-b__1", "code__github__only-b");
    // The same task graded against another contract in B.
    pass(a, "contract__1", "code__github__contract");
    support::write_verified_final_response(b, "contract__1", "code__github__contract", true);
    unattest(b, "contract__1", false);
    let same = arms::compare(&arm("A", a), &arm("B", b));
    assert_eq!(same.warnings.len(), 2, "{:?}", same.warnings);
    assert!(same.warnings[0].starts_with(
        "Task sets differ (only in A: code__github__only-a; only in B: code__github__only-b)"
    ));
    assert!(
        same.warnings[1]
            .starts_with("Contract hashes differ for 1 shared task(s) (code__github__contract)")
    );

    // B graded by an older evaluator: its record is kept, labelled, and not re-run.
    let older = EVALUATOR_VERSION - 1;
    for trial in ["same__1", "only-b__1", "contract__1"] {
        let directory = b.join(trial);
        let mut verification = read(&directory.join("verifier/verification.json"));
        verification["evaluator_version"] = json!(older);
        write(&directory.join("verifier/verification.json"), &verification);
        for file in ["verifier/reward.json", "verifier/assessment.json"] {
            let mut value = read(&directory.join(file));
            value["evaluator_version"] = json!(older);
            write(&directory.join(file), &value);
        }
        let mut result = read(&directory.join("result.json"));
        result["verifier_result"]["rewards"]["evaluator_version"] = json!(older);
        write(&directory.join("result.json"), &result);
    }
    let (left, right) = (arm("A", a), arm("B", b));
    for trial in &right.trials {
        assert_eq!(trial.status, Status::Pass);
        assert_eq!(trial.assurance, Assurance::Recorded);
        assert!(
            trial.note.as_ref().unwrap().starts_with(&format!(
                "graded by evaluator {older}; this build is evaluator {EVALUATOR_VERSION}"
            )),
            "{:?}",
            trial.note
        );
    }
    let summary = arms::summarize(&right);
    assert_eq!(summary.evaluator_versions, [older]);
    assert_eq!(
        summary.pass_rate.assessed, 3,
        "a recorded result still counts"
    );
    assert!(summary.warnings[0].starts_with("3 trial(s) are recorded only"));
    let paired = arms::compare(&left, &right);
    assert!(
        paired.warnings[0].starts_with(&format!(
            "Evaluator versions differ (A: {EVALUATOR_VERSION}; B: {older})"
        )),
        "{:?}",
        paired.warnings
    );
    let markdown = arms::report(&[left, right]).markdown();
    assert!(
        markdown.contains("> **Warning (B vs A):** Evaluator versions differ"),
        "{markdown}"
    );
    assert!(
        markdown.contains("> **Warning (B):** 3 trial(s) are recorded only"),
        "{markdown}"
    );
}

#[test]
fn a_result_that_disagrees_with_its_collected_artifacts_leaves_every_rate() {
    type Edit = fn(&Path);
    let edits: [(&str, Edit, &str); 6] = [
        (
            "host reward raised",
            |trial| {
                let mut result = read(&trial.join("result.json"));
                result["verifier_result"]["rewards"]["reward"] = json!(1.0);
                result["verifier_result"]["rewards"]["outcome_code"] = json!(0.0);
                write(&trial.join("result.json"), &result);
            },
            "the recorded reward differs from verifier/reward.json",
        ),
        (
            "host and verifier reward raised together",
            |trial| {
                let mut result = read(&trial.join("result.json"));
                for (name, value) in [("reward", 1.0), ("outcome_code", 0.0), ("answer_ok", 1.0)] {
                    result["verifier_result"]["rewards"][name] = json!(value);
                }
                write(&trial.join("result.json"), &result);
                write(
                    &trial.join("verifier/reward.json"),
                    &result["verifier_result"]["rewards"],
                );
            },
            "the recorded reward metrics differ from the re-run assessment",
        ),
        (
            "saved assessment rewritten",
            |trial| {
                let mut saved = read(&trial.join("verifier/assessment.json"));
                saved["outcome"] = json!("pass");
                saved["reward"] = json!(1.0);
                write(&trial.join("verifier/assessment.json"), &saved);
            },
            "the saved assessment does not reproduce",
        ),
        (
            "answer replaced after collection",
            |trial| fs::write(trial.join("artifacts/agent/answer.md"), "Maya").unwrap(),
            "artifacts/agent/answer.md differs from the bytes the verifier recorded",
        ),
        (
            "evidence replaced after collection",
            |trial| {
                let path = trial.join("artifacts/world/evidence.json");
                let mut bytes = fs::read(&path).unwrap();
                bytes.push(b'\n');
                fs::write(path, bytes).unwrap();
            },
            "artifacts/world/evidence.json differs from the bytes the verifier recorded",
        ),
        (
            "another task's result",
            |trial| {
                let mut result = read(&trial.join("result.json"));
                result["task_name"] = json!("openrouter/tool-bench__code__github__other");
                write(&trial.join("result.json"), &result);
            },
            "the host task identity does not match the scenario the verifier graded",
        ),
    ];
    for (name, edit, reason) in edits {
        let root = tempfile::tempdir().unwrap();
        let root = root.path();
        fail(root, "edited__1", "code__github__x");
        pass(root, "honest__1", "code__github__y");
        edit(&root.join("edited__1"));
        let loaded = arm("A", root);
        let edited = loaded
            .trials
            .iter()
            .find(|trial| trial.trial == "edited__1")
            .unwrap();
        assert_eq!(edited.status, Status::ReverificationFailed, "{name}");
        assert_eq!(edited.assurance, Assurance::Failed, "{name}");
        assert!(
            edited.note.as_ref().unwrap().contains(reason),
            "{name}: {:?}",
            edited.note
        );
        let summary = arms::summarize(&loaded);
        let rate = &summary.pass_rate;
        assert_eq!(
            (rate.passed, rate.assessed, rate.excluded),
            (1, 1, 1),
            "{name}"
        );
        assert!(
            summary.warnings[0].starts_with("1 trial(s) failed re-verification"),
            "{name}"
        );
    }

    // Artifacts that were never downloaded cannot contradict anything: the record stands,
    // labelled as not re-run.
    let root = tempfile::tempdir().unwrap();
    fail(root.path(), "trimmed__1", "code__github__x");
    fs::remove_file(root.path().join("trimmed__1/artifacts/world/evidence.json")).unwrap();
    let loaded = arm("A", root.path());
    assert_eq!(loaded.trials[0].status, Status::Fail);
    assert_eq!(loaded.trials[0].assurance, Assurance::Recorded);
    assert!(
        loaded.trials[0]
            .note
            .as_ref()
            .unwrap()
            .contains("was not collected")
    );
}

#[test]
fn attested_trials_keep_the_stronger_tier_and_host_only_results_the_weakest() {
    let root = tempfile::tempdir().unwrap();
    let root = root.path();
    // A strict-controller trial: runner receipt and realized manifest present.
    support::write_verified(root, "attested__1", "code__github__a", true);
    // The same, with the realized manifest replaced afterwards.
    support::write_verified(root, "tampered__1", "code__github__b", true);
    fs::write(
        root.join("tampered__1/verifier/realized-manifest.json"),
        "{}",
    )
    .unwrap();
    // Only a host result, as an old scalar export has.
    fs::create_dir_all(root.join("scalar__1")).unwrap();
    write(
        &root.join("scalar__1/result.json"),
        &json!({
            "task_name": "openrouter/tool-bench__code__github__c", "trial_name": "scalar__1",
            "agent_info": {"name": "claude-code"}, "exception_info": null,
            "verifier_result": {"rewards": {"reward": 0.0, "outcome_code": 1.0, "api_calls": 4.0}}
        }),
    );
    let loaded = arm("A", root);
    let tiers: Vec<_> = loaded
        .trials
        .iter()
        .map(|trial| (trial.trial.as_str(), trial.status, trial.assurance))
        .collect();
    assert_eq!(
        tiers,
        [
            ("attested__1", Status::Pass, Assurance::Attested),
            (
                "tampered__1",
                Status::ReverificationFailed,
                Assurance::Failed
            ),
            ("scalar__1", Status::Fail, Assurance::Recorded),
        ]
    );
    assert!(
        loaded.trials[2]
            .note
            .as_ref()
            .unwrap()
            .contains("no verifier/verification.json"),
        "{:?}",
        loaded.trials[2].note
    );
    let summary = arms::summarize(&loaded);
    assert_eq!(
        (
            summary.assurance["attested"],
            summary.assurance["recorded"],
            summary.assurance["failed"]
        ),
        (1, 1, 1)
    );
    assert_eq!(
        (summary.pass_rate.passed, summary.pass_rate.assessed),
        (1, 2)
    );
}

#[test]
fn an_agent_timeout_that_was_still_graded_is_an_assessed_trial() {
    let root = tempfile::tempdir().unwrap();
    let root = root.path();
    for (trial, task, passes) in [
        ("slow__1", "code__github__a", false),
        ("late__1", "code__github__b", true),
    ] {
        support::write_verified(root, trial, task, passes);
        unattest(root, trial, false);
        let mut result = read(&root.join(trial).join("result.json"));
        result["exception_info"] = json!({
            "exception_type": "AgentTimeoutError",
            "exception_message": "Agent execution timed out after 900.0 seconds"
        });
        write(&root.join(trial).join("result.json"), &result);
    }
    let loaded = arm("A", root);
    assert_eq!(
        statuses(&loaded),
        [("slow__1", Status::Fail), ("late__1", Status::Pass)]
    );
    let summary = arms::summarize(&loaded);
    assert_eq!(
        (summary.pass_rate.passed, summary.pass_rate.assessed),
        (1, 2)
    );
    let exceptions = &summary.exceptions_on_assessed_trials;
    assert_eq!(
        (exceptions[0].name.as_str(), exceptions[0].trials),
        ("AgentTimeoutError", 2)
    );
    assert!(
        arms::report(&[loaded])
            .markdown()
            .contains("| Host exceptions on assessed trials | 2 × AgentTimeoutError |")
    );
}

#[test]
fn trials_are_found_at_any_depth_and_agent_written_files_are_not_trials() {
    let root = tempfile::tempdir().unwrap();
    let root = root.path();
    // The production layout, and a local `harbor run -o` layout, side by side.
    pass(root, "runs/wf-1/trials/1/trials/1/a__1", "code__github__a");
    fail(root, "job/2026-11-10__12-00-00/b__1", "code__github__b");
    write(
        &root.join("job/2026-11-10__12-00-00/result.json"),
        &json!({"id": "job", "stats": {}}),
    );
    write(
        &root.join("job/2026-11-10__12-00-00/config.json"),
        &json!({"jobs_dir": "job"}),
    );
    // A result the agent wrote into its own artifacts is inside a trial, never one itself.
    let planted = root.join("job/2026-11-10__12-00-00/b__1/artifacts/agent/nested");
    fs::create_dir_all(planted.join("verifier")).unwrap();
    write(
        &planted.join("result.json"),
        &json!({"task_name": "openrouter/tool-bench__code__github__b", "trial_name": "planted",
            "verifier_result": {"rewards": {"reward": 1.0, "outcome_code": 0.0}}}),
    );
    #[cfg(unix)]
    std::os::unix::fs::symlink(root.join("runs"), root.join("link")).unwrap();
    let loaded = arm("A", root);
    assert_eq!(
        statuses(&loaded),
        [("a__1", Status::Pass), ("b__1", Status::Fail)]
    );
    assert_eq!(
        loaded.trials[0].directory,
        "runs/wf-1/trials/1/trials/1/a__1"
    );
    #[cfg(unix)]
    {
        assert_eq!(loaded.skipped_links.len(), 1);
        assert!(arms::summarize(&loaded).warnings[0].contains("symbolic link"));
    }

    let empty = tempfile::tempdir().unwrap();
    let error = arms::load_arm("A", empty.path()).unwrap_err().to_string();
    assert!(error.contains("no Harbor trial directories"), "{error}");
}

fn bench_report(arguments: &[&std::ffi::OsStr]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_bench-report"))
        .args(arguments)
        .output()
        .unwrap()
}

#[test]
fn the_cli_reports_unattested_runs_by_default_and_refuses_them_when_attestation_is_required() {
    let (a, b) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    fail(a.path(), "x__1", "code__github__x");
    pass(b.path(), "x__1", "code__github__x");
    let (a, b) = (a.path().as_os_str(), b.path().as_os_str());

    let output = bench_report(&[a, b]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let markdown = String::from_utf8(output.stdout).unwrap();
    assert!(
        markdown.starts_with("# tool-bench arm report"),
        "{markdown}"
    );
    assert!(markdown.contains("| **Pass rate (assessed trials)** | **0.0%** | **100.0%** |"));
    assert!(markdown.contains("| code__github__x | 0/1 (0%) | 1/1 (100%) | +100 |"));
    assert!(markdown.contains("Unattested tier."));

    let output = bench_report(&[a, b, "--json".as_ref()]);
    assert!(output.status.success());
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["format"], "arm_report_v1");
    assert_eq!(report["evaluator_version"], EVALUATOR_VERSION);
    assert_eq!(report["arms"][0]["label"], "A");
    assert_eq!(report["arms"][0]["pass_rate"]["rate"], 0.0);
    assert_eq!(report["arms"][1]["pass_rate"]["rate"], 1.0);
    assert_eq!(report["arms"][1]["agents"][0]["name"], MODEL);
    assert_eq!(report["arms"][0]["trial_records"][0]["status"], "fail");
    assert_eq!(
        report["arms"][0]["trial_records"][0]["assurance"],
        "reproduced"
    );
    assert_eq!(report["arms"][0]["groups"][0]["group"], "code");
    assert_eq!(report["arms"][0]["groups"][0]["assessed"], 1);
    assert_eq!(report["comparisons"][0]["b_better"], 1);
    assert_eq!(
        report["comparisons"][0]["flips"][0]["task"],
        "code__github__x"
    );

    // One run directory is a report on one arm, with nothing to pair.
    let output = bench_report(&[a, "--json".as_ref()]);
    assert!(output.status.success());
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["arms"].as_array().unwrap().len(), 1);
    assert!(report["comparisons"].as_array().unwrap().is_empty());

    // The strict comparison does not admit a trial without a runner receipt.
    let output = bench_report(&["--require-attested".as_ref(), a, b, "--json".as_ref()]);
    assert!(!output.status.success() && output.stdout.is_empty());
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("assessment lacks checked source and execution provenance"),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );

    // Options of the strict comparison are not silently ignored by the default report.
    for option in ["--allow-agent-difference", "--release"] {
        let output = bench_report(&[a, b, option.as_ref()]);
        assert!(!output.status.success() && output.stdout.is_empty());
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("add --require-attested"),
            "{option}"
        );
    }
    for arguments in [
        vec![],
        vec!["--json".as_ref()],
        vec![a, "--frobnicate".as_ref()],
    ] {
        let output = bench_report(&arguments);
        assert!(
            !output.status.success() && output.stdout.is_empty(),
            "{arguments:?}"
        );
    }
    let missing = bench_report(&["/nonexistent/run".as_ref()]);
    assert!(!missing.status.success());
    assert!(String::from_utf8_lossy(&missing.stderr).contains("is not a directory"));
}

#[test]
fn the_cli_explains_why_the_trials_of_one_task_ended_as_they_did() {
    let (oracle, variants) = (fixtures("oracle"), fixtures("variants"));
    let (oracle, variants) = (oracle.as_os_str(), variants.as_os_str());
    let output = bench_report(&[
        oracle,
        variants,
        "--explain".as_ref(),
        "search__exa__find-vp-engineering-profile".as_ref(),
    ]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let text = String::from_utf8(output.stdout).unwrap();
    for line in [
        "# Why: search__exa__find-vp-engineering-profile",
        "## Arm A, trial `search__exa__find-vp-engineering__wM2CQWr`: pass",
        "- Every check passed.",
        "## Arm B, trial `search__exa__find-vp-engineering__WRONGAN`: fail",
        "- Checks not satisfied: answer_ok",
        "  1. **answer** (failed) at `answer`: answer states another authored record's value as the url",
        "  > https://www.linkedin.com/in/priya-raman-4b21a9",
        "## Arm B, trial `search__exa__find-vp-engineering__WITHHLD`: withheld reward (verifier could not assess)",
        "- Host exception: RewardFileNotFoundError: No reward file found at",
    ] {
        assert!(text.contains(line), "missing {line:?} in\n{text}");
    }

    // A trial name selects its task; JSON carries the same typed failures.
    let output = bench_report(&[
        variants,
        "--json".as_ref(),
        "--explain".as_ref(),
        "search__exa__find-vp-engineering__UNRESLV".as_ref(),
    ]);
    assert!(output.status.success());
    let explained: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(explained.as_array().unwrap().len(), 1);
    assert_eq!(explained[0]["status"], "unresolved");
    assert_eq!(
        explained[0]["failed_checks"][0]["disposition"],
        "unresolved"
    );
    assert_eq!(
        explained[0]["failed_checks"][0]["evidence_source"],
        "answer_channel"
    );

    let output = bench_report(&[variants, "--explain".as_ref(), "no-such-task".as_ref()]);
    assert!(!output.status.success() && output.stdout.is_empty());
    let output = bench_report(&[variants, "--explain".as_ref(), "find".as_ref()]);
    assert!(String::from_utf8_lossy(&output.stderr).contains("matches 2 tasks"));
}
