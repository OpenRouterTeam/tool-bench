use std::{
    future::{Future, poll_fn},
    path::Path,
    pin::Pin,
    sync::{Arc, Mutex, mpsc},
    task::Poll,
    time::Duration,
};

use axum::{
    Extension, Router,
    body::Body,
    extract::State,
    http::{Request, StatusCode},
    routing::post,
};
use mock_core::{
    dataset::Dataset,
    events::{self, EventKind, OperationKind},
    evidence::WorldEvidence,
    grading::{Expect, Outcome},
    lifecycle::{LifecycleError, Operation, Phase},
    serve::service_app,
    service::MockService,
    world::{SharedWorld, WorldState, shared},
};
use serde_json::json;
use tokio::sync::Notify;
use tower::ServiceExt;

fn world() -> SharedWorld {
    world_with_contract(contract())
}

fn world_with_contract(expected: Expect) -> SharedWorld {
    let directory =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/example/github/basic");
    let mut dataset = Dataset::load(&directory).unwrap();
    dataset.contract = expected;
    shared(WorldState::from_dataset(dataset).unwrap())
}

fn begin(world: &SharedWorld) -> Operation {
    let operation = world
        .begin_operation(OperationKind::Http, "github", "192.0.2.10", "POST", "/work")
        .unwrap();
    receive(&operation);
    operation
}

fn receive(operation: &Operation) {
    operation.record_head(mock_core::request::RequestHead {
        host: None,
        scheme: if operation.kind == OperationKind::Internal {
            "internal"
        } else {
            "http"
        }
        .into(),
        content_type: None,
        query: None,
        headers: Vec::new(),
    });
    operation.record_body(mock_core::request::CapturedBody::capture(b""));
}

fn finish(operation: Operation, status: u16, complete: bool) {
    operation.record_response(mock_core::journal::ResponseRecord {
        status,
        response_body: serde_json::Value::Null,
    });
    operation.finish(status, complete);
}

fn evidence(world: &SharedWorld) -> WorldEvidence {
    WorldEvidence::capture(&world.lock().unwrap(), &[], Ok(vec!["192.0.2.10".into()]))
}

fn contract() -> Expect {
    serde_json::from_value(json!({
        "version": 2, "audit_history": true, "answer": {"contains": ["Maya"]}
    }))
    .unwrap()
}

async fn expect_pending(mut future: Pin<&mut impl Future<Output = ()>>) {
    poll_fn(|context| {
        assert!(future.as_mut().poll(context).is_pending());
        Poll::Ready(())
    })
    .await;
}

#[tokio::test]
async fn idle_finalization_is_idempotent_and_unsealed_snapshots_cannot_pass() {
    let world = world();
    let diagnostic = evidence(&world).assess(&contract(), Some("Maya"), false);
    assert_eq!(diagnostic.outcome, Outcome::EnvironmentError);
    world.finalize(Duration::ZERO).await;
    let first = evidence(&world);
    assert_eq!(first.finalization.phase, Phase::Sealed);
    assert_eq!(first.assess(&contract(), Some("Maya"), false).reward, 1.0);
    world.finalize(Duration::from_secs(1)).await;
    assert_eq!(
        serde_json::to_value(&first).unwrap(),
        serde_json::to_value(evidence(&world)).unwrap()
    );
    assert_eq!(world.lock().unwrap().reset(), Err(LifecycleError::Closed));
    assert!(
        world
            .begin_operation(OperationKind::Http, "github", "tools", "GET", "/late")
            .is_err()
    );
    assert!(
        world
            .begin_operation(OperationKind::Internal, "github", "tools", "GET", "/late")
            .is_err()
    );
}

