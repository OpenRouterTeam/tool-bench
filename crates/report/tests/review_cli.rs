use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};

use mock_core::review::{
    ApprovedReviewer, REVIEW_PROTOCOL, ReviewCase, ReviewPolicy, ReviewerKind,
};
use serde_json::{Value, json};

mod support;

struct Harness {
    root: tempfile::TempDir,
    policy: ReviewPolicy,
    policy_path: PathBuf,
    store: PathBuf,
    instructions: PathBuf,
}

fn review(arguments: &[String]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_bench-review"))
        .args(arguments)
        .output()
        .unwrap()
}

fn success(output: &Output) {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn text(path: &Path) -> String {
    path.to_str().unwrap().into()
}

impl Harness {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
        }
        let mut reviewers = BTreeMap::new();
        for name in ["first", "second"] {
            let private = root.path().join(format!("{name}.key"));
            let public = root.path().join(format!("{name}.public"));
            let output = review(&[
                "keygen".into(),
                "--private-key".into(),
                text(&private),
                "--public-key".into(),
                text(&public),
            ]);
            success(&output);
            let secret: Value = serde_json::from_slice(&fs::read(&private).unwrap()).unwrap();
            assert!(
                !String::from_utf8_lossy(&output.stdout).contains(secret["seed"].as_str().unwrap())
            );
            let public: Value = serde_json::from_slice(&fs::read(public).unwrap()).unwrap();
            reviewers.insert(
                name.into(),
                ApprovedReviewer {
                    public_key: public["public_key"].as_str().unwrap().into(),
                    kind: ReviewerKind::ScriptedFixture,
                },
            );
        }
        let policy = ReviewPolicy {
            schema_version: 1,
            protocol: REVIEW_PROTOCOL.into(),
            reviewers,
        };
        let policy_path = root.path().join("policy.json");
        fs::write(&policy_path, serde_json::to_vec(&policy).unwrap()).unwrap();
        let store = root.path().join("reviews");
        fs::create_dir(&store).unwrap();
        let instructions = root.path().join("tasks");
        fs::create_dir_all(instructions.join("example__github__basic")).unwrap();
        fs::write(
            instructions.join("example__github__basic/instruction.md"),
            "instruction",
        )
        .unwrap();
        Self {
            root,
            policy,
            policy_path,
            store,
            instructions,
        }
    }

    fn trial(&self, name: &str, known: bool) -> PathBuf {
        let output = self.root.path().join("trials");
        support::write_reviewable(&output, name, "example__github__basic", &self.policy, known);
        output.join(name)
    }

    fn export(&self, trial: &Path) -> ReviewCase {
        let output = self.root.path().join(format!(
            "{}-case.json",
            trial.file_name().unwrap().to_str().unwrap()
        ));
        success(&review(&[
            "export".into(),
            "--trial".into(),
            text(trial),
            "--instruction".into(),
            text(
                &self
                    .instructions
                    .join("example__github__basic/instruction.md"),
            ),
            "--policy".into(),
            text(&self.policy_path),
            "--out".into(),
            text(&output),
        ]));
        serde_json::from_slice(&fs::read(output).unwrap()).unwrap()
    }

    fn sign(&self, case: &ReviewCase, name: &str, decision: &str) -> PathBuf {
        let digest = case.sha256(&self.policy).unwrap();
        let digest = digest.strip_prefix("sha256:").unwrap();
        let directory = self.store.join(digest);
        fs::create_dir_all(&directory).unwrap();
        let case_path = self.root.path().join(format!("{digest}-sign-case.json"));
        fs::write(&case_path, serde_json::to_vec(case).unwrap()).unwrap();
        let decisions:serde_json::Map<_,_>=case.issues.keys().map(|identity|(
            identity.clone(),json!({"decision":decision,"rationale":"Scripted CLI control, not an operational human assessment."})
        )).collect();
        let draft = self.root.path().join(format!("{digest}-{name}-draft.json"));
        fs::write(&draft, serde_json::to_vec(&decisions).unwrap()).unwrap();
        let output = directory.join(format!("{name}.json"));
        success(&review(&[
            "sign".into(),
            "--case".into(),
            text(&case_path),
            "--policy".into(),
            text(&self.policy_path),
            "--reviewer".into(),
            name.into(),
            "--private-key".into(),
            text(&self.root.path().join(format!("{name}.key"))),
            "--decisions".into(),
            text(&draft),
            "--out".into(),
            text(&output),
        ]));
        output
    }

    fn report(&self, left: &Path, right: &Path, reviewed: bool) -> Output {
        let mut command = Command::new(env!("CARGO_BIN_EXE_bench-report"));
        command.arg(left).arg(right).arg("--json");
        if reviewed {
            command
                .arg("--review-policy")
                .arg(&self.policy_path)
                .arg("--review-store")
                .arg(&self.store)
                .arg("--instructions-root")
                .arg(&self.instructions);
        }
        command.output().unwrap()
    }
}

