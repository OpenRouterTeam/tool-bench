//! Report loading and explanation of trials captured at the host provider proxy.
//!
//! The inference documents are real proxy output sealed by the runner
//! (`deploy/runner/test_proxy_capture.py` keeps the fixtures equal to live output).

use std::{fs, path::Path, process::Command};

use mock_core::runner::sha256;
use report::load_trials;
use serde_json::{Value, json};

mod support;

fn read(root: &Path, name: &str) -> Value {
    serde_json::from_slice(&fs::read(root.join(name)).unwrap()).unwrap()
}

fn write(root: &Path, name: &str, value: &Value) {
    fs::write(root.join(name), serde_json::to_vec(value).unwrap()).unwrap();
}

fn capture(name: &str) -> Vec<u8> {
    fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../mock-core/tests/fixtures/proxy-capture")
            .join(format!("{name}.json")),
    )
    .unwrap()
}

/// A trial directory whose verifier outputs come from the real `bench-verify`.
fn proxied_trial(root: &Path, name: &str) {
    support::write_verified_final_response(root, "trial", "example__github__basic", true);
    let trial = root.join("trial");
    let verifier = trial.join("verifier");
    let inference_bytes = capture(name);
    let inference: Value = serde_json::from_slice(&inference_bytes).unwrap();
    let mut configured = inference["identity"].clone();
    configured["observed_model"] = Value::Null;
    let mut manifest = read(&verifier, "realized-manifest.json");
    manifest["execution_kind"] = json!("native");
    manifest["agent"]["name"] = json!(mock_core::transport::PROXY_AGENT_NAME);
    manifest["inference_identity"] = configured;
    write(&verifier, "realized-manifest.json", &manifest);
    fs::write(verifier.join("inference-evidence.json"), &inference_bytes).unwrap();
    fs::write(
        trial.join("artifacts/world/inference-evidence.json"),
        &inference_bytes,
    )
    .unwrap();
    // The container wrote a different answer than the provider returned.
    fs::write(trial.join("artifacts/agent/answer.md"), "Mallory").unwrap();
    let mut policy = read(&verifier, "runner-policy.json");
    policy["proxy_inference_adapter"] = inference["adapter"].clone();
    write(&verifier, "runner-policy.json", &policy);
    let mut admission = read(&verifier, "runner-admission.json");
    admission["inference_evidence_sha256"] = json!(sha256(&inference_bytes));
    admission["manifest_sha256"] = json!(sha256(
        &fs::read(verifier.join("realized-manifest.json")).unwrap()
    ));
    admission["answer_sha256"] = json!(sha256(b"Mallory"));
    write(&verifier, "runner-admission.json", &admission);
    // Grade exactly as `bench-verify` does: bound capture, declared answer channel,
    // then the structured inference classification.
    let contract: mock_core::grading::Expect =
        serde_json::from_slice(&fs::read(verifier.join("contract.json")).unwrap()).unwrap();
    let evidence: mock_core::evidence::WorldEvidence =
        serde_json::from_slice(&fs::read(trial.join("artifacts/world/evidence.json")).unwrap())
            .unwrap();
    let captured: mock_core::inference::InferenceEvidence =
        serde_json::from_slice(&inference_bytes).unwrap();
    let answer = captured.final_text();
    let mut assessed = evidence.assess(&contract, answer, false);
    captured.apply_to_grade(&mut assessed);
    assessed.admit_runner(Ok(()));
    write(&verifier, "assessment.json", &json!(assessed));
    let mut verification = read(&verifier, "verification.json");
    verification["inference_evidence_sha256"] = admission["inference_evidence_sha256"].clone();
    verification["manifest_sha256"] = admission["manifest_sha256"].clone();
    verification["answer_sha256"] = admission["answer_sha256"].clone();
    verification["runner_admission_sha256"] = json!(sha256(
        &fs::read(verifier.join("runner-admission.json")).unwrap()
    ));
    verification["runner_policy_sha256"] = json!(sha256(
        &fs::read(verifier.join("runner-policy.json")).unwrap()
    ));
    verification["inference_classification"] = inference["classification"].clone();
    verification["answer_source"] = json!("native_final_response");
    verification["graded_answer_sha256"] = json!(answer.map(|text| sha256(text.as_bytes())));
    write(&verifier, "verification.json", &verification);
    let mut result = read(&trial, "result.json");
    result["agent_info"]["name"] = json!(mock_core::transport::PROXY_AGENT_NAME);
    result["verifier_result"]["rewards"] = json!(assessed.metrics());
    write(&trial, "result.json", &result);
}

