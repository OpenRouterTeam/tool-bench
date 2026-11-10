//! Integration tests for fault schedule + Stripe idempotency (proposal 13).
//!
//! Validates:
//! - Response-lost: write commits but client never receives the result body.
//! - Idempotency key replay returns the committed result without duplicating.
//! - Changed parameters under a reused key are correctly rejected.
//! - Transient read faults return errors without executing.
//! - WorldState reset replays identical fault schedules.
//! - Independently valid plan order remains correct.

mod support;

use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use mock_core::{
    dataset::Dataset,
    faults::{FaultDecision, FaultKind, FaultRule, FaultSchedule},
    serve::service_app,
    world::{SharedWorld, WorldState, shared},
};
use serde_json::Value;
use tower::ServiceExt;

#[tokio::test]
async fn cache_hits_do_not_consume_a_later_fault_occurrence() {
    for fault in [
        FaultKind::ResponseLost { occurrence: 2 },
        FaultKind::TransientError {
            status: 503,
            occurrences: vec![2],
        },
    ] {
        let (world, app) = stripe_world("business/stripe/refund-duplicate-charge");
        schedule(&world, "PostRefunds", None, fault.clone());
        for _ in 0..2 {
            let (status, _) = stripe_rest(
                &app,
                "POST",
                "/v1/refunds",
                "charge=ch_r1lxhPeTsZKQxIfuhPANytDa&amount=12000",
                &[("idempotency-key", "cached")],
            )
            .await;
            assert_eq!(status, StatusCode::OK);
        }
        assert_eq!(world.lock().unwrap().fault_records().len(), 1);
        let request = Request::builder()
            .method("POST")
            .uri("/v1/refunds")
            .header("authorization", "Bearer sk_test_xxx")
            .header("idempotency-key", "new-operation")
            .header("content-type", "application/x-www-form-urlencoded")
            .body(Body::from(
                "charge=ch_r1lxhPeTsZKQxIfuhPANytDa&amount=12000",
            ))
            .unwrap();
        let response = app.oneshot(request).await.unwrap();
        match fault {
            FaultKind::ResponseLost { .. } => {
                assert!(to_bytes(response.into_body(), usize::MAX).await.is_err());
            }
            FaultKind::TransientError { status, .. } => {
                assert_eq!(response.status().as_u16(), status);
                to_bytes(response.into_body(), usize::MAX).await.unwrap();
            }
        }
        let state = world.lock().unwrap();
        assert_eq!(state.fault_records()[1].occurrence, 2);
        mock_core::faults::validate_evidence(
            &state.initial_snapshot(),
            &state.snapshot(),
            state.fault_records(),
            state.journal.entries(),
            state.events().entries(),
        )
        .unwrap();
    }
}

#[tokio::test]
async fn replacing_a_schedule_after_activity_returns_an_error_without_panicking() {
    let (world, app) = stripe_world("business/stripe/refund-duplicate-charge");
    stripe_rest(&app, "GET", "/v1/charges", "", &[]).await;
    let result = world
        .lock()
        .unwrap()
        .set_fault_schedule(FaultSchedule::new(vec![]).unwrap());
    assert_eq!(
        result,
        Err(mock_core::faults::FaultScheduleError::TrialAlreadyStarted)
    );
    assert!(world.lock().unwrap().fault_schedule().is_none());
}