#[test]
fn cli_export_sign_verify_and_report_preserve_raw_files_and_label_effective_controls() {
    let harness = Harness::new();
    let left = harness.trial("left", false);
    let right = harness.trial("right", false);
    let originals: Vec<_> = [&left, &right]
        .into_iter()
        .map(|path| fs::read(path.join("result.json")).unwrap())
        .collect();
    for (trial, decision) in [(&left, "satisfied"), (&right, "violated")] {
        let case = harness.export(trial);
        for name in ["first", "second"] {
            harness.sign(&case, name, decision);
        }
        let output = harness.root.path().join(format!(
            "{}-effective.json",
            trial.file_name().unwrap().to_str().unwrap()
        ));
        success(&review(&[
            "verify".into(),
            "--trial".into(),
            text(trial),
            "--instruction".into(),
            text(
                &harness
                    .instructions
                    .join("example__github__basic/instruction.md"),
            ),
            "--policy".into(),
            text(&harness.policy_path),
            "--review-store".into(),
            text(&harness.store),
            "--out".into(),
            text(&output),
        ]));
        let artifact: Value = serde_json::from_slice(&fs::read(output).unwrap()).unwrap();
        assert_eq!(artifact["raw"]["outcome"], "unresolved");
        assert_eq!(
            artifact["effective"]["outcome"],
            if decision == "satisfied" {
                "pass"
            } else {
                "fail"
            }
        );
    }
    let raw = harness.report(&left, &right, false);
    assert!(!raw.status.success() && raw.stdout.is_empty());
    let output = harness.report(&left, &right, true);
    success(&output);
    let comparison: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        comparison["admission_level"],
        "source_bound_scripted_review_controls_only"
    );
    assert_eq!(comparison["mean_a"], 1.0);
    assert_eq!(comparison["mean_b"], 0.0);
    assert_eq!(
        comparison["content_review"]["raw_outcomes_a"]["unresolved"],
        1
    );
    assert_eq!(
        comparison["content_review"]["raw_outcomes_b"]["unresolved"],
        1
    );
    assert_eq!(
        comparison["content_review"]["trials_a"][0]["reviewers"],
        json!(["first", "second"])
    );
    for (trial, original) in [&left, &right].into_iter().zip(originals) {
        assert_eq!(fs::read(trial.join("result.json")).unwrap(), original);
    }
}

#[test]
fn missing_signatures_and_mutated_signed_decisions_cannot_publish_json() {
    let harness = Harness::new();
    let trial = harness.trial("trial", false);
    let case = harness.export(&trial);
    harness.sign(&case, "first", "satisfied");
    let missing = harness.report(&trial, &trial, true);
    assert!(!missing.status.success() && missing.stdout.is_empty());
    let second = harness.sign(&case, "second", "satisfied");
    success(&harness.report(&trial, &trial, true));
    let mut signed: Value = serde_json::from_slice(&fs::read(&second).unwrap()).unwrap();
    let decisions = signed["statement"]["decisions"].as_object_mut().unwrap();
    decisions.values_mut().next().unwrap()["decision"] = json!("violated");
    fs::write(second, serde_json::to_vec(&signed).unwrap()).unwrap();
    let forged = harness.report(&trial, &trial, true);
    assert!(!forged.status.success() && forged.stdout.is_empty());
    assert!(String::from_utf8_lossy(&forged.stderr).contains("signature"));
}

