use std::{path::Path, sync::Arc};

use axum::{
    Extension, Json, Router,
    body::Body,
    extract::State,
    http::{Request, StatusCode},
    routing::{get, put},
};
use mock_core::{
    dataset::Dataset,
    events::EventKind,
    preservation::StateChange,
    serve::service_app,
    service::MockService,
    world::{SharedWorld, WorldState, shared},
};
use serde_json::{Value, json};
use tokio::sync::Notify;
use tower::ServiceExt;

#[derive(Clone, Default)]
struct Signals {
    committed: Arc<Notify>,
    release: Arc<Notify>,
}

struct TestService {
    signals: Signals,
}

impl MockService for TestService {
    fn name(&self) -> &'static str {
        "github"
    }

    fn port(&self) -> u16 {
        0
    }

    fn hosts(&self) -> &'static [&'static str] {
        &[]
    }

    fn router(&self) -> Router<SharedWorld> {
        Router::new()
            .route("/state", get(read).put(write))
            .route("/query", axum::routing::post(read))
            .route("/slow-state", put(slow_write))
            .route("/fail", put(failed_write))
            .layer(Extension(self.signals.clone()))
    }
}

async fn read(State(world): State<SharedWorld>) -> Json<Value> {
    let world = world.lock().unwrap();
    Json(world.service("github").unwrap().store.snapshot())
}

fn set_state(world: &SharedWorld, state: Value) {
    world
        .lock()
        .unwrap()
        .service_mut("github")
        .store
        .collection_mut("issues")
        .get_mut("octo-org/app#1")
        .unwrap()["state"] = state;
}

async fn write(State(world): State<SharedWorld>, Json(body): Json<Value>) -> StatusCode {
    set_state(&world, body);
    StatusCode::OK
}

async fn failed_write(State(world): State<SharedWorld>, Json(body): Json<Value>) -> StatusCode {
    set_state(&world, body);
    StatusCode::INTERNAL_SERVER_ERROR
}

async fn slow_write(
    State(world): State<SharedWorld>,
    Extension(signals): Extension<Signals>,
    Json(body): Json<Value>,
) -> StatusCode {
    set_state(&world, body);
    signals.committed.notify_one();
    signals.release.notified().await;
    StatusCode::OK
}

fn setup() -> (SharedWorld, Router, Signals) {
    let directory =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/example/github/basic");
    let world = shared(WorldState::from_dataset(Dataset::load(&directory).unwrap()).unwrap());
    let signals = Signals::default();
    let app = service_app(
        world.clone(),
        &TestService {
            signals: signals.clone(),
        },
    );
    (world, app, signals)
}

fn request(path: &str, state: Value) -> Request<Body> {
    Request::put(path)
        .header("content-type", "application/json")
        .body(Body::from(state.to_string()))
        .unwrap()
}

#[tokio::test]
async fn records_committed_changes_even_when_the_final_state_is_restored() {
    let (world, app, _) = setup();
    let initial = world.lock().unwrap().initial_snapshot();
    app.clone()
        .oneshot(request("/state", json!("closed")))
        .await
        .unwrap();
    app.oneshot(request("/state", json!("open"))).await.unwrap();
    let world = world.lock().unwrap();
    assert_eq!(world.snapshot()["services"], initial["services"]);
    let commits: Vec<_> = world
        .events()
        .entries()
        .iter()
        .filter_map(|event| match &event.event {
            EventKind::StateCommitted { records } => Some((event, records)),
            _ => None,
        })
        .collect();
    assert_eq!(commits.len(), 2);
    assert_eq!(commits[0].1[0].after.as_ref().unwrap()["state"], "closed");
    assert_eq!(commits[1].1[0].after.as_ref().unwrap()["state"], "open");
    assert!(commits.iter().all(|(event, _)| event.operation.is_some()));
}

