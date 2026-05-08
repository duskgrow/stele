use std::sync::Arc;

use serde_json::json;
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

use stele::config::{Config, FnsConfig, IndexConfig, ServerConfig};
use stele::fns::FnsClient;
use stele::index::IndexEngine;
use stele::ops::{Operation, OperationRegistry};

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
        "---\ntitle: {}\npage_type: Entity\ntags:\n  - test\nrelated: []\nsources: []\nstatus: Evergreen\n---\n{}\n",
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

fn fns_array_response(data: Vec<&str>) -> serde_json::Value {
    json!({"code": 1, "status": true, "message": "Success", "data": data})
}

fn fns_success_response() -> serde_json::Value {
    json!({"code": 1, "status": true, "message": "Success", "data": null})
}

async fn setup_note_get_mock(server: &MockServer, slug: &str, content: &str) {
    Mock::given(method("GET"))
        .and(path("/api/note"))
        .and(query_param("vault", "test-vault"))
        .and(query_param("path", slug))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(fns_string_response(content)),
        )
        .mount(server)
        .await;
}

async fn setup_note_put_mock(server: &MockServer, slug: &str) {
    Mock::given(method("POST"))
        .and(path("/api/note"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(fns_success_response()),
        )
        .mount(server)
        .await;
}

async fn setup_note_delete_mock(server: &MockServer, slug: &str) {
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

#[tokio::test]
async fn test_tools_list_returns_all_operations() {
    let server = MockServer::start().await;
    let reg = test_registry(&server.uri()).await;

    let ops = reg.list_operations();
    assert_eq!(ops.len(), 12);

    let names: Vec<&str> = ops.iter().map(|o| o.name.as_str()).collect();
    assert!(names.contains(&"page.get"));
    assert!(names.contains(&"page.put"));
    assert!(names.contains(&"page.delete"));
    assert!(names.contains(&"page.list"));
    assert!(names.contains(&"page.append"));
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
        .execute(Operation::PagePut {
            slug: "test-page".into(),
            content: content.clone(),
            etag: None,
        })
        .await
        .expect("put should succeed");
    assert_eq!(put_result["slug"], "test-page");
    assert_eq!(put_result["indexed"], true);

    let get_result = reg
        .execute(Operation::PageGet {
            slug: "test-page".into(),
        })
        .await
        .expect("get should succeed");
    assert_eq!(get_result["slug"], "test-page");
    assert_eq!(get_result["content"].as_str().unwrap(), content);
    assert!(get_result["frontmatter"].is_object());
    assert!(get_result["metadata"].is_object());
}

#[tokio::test]
async fn test_page_put_auto_indexes() {
    let server = MockServer::start().await;
    let reg = test_registry(&server.uri()).await;

    let content = sample_markdown("Indexed Page", "Searchable content about rust.");

    setup_note_put_mock(&server, "indexed-page").await;

    reg.execute(Operation::PagePut {
        slug: "indexed-page".into(),
        content,
        etag: None,
    })
    .await
    .expect("put should succeed");

    let search_result = reg
        .execute(Operation::Search {
            query: "rust".into(),
            limit: Some(10),
            type_filter: None,
        })
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

    let content = sample_markdown_with_link("target-page");

    setup_note_put_mock(&server, "source-page").await;

    let put_result = reg
        .execute(Operation::PagePut {
            slug: "source-page".into(),
            content,
            etag: None,
        })
        .await
        .expect("put should succeed");
    assert!(put_result["links_count"].as_u64().unwrap() >= 1);

    let backlinks = reg
        .execute(Operation::GraphBacklinks {
            slug: "target-page".into(),
        })
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

    let content = sample_markdown("Delete Me", "Temporary content.");

    setup_note_put_mock(&server, "delete-me").await;
    setup_note_delete_mock(&server, "delete-me").await;

    reg.execute(Operation::PagePut {
        slug: "delete-me".into(),
        content,
        etag: None,
    })
    .await
    .expect("put should succeed");

    let stats_before = reg.execute(Operation::Stats).await.unwrap();
    assert!(stats_before["total_pages"].as_i64().unwrap() >= 1);

    let del_result = reg
        .execute(Operation::PageDelete {
            slug: "delete-me".into(),
        })
        .await
        .expect("delete should succeed");
    assert_eq!(del_result["deleted"], true);

    let stats_after = reg.execute(Operation::Stats).await.unwrap();
    assert_eq!(stats_after["total_pages"].as_i64().unwrap(), 0);
}

#[tokio::test]
async fn test_page_list_returns_files() {
    let server = MockServer::start().await;
    let reg = test_registry(&server.uri()).await;

    setup_list_mock(&server, &["alpha.md", "beta.md", "gamma.md"]).await;

    let result = reg
        .execute(Operation::PageList { dir: None })
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
        .execute(Operation::Search {
            query: "quantum".into(),
            limit: Some(10),
            type_filter: None,
        })
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

    let md_entity = "---\ntitle: Rust Language\npage_type: Entity\ntags: []\nrelated: []\nsources: []\nstatus: Evergreen\n---\nRust is a systems language.\n";
    let page_e = stele::parser::page::parse_page(md_entity, "rust-lang").unwrap();
    index.index_page(&page_e).await.unwrap();

    let md_concept = "---\ntitle: Ownership Concept\npage_type: Concept\ntags: []\nrelated: []\nsources: []\nstatus: Evergreen\n---\nOwnership is a Rust concept.\n";
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
        .execute(Operation::Search {
            query: "rust".into(),
            limit: Some(10),
            type_filter: Some("Entity".into()),
        })
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
        .execute(Operation::GraphQuery {
            slug: "page-a".into(),
            depth: Some(1),
        })
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

    let md_b = "---\ntitle: Page B\npage_type: Entity\ntags: []\nrelated: []\nsources: []\nstatus: Evergreen\n---\nAlso links to [[target]].\n";
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
        .execute(Operation::GraphBacklinks {
            slug: "target".into(),
        })
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
        .execute(Operation::Sync { dir: None })
        .await
        .expect("sync should succeed");

    assert_eq!(result["pages_indexed"], 2);
    assert_eq!(result["pages_removed"], 0);
    assert!(result["errors"].as_array().unwrap().is_empty());

    let stats = reg.execute(Operation::Stats).await.unwrap();
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

    reg.execute(Operation::Sync { dir: None })
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
        .execute(Operation::Sync { dir: None })
        .await
        .expect("second sync should succeed");

    assert_eq!(result["pages_removed"], 1);

    let stats = reg.execute(Operation::Stats).await.unwrap();
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
        .execute(Operation::Maintain {
            scope: Some("orphans".into()),
        })
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

    let result = reg.execute(Operation::Stats).await.expect("stats should succeed");

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

    reg.execute(Operation::Sync { dir: None })
        .await
        .expect("initial sync should succeed");

    let stats_before = reg.execute(Operation::Stats).await.unwrap();
    assert_eq!(stats_before["total_pages"], 1);

    let result = reg
        .execute(Operation::Reindex)
        .await
        .expect("reindex should succeed");

    assert_eq!(result["reindexed"], true);

    let stats_after = reg.execute(Operation::Stats).await.unwrap();
    assert_eq!(stats_after["total_pages"], 1);
}

#[tokio::test]
async fn test_page_etag_conflict() {
    let server = MockServer::start().await;
    let reg = test_registry(&server.uri()).await;

    let content = sample_markdown("Etag Page", "Content.");

    setup_note_put_mock(&server, "etag-page").await;

    reg.execute(Operation::PagePut {
        slug: "etag-page".into(),
        content: content.clone(),
        etag: None,
    })
    .await
    .expect("first put should succeed");

    let result = reg
        .execute(Operation::PagePut {
            slug: "etag-page".into(),
            content,
            etag: Some("wrong-etag-hash".into()),
        })
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
        .execute(Operation::PageGet {
            slug: "nonexistent".into(),
        })
        .await;

    assert!(result.is_err());
    let err_str = result.unwrap_err().to_string();
    assert!(
        err_str.contains("fns") || err_str.contains("server error"),
        "expected FNS error, got: {}",
        err_str
    );
}
