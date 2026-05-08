use std::fmt;
use std::sync::Arc;

use serde_json::json;

use crate::config::Config;
use crate::fns::FnsClient;
use crate::index::IndexEngine;
use crate::ops::{maintain, page, search, sync};
use crate::types::Result;

/// A single operation that can be dispatched through the registry.
#[derive(Debug, Clone)]
pub enum Operation {
    PageGet { slug: String },
    PagePut { slug: String, content: String, etag: Option<String> },
    PageDelete { slug: String },
    PageList { dir: Option<String> },
    Append { slug: String, content: String },
    Search { query: String, limit: Option<i64>, type_filter: Option<String> },
    GraphQuery { slug: String, depth: Option<usize> },
    GraphBacklinks { slug: String },
    Sync { dir: Option<String> },
    Maintain { scope: Option<String> },
    Stats,
    Reindex,
}

/// Metadata describing an operation for tool listings and MCP schemas.
#[derive(Debug, Clone)]
pub struct OperationMeta {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}

/// Central registry that holds FNS, index, and config references,
/// and dispatches operations to their handlers.
pub struct OperationRegistry {
    fns: Arc<FnsClient>,
    index: Arc<IndexEngine>,
    config: Config,
}

impl fmt::Debug for OperationRegistry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OperationRegistry")
            .field("fns", &"<FnsClient>")
            .field("index", &"<IndexEngine>")
            .field("config", &self.config)
            .finish()
    }
}

impl OperationRegistry {
    /// Create a new registry with the given dependencies.
    pub fn new(fns: Arc<FnsClient>, index: Arc<IndexEngine>, config: Config) -> Self {
        Self { fns, index, config }
    }

    /// List all supported operations with their metadata and JSON schemas.
    pub fn list_operations(&self) -> Vec<OperationMeta> {
        vec![
            OperationMeta {
                name: "page.get".into(),
                description: "Retrieve a page by slug".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "slug": { "type": "string" }
                    },
                    "required": ["slug"]
                }),
            },
            OperationMeta {
                name: "page.put".into(),
                description: "Create or update a page".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "slug": { "type": "string" },
                        "content": { "type": "string" },
                        "etag": { "type": "string" }
                    },
                    "required": ["slug", "content"]
                }),
            },
            OperationMeta {
                name: "page.delete".into(),
                description: "Delete a page by slug".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "slug": { "type": "string" }
                    },
                    "required": ["slug"]
                }),
            },
            OperationMeta {
                name: "page.list".into(),
                description: "List pages, optionally filtered by directory".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "dir": { "type": "string" }
                    }
                }),
            },
            OperationMeta {
                name: "page.append".into(),
                description: "Append content to an existing page".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "slug": { "type": "string" },
                        "content": { "type": "string" }
                    },
                    "required": ["slug", "content"]
                }),
            },
            OperationMeta {
                name: "search".into(),
                description: "Full-text search across pages".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "query": { "type": "string" },
                        "limit": { "type": "integer" },
                        "type_filter": { "type": "string" }
                    },
                    "required": ["query"]
                }),
            },
            OperationMeta {
                name: "graph.query".into(),
                description: "Query the page graph to a given depth".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "slug": { "type": "string" },
                        "depth": { "type": "integer" }
                    },
                    "required": ["slug"]
                }),
            },
            OperationMeta {
                name: "graph.backlinks".into(),
                description: "Find all pages that link to a given slug".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "slug": { "type": "string" }
                    },
                    "required": ["slug"]
                }),
            },
            OperationMeta {
                name: "sync".into(),
                description: "Synchronize pages from FNS vault".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "dir": { "type": "string" }
                    }
                }),
            },
            OperationMeta {
                name: "maintain".into(),
                description: "Run maintenance tasks (lint, orphans, backlinks, full)".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "scope": {
                            "type": "string",
                            "enum": ["lint", "orphans", "backlinks", "full"]
                        }
                    }
                }),
            },
            OperationMeta {
                name: "stats".into(),
                description: "Get index statistics".into(),
                input_schema: json!({
                    "type": "object"
                }),
            },
            OperationMeta {
                name: "reindex".into(),
                description: "Rebuild the full-text search index".into(),
                input_schema: json!({
                    "type": "object"
                }),
            },
        ]
    }

    pub async fn execute(&self, op: Operation) -> Result<serde_json::Value> {
        match op {
            Operation::PageGet { slug } => {
                page::handle_page_get(&self.fns, &self.index, &slug).await
            }
            Operation::PagePut { slug, content, etag } => {
                page::handle_page_put(&self.fns, &self.index, &slug, &content, etag.as_deref()).await
            }
            Operation::PageDelete { slug } => {
                page::handle_page_delete(&self.fns, &self.index, &slug).await
            }
            Operation::PageList { dir } => {
                page::handle_page_list(&self.fns, dir.as_deref()).await
            }
            Operation::Append { slug, content } => {
                page::handle_page_append(&self.fns, &self.index, &slug, &content).await
            }
            Operation::Search { query, limit, type_filter } => {
                search::handle_search(&self.index, &query, limit, type_filter.as_deref()).await
            }
            Operation::GraphQuery { slug, depth } => {
                search::handle_graph_query(&self.index, &slug, depth).await
            }
            Operation::GraphBacklinks { slug } => {
                search::handle_graph_backlinks(&self.index, &slug).await
            }
            Operation::Sync { dir } => {
                sync::handle_sync(&self.fns, &self.index, dir.as_deref()).await
            }
            Operation::Maintain { scope } => {
                maintain::handle_maintain(&self.index, scope.as_deref()).await
            }
            Operation::Stats => {
                search::handle_stats(&self.index).await
            }
            Operation::Reindex => {
                maintain::handle_reindex(&self.fns, &self.index).await
            }
        }
    }
}

