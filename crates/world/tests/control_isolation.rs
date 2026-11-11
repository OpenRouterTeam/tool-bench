use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr},
    path::Path,
};

use mock_core::{
    dataset::Dataset,
    world::{WorldState, shared},
};
use world::{PortChoice, Running};

async fn start(token: Option<&str>) -> Running {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/example/github/basic");
    let state = shared(WorldState::from_dataset(Dataset::load(&path).unwrap()).unwrap());
    world::spawn_guarded(
        state,
        IpAddr::V4(Ipv4Addr::UNSPECIFIED),
        PortChoice::Ephemeral,
        token.map(str::to_owned),
    )
    .await
    .unwrap()
}

fn local(mut address: SocketAddr) -> SocketAddr {
    address.set_ip(IpAddr::V4(Ipv4Addr::LOCALHOST));
    address
}

fn client() -> reqwest::Client {
    reqwest::Client::builder().no_proxy().build().unwrap()
}

#[tokio::test]
async fn protected_bootstrap_binds_only_its_selected_interface() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/example/github/basic");
    let state = shared(WorldState::from_dataset(Dataset::load(&path).unwrap()).unwrap());
    let running = world::spawn_with_access(
        state,
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
    assert_eq!(running.bootstrap.ip(), IpAddr::V4(Ipv4Addr::LOCALHOST));
    assert!(
        running
            .services
            .values()
            .all(|address| address.ip().is_unspecified())
    );
    let response = client()
        .get(format!("http://{}/_bench/ca.pem", running.bootstrap))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    assert_eq!(response.text().await.unwrap(), running.ca_pem);
}

#[tokio::test]
async fn explicit_bootstrap_bindings_cannot_silently_become_wildcard_or_unprotected() {
    for (token, address) in [
        (Some("trial-secret".to_owned()), Ipv4Addr::UNSPECIFIED),
        (None, Ipv4Addr::LOCALHOST),
    ] {
        let path =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/example/github/basic");
        let state = shared(WorldState::from_dataset(Dataset::load(&path).unwrap()).unwrap());
        let result = world::spawn_with_access(
            state,
            IpAddr::V4(Ipv4Addr::UNSPECIFIED),
            PortChoice::Ephemeral,
            world::AccessPolicy {
                admin_token: token,
                tools_host: None,
                bootstrap_bind: Some(IpAddr::V4(address)),
            },
        )
        .await;
        assert!(matches!(result, Err(error) if error.kind() == std::io::ErrorKind::InvalidInput));
    }
}

#[tokio::test]
async fn protected_fixed_port_library_runs_cannot_omit_the_bootstrap_interface() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/example/github/basic");
    let state = shared(WorldState::from_dataset(Dataset::load(&path).unwrap()).unwrap());
    let result = world::spawn_guarded(
        state,
        IpAddr::V4(Ipv4Addr::UNSPECIFIED),
        PortChoice::Fixed,
        Some("trial-secret".into()),
    )
    .await;
    assert!(matches!(result, Err(error) if error.kind() == std::io::ErrorKind::InvalidInput));
}

#[test]
fn protected_cli_startup_fails_before_listening_without_a_concrete_bootstrap_host() {
    let scenario =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/example/github/basic");
    for host in [None, Some("0.0.0.0")] {
        let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_world"));
        command
            .env_clear()
            .env("SCENARIO_DIR", &scenario)
            .env("BENCH_ADMIN_TOKEN", "trial-secret");
        if let Some(host) = host {
            command.env("BENCH_BOOTSTRAP_BIND_HOST", host);
        }
        let output = command.output().unwrap();
        assert!(!output.status.success());
        let error = String::from_utf8_lossy(&output.stderr);
        assert!(error.contains("bootstrap") || error.contains("BENCH_BOOTSTRAP_BIND_HOST"));
    }
}

#[tokio::test]
async fn protected_control_is_loopback_only_while_the_bootstrap_listener_is_network_bound() {
    let running = start(Some("trial-secret")).await;
    assert_eq!(running.admin.ip(), IpAddr::V4(Ipv4Addr::LOCALHOST));
    assert!(running.bootstrap.ip().is_unspecified());
    assert_ne!(running.admin.port(), running.bootstrap.port());
    assert!(
        running
            .services
            .values()
            .all(|address| address.ip().is_unspecified())
    );
}

#[tokio::test]
async fn tools_can_bootstrap_health_and_ca_without_receiving_control_routes() {
    let running = start(Some("trial-secret")).await;
    let base = format!("http://{}", local(running.bootstrap));
    let response = client()
        .get(format!("{base}/_bench/health"))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    assert_eq!(
        response.json::<serde_json::Value>().await.unwrap(),
        serde_json::json!({"status":"ok"})
    );
    let certificate = client()
        .get(format!("{base}/_bench/ca.pem"))
        .send()
        .await
        .unwrap();
    assert_eq!(certificate.status(), 200);
    assert_eq!(certificate.text().await.unwrap(), running.ca_pem);
}

#[tokio::test]
async fn a_valid_admin_credential_cannot_expose_control_routes_on_the_bootstrap_listener() {
    let running = start(Some("trial-secret")).await;
    let base = format!("http://{}", local(running.bootstrap));
    for path in [
        "/_bench/state",
        "/_bench/initial_state",
        "/_bench/journal",
        "/_bench/events",
        "/_bench/evidence",
        "/_bench/reset",
        "/_bench/grade",
        "/_bench/answer",
        "/_bench/finalize",
        "/_bench/user/exchange",
    ] {
        for method in [reqwest::Method::GET, reqwest::Method::POST] {
            let response = client()
                .request(method.clone(), format!("{base}{path}"))
                .bearer_auth("trial-secret")
                .json(&serde_json::json!({}))
                .send()
                .await
                .unwrap();
            assert_eq!(response.status(), 404, "{method} {path}");
        }
    }
}

#[tokio::test]
async fn local_control_still_requires_the_trial_credential() {
    let running = start(Some("trial-secret")).await;
    let base = format!("http://{}", running.admin);
    assert_eq!(
        client()
            .get(format!("{base}/_bench/state"))
            .send()
            .await
            .unwrap()
            .status(),
        401
    );
    assert_eq!(
        client()
            .get(format!("{base}/_bench/state"))
            .bearer_auth("wrong")
            .send()
            .await
            .unwrap()
            .status(),
        401
    );
    let response = client()
        .get(format!("{base}/_bench/state"))
        .bearer_auth("trial-secret")
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    assert!(
        response.json::<serde_json::Value>().await.unwrap()["services"]["github"]["issues"]
            .is_object()
    );
}

#[tokio::test]
async fn explicit_unprotected_diagnostics_keep_the_legacy_single_listener() {
    let running = start(None).await;
    assert_eq!(running.admin, running.bootstrap);
    let response = client()
        .get(format!("http://{}/_bench/state", local(running.admin)))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
}
