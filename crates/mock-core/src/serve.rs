use std::net::SocketAddr;

use axum::{
    Router,
    body::{Body, to_bytes},
    extract::{ConnectInfo, Request, State},
    http::{Method, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
};
use tokio::net::TcpListener;

use crate::{
    access::require_caller,
    events::{self, OperationKind},
    journal::{ResponseRecord, body_value},
    request::{CapturedBody, RequestHead},
    service::MockService,
    world::SharedWorld,
};

const MAX_BODY_BYTES: usize = 32 * 1024 * 1024;

#[derive(Clone)]
struct Recorder {
    world: SharedWorld,
    service: &'static str,
    caller_host: Option<String>,
}

/// The service's router with the declared clock policy and request journaling.
pub fn service_app(world: SharedWorld, service: &dyn MockService) -> Router {
    service_app_with_caller(world, service, None)
}

pub fn service_app_with_caller(
    world: SharedWorld,
    service: &dyn MockService,
    caller_host: Option<String>,
) -> Router {
    world
        .prepare_service(service.name(), service.initializer())
        .expect("service initialization must precede activity");
    let recorder = Recorder {
        world: world.clone(),
        service: service.name(),
        caller_host,
    };
    service
        .router()
        .layer(middleware::from_fn_with_state(recorder, record))
        .with_state(world)
}

/// Serves `app`, attaching the caller's address to each request for the journal.
pub async fn serve(listener: TcpListener, app: Router) -> std::io::Result<()> {
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await
}

async fn record(State(recorder): State<Recorder>, request: Request, next: Next) -> Response {
    let (parts, body) = request.into_parts();
    let method = parts.method.clone();
    let header = |name: &str| {
        parts
            .headers
            .get(name)
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned)
    };
    let host = header("host").or_else(|| parts.uri.authority().map(|a| a.to_string()));
    let scheme = header("x-forwarded-proto").unwrap_or_else(|| "http".to_owned());
    let content_type = header("content-type");
    let path = parts.uri.path().to_owned();
    let query = parts.uri.query().map(str::to_owned);
    let source_address = parts
        .extensions
        .get::<ConnectInfo<SocketAddr>>()
        .map(|info| info.0.ip());
    let source = source_address
        .map(|address| address.to_string())
        .unwrap_or_else(|| "unknown".to_owned());
    let Ok(operation) = recorder.world.begin_operation(
        OperationKind::Http,
        recorder.service,
        &source,
        method.as_str(),
        &path,
    ) else {
        return crate::http::json(
            StatusCode::SERVICE_UNAVAILABLE,
            serde_json::json!({"error": {"message": "service unavailable", "type": "service_unavailable"}}),
        );
    };
    operation.record_head(RequestHead {
        host,
        scheme,
        content_type,
        query,
        headers: operation.capture_headers(&parts.headers),
    });
    let request_bytes = match to_bytes(body, MAX_BODY_BYTES).await {
        Ok(bytes) => bytes,
        Err(_) => {
            operation.finish(StatusCode::PAYLOAD_TOO_LARGE.as_u16(), true);
            return StatusCode::PAYLOAD_TOO_LARGE.into_response();
        }
    };
    operation.record_body(CapturedBody::capture(&request_bytes));

    let (mut response_parts, response_bytes, body_complete) =
        events::scope(operation.context().clone(), async {
            let access = match &recorder.caller_host {
                Some(host) => require_caller(host, source_address, false).await,
                None => Ok(()),
            };
            let response = match access {
                Ok(()) => {
                    let clock_step = if is_mutating(&method) {
                        recorder.world.transport_clock_step()
                    } else {
                        Ok(())
                    };
                    match clock_step {
                        Ok(()) => next.run(Request::from_parts(parts, Body::from(request_bytes.clone()))).await,
                        Err(error) => {
                            recorder.world.fail_operation(&operation, "business_clock_invalid", &error.to_string());
                            crate::http::json(
                                StatusCode::SERVICE_UNAVAILABLE,
                                serde_json::json!({"error": {"message": "service unavailable", "type": "service_unavailable"}}),
                            )
                        }
                    }
                }
                Err(StatusCode::SERVICE_UNAVAILABLE) => {
                    recorder.world.fail_operation(
                        &operation,
                        "caller_resolution_failed",
                        "the trusted API caller host could not be resolved",
                    );
                    crate::http::json(
                        StatusCode::SERVICE_UNAVAILABLE,
                        serde_json::json!({"error": {"message": "service unavailable", "type": "service_unavailable"}}),
                    )
                }
                Err(status) => crate::http::json(
                    status,
                    serde_json::json!({"error": {"message": "access denied", "type": "access_denied"}}),
                ),
            };
            let (response_parts, response_body) = response.into_parts();
            match to_bytes(response_body, MAX_BODY_BYTES).await {
                Ok(bytes) => (response_parts, bytes, true),
                Err(_) => (response_parts, Default::default(), false),
            }
        })
        .await;
    if !body_complete {
        response_parts.status = StatusCode::INTERNAL_SERVER_ERROR;
    }
    let delivery_lost = response_parts
        .extensions
        .remove::<crate::faults::ResponseDeliveryLost>()
        .is_some();
    if delivery_lost {
        operation.record_delivery_loss();
    }

    operation.record_response(ResponseRecord {
        status: response_parts.status.as_u16(),
        response_body: body_value(&response_bytes),
    });
    operation.finish(response_parts.status.as_u16(), body_complete);

    let body = if delivery_lost || !body_complete {
        response_parts
            .headers
            .remove(axum::http::header::CONTENT_LENGTH);
        crate::faults::failed_response_body()
    } else {
        Body::from(response_bytes)
    };
    Response::from_parts(response_parts, body)
}

fn is_mutating(method: &Method) -> bool {
    !matches!(*method, Method::GET | Method::HEAD | Method::OPTIONS)
}
