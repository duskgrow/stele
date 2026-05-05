use std::sync::Arc;

use serde_json::{Value, json};
use tracing::{debug, warn};

use super::prompts::PromptRegistry;
use super::protocol::{
    JsonRpcError, JsonRpcRequest, JsonRpcResponse, McpCapabilities, PromptsCapability,
    ResourcesCapability, ToolContent, ToolResult, ToolsCapability,
};
use super::resources::ResourceRegistry;
use crate::services::tools::ToolRegistry;
use crate::storage::FileBackend;

const PROTOCOL_VERSION: &str = "2024-11-05";
const SERVER_NAME: &str = "wikiops";
const SERVER_VERSION: &str = "0.1.0";

pub struct McpServer {
    capabilities: McpCapabilities,
    tool_registry: Arc<ToolRegistry>,
    resource_registry: Arc<ResourceRegistry>,
    prompt_registry: Arc<PromptRegistry>,
}

impl McpServer {
    pub fn new(
        tool_registry: Arc<ToolRegistry>,
        resource_registry: Arc<ResourceRegistry>,
        file_backend: Arc<dyn FileBackend>,
    ) -> Self {
        Self {
            capabilities: McpCapabilities {
                tools: Some(ToolsCapability {
                    list_changed: Some(false),
                }),
                resources: Some(ResourcesCapability {
                    list_changed: Some(false),
                    subscribe: Some(false),
                }),
                prompts: Some(PromptsCapability {
                    list_changed: Some(false),
                }),
            },
            tool_registry,
            resource_registry,
            prompt_registry: Arc::new(PromptRegistry::new(file_backend)),
        }
    }

    /// Routes request to method handler. Returns `None` for notifications (no `id`).
    pub async fn handle(&self, req: JsonRpcRequest) -> Option<JsonRpcResponse> {
        debug!(method = %req.method, id = ?req.id, "MCP request");

        if req.id.is_none() {
            self.handle_notification(&req.method).await;
            return None;
        }

        let id = req.id.clone();
        let result = match req.method.as_str() {
            "initialize" => self.handle_initialize(req.params),
            "tools/list" => self.handle_tools_list(),
            "tools/call" => self.handle_tools_call(req.params).await,
            "resources/list" => self.handle_resources_list().await,
            "resources/read" => self.handle_resources_read(req.params).await,
            "prompts/list" => self.handle_prompts_list().await,
            "prompts/get" => self.handle_prompts_get(req.params).await,
            "ping" => Ok(json!({})),
            _ => {
                warn!(method = %req.method, "unknown MCP method");
                return Some(JsonRpcResponse {
                    jsonrpc: "2.0".into(),
                    id,
                    result: None,
                    error: Some(JsonRpcError {
                        code: JsonRpcError::METHOD_NOT_FOUND,
                        message: format!("Method not found: {}", req.method),
                        data: None,
                    }),
                });
            }
        };

        Some(match result {
            Ok(value) => JsonRpcResponse {
                jsonrpc: "2.0".into(),
                id,
                result: Some(value),
                error: None,
            },
            Err(err) => JsonRpcResponse {
                jsonrpc: "2.0".into(),
                id,
                result: None,
                error: Some(err),
            },
        })
    }

    fn handle_initialize(&self, params: Option<Value>) -> Result<Value, JsonRpcError> {
        let client_protocol = params
            .as_ref()
            .and_then(|p| p.get("protocolVersion"))
            .and_then(|v| v.as_str())
            .unwrap_or("");

        debug!(client_protocol, "initialize handshake");

        Ok(json!({
            "protocolVersion": PROTOCOL_VERSION,
            "capabilities": self.capabilities,
            "serverInfo": {
                "name": SERVER_NAME,
                "version": SERVER_VERSION,
            }
        }))
    }

    fn handle_tools_list(&self) -> Result<Value, JsonRpcError> {
        Ok(json!({ "tools": self.tool_registry.list_tools() }))
    }