fn summary(root: &Path) -> Value {
    let output = Command::new(env!("CARGO_BIN_EXE_bench-report"))
        .args(["--trials", root.to_str().unwrap(), "--json"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

#[test]
fn report_grades_the_provider_reply_and_rejects_substituted_proxy_captures() {
    let directory = tempfile::tempdir().unwrap();
    proxied_trial(directory.path(), "completed");
    let trials = load_trials(directory.path()).unwrap();
    assert_eq!(trials[0].reward, 1.0);
    let captured = trials[0]
        .provenance
        .as_ref()
        .unwrap()
        .inference
        .as_ref()
        .unwrap();
    assert_eq!(captured.final_text(), Some("Maya"));
    assert_eq!(
        captured.adapter.name,
        mock_core::transport::PROXY_ADAPTER_NAME
    );
    for mutation in ["artifact", "another_trial", "relabelled"] {
        let directory = tempfile::tempdir().unwrap();
        proxied_trial(directory.path(), "completed");
        let trial = directory.path().join("trial");
        let bytes = match mutation {
            // A look-alike document where the collected artifact should be.
            "artifact" => {
                fs::write(
                    trial.join("artifacts/world/inference-evidence.json"),
                    capture("recovered_rate_limit"),
                )
                .unwrap();
                continue_check(directory.path(), mutation);
                continue;
            }
            // A genuine capture from a different session, receipt re-pointed at it.
            "another_trial" => {
                let mut other: Value = serde_json::from_slice(&capture("completed")).unwrap();
                other["session_id"] = json!("another-trial__env");
                serde_json::to_vec(&other).unwrap()
            }
            _ => {
                let mut other: Value =
                    serde_json::from_slice(&capture("no_public_response")).unwrap();
                other["classification"] = json!("completed");
                serde_json::to_vec(&other).unwrap()
            }
        };
        for path in [
            "verifier/inference-evidence.json",
            "artifacts/world/inference-evidence.json",
        ] {
            fs::write(trial.join(path), &bytes).unwrap();
        }
        let mut admission = read(&trial, "verifier/runner-admission.json");
        admission["inference_evidence_sha256"] = json!(sha256(&bytes));
        write(&trial, "verifier/runner-admission.json", &admission);
        let mut verification = read(&trial, "verifier/verification.json");
        verification["inference_evidence_sha256"] = json!(sha256(&bytes));
        verification["runner_admission_sha256"] = json!(sha256(
            &fs::read(trial.join("verifier/runner-admission.json")).unwrap()
        ));
        write(&trial, "verifier/verification.json", &verification);
        continue_check(directory.path(), mutation);
    }
}

fn continue_check(root: &Path, mutation: &str) {
    assert!(load_trials(root).is_err(), "{mutation}");
}

#[test]
fn a_failed_trial_is_explained_from_the_report_alone() {
    for (name, outcome, label, domain, failed_source) in [
        (
            "first_turn_context_rejection",
            "context_overflow",
            "first_turn_context_rejection",
            "agent",
            "inference_evidence",
        ),
        (
            "later_turn_context_rejection",
            "context_overflow",
            "later_turn_context_rejection",
            "agent",
            "inference_evidence",
        ),
        (
            "no_public_response",
            "fail",
            "no_public_response",
            "agent",
            "world_evidence",
        ),
        (
            "unrecovered_rate_limit",
            "environment_error",
            "unrecovered_rate_limit",
            "infrastructure",
            "inference_evidence",
        ),
        (
            "capture_gap",
            "environment_error",
            "failed_finalization",
            "infrastructure",
            "inference_evidence",
        ),
    ] {
        let directory = tempfile::tempdir().unwrap();
        proxied_trial(directory.path(), name);
        let summary = summary(directory.path());
        let trial = &summary["trials"][0];
        let explanation = &trial["explanation"];
        assert_eq!(trial["outcome"], outcome, "{name}");
        assert_eq!(explanation["verified"], true, "{name}: {explanation}");
        assert_eq!(explanation["failure_class"]["label"], label, "{name}");
        assert_eq!(explanation["failure_class"]["domain"], domain, "{name}");
        assert_eq!(explanation["answer_channel"], "native_final_response");
        let checks = explanation["failed_checks"].as_array().unwrap();
        assert!(!checks.is_empty(), "{name}");
        if failed_source == "inference_evidence" {
            assert!(
                checks
                    .iter()
                    .any(|check| check["evidence_source"] == failed_source
                        && check["location"]
                            .as_str()
                            .unwrap()
                            .starts_with("inference/")),
                "{name}: {checks:?}"
            );
        }
        assert!(explanation["evidence"]["inference"].is_string(), "{name}");
        let accounting = &explanation["accounting"];
        let unknown: Vec<&str> = accounting["unknown"]
            .as_array()
            .unwrap()
            .iter()
            .map(|value| value.as_str().unwrap())
            .collect();
        // A rejected or failed request reported no usage: unknown, never zero.
        for (field, value) in accounting.as_object().unwrap() {
            if unknown.contains(&field.as_str()) {
                assert!(value.is_null(), "{name}: {field}");
            }
        }
        assert!(unknown.contains(&"cost_usd"), "{name}");
        if name == "capture_gap" {
            assert_eq!(
                explanation["capture_faults"],
                json!(["unobserved_assistant_turn"])
            );
            assert!(unknown.contains(&"retry_attempts"));
        } else {
            assert!(accounting["provider_latency_ms"].is_u64(), "{name}");
            assert!(explanation["terminal_exchange"]["request_sha256"].is_string());
        }
        if name == "unrecovered_rate_limit" {
            assert_eq!(accounting["retry_attempts"], 1);
            assert_eq!(explanation["terminal_exchange"]["failure"], "rate_limit");
            assert_eq!(explanation["terminal_exchange"]["status"], 429);
        }
    }
}

#[test]
fn a_recovered_rate_limit_keeps_its_result_and_reports_the_retry() {
    let directory = tempfile::tempdir().unwrap();
    proxied_trial(directory.path(), "recovered_rate_limit");
    let summary = summary(directory.path());
    let trial = &summary["trials"][0];
    assert_eq!(trial["reward"], 1.0);
    assert_eq!(trial["inference"]["classification"], "recovered_rate_limit");
    let explanation = &trial["explanation"];
    assert_eq!(explanation["failure_class"]["label"], "pass");
    assert_eq!(explanation["accounting"]["retry_attempts"], 1);
    assert_eq!(explanation["accounting"]["retried_turns"], 1);
    // The rate-limited attempt reported no usage, so retry cost is unknown, not zero.
    assert!(explanation["accounting"]["retry_cost_usd"].is_null());
    assert!(explanation["accounting"]["final_response_latency_ms"].is_u64());
}

#[test]
fn an_unbindable_capture_is_explained_as_unverified_not_relabelled() {
    let directory = tempfile::tempdir().unwrap();
    proxied_trial(directory.path(), "resumed_session");
    // The verifier rejects a resumed session at binding; mirror its saved outcome.
    let trial = directory.path().join("trial");
    let mut result = read(&trial, "result.json");
    result["verifier_result"]["rewards"]["outcome_code"] = json!(5.0);
    result["verifier_result"]["rewards"]["reward"] = json!(0.0);
    write(&trial, "result.json", &result);
    let summary = summary(directory.path());
    let explanation = &summary["trials"][0]["explanation"];
    assert_eq!(summary["trials"][0]["outcome"], "environment_error");
    assert_eq!(explanation["verified"], false);
    assert!(
        explanation["unverified_reason"]
            .as_str()
            .unwrap()
            .contains("resumed native sessions")
    );
    assert!(explanation["failure_class"].is_null());
}
