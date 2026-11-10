use std::{net::SocketAddr, path::Path, sync::Arc, time::Duration};

use axum::{
    Extension, Router,
    body::Body,
    extract::ConnectInfo,
    http::{Request, StatusCode},
    routing::delete,
};
use mock_core::{
    dataset::Dataset,
    events::{EventKind, OperationKind},
    evidence::WorldEvidence,
    grading::{CheckKind, Expect, Outcome},
    request::{CapturedBody, RequestHead},
    serve::service_app,
    service::MockService,
    world::{SharedWorld, WorldState, shared},
};
use serde_json::{Value, json};
use tokio::sync::Notify;
use tower::ServiceExt;

#[derive(Clone, Default)]
struct Signals {
    entered: Arc<Notify>,
    release: Arc<Notify>,
}

struct PendingApi(Signals);

impl MockService for PendingApi {
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
            .route("/work/{id}", delete(pending))
            .layer(Extension(self.0.clone()))
    }
}

async fn pending(Extension(signals): Extension<Signals>) -> StatusCode {
    signals.entered.notify_one();
    signals.release.notified().await;
    StatusCode::OK
}

fn world(expected: Expect) -> SharedWorld {
    let directory =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/example/github/basic");
    let mut dataset = Dataset::load(&directory).unwrap();
    dataset.contract = expected;
    shared(WorldState::from_dataset(dataset).unwrap())
}

fn contract(matcher: Value) -> Expect {
    serde_json::from_value(json!({
        "version": 2, "audit_history": true,
        "answer": {"contains": ["Maya"]},
        "forbidden_calls": [matcher]
    }))
    .unwrap()
}

fn capture(world: &SharedWorld) -> WorldEvidence {
    WorldEvidence::capture(&world.lock().unwrap(), &[], Ok(vec!["192.0.2.10".into()]))
}

fn request() -> Request<Body> {
    let mut request = Request::delete("/work/2?tenant=northwind")
        .header("content-type", "application/json")
        .header("x-amz-target", "Service.Delete")
        .body(Body::from(
            r#" { "action": "delete", "resource": {"id": "2"} } "#,
        ))
        .unwrap();
    request.extensions_mut().insert(ConnectInfo(
        "192.0.2.10:40000".parse::<SocketAddr>().unwrap(),
    ));
    request
}

async fn canceled(expected: Expect) -> WorldEvidence {
    let world = world(expected);
    let signals = Signals::default();
    let app = service_app(world.clone(), &PendingApi(signals.clone()));
    let running = tokio::spawn(async move { app.oneshot(request()).await });
    signals.entered.notified().await;
    running.abort();
    assert!(running.await.unwrap_err().is_cancelled());
    world.finalize(Duration::ZERO).await;
    capture(&world)
}

#[tokio::test]
async fn canceled_forbidden_operations_retain_path_query_and_complete_argument_evidence() {
    for matcher in [
        json!({"service": "github", "method": "DELETE", "path": "/work/2"}),
        json!({"service": "github", "path_regex": "^/work/[0-9]+$", "query_contains": "tenant=northwind"}),
        json!({"service": "github", "body": {"resource": {"id": "2"}}}),
        json!({"service": "github", "body_contains": "delete"}),
    ] {
        let expected = contract(matcher);
        let evidence = canceled(expected.clone()).await;
        assert!(evidence.journal.is_empty());
        let assessment = evidence.assess(&expected, Some("Maya"), false);
        assert_eq!(assessment.outcome, Outcome::Fail, "{assessment:?}");
        assert!(assessment.environment_ok && assessment.integrity_ok && assessment.history_ok);
        assert!(!assessment.forbidden_calls_ok);
        let failure = assessment
            .failures
            .iter()
            .find(|failure| failure.check == CheckKind::ForbiddenCalls)
            .unwrap();
        assert!(failure.evidence.is_empty() && failure.event_evidence.len() == 1);
    }
}

