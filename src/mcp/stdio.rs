use std::sync::Arc;

use rmcp::ServiceExt;
use tokio::io::{AsyncRead, AsyncWrite};
use tracing::{error, info};

use crate::mcp::create_server;
use crate::ops::OperationRegistry;
use crate::types::Result;

/// Run the Stele MCP server over a byte-stream transport.
///
/// Creates a `SteleMcpServer` from the given `OperationRegistry` and serves it
/// using the provided reader/writer pair (e.g. stdin/stdout, or a pipe).
/// Blocks until the transport closes (e.g. EOF on the reader), then returns
/// gracefully.
pub async fn run_stdio_with<R, W>(registry: Arc<OperationRegistry>, read: R, write: W) -> Result<()>
where
    R: AsyncRead + Send + Unpin + 'static,
    W: AsyncWrite + Send + Unpin + 'static,
{
    let server = create_server(registry);

    info!("Starting Stele MCP server on stdio transport");

    let service = server
        .serve((read, write))
        .await
        .map_err(|e| crate::types::Error::Mcp(format!("stdio initialization failed: {e}")))?;

    info!("MCP server initialized, waiting for requests");

    match service.waiting().await {
        Ok(reason) => {
            info!("MCP server shutdown gracefully: {:?}", reason);
            Ok(())
        }
        Err(e) => {
            error!("MCP server task error: {e}");
            Err(crate::types::Error::Mcp(format!("stdio task error: {e}")))
        }
    }
}

/// Run the Stele MCP server over the process's real stdin/stdout.
///
/// Convenience wrapper around [`run_stdio_with`] that connects to the
/// standard I/O handles.
pub async fn run_stdio(registry: Arc<OperationRegistry>) -> Result<()> {
    run_stdio_with(registry, tokio::io::stdin(), tokio::io::stdout()).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::*;

    use rmcp::ServerHandler;

    #[tokio::test]
    async fn test_stdio_compiles() {
        let registry = Arc::new(test_registry().await);
        let server = create_server(registry);
        let info = server.get_info();
        assert_eq!(info.server_info.name, "stele");
    }

    #[tokio::test]
    async fn test_run_stdio_handles_closed_stdin() {
        let registry = Arc::new(test_registry().await);
        // Use empty() (immediate EOF) and sink() so the server sees a closed
        // transport right away — no timeout needed. The server should return
        // promptly with a connection-closed error.
        let result = run_stdio_with(registry, tokio::io::empty(), tokio::io::sink()).await;
        assert!(result.is_err(), "expected error on closed stdin, got Ok");
    }
}
