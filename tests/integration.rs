use std::sync::Arc;

use chrono::NaiveDate;
use serde_json::{Value, json};

use wikiops::mcp::protocol::{JsonRpcError, JsonRpcRequest};
use wikiops::mcp::resources::ResourceRegistry;
use wikiops::mcp::server::McpServer;
use wikiops::models::{Frontmatter, Link, Page, PageType};
use wikiops::services::tools::ToolRegistry;
use wikiops::storage::{BackendError, FileBackend, FileMeta, FileStat};
use wikiops::storage::sqlite::SqliteBackend;

struct NoopBackend;

#[async_trait::async_trait]
impl FileBackend for NoopBackend {
    async fn get(&self, _: &str) -> Result<String, BackendError> {
        Err(BackendError::NotFound("noop".into()))
    }
    async fn put(&self, _: &str, _: &str) -> Result<(), BackendError> {
        Ok(())
    }
    async fn append(&self, _: &str, _: &str) -> Result<(), BackendError> {
        Ok(())
    }
    async fn delete(&self, _: &str) -> Result<(), BackendError> {
        Ok(())
    }
    async fn list(&self, _: &str) -> Result<Vec<FileMeta>, BackendError> {
        Ok(vec![])
    }
    async fn exists(&self, _: &str) -> Result<bool, BackendError> {
        Ok(false)
    }
    async fn stat(&self, _: &str) -> Result<FileStat, BackendError> {
        Err(BackendError::NotFound("noop".into()))
    }
}

async fn in_memory_db() -> SqliteBackend {
    SqliteBackend::new(":memory:")
        .await
        .expect("creating in-memory SQLite backend")
}

async fn test_mcp_server() -> McpServer {
    let db = Arc::new(in_memory_db().await);
    let tool_registry = Arc::new(ToolRegistry::new(db));
    let file_backend = Arc::new(NoopBackend);
    let resource_registry = Arc::new(ResourceRegistry::new(file_backend.clone()));
    McpServer::new(tool_registry, resource_registry, file_backend)
}

fn make_request(method: &str, id: Option<Value>, params: Option<Value>) -> JsonRpcRequest {
    JsonRpcRequest {
        jsonrpc: "2.0".into(),
        id,
        method: method.into(),
        params,
    }
}

fn sample_page(slug: &str, title: &str, page_type: PageType, compiled_truth: &str) -> Page {
    Page {
        slug: slug.to_string(),
        vault: "forge".to_string(),
        frontmatter: Frontmatter {
            r#type: page_type,
            title: title.to_string(),
            tags: vec!["test".to_string()],
            related: vec![],
            sources: vec![],
            date: NaiveDate::from_ymd_opt(2026, 5, 5).unwrap(),
            status: None,
        },
        compiled_truth: compiled_truth.to_string(),
        timeline: vec![],
        content_hash: format!("sha256:{}", slug),
        raw_content: format!("# {}", title),
    }
}

#[tokio::test]
async fn test_mcp_initialize_handshake() {
    let server = test_mcp_server().await;

    let req = make_request(
        "initialize",
        Some(json!(1)),
        Some(json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": { "name": "integration-test", "version": "0.1" }
        })),
    );

    let resp = server.handle(req).await.unwrap();

    assert_eq!(resp.jsonrpc, "2.0");
    assert_eq!(resp.id, Some(json!(1)));
    assert!(resp.error.is_none(), "unexpected error: {:?}", resp.error);

    let result = resp.result.unwrap();
    assert_eq!(result["protocolVersion"], "2024-11-05");
    assert_eq!(result["serverInfo"]["name"], "wikiops");
    assert_eq!(result["serverInfo"]["version"], "0.1.0");
    assert!(result["capabilities"]["tools"].is_object());
    assert!(result["capabilities"]["resources"].is_object());
    assert!(result["capabilities"]["prompts"].is_object());
}