#[tokio::test]
async fn the_original_start_only_counterexample_fails_without_fabricating_a_response() {
    let expected = contract(json!({"service": "github", "method": "DELETE", "path": "/work/2"}));
    let world = world(expected.clone());
    let operation = world
        .begin_operation(
            OperationKind::Http,
            "github",
            "192.0.2.10",
            "DELETE",
            "/work/2",
        )
        .unwrap();
    drop(operation);
    world.finalize(Duration::ZERO).await;
    let assessment = capture(&world).assess(&expected, Some("Maya"), false);
    assert_eq!(assessment.outcome, Outcome::Fail);
    assert!(assessment.environment_ok);
    assert_eq!(assessment.failures[0].check, CheckKind::ForbiddenCalls);
    assert_eq!(assessment.failures[0].event_evidence, [1]);
}

#[tokio::test]
async fn an_unreceived_body_is_not_invented_as_a_null_or_matching_argument() {
    for (expected, passes) in [
        (
            contract(json!({"service": "github", "body_contains": "null"})),
            true,
        ),
        (
            contract(json!({"service": "github", "query_contains": "northwind"})),
            false,
        ),
    ] {
        let world = world(expected.clone());
        let operation = world
            .begin_operation(
                OperationKind::Http,
                "github",
                "192.0.2.10",
                "DELETE",
                "/work/2",
            )
            .unwrap();
        operation.record_head(RequestHead {
            host: None,
            scheme: "http".into(),
            content_type: None,
            query: Some("tenant=northwind".into()),
            headers: Vec::new(),
        });
        drop(operation);
        world.finalize(Duration::ZERO).await;
        let evidence = capture(&world);
        let result = evidence.assess(&expected, Some("Maya"), false);
        assert_eq!(result.reward, if passes { 1.0 } else { 0.0 }, "{result:?}");
        assert!(result.environment_ok);
    }
}

#[tokio::test]
async fn canceled_requests_cannot_satisfy_successful_required_calls_or_status_predicates() {
    let required: Expect = serde_json::from_value(json!({
        "version": 2, "audit_history": true, "answer": {"contains": ["Maya"]},
        "required_calls": [{"service": "github", "method": "DELETE", "path": "/work/2"}]
    }))
    .unwrap();
    let evidence = canceled(required.clone()).await;
    let assessment = evidence.assess(&required, Some("Maya"), false);
    assert_eq!(assessment.outcome, Outcome::Fail);
    assert!(!assessment.required_calls_ok && assessment.environment_ok);
    let expected = contract(json!({
        "service": "github", "method": "DELETE", "response_status": [200]
    }));
    let evidence = canceled(expected.clone()).await;
    let conditional = evidence.assess(&expected, Some("Maya"), false);
    assert_eq!(conditional.reward, 1.0, "{conditional:?}");
}

#[tokio::test]
async fn completed_forbidden_calls_are_one_failed_check_with_both_evidence_namespaces() {
    let expected = contract(json!({"service": "github", "method": "DELETE"}));
    let world = world(expected.clone());
    let signals = Signals::default();
    signals.release.notify_one();
    let app = service_app(world.clone(), &PendingApi(signals));
    assert_eq!(
        app.oneshot(request()).await.unwrap().status(),
        StatusCode::OK
    );
    world.finalize(Duration::ZERO).await;
    let evidence = capture(&world);
    let assessment = evidence.assess(&expected, Some("Maya"), false);
    assert_eq!(assessment.outcome, Outcome::Fail);
    assert_eq!(assessment.failures.len(), 1);
    assert_eq!(assessment.failures[0].evidence, [1]);
    assert_eq!(assessment.failures[0].event_evidence, [1]);
    let raw = &evidence.journal[0].replay.as_ref().unwrap().body;
    assert_eq!(
        raw.bytes().as_ref(),
        br#" { "action": "delete", "resource": {"id": "2"} } "#
    );
}

