//! Streamable-HTTP MCP front end (single endpoint, JSON responses, `Mcp-Session-Id`).
//! What happens behind a session is up to the [`Backend`].

use std::{
    collections::HashMap,
    future::Future,
    net::SocketAddr,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
};

use std::convert::Infallible;

use axum::response::sse::{Event, Sse};
use axum::{
    Extension, Router,
    body::Bytes,
    extract::{ConnectInfo, State},
    http::{
        HeaderMap, HeaderValue, StatusCode,
        header::{CONTENT_TYPE, USER_AGENT},
    },
    response::{IntoResponse, Response},
    routing::post,
};
use serde_json::{Value, json};
use tokio_stream::{StreamExt, wrappers::UnboundedReceiverStream};

pub const SESSION_HEADER: &str = "mcp-session-id";

/// A backend's answer to a request: one JSON-RPC response, or a stream of messages
/// (server→client requests, then the response) sent to the client as SSE.
pub enum Reply {
    Json(Value),
    Stream(tokio::sync::mpsc::UnboundedReceiver<Value>),
}

/// One MCP server implementation behind the HTTP front.
pub trait Backend: Send + Sync + 'static {
    type Session: Send + Sync + 'static;
    /// Called for an `initialize` request without a session header.
    fn open(&self) -> impl Future<Output = anyhow::Result<Self::Session>> + Send;
    /// A request (has `id` and `method`).
    fn request(
        &self,
        session: &Self::Session,
        msg: Value,
    ) -> impl Future<Output = anyhow::Result<Reply>> + Send;
    /// A notification or client response.
    fn notify(
        &self,
        session: &Self::Session,
        msg: Value,
    ) -> impl Future<Output = anyhow::Result<()>> + Send;
    fn close(&self, session: Arc<Self::Session>) -> impl Future<Output = ()> + Send;
}

/// One MCP request as the front received it: who sent it, by what it said of itself, and
/// what it asked for. No body is kept.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Access {
    /// Address of the client, without its port. Absent when the listener was not
    /// served with connection information.
    pub peer: Option<String>,
    pub user_agent: Option<String>,
    /// The session the request carried, or the one an accepted `initialize` was given.
    pub session: Option<String>,
    /// The JSON-RPC method; empty for a client response.
    pub method: String,
    /// `clientInfo.name` of an `initialize` request.
    pub client: Option<String>,
    /// The tool a `tools/call` named.
    pub tool: Option<String>,
}

/// Passive observer of the requests a front received and the response bodies a client
/// was actually served.
pub trait Observer: Send + Sync + 'static {
    fn served(&self, method: &str, body: &str);
    /// Every well-formed request, before it is answered, including one the front then
    /// refuses for its session.
    fn accessed(&self, _access: &Access) {}
}

struct Front<B: Backend> {
    backend: B,
    observer: Option<Arc<dyn Observer>>,
    sessions: Mutex<HashMap<String, Arc<B::Session>>>,
    next: AtomicU64,
}

pub fn router<B: Backend>(backend: B) -> Router {
    router_observed(backend, None)
}

/// [`router`] that also reports every served response body to `observer`.
pub fn router_observed<B: Backend>(backend: B, observer: Option<Arc<dyn Observer>>) -> Router {
    let front = Arc::new(Front {
        backend,
        observer,
        sessions: Mutex::new(HashMap::new()),
        next: AtomicU64::new(1),
    });
    Router::new()
        .route(
            "/mcp",
            post(handle_post::<B>)
                .delete(handle_delete::<B>)
                .get(|| async { StatusCode::METHOD_NOT_ALLOWED }),
        )
        .with_state(front)
}