#[tokio::test]
async fn test_tools_list_returns_all_tools() {
    let server = test_mcp_server().await;

    let req = make_request("tools/list", Some(json!(2)), None);
    let resp = server.handle(req).await.unwrap();

    assert!(resp.error.is_none(), "unexpected error: {:?}", resp.error);
    let result = resp.result.unwrap();
    let tools = result["tools"].as_array().unwrap();

    let tool_names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();

    assert!(tool_names.contains(&"brain_search"), "missing brain_search");
    assert!(tool_names.contains(&"brain_stats"), "missing brain_stats");
    assert!(tool_names.contains(&"brain_maintain"), "missing brain_maintain");
    assert!(tool_names.contains(&"brain_append"), "missing brain_append");
    assert!(tool_names.contains(&"brain_list"), "missing brain_list");
    assert!(tool_names.contains(&"brain_sync"), "missing brain_sync");
    assert!(tool_names.contains(&"brain_query"), "missing brain_query");

    for tool in tools {
        assert!(tool["name"].is_string());
        assert!(tool["description"].is_string());
        assert!(tool["inputSchema"].is_object());
    }
}

#[tokio::test]
async fn test_brain_put_indexes_page_in_sqlite() {
    let db = in_memory_db().await;

    let page = sample_page(
        "wiki/entities/rust",
        "Rust Programming Language",
        PageType::Entity,
        "Rust is a systems programming language focused on safety and performance.",
    );

    db.index_page(&page).await.unwrap();

    let row = db.get_page("wiki/entities/rust").await.unwrap();
    assert!(row.is_some(), "page should be indexed");

    let row = row.unwrap();
    assert_eq!(row.slug, "wiki/entities/rust");
    assert_eq!(row.title, "Rust Programming Language");
    assert_eq!(row.page_type, "entity");
    assert_eq!(row.vault, "forge");
    assert_eq!(
        row.compiled_truth.as_deref(),
        Some("Rust is a systems programming language focused on safety and performance.")
    );
}

#[tokio::test]
async fn test_brain_get_retrieves_indexed_page() {
    let db = in_memory_db().await;

    let page = sample_page(
        "wiki/concepts/memory-safety",
        "Memory Safety",
        PageType::Concept,
        "Memory safety ensures programs do not access invalid memory.",
    );

    db.index_page(&page).await.unwrap();

    let row = db.get_page("wiki/concepts/memory-safety").await.unwrap();
    assert!(row.is_some());

    let row = row.unwrap();
    assert_eq!(row.slug, "wiki/concepts/memory-safety");
    assert_eq!(row.title, "Memory Safety");
    assert_eq!(row.page_type, "concept");
    assert!(row.created_at.len() > 0);
    assert!(row.updated_at.len() > 0);
}

#[tokio::test]
async fn test_brain_get_nonexistent_returns_none() {
    let db = in_memory_db().await;

    let row = db.get_page("wiki/does-not-exist").await.unwrap();
    assert!(row.is_none());
}

#[tokio::test]
async fn test_brain_put_then_get_roundtrip() {
    let db = in_memory_db().await;

    let pages = vec![
        sample_page("wiki/a", "Alpha", PageType::Entity, "Alpha content"),
        sample_page("wiki/b", "Beta", PageType::Concept, "Beta content"),
        sample_page("wiki/c", "Gamma", PageType::Source, "Gamma content"),
    ];

    for page in &pages {
        db.index_page(page).await.unwrap();
    }

    for page in &pages {
        let row = db.get_page(&page.slug).await.unwrap();
        assert!(row.is_some(), "page {} should exist", page.slug);
        assert_eq!(row.unwrap().title, page.frontmatter.title);
    }
}