#[tokio::test]
async fn commit_order_is_independent_of_response_completion_order() {
    let (world, app, signals) = setup();
    let slow_app = app.clone();
    let slow = tokio::spawn(async move {
        slow_app
            .oneshot(request("/slow-state", json!("closed")))
            .await
            .unwrap()
    });
    signals.committed.notified().await;
    app.oneshot(request("/state", json!("open"))).await.unwrap();
    signals.release.notify_one();
    slow.await.unwrap();
    let world = world.lock().unwrap();
    let commits: Vec<_> = world
        .events()
        .entries()
        .iter()
        .filter(|event| matches!(event.event, EventKind::StateCommitted { .. }))
        .collect();
    assert_eq!(commits.len(), 2);
    assert_eq!(commits[0].operation.as_ref().unwrap().path, "/slow-state");
    assert_eq!(commits[1].operation.as_ref().unwrap().path, "/state");
    assert_eq!(world.journal.entries()[0].path, "/state");
    assert_eq!(world.journal.entries()[1].path, "/slow-state");
    assert_eq!(
        world.journal.entries()[0].operation_id,
        Some(commits[1].operation.as_ref().unwrap().id)
    );
}

#[tokio::test]
async fn read_only_post_requests_do_not_create_mutation_events() {
    let (world, app, _) = setup();
    app.oneshot(Request::post("/query").body(Body::empty()).unwrap())
        .await
        .unwrap();
    let world = world.lock().unwrap();
    assert_eq!(world.events().entries().len(), 4);
    assert!(matches!(
        world.events().entries()[1].event,
        EventKind::RequestHeadReceived { .. }
    ));
    assert!(matches!(
        world.events().entries()[2].event,
        EventKind::RequestBodyReceived { .. }
    ));
    assert!(
        !world
            .events()
            .entries()
            .iter()
            .any(|event| matches!(event.event, EventKind::StateCommitted { .. }))
    );
}

#[tokio::test]
async fn failed_responses_do_not_erase_committed_effects() {
    let (world, app, _) = setup();
    let response = app
        .oneshot(request("/fail", json!("closed")))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    let world = world.lock().unwrap();
    assert!(
        world
            .events()
            .entries()
            .iter()
            .any(|event| matches!(event.event, EventKind::StateCommitted { .. }))
    );
    assert!(matches!(
        world.events().entries().last().unwrap().event,
        EventKind::ResponseCompleted {
            status: 500,
            body_complete: true
        }
    ));
}

#[tokio::test]
async fn resetting_starts_a_fresh_history_without_recording_a_false_reset_mutation() {
    let (world, app, _) = setup();
    app.clone()
        .oneshot(request("/state", json!("closed")))
        .await
        .unwrap();
    world.lock().unwrap().reset().unwrap();
    assert!(world.lock().unwrap().events().entries().is_empty());
    app.oneshot(request("/state", json!("closed")))
        .await
        .unwrap();
    let world = world.lock().unwrap();
    assert_eq!(world.events().entries()[0].seq, 1);
    assert!(matches!(
        world.events().entries()[0].event,
        EventKind::RequestStarted
    ));
}

#[test]
fn record_history_distinguishes_a_null_value_from_an_absent_record() {
    for (before, after) in [
        (None, Some(Value::Null)),
        (Some(Value::Null), None),
        (Some(json!({})), Some(Value::Null)),
    ] {
        let change = StateChange {
            pointer: "/services/example/objects/record".into(),
            before,
            after,
        };
        let encoded = serde_json::to_value(&change).unwrap();
        let decoded: StateChange = serde_json::from_value(encoded).unwrap();
        assert_eq!(decoded, change);
    }
}

#[tokio::test]
async fn service_setup_is_materialized_once_and_cannot_rewrite_a_started_baseline() {
    fn initialize(service: &mut mock_core::world::ServiceState, _now: i64) {
        let id = service.ids.next_int("initial", 1);
        service
            .store
            .collection_mut("objects")
            .insert(id.to_string(), json!({"ready": true}));
    }
    let (world, app, _) = setup();
    world.prepare_service("prepared", Some(initialize)).unwrap();
    world.prepare_service("prepared", Some(initialize)).unwrap();
    let initial = world.lock().unwrap().initial_snapshot();
    assert_eq!(
        initial["services"]["prepared"]["objects"]
            .as_object()
            .unwrap()
            .len(),
        1
    );
    assert!(world.lock().unwrap().events().entries().is_empty());
    app.oneshot(Request::get("/state").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert!(world.prepare_service("late", Some(initialize)).is_err());
    assert!(world.lock().unwrap().service("late").is_none());
    world.lock().unwrap().reset().unwrap();
    assert_eq!(
        world.lock().unwrap().snapshot()["services"],
        initial["services"]
    );
}
