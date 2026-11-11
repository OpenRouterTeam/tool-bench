use std::{
    net::{IpAddr, Ipv4Addr},
    path::Path,
};

use mock_core::{
    dataset::Dataset,
    grading::EVALUATOR_VERSION,
    world::{WorldState, shared},
};
use serde_json::{Value, json};
use world::{PortChoice, spawn};

#[tokio::test]
async fn grades_a_trial_end_to_end_over_http() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/example/github/basic");
    let expect = json!({
        "state": [{"pointer": "/services/github/issues/octo-org~1app#4/title", "equals": "Login page returns 500"}],
        "required_calls": [{"service": "github", "method": "POST", "path": "/api/v3/repos/octo-org/app/issues"}],
        "answer": {"contains": ["#4"]}
    });
    let mut dataset = Dataset::load(&dir).unwrap();
    dataset.contract = serde_json::from_value(expect.clone()).unwrap();
    let state = shared(WorldState::from_dataset(dataset).unwrap());
    let running = spawn(
        state,
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        PortChoice::Ephemeral,
    )
    .await
    .unwrap();
    let http = reqwest::Client::builder().no_proxy().build().unwrap();
    let gh = running.services["github"];

    http.post(format!("http://{gh}/api/v3/repos/octo-org/app/issues"))
        .body(r#"{"title":"Login page returns 500"}"#)
        .send()
        .await
        .unwrap();
    http.post(format!("http://{}/_bench/answer", running.admin))
        .body("Opened issue #4: Login page returns 500")
        .send()
        .await
        .unwrap();

    let grade = |tools_host: &'static str| {
        let http = http.clone();
        let expect = expect.clone();
        let admin = running.admin;
        async move {
            http.post(format!(
                "http://{admin}/_bench/grade?tools_host={tools_host}"
            ))
            .json(&expect)
            .send()
            .await
            .unwrap()
            .json::<Value>()
            .await
            .unwrap()
        }
    };
    let pass = grade("127.0.0.1").await;
    assert_eq!(pass["reward"], 1.0);
    assert_eq!(pass["api_calls"], 1.0);
    assert_eq!(pass["mcp_bypass"], 0.0);

    // From any other "tools" address the same call is a bypass and does not count.
    let bypass = grade("192.0.2.1").await;
    assert_eq!(bypass["reward"], 0.0);
    assert_eq!(bypass["mcp_bypass"], 1.0);
}

#[tokio::test]
async fn assessment_preserves_the_original_world_and_explains_collateral_changes() {
    let directory =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/example/github/basic");
    let contract = json!({"version": 2, "answer": {"contains": ["Maya"]}});
    let mut dataset = Dataset::load(&directory).unwrap();
    dataset.contract = serde_json::from_value(contract.clone()).unwrap();
    let state = shared(WorldState::from_dataset(dataset).unwrap());
    let running = spawn(
        state,
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        PortChoice::Ephemeral,
    )
    .await
    .unwrap();
    let http = reqwest::Client::builder().no_proxy().build().unwrap();
    let admin_url = format!("http://{}", running.admin);
    let health = http
        .get(format!("{admin_url}/_bench/health"))
        .send()
        .await
        .unwrap()
        .json::<Value>()
        .await
        .unwrap();
    assert_eq!(health, json!({"status": "ok"}));
    http.post(format!(
        "http://{}/api/v3/repos/octo-org/app/issues",
        running.services["github"]
    ))
    .json(&json!({"title": "Unrequested issue"}))
    .send()
    .await
    .unwrap()
    .error_for_status()
    .unwrap();
    http.post(format!("{admin_url}/_bench/answer"))
        .body("Maya")
        .send()
        .await
        .unwrap();
    let initial = http
        .get(format!("{admin_url}/_bench/initial_state"))
        .send()
        .await
        .unwrap()
        .json::<Value>()
        .await
        .unwrap();
    assert!(
        initial["services"]["github"]["issues"]
            .get("octo-org/app#4")
            .is_none()
    );
    let assessment = http
        .post(format!(
            "{admin_url}/_bench/assessment?tools_host=127.0.0.1"
        ))
        .json(&contract)
        .send()
        .await
        .unwrap()
        .json::<Value>()
        .await
        .unwrap();
    assert_eq!(assessment["answer_ok"], true);
    assert_eq!(assessment["preservation_checked"], true);
    assert_eq!(assessment["preservation_ok"], false);
    assert_eq!(assessment["reward"], 0.0);
    assert_eq!(assessment["outcome"], "fail");
    assert!(
        assessment["failures"]
            .as_array()
            .unwrap()
            .iter()
            .any(|failure| failure["check"] == "preservation")
    );
    let metrics = http
        .post(format!("{admin_url}/_bench/grade?tools_host=127.0.0.1"))
        .json(&contract)
        .send()
        .await
        .unwrap()
        .json::<Value>()
        .await
        .unwrap();
    assert_eq!(metrics["reward"], assessment["reward"]);
    assert_eq!(metrics["contract_version"], 2.0);
    assert_eq!(metrics["preservation_ok"], 0.0);
}

