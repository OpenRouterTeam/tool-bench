use std::{
    net::{IpAddr, SocketAddr},
    path::Path,
};

use axum::{
    Router,
    body::{Body, to_bytes},
    extract::{ConnectInfo, State},
    http::{Request, StatusCode},
    routing::post,
};
use mock_core::{
    access::require_caller,
    dataset::Dataset,
    evidence::WorldEvidence,
    grading::{Expect, Outcome},
    serve::service_app_with_caller,
    service::MockService,
    world::{SharedWorld, WorldState, shared},
};
use serde_json::{Value, json};
use tower::ServiceExt;

struct Records;

impl MockService for Records {
    fn name(&self) -> &'static str {
        "records"
    }
    fn port(&self) -> u16 {
        0
    }
    fn hosts(&self) -> &'static [&'static str] {
        &[]
    }
    fn router(&self) -> Router<SharedWorld> {
        Router::new().route(
            "/records",
            post(|State(world): State<SharedWorld>| async move {
                world
                    .lock()
                    .unwrap()
                    .service_mut("records")
                    .store
                    .collection_mut("records")
                    .insert("created", json!({"id": "created"}));
                StatusCode::CREATED
            }),
        )
    }
}

fn world() -> SharedWorld {
    let directory =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/example/github/basic");
    shared(WorldState::from_dataset(Dataset::load(&directory).unwrap()).unwrap())
}

async fn call(world: SharedWorld, source: Option<&str>) -> (StatusCode, Value) {
    call_with_host(world, source, "127.0.0.1").await
}

async fn call_with_host(
    world: SharedWorld,
    source: Option<&str>,
    host: &str,
) -> (StatusCode, Value) {
    let app = service_app_with_caller(world, &Records, Some(host.into()));
    let mut request = Request::builder()
        .method("POST")
        .uri("/records")
        .header("x-forwarded-for", "127.0.0.1")
        .header("forwarded", "for=127.0.0.1")
        .body(Body::empty())
        .unwrap();
    if let Some(source) = source {
        request
            .extensions_mut()
            .insert(ConnectInfo(source.parse::<SocketAddr>().unwrap()));
    }
    let response = app.oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

#[tokio::test]
async fn denied_calls_are_recorded_without_executing_mutations_or_advancing_time() {
    let world = world();
    let initial = world.lock().unwrap().snapshot();
    let (status, body) = call(world.clone(), Some("192.0.2.7:41000")).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["error"]["type"], "access_denied");
    let world = world.lock().unwrap();
    assert_eq!(world.snapshot(), initial);
    assert_eq!(world.journal.entries().len(), 1);
    assert_eq!(world.journal.entries()[0].status, 403);
    assert_eq!(world.journal.entries()[0].source, "192.0.2.7");
    assert!(!world.events().entries().is_empty());
}

#[tokio::test]
async fn missing_peer_evidence_is_not_replaced_with_forwarded_headers() {
    let world = world();
    let (status, _) = call(world.clone(), None).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert!(
        world.lock().unwrap().snapshot()["services"]
            .get("records")
            .is_none()
    );
}

#[tokio::test]
async fn a_resolved_tools_peer_can_execute_the_normal_operation() {
    let world = world();
    let (status, _) = call(world.clone(), Some("127.0.0.1:41000")).await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(
        world.lock().unwrap().snapshot()["services"]["records"]["records"]["created"]["id"],
        "created"
    );
}

#[tokio::test]
async fn loopback_is_an_explicit_admin_exception_not_a_general_api_exception() {
    let loopback: IpAddr = "127.0.0.1".parse().unwrap();
    assert_eq!(
        require_caller("192.0.2.10", Some(loopback), true).await,
        Ok(())
    );
    assert_eq!(
        require_caller("192.0.2.10", Some(loopback), false).await,
        Err(StatusCode::FORBIDDEN)
    );
    assert_eq!(
        require_caller("127.0.0.1", None, true).await,
        Err(StatusCode::FORBIDDEN)
    );
    assert_eq!(
        require_caller("", Some("192.0.2.7".parse().unwrap()), false).await,
        Err(StatusCode::SERVICE_UNAVAILABLE)
    );
}

#[tokio::test]
async fn failed_origin_resolution_is_an_environment_fault_even_if_the_answer_is_correct() {
    let world = world();
    let (status, _) = call_with_host(world.clone(), Some("127.0.0.1:41000"), "").await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    let evidence =
        WorldEvidence::capture(&world.lock().unwrap(), &[], Ok(vec!["127.0.0.1".into()]));
    let contract: Expect = serde_json::from_value(json!({
        "version": 2,
        "audit_history": true,
        "answer": {"contains": ["Maya"]}
    }))
    .unwrap();
    let assessment = evidence.assess(&contract, Some("Maya"), false);
    assert_eq!(assessment.outcome, Outcome::EnvironmentError);
    assert_eq!(assessment.reward, 0.0);
    assert!(assessment.failures.iter().any(|failure| {
        failure.location == "environment/caller_resolution_failed" && failure.event_evidence == [4]
    }));
}
