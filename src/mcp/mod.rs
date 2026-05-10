/// MCP HTTP transport.
pub mod http;

use std::sync::Arc;

use rmcp::{
    ServerHandler,
    model::{
        CallToolRequestParams, CallToolResult, Content, ErrorCode, ErrorData, JsonObject,
        ListToolsResult, PaginatedRequestParams, ProtocolVersion, ServerCapabilities, ServerInfo,
        Tool,
    },
    service::{RequestContext, RoleServer},
};

/// MCP stdio transport.
pub mod stdio;

use crate::ops::{Operation, OperationRegistry};
use crate::types::TimelineAppendInput;

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

    fn parse_operation(
        name: &str,
        args: Option<JsonObject>,
    ) -> std::result::Result<Operation, ErrorData> {
        let args = args.unwrap_or_default();

        let get_string = |key: &str| -> std::result::Result<String, ErrorData> {
            args.get(key)
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .ok_or_else(|| {
                    ErrorData::invalid_params(
                        format!("missing required field: {}", key),
                        None,
                    )
                })
        };

        let get_opt_string = |key: &str| -> Option<String> {
            args.get(key).and_then(|v| v.as_str()).map(|s| s.to_string())
        };

        let get_opt_i64 = |key: &str| -> Option<i64> {
            args.get(key).and_then(|v| v.as_i64())
        };

        let get_opt_usize = |key: &str| -> Option<usize> {
            args.get(key).and_then(|v| v.as_u64()).map(|n| n as usize)
        };

        let get_opt_value = |key: &str| -> Option<serde_json::Value> {
            args.get(key).cloned()
        };

        match name {
            "page.get" => Ok(Operation::PageGet {
                slug: get_string("slug")?,
            }),
            "page.put" => {
                let slug = get_string("slug")?;
                let body = get_string("body")?;
                let frontmatter_updates = get_opt_value("frontmatter");
                let etag = get_opt_string("etag");

                let timeline_obj = args.get("timeline").ok_or_else(|| {
                    ErrorData::invalid_params(
                        "missing required field: timeline".to_string(),
                        None,
                    )
                })?;
                let timeline_content = timeline_obj
                    .get("content")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
                    .ok_or_else(|| {
                        ErrorData::invalid_params(
                            "missing required field: timeline.content".to_string(),
                            None,
                        )
                    })?;
                let timeline_agent = timeline_obj
                    .get("agent")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());

                Ok(Operation::PagePut {
                    slug,
                    body,
                    frontmatter_updates,
                    timeline_append: TimelineAppendInput {
                        content: timeline_content,
                        agent: timeline_agent,
                    },
                    etag,
                })
            }
            "page.delete" => Ok(Operation::PageDelete {
                slug: get_string("slug")?,
            }),
            "page.list" => Ok(Operation::PageList {
                dir: get_opt_string("dir"),
            }),
            "search" => Ok(Operation::Search {
                query: get_string("query")?,
                limit: get_opt_i64("limit"),
                type_filter: get_opt_string("type_filter"),
            }),
            "graph.query" => Ok(Operation::GraphQuery {
                slug: get_string("slug")?,
                depth: get_opt_usize("depth"),
            }),
            "graph.backlinks" => Ok(Operation::GraphBacklinks {
                slug: get_string("slug")?,
            }),
            "sync" => Ok(Operation::Sync {
                dir: get_opt_string("dir"),
            }),
            "maintain" => Ok(Operation::Maintain {
                scope: get_opt_string("scope"),
            }),
            "stats" => Ok(Operation::Stats),
            "reindex" => Ok(Operation::Reindex),
            _ => Err(ErrorData::new(
                ErrorCode::INVALID_PARAMS,
                format!("unknown tool: {}", name),
                None,
            )),
        }
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
        let op = Self::parse_operation(&request.name, request.arguments)?;
        let result = self.registry.execute(op).await;
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

        assert_eq!(tools.len(), 11, "expected 11 tools, got {}", tools.len());

        let names: Vec<String> = tools.iter().map(|t| t.name.to_string()).collect();
        assert!(names.contains(&"page.get".to_string()));
        assert!(names.contains(&"page.put".to_string()));
        assert!(names.contains(&"page.delete".to_string()));
        assert!(names.contains(&"page.list".to_string()));
        assert!(names.contains(&"search".to_string()));
        assert!(names.contains(&"graph.query".to_string()));
        assert!(names.contains(&"graph.backlinks".to_string()));
        assert!(names.contains(&"sync".to_string()));
        assert!(names.contains(&"maintain".to_string()));
        assert!(names.contains(&"stats".to_string()));
        assert!(names.contains(&"reindex".to_string()));

        for tool in &tools {
            assert!(!tool.name.is_empty(), "tool name must not be empty");
            assert!(
                tool.description.as_ref().map_or(false, |d| !d.is_empty()),
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

        let op = SteleMcpServer::parse_operation("stats", None).unwrap();
        let result = registry.execute(op).await.unwrap();

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
        let result = SteleMcpServer::parse_operation("unknown.tool", None);
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_parse_operation_page_put() {
        let args = rmcp::object!({
            "slug": "test-page",
            "body": "# Hello",
            "timeline": { "content": "Created page" }
        });
        let op = SteleMcpServer::parse_operation("page.put", Some(args)).unwrap();
        match op {
            Operation::PagePut { slug, body, frontmatter_updates, timeline_append, etag } => {
                assert_eq!(slug, "test-page");
                assert_eq!(body, "# Hello");
                assert!(frontmatter_updates.is_none());
                assert_eq!(timeline_append.content, "Created page");
                assert_eq!(timeline_append.agent, None);
                assert_eq!(etag, None);
            }
            other => panic!("expected PagePut, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_parse_operation_page_put_full() {
        let args = rmcp::object!({
            "slug": "test-page",
            "body": "Body content",
            "frontmatter": { "title": "Test", "status": "Budding" },
            "timeline": { "content": "Updated", "agent": "claude" },
            "etag": "abc123"
        });
        let op = SteleMcpServer::parse_operation("page.put", Some(args)).unwrap();
        match op {
            Operation::PagePut { slug, body, frontmatter_updates, timeline_append, etag } => {
                assert_eq!(slug, "test-page");
                assert_eq!(body, "Body content");
                assert!(frontmatter_updates.is_some());
                let fm = frontmatter_updates.unwrap();
                assert_eq!(fm["title"].as_str().unwrap(), "Test");
                assert_eq!(timeline_append.content, "Updated");
                assert_eq!(timeline_append.agent, Some("claude".to_string()));
                assert_eq!(etag, Some("abc123".to_string()));
            }
            other => panic!("expected PagePut, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_parse_operation_page_put_missing_timeline() {
        let args = rmcp::object!({
            "slug": "test-page",
            "body": "# Hello"
        });
        let result = SteleMcpServer::parse_operation("page.put", Some(args));
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.message.contains("missing required field: timeline"));
    }

    #[tokio::test]
    async fn test_parse_operation_page_put_missing_timeline_content() {
        let args = rmcp::object!({
            "slug": "test-page",
            "body": "# Hello",
            "timeline": { "agent": "claude" }
        });
        let result = SteleMcpServer::parse_operation("page.put", Some(args));
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.message.contains("timeline.content"));
    }

    #[tokio::test]
    async fn test_parse_operation_page_delete() {
        let args = rmcp::object!({"slug": "test-page"});
        let op = SteleMcpServer::parse_operation("page.delete", Some(args)).unwrap();
        match op {
            Operation::PageDelete { slug } => assert_eq!(slug, "test-page"),
            other => panic!("expected PageDelete, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_parse_operation_page_list() {
        let args = rmcp::object!({"dir": "notes"});
        let op = SteleMcpServer::parse_operation("page.list", Some(args)).unwrap();
        match op {
            Operation::PageList { dir } => assert_eq!(dir, Some("notes".into())),
            other => panic!("expected PageList, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_parse_operation_search() {
        let args = rmcp::object!({
            "query": "rust",
            "limit": 10,
            "type_filter": "note"
        });
        let op = SteleMcpServer::parse_operation("search", Some(args)).unwrap();
        match op {
            Operation::Search {
                query,
                limit,
                type_filter,
            } => {
                assert_eq!(query, "rust");
                assert_eq!(limit, Some(10));
                assert_eq!(type_filter, Some("note".into()));
            }
            other => panic!("expected Search, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_parse_operation_graph_query() {
        let args = rmcp::object!({
            "slug": "foo",
            "depth": 2
        });
        let op = SteleMcpServer::parse_operation("graph.query", Some(args)).unwrap();
        match op {
            Operation::GraphQuery { slug, depth } => {
                assert_eq!(slug, "foo");
                assert_eq!(depth, Some(2));
            }
            other => panic!("expected GraphQuery, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_parse_operation_graph_backlinks() {
        let args = rmcp::object!({"slug": "foo"});
        let op = SteleMcpServer::parse_operation("graph.backlinks", Some(args)).unwrap();
        match op {
            Operation::GraphBacklinks { slug } => assert_eq!(slug, "foo"),
            other => panic!("expected GraphBacklinks, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_parse_operation_sync() {
        let args = rmcp::object!({"dir": "notes"});
        let op = SteleMcpServer::parse_operation("sync", Some(args)).unwrap();
        match op {
            Operation::Sync { dir } => assert_eq!(dir, Some("notes".into())),
            other => panic!("expected Sync, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_parse_operation_maintain() {
        let args = rmcp::object!({"scope": "lint"});
        let op = SteleMcpServer::parse_operation("maintain", Some(args)).unwrap();
        match op {
            Operation::Maintain { scope } => assert_eq!(scope, Some("lint".into())),
            other => panic!("expected Maintain, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_parse_operation_stats() {
        let op = SteleMcpServer::parse_operation("stats", None).unwrap();
        match op {
            Operation::Stats => {}
            other => panic!("expected Stats, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_parse_operation_reindex() {
        let op = SteleMcpServer::parse_operation("reindex", None).unwrap();
        match op {
            Operation::Reindex => {}
            other => panic!("expected Reindex, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_parse_operation_missing_required_field() {
        let args = rmcp::object!({});
        let result = SteleMcpServer::parse_operation("page.get", Some(args));
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.message.contains("missing required field: slug"));
    }

}
