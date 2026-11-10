use std::{
    net::{IpAddr, Ipv4Addr},
    path::Path,
};

use catalog::Catalog;
use mock_core::{
    dataset::Dataset,
    journal::JournalEntry,
    world::{WorldState, shared},
};
use world::{PortChoice, replay::replay, spawn};

#[tokio::test]
async fn a_recorded_session_replays_identically_including_tls_and_form_bodies() {
    let dir =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/code/github/file-login-bug");
    let state = shared(WorldState::from_dataset(Dataset::load(&dir).unwrap()).unwrap());
    let running = spawn(
        state.clone(),
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        PortChoice::Ephemeral,
    )
    .await
    .unwrap();
    let http = reqwest::Client::builder().no_proxy().build().unwrap();
    let gh = running.services["github"];
    let okta = running.services["okta"];
    http.get(format!(
        "http://{gh}/api/v3/repos/octo-org/app/issues?state=all&per_page=2"
    ))
    .header("x-forwarded-proto", "https")
    .header("host", "github.octo-corp.com")
    .send()
    .await
    .unwrap();
    http.post(format!("http://{gh}/api/v3/repos/octo-org/app/issues"))
        .header("content-type", "application/json")
        .body(r#"{"title":"Replay me","body":"x"}"#)
        .send()
        .await
        .unwrap();
    http.post(format!("http://{okta}/oauth2/v1/token"))
        .header("content-type", "application/x-www-form-urlencoded")
        .body("grant_type=refresh_token")
        .send()
        .await
        .unwrap();
    http.get(format!("http://{gh}/nope")).send().await.unwrap();

    let entries: Vec<JournalEntry> = state.lock().unwrap().journal.entries().to_vec();
    assert_eq!(entries.len(), 4);
    assert_eq!(entries[0].scheme, "https");
    assert_eq!(entries[1].content_type.as_deref(), Some("application/json"));
    assert_eq!(replay(&dir, &entries).await.unwrap(), vec![]);

    // A tampered recording is caught.
    let mut tampered = entries.clone();
    tampered[1].response_body["number"] = serde_json::json!(99);
    let mismatches = replay(&dir, &tampered).await.unwrap();
    assert_eq!(mismatches.len(), 1);
    assert_eq!(mismatches[0].seq, 2);
    assert_eq!(
        mismatches[0].kind,
        world::replay::ReplayIssue::ResponseMismatch
    );
    let mut nonserial = entries;
    nonserial[0].operation_id = Some(1000);
    let issues = replay(&dir, &nonserial).await.unwrap();
    assert!(!issues.is_empty());
    assert!(
        issues
            .iter()
            .all(|issue| issue.kind == world::replay::ReplayIssue::ContextUnavailable)
    );
}

#[tokio::test]
async fn authenticated_firecrawl_requests_replay_with_exact_bodies_and_declared_fixture_headers() {
    let catalog =
        Catalog::load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../catalog/services.toml"))
            .unwrap();
    let credential = catalog.launch("firecrawl").unwrap().env["FIRECRAWL_API_KEY"].clone();
    let directory = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../scenarios/search/firecrawl/northwind-team-pricing");
    let state = shared(WorldState::from_dataset(Dataset::load(&directory).unwrap()).unwrap());
    let running = spawn(
        state.clone(),
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        PortChoice::Ephemeral,
    )
    .await
    .unwrap();
    let http = reqwest::Client::builder().no_proxy().build().unwrap();
    let endpoint = format!("http://{}/v2/search", running.services["firecrawl"]);
    for (sources, expected) in [
        (serde_json::json!(["web", "alexandria"]), 403),
        (serde_json::json!(["web"]), 200),
    ] {
        let response = http.post(&endpoint)
            .bearer_auth(&credential)
            .header("content-type", "application/json")
            .body(format!(" {{ \"query\": \"Northwind Analytics pricing Team plan\", \"sources\": {sources} }} "))
            .send().await.unwrap();
        assert_eq!(response.status().as_u16(), expected);
    }
    let entries = state.lock().unwrap().journal.entries().to_vec();
    assert_eq!(entries.len(), 2);
    let recorded_headers =
        serde_json::to_string(&entries[0].replay.as_ref().unwrap().headers).unwrap();
    assert!(!recorded_headers.contains(&credential));
    let mismatches = replay(&directory, &entries).await.unwrap();
    assert!(mismatches.is_empty(), "{mismatches:?}");
}

#[tokio::test]
async fn unavailable_credentials_or_legacy_context_are_not_reported_as_backend_nondeterminism() {
    let directory =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/example/github/basic");
    let state = shared(WorldState::from_dataset(Dataset::load(&directory).unwrap()).unwrap());
    let running = spawn(
        state.clone(),
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        PortChoice::Ephemeral,
    )
    .await
    .unwrap();
    reqwest::Client::builder()
        .no_proxy()
        .build()
        .unwrap()
        .get(format!(
            "http://{}/v2/search",
            running.services["firecrawl"]
        ))
        .header("authorization", "Bearer private-unregistered-value")
        .send()
        .await
        .unwrap();
    let entries = state.lock().unwrap().journal.entries().to_vec();
    assert!(
        !serde_json::to_string(&entries)
            .unwrap()
            .contains("private-unregistered-value")
    );
    let issues = replay(&directory, &entries).await.unwrap();
    assert_eq!(issues.len(), 1);
    assert_eq!(
        issues[0].kind,
        world::replay::ReplayIssue::ContextUnavailable
    );
    let mut legacy = entries;
    legacy[0].replay = None;
    let issues = replay(&directory, &legacy).await.unwrap();
    assert!(
        issues
            .iter()
            .all(|issue| issue.kind == world::replay::ReplayIssue::ContextUnavailable)
    );
}

#[tokio::test]
async fn replay_does_not_follow_redirects_or_create_unrecorded_requests() {
    let directory = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../scenarios/code/github/ci-failure-root-cause");
    let state = shared(WorldState::from_dataset(Dataset::load(&directory).unwrap()).unwrap());
    let running = spawn(
        state.clone(),
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        PortChoice::Ephemeral,
    )
    .await
    .unwrap();
    let response = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap()
        .get(format!(
            "http://{}/api/v3/repos/octo-org/payments-api/actions/jobs/50101/logs",
            running.services["github"]
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 302);
    let entries = state.lock().unwrap().journal.entries().to_vec();
    assert_eq!(entries.len(), 1);
    assert!(replay(&directory, &entries).await.unwrap().is_empty());
    assert_eq!(state.lock().unwrap().journal.entries().len(), 1);
}