#[test]
fn known_failures_survive_consensus_and_pending_content_is_not_hidden_by_failure() {
    let harness = Harness::new();
    let trial = harness.trial("known", true);
    let case = harness.export(&trial);
    let pending = harness.report(&trial, &trial, true);
    assert!(!pending.status.success() && pending.stdout.is_empty());
    for name in ["first", "second"] {
        harness.sign(&case, name, "satisfied");
    }
    let output = harness.report(&trial, &trial, true);
    success(&output);
    let comparison: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(comparison["mean_a"], 0.0);
    assert_eq!(comparison["content_review"]["raw_outcomes_a"]["fail"], 1);
}

#[test]
fn agent_controlled_approval_files_cached_summaries_and_internal_stores_are_not_authority() {
    let harness = Harness::new();
    let trial = harness.trial("trial", false);
    fs::create_dir_all(trial.join("artifacts/reviews")).unwrap();
    fs::write(
        trial.join("verifier/adjudication.json"),
        r#"{"approved":true,"reward":1}"#,
    )
    .unwrap();
    fs::write(
        trial.join("artifacts/reviews/result.json"),
        r#"{"approved":true,"reward":1}"#,
    )
    .unwrap();
    let no_flag = harness.report(&trial, &trial, false);
    assert!(!no_flag.status.success() && no_flag.stdout.is_empty());
    let output = Command::new(env!("CARGO_BIN_EXE_bench-report"))
        .arg(&trial)
        .arg(&trial)
        .arg("--json")
        .arg("--review-policy")
        .arg(&harness.policy_path)
        .arg("--review-store")
        .arg(trial.join("artifacts/reviews"))
        .arg("--instructions-root")
        .arg(&harness.instructions)
        .output()
        .unwrap();
    assert!(!output.status.success() && output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains("outside"));
    let case = harness.export(&trial);
    let directory = harness.store.join(
        case.sha256(&harness.policy)
            .unwrap()
            .trim_start_matches("sha256:"),
    );
    fs::create_dir_all(&directory).unwrap();
    fs::write(
        directory.join("first.json"),
        r#"{"approved":true,"reward":1}"#,
    )
    .unwrap();
    let forged = harness.report(&trial, &trial, true);
    assert!(!forged.status.success() && forged.stdout.is_empty());
}

#[test]
fn keys_and_case_exports_are_create_only_and_do_not_overwrite_collected_data() {
    let harness = Harness::new();
    let private = harness.root.path().join("first.key");
    let original = fs::read(&private).unwrap();
    let output = review(&[
        "keygen".into(),
        "--private-key".into(),
        text(&private),
        "--public-key".into(),
        text(&harness.root.path().join("another.public")),
    ]);
    assert!(!output.status.success());
    assert_eq!(fs::read(private).unwrap(), original);
    let trial = harness.trial("trial", false);
    let original = fs::read(trial.join("result.json")).unwrap();
    let output = review(&[
        "export".into(),
        "--trial".into(),
        text(&trial),
        "--instruction".into(),
        text(
            &harness
                .instructions
                .join("example__github__basic/instruction.md"),
        ),
        "--policy".into(),
        text(&harness.policy_path),
        "--out".into(),
        text(&trial.join("result.json")),
    ]);
    assert!(!output.status.success());
    assert_eq!(fs::read(trial.join("result.json")).unwrap(), original);
}