    async fn handle_tools_call(&self, params: Option<Value>) -> Result<Value, JsonRpcError> {
        let name = params
            .as_ref()
            .and_then(|p| p.get("name"))
            .and_then(|v| v.as_str());

        match name {
            Some(tool_name) => {
                let arguments = params
                    .as_ref()
                    .and_then(|p| p.get("arguments"))
                    .cloned()
                    .unwrap_or_else(|| json!({}));
                let result = self.tool_registry.call(tool_name, arguments).await?;
                let text = serde_json::to_string(&result).unwrap_or_else(|_| "{}".to_string());
                let tool_result = ToolResult {
                    content: vec![ToolContent {
                        content_type: "text".into(),
                        text,
                    }],
                    is_error: Some(false),
                };
                serde_json::to_value(tool_result).map_err(|e| JsonRpcError {
                    code: JsonRpcError::INTERNAL_ERROR,
                    message: format!("Serialization error: {}", e),
                    data: None,
                })
            }
            None => Err(JsonRpcError {
                code: JsonRpcError::INVALID_PARAMS,
                message: "Missing required 'name' parameter".into(),
                data: None,
            }),
        }
    }

    async fn handle_resources_list(&self) -> Result<Value, JsonRpcError> {
        let resources = self.resource_registry.list_resources().await;
        Ok(json!({ "resources": resources }))
    }

    async fn handle_resources_read(&self, params: Option<Value>) -> Result<Value, JsonRpcError> {
        let uri = params
            .as_ref()
            .and_then(|p| p.get("uri"))
            .and_then(|v| v.as_str());

        match uri {
            Some(resource_uri) => match self.resource_registry.read_resource(resource_uri).await {
                Ok(content) => Ok(json!({
                    "contents": [
                        {
                            "uri": resource_uri,
                            "mimeType": "text/markdown",
                            "text": content
                        }
                    ]
                })),
                Err(e) => {
                    warn!(uri = %resource_uri, error = %e, "resource read failed");
                    Err(JsonRpcError {
                        code: JsonRpcError::INVALID_PARAMS,
                        message: format!("Failed to read resource: {}", e),
                        data: None,
                    })
                }
            },
            None => Err(JsonRpcError {
                code: JsonRpcError::INVALID_PARAMS,
                message: "Missing required 'uri' parameter".into(),
                data: None,
            }),
        }
    }

    async fn handle_prompts_list(&self) -> Result<Value, JsonRpcError> {
        let prompts = self.prompt_registry.list().await;
        Ok(json!({ "prompts": prompts }))
    }

    async fn handle_prompts_get(&self, params: Option<Value>) -> Result<Value, JsonRpcError> {
        let name = params
            .as_ref()
            .and_then(|p| p.get("name"))
            .and_then(|v| v.as_str());

        match name {
            Some(prompt_name) => {
                let args = params
                    .as_ref()
                    .and_then(|p| p.get("arguments"))
                    .and_then(|v| v.as_object())
                    .map(|obj| {
                        obj.iter()
                            .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                            .collect()
                    })
                    .unwrap_or_default();

                match self.prompt_registry.get(prompt_name, args).await {
                    Ok(rendered) => Ok(json!({
                        "description": prompt_name,
                        "messages": [
                            {
                                "role": "user",
                                "content": {
                                    "type": "text",
                                    "text": rendered
                                }
                            }
                        ]
                    })),
                    Err(err) => Err(JsonRpcError {
                        code: JsonRpcError::INVALID_PARAMS,
                        message: err.to_string(),
                        data: None,
                    }),
                }
            }
            None => Err(JsonRpcError {
                code: JsonRpcError::INVALID_PARAMS,
                message: "Missing required 'name' parameter".into(),
                data: None,
            }),
        }
    }

