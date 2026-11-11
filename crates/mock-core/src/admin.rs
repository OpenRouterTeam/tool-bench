use axum::{
    Json, Router,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
};
use serde_json::{Value, json};

use crate::world::SharedWorld;

/// Benchmark control plane, bound only on the admin port.
pub fn admin_app(world: SharedWorld) -> Router {
    Router::new()
        .route("/_bench/health", get(health))
        .route("/_bench/journal", get(journal))
        .route("/_bench/events", get(events))
        .route("/_bench/state", get(state))
        .route("/_bench/initial_state", get(initial_state))
        .route("/_bench/reset", post(reset))
        .with_state(world)
}

async fn health() -> Json<Value> {
    Json(json!({ "status": "ok" }))
}

async fn journal(State(world): State<SharedWorld>) -> Json<Value> {
    let world = world.lock().expect("world lock");
    Json(json!({ "entries": world.journal.entries() }))
}

async fn events(State(world): State<SharedWorld>) -> Json<Value> {
    let world = world.lock().expect("world lock");
    Json(json!({ "events": world.events().entries() }))
}

async fn state(State(world): State<SharedWorld>) -> Json<Value> {
    Json(world.lock().expect("world lock").snapshot())
}

async fn initial_state(State(world): State<SharedWorld>) -> Json<Value> {
    Json(world.lock().expect("world lock").initial_snapshot())
}

async fn reset(State(world): State<SharedWorld>) -> Response {
    match world.lock().expect("world lock").reset() {
        Ok(()) => Json(json!({ "status": "reset" })).into_response(),
        Err(error) => (
            StatusCode::CONFLICT,
            Json(json!({"error": error.to_string()})),
        )
            .into_response(),
    }
}