#[tokio::test]
async fn draining_waits_for_admitted_work_and_rejects_new_roots() {
    let world = world();
    let operation = begin(&world);
    let finalization = world.finalize(Duration::from_secs(5));
    tokio::pin!(finalization);
    expect_pending(finalization.as_mut()).await;
    assert_eq!(world.lock().unwrap().finalization().phase, Phase::Draining);
    assert_eq!(world.lock().unwrap().reset(), Err(LifecycleError::Busy));
    assert!(
        world
            .begin_operation(OperationKind::Http, "github", "tools", "GET", "/late")
            .is_err()
    );
    assert!(
        world
            .begin_operation(OperationKind::Internal, "github", "tools", "GET", "/late")
            .is_err()
    );
    finish(operation, 200, true);
    finalization.await;
    let evidence = evidence(&world);
    assert!(
        evidence
            .finalization
            .validate(&evidence.scenario, &evidence.events)
            .is_ok()
    );
}

#[tokio::test]
async fn retained_execution_outlives_a_canceled_response_and_finishes_during_drain() {
    let world = world();
    let operation = begin(&world);
    let execution = world.retain_execution(operation.context()).unwrap();
    drop(operation);
    assert!(matches!(
        world
            .lock()
            .unwrap()
            .events()
            .entries()
            .last()
            .unwrap()
            .event,
        EventKind::OperationAborted
    ));
    let finalization = world.finalize(Duration::from_secs(5));
    tokio::pin!(finalization);
    expect_pending(finalization.as_mut()).await;
    assert_eq!(
        world.lock().unwrap().finalization().pending[0].executions,
        1
    );
    execution.run(|| {
        let child = world
            .begin_operation(
                OperationKind::Internal,
                "github",
                "192.0.2.10",
                "GET",
                "/work",
            )
            .unwrap();
        receive(&child);
        finish(child, 200, true);
    });
    finalization.await;
    let evidence = evidence(&world);
    assert_eq!(
        evidence.assess(&contract(), Some("Maya"), false).reward,
        1.0
    );
}

#[tokio::test]
async fn timeout_is_a_persistent_environment_fault_not_a_later_success() {
    let world = world();
    let operation = begin(&world);
    world.finalize(Duration::ZERO).await;
    assert_eq!(world.lock().unwrap().finalization().phase, Phase::Failed);
    drop(operation);
    world.finalize(Duration::from_secs(1)).await;
    let captured = evidence(&world);
    assert!(captured.finalization.pending.is_empty());
    assert_eq!(captured.finalization.phase, Phase::Failed);
    assert_eq!(
        captured.assess(&contract(), Some("Maya"), false).outcome,
        Outcome::EnvironmentError
    );
}

#[tokio::test]
async fn concurrent_finalizers_wake_when_the_last_execution_finishes() {
    let world = world();
    let operation = begin(&world);
    let first = world.finalize(Duration::from_secs(5));
    let second = world.finalize(Duration::ZERO);
    tokio::pin!(first, second);
    expect_pending(first.as_mut()).await;
    expect_pending(second.as_mut()).await;
    drop(operation);
    tokio::join!(first, second);
    assert_eq!(
        world.lock().unwrap().finalization().drain_timeout_ms,
        Some(5000)
    );
    assert!(
        world
            .lock()
            .unwrap()
            .finalization()
            .drain_elapsed_ms
            .is_some()
    );
    assert_eq!(world.lock().unwrap().finalization().phase, Phase::Sealed);
}

#[tokio::test]
async fn closed_contexts_cannot_retain_or_admit_new_work() {
    let world = world();
    let operation = begin(&world);
    let context = operation.context().clone();
    finish(operation, 200, true);
    assert!(world.retain_execution(&context).is_err());
    events::sync_scope(Some(context.clone()), || {
        assert!(
            world
                .begin_operation(OperationKind::Internal, "github", "tools", "GET", "/stale")
                .is_err()
        );
    });
    world.lock().unwrap().reset().unwrap();
    assert!(world.lock().unwrap().events().entries().is_empty());
    let next = begin(&world);
    assert_eq!(next.id, context.id);
    assert_ne!(next.generation, context.generation);
    assert!(world.retain_execution(&context).is_err());
    events::sync_scope(Some(context), || {
        assert!(
            world
                .begin_operation(OperationKind::Internal, "github", "tools", "GET", "/stale")
                .is_err()
        );
    });
    finish(next, 200, true);
}

