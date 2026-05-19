/// MCP HTTP transport.
pub mod http;

use std::sync::Arc;

use rmcp::{
    ServerHandler,
    model::{
        CallToolRequestParams, CallToolResult, Content, ErrorData, JsonObject, ListToolsResult,
        PaginatedRequestParams, ProtocolVersion, ServerCapabilities, ServerInfo, Tool,
    },
    service::{RequestContext, RoleServer},
};

/// MCP stdio transport.
pub mod stdio;

use crate::ops::OperationRegistry;

/// The MCP server implementation exposing stele operations as tools.
#[derive(Clone)]
pub struct SteleMcpServer {
    registry: Arc<OperationRegistry>,
}

/// Create an MCP server from an operation registry.
pub fn create_server(registry: Arc<OperationRegistry>) -> SteleMcpServer {
    SteleMcpServer { registry }
}

impl SteleMcpServer {
    fn meta_to_tool(meta: &crate::ops::OperationMeta) -> Tool {
        let schema = match meta.input_schema.clone() {
            serde_json::Value::Object(map) => Arc::new(map),
            _ => Arc::new(JsonObject::default()),
        };
        Tool::new(meta.name.clone(), meta.description.clone(), schema)
    }
}

impl ServerHandler for SteleMcpServer {
    fn get_info(&self) -> ServerInfo {
        let mut info = ServerInfo::default();
        info.protocol_version = ProtocolVersion::V_2025_11_25;
        info.capabilities = ServerCapabilities::builder().enable_tools().build();
        info.server_info.name = "stele".to_string();
        info.server_info.version = env!("CARGO_PKG_VERSION").to_string();
        info.instructions = Some(
            "Stele MCP server exposing page operations, search, graph queries, sync, maintenance, stats, and reindex tools.".to_string(),
        );
        info
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> std::result::Result<ListToolsResult, ErrorData> {
        let metas = self.registry.list_operations();
        let tools: Vec<Tool> = metas.iter().map(SteleMcpServer::meta_to_tool).collect();
        Ok(ListToolsResult::with_all_items(tools))
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> std::result::Result<CallToolResult, ErrorData> {
        let result = self
            .registry
            .execute_mcp(&request.name, request.arguments)
            .await;
        match result {
            Ok(value) => Ok(CallToolResult::structured(value)),
            Err(e) => Ok(CallToolResult::error(vec![Content::text(e.to_string())])),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::*;

    #[tokio::test]
    async fn test_tools_list() {
        let registry = Arc::new(test_registry().await);
        let metas = registry.list_operations();
        let tools: Vec<Tool> = metas.iter().map(SteleMcpServer::meta_to_tool).collect();

        assert_eq!(tools.len(), 12, "expected 12 tools, got {}", tools.len());

        let names: Vec<String> = tools.iter().map(|t| t.name.to_string()).collect();
        assert!(names.contains(&"page.get".to_string()));
        assert!(names.contains(&"page.put".to_string()));
        assert!(names.contains(&"page.delete".to_string()));
        assert!(names.contains(&"page.list".to_string()));
        assert!(names.contains(&"search".to_string()));
        assert!(names.contains(&"graph.query".to_string()));
        assert!(names.contains(&"sync".to_string()));
        assert!(names.contains(&"maintain".to_string()));
        assert!(names.contains(&"stats".to_string()));
        assert!(names.contains(&"reindex".to_string()));

        for tool in &tools {
            assert!(!tool.name.is_empty(), "tool name must not be empty");
            assert!(
                tool.description.as_ref().is_some_and(|d| !d.is_empty()),
                "tool '{}' description must not be empty",
                tool.name
            );
            assert!(
                !tool.input_schema.is_empty(),
                "tool '{}' input_schema must not be empty",
                tool.name
            );
        }
    }

    #[tokio::test]
    async fn test_tools_call_dispatch() {
        let registry = Arc::new(test_registry().await);
        let result = registry.execute_mcp("stats", None).await.unwrap();

        assert!(result.get("total_pages").is_some());
        assert!(result.get("total_links").is_some());
    }

    #[tokio::test]
    async fn test_server_info() {
        let registry = Arc::new(test_registry().await);
        let server = create_server(registry);

        let info = server.get_info();
        assert_eq!(info.server_info.name, "stele");
        assert_eq!(info.server_info.version, env!("CARGO_PKG_VERSION"));
        assert_eq!(info.protocol_version.as_str(), "2025-11-25");
        assert!(info.capabilities.tools.is_some());
    }

    #[tokio::test]
    async fn test_unknown_tool() {
        let registry = Arc::new(test_registry().await);
        let result = registry.execute_mcp("unknown.tool", None).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_call_tool_dispatches_via_registry() {
        let registry = Arc::new(test_registry().await);

        let result = registry.execute_mcp("stats", None).await;
        assert!(result.is_ok(), "stats dispatch failed: {:?}", result.err());
        let value = result.unwrap();
        assert!(value.get("total_pages").is_some());

        let result = registry.execute_mcp("reindex", None).await;
        assert!(
            result.is_ok(),
            "reindex dispatch failed: {:?}",
            result.err()
        );
        assert_eq!(result.unwrap()["reindexed"].as_bool(), Some(true));

        let result = registry.execute_mcp("unknown.tool", None).await;
        assert!(result.is_err(), "unknown tool should fail");
    }
}