#[tokio::test]
async fn test_brain_search_finds_indexed_content() {
    let db = in_memory_db().await;

    db.index_page(&sample_page(
        "wiki/quantum",
        "Quantum Computing",
        PageType::Entity,
        "Quantum computing uses qubits for computation.",
    ))
    .await
    .unwrap();

    db.index_page(&sample_page(
        "wiki/classical",
        "Classical Computing",
        PageType::Entity,
        "Classical computing uses binary bits.",
    ))
    .await
    .unwrap();

    db.index_page(&sample_page(
        "wiki/ai",
        "Artificial Intelligence",
        PageType::Concept,
        "AI involves machine learning and neural networks.",
    ))
    .await
    .unwrap();

    let hits = db.search_keyword("quantum", 10, None).await.unwrap();
    assert!(!hits.is_empty(), "should find quantum page");
    assert!(hits.iter().any(|h| h.slug == "wiki/quantum"));

    let hits = db.search_keyword("computing", 10, None).await.unwrap();
    assert!(hits.len() >= 2, "should find both computing pages");

    let hits = db
        .search_keyword("computing", 10, Some("entity"))
        .await
        .unwrap();
    assert!(!hits.is_empty());

    let hits = db.search_keyword("zzzznonexistent", 10, None).await.unwrap();
    assert!(hits.is_empty());
}

#[tokio::test]
async fn test_brain_search_respects_limit() {
    let db = in_memory_db().await;

    for i in 0..10 {
        db.index_page(&sample_page(
            &format!("wiki/page{}", i),
            &format!("Search Result {}", i),
            PageType::Entity,
            "Common searchable content for limit test.",
        ))
        .await
        .unwrap();
    }

    let hits = db.search_keyword("searchable", 3, None).await.unwrap();
    assert!(hits.len() <= 3, "should respect limit");
}

