use mock_core::runner::{RunnerAdmission, RunnerPolicy, sha256};
use serde_json::{Value, json};

fn policy() -> RunnerPolicy {
    serde_json::from_value(json!({
        "schema_version": 1,
        "runner_protocol": "tool-bench-runner-v3",
        "verification_protocol": "offline-world-evidence-v5",
        "harbor_version": "0.23.0",
        "adapter_sha256": "a".repeat(64),
    }))
    .unwrap()
}

fn receipt() -> Value {
    json!({
        "schema_version": 1,
        "runner_protocol": "tool-bench-runner-v3",
        "verification_protocol": "offline-world-evidence-v5",
        "scenario": "business/stripe/customer-mrr",
        "session_id": "unique-trial__env",
        "harbor_version": "0.23.0",
        "adapter_sha256": "a".repeat(64),
        "contract_sha256": sha256(b"actual contract"),
        "evidence_sha256": sha256(b"actual world evidence"),
        "main_artifacts_sha256": sha256(b"actual main snapshot"),
        "answer_sha256": sha256(b"actual answer"),
        "manifest_sha256": sha256(b"actual execution manifest"),
        "phase": "stopped",
        "failure": null,
        "main": {
            "container_id": "b".repeat(64),
            "status": "exited",
            "running": false,
            "paused": false,
            "restarting": false,
            "started_at": "2026-01-01T00:00:00Z",
            "finished_at": "2026-01-01T00:01:00Z",
        },
        "collection_checks": 2,
    })
}

#[test]
fn a_shutdown_receipt_requires_the_approved_runner_and_exact_collected_bytes() {
    let admission: RunnerAdmission = serde_json::from_value(receipt()).unwrap();
    admission
        .validate(
            &policy(),
            "business/stripe/customer-mrr",
            b"actual contract",
            b"actual world evidence",
            Some(b"actual answer"),
        )
        .unwrap();
    for (scenario, contract, evidence) in [
        (
            "other/scenario/id",
            b"actual contract".as_slice(),
            b"actual world evidence".as_slice(),
        ),
        (
            "business/stripe/customer-mrr",
            b"changed contract".as_slice(),
            b"actual world evidence".as_slice(),
        ),
        (
            "business/stripe/customer-mrr",
            b"actual contract".as_slice(),
            b"changed world evidence".as_slice(),
        ),
    ] {
        assert!(
            admission
                .validate(
                    &policy(),
                    scenario,
                    contract,
                    evidence,
                    Some(b"actual answer")
                )
                .is_err()
        );
    }
}

#[test]
fn unsupported_runner_versions_protocols_and_unproven_shutdown_are_rejected() {
    for (pointer, changed) in [
        ("/schema_version", json!(2)),
        ("/runner_protocol", json!("unknown")),
        ("/verification_protocol", json!("unknown")),
        ("/harbor_version", json!("unvalidated")),
        ("/adapter_sha256", json!("c".repeat(64))),
        ("/session_id", json!(" ")),
        ("/phase", json!("open")),
        ("/phase", json!("failed")),
        ("/failure", json!("stop failed")),
        ("/main", Value::Null),
        ("/main/container_id", json!("")),
        ("/main/status", json!("running")),
        ("/main/running", json!(true)),
        ("/main/paused", json!(true)),
        ("/main/restarting", json!(true)),
        ("/main/started_at", json!("")),
        ("/main/finished_at", json!("0001-01-01T00:00:00Z")),
        ("/collection_checks", json!(0)),
        ("/evidence_sha256", Value::Null),
        ("/manifest_sha256", Value::Null),
        ("/main_artifacts_sha256", Value::Null),
        ("/main_artifacts_sha256", json!("sha256:wrong")),
        ("/answer_sha256", Value::Null),
        ("/answer_sha256", json!(sha256(b"forged answer"))),
    ] {
        let mut value = receipt();
        *value.pointer_mut(pointer).unwrap() = changed;
        let admission: RunnerAdmission = serde_json::from_value(value).unwrap();
        assert!(
            admission
                .validate(
                    &policy(),
                    "business/stripe/customer-mrr",
                    b"actual contract",
                    b"actual world evidence",
                    Some(b"actual answer"),
                )
                .is_err(),
            "{pointer}"
        );
    }
}

#[test]
fn absent_answers_are_explicit_and_cannot_be_substituted_after_collection() {
    let mut value = receipt();
    value["answer_sha256"] = Value::Null;
    let admission: RunnerAdmission = serde_json::from_value(value).unwrap();
    assert!(
        admission
            .validate(
                &policy(),
                "business/stripe/customer-mrr",
                b"actual contract",
                b"actual world evidence",
                None,
            )
            .is_ok()
    );
    assert!(
        admission
            .validate(
                &policy(),
                "business/stripe/customer-mrr",
                b"actual contract",
                b"actual world evidence",
                Some(b"later fabricated answer"),
            )
            .is_err()
    );
}