#[tokio::test]
async fn unrelated_resource_read_does_not_consume_target_fault() {
    let (world, app) = stripe_world("business/stripe/refund-duplicate-charge");
    schedule(
        &world,
        "GetChargesCharge",
        Some("ch_r1lxhPeTsZKQxIfuhPANytDa"),
        FaultKind::TransientError {
            status: 503,
            occurrences: vec![1],
        },
    );
    let (status, _) = stripe_rest(&app, "GET", "/v1/charges/ch_other", "", &[]).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = stripe_rest(
        &app,
        "GET",
        "/v1/charges/ch_r1lxhPeTsZKQxIfuhPANytDa",
        "",
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    let (status, _) = stripe_rest(
        &app,
        "GET",
        "/v1/charges/ch_r1lxhPeTsZKQxIfuhPANytDa",
        "",
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn trailing_slash_and_encoded_resource_use_the_same_replay_semantics() {
    let (world, app) = stripe_world("business/stripe/refund-duplicate-charge");
    schedule(
        &world,
        "GetChargesCharge",
        Some("ch_r1lxhPeTsZKQxIfuhPANytDa"),
        FaultKind::TransientError {
            status: 503,
            occurrences: vec![1],
        },
    );
    let (status, _) = stripe_rest(
        &app,
        "GET",
        "/v1/charges/%63h_r1lxhPeTsZKQxIfuhPANytDa/",
        "",
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    let state = world.lock().unwrap();
    mock_core::faults::validate_evidence(
        &state.initial_snapshot(),
        &state.snapshot(),
        state.fault_records(),
        state.journal.entries(),
        state.events().entries(),
    )
    .unwrap();
}

#[tokio::test]
async fn the_mcp_route_uses_the_same_semantic_fault_counter_as_rest() {
    let (world, app) = stripe_world("business/stripe/refund-duplicate-charge");
    schedule(
        &world,
        "GetChargesCharge",
        Some("ch_r1lxhPeTsZKQxIfuhPANytDa"),
        FaultKind::TransientError {
            status: 503,
            occurrences: vec![1],
        },
    );
    let request = Request::builder()
        .method("POST")
        .uri(mock_core::mcp::TOOLS_CALL_PATH)
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::json!({
                "name": "stripe_api_read",
                "arguments": {
                    "stripe_api_operation_id": "GetChargesCharge",
                    "parameters": {"charge": "ch_r1lxhPeTsZKQxIfuhPANytDa"},
                    "stripe_context": "acct_1FwY8bQk2LmN4pRs",
                    "livemode": false
                }
            })
            .to_string(),
        ))
        .unwrap();
    let response = app.clone().oneshot(request).await.unwrap();
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let result: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(result["isError"], true);
    let (status, _) = stripe_rest(
        &app,
        "GET",
        "/v1/charges/ch_r1lxhPeTsZKQxIfuhPANytDa",
        "",
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let state = world.lock().unwrap();
    mock_core::faults::validate_evidence(
        &state.initial_snapshot(),
        &state.snapshot(),
        state.fault_records(),
        state.journal.entries(),
        state.events().entries(),
    )
    .unwrap();
}

#[tokio::test]
async fn lost_response_is_a_real_network_failure_after_commit() {
    let (world, app) = stripe_world("business/stripe/refund-duplicate-charge");
    schedule(
        &world,
        "PostRefunds",
        None,
        FaultKind::ResponseLost { occurrence: 1 },
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(mock_core::serve::serve(listener, app));
    let result = reqwest::Client::new()
        .post(format!("http://{address}/v1/refunds"))
        .bearer_auth("sk_test_xxx")
        .header("content-type", "application/x-www-form-urlencoded")
        .body("charge=ch_r1lxhPeTsZKQxIfuhPANytDa&amount=24000")
        .send()
        .await;
    match result {
        Ok(response) => assert!(response.bytes().await.is_err()),
        Err(error) => assert!(!error.is_timeout(), "{error}"),
    }
    server.abort();
    let snapshot = world.lock().unwrap().snapshot();
    assert_eq!(
        snapshot["services"]["stripe"]["refunds"]
            .as_object()
            .unwrap()
            .len(),
        1
    );
}

#[tokio::test]
async fn unkeyed_retry_really_can_duplicate_the_partial_refund() {
    let (world, app) = stripe_world("business/stripe/refund-duplicate-charge");
    schedule(
        &world,
        "PostRefunds",
        None,
        FaultKind::ResponseLost { occurrence: 1 },
    );
    let request = Request::builder()
        .method("POST")
        .uri("/v1/refunds")
        .header("authorization", "Bearer sk_test_xxx")
        .header("content-type", "application/x-www-form-urlencoded")
        .body(Body::from(
            "charge=ch_r1lxhPeTsZKQxIfuhPANytDa&amount=24000",
        ))
        .unwrap();
    let response = app.clone().oneshot(request).await.unwrap();
    assert!(to_bytes(response.into_body(), usize::MAX).await.is_err());
    let (status, _) = stripe_rest(
        &app,
        "POST",
        "/v1/refunds",
        "charge=ch_r1lxhPeTsZKQxIfuhPANytDa&amount=24000",
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let snapshot = world.lock().unwrap().snapshot();
    assert_eq!(
        snapshot["services"]["stripe"]["refunds"]
            .as_object()
            .unwrap()
            .len(),
        2
    );
}

#[tokio::test]
async fn keyed_get_requests_are_never_cached() {
    let (world, app) = stripe_world("business/stripe/refund-duplicate-charge");
    let (_, before) = stripe_rest(
        &app,
        "GET",
        "/v1/refunds",
        "",
        &[("idempotency-key", "read")],
    )
    .await;
    stripe_rest(
        &app,
        "POST",
        "/v1/refunds",
        "charge=ch_r1lxhPeTsZKQxIfuhPANytDa&amount=24000",
        &[],
    )
    .await;
    let (_, after) = stripe_rest(
        &app,
        "GET",
        "/v1/refunds",
        "",
        &[("idempotency-key", "read")],
    )
    .await;
    assert_ne!(before, after);
    assert!(
        world
            .lock()
            .unwrap()
            .service("stripe")
            .unwrap()
            .session
            .collection("_idempotency")
            .is_none()
    );
}

#[tokio::test]
async fn lost_response_evidence_replays_and_rejects_record_and_delivery_substitution() {
    let (world, app) = stripe_world("business/stripe/refund-duplicate-charge");
    schedule(
        &world,
        "PostRefunds",
        None,
        FaultKind::ResponseLost { occurrence: 1 },
    );
    let request = Request::builder()
        .method("POST")
        .uri("/v1/refunds")
        .header("authorization", "Bearer sk_test_xxx")
        .header("idempotency-key", "evidence")
        .header("content-type", "application/x-www-form-urlencoded")
        .body(Body::from(
            "charge=ch_r1lxhPeTsZKQxIfuhPANytDa&amount=24000",
        ))
        .unwrap();
    let response = app.oneshot(request).await.unwrap();
    assert!(to_bytes(response.into_body(), usize::MAX).await.is_err());
    let state = world.lock().unwrap();
    let validate = |records: &[mock_core::faults::FaultRecord],
                    events: &[mock_core::events::OperationEvent]| {
        mock_core::faults::validate_evidence(
            &state.initial_snapshot(),
            &state.snapshot(),
            records,
            state.journal.entries(),
            events,
        )
    };
    validate(state.fault_records(), state.events().entries()).unwrap();
    let mut records = state.fault_records().to_vec();
    records[0].resource = Some("ch_other".into());
    assert!(validate(&records, state.events().entries()).is_err());
    assert!(validate(&[], state.events().entries()).is_err());
    let events: Vec<_> = state
        .events()
        .entries()
        .iter()
        .filter(|event| {
            !matches!(
                event.event,
                mock_core::events::EventKind::ResponseDeliveryLost
            )
        })
        .cloned()
        .collect();
    assert!(validate(state.fault_records(), &events).is_err());
}

#[tokio::test]
async fn full_refund_retry_passes_sealed_contract_and_reset_replays_the_loss() {
    let directory = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../scenarios/business/stripe/refund-duplicate-charge");
    let mut dataset = Dataset::load(&directory).unwrap();
    dataset.contract.interfaces = mock_core::grading::InterfacePolicy::DirectAllowed;
    dataset.fault_schedule = Some(
        FaultSchedule::new(vec![FaultRule {
            service: "stripe".into(),
            operation_id: "PostRefunds".into(),
            resource: None,
            fault: FaultKind::ResponseLost { occurrence: 1 },
        }])
        .unwrap(),
    );
    let contract = dataset.contract.clone();
    let world = shared(WorldState::from_dataset(dataset).unwrap());
    let app = service_app(world.clone(), &mock_stripe::Stripe);
    for attempt in 0..2 {
        if attempt > 0 {
            world.lock().unwrap().reset().unwrap();
        }
        let request = Request::builder()
            .method("POST")
            .uri("/v1/refunds")
            .header("authorization", "Bearer sk_test_xxx")
            .header("idempotency-key", "full-refund")
            .header("content-type", "application/json")
            .body(Body::from(
                serde_json::json!({
                    "charge": "ch_r1lxhPeTsZKQxIfuhPANytDa",
                    "amount": 48000,
                    "reason": "duplicate",
                })
                .to_string(),
            ))
            .unwrap();
        let response = app.clone().oneshot(request).await.unwrap();
        assert!(to_bytes(response.into_body(), usize::MAX).await.is_err());
        let (status, _) = stripe_rest(
            &app,
            "POST",
            "/v1/refunds",
            "reason=duplicate&amount=48000&charge=ch_r1lxhPeTsZKQxIfuhPANytDa",
            &[("idempotency-key", "full-refund")],
        )
        .await;
        assert_eq!(status, StatusCode::OK);
    }
    world.finalize(std::time::Duration::ZERO).await;
    let evidence = mock_core::evidence::WorldEvidence::capture(
        &world.lock().unwrap(),
        &[],
        Ok(vec!["unknown".into()]),
    );
    let encoded = serde_json::to_vec(&evidence).unwrap();
    let decoded: mock_core::evidence::WorldEvidence = serde_json::from_slice(&encoded).unwrap();
    let assessment = decoded.assess(&contract, None, false);
    assert_eq!(
        assessment.outcome,
        mock_core::grading::Outcome::Pass,
        "{assessment:?}"
    );
    let mut changed = decoded;
    changed.fault_records.clear();
    assert_eq!(
        changed.assess(&contract, None, false).outcome,
        mock_core::grading::Outcome::EnvironmentError
    );
}

#[tokio::test]
async fn nested_json_field_order_does_not_change_idempotency_fingerprint() {
    let (world, app) = stripe_world("business/stripe/refund-duplicate-charge");
    let mut results = Vec::new();
    for body in [
        r#"{"name":"Order","metadata":{"alpha":"one","beta":"two"}}"#,
        r#"{"metadata":{"beta":"two","alpha":"one"},"name":"Order"}"#,
    ] {
        let request = Request::builder()
            .method("POST")
            .uri("/v1/customers")
            .header("authorization", "Bearer sk_test_xxx")
            .header("idempotency-key", "ordered-customer")
            .header("content-type", "application/json")
            .body(Body::from(body))
            .unwrap();
        let response = app.clone().oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        results.push(to_bytes(response.into_body(), usize::MAX).await.unwrap());
    }
    assert_eq!(results[0], results[1]);
    assert_eq!(
        world
            .lock()
            .unwrap()
            .service("stripe")
            .unwrap()
            .session
            .collection("_idempotency")
            .unwrap()
            .iter()
            .count(),
        1,
    );
}

fn schedule(world: &SharedWorld, operation_id: &str, resource: Option<&str>, fault: FaultKind) {
    world
        .lock()
        .unwrap()
        .set_fault_schedule(
            FaultSchedule::new(vec![FaultRule {
                service: "stripe".into(),
                operation_id: operation_id.into(),
                resource: resource.map(str::to_owned),
                fault,
            }])
            .unwrap(),
        )
        .unwrap();
}

#[tokio::test]
async fn transient_fault_is_applied_by_the_real_router_without_mutation() {
    let (world, app) = stripe_world("business/stripe/refund-duplicate-charge");
    schedule(
        &world,
        "PostRefunds",
        Some("ch_r1lxhPeTsZKQxIfuhPANytDa"),
        FaultKind::TransientError {
            status: 503,
            occurrences: vec![1],
        },
    );
    let initial = world.lock().unwrap().snapshot()["services"].clone();
    let (status, _) = stripe_rest(
        &app,
        "POST",
        "/v1/refunds",
        "charge=ch_r1lxhPeTsZKQxIfuhPANytDa&amount=48000",
        &[("idempotency-key", "retry")],
    )
    .await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(world.lock().unwrap().snapshot()["services"], initial);
    let (status, _) = stripe_rest(
        &app,
        "POST",
        "/v1/refunds",
        "charge=ch_r1lxhPeTsZKQxIfuhPANytDa&amount=48000",
        &[("idempotency-key", "retry")],
    )
    .await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn committed_refund_has_a_lost_body_and_keyed_retry_does_not_duplicate() {
    let (world, app) = stripe_world("business/stripe/refund-duplicate-charge");
    schedule(
        &world,
        "PostRefunds",
        None,
        FaultKind::ResponseLost { occurrence: 1 },
    );
    let request = Request::builder()
        .method("POST")
        .uri("/v1/refunds")
        .header("host", "world:8121")
        .header("authorization", "Bearer sk_test_xxx")
        .header("idempotency-key", "lost-refund")
        .header("content-type", "application/x-www-form-urlencoded")
        .body(Body::from(
            "charge=ch_r1lxhPeTsZKQxIfuhPANytDa&amount=24000&reason=duplicate",
        ))
        .unwrap();
    let response = app.clone().oneshot(request).await.unwrap();
    assert!(to_bytes(response.into_body(), usize::MAX).await.is_err());
    let committed = world.lock().unwrap().snapshot()["services"].clone();
    assert_eq!(committed["stripe"]["refunds"].as_object().unwrap().len(), 1);
    let (status, body) = stripe_rest(
        &app,
        "POST",
        "/v1/refunds",
        "reason=duplicate&amount=24000&charge=ch_r1lxhPeTsZKQxIfuhPANytDa",
        &[("idempotency-key", "lost-refund")],
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["id"].as_str().unwrap().starts_with("re_"));
    assert_eq!(world.lock().unwrap().snapshot()["services"], committed);
}

#[tokio::test]
async fn the_same_key_cannot_be_reused_on_a_different_path_resource() {
    let (_world, app) = stripe_world("business/stripe/refund-duplicate-charge");
    let (status, _) = stripe_rest(&app, "POST", "/v1/customers", "name=First", &[]).await;
    assert_eq!(status, StatusCode::OK);
    let (_, first) = stripe_rest(&app, "POST", "/v1/customers", "name=Second", &[]).await;
    let (_, second) = stripe_rest(&app, "POST", "/v1/customers", "name=Third", &[]).await;
    let path_first = format!("/v1/customers/{}", first["id"].as_str().unwrap());
    let path_second = format!("/v1/customers/{}", second["id"].as_str().unwrap());
    let (status, _) = stripe_rest(
        &app,
        "POST",
        &path_first,
        "name=Changed",
        &[("idempotency-key", "same-key")],
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = stripe_rest(
        &app,
        "POST",
        &path_second,
        "name=Changed",
        &[("idempotency-key", "same-key")],
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

fn stripe_world(scenario_id: &str) -> (SharedWorld, Router) {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../scenarios")
        .join(scenario_id);
    let dataset = Dataset::load(&dir).unwrap();
    let world = shared(WorldState::from_dataset(dataset).unwrap());
    let app = service_app(world.clone(), &mock_stripe::Stripe);
    (world, app)
}

async fn stripe_rest(
    app: &Router,
    method: &str,
    path: &str,
    body: &str,
    headers: &[(&str, &str)],
) -> (StatusCode, Value) {
    let mut builder = Request::builder()
        .method(method)
        .uri(path)
        .header("host", "world:8121")
        .header("authorization", "Bearer sk_test_xxx")
        .header("content-type", "application/x-www-form-urlencoded");
    for (k, v) in headers {
        builder = builder.header(*k, *v);
    }
    let request = builder.body(Body::from(body.to_owned())).unwrap();
    let response = app.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let value: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, value)
}

// ---------------------------------------------------------------------------
// Idempotency key basics (no faults)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn idempotency_key_returns_cached_result_on_replay() {
    let (world, app) = stripe_world("business/stripe/refund-duplicate-charge");

    // First refund with idempotency key
    let (status, body) = stripe_rest(
        &app,
        "POST",
        "/v1/refunds",
        "charge=ch_r1lxhPeTsZKQxIfuhPANytDa&amount=48000&reason=duplicate",
        &[("idempotency-key", "idem_refund_1")],
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let refund_id = body["id"].as_str().unwrap().to_owned();
    assert!(refund_id.starts_with("re_"));

    // Replay with same key and params
    let (status2, body2) = stripe_rest(
        &app,
        "POST",
        "/v1/refunds",
        "charge=ch_r1lxhPeTsZKQxIfuhPANytDa&amount=48000&reason=duplicate",
        &[("idempotency-key", "idem_refund_1")],
    )
    .await;
    assert_eq!(status2, StatusCode::OK);
    assert_eq!(body2["id"], refund_id, "replay returns the same refund");

    // Verify only one refund was created (no duplication)
    let state = world.lock().unwrap();
    let refunds = state
        .service("stripe")
        .unwrap()
        .store
        .collection("refunds")
        .unwrap();
    let refund_count = refunds.iter().count();
    assert_eq!(refund_count, 1, "exactly one refund created despite replay");
}

#[tokio::test]
async fn idempotency_key_rejects_changed_parameters() {
    let (_world, app) = stripe_world("business/stripe/refund-duplicate-charge");

    // First refund
    let (status, _) = stripe_rest(
        &app,
        "POST",
        "/v1/refunds",
        "charge=ch_r1lxhPeTsZKQxIfuhPANytDa&amount=48000&reason=duplicate",
        &[("idempotency-key", "idem_refund_2")],
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    // Same key, different amount
    let (status2, body2) = stripe_rest(
        &app,
        "POST",
        "/v1/refunds",
        "charge=ch_r1lxhPeTsZKQxIfuhPANytDa&amount=10000&reason=duplicate",
        &[("idempotency-key", "idem_refund_2")],
    )
    .await;
    assert_eq!(status2, StatusCode::BAD_REQUEST);
    assert!(
        body2["error"]["code"]
            .as_str()
            .unwrap_or_default()
            .contains("idempotency_key_in_use"),
        "should reject with idempotency_key_in_use, got {body2}"
    );
}

// ---------------------------------------------------------------------------
// Fault schedule integration with WorldState
// ---------------------------------------------------------------------------

#[tokio::test]
async fn fault_schedule_on_world_state_persists_through_check() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../scenarios/business/stripe/refund-duplicate-charge");
    let dataset = Dataset::load(&dir).unwrap();
    let mut state = WorldState::from_dataset(dataset).unwrap();

    let schedule = FaultSchedule::new(vec![FaultRule {
        service: "stripe".into(),
        operation_id: "GetChargesCharge".into(),
        resource: None,
        fault: FaultKind::TransientError {
            status: 503,
            occurrences: vec![1],
        },
    }])
    .unwrap();
    state.set_fault_schedule(schedule).unwrap();

    // First check: transient
    let d1 = state.check_fault("stripe", "GetChargesCharge", None);
    assert_eq!(d1, FaultDecision::TransientError { status: 503 });

    // Second check: none
    let d2 = state.check_fault("stripe", "GetChargesCharge", None);
    assert_eq!(d2, FaultDecision::None);

    // Records captured
    assert_eq!(state.fault_records().len(), 2);
}

#[tokio::test]
async fn world_reset_replays_fault_schedule() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../scenarios/business/stripe/refund-duplicate-charge");
    let dataset = Dataset::load(&dir).unwrap();
    let mut state = WorldState::from_dataset(dataset).unwrap();

    let schedule = FaultSchedule::new(vec![FaultRule {
        service: "stripe".into(),
        operation_id: "PostRefunds".into(),
        resource: None,
        fault: FaultKind::ResponseLost { occurrence: 1 },
    }])
    .unwrap();
    state.set_fault_schedule(schedule).unwrap();

    let d1 = state.check_fault("stripe", "PostRefunds", None);
    assert_eq!(d1, FaultDecision::ResponseLost);
    let d2 = state.check_fault("stripe", "PostRefunds", None);
    assert_eq!(d2, FaultDecision::None);

    state.reset().unwrap();

    // After reset, same schedule replays
    let d3 = state.check_fault("stripe", "PostRefunds", None);
    assert_eq!(d3, FaultDecision::ResponseLost);

    // Records cleared on reset
    assert_eq!(state.fault_records().len(), 1);
}

// ---------------------------------------------------------------------------
// Missing instrumentation is an environment/admission error
// ---------------------------------------------------------------------------

#[test]
fn missing_instrumentation_is_environment_error() {
    let err = FaultSchedule::new(vec![FaultRule {
        service: "jira".into(),
        operation_id: "CreateIssue".into(),
        resource: None,
        fault: FaultKind::TransientError {
            status: 500,
            occurrences: vec![1],
        },
    }])
    .unwrap_err();
    assert!(
        matches!(
            err,
            mock_core::faults::FaultScheduleError::UnsupportedOperation { .. }
        ),
        "unsupported operation must be an error, not silent: {err:?}"
    );
}
