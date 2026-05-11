use std::fmt;
use std::sync::Arc;

use serde_json::Value;

use crate::config::Config;
use crate::fns::FnsClient;
use crate::index::IndexEngine;
use crate::ops::handler::{OpExec, OpHandler, OperationContext};
use crate::types::Result;

/// Metadata describing an operation for tool listings and MCP schemas.
#[derive(Debug, Clone)]
pub struct OperationMeta {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}

/// Central registry that holds context and dispatches operations via inventory.
pub struct OperationRegistry {
    context: OperationContext,
}

impl fmt::Debug for OperationRegistry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OperationRegistry")
            .field("fns", &"<FnsClient>")
            .field("index", &"<IndexEngine>")
            .field("config", &self.context.config)
            .finish()
    }
}

impl OperationRegistry {
    pub fn new(fns: Arc<FnsClient>, index: Arc<IndexEngine>, config: Config) -> Self {
        Self {
            context: OperationContext { fns, index, config },
        }
    }

    pub fn config(&self) -> &Config {
        &self.context.config
    }

    /// List all registered operations from inventory, sorted by name.
    pub fn list_operations(&self) -> Vec<OperationMeta> {
        let mut ops: Vec<OperationMeta> = inventory::iter::<&'static dyn OpHandler>
            .into_iter()
            .map(|h| OperationMeta {
                name: h.name().to_string(),
                description: h.description().to_string(),
                input_schema: h.input_schema(),
            })
            .collect();
        ops.sort_by_key(|m| m.name.clone());
        ops
    }