#[tokio::test]
async fn test_brain_stats_empty_database() {
    let server = test_mcp_server().await;

    let req = make_request(
        "tools/call",
        Some(json!(10)),
        Some(json!({
            "name": "brain_stats",
            "arguments": {}
        })),
    );

    let resp = server.handle(req).await.unwrap();
    assert!(resp.error.is_none(), "unexpected error: {:?}", resp.error);

    let result = resp.result.unwrap();
    let content = result["content"].as_array().unwrap();
    assert_eq!(content[0]["type"], "text");

    let text: Value = serde_json::from_str(content[0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(text["total_pages"], 0);
    assert_eq!(text["total_links"], 0);
    assert_eq!(text["orphan_pages"], 0);
    assert!(text["db_size_mb"].is_number());
}

#[tokio::test]
async fn test_brain_stats_with_data() {
    let db = in_memory_db().await;

    db.index_page(&sample_page(
        "wiki/e1",
        "Entity 1",
        PageType::Entity,
        "Content one",
    ))
    .await
    .unwrap();

    db.index_page(&sample_page(
        "wiki/e2",
        "Entity 2",
        PageType::Entity,
        "Content two",
    ))
    .await
    .unwrap();

    db.index_page(&sample_page(
        "wiki/c1",
        "Concept 1",
        PageType::Concept,
        "Concept content",
    ))
    .await
    .unwrap();

    db.update_links(
        "wiki/e1",
        &[Link {
            source_slug: "wiki/e1".to_string(),
            target_slug: "wiki/c1".to_string(),
            link_type: "link".to_string(),
            context_snippet: None,
        }],
    )
    .await
    .unwrap();

    let stats = db.get_stats().await.unwrap();

    assert_eq!(stats.total_pages, 3);
    assert_eq!(stats.total_links, 1);
    assert!(stats.last_sync.is_some());
    assert_eq!(stats.by_type.get("entity"), Some(&2));
    assert_eq!(stats.by_type.get("concept"), Some(&1));
}

#[tokio::test]
async fn test_brain_maintain_full_scope() {
    let db = in_memory_db().await;

    db.index_page(&sample_page(
        "wiki/good.md",
        "Good Page",
        PageType::Entity,
        "Well-formed page.",
    ))
    .await
    .unwrap();

    db.index_page(&sample_page(
        "wiki/BadName.md",
        "Bad Name",
        PageType::Entity,
        "Has uppercase in slug.",
    ))
    .await
    .unwrap();

    db.update_links(
        "wiki/good.md",
        &[Link {
            source_slug: "wiki/good.md".to_string(),
            target_slug: "wiki/missing.md".to_string(),
            link_type: "link".to_string(),
            context_snippet: None,
        }],
    )
    .await
    .unwrap();

    let tool_registry = ToolRegistry::new(Arc::new(db));

    let result = tool_registry
        .call("brain_maintain", json!({ "scope": "full" }))
        .await;

    assert!(
        result.is_ok(),
        "brain_maintain should succeed: {:?}",
        result.err()
    );

    let value = result.unwrap();
    let text = value.to_string();
    assert!(text.contains("issues_found") || text.contains("scope"));
}

#[tokio::test]
async fn test_brain_maintain_orphans_scope() {
    let db = in_memory_db().await;

    db.index_page(&sample_page(
        "wiki/exists.md",
        "Exists",
        PageType::Entity,
        "Content.",
    ))
    .await
    .unwrap();

    db.update_links(
        "wiki/exists.md",
        &[Link {
            source_slug: "wiki/exists.md".to_string(),
            target_slug: "wiki/ghost.md".to_string(),
            link_type: "link".to_string(),
            context_snippet: None,
        }],
    )
    .await
    .unwrap();

    let tool_registry = ToolRegistry::new(Arc::new(db));

    let result = tool_registry
        .call("brain_maintain", json!({ "scope": "orphans" }))
        .await
        .unwrap();

    let text = result.to_string();
    assert!(text.contains("orphan") || text.contains("issues_found"));
}

#[tokio::test]
async fn test_mcp_notification_returns_none() {
    let server = test_mcp_server().await;

    let req = make_request("notifications/initialized", None, None);
    let resp = server.handle(req).await;
    assert!(resp.is_none(), "notifications must not produce a response");
}

#[tokio::test]
async fn test_mcp_unknown_method_returns_error() {
    let server = test_mcp_server().await;

    let req = make_request("nonexistent/method", Some(json!(99)), None);
    let resp = server.handle(req).await.unwrap();

    assert!(resp.result.is_none());
    let err = resp.error.unwrap();
    assert_eq!(err.code, JsonRpcError::METHOD_NOT_FOUND);
}

#[tokio::test]
async fn test_mcp_tools_call_missing_name_returns_error() {
    let server = test_mcp_server().await;

    let req = make_request("tools/call", Some(json!(10)), Some(json!({})));
    let resp = server.handle(req).await.unwrap();

    let err = resp.error.unwrap();
    assert_eq!(err.code, JsonRpcError::INVALID_PARAMS);
}

#[tokio::test]
async fn test_mcp_ping_returns_empty_object() {
    let server = test_mcp_server().await;

    let req = make_request("ping", Some(json!(5)), None);
    let resp = server.handle(req).await.unwrap();

    assert_eq!(resp.result, Some(json!({})));
    assert!(resp.error.is_none());
}

#[tokio::test]
async fn test_link_graph_roundtrip() {
    let db = in_memory_db().await;

    db.index_page(&sample_page("wiki/a", "Page A", PageType::Entity, "A"))
        .await
        .unwrap();
    db.index_page(&sample_page("wiki/b", "Page B", PageType::Entity, "B"))
        .await
        .unwrap();
    db.index_page(&sample_page("wiki/c", "Page C", PageType::Entity, "C"))
        .await
        .unwrap();

    db.update_links(
        "wiki/a",
        &[
            Link {
                source_slug: "wiki/a".to_string(),
                target_slug: "wiki/b".to_string(),
                link_type: "link".to_string(),
                context_snippet: Some("see B".to_string()),
            },
            Link {
                source_slug: "wiki/a".to_string(),
                target_slug: "wiki/c".to_string(),
                link_type: "works_at".to_string(),
                context_snippet: None,
            },
        ],
    )
    .await
    .unwrap();

    let bl_b = db.get_backlinks("wiki/b").await.unwrap();
    assert_eq!(bl_b.len(), 1);
    assert_eq!(bl_b[0].source_slug, "wiki/a");

    let bl_c = db.get_backlinks("wiki/c").await.unwrap();
    assert_eq!(bl_c.len(), 1);
    assert_eq!(bl_c[0].link_type, "works_at");

    let outgoing = db.get_outgoing_link_targets("wiki/a").await.unwrap();
    assert_eq!(outgoing.len(), 2);
    assert!(outgoing.contains(&"wiki/b".to_string()));
    assert!(outgoing.contains(&"wiki/c".to_string()));

    assert!(db.has_direct_link("wiki/a", "wiki/b").await.unwrap());
    assert!(!db.has_direct_link("wiki/b", "wiki/a").await.unwrap());

    db.update_links(
        "wiki/a",
        &[Link {
            source_slug: "wiki/a".to_string(),
            target_slug: "wiki/b".to_string(),
            link_type: "link".to_string(),
            context_snippet: None,
        }],
    )
    .await
    .unwrap();

    let bl_c = db.get_backlinks("wiki/c").await.unwrap();
    assert!(
        bl_c.is_empty(),
        "C should have no backlinks after replacement"
    );
}

#[tokio::test]
async fn test_page_upsert_updates_existing() {
    let db = in_memory_db().await;

    let mut page = sample_page("wiki/upsert", "Original", PageType::Entity, "Original content");
    db.index_page(&page).await.unwrap();

    page.frontmatter.title = "Updated Title".to_string();
    page.content_hash = "new-hash".to_string();
    page.compiled_truth = "Updated content.".to_string();
    db.index_page(&page).await.unwrap();

    let row = db.get_page("wiki/upsert").await.unwrap().unwrap();
    assert_eq!(row.title, "Updated Title");
    assert_eq!(row.content_hash, "new-hash");
    assert_eq!(row.compiled_truth.as_deref(), Some("Updated content."));
}

#[tokio::test]
async fn test_page_remove_cleans_links() {
    let db = in_memory_db().await;

    db.index_page(&sample_page("wiki/src", "Source", PageType::Entity, "S"))
        .await
        .unwrap();
    db.index_page(&sample_page("wiki/tgt", "Target", PageType::Entity, "T"))
        .await
        .unwrap();

    db.update_links(
        "wiki/src",
        &[Link {
            source_slug: "wiki/src".to_string(),
            target_slug: "wiki/tgt".to_string(),
            link_type: "link".to_string(),
            context_snippet: None,
        }],
    )
    .await
    .unwrap();

    let removed = db.remove_page("wiki/src").await.unwrap();
    assert!(removed);

    assert!(db.get_page("wiki/src").await.unwrap().is_none());

    let backlinks = db.get_backlinks("wiki/tgt").await.unwrap();
    assert!(backlinks.is_empty());
}

#[tokio::test]
async fn test_brain_search_via_mcp_tools_call() {
    let db = Arc::new(in_memory_db().await);

    db.index_page(&sample_page(
        "wiki/rust-lang",
        "Rust Language",
        PageType::Entity,
        "Rust is a modern systems programming language.",
    ))
    .await
    .unwrap();

    let tool_registry = Arc::new(ToolRegistry::new(db));
    let file_backend = Arc::new(NoopBackend);
    let resource_registry = Arc::new(ResourceRegistry::new(file_backend.clone()));
    let server = McpServer::new(tool_registry, resource_registry, file_backend);

    let req = make_request(
        "tools/call",
        Some(json!(20)),
        Some(json!({
            "name": "brain_search",
            "arguments": {
                "query": "rust",
                "limit": 10
            }
        })),
    );

    let resp = server.handle(req).await.unwrap();
    assert!(
        resp.error.is_none(),
        "brain_search should succeed: {:?}",
        resp.error
    );

    let result = resp.result.unwrap();
    let content = result["content"].as_array().unwrap();
    let text: Value = serde_json::from_str(content[0]["text"].as_str().unwrap()).unwrap();

    assert_eq!(text["query"], "rust");
    assert!(text["total"].as_u64().unwrap() >= 1);

    let results = text["results"].as_array().unwrap();
    assert!(results.iter().any(|r| r["slug"] == "wiki/rust-lang"));
}