#[cfg(unix)]
#[test]
fn exposed_private_keys_and_symlinked_review_files_fail_closed() {
    use std::os::unix::fs::{PermissionsExt, symlink};
    let harness = Harness::new();
    let trial = harness.trial("trial", false);
    let case = harness.export(&trial);
    let first = harness.sign(&case, "first", "satisfied");
    let second = harness.sign(&case, "second", "satisfied");
    let saved = harness.root.path().join("saved-review.json");
    fs::rename(&second, &saved).unwrap();
    symlink(&saved, &second).unwrap();
    let output = harness.report(&trial, &trial, true);
    assert!(!output.status.success() && output.stdout.is_empty());
    fs::remove_file(&second).unwrap();
    fs::rename(saved, &second).unwrap();
    success(&harness.report(&trial, &trial, true));
    fs::set_permissions(
        harness.root.path().join("first.key"),
        fs::Permissions::from_mode(0o644),
    )
    .unwrap();
    let case_path = harness.root.path().join("exposed-case.json");
    fs::write(&case_path, serde_json::to_vec(&case).unwrap()).unwrap();
    let decisions: serde_json::Map<_, _> = case
        .issues
        .keys()
        .map(|identity| {
            (
                identity.clone(),
                json!({"decision":"satisfied","rationale":"fixture"}),
            )
        })
        .collect();
    let draft = harness.root.path().join("exposed-draft.json");
    fs::write(&draft, serde_json::to_vec(&decisions).unwrap()).unwrap();
    let output = review(&[
        "sign".into(),
        "--case".into(),
        text(&case_path),
        "--policy".into(),
        text(&harness.policy_path),
        "--reviewer".into(),
        "first".into(),
        "--private-key".into(),
        text(&harness.root.path().join("first.key")),
        "--decisions".into(),
        text(&draft),
        "--out".into(),
        text(&harness.root.path().join("must-not-exist.json")),
    ]);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("permissions"));
    assert!(first.exists());
}

#[test]
fn raw_and_review_assisted_conditions_cannot_be_mixed_or_share_a_release_label() {
    let harness = Harness::new();
    let root = harness.root.path().join("resolved");
    support::write_resolved_with_policy(&root, "trial", "example__github__basic", &harness.policy);
    let raw = report::load_trials(&root).unwrap();
    let reviewed = report::review::coordinator::load_trials(
        &root,
        &harness.policy,
        &harness.store,
        &harness.instructions,
    )
    .unwrap();
    assert!(report::compare(&raw, &reviewed).is_err());
    let raw_report = report::compare(&raw, &raw).unwrap();
    let reviewed_report = report::compare(&reviewed, &reviewed).unwrap();
    assert_ne!(raw_report.release_sha256, reviewed_report.release_sha256);
    assert!(raw_report.content_review.is_none());
    assert_eq!(reviewed_report.content_review.unwrap().reviewed_trials_a, 0);
    let partial = Command::new(env!("CARGO_BIN_EXE_bench-report"))
        .arg(&root)
        .arg(&root)
        .arg("--json")
        .arg("--review-policy")
        .arg(&harness.policy_path)
        .output()
        .unwrap();
    assert!(!partial.status.success() && partial.stdout.is_empty());
}

#[test]
fn one_unreviewed_attempt_blocks_the_whole_comparison_without_dropping_it() {
    let harness = Harness::new();
    let left = harness.root.path().join("left-arm");
    let right = harness.root.path().join("right-arm");
    for root in [&left, &right] {
        for name in ["first-attempt", "second-attempt"] {
            let trial_name = format!("{}-{name}", if root == &left { "left" } else { "right" });
            support::write_reviewable(
                root,
                &trial_name,
                "example__github__basic",
                &harness.policy,
                false,
            );
            if root == &right && name == "second-attempt" {
                continue;
            }
            let case =
                report::review::prepare(&root.join(trial_name), "instruction", &harness.policy)
                    .unwrap();
            for reviewer in ["first", "second"] {
                harness.sign(case.case(), reviewer, "satisfied");
            }
        }
    }
    let output = harness.report(&left, &right, true);
    assert!(!output.status.success() && output.stdout.is_empty());
    let trials = report::review::coordinator::load_trials(
        &right,
        &harness.policy,
        &harness.store,
        &harness.instructions,
    )
    .unwrap();
    assert_eq!(trials.len(), 2);
    assert!(
        trials
            .iter()
            .any(|trial| trial.outcome == report::Outcome::Unresolved)
    );
}

#[test]
fn oversized_policies_and_unknown_coordinator_options_fail_before_outputs() {
    let harness = Harness::new();
    let oversized = harness.root.path().join("too-large-policy.json");
    fs::write(&oversized, " ".repeat(64 * 1024 + 1)).unwrap();
    assert!(report::review::coordinator::read_policy(&oversized).is_err());
    let output = review(&[
        "keygen".into(),
        "--private-key".into(),
        "unused".into(),
        "--unrecognized".into(),
        "value".into(),
    ]);
    assert!(!output.status.success());
}