    fn find_handler(&self, name: &str) -> Option<&'static dyn OpHandler> {
        inventory::iter::<&'static dyn OpHandler>
            .into_iter()
            .find(|h| h.name() == name)
            .copied()
    }

    /// Execute an operation by name with MCP JSON arguments.
    pub async fn execute_mcp(
        &self,
        name: &str,
        args: Option<serde_json::Map<String, Value>>,
    ) -> Result<Value> {
        let handler = self
            .find_handler(name)
            .ok_or_else(|| crate::types::Error::Config(format!("unknown tool: {}", name)))?;
        let op = handler
            .from_mcp_args(args)
            .map_err(|e| crate::types::Error::Config(e.to_string()))?;
        op.execute(&self.context)
            .await
            .map_err(|e| crate::types::Error::Config(e.to_string()))
    }

    /// Execute an operation from an already-constructed OpExec.
    pub async fn execute_op(&self, op: Box<dyn OpExec>) -> Result<Value> {
        op.execute(&self.context)
            .await
            .map_err(|e| crate::types::Error::Config(e.to_string()))
    }

    /// Get the operation context (for CLI use).
    pub fn context(&self) -> &OperationContext {
        &self.context
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::*;

    #[tokio::test]
    async fn test_all_ops_have_meta() {
        let reg = test_registry().await;
        let metas = reg.list_operations();
        assert_eq!(metas.len(), 11, "expected 11 operations, got {}", metas.len());

        for meta in &metas {
            assert!(!meta.name.is_empty(), "operation name must not be empty");
            assert!(
                !meta.description.is_empty(),
                "operation '{}' description must not be empty",
                meta.name
            );
            assert!(
                meta.input_schema.is_object(),
                "operation '{}' input_schema must be a JSON object",
                meta.name
            );
        }
    }

    #[tokio::test]
    async fn test_dispatch_routing() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;

        let sample_md = "---\ntitle: Test\npage_type: Entity\ntags: []\nsources: []\n---\nContent for test.\n";

        Mock::given(method("GET"))
            .and(path("/api/folder/notes"))
            .and(wiremock::matchers::query_param("vault", "test-vault"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 1,
                "status": true,
                "message": "Success",
                "data": {
                    "list": [{"path": "test.md"}],
                    "pager": {"totalRows": 1}
                }
            })))
            .mount(&server)
            .await;

        Mock::given(method("GET"))
            .and(path("/api/folders"))
            .and(wiremock::matchers::query_param("vault", "test-vault"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 1,
                "status": true,
                "message": "Success",
                "data": []
            })))
            .mount(&server)
            .await;

        Mock::given(method("GET"))
            .and(path("/api/note"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 1,
                "status": true,
                "message": "Success",
                "data": { "content": sample_md, "path": "test", "fileLinks": {}, "version": 1 }
            })))
            .mount(&server)
            .await;

        Mock::given(method("GET"))
            .and(path("/api/note"))
            .and(wiremock::matchers::query_param("path", "test"))
            .respond_with(ResponseTemplate::new(404).set_body_string("not found"))
            .mount(&server)
            .await;

        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 1,
                "status": true,
                "message": "Success",
                "data": null
            })))
            .mount(&server)
            .await;

        Mock::given(method("DELETE"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 1,
                "status": true,
                "message": "Success",
                "data": null
            })))
            .mount(&server)
            .await;

        let fns = Arc::new(FnsClient::new(
            server.uri(),
            "test-token".into(),
            "test-vault".into(),
        ));
        let index = Arc::new(
            IndexEngine::new("sqlite::memory:")
                .await
                .expect("in-memory index"),
        );
        let config = Config {
            server: crate::config::ServerConfig {
                host: "127.0.0.1".into(),
                port: 8080,
            },
            fns: crate::config::FnsConfig {
                base_url: server.uri(),
                token: "test-token".into(),
                vault: "test-vault".into(),
            },
            index: crate::config::IndexConfig {
                db_path: "sqlite::memory:".into(),
            },
        };
        let reg = OperationRegistry::new(fns, index, config);

        let args = serde_json::json!({"slug": "test"});
        let page_get = reg.execute_mcp("page.get", Some(serde_json::from_value(args).unwrap())).await;
        assert!(page_get.is_ok(), "page.get failed: {:?}", page_get.err());
        let val = page_get.unwrap();
        assert!(val.get("slug").is_some());
        assert!(val.get("body").is_some());
        assert!(val.get("frontmatter").is_some());
        assert!(val.get("timeline").is_some());

        let args = serde_json::json!({
            "slug": "test",
            "body": "Content for test.\n",
            "frontmatter": {
                "title": "Test",
                "page_type": "Entity",
                "tags": [],
                "related": [],
                "sources": [],
                "status": "Seedling"
            },
            "timeline": { "content": "Initial creation" }
        });
        let page_put = reg.execute_mcp("page.put", Some(serde_json::from_value(args).unwrap())).await;
        assert!(page_put.is_ok(), "page.put failed: {:?}", page_put.err());
        assert!(page_put.unwrap().get("indexed").is_some());

        let page_list = reg.execute_mcp("page.list", None).await;
        assert!(page_list.is_ok(), "page.list failed: {:?}", page_list.err());
        assert!(page_list.unwrap().get("files").is_some());

        let args = serde_json::json!({"query": "Content", "limit": 10});
        let search = reg.execute_mcp("search", Some(serde_json::from_value(args).unwrap())).await;
        assert!(search.is_ok(), "search failed: {:?}", search.err());
        assert!(search.unwrap().get("results").is_some());

        let args = serde_json::json!({"slug": "test", "depth": 1});
        let graph_q = reg.execute_mcp("graph.query", Some(serde_json::from_value(args).unwrap())).await;
        assert!(graph_q.is_ok(), "graph.query failed: {:?}", graph_q.err());

        let args = serde_json::json!({"slug": "test"});
        let graph_bl = reg.execute_mcp("graph.backlinks", Some(serde_json::from_value(args).unwrap())).await;
        assert!(graph_bl.is_ok(), "graph.backlinks failed: {:?}", graph_bl.err());

        let sync = reg.execute_mcp("sync", None).await;
        assert!(sync.is_ok(), "sync failed: {:?}", sync.err());

        let args = serde_json::json!({"scope": "full"});
        let maintain = reg.execute_mcp("maintain", Some(serde_json::from_value(args).unwrap())).await;
        assert!(maintain.is_ok(), "maintain failed: {:?}", maintain.err());
        assert!(maintain.unwrap().get("issues_count").is_some());

        let stats = reg.execute_mcp("stats", None).await;
        assert!(stats.is_ok(), "stats failed: {:?}", stats.err());
        assert!(stats.unwrap().get("total_pages").is_some());

        let reindex = reg.execute_mcp("reindex", None).await;
        assert!(reindex.is_ok(), "reindex failed: {:?}", reindex.err());
        assert_eq!(reindex.unwrap()["reindexed"].as_bool(), Some(true));

        let args = serde_json::json!({"slug": "test"});
        let page_del = reg.execute_mcp("page.delete", Some(serde_json::from_value(args).unwrap())).await;
        assert!(page_del.is_ok(), "page.delete failed: {:?}", page_del.err());
        assert_eq!(page_del.unwrap()["deleted"].as_bool(), Some(true));
    }

    #[tokio::test]
    async fn test_operation_names_unique() {
        let reg = test_registry().await;
        let metas = reg.list_operations();
        let mut names: Vec<&str> = metas.iter().map(|m| m.name.as_str()).collect();
        let original_len = names.len();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), original_len, "duplicate operation names found");
    }

    #[tokio::test]
    async fn test_schemas_valid_json() {
        let reg = test_registry().await;
        let metas = reg.list_operations();
        for meta in &metas {
            let serialized = serde_json::to_string(&meta.input_schema)
                .unwrap_or_else(|e| panic!("schema for '{}' failed to serialize: {}", meta.name, e));
            let _: serde_json::Value = serde_json::from_str(&serialized)
                .unwrap_or_else(|e| panic!("schema for '{}' failed to round-trip: {}", meta.name, e));
        }
    }
}
