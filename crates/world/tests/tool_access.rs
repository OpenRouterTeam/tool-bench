//! The tools container reports each MCP request it receives to the world, which seals
//! the reports with its evidence. The verifier then says how many clients reached the
//! tool servers and how many requests came from one other than the first.

use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr},
    path::Path,
};

use mock_core::{
    dataset::Dataset,
    evidence::WorldEvidence,
    grading::Expect,
    world::{WorldState, shared},
};
use serde_json::{Value, json};
use world::{PortChoice, TOOL_ACCESS_PATH};

fn local(mut address: SocketAddr) -> SocketAddr {
    address.set_ip(IpAddr::V4(Ipv4Addr::LOCALHOST));
    address
}

fn client() -> reqwest::Client {
    reqwest::Client::builder().no_proxy().build().unwrap()
}

fn request(session: &str, method: &str, agent: &str, name: Option<&str>) -> Value {
    let mut report = json!({
        "endpoint": "github", "event": "request", "peer": "172.20.0.4",
        "user_agent": agent, "session": session, "method": method
    });
    if let Some(name) = name {
        report["client"] = json!(name);
    }
    report
}

#[tokio::test]
async fn reports_are_sealed_with_the_evidence_and_summarized_in_the_result() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/example/github/basic");
    let dataset = Dataset::load(&path).unwrap();
    let contract: Expect = dataset.contract.clone();
    let running = world::spawn_with_access(
        shared(WorldState::from_dataset(dataset).unwrap()),
        IpAddr::V4(Ipv4Addr::UNSPECIFIED),
        PortChoice::Ephemeral,
        world::AccessPolicy {
            admin_token: Some("trial-secret".into()),
            tools_host: None,
            bootstrap_bind: Some(IpAddr::V4(Ipv4Addr::LOCALHOST)),
        },
    )
    .await
    .unwrap();
    let http = client();
    let report_url = format!("http://{}{TOOL_ACCESS_PATH}", running.bootstrap);
    let admin = format!("http://{}", local(running.admin));

    // The harness connects first; later the shell opens a session of its own.
    for report in [
        json!({"endpoint": "github", "event": "registered"}),
        request(
            "tb-000001",
            "initialize",
            "claude-code/2.1.235",
            Some("claude-code"),
        ),
        request("tb-000001", "tools/list", "claude-code/2.1.235", None),
        request("tb-000002", "initialize", "curl/8.5.0", Some("shell")),
        request("tb-000002", "tools/call", "curl/8.5.0", None),
        json!({"endpoint": "github", "event": "tool_error"}),
        request("tb-000001", "tools/call", "claude-code/2.1.235", None),
    ] {
        let response = http.post(&report_url).json(&report).send().await.unwrap();
        assert_eq!(response.status(), 204, "{report}");
    }
    // A report is not a world request: it is neither journaled nor an API call.
    let journal: Value = http
        .get(format!("{admin}/_bench/journal"))
        .bearer_auth("trial-secret")
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(journal["entries"], json!([]));

    let evidence: WorldEvidence = http
        .post(format!("{admin}/_bench/finalize?tools_host=localhost"))
        .bearer_auth("trial-secret")
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(evidence.tool_access.len(), 7);
    assert_eq!(evidence.tool_access[3].seq, 4);
    assert_eq!(
        evidence.tool_access[3].report.user_agent.as_deref(),
        Some("curl/8.5.0")
    );

    // Sealed evidence takes no more reports.
    let late = http
        .post(&report_url)
        .json(&request(
            "tb-000003",
            "initialize",
            "curl/8.5.0",
            Some("shell"),
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(late.status(), 409);

    let assessment = evidence.assess(&contract, None, false);
    let servers = assessment.tool_servers.as_ref().unwrap();
    assert!(servers.observed);
    assert_eq!((servers.requests, servers.tool_calls), (5, 2));
    assert_eq!(servers.tool_call_errors, 1);
    assert_eq!(servers.clients.len(), 2);
    assert_eq!(servers.outside_requests, 2);
    let metrics = assessment.metrics();
    assert_eq!(metrics["tool_server_observed"], 1.0);
    assert_eq!(metrics["tool_server_clients"], 2.0);
    assert_eq!(metrics["tool_server_outside_requests"], 2.0);
    assert_eq!(metrics["tool_server_side_channel"], 1.0);
    assert_eq!(metrics["tool_call_errors"], 1.0);
    // The flag is a finding about the trial, not a failure of it.
    assert!(assessment.integrity_ok);
    assert_eq!(metrics["mcp_bypass"], 0.0);
}

#[tokio::test]
async fn a_malformed_report_is_refused_and_keeps_nothing() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/example/github/basic");
    let state = shared(WorldState::from_dataset(Dataset::load(&path).unwrap()).unwrap());
    let running = world::spawn(
        state.clone(),
        IpAddr::V4(Ipv4Addr::UNSPECIFIED),
        PortChoice::Ephemeral,
    )
    .await
    .unwrap();
    let url = format!("http://{}{TOOL_ACCESS_PATH}", local(running.bootstrap));
    let response = client()
        .post(&url)
        .json(&json!({"endpoint": "github", "event": "deleted"}))
        .send()
        .await
        .unwrap();
    assert!(response.status().is_client_error(), "{}", response.status());
    assert!(state.lock().unwrap().tool_access().is_empty());
}