#[tokio::test]
async fn one_exported_snapshot_produces_the_same_offline_and_live_assessment() {
    let directory =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/example/github/basic");
    let contract: mock_core::grading::Expect = serde_json::from_value(json!({
        "version": 2, "audit_history": true, "answer": {"contains": ["Maya"]}
    }))
    .unwrap();
    let mut dataset = Dataset::load(&directory).unwrap();
    dataset.contract = contract.clone();
    let state = shared(WorldState::from_dataset(dataset).unwrap());
    let running = spawn(
        state,
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        PortChoice::Ephemeral,
    )
    .await
    .unwrap();
    let http = reqwest::Client::builder().no_proxy().build().unwrap();
    let admin = format!("http://{}", running.admin);
    http.post(format!("{admin}/_bench/answer"))
        .body("Maya")
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap();
    let exported: mock_core::evidence::WorldEvidence = http
        .post(format!("{admin}/_bench/finalize?tools_host=127.0.0.1"))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    let offline = exported.assess(&contract, Some("Maya"), false);
    assert_eq!(offline.reward, 1.0, "{offline:?}");
    let live = http
        .post(format!("{admin}/_bench/grade?tools_host=127.0.0.1"))
        .json(&contract)
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json::<Value>()
        .await
        .unwrap();
    assert_eq!(live, serde_json::to_value(offline.metrics()).unwrap());
    assert_eq!(exported.scenario, "example/github/basic");
    assert!(!exported.tools_ips.is_empty());
}

#[tokio::test]
async fn finalization_freezes_the_http_world_and_rejects_reset_and_answer_replacement() {
    let directory =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/example/github/basic");
    let state = shared(WorldState::from_dataset(Dataset::load(&directory).unwrap()).unwrap());
    let running = spawn(
        state,
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        PortChoice::Ephemeral,
    )
    .await
    .unwrap();
    let http = reqwest::Client::builder().no_proxy().build().unwrap();
    let admin = format!("http://{}", running.admin);
    let before: Value = http
        .get(format!("{admin}/_bench/evidence?tools_host=127.0.0.1"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(before["finalization"]["phase"], "open");
    let sealed: Value = http
        .post(format!("{admin}/_bench/finalize?tools_host=127.0.0.1"))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(sealed["finalization"]["phase"], "sealed");
    assert_eq!(
        sealed["schema_version"],
        mock_core::evidence::EVIDENCE_SCHEMA_VERSION
    );
    assert_eq!(sealed["evaluator_version"], EVALUATOR_VERSION);
    let response = http
        .post(format!(
            "http://{}/api/v3/repos/octo-org/app/issues",
            running.services["github"]
        ))
        .json(&json!({"title": "late write"}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 503);
    for path in ["/_bench/reset", "/_bench/answer"] {
        let response = http
            .post(format!("{admin}{path}"))
            .body("replacement")
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 409);
    }
    let repeated: Value = http
        .post(format!("{admin}/_bench/finalize?tools_host=127.0.0.1"))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(sealed, repeated);
}