#[tokio::test]
async fn a_panicked_background_execution_closes_its_lease_and_is_an_environment_fault() {
    let world = world();
    let operation = begin(&world);
    let execution = world.retain_execution(operation.context()).unwrap();
    drop(operation);
    let worker =
        tokio::task::spawn_blocking(move || execution.run(|| panic!("injected worker failure")));
    assert!(worker.await.is_err());
    world.finalize(Duration::ZERO).await;
    let captured = evidence(&world);
    assert_eq!(captured.finalization.phase, Phase::Sealed);
    assert!(
        captured
            .finalization
            .validate(&captured.scenario, &captured.events)
            .is_ok()
    );
    let assessment = captured.assess(&contract(), Some("Maya"), false);
    assert_eq!(assessment.outcome, Outcome::EnvironmentError);
    assert!(
        assessment
            .failures
            .iter()
            .any(|failure| failure.location == "environment/execution_panicked")
    );
}

#[tokio::test]
async fn finalization_rejects_missing_terminal_and_retained_execution_evidence() {
    let world = world();
    let operation = begin(&world);
    let execution = world.retain_execution(operation.context()).unwrap();
    finish(operation, 200, true);
    drop(execution);
    world.finalize(Duration::ZERO).await;
    for changed in ["events", "release", "context", "pending"] {
        let mut captured = evidence(&world);
        match changed {
            "events" => {
                captured.events.pop();
            }
            "release" => {
                captured.events.pop();
                captured.finalization.event_count = captured.events.len();
            }
            "context" => captured.events[2].operation.as_mut().unwrap().source = "forged".into(),
            "pending" => {
                captured
                    .finalization
                    .pending
                    .push(mock_core::lifecycle::PendingOperation {
                        operation: captured.events[0].operation.clone().unwrap(),
                        executions: 1,
                    })
            }
            _ => unreachable!(),
        }
        assert_eq!(
            captured.assess(&contract(), Some("Maya"), false).outcome,
            Outcome::EnvironmentError,
            "{changed}"
        );
    }
}

#[tokio::test]
async fn incomplete_response_capture_is_an_environment_fault_even_without_history_checks() {
    for version in [1, 2] {
        let contract: Expect = serde_json::from_value(json!({
            "version": version, "answer": {"contains": ["Maya"]}
        }))
        .unwrap();
        let world = world_with_contract(contract.clone());
        finish(begin(&world), 500, false);
        world.finalize(Duration::ZERO).await;
        let captured = evidence(&world);
        assert_eq!(
            captured.assess(&contract, Some("Maya"), false).outcome,
            Outcome::EnvironmentError
        );
    }
}

#[tokio::test(start_paused = true)]
async fn an_interrupted_collector_cannot_turn_late_work_into_a_passing_retry() {
    let world = world();
    let operation = begin(&world);
    {
        let collecting = world.finalize(Duration::from_secs(5));
        tokio::pin!(collecting);
        expect_pending(collecting.as_mut()).await;
    }
    tokio::time::advance(Duration::from_secs(6)).await;
    finish(operation, 200, true);
    world.finalize(Duration::from_secs(30)).await;
    let captured = evidence(&world);
    assert_eq!(captured.finalization.phase, Phase::Failed);
    assert_eq!(captured.finalization.drain_timeout_ms, Some(5000));
    assert_eq!(captured.finalization.drain_elapsed_ms, Some(6000));
    assert_eq!(
        captured.assess(&contract(), Some("Maya"), false).outcome,
        Outcome::EnvironmentError
    );
}

