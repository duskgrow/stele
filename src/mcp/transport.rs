use std::convert::Infallible;
use std::sync::Arc;
use std::time::Duration;

use axum::extract::State;
use axum::http::{HeaderMap, HeaderName, StatusCode};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use dashmap::DashMap;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;
use tracing::{info, warn};
use uuid::Uuid;

use super::protocol::JsonRpcRequest;
use super::server::McpServer;
use crate::config::McpConfig;

type SseSender = mpsc::Sender<Result<Event, Infallible>>;
type SseReceiver = mpsc::Receiver<Result<Event, Infallible>>;

struct Session;

pub struct StreamableHttpTransport;

impl StreamableHttpTransport {
    pub fn router(mcp_server: Arc<McpServer>, config: McpConfig) -> Router {
        let sessions: Arc<DashMap<String, Session>> = Arc::new(DashMap::new());

        let state = AppState {
            mcp_server,
            sessions,
            config,
        };

        Router::new()
            .route("/mcp", post(handle_mcp_post))
            .route("/mcp", get(handle_mcp_get))
            .with_state(state)
            .layer(cors_layer())
            .layer(TraceLayer::new_for_http())
    }
}

#[derive(Clone)]
struct AppState {
    mcp_server: Arc<McpServer>,
    sessions: Arc<DashMap<String, Session>>,
    config: McpConfig,
}

async fn handle_mcp_post(
    headers: HeaderMap,
    State(state): State<AppState>,
    Json(req): Json<JsonRpcRequest>,
) -> Response {
    if let Some(ref expected) = state.config.api_key {
        if !check_api_key(&headers, expected) {
            return (StatusCode::UNAUTHORIZED, "invalid api key").into_response();
        }
    }

    let session_id = extract_or_create_session(&headers, &state.sessions);
    let is_streaming = headers
        .get("accept")
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.contains("text/event-stream"));

    if is_streaming {
        build_sse_response(state.mcp_server, req, &session_id).await
    } else {
        build_json_response(state.mcp_server, req, &session_id).await
    }
}

async fn handle_mcp_get(headers: HeaderMap, State(state): State<AppState>) -> Response {
    if let Some(ref expected) = state.config.api_key {
        if !check_api_key(&headers, expected) {
            return (StatusCode::UNAUTHORIZED, "invalid api key").into_response();
        }
    }

    let (tx, rx): (SseSender, SseReceiver) = mpsc::channel(16);

    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(30)).await;
            let event = Event::default().event("notification").data(
                r#"{"jsonrpc":"2.0","method":"notifications/resources/list_changed","params":{}}"#,
            );
            if tx.send(Ok(event)).await.is_err() {
                break;
            }
        }
    });

    Sse::new(ReceiverStream::new(rx))
        .keep_alive(KeepAlive::default())
        .into_response()
}

fn extract_or_create_session(headers: &HeaderMap, sessions: &DashMap<String, Session>) -> String {
    let existing = headers
        .get("mcp-session-id")
        .and_then(|v| v.to_str().ok())
        .filter(|id| sessions.contains_key(*id));

    if let Some(id) = existing {
        return id.to_string();
    }

    let id = Uuid::new_v4().to_string();
    sessions.insert(id.clone(), Session);
    id
}

fn check_api_key(headers: &HeaderMap, expected: &str) -> bool {
    let from_bearer = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(|token| token == expected);

    let from_header = headers
        .get("x-api-key")
        .and_then(|v| v.to_str().ok())
        .map(|token| token == expected);

    from_bearer.unwrap_or(false) || from_header.unwrap_or(false)
}

async fn build_json_response(
    server: Arc<McpServer>,
    req: JsonRpcRequest,
    session_id: &str,
) -> Response {
    match server.handle(req).await {
        Some(resp) => (
            StatusCode::OK,
            [("mcp-session-id", session_id.to_string())],
            Json(resp),
        )
            .into_response(),
        None => (
            StatusCode::ACCEPTED,
            [("mcp-session-id", session_id.to_string())],
        )
            .into_response(),
    }
}

async fn build_sse_response(
    server: Arc<McpServer>,
    req: JsonRpcRequest,
    session_id: &str,
) -> Response {
    let (tx, rx): (SseSender, SseReceiver) = mpsc::channel(16);

    tokio::spawn(async move {
        if let Some(resp) = server.handle(req).await {
            match serde_json::to_string(&resp) {
                Ok(json) => {
                    let event = Event::default().event("message").data(json);
                    if tx.send(Ok(event)).await.is_err() {
                        tracing::debug!("sse client disconnected during message send");
                    }
                }
                Err(e) => warn!(error = %e, "serialization error in sse response"),
            }
        }
        if tx.send(Ok(Event::default().event("done").data("{}"))).await.is_err() {
            tracing::debug!("sse client disconnected during done send");
        }
    });

    (
        StatusCode::OK,
        [("mcp-session-id", session_id.to_string())],
        Sse::new(ReceiverStream::new(rx)).keep_alive(KeepAlive::default()),
    )
        .into_response()
}

fn cors_layer() -> CorsLayer {
    CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any)
        .expose_headers([HeaderName::from_static("mcp-session-id")])
}

pub async fn run_server(
    mcp_server: Arc<McpServer>,
    config: McpConfig,
    host: &str,
    port: u16,
) -> anyhow::Result<()> {
    let app = StreamableHttpTransport::router(mcp_server, config);
    let addr = format!("{}:{}", host, port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;

    info!("MCP server listening on {}", addr);

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    info!("MCP server shut down");
    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = tokio::signal::ctrl_c();

    #[cfg(unix)]
    {
        let mut sigterm =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()).unwrap();

        tokio::select! {
            _ = ctrl_c => warn!("received SIGINT, shutting down"),
            _ = sigterm.recv() => warn!("received SIGTERM, shutting down"),
        }
    }

    #[cfg(not(unix))]
    {
        ctrl_c.await.ok();
        warn!("received Ctrl+C, shutting down");
    }
}
