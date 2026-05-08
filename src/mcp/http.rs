use std::sync::Arc;

use rmcp::transport::streamable_http_server::{
    StreamableHttpServerConfig, StreamableHttpService, session::local::LocalSessionManager,
};

use crate::mcp::{SteleMcpServer, create_server};
use crate::ops::OperationRegistry;
use crate::types::{Error, Result};

/// Start the MCP server over Streamable HTTP transport.
///
/// Binds to `host:port` and serves MCP JSON-RPC at the `/mcp` endpoint.
///
/// The server operates in stateful mode by default, meaning:
/// - `POST /mcp` with an `initialize` request creates a session and returns
///   an `MCP-Session-Id` header.
/// - Subsequent `POST /mcp` requests must include `MCP-Session-Id` and
///   `MCP-Protocol-Version` headers.
/// - `GET /mcp` opens an SSE stream for server-initiated messages.
/// - `DELETE /mcp` terminates the session.
///
/// DNS rebinding protection is enforced by validating `Host` and `Origin`
/// headers, restricting access to localhost by default.
pub async fn run_http(registry: Arc<OperationRegistry>, host: &str, port: u16) -> Result<()> {
    let server = create_server(registry);

    let addr = format!("{host}:{port}");

    // Build allowed origins for DNS rebinding protection.
    // Only localhost variants with the configured port are permitted.
    let allowed_origins = build_localhost_origins(port);

    let config = StreamableHttpServerConfig::default()
        .with_stateful_mode(true)
        .with_json_response(false)
        .with_allowed_origins(allowed_origins);

    let service: StreamableHttpService<SteleMcpServer, LocalSessionManager> =
        StreamableHttpService::new(
            move || Ok(server.clone()),
            Arc::new(LocalSessionManager::default()),
            config,
        );

    let router = axum::Router::new().nest_service("/mcp", service);

    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .map_err(|e| Error::Mcp(format!("failed to bind to {addr}: {e}")))?;

    tracing::info!("MCP HTTP server listening on {addr}");

    axum::serve(listener, router)
        .await
        .map_err(|e| Error::Mcp(format!("HTTP server error: {e}")))
}

/// Build the list of allowed localhost origins for a given port.
fn build_localhost_origins(port: u16) -> Vec<String> {
    vec![
        format!("http://localhost:{port}"),
        format!("http://127.0.0.1:{port}"),
        format!("http://[::1]:{port}"),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::*;

    #[tokio::test]
    async fn test_http_compiles() {
        let registry = Arc::new(test_registry().await);
        let server = create_server(registry);

        let config = StreamableHttpServerConfig::default()
            .with_stateful_mode(true)
            .with_allowed_origins(build_localhost_origins(9999));

        let _service: StreamableHttpService<SteleMcpServer, LocalSessionManager> =
            StreamableHttpService::new(
                move || Ok(server.clone()),
                Arc::new(LocalSessionManager::default()),
                config,
            );
    }

    #[tokio::test]
    async fn test_http_server_start_and_initialize() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .unwrap();
        let addr = listener.local_addr().unwrap();
        let port = addr.port();

        let registry = Arc::new(test_registry().await);
        let server = create_server(registry);

        let config = StreamableHttpServerConfig::default()
            .with_stateful_mode(true)
            .with_allowed_origins(build_localhost_origins(port));

        let service: StreamableHttpService<SteleMcpServer, LocalSessionManager> =
            StreamableHttpService::new(
                move || Ok(server.clone()),
                Arc::new(LocalSessionManager::default()),
                config,
            );

        let router = axum::Router::new().nest_service("/mcp", service);

        tokio::spawn(async move {
            let _ = axum::serve(listener, router).await;
        });

        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        let client = reqwest::Client::new();
        let init_body = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "initialize",
            "params": {
                "protocolVersion": "2025-11-25",
                "capabilities": {},
                "clientInfo": { "name": "test-client", "version": "0.1" }
            },
            "id": 1
        });

        let response = client
            .post(format!("http://127.0.0.1:{port}/mcp"))
            .header("Content-Type", "application/json")
            .header("Accept", "application/json, text/event-stream")
            .body(init_body.to_string())
            .send()
            .await
            .unwrap();

        assert!(
            response.status().is_success(),
            "expected 2xx for initialize, got {}",
            response.status()
        );

        let session_id = response
            .headers()
            .get("mcp-session-id")
            .expect("expected MCP-Session-Id header on initialize response")
            .to_str()
            .unwrap()
            .to_string();

        assert!(!session_id.is_empty(), "session ID must not be empty");

        let list_body = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "tools/list",
            "params": {},
            "id": 2
        });

        let response2 = client
            .post(format!("http://127.0.0.1:{port}/mcp"))
            .header("Content-Type", "application/json")
            .header("Accept", "application/json, text/event-stream")
            .header("Mcp-Session-Id", &session_id)
            .header("Mcp-Protocol-Version", "2025-11-25")
            .body(list_body.to_string())
            .send()
            .await
            .unwrap();

        assert!(
            response2.status().is_success(),
            "expected 2xx for tools/list, got {}",
            response2.status()
        );

        let response3 = client
            .delete(format!("http://127.0.0.1:{port}/mcp"))
            .header("Mcp-Session-Id", &session_id)
            .header("Mcp-Protocol-Version", "2025-11-25")
            .send()
            .await
            .unwrap();

        assert!(
            response3.status().is_success(),
            "expected 2xx for DELETE, got {}",
            response3.status()
        );
    }

    #[tokio::test]
    async fn test_origin_validation_rejects_non_localhost() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .unwrap();
        let addr = listener.local_addr().unwrap();
        let port = addr.port();

        let registry = Arc::new(test_registry().await);
        let server = create_server(registry);

        let config = StreamableHttpServerConfig::default()
            .with_stateful_mode(true)
            .with_allowed_origins(build_localhost_origins(port));

        let service: StreamableHttpService<SteleMcpServer, LocalSessionManager> =
            StreamableHttpService::new(
                move || Ok(server.clone()),
                Arc::new(LocalSessionManager::default()),
                config,
            );

        let router = axum::Router::new().nest_service("/mcp", service);

        tokio::spawn(async move {
            let _ = axum::serve(listener, router).await;
        });

        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        let client = reqwest::Client::new();
        let init_body = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "initialize",
            "params": {
                "protocolVersion": "2025-11-25",
                "capabilities": {},
                "clientInfo": { "name": "test-client", "version": "0.1" }
            },
            "id": 1
        });

        let response = client
            .post(format!("http://127.0.0.1:{port}/mcp"))
            .header("Content-Type", "application/json")
            .header("Accept", "application/json, text/event-stream")
            .header("Origin", "https://evil.example.com")
            .body(init_body.to_string())
            .send()
            .await
            .unwrap();

        assert!(
            response.status() == reqwest::StatusCode::FORBIDDEN,
            "expected 403 for non-localhost origin, got {}",
            response.status()
        );
    }

    #[test]
    fn test_build_localhost_origins() {
        let origins = build_localhost_origins(8080);
        assert_eq!(origins.len(), 3);
        assert_eq!(origins[0], "http://localhost:8080");
        assert_eq!(origins[1], "http://127.0.0.1:8080");
        assert_eq!(origins[2], "http://[::1]:8080");
    }
}