#[tokio::test(start_paused = true)]
async fn delayed_collection_preserves_the_actual_on_time_drain_duration() {
    let world = world();
    let operation = begin(&world);
    {
        let collecting = world.finalize(Duration::from_secs(5));
        tokio::pin!(collecting);
        expect_pending(collecting.as_mut()).await;
    }
    tokio::time::advance(Duration::from_secs(4)).await;
    finish(operation, 200, true);
    tokio::time::advance(Duration::from_secs(10)).await;
    world.finalize(Duration::ZERO).await;
    let captured = evidence(&world);
    assert_eq!(captured.finalization.phase, Phase::Sealed);
    assert_eq!(captured.finalization.drain_timeout_ms, Some(5000));
    assert_eq!(captured.finalization.drain_elapsed_ms, Some(4000));
    assert_eq!(
        captured.assess(&contract(), Some("Maya"), false).reward,
        1.0
    );
}

#[tokio::test]
async fn inconsistent_sealed_timing_cannot_be_used_as_valid_finalization_proof() {
    let world = world();
    world.finalize(Duration::ZERO).await;
    let mut captured = evidence(&world);
    captured.finalization.drain_elapsed_ms = Some(1);
    assert_eq!(
        captured.assess(&contract(), Some("Maya"), false).outcome,
        Outcome::EnvironmentError
    );
}

#[derive(Clone)]
struct Signals {
    started: Arc<Notify>,
    release: Arc<Mutex<Option<mpsc::Receiver<()>>>>,
}

struct DelayedWriter(Signals);

impl MockService for DelayedWriter {
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
            .route("/write", post(delayed_write))
            .layer(Extension(self.0.clone()))
    }
}

async fn delayed_write(
    State(world): State<SharedWorld>,
    Extension(signals): Extension<Signals>,
) -> StatusCode {
    let context = events::current().unwrap();
    let execution = world.retain_execution(&context).unwrap();
    let release = signals.release.lock().unwrap().take().unwrap();
    tokio::task::spawn_blocking(move || {
        execution.run(|| {
            signals.started.notify_one();
            if release.recv().is_err() {
                return;
            }
            let child = world
                .begin_operation(
                    OperationKind::Internal,
                    "github",
                    "unknown",
                    "POST",
                    "/commit",
                )
                .unwrap();
            receive(&child);
            events::sync_scope(Some(child.context().clone()), || {
                world
                    .lock()
                    .unwrap()
                    .service_mut("github")
                    .store
                    .collection_mut("issues")
                    .get_mut("octo-org/app#1")
                    .unwrap()["state"] = json!("closed");
            });
            finish(child, 200, true);
        })
    })
    .await
    .unwrap();
    StatusCode::OK
}

#[tokio::test]
async fn canceled_http_work_is_drained_before_capture_and_late_requests_cannot_change_it() {
    let world = world();
    let (release, receiver) = mpsc::channel();
    let signals = Signals {
        started: Arc::new(Notify::new()),
        release: Arc::new(Mutex::new(Some(receiver))),
    };
    let app = service_app(world.clone(), &DelayedWriter(signals.clone()));
    let running_app = app.clone();
    let request = tokio::spawn(async move {
        running_app
            .oneshot(Request::post("/write").body(Body::empty()).unwrap())
            .await
    });
    signals.started.notified().await;
    request.abort();
    assert!(request.await.unwrap_err().is_cancelled());
    let finalization = world.finalize(Duration::from_secs(5));
    tokio::pin!(finalization);
    expect_pending(finalization.as_mut()).await;
    release.send(()).unwrap();
    finalization.await;
    let captured = evidence(&world);
    assert_eq!(
        captured.state["services"]["github"]["issues"]["octo-org/app#1"]["state"],
        "closed"
    );
    assert!(
        captured
            .finalization
            .validate(&captured.scenario, &captured.events)
            .is_ok()
    );
    let response = app
        .oneshot(Request::post("/write").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(
        serde_json::to_value(&captured).unwrap(),
        serde_json::to_value(evidence(&world)).unwrap()
    );
}