#[allow(dead_code)]
fn op_name(op: &Operation) -> &'static str {
    match op {
        Operation::PageGet { .. } => "page.get",
        Operation::PagePut { .. } => "page.put",
        Operation::PageDelete { .. } => "page.delete",
        Operation::PageList { .. } => "page.list",
        Operation::Append { .. } => "page.append",
        Operation::Search { .. } => "search",
        Operation::GraphQuery { .. } => "graph.query",
        Operation::GraphBacklinks { .. } => "graph.backlinks",
        Operation::Sync { .. } => "sync",
        Operation::Maintain { .. } => "maintain",
        Operation::Stats => "stats",
        Operation::Reindex => "reindex",
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
        assert_eq!(metas.len(), 12, "expected 12 operations, got {}", metas.len());

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

        let sample_md = "---\ntitle: Test\npage_type: Entity\ntags: []\nrelated: []\nsources: []\nstatus: Seedling\n---\nContent for test.\n";

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
            .and(path("/api/note"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 1,
                "status": true,
                "message": "Success",
                "data": { "content": sample_md, "path": "test", "fileLinks": {}, "version": 1 }
            })))
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

        let page_get = reg.execute(Operation::PageGet { slug: "test".into() }).await;
        assert!(page_get.is_ok(), "PageGet failed: {:?}", page_get.err());
        let val = page_get.unwrap();
        assert!(val.get("slug").is_some());
        assert!(val.get("content").is_some());

        let page_put = reg.execute(Operation::PagePut {
            slug: "test".into(),
            content: sample_md.to_string(),
            etag: None,
        }).await;
        assert!(page_put.is_ok(), "PagePut failed: {:?}", page_put.err());
        assert!(page_put.unwrap().get("indexed").is_some());

        let page_list = reg.execute(Operation::PageList { dir: None }).await;
        assert!(page_list.is_ok(), "PageList failed: {:?}", page_list.err());
        assert!(page_list.unwrap().get("files").is_some());

        let search = reg.execute(Operation::Search {
            query: "Content".into(),
            limit: Some(10),
            type_filter: None,
        }).await;
        assert!(search.is_ok(), "Search failed: {:?}", search.err());
        assert!(search.unwrap().get("results").is_some());

        let graph_q = reg.execute(Operation::GraphQuery {
            slug: "test".into(),
            depth: Some(1),
        }).await;
        assert!(graph_q.is_ok(), "GraphQuery failed: {:?}", graph_q.err());

        let graph_bl = reg.execute(Operation::GraphBacklinks {
            slug: "test".into(),
        }).await;
        assert!(graph_bl.is_ok(), "GraphBacklinks failed: {:?}", graph_bl.err());

        let sync = reg.execute(Operation::Sync { dir: None }).await;
        assert!(sync.is_ok(), "Sync failed: {:?}", sync.err());

        let maintain = reg.execute(Operation::Maintain { scope: Some("full".into()) }).await;
        assert!(maintain.is_ok(), "Maintain failed: {:?}", maintain.err());
        assert!(maintain.unwrap().get("issues_count").is_some());

        let stats = reg.execute(Operation::Stats).await;
        assert!(stats.is_ok(), "Stats failed: {:?}", stats.err());
        assert!(stats.unwrap().get("total_pages").is_some());

        let reindex = reg.execute(Operation::Reindex).await;
        assert!(reindex.is_ok(), "Reindex failed: {:?}", reindex.err());
        assert_eq!(reindex.unwrap()["reindexed"].as_bool(), Some(true));

        let page_del = reg.execute(Operation::PageDelete { slug: "test".into() }).await;
        assert!(page_del.is_ok(), "PageDelete failed: {:?}", page_del.err());
        assert_eq!(page_del.unwrap()["deleted"].as_bool(), Some(true));

        let page_append = reg.execute(Operation::Append {
            slug: "test".into(),
            content: "\nAppended text.".into(),
        }).await;
        assert!(page_append.is_ok(), "Append failed: {:?}", page_append.err());
        let val = page_append.unwrap();
        assert_eq!(val["slug"].as_str(), Some("test"));
        assert_eq!(val["appended"].as_bool(), Some(true));
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

    #[test]
    fn test_op_name_helper() {
        assert_eq!(op_name(&Operation::PageGet { slug: "x".into() }), "page.get");
        assert_eq!(op_name(&Operation::PagePut { slug: "x".into(), content: "y".into(), etag: None }), "page.put");
        assert_eq!(op_name(&Operation::PageDelete { slug: "x".into() }), "page.delete");
        assert_eq!(op_name(&Operation::PageList { dir: None }), "page.list");
        assert_eq!(op_name(&Operation::Append { slug: "x".into(), content: "y".into() }), "page.append");
        assert_eq!(op_name(&Operation::Search { query: "q".into(), limit: None, type_filter: None }), "search");
        assert_eq!(op_name(&Operation::GraphQuery { slug: "x".into(), depth: None }), "graph.query");
        assert_eq!(op_name(&Operation::GraphBacklinks { slug: "x".into() }), "graph.backlinks");
        assert_eq!(op_name(&Operation::Sync { dir: None }), "sync");
        assert_eq!(op_name(&Operation::Maintain { scope: None }), "maintain");
        assert_eq!(op_name(&Operation::Stats), "stats");
        assert_eq!(op_name(&Operation::Reindex), "reindex");
    }
}
