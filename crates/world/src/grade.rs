//! Verifier endpoints. The task's `tests/test.sh` posts the agent's answer, then asks the
//! world to grade itself against `tests/expect.json`; the reply is Harbor's `reward.json`.

use std::{collections::HashMap, sync::LazyLock, time::Duration};

use axum::{
    Json, Router,
    extract::{Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
};
use mock_core::{
    evidence::WorldEvidence,
    grading::{Expect, Grade},
    lifecycle::Phase,
    startup::StartupCall,
    world::SharedWorld,
};
use serde_json::{Value, json};

/// Written by `deploy/capture-stack.sh` from a capture of every MCP endpoint.
static STARTUP_CALLS: LazyLock<Vec<StartupCall>> = LazyLock::new(|| {
    serde_json::from_str(include_str!("../startup-calls.json")).expect("startup-calls.json")
});

/// Compose service name of the tools container; its IPs are the legitimate API callers.
pub const TOOLS_HOST: &str = "tools";

pub fn routes(world: SharedWorld) -> Router {
    Router::new()
        .route("/_bench/answer", post(answer))
        .route("/_bench/grade", post(grade_trial))
        .route("/_bench/assessment", post(assess_trial))
        .route("/_bench/evidence", get(export_evidence))
        .route("/_bench/finalize", post(finalize_evidence))
        .with_state(world)
}

async fn answer(State(world): State<SharedWorld>, body: String) -> Response {
    let mut world = world.lock().expect("world lock");
    if world.finalization().phase != Phase::Open {
        return StatusCode::CONFLICT.into_response();
    }
    world.answer = Some(body);
    Json(json!({ "status": "stored" })).into_response()
}

/// Query: `context_overflow=1` when the agent logs show a length rejection;
/// `tools_host` overrides [`TOOLS_HOST`] (tests).
async fn grade_trial(
    State(world): State<SharedWorld>,
    Query(query): Query<HashMap<String, String>>,
    Json(expect): Json<Expect>,
) -> Json<Value> {
    Json(json!(evaluate(world, query, expect).await.metrics()))
}

async fn assess_trial(
    State(world): State<SharedWorld>,
    Query(query): Query<HashMap<String, String>>,
    Json(expect): Json<Expect>,
) -> Json<Grade> {
    Json(evaluate(world, query, expect).await)
}

async fn export_evidence(
    State(world): State<SharedWorld>,
    Query(query): Query<HashMap<String, String>>,
) -> Json<WorldEvidence> {
    Json(capture(world, &query).await)
}

async fn finalize_evidence(
    State(world): State<SharedWorld>,
    Query(query): Query<HashMap<String, String>>,
) -> Json<WorldEvidence> {
    world.finalize(Duration::from_secs(20)).await;
    Json(capture(world, &query).await)
}

async fn evaluate(world: SharedWorld, query: HashMap<String, String>, expect: Expect) -> Grade {
    world.finalize(Duration::from_secs(20)).await;
    let evidence = capture(world, &query).await;
    evidence.assess(
        &expect,
        evidence.answer.as_deref(),
        query
            .get("context_overflow")
            .is_some_and(|value| value == "1"),
    )
}

async fn capture(world: SharedWorld, query: &HashMap<String, String>) -> WorldEvidence {
    let host = query.get("tools_host").map_or(TOOLS_HOST, String::as_str);
    let origin = tokio::net::lookup_host((host, 0))
        .await
        .map(|addresses| addresses.map(|address| address.ip().to_string()).collect())
        .map_err(|_| "the producer could not resolve the trusted tools host".to_owned());
    let world = world.lock().expect("world lock");
    WorldEvidence::capture(&world, &STARTUP_CALLS, origin)
}