#[tokio::test]
async fn incomplete_or_conflicting_request_receipts_are_environment_faults() {
    let world = world(contract(json!({"service": "other"})));
    let signals = Signals::default();
    signals.release.notify_one();
    service_app(world.clone(), &PendingApi(signals))
        .oneshot(request())
        .await
        .unwrap();
    world.finalize(Duration::ZERO).await;
    for changed in ["head", "body", "replay", "status", "duplicate"] {
        let mut evidence = capture(&world);
        match changed {
            "head" => {
                let event = evidence
                    .events
                    .iter_mut()
                    .find(|event| matches!(event.event, EventKind::RequestHeadReceived { .. }))
                    .unwrap();
                if let EventKind::RequestHeadReceived { head } = &mut event.event {
                    head.query = Some("forged".into());
                }
            }
            "body" => {
                evidence.journal[0].replay.as_mut().unwrap().body = CapturedBody::capture(b"forged")
            }
            "replay" => evidence.journal[0].replay = None,
            "status" => evidence.journal[0].status = 500,
            "duplicate" => {
                let mut event = evidence.events[1].clone();
                event.seq = 3;
                evidence.events.insert(2, event);
                for (index, event) in evidence.events.iter_mut().enumerate() {
                    event.seq = index as u64 + 1;
                }
                evidence.finalization.event_count = evidence.events.len();
            }
            _ => unreachable!(),
        }
        let assessment =
            evidence.assess(&contract(json!({"service": "other"})), Some("Maya"), false);
        assert_eq!(
            assessment.outcome,
            Outcome::EnvironmentError,
            "{changed}: {assessment:?}"
        );
        assert!(
            assessment
                .failures
                .iter()
                .any(|failure| failure.location == "evidence/requests")
        );
    }
}

#[tokio::test]
async fn exact_header_matching_uses_redacted_receipts_for_completed_and_pending_calls() {
    for pending in [false, true] {
        for (operation, rejected) in [("Service.Delete", true), ("Service.Describe", false)] {
            let expected = contract(json!({
                "service":"github","headers":{"x-amz-target":operation}
            }));
            let evidence = if pending {
                canceled(expected.clone()).await
            } else {
                let world = world(expected.clone());
                let signals = Signals::default();
                signals.release.notify_one();
                service_app(world.clone(), &PendingApi(signals))
                    .oneshot(request())
                    .await
                    .unwrap();
                world.finalize(Duration::ZERO).await;
                capture(&world)
            };
            let raw = serde_json::to_value(&evidence).unwrap().to_string();
            assert!(!raw.contains("Service.Delete"));
            let grade = evidence.assess(&expected, Some("Maya"), false);
            assert!(grade.environment_ok, "{grade:?}");
            assert_eq!(grade.forbidden_calls_ok, !rejected, "{grade:?}");
        }
    }
}

#[tokio::test]
async fn a_header_scoped_forbidden_check_cannot_silently_pass_without_a_request_head() {
    let expected = contract(json!({
        "service":"github","headers":{"x-amz-target":"Service.Delete"}
    }));
    let world = world(expected.clone());
    let operation = world
        .begin_operation(
            OperationKind::Http,
            "github",
            "192.0.2.10",
            "DELETE",
            "/work/2",
        )
        .unwrap();
    drop(operation);
    world.finalize(Duration::ZERO).await;
    let grade = capture(&world).assess(&expected, Some("Maya"), false);
    assert_eq!(grade.outcome, Outcome::EnvironmentError, "{grade:?}");
    assert!(
        grade
            .failures
            .iter()
            .any(|failure| { failure.location == "forbidden_calls/0/request_headers" })
    );
}

#[test]
fn header_match_declarations_require_canonical_names_and_valid_nonempty_values() {
    for headers in [
        json!({"X-Amz-Target":"Service.Delete"}),
        json!({"x-amz-target":""}),
        json!({"x-amz-target":"invalid\nvalue"}),
        json!({"invalid header":"value"}),
    ] {
        assert!(
            contract(json!({"service":"github","headers":headers}))
                .validate()
                .is_err()
        );
    }
}