    async fn handle_notification(&self, method: &str) {
        match method {
            "notifications/initialized" => {
                debug!("client initialized");
            }
            "notifications/cancelled" => {
                debug!("client cancelled request");
            }
            _ => {
                warn!(method = %method, "unknown notification");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    async fn test_server() -> McpServer {
        let backend = crate::storage::sqlite::SqliteBackend::new(":memory:")
            .await
            .unwrap();
        let registry = Arc::new(crate::services::tools::ToolRegistry::new(Arc::new(backend)));

        struct NoopBackend;
        #[async_trait::async_trait]
        impl crate::storage::FileBackend for NoopBackend {
            async fn get(&self, _: &str) -> Result<String, crate::storage::BackendError> { Err(crate::storage::BackendError::NotFound("noop".into())) }
            async fn put(&self, _: &str, _: &str) -> Result<(), crate::storage::BackendError> { Ok(()) }
            async fn append(&self, _: &str, _: &str) -> Result<(), crate::storage::BackendError> { Ok(()) }
            async fn delete(&self, _: &str) -> Result<(), crate::storage::BackendError> { Ok(()) }
            async fn list(&self, _: &str) -> Result<Vec<crate::storage::FileMeta>, crate::storage::BackendError> { Ok(vec![]) }
            async fn exists(&self, _: &str) -> Result<bool, crate::storage::BackendError> { Ok(false) }
            async fn stat(&self, _: &str) -> Result<crate::storage::FileStat, crate::storage::BackendError> { Err(crate::storage::BackendError::NotFound("noop".into())) }
        }

        let file_backend = Arc::new(NoopBackend);
        let resource_registry = Arc::new(crate::mcp::resources::ResourceRegistry::new(file_backend.clone()));
        McpServer::new(registry, resource_registry, file_backend)
    }

    fn make_request(method: &str, id: Option<Value>, params: Option<Value>) -> JsonRpcRequest {
        JsonRpcRequest {
            jsonrpc: "2.0".into(),
            id,
            method: method.into(),
            params,
        }
    }

    #[tokio::test]
    async fn initialize_returns_server_info() {
        let server = test_server().await;
        let req = make_request(
            "initialize",
            Some(json!(1)),
            Some(json!({
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": { "name": "test", "version": "0.1" }
            })),
        );

        let resp = server.handle(req).await.unwrap();
        assert_eq!(resp.id, Some(json!(1)));
        assert!(resp.error.is_none());

        let result = resp.result.unwrap();
        assert_eq!(result["protocolVersion"], PROTOCOL_VERSION);
        assert_eq!(result["serverInfo"]["name"], SERVER_NAME);
        assert!(result["capabilities"]["tools"].is_object());
    }

    #[tokio::test]
    async fn ping_returns_empty_object() {
        let server = test_server().await;
        let req = make_request("ping", Some(json!(2)), None);

        let resp = server.handle(req).await.unwrap();
        assert_eq!(resp.result, Some(json!({})));
    }

    #[tokio::test]
    async fn tools_list_includes_brain_stats() {
        let server = test_server().await;
        let req = make_request("tools/list", Some(json!(3)), None);

        let resp = server.handle(req).await.unwrap();
        let result = resp.result.unwrap();
        let tools = result["tools"].as_array().unwrap();
        assert!(
            tools.iter().any(|t| t["name"] == "brain_stats"),
            "brain_stats should be registered"
        );
    }

    #[tokio::test]
    async fn resources_list_returns_log_latest() {
        let server = test_server().await;
        let req = make_request("resources/list", Some(json!(4)), None);

        let resp = server.handle(req).await.unwrap();
        let result = resp.result.unwrap();
        let resources = result["resources"].as_array().unwrap();
        assert_eq!(resources.len(), 1);
        assert_eq!(resources[0]["uri"], "log://latest");
    }

    #[tokio::test]
    async fn prompts_list_returns_five_prompts() {
        // Create a mock backend with prompt files in skills/prompts/
        struct MockPromptBackend;
        #[async_trait::async_trait]
        impl crate::storage::FileBackend for MockPromptBackend {
            async fn get(&self, path: &str) -> Result<String, crate::storage::BackendError> {
                match path {
                    "skills/prompts/ingest.md" => Ok("brain://ingest\n\n## 场景\nIngest content.".into()),
                    "skills/prompts/query.md" => Ok("brain://query\n\n## 场景\nQuery knowledge.".into()),
                    "skills/prompts/enrich.md" => Ok("brain://enrich\n\n## 场景\nEnrich links.".into()),
                    "skills/prompts/maintain.md" => Ok("brain://maintain\n\n## 场景\nMaintain health.".into()),
                    "skills/prompts/deep-research.md" => Ok("brain://deep-research\n\n## 场景\nDeep research.".into()),
                    _ => Err(crate::storage::BackendError::NotFound(path.into())),
                }
            }
            async fn put(&self, _: &str, _: &str) -> Result<(), crate::storage::BackendError> { Ok(()) }
            async fn append(&self, _: &str, _: &str) -> Result<(), crate::storage::BackendError> { Ok(()) }
            async fn delete(&self, _: &str) -> Result<(), crate::storage::BackendError> { Ok(()) }
            async fn list(&self, dir: &str) -> Result<Vec<crate::storage::FileMeta>, crate::storage::BackendError> {
                if dir == "skills/prompts" {
                    Ok(vec![
                        crate::storage::FileMeta { path: "skills/prompts/ingest.md".into(), is_dir: false, size: 0, modified: None },
                        crate::storage::FileMeta { path: "skills/prompts/query.md".into(), is_dir: false, size: 0, modified: None },
                        crate::storage::FileMeta { path: "skills/prompts/enrich.md".into(), is_dir: false, size: 0, modified: None },
                        crate::storage::FileMeta { path: "skills/prompts/maintain.md".into(), is_dir: false, size: 0, modified: None },
                        crate::storage::FileMeta { path: "skills/prompts/deep-research.md".into(), is_dir: false, size: 0, modified: None },
                    ])
                } else {
                    Ok(vec![])
                }
            }
            async fn exists(&self, _: &str) -> Result<bool, crate::storage::BackendError> { Ok(false) }
            async fn stat(&self, _: &str) -> Result<crate::storage::FileStat, crate::storage::BackendError> { Err(crate::storage::BackendError::NotFound("noop".into())) }
        }

        let fb = Arc::new(MockPromptBackend);
        let db = Arc::new(crate::storage::sqlite::SqliteBackend::new(":memory:").await.unwrap());
        let tool_registry = Arc::new(crate::services::tools::ToolRegistry::new(db));
        let resource_registry = Arc::new(crate::mcp::resources::ResourceRegistry::new(fb.clone()));
        let server = McpServer::new(tool_registry, resource_registry, fb);

        let req = make_request("prompts/list", Some(json!(5)), None);
        let resp = server.handle(req).await.unwrap();
        let result = resp.result.unwrap();
        let prompts = result["prompts"].as_array().unwrap();
        assert_eq!(prompts.len(), 5);
        assert!(prompts.iter().any(|p| p["name"] == "ingest"));
        assert!(prompts.iter().any(|p| p["name"] == "query"));
    }

    #[tokio::test]
    async fn unknown_method_returns_error() {
        let server = test_server().await;
        let req = make_request("bogus/method", Some(json!(99)), None);

        let resp = server.handle(req).await.unwrap();
        assert!(resp.result.is_none());
        let err = resp.error.unwrap();
        assert_eq!(err.code, JsonRpcError::METHOD_NOT_FOUND);
    }

    #[tokio::test]
    async fn notification_returns_none() {
        let server = test_server().await;
        let req = make_request("notifications/initialized", None, None);

        let resp = server.handle(req).await;
        assert!(resp.is_none(), "notifications must not produce a response");
    }

    #[tokio::test]
    async fn tools_call_missing_name_returns_error() {
        let server = test_server().await;
        let req = make_request("tools/call", Some(json!(10)), Some(json!({})));

        let resp = server.handle(req).await.unwrap();
        let err = resp.error.unwrap();
        assert_eq!(err.code, JsonRpcError::INVALID_PARAMS);
    }

    #[tokio::test]
    async fn tools_call_brain_stats_returns_results() {
        let server = test_server().await;

        let req = make_request(
            "tools/call",
            Some(json!(12)),
            Some(json!({
                "name": "brain_stats",
                "arguments": {}
            })),
        );

        let resp = server.handle(req).await.unwrap();
        assert!(
            resp.error.is_none(),
            "expected success, got error: {:?}",
            resp.error
        );
        let result = resp.result.unwrap();
        let content = result["content"].as_array().unwrap();
        assert_eq!(content[0]["type"], "text");

        let text: serde_json::Value =
            serde_json::from_str(content[0]["text"].as_str().unwrap()).unwrap();
        assert_eq!(text["total_pages"], 0);
        assert_eq!(text["total_links"], 0);
        assert_eq!(text["orphan_pages"], 0);
    }

    #[tokio::test]
    async fn resources_read_missing_uri_returns_error() {
        let server = test_server().await;
        let req = make_request("resources/read", Some(json!(13)), Some(json!({})));

        let resp = server.handle(req).await.unwrap();
        let err = resp.error.unwrap();
        assert_eq!(err.code, JsonRpcError::INVALID_PARAMS);
    }
}
