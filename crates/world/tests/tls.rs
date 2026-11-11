use std::{
    net::{IpAddr, Ipv4Addr},
    path::Path,
};

use mock_core::{
    dataset::Dataset,
    world::{WorldState, shared},
};
use serde_json::Value;
use tower::ServiceExt;
use world::{PortChoice, Running, spawn};

async fn start() -> Running {
    start_scenario("example/github/basic").await
}

async fn start_scenario(id: &str) -> Running {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../scenarios")
        .join(id);
    let state = shared(WorldState::from_dataset(Dataset::load(&dir).unwrap()).unwrap());
    spawn(
        state,
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        PortChoice::Ephemeral,
    )
    .await
    .unwrap()
}

#[tokio::test]
async fn firecrawl_account_origin_keeps_polling_on_tls_without_internal_authorities() {
    let running = start_scenario("search/firecrawl/crawl-api-reference").await;
    let host = "api.firecrawl.dev";
    let origin = format!("https://{host}:{}", running.tls.port());
    let client = client(&running, host);
    let admitted: Value = client
        .post(format!("{origin}/v2/crawl"))
        .bearer_auth("local-fixture-credential")
        .header("x-forwarded-proto", "http")
        .json(&serde_json::json!({
            "url":"https://docs.lumenpay.dev/api/overview",
            "crawlEntireDomain":true,"includePaths":["^/api/.*"],"limit":50
        }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(admitted["success"], true);
    assert_eq!(
        admitted["url"],
        format!("{origin}/v2/crawl/{}", admitted["id"].as_str().unwrap())
    );
    let observed: Value = client
        .get(admitted["url"].as_str().unwrap())
        .bearer_auth("local-fixture-credential")
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(observed["status"], "scraping");
    assert!(!admitted.to_string().contains("world:8102"));
    assert!(!observed.to_string().contains("world:8102"));
}

#[tokio::test]
async fn pagerduty_account_origin_keeps_directory_and_schedule_references_on_tls() {
    let running = start_scenario("ops/pagerduty/cover-shift-override").await;
    let host = "api.pagerduty.com";
    let origin = format!("https://{host}:{}", running.tls.port());
    let client = client(&running, host);
    let user: Value = client
        .get(format!("{origin}/users/me"))
        .header("authorization", "Token token=local-fixture-credential")
        .header("x-forwarded-proto", "http")
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(user["user"]["id"], "PA1EXKM");
    assert_eq!(user["user"]["self"], format!("{origin}/users/PA1EXKM"));
    let schedule: Value = client
        .get(format!("{origin}/schedules/PSPLTPR"))
        .header("authorization", "Token token=local-fixture-credential")
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(
        schedule["schedule"]["self"],
        format!("{origin}/schedules/PSPLTPR")
    );
    assert!(!user.to_string().contains("world:8111"));
    assert!(!schedule.to_string().contains("world:8111"));
    drop(running);
}

#[tokio::test]
async fn terraform_account_origin_serves_both_download_shapes_without_internal_authorities() {
    let running = start_scenario("code/terraform/approve-nat-gateway-change").await;
    let host = "app.terraform.io";
    let origin = format!("https://{host}:{}", running.tls.port());
    let client = client(&running, host);
    let metadata: Value = client
        .get(format!(
            "{origin}/api/v2/state-versions/sv-2HkRH31vgu6V7Nh1"
        ))
        .bearer_auth("local-fixture-credential")
        .header("x-forwarded-proto", "http")
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(metadata["data"]["attributes"].get("lineage").is_none());
    for (field, shape) in [
        ("hosted-state-download-url", "version"),
        ("hosted-json-state-download-url", "format_version"),
    ] {
        let url = metadata["data"]["attributes"][field].as_str().unwrap();
        assert!(url.starts_with(&format!("{origin}/archivist/")));
        assert!(!url.contains("world") && !url.contains("8109"));
        let state: Value = client.get(url).send().await.unwrap().json().await.unwrap();
        assert!(state.get(shape).is_some(), "{field}: {state}");
    }
    drop(running);
}

#[tokio::test]
async fn grafana_account_origin_keeps_oncall_discovery_and_pagination_on_tls() {
    let running = start_scenario("ops/grafana/ack-checkout-page-oncall").await;
    let host = "grafana.octo-corp.com";
    let origin = format!("https://{host}:{}", running.tls.port());
    let client = client(&running, host);
    let settings: Value = client
        .get(format!("{origin}/api/plugins/grafana-irm-app/settings"))
        .bearer_auth(mock_grafana::TOKEN)
        .header("x-forwarded-proto", "http")
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(
        settings["jsonData"]["onCallApiUrl"],
        format!("{origin}/oncall")
    );
    let groups: Value = client
        .get(format!(
            "{origin}/oncall/api/v1/alert_groups/?state=new&page=2"
        ))
        .bearer_auth(mock_grafana::TOKEN)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let previous = groups["previous"].as_str().unwrap();
    assert!(previous.starts_with(&format!("{origin}/oncall/")));
    assert!(previous.contains("state=new") && previous.contains("page=1"));
    let prior: Value = client
        .get(previous)
        .bearer_auth(mock_grafana::TOKEN)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(
        prior["results"]
            .as_array()
            .unwrap()
            .iter()
            .all(|group| group["state"] == "new")
    );
}

#[tokio::test]
async fn gitlab_account_hostnames_produce_ordinary_tls_identity_and_resource_links() {
    for (scenario, host, username) in [
        (
            "code/gitlab/file-circuit-breaker-issue",
            "gitlab.northwind.internal",
            "sam.okafor",
        ),
        (
            "code/multi/argocd-degraded-gitlab-commit",
            "gitlab.lumen.internal",
            "maria.okafor",
        ),
    ] {
        let running = start_scenario(scenario).await;
        let origin = format!("https://{host}:{}", running.tls.port());
        let response = client(&running, host)
            .get(format!("{origin}/api/v4/user"))
            .header("x-forwarded-proto", "http")
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        let body: Value = response.json().await.unwrap();
        assert_eq!(body["username"], username);
        assert_eq!(body["web_url"], format!("{origin}/{username}"));
        assert!(
            body["avatar_url"]
                .as_str()
                .unwrap()
                .starts_with("https://secure.gravatar.com/")
        );
    }
}

#[tokio::test]
async fn gitlab_public_pagination_links_remain_functional_and_keep_filters() {
    let running = start_scenario("code/gitlab/file-circuit-breaker-issue").await;
    let host = "gitlab.northwind.internal";
    let client = client(&running, host);
    let origin = format!("https://{host}:{}", running.tls.port());
    let response = client
        .get(format!(
            "{origin}/api/v4/projects?per_page=1&order_by=id&sort=asc"
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let link = response.headers()["link"].to_str().unwrap().to_owned();
    let next = link
        .split(',')
        .find(|part| part.contains("rel=\"next\""))
        .unwrap()
        .trim()
        .split_once('>')
        .unwrap()
        .0
        .trim_start_matches('<');
    assert!(next.starts_with(&origin));
    assert!(next.contains("order_by=id") && next.contains("sort=asc"));
    let first: Value = response.json().await.unwrap();
    let second: Value = client.get(next).send().await.unwrap().json().await.unwrap();
    assert_ne!(first[0]["id"], second[0]["id"]);
    assert_eq!(
        first[0]["web_url"],
        format!("{origin}/northwind/payments/checkout-api")
    );
}

#[tokio::test]
async fn gitlab_surface_repairs_do_not_scrub_authored_business_text() {
    let running = start_scenario("code/gitlab/file-circuit-breaker-issue").await;
    let host = "gitlab.northwind.internal";
    let authored = "The log contains http://world:8107; retain this exact diagnostic text.";
    let response = client(&running, host)
        .post(format!(
            "https://{host}:{}/api/v4/projects/northwind%2Fpayments%2Fcheckout-api/issues",
            running.tls.port()
        ))
        .json(&serde_json::json!({"title":"A diagnostic note","description":authored}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 201);
    let body: Value = response.json().await.unwrap();
    assert_eq!(body["description"], authored);
    assert!(
        body["web_url"]
            .as_str()
            .unwrap()
            .starts_with("https://gitlab.northwind.internal:")
    );
}

#[tokio::test]
async fn unknown_tls_authorities_do_not_expose_mock_infrastructure_details() {
    let running = start().await;
    let response = client(&running, "api.github.com")
        .get(format!(
            "https://api.github.com:{}/anything",
            running.tls.port()
        ))
        .header("host", "unlisted-workspace.example")
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 421);
    let body: Value = response.json().await.unwrap();
    assert_eq!(
        body,
        serde_json::json!({"message":"421 Misdirected Request"})
    );
}

#[tokio::test]
async fn authority_only_requests_keep_their_account_origin_in_generated_links() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../scenarios/code/gitlab/file-circuit-breaker-issue");
    let state = shared(WorldState::from_dataset(Dataset::load(&dir).unwrap()).unwrap());
    let app = world::tls::host_router(state, &world::services());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("https://gitlab.northwind.internal/api/v4/user")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let bytes = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let body: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(
        body["web_url"],
        "https://gitlab.northwind.internal/sam.okafor"
    );
}

/// A client that trusts only the world's CA and resolves `host` to the world's TLS port.
fn client(running: &Running, host: &str) -> reqwest::Client {
    let ca = reqwest::Certificate::from_pem(running.ca_pem.as_bytes()).unwrap();
    reqwest::Client::builder()
        .no_proxy()
        .tls_certs_only([ca])
        .resolve(host, running.tls)
        .build()
        .unwrap()
}

#[tokio::test]
async fn serves_real_hostnames_over_tls_with_the_local_ca() {
    let running = start().await;
    let resp = client(&running, "api.github.com")
        .get(format!(
            "https://api.github.com:{}/api/v3/user",
            running.tls.port()
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["login"], "mona");

    let journal: Value = reqwest::Client::builder()
        .no_proxy()
        .build()
        .unwrap()
        .get(format!("http://{}/_bench/journal", running.admin))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(journal["entries"][0]["service"], "github");
    assert_eq!(journal["entries"][0]["path"], "/api/v3/user");
}

#[tokio::test]
async fn absolute_urls_use_https_behind_tls() {
    let running = start().await;
    let issues: Value = client(&running, "api.github.com")
        .get(format!(
            "https://api.github.com:{}/api/v3/repos/octo-org/app/issues?per_page=1",
            running.tls.port()
        ))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    // Stored fixture URLs are untouched; only generated links use the request scheme.
    assert_eq!(issues.as_array().unwrap().len(), 1);
    let resp = client(&running, "api.github.com")
        .get(format!(
            "https://api.github.com:{}/api/v3/repos/octo-org/app/issues?per_page=1",
            running.tls.port()
        ))
        .send()
        .await
        .unwrap();
    let link = resp.headers()["link"].to_str().unwrap().to_owned();
    assert!(link.starts_with("<https://api.github.com:"), "{link}");
}

#[tokio::test]
async fn hosts_outside_the_certificate_fail_tls() {
    let running = start().await;
    let err = client(&running, "evil.example")
        .get(format!("https://evil.example:{}/", running.tls.port()))
        .send()
        .await
        .unwrap_err();
    assert!(err.is_connect() || err.is_request(), "{err:?}");
}

#[tokio::test]
async fn ca_certificate_is_published_on_admin() {
    let running = start().await;
    let pem = reqwest::Client::builder()
        .no_proxy()
        .build()
        .unwrap()
        .get(format!("http://{}/_bench/ca.pem", running.admin))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert_eq!(pem, running.ca_pem);
    assert!(pem.starts_with("-----BEGIN CERTIFICATE-----"));
}

#[tokio::test]
async fn origin_restrictions_apply_before_both_http_and_tls_dispatch() {
    let directory =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/example/github/basic");
    let state = shared(WorldState::from_dataset(Dataset::load(&directory).unwrap()).unwrap());
    let running = world::spawn_with_access(
        state.clone(),
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        PortChoice::Ephemeral,
        world::AccessPolicy {
            admin_token: Some("secret".into()),
            tools_host: Some("192.0.2.10".into()),
            bootstrap_bind: None,
        },
    )
    .await
    .unwrap();
    let response = client(&running, "api.github.com")
        .get(format!(
            "https://api.github.com:{}/api/v3/user",
            running.tls.port()
        ))
        .header("x-forwarded-for", "192.0.2.10")
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 403);
    let response = reqwest::Client::builder()
        .no_proxy()
        .build()
        .unwrap()
        .get(format!("http://{}/api/v3/user", running.services["github"]))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 403);
    let state = state.lock().unwrap();
    assert_eq!(state.journal.entries().len(), 2);
    assert!(
        state
            .journal
            .entries()
            .iter()
            .all(|entry| entry.status == 403)
    );
}
