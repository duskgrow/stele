use std::sync::Arc;

use serde_json::json;
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

use stele::config::{Config, FnsConfig, IndexConfig, ServerConfig};
use stele::fns::FnsClient;
use stele::index::IndexEngine;
use stele::ops::OperationRegistry;

fn test_config(fns_url: &str) -> Config {
    Config {
        server: ServerConfig {
            host: "127.0.0.1".into(),
            port: 8080,
        },
        fns: FnsConfig {
            base_url: fns_url.to_string(),
            token: "test-token".into(),
            vault: "test-vault".into(),
        },
        index: IndexConfig {
            db_path: "sqlite::memory:".into(),
        },
    }
}

async fn test_index() -> IndexEngine {
    IndexEngine::new("sqlite::memory:")
        .await
        .expect("in-memory index")
}

async fn test_registry(fns_url: &str) -> OperationRegistry {
    let fns = Arc::new(FnsClient::new(
        fns_url.to_string(),
        "test-token".into(),
        "test-vault".into(),
    ));
    let index = Arc::new(test_index().await);
    let config = test_config(fns_url);
    OperationRegistry::new(fns, index, config)
}

fn sample_markdown(title: &str, body: &str) -> String {
    format!(
        "---\ntitle: {}\npage_type: Entity\ntags:\n  - test\nsources: []\n---\n{}\n",
        title, body
    )
}

fn sample_markdown_with_link(target: &str) -> String {
    sample_markdown(
        "Link Page",
        &format!("This references [[{}]].", target),
    )
}

fn fns_string_response(data: &str) -> serde_json::Value {
    json!({"code": 1, "status": true, "message": "Success", "data": {"content": data, "path": "", "fileLinks": {}, "version": 1}})
}

fn fns_success_response() -> serde_json::Value {
    json!({"code": 1, "status": true, "message": "Success", "data": null})
}

async fn setup_note_get_mock(server: &MockServer, slug: &str, content: &str) {
    let fns_path = if slug.ends_with(".md") {
        slug.to_string()
    } else {
        format!("{slug}.md")
    };
    Mock::given(method("GET"))
        .and(path("/api/note"))
        .and(query_param("vault", "test-vault"))
        .and(query_param("path", fns_path))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(fns_string_response(content)),
        )
        .mount(server)
        .await;
}

fn sample_frontmatter(title: &str) -> serde_json::Value {
    serde_json::json!({
        "title": title,
        "page_type": "Entity",
        "tags": ["test"],
        
        "sources": [],
        "visibility": "shared"
    })
}