async fn handle_post<B: Backend>(
    State(front): State<Arc<Front<B>>>,
    peer: Option<Extension<ConnectInfo<SocketAddr>>>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let Ok(msg) = serde_json::from_slice::<Value>(&body) else {
        return rpc_error(StatusCode::BAD_REQUEST, Value::Null, -32700, "Parse error");
    };
    if !msg.is_object() {
        return rpc_error(
            StatusCode::BAD_REQUEST,
            Value::Null,
            -32600,
            "Batching is not supported",
        );
    }
    let id = msg.get("id").cloned().unwrap_or(Value::Null);
    let header = headers.get(SESSION_HEADER).and_then(|v| v.to_str().ok());
    let is_initialize = msg["method"] == "initialize";
    let text = |value: &Value| value.as_str().map(str::to_owned);
    let mut access = Access {
        peer: peer.map(|Extension(ConnectInfo(address))| address.ip().to_string()),
        user_agent: headers
            .get(USER_AGENT)
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned),
        session: header.map(str::to_owned),
        method: msg["method"].as_str().unwrap_or_default().to_owned(),
        client: text(&msg["params"]["clientInfo"]["name"]).filter(|_| is_initialize),
        tool: text(&msg["params"]["name"]).filter(|_| msg["method"] == "tools/call"),
    };

    let resolved = match header {
        None if is_initialize => match front.backend.open().await {
            Ok(session) => {
                let session = Arc::new(session);
                let session_id = format!("tb-{:06}", front.next.fetch_add(1, Ordering::Relaxed));
                front
                    .sessions
                    .lock()
                    .expect("sessions lock")
                    .insert(session_id.clone(), session.clone());
                access.session = Some(session_id.clone());
                Ok((session_id, session))
            }
            Err(e) => Err(rpc_error(
                StatusCode::BAD_GATEWAY,
                id.clone(),
                -32603,
                &e.to_string(),
            )),
        },
        None => Err(rpc_error(
            StatusCode::BAD_REQUEST,
            id.clone(),
            -32600,
            "Missing Mcp-Session-Id",
        )),
        Some(sid) => match front
            .sessions
            .lock()
            .expect("sessions lock")
            .get(sid)
            .cloned()
        {
            Some(session) => Ok((sid.to_owned(), session)),
            None => Err(rpc_error(
                StatusCode::NOT_FOUND,
                id.clone(),
                -32001,
                "Session not found",
            )),
        },
    };
    if let Some(observer) = &front.observer {
        observer.accessed(&access);
    }
    let (session_id, session) = match resolved {
        Ok(resolved) => resolved,
        Err(refusal) => return refusal,
    };

    let is_request = msg.get("id").is_some() && msg.get("method").is_some();
    let method = msg["method"].as_str().unwrap_or_default().to_owned();
    let observer = front.observer.clone();
    let mut response = if is_request {
        match front.backend.request(&session, msg).await {
            Ok(Reply::Json(reply)) => {
                let body = reply.to_string();
                if let Some(observer) = &observer {
                    observer.served(&method, &body);
                }
                (StatusCode::OK, [(CONTENT_TYPE, "application/json")], body).into_response()
            }
            Ok(Reply::Stream(rx)) => {
                let events = UnboundedReceiverStream::new(rx).map(move |msg| {
                    let body = msg.to_string();
                    if let Some(observer) = &observer {
                        observer.served(&method, &body);
                    }
                    Ok::<_, Infallible>(Event::default().event("message").data(body))
                });
                Sse::new(events).into_response()
            }
            Err(e) => {
                let message = e.to_string();
                if let Some(observer) = &observer {
                    let body = json!({"jsonrpc": "2.0", "id": id, "error": {"code": -32603, "message": message}});
                    observer.served(&method, &body.to_string());
                }
                rpc_error(StatusCode::BAD_GATEWAY, id, -32603, &message)
            }
        }
    } else {
        match front.backend.notify(&session, msg).await {
            Ok(()) => StatusCode::ACCEPTED.into_response(),
            Err(e) => rpc_error(StatusCode::BAD_GATEWAY, id, -32603, &e.to_string()),
        }
    };
    response.headers_mut().insert(
        SESSION_HEADER,
        HeaderValue::from_str(&session_id).expect("session ids are ASCII"),
    );
    response
}

async fn handle_delete<B: Backend>(
    State(front): State<Arc<Front<B>>>,
    headers: HeaderMap,
) -> StatusCode {
    let Some(sid) = headers.get(SESSION_HEADER).and_then(|v| v.to_str().ok()) else {
        return StatusCode::BAD_REQUEST;
    };
    let removed = front.sessions.lock().expect("sessions lock").remove(sid);
    match removed {
        Some(session) => {
            front.backend.close(session).await;
            StatusCode::NO_CONTENT
        }
        None => StatusCode::NOT_FOUND,
    }
}

fn rpc_error(status: StatusCode, id: Value, code: i64, message: &str) -> Response {
    let body = json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}});
    (
        status,
        [(CONTENT_TYPE, "application/json")],
        body.to_string(),
    )
        .into_response()
}