async fn setup_note_put_mock(server: &MockServer, _slug: &str) {
    Mock::given(method("GET"))
        .and(path("/api/note"))
        .respond_with(ResponseTemplate::new(404).set_body_string("not found"))
        .up_to_n_times(1)
        .mount(server)
        .await;

    Mock::given(method("POST"))
        .and(path("/api/note"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(fns_success_response()),
        )
        .mount(server)
        .await;
}

async fn setup_note_delete_mock(server: &MockServer, _slug: &str) {
    Mock::given(method("DELETE"))
        .and(path("/api/note"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(fns_success_response()),
        )
        .mount(server)
        .await;
}

async fn setup_list_mock(server: &MockServer, files: &[&str]) {
    let list_items: Vec<serde_json::Value> = files
        .iter()
        .map(|f| json!({"path": f}))
        .collect();
    let total = files.len();
    let response_data = json!({
        "list": list_items,
        "pager": { "totalRows": total }
    });
    Mock::given(method("GET"))
        .and(path("/api/folder/notes"))
        .and(query_param("vault", "test-vault"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({"code": 1, "status": true, "message": "Success", "data": response_data})),
        )
        .mount(server)
        .await;
}

async fn setup_folders_mock(server: &MockServer, folders: &[&str]) {
    let folder_items: Vec<serde_json::Value> = folders
        .iter()
        .map(|f| json!({"path": f}))
        .collect();
    Mock::given(method("GET"))
        .and(path("/api/folders"))
        .and(query_param("vault", "test-vault"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({"code": 1, "status": true, "message": "Success", "data": folder_items})),
        )
        .mount(server)
        .await;
}

async fn test_registry_with_index(fns_url: &str) -> (OperationRegistry, Arc<IndexEngine>) {
    let fns = Arc::new(FnsClient::new(
        fns_url.to_string(),
        "test-token".into(),
        "test-vault".into(),
    ));
    let index = Arc::new(test_index().await);
    let config = test_config(fns_url);
    let reg = OperationRegistry::new(fns, index.clone(), config);
    (reg, index)
}

fn markdown_with_timeline(title: &str, body: &str, entries: &[(&str, &str)]) -> String {
    let timeline = entries
        .iter()
        .map(|(date, content)| format!("- {date}: {content}"))
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "---\ntitle: {}\npage_type: Entity\ntags:\n  - test\nsources: []\n---\n{}\n---\n{}\n",
        title, body, timeline
    )
}

#[tokio::test]
async fn test_tools_list_returns_all_operations() {
    let server = MockServer::start().await;
    let reg = test_registry(&server.uri()).await;

    let ops = reg.list_operations();
    assert_eq!(ops.len(), 11);

    let names: Vec<&str> = ops.iter().map(|o| o.name.as_str()).collect();
    assert!(names.contains(&"page.get"));
    assert!(names.contains(&"page.put"));
    assert!(names.contains(&"page.delete"));
    assert!(names.contains(&"page.list"));
    assert!(names.contains(&"search"));
    assert!(names.contains(&"graph.query"));
    assert!(names.contains(&"graph.backlinks"));
    assert!(names.contains(&"sync"));
    assert!(names.contains(&"maintain"));
    assert!(names.contains(&"stats"));
    assert!(names.contains(&"reindex"));

    for op in &ops {
        assert!(!op.name.is_empty());
        assert!(!op.description.is_empty());
        assert!(op.input_schema.is_object());
    }
}

#[tokio::test]
async fn test_page_put_then_get_roundtrip() {
    let server = MockServer::start().await;
    let reg = test_registry(&server.uri()).await;

    let content = sample_markdown("Test Page", "Compiled truth content.");

    setup_note_put_mock(&server, "test-page").await;
    setup_note_get_mock(&server, "test-page", &content).await;

    let put_result = reg
        .execute_mcp("page.put", Some(json!({"slug": "test-page", "body": "Compiled truth content.\n", "frontmatter": sample_frontmatter("Test Page"), "timeline": {"content": "Created"}}).as_object().cloned().unwrap()))
        .await
        .expect("put should succeed");
    assert_eq!(put_result["slug"], "test-page");
    assert_eq!(put_result["indexed"], true);

    let get_result = reg
        .execute_mcp("page.get", Some(json!({"slug": "test-page"}).as_object().cloned().unwrap()))
        .await
        .expect("get should succeed");
    assert_eq!(get_result["slug"], "test-page");
    assert!(get_result["body"].is_string());
    assert!(get_result["frontmatter"].is_object());
    assert!(get_result["timeline"].is_array());
    assert!(get_result["content_hash"].is_string());
}

#[tokio::test]
async fn test_page_put_auto_indexes() {
    let server = MockServer::start().await;
    let reg = test_registry(&server.uri()).await;

    let _content = sample_markdown("Indexed Page", "Searchable content about rust.");

    setup_note_put_mock(&server, "indexed-page").await;

    reg.execute_mcp("page.put", Some(json!({"slug": "indexed-page", "body": "Searchable content about rust.\n", "frontmatter": sample_frontmatter("Indexed Page"), "timeline": {"content": "Created"}}).as_object().cloned().unwrap()))
    .await
    .expect("put should succeed");

    let search_result = reg
        .execute_mcp("search", Some(json!({"query": "rust", "limit": 10}).as_object().cloned().unwrap()))
        .await
        .expect("search should succeed");
    assert!(search_result["total"].as_u64().unwrap() >= 1);
    let results = search_result["results"].as_array().unwrap();
    assert!(results.iter().any(|r| r["slug"] == "indexed-page"));
}

#[tokio::test]
async fn test_page_put_extracts_wikilinks() {
    let server = MockServer::start().await;
    let reg = test_registry(&server.uri()).await;

    let _content = sample_markdown_with_link("target-page");

    setup_note_put_mock(&server, "source-page").await;

    let put_result = reg
        .execute_mcp("page.put", Some(json!({"slug": "source-page", "body": "This references [[target-page]].\n", "frontmatter": sample_frontmatter("Link Page"), "timeline": {"content": "Created"}}).as_object().cloned().unwrap()))
        .await
        .expect("put should succeed");
    assert!(put_result["links_count"].as_u64().unwrap() >= 1);

    let backlinks = reg
        .execute_mcp("graph.backlinks", Some(json!({"slug": "target-page"}).as_object().cloned().unwrap()))
        .await
        .expect("backlinks should succeed");
    assert!(backlinks["count"].as_u64().unwrap() >= 1);
    let sources: Vec<&str> = backlinks["backlinks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|b| b["source_slug"].as_str().unwrap())
        .collect();
    assert!(sources.contains(&"source-page"));
}

#[tokio::test]
async fn test_page_delete_removes_from_index() {
    let server = MockServer::start().await;
    let reg = test_registry(&server.uri()).await;

    setup_note_put_mock(&server, "delete-me").await;
    setup_note_delete_mock(&server, "delete-me").await;

    reg.execute_mcp("page.put", Some(json!({"slug": "delete-me", "body": "Temporary content.\n", "frontmatter": sample_frontmatter("Delete Me"), "timeline": {"content": "Created"}}).as_object().cloned().unwrap()))
    .await
    .expect("put should succeed");

    let stats_before = reg.execute_mcp("stats", None).await.unwrap();
    assert!(stats_before["total_pages"].as_i64().unwrap() >= 1);

    let del_result = reg
        .execute_mcp("page.delete", Some(json!({"slug": "delete-me"}).as_object().cloned().unwrap()))
        .await
        .expect("delete should succeed");
    assert_eq!(del_result["deleted"], true);

    let stats_after = reg.execute_mcp("stats", None).await.unwrap();
    assert_eq!(stats_after["total_pages"].as_i64().unwrap(), 0);
}

#[tokio::test]
async fn test_page_list_returns_files() {
    let server = MockServer::start().await;
    let reg = test_registry(&server.uri()).await;

    setup_list_mock(&server, &["alpha.md", "beta.md", "gamma.md"]).await;
    setup_folders_mock(&server, &[]).await;

    let result = reg
        .execute_mcp("page.list", None)
        .await
        .expect("list should succeed");

    let files = result["files"].as_array().unwrap();
    assert_eq!(files.len(), 3);
    assert_eq!(result["count"], 3);
}

#[tokio::test]
async fn test_search_finds_indexed_page() {
    let server = MockServer::start().await;
    let index = test_index().await;

    let md_a = sample_markdown("Quantum Physics", "Quantum mechanics is fascinating.");
    let page_a = stele::parser::page::parse_page(&md_a, "quantum").unwrap();
    index.index_page(&page_a).await.unwrap();

    let md_b = sample_markdown("Classical Physics", "Newtonian mechanics.");
    let page_b = stele::parser::page::parse_page(&md_b, "classical").unwrap();
    index.index_page(&page_b).await.unwrap();

    let fns = Arc::new(FnsClient::new(
        server.uri(),
        "test-token".into(),
        "test-vault".into(),
    ));
    let config = test_config(&server.uri());
    let reg = OperationRegistry::new(fns, Arc::new(index), config);

    let result = reg
        .execute_mcp("search", Some(json!({"query": "quantum", "limit": 10}).as_object().cloned().unwrap()))
        .await
        .expect("search should succeed");

    assert_eq!(result["query"], "quantum");
    assert!(result["total"].as_u64().unwrap() >= 1);
    let results = result["results"].as_array().unwrap();
    assert!(results.iter().any(|r| r["slug"] == "quantum"));
}

#[tokio::test]
async fn test_search_type_filter() {
    let server = MockServer::start().await;
    let index = test_index().await;

    let md_entity = "---\ntitle: Rust Language\npage_type: Entity\ntags: []\nsources: []\n---\nRust is a systems language.\n";
    let page_e = stele::parser::page::parse_page(md_entity, "rust-lang").unwrap();
    index.index_page(&page_e).await.unwrap();

    let md_concept = "---\ntitle: Ownership Concept\npage_type: Concept\ntags: []\nsources: []\n---\nOwnership is a Rust concept.\n";
    let page_c = stele::parser::page::parse_page(md_concept, "ownership").unwrap();
    index.index_page(&page_c).await.unwrap();

    let fns = Arc::new(FnsClient::new(
        server.uri(),
        "test-token".into(),
        "test-vault".into(),
    ));
    let config = test_config(&server.uri());
    let reg = OperationRegistry::new(fns, Arc::new(index), config);

    let result = reg
        .execute_mcp("search", Some(json!({"query": "rust", "limit": 10, "type_filter": "Entity"}).as_object().cloned().unwrap()))
        .await
        .expect("search should succeed");

    let results = result["results"].as_array().unwrap();
    assert!(results.iter().any(|r| r["slug"] == "rust-lang"));
    assert!(!results.iter().any(|r| r["slug"] == "ownership"));
}

#[tokio::test]
async fn test_graph_query_returns_outlinks() {
    let server = MockServer::start().await;
    let index = test_index().await;

    let md_a = sample_markdown_with_link("page-b");
    let page_a = stele::parser::page::parse_page(&md_a, "page-a").unwrap();
    index.index_page(&page_a).await.unwrap();

    let md_b = sample_markdown("Page B", "B content.");
    let page_b = stele::parser::page::parse_page(&md_b, "page-b").unwrap();
    index.index_page(&page_b).await.unwrap();

    let links = stele::parser::wikilink::extract_links_for_page(&page_a.compiled_truth, "page-a");
    index.update_links("page-a", &links).await.unwrap();

    let fns = Arc::new(FnsClient::new(
        server.uri(),
        "test-token".into(),
        "test-vault".into(),
    ));
    let config = test_config(&server.uri());
    let reg = OperationRegistry::new(fns, Arc::new(index), config);

    let result = reg
        .execute_mcp("graph.query", Some(json!({"slug": "page-a", "depth": 1}).as_object().cloned().unwrap()))
        .await
        .expect("graph query should succeed");

    assert_eq!(result["slug"], "page-a");
    let outlinks = result["outlinks"].as_array().unwrap();
    assert!(!outlinks.is_empty());
    let targets: Vec<&str> = outlinks
        .iter()
        .map(|l| l["target_slug"].as_str().unwrap())
        .collect();
    assert!(targets.contains(&"page-b"));
}

#[tokio::test]
async fn test_graph_backlinks() {
    let server = MockServer::start().await;
    let index = test_index().await;

    let md_a = sample_markdown_with_link("target");
    let page_a = stele::parser::page::parse_page(&md_a, "page-a").unwrap();
    index.index_page(&page_a).await.unwrap();

    let md_b = "---\ntitle: Page B\npage_type: Entity\ntags: []\nsources: []\n---\nAlso links to [[target]].\n";
    let page_b = stele::parser::page::parse_page(md_b, "page-b").unwrap();
    index.index_page(&page_b).await.unwrap();

    let md_target = sample_markdown("Target", "Target content.");
    let page_target = stele::parser::page::parse_page(&md_target, "target").unwrap();
    index.index_page(&page_target).await.unwrap();

    let links_a = stele::parser::wikilink::extract_links_for_page(&page_a.compiled_truth, "page-a");
    index.update_links("page-a", &links_a).await.unwrap();
    let links_b = stele::parser::wikilink::extract_links_for_page(&page_b.compiled_truth, "page-b");
    index.update_links("page-b", &links_b).await.unwrap();

    let fns = Arc::new(FnsClient::new(
        server.uri(),
        "test-token".into(),
        "test-vault".into(),
    ));
    let config = test_config(&server.uri());
    let reg = OperationRegistry::new(fns, Arc::new(index), config);

    let result = reg
        .execute_mcp("graph.backlinks", Some(json!({"slug": "target"}).as_object().cloned().unwrap()))
        .await
        .expect("backlinks should succeed");

    assert_eq!(result["count"], 2);
    let sources: Vec<&str> = result["backlinks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|b| b["source_slug"].as_str().unwrap())
        .collect();
    assert!(sources.contains(&"page-a"));
    assert!(sources.contains(&"page-b"));
}

#[tokio::test]
async fn test_sync_indexes_from_fns() {
    let server = MockServer::start().await;
    let reg = test_registry(&server.uri()).await;

    setup_list_mock(&server, &["alpha.md", "beta.md"]).await;
    setup_folders_mock(&server, &[]).await;

    let md_alpha = sample_markdown("Alpha", "Alpha content.");
    setup_note_get_mock(&server, "alpha.md", &md_alpha).await;

    let md_beta = sample_markdown("Beta", "Beta content.");
    setup_note_get_mock(&server, "beta.md", &md_beta).await;

    let result = reg
        .execute_mcp("sync", None)
        .await
        .expect("sync should succeed");

    assert_eq!(result["pages_indexed"], 2);
    assert_eq!(result["pages_removed"], 0);
    assert!(result["errors"].as_array().unwrap().is_empty());

    let stats = reg.execute_mcp("stats", None).await.unwrap();
    assert_eq!(stats["total_pages"], 2);
}

#[tokio::test]
async fn test_sync_removes_deleted_pages() {
    let server = MockServer::start().await;
    let reg = test_registry(&server.uri()).await;

    let md_keep = sample_markdown("Keep", "Keep content.");
    let md_remove = sample_markdown("Remove", "Remove content.");

    Mock::given(method("GET"))
        .and(path("/api/folder/notes"))
        .and(query_param("vault", "test-vault"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({"code": 1, "status": true, "message": "Success", "data": {"list": [{"path": "keep.md"}, {"path": "remove.md"}], "pager": {"totalRows": 2}}})),
        )
        .up_to_n_times(1)
        .mount(&server)
        .await;

    setup_folders_mock(&server, &[]).await;

    Mock::given(method("GET"))
        .and(path("/api/note"))
        .and(query_param("path", "keep.md"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(fns_string_response(&md_keep)),
        )
        .up_to_n_times(1)
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path("/api/note"))
        .and(query_param("path", "remove.md"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(fns_string_response(&md_remove)),
        )
        .up_to_n_times(1)
        .mount(&server)
        .await;

    reg.execute_mcp("sync", None)
        .await
        .expect("first sync should succeed");

    Mock::given(method("GET"))
        .and(path("/api/folder/notes"))
        .and(query_param("vault", "test-vault"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({"code": 1, "status": true, "message": "Success", "data": {"list": [{"path": "keep.md"}], "pager": {"totalRows": 1}}})),
        )
        .mount(&server)
        .await;

    setup_folders_mock(&server, &[]).await;

    Mock::given(method("GET"))
        .and(path("/api/note"))
        .and(query_param("path", "keep.md"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(fns_string_response(&md_keep)),
        )
        .mount(&server)
        .await;

    let result = reg
        .execute_mcp("sync", None)
        .await
        .expect("second sync should succeed");

    assert_eq!(result["pages_removed"], 1);

    let stats = reg.execute_mcp("stats", None).await.unwrap();
    assert_eq!(stats["total_pages"], 1);
}

#[tokio::test]
async fn test_maintain_detects_orphans() {
    let server = MockServer::start().await;
    let index = test_index().await;

    let md_a = sample_markdown("Page A", "Content A.");
    let page_a = stele::parser::page::parse_page(&md_a, "page-a").unwrap();
    index.index_page(&page_a).await.unwrap();

    let md_b = sample_markdown("Page B", "Content B.");
    let page_b = stele::parser::page::parse_page(&md_b, "page-b").unwrap();
    index.index_page(&page_b).await.unwrap();

    let links = stele::parser::wikilink::extract_links_for_page(&page_a.compiled_truth, "page-a");
    if !links.is_empty() {
        index.update_links("page-a", &links).await.unwrap();
    }

    let fns = Arc::new(FnsClient::new(
        server.uri(),
        "test-token".into(),
        "test-vault".into(),
    ));
    let config = test_config(&server.uri());
    let reg = OperationRegistry::new(fns, Arc::new(index), config);

    let result = reg
        .execute_mcp("maintain", Some(json!({"scope": "orphans"}).as_object().cloned().unwrap()))
        .await
        .expect("maintain should succeed");

    assert_eq!(result["scope"], "orphans");
    let issues = result["issues"].as_array().unwrap();
    assert!(!issues.is_empty());

    let orphan_msgs: Vec<&str> = issues
        .iter()
        .map(|i| i["message"].as_str().unwrap())
        .collect();
    assert!(orphan_msgs.iter().any(|m| m.contains("page-a")));
}

#[tokio::test]
async fn test_stats_returns_counts() {
    let server = MockServer::start().await;
    let index = test_index().await;

    let md_1 = sample_markdown("Page 1", "Content 1.");
    let page_1 = stele::parser::page::parse_page(&md_1, "page-1").unwrap();
    index.index_page(&page_1).await.unwrap();

    let md_2 = sample_markdown("Page 2", "Content 2.");
    let page_2 = stele::parser::page::parse_page(&md_2, "page-2").unwrap();
    index.index_page(&page_2).await.unwrap();

    let md_3 = sample_markdown_with_link("page-2");
    let page_3 = stele::parser::page::parse_page(&md_3, "page-3").unwrap();
    index.index_page(&page_3).await.unwrap();

    let links = stele::parser::wikilink::extract_links_for_page(&page_3.compiled_truth, "page-3");
    index.update_links("page-3", &links).await.unwrap();

    let fns = Arc::new(FnsClient::new(
        server.uri(),
        "test-token".into(),
        "test-vault".into(),
    ));
    let config = test_config(&server.uri());
    let reg = OperationRegistry::new(fns, Arc::new(index), config);

    let result = reg.execute_mcp("stats", None).await.expect("stats should succeed");

    assert_eq!(result["total_pages"], 3);
    assert!(result["total_links"].as_i64().unwrap() >= 1);
    assert!(result["pages_by_type"].is_object());
    assert_eq!(result["pages_by_type"]["Entity"].as_i64().unwrap(), 3);
}

#[tokio::test]
async fn test_reindex_rebuilds_index() {
    let server = MockServer::start().await;
    let reg = test_registry(&server.uri()).await;

    let md = sample_markdown("Pre-existing", "Old content.");

    setup_list_mock(&server, &["pre-existing.md"]).await;
    setup_note_get_mock(&server, "pre-existing.md", &md).await;

    reg.execute_mcp("sync", None)
        .await
        .expect("initial sync should succeed");

    let stats_before = reg.execute_mcp("stats", None).await.unwrap();
    assert_eq!(stats_before["total_pages"], 1);

    let result = reg
        .execute_mcp("reindex", None)
        .await
        .expect("reindex should succeed");

    assert_eq!(result["reindexed"], true);

    let stats_after = reg.execute_mcp("stats", None).await.unwrap();
    assert_eq!(stats_after["total_pages"], 1);
}

#[tokio::test]
async fn test_page_etag_conflict() {
    let server = MockServer::start().await;
    let reg = test_registry(&server.uri()).await;


    setup_note_put_mock(&server, "etag-page").await;

    reg.execute_mcp("page.put", Some(json!({"slug": "etag-page", "body": "Content.\n", "frontmatter": sample_frontmatter("Etag Page"), "timeline": {"content": "Created"}}).as_object().cloned().unwrap()))
    .await
    .expect("first put should succeed");

    let result = reg
        .execute_mcp("page.put", Some(json!({"slug": "etag-page", "body": "Content.\n", "frontmatter": sample_frontmatter("Etag Page"), "timeline": {"content": "Updated"}, "etag": "wrong-etag-hash"}).as_object().cloned().unwrap()))
        .await;

    assert!(result.is_err());
    let err = result.unwrap_err();
    let err_str = err.to_string();
    assert!(
        err_str.contains("conflict") || err_str.contains("etag"),
        "expected conflict error, got: {}",
        err_str
    );
}

#[tokio::test]
async fn test_fns_error_propagates() {
    let server = MockServer::start().await;
    let reg = test_registry(&server.uri()).await;

    Mock::given(method("GET"))
        .and(path("/api/note"))
        .respond_with(ResponseTemplate::new(500).set_body_string("internal server error"))
        .mount(&server)
        .await;

    let result = reg
        .execute_mcp("page.get", Some(json!({"slug": "nonexistent"}).as_object().cloned().unwrap()))
        .await;

    assert!(result.is_err());
    let err_str = result.unwrap_err().to_string();
    assert!(
        err_str.contains("fns") || err_str.contains("server error"),
        "expected FNS error, got: {}",
        err_str
    );
}

#[tokio::test]
async fn test_structured_api_roundtrip() {
    let server = MockServer::start().await;
    let reg = test_registry(&server.uri()).await;
    let today = chrono::Utc::now().format("%Y-%m-%d").to_string();

    setup_note_put_mock(&server, "structured-test").await;

    let put_result = reg
        .execute_mcp("page.put", Some(json!({"slug": "structured-test", "body": "Structured body content.\n", "frontmatter": sample_frontmatter("Structured Test"), "timeline": {"content": "Created via API"}}).as_object().cloned().unwrap()))
        .await
        .expect("put should succeed");

    assert_eq!(put_result["slug"], "structured-test");
    assert_eq!(put_result["indexed"], true);
    assert!(put_result["content_hash"].is_string());
    assert_eq!(put_result["timeline_count"], 1);

    let content_with_timeline = format!(
        "---\ntitle: Structured Test\npage_type: Entity\ntags:\n  - test\nsources: []\n---\nStructured body content.\n---\n- {}: Created via API\n",
        today
    );
    setup_note_get_mock(&server, "structured-test", &content_with_timeline).await;

    let get_result = reg
        .execute_mcp("page.get", Some(json!({"slug": "structured-test"}).as_object().cloned().unwrap()))
        .await
        .expect("get should succeed");

    assert_eq!(get_result["slug"], "structured-test");
    assert_eq!(get_result["body"], "Structured body content.");
    let fm = get_result["frontmatter"].as_object().unwrap();
    assert_eq!(fm["title"], "Structured Test");
    assert_eq!(fm["page_type"], "Entity");
    assert!(fm["tags"].is_array());
    let timeline = get_result["timeline"].as_array().unwrap();
    assert_eq!(timeline.len(), 1);
    assert_eq!(timeline[0]["content"], "Created via API");
    assert!(get_result["content_hash"].is_string());
}

#[tokio::test]
async fn test_timeline_append_only() {
    let server = MockServer::start().await;
    let reg = test_registry(&server.uri()).await;
    let today = chrono::Utc::now().format("%Y-%m-%d").to_string();

    setup_note_put_mock(&server, "timeline-test").await;

    let result1 = reg
        .execute_mcp("page.put", Some(json!({"slug": "timeline-test", "body": "Initial body.\n", "frontmatter": sample_frontmatter("Timeline Test"), "timeline": {"content": "First entry"}}).as_object().cloned().unwrap()))
        .await
        .expect("first put should succeed");
    assert_eq!(result1["timeline_count"], 1);

    let content_1 = markdown_with_timeline("Timeline Test", "Initial body.", &[( &today, "First entry")]);
    Mock::given(method("GET"))
        .and(path("/api/note"))
        .and(query_param("vault", "test-vault"))
        .and(query_param("path", "timeline-test.md"))
        .respond_with(ResponseTemplate::new(200).set_body_json(fns_string_response(&content_1)))
        .up_to_n_times(1)
        .mount(&server)
        .await;

    let result2 = reg
        .execute_mcp("page.put", Some(json!({"slug": "timeline-test", "body": "Updated body.\n", "timeline": {"content": "Second entry"}}).as_object().cloned().unwrap()))
        .await
        .expect("second put should succeed");
    assert_eq!(result2["timeline_count"], 2);

    let content_2 = markdown_with_timeline(
        "Timeline Test",
        "Updated body.",
        &[( &today, "First entry"), ( &today, "Second entry")],
    );
    setup_note_get_mock(&server, "timeline-test", &content_2).await;

    let get_result = reg
        .execute_mcp("page.get", Some(json!({"slug": "timeline-test"}).as_object().cloned().unwrap()))
        .await
        .expect("get should succeed");

    let timeline = get_result["timeline"].as_array().unwrap();
    assert_eq!(timeline.len(), 2);
    let contents: Vec<&str> = timeline
        .iter()
        .map(|e| e["content"].as_str().unwrap())
        .collect();
    assert!(contents.contains(&"First entry"));
    assert!(contents.contains(&"Second entry"));
}

#[tokio::test]
async fn test_frontmatter_merge() {
    let server = MockServer::start().await;
    let reg = test_registry(&server.uri()).await;
    let today = chrono::Utc::now().format("%Y-%m-%d").to_string();

    let fm_full = serde_json::json!({
        "title": "Original Title",
        "page_type": "Concept",
        "tags": ["rust", "test"],
        
        "sources": ["https://example.com"],
        "visibility": "shared"
    });
    setup_note_put_mock(&server, "merge-test").await;

    let _result1 = reg
        .execute_mcp("page.put", Some(json!({"slug": "merge-test", "body": "Original body.\n", "frontmatter": fm_full, "timeline": {"content": "Created"}}).as_object().cloned().unwrap()))
        .await
        .expect("first put should succeed");

    let content_original = format!(
        "---\ntitle: Original Title\npage_type: Concept\ntags:\n  - rust\n  - test\nsources:\n  - https://example.com\n---\nOriginal body.\n---\n- {}: Created\n",
        today
    );
    Mock::given(method("GET"))
        .and(path("/api/note"))
        .and(query_param("vault", "test-vault"))
        .and(query_param("path", "merge-test.md"))
        .respond_with(ResponseTemplate::new(200).set_body_json(fns_string_response(&content_original)))
        .up_to_n_times(1)
        .mount(&server)
        .await;

    let fm_partial = serde_json::json!({
        "title": "Updated Title",
        "visibility": "shared"
    });

    let result2 = reg
        .execute_mcp("page.put", Some(json!({"slug": "merge-test", "body": "Updated body.\n", "frontmatter": fm_partial, "timeline": {"content": "Updated"}}).as_object().cloned().unwrap()))
        .await
        .expect("second put should succeed");
    assert_eq!(result2["timeline_count"], 2);

    let content_merged = format!(
        "---\ntitle: Updated Title\npage_type: Concept\ntags:\n  - rust\n  - test\nsources:\n  - https://example.com\n---\nUpdated body.\n---\n- {}: Created\n- {}: Updated\n",
        today, today
    );
    setup_note_get_mock(&server, "merge-test", &content_merged).await;

    let get_result = reg
        .execute_mcp("page.get", Some(json!({"slug": "merge-test"}).as_object().cloned().unwrap()))
        .await
        .expect("get should succeed");

    let fm = get_result["frontmatter"].as_object().unwrap();
    assert_eq!(fm["title"], "Updated Title");
    assert_eq!(fm["page_type"], "Concept");
    let tags = fm["tags"].as_array().unwrap();
    assert!(tags.iter().any(|t| t == "rust"));
    assert!(tags.iter().any(|t| t == "test"));
    let sources = fm["sources"].as_array().unwrap();
    assert!(sources.iter().any(|s| s == "https://example.com"));
}

#[tokio::test]
async fn test_page_list_with_folders() {
    let server = MockServer::start().await;
    let reg = test_registry(&server.uri()).await;

    setup_list_mock(&server, &["alpha.md", "beta.md", "gamma.md"]).await;
    setup_folders_mock(&server, &["wiki", "projects", "archive"]).await;

    let result = reg
        .execute_mcp("page.list", None)
        .await
        .expect("list should succeed");

    let files = result["files"].as_array().unwrap();
    assert_eq!(files.len(), 3);
    assert!(files.iter().any(|f| f == "alpha.md"));
    assert!(files.iter().any(|f| f == "beta.md"));
    assert!(files.iter().any(|f| f == "gamma.md"));

    let folders = result["folders"].as_array().unwrap();
    assert_eq!(folders.len(), 3);
    assert!(folders.iter().any(|f| f == "wiki"));
    assert!(folders.iter().any(|f| f == "projects"));
    assert!(folders.iter().any(|f| f == "archive"));

    assert_eq!(result["count"], 6);
}

#[tokio::test]
async fn test_maintain_lint_clean() {
    let server = MockServer::start().await;
    let reg = test_registry(&server.uri()).await;

    setup_note_put_mock(&server, "valid-page").await;
    reg.execute_mcp("page.put", Some(json!({"slug": "valid-page", "body": "Valid content.\n", "frontmatter": sample_frontmatter("Valid Page"), "timeline": {"content": "Created"}}).as_object().cloned().unwrap()))
    .await
    .expect("put should succeed");

    let result = reg
        .execute_mcp("maintain", Some(json!({"scope": "lint"}).as_object().cloned().unwrap()))
        .await
        .expect("maintain should succeed");

    assert_eq!(result["scope"], "lint");
    assert_eq!(result["issues_count"], 0);
    let issues = result["issues"].as_array().unwrap();
    assert!(issues.is_empty(), "valid pages should produce no lint issues");
}

#[tokio::test]
async fn test_maintain_backlinks_clean() {
    let server = MockServer::start().await;
    let reg = test_registry(&server.uri()).await;

    setup_note_put_mock(&server, "page-a").await;
    reg.execute_mcp("page.put", Some(json!({"slug": "page-a", "body": "See [[page-b]] for more.\n", "frontmatter": sample_frontmatter("Page A"), "timeline": {"content": "Created"}}).as_object().cloned().unwrap()))
    .await
    .expect("put page-a should succeed");

    setup_note_put_mock(&server, "page-b").await;
    reg.execute_mcp("page.put", Some(json!({"slug": "page-b", "body": "Page B content.\n", "frontmatter": sample_frontmatter("Page B"), "timeline": {"content": "Created"}}).as_object().cloned().unwrap()))
    .await
    .expect("put page-b should succeed");

    let result = reg
        .execute_mcp("maintain", Some(json!({"scope": "backlinks"}).as_object().cloned().unwrap()))
        .await
        .expect("maintain should succeed");

    assert_eq!(result["scope"], "backlinks");
    assert_eq!(result["issues_count"], 0);
    let issues = result["issues"].as_array().unwrap();
    assert!(
        issues.is_empty(),
        "valid wikilinks should produce no broken backlink issues"
    );
}

#[tokio::test]
async fn test_search_cjk() {
    let server = MockServer::start().await;
    let reg = test_registry(&server.uri()).await;

    setup_note_put_mock(&server, "cjk-page").await;
    reg.execute_mcp("page.put", Some(json!({"slug": "cjk-page", "body": "这是一条测试记录，用于验证中文搜索功能。\n", "frontmatter": serde_json::json!({
            "title": "中文测试页面",
            "page_type": "Concept",
            "tags": ["test"],
            
            "sources": [],
            "visibility": "shared"
        }), "timeline": {"content": "Created"}}).as_object().cloned().unwrap()))
    .await
    .expect("put should succeed");

    let result = reg
        .execute_mcp("search", Some(json!({"query": "中文搜索", "limit": 10}).as_object().cloned().unwrap()))
        .await
        .expect("search should succeed");

    assert!(result["total"].as_u64().unwrap() >= 1);
    let results = result["results"].as_array().unwrap();
    assert!(results.iter().any(|r| r["slug"] == "cjk-page"));
}

#[tokio::test]
async fn test_sync_normalizes_slugs() {
    let server = MockServer::start().await;
    let (reg, index) = test_registry_with_index(&server.uri()).await;

    setup_list_mock(&server, &["wiki/test-page.md", "folder/nested.md"]).await;
    setup_folders_mock(&server, &[]).await;

    let md1 = sample_markdown("Test Page", "Test content.");
    setup_note_get_mock(&server, "wiki/test-page.md", &md1).await;

    let md2 = sample_markdown("Nested Page", "Nested content.");
    setup_note_get_mock(&server, "folder/nested.md", &md2).await;

    let result = reg
        .execute_mcp("sync", None)
        .await
        .expect("sync should succeed");

    assert_eq!(result["pages_indexed"], 2);
    assert_eq!(result["errors"].as_array().unwrap().len(), 0);

    let page1 = index.get_page("wiki/test-page").await.unwrap();
    assert!(page1.is_some(), "slug should be normalized to wiki/test-page");

    let page1_md = index.get_page("wiki/test-page.md").await.unwrap();
    assert!(page1_md.is_none(), "slug should not have .md suffix");

    let page2 = index.get_page("folder/nested").await.unwrap();
    assert!(page2.is_some(), "slug should be normalized to folder/nested");

    let page2_md = index.get_page("folder/nested.md").await.unwrap();
    assert!(page2_md.is_none(), "slug should not have .md suffix");
}
