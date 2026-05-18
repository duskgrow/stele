use std::collections::HashSet;

use serde_json::json;
use tracing::{info, warn};

use crate::fns::FnsClient;
use crate::index::IndexEngine;
use crate::ops::is_hidden_path;
use crate::parser::{page as page_parser, wikilink};
use crate::types::Result;

/// Result of a sync operation.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SyncResult {
    pub files_changed: usize,
    pub pages_indexed: usize,
    pub pages_removed: usize,
    pub links_updated: usize,
    pub errors: Vec<String>,
}

/// Maximum recursion depth for directory traversal.
const MAX_SYNC_DEPTH: usize = 10;

/// Recursively sync Markdown notes in `dir` and all its subdirectories, skipping hidden paths.
/// By default callers pass `wiki/`; `raw/` is temporary source material and is not indexed by sync.
async fn sync_directory(
    fns: &FnsClient,
    index: &IndexEngine,
    dir: &str,
    fns_slugs: &mut HashSet<String>,
    result: &mut SyncResult,
    depth: usize,
) {
    if depth > MAX_SYNC_DEPTH {
        warn!(
            "max sync depth ({}) reached at directory: {}",
            MAX_SYNC_DEPTH, dir
        );
        return;
    }

    let notes = match fns.list_notes(dir).await {
        Ok(n) => n,
        Err(e) => {
            result
                .errors
                .push(format!("failed to list notes in {}: {}", dir, e));
            return;
        }
    };
    let notes: Vec<String> = notes.into_iter().filter(|p| !is_hidden_path(p)).collect();

    for file_path in &notes {
        match page_parser::normalize_slug(file_path) {
            Ok(normalized) => {
                fns_slugs.insert(normalized);
            }
            Err(e) => {
                let err_msg = format!("failed to normalize slug for {}: {}", file_path, e);
                warn!("{}", err_msg);
                result.errors.push(err_msg);
                continue;
            }
        }

        let content = match fns.get_note(file_path).await {
            Ok(c) => c,
            Err(e) => {
                let err_msg = format!("failed to fetch {}: {}", file_path, e);
                warn!("{}", err_msg);
                result.errors.push(err_msg);
                continue;
            }
        };

        let slug = match page_parser::normalize_slug(file_path) {
            Ok(s) => s,
            Err(e) => {
                let err_msg = format!("failed to normalize slug for {}: {}", file_path, e);
                warn!("{}", err_msg);
                result.errors.push(err_msg);
                continue;
            }
        };
        let page = match page_parser::parse_page(&content, &slug) {
            Ok(p) => p,
            Err(e) => {
                let err_msg = format!("failed to parse {}: {}", file_path, e);
                warn!("{}", err_msg);
                result.errors.push(err_msg);
                continue;
            }
        };

        let existing = match index.get_page(&slug).await {
            Ok(p) => p,
            Err(e) => {
                let err_msg = format!("failed to check index for {}: {}", slug, e);
                warn!("{}", err_msg);
                result.errors.push(err_msg);
                continue;
            }
        };

        let is_new = existing.is_none();
        let is_changed = existing
            .as_ref()
            .map(|p| p.content_hash != page.content_hash)
            .unwrap_or(false);

        if !is_new && !is_changed {
            continue;
        }

        if let Err(e) = index.index_page(&page).await {
            let err_msg = format!("failed to index {}: {}", slug, e);
            warn!("{}", err_msg);
            result.errors.push(err_msg);
            continue;
        }

        result.pages_indexed += 1;
        if is_new {
            info!("indexed new page: {}", slug);
        } else {
            info!("updated page: {}", slug);
        }

        let links = wikilink::extract_links_for_page(&page.compiled_truth, &slug);

        let link_count = links.len();
        if let Err(e) = index.update_links(&slug, &links).await {
            let err_msg = format!("failed to update links for {}: {}", slug, e);
            warn!("{}", err_msg);
            result.errors.push(err_msg);
        } else {
            result.links_updated += link_count;
        }

        result.files_changed += 1;
    }

    let folders = match fns.list_folders(dir).await {
        Ok(f) => f,
        Err(e) => {
            result
                .errors
                .push(format!("failed to list folders in {}: {}", dir, e));
            return;
        }
    };
    let folders: Vec<String> = folders.into_iter().filter(|p| !is_hidden_path(p)).collect();

    for folder in folders {
        Box::pin(sync_directory(
            fns,
            index,
            &folder,
            fns_slugs,
            result,
            depth + 1,
        ))
        .await;
    }
}

/// Synchronize wiki Markdown pages from FNS to the local index.
/// Defaults to `wiki/`, indexes only Markdown files, skips hidden paths, and leaves `raw/` unindexed.
pub async fn handle_sync(
    fns: &FnsClient,
    index: &IndexEngine,
    dir: Option<&str>,
) -> Result<serde_json::Value> {
    let sync_dir = dir.unwrap_or("wiki");
    info!("starting sync for directory: {}", sync_dir);

    let mut fns_slugs = HashSet::new();
    let mut result = SyncResult {
        files_changed: 0,
        pages_indexed: 0,
        pages_removed: 0,
        links_updated: 0,
        errors: Vec::new(),
    };

    sync_directory(fns, index, sync_dir, &mut fns_slugs, &mut result, 0).await;

    info!(
        "found {} total files across all directories",
        fns_slugs.len()
    );

    let local_slugs = index.list_slugs().await?;
    for slug in local_slugs {
        if !fns_slugs.contains(&slug) {
            if let Err(e) = index.remove_page(&slug).await {
                let err_msg = format!("failed to remove {}: {}", slug, e);
                warn!("{}", err_msg);
                result.errors.push(err_msg);
            } else {
                info!("removed deleted page: {}", slug);
                result.pages_removed += 1;
            }
        }
    }

    info!(
        "sync complete: {} changed, {} indexed, {} removed, {} links, {} errors",
        result.files_changed,
        result.pages_indexed,
        result.pages_removed,
        result.links_updated,
        result.errors.len()
    );

    Ok(json!({
        "files_changed": result.files_changed,
        "pages_indexed": result.pages_indexed,
        "pages_removed": result.pages_removed,
        "links_updated": result.links_updated,
        "errors": result.errors,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn fns_response(data: serde_json::Value) -> serde_json::Value {
        json!({
            "code": 1,
            "status": true,
            "message": "Success",
            "data": data
        })
    }

    fn fns_string_response(s: &str) -> serde_json::Value {
        json!({
            "code": 1,
            "status": true,
            "message": "Success",
            "data": { "content": s, "path": "", "fileLinks": {}, "version": 1 }
        })
    }

    fn sample_markdown(title: &str, body: &str) -> String {
        format!(
            "\
---
title: {}
page_type: Concept
tags: []
sources: []
---
{}
",
            title, body
        )
    }

    fn sample_markdown_with_links(title: &str, targets: &[&str]) -> String {
        let link_text: String = targets
            .iter()
            .map(|t| format!("[[{}]]", t))
            .collect::<Vec<_>>()
            .join(" ");
        sample_markdown(title, &link_text)
    }

    async fn setup_list_mock_for_dir(server: &MockServer, dir: &str, files: &[&str]) {
        let list_items: Vec<serde_json::Value> = files.iter().map(|f| json!({"path": f})).collect();
        let total = files.len();
        let response_data = json!({
            "list": list_items,
            "pager": { "totalRows": total }
        });
        Mock::given(method("GET"))
            .and(path("/api/folder/notes"))
            .and(query_param("vault", "test-vault"))
            .and(query_param("path", dir))
            .respond_with(ResponseTemplate::new(200).set_body_json(fns_response(response_data)))
            .mount(server)
            .await;
    }

    async fn setup_folders_mock(server: &MockServer, dir: &str) {
        Mock::given(method("GET"))
            .and(path("/api/folders"))
            .and(query_param("vault", "test-vault"))
            .and(query_param("path", dir))
            .respond_with(ResponseTemplate::new(200).set_body_json(fns_response(json!([]))))
            .mount(server)
            .await;
    }

    async fn setup_folders_mock_with(server: &MockServer, dir: &str, subdirs: &[&str]) {
        let folder_items: Vec<serde_json::Value> = subdirs
            .iter()
            .map(|f| json!({"path": f, "pathHash": "abc123"}))
            .collect();
        Mock::given(method("GET"))
            .and(path("/api/folders"))
            .and(query_param("vault", "test-vault"))
            .and(query_param("path", dir))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(fns_response(json!(folder_items))),
            )
            .mount(server)
            .await;
    }

    async fn setup_note_mock(server: &MockServer, filename: &str, content: &str) {
        Mock::given(method("GET"))
            .and(path("/api/note"))
            .and(query_param("vault", "test-vault"))
            .and(query_param("path", filename))
            .respond_with(ResponseTemplate::new(200).set_body_json(fns_string_response(content)))
            .mount(server)
            .await;
    }

    #[tokio::test]
    async fn test_sync_new_pages() {
        let server = MockServer::start().await;
        let index = IndexEngine::new(":memory:").await.unwrap();

        setup_list_mock_for_dir(&server, "wiki", &["a.md", "b.md", "c.md"]).await;
        setup_folders_mock(&server, "wiki").await;

        for name in &["a", "b", "c"] {
            let content =
                sample_markdown(&format!("Page {}", name), &format!("Content of {}", name));
            setup_note_mock(&server, &format!("{}.md", name), &content).await;
        }

        let fns = FnsClient::new(
            server.uri(),
            "test-token".to_string(),
            "test-vault".to_string(),
        );
        let result = handle_sync(&fns, &index, None).await.unwrap();

        assert_eq!(result["files_changed"], 3);
        assert_eq!(result["pages_indexed"], 3);
        assert_eq!(result["pages_removed"], 0);
        assert_eq!(result["errors"].as_array().unwrap().len(), 0);

        assert!(index.get_page("a").await.unwrap().is_some());
        assert!(index.get_page("b").await.unwrap().is_some());
        assert!(index.get_page("c").await.unwrap().is_some());
    }

    #[tokio::test]
    async fn test_sync_changed_page() {
        let server = MockServer::start().await;
        let index = IndexEngine::new(":memory:").await.unwrap();

        // Pre-index with old content
        let old_content = sample_markdown("Old Title", "Old content");
        let old_page = page_parser::parse_page(&old_content, "changed").unwrap();
        index.index_page(&old_page).await.unwrap();

        setup_list_mock_for_dir(&server, "wiki", &["changed.md"]).await;
        setup_folders_mock(&server, "wiki").await;

        let new_content = sample_markdown("New Title", "New content");
        setup_note_mock(&server, "changed.md", &new_content).await;

        let fns = FnsClient::new(
            server.uri(),
            "test-token".to_string(),
            "test-vault".to_string(),
        );
        let result = handle_sync(&fns, &index, None).await.unwrap();

        assert_eq!(result["files_changed"], 1);
        assert_eq!(result["pages_indexed"], 1);
        assert_eq!(result["errors"].as_array().unwrap().len(), 0);

        let page = index.get_page("changed").await.unwrap().unwrap();
        assert_eq!(page.frontmatter.title, "New Title");
    }

    #[tokio::test]
    async fn test_sync_deleted_page() {
        let server = MockServer::start().await;
        let index = IndexEngine::new(":memory:").await.unwrap();

        // Pre-index a page
        let content = sample_markdown("To Delete", "Content");
        let page = page_parser::parse_page(&content, "to-delete").unwrap();
        index.index_page(&page).await.unwrap();
        assert!(index.get_page("to-delete").await.unwrap().is_some());

        // FNS returns empty list
        setup_list_mock_for_dir(&server, "wiki", &[]).await;
        setup_folders_mock(&server, "wiki").await;

        let fns = FnsClient::new(
            server.uri(),
            "test-token".to_string(),
            "test-vault".to_string(),
        );
        let result = handle_sync(&fns, &index, None).await.unwrap();

        assert_eq!(result["pages_removed"], 1);
        assert_eq!(result["files_changed"], 0);
        assert_eq!(result["errors"].as_array().unwrap().len(), 0);

        assert!(index.get_page("to-delete").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_sync_resilient_to_failure() {
        let server = MockServer::start().await;
        let index = IndexEngine::new(":memory:").await.unwrap();

        setup_list_mock_for_dir(&server, "wiki", &["good1.md", "bad.md", "good2.md"]).await;
        setup_folders_mock(&server, "wiki").await;

        // good1 succeeds
        let content1 = sample_markdown("Good 1", "Content 1");
        setup_note_mock(&server, "good1.md", &content1).await;

        // bad returns 500
        Mock::given(method("GET"))
            .and(path("/api/note"))
            .and(query_param("path", "bad.md"))
            .respond_with(ResponseTemplate::new(500).set_body_string("server error"))
            .mount(&server)
            .await;

        // good2 succeeds
        let content2 = sample_markdown("Good 2", "Content 2");
        setup_note_mock(&server, "good2.md", &content2).await;

        let fns = FnsClient::new(
            server.uri(),
            "test-token".to_string(),
            "test-vault".to_string(),
        );
        let result = handle_sync(&fns, &index, None).await.unwrap();

        assert_eq!(result["pages_indexed"], 2);
        assert_eq!(result["errors"].as_array().unwrap().len(), 1);
        assert!(result["errors"][0].as_str().unwrap().contains("bad.md"));

        assert!(index.get_page("good1").await.unwrap().is_some());
        assert!(index.get_page("good2").await.unwrap().is_some());
        assert!(index.get_page("bad").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_sync_links_updated() {
        let server = MockServer::start().await;
        let index = IndexEngine::new(":memory:").await.unwrap();

        setup_list_mock_for_dir(&server, "wiki", &["source.md", "target.md"]).await;
        setup_folders_mock(&server, "wiki").await;

        let source_content = sample_markdown_with_links("Source", &["target.md"]);
        setup_note_mock(&server, "source.md", &source_content).await;

        let target_content = sample_markdown("Target", "No links here");
        setup_note_mock(&server, "target.md", &target_content).await;

        let fns = FnsClient::new(
            server.uri(),
            "test-token".to_string(),
            "test-vault".to_string(),
        );
        let result = handle_sync(&fns, &index, None).await.unwrap();

        assert_eq!(result["pages_indexed"], 2);
        assert_eq!(result["links_updated"], 1);
        assert_eq!(result["errors"].as_array().unwrap().len(), 0);

        assert!(index.has_link("source", "target").await.unwrap());
    }

    #[tokio::test]
    async fn test_sync_skips_unchanged() {
        let server = MockServer::start().await;
        let index = IndexEngine::new(":memory:").await.unwrap();

        // Pre-index with same content
        let content = sample_markdown("Existing", "Content");
        let page = page_parser::parse_page(&content, "existing").unwrap();
        index.index_page(&page).await.unwrap();

        setup_list_mock_for_dir(&server, "wiki", &["existing.md"]).await;
        setup_folders_mock(&server, "wiki").await;
        setup_note_mock(&server, "existing.md", &content).await;

        let fns = FnsClient::new(
            server.uri(),
            "test-token".to_string(),
            "test-vault".to_string(),
        );
        let result = handle_sync(&fns, &index, None).await.unwrap();

        assert_eq!(result["files_changed"], 0);
        assert_eq!(result["pages_indexed"], 0);
        assert_eq!(result["pages_removed"], 0);
        assert_eq!(result["links_updated"], 0);
        assert_eq!(result["errors"].as_array().unwrap().len(), 0);
    }

    #[tokio::test]
    async fn test_sync_mixed_new_changed_deleted() {
        let server = MockServer::start().await;
        let index = IndexEngine::new(":memory:").await.unwrap();

        // Pre-index "unchanged" and "will-change"
        let unchanged_content = sample_markdown("Unchanged", "Same");
        let unchanged_page = page_parser::parse_page(&unchanged_content, "unchanged").unwrap();
        index.index_page(&unchanged_page).await.unwrap();

        let old_content = sample_markdown("Old", "Old content");
        let old_page = page_parser::parse_page(&old_content, "will-change").unwrap();
        index.index_page(&old_page).await.unwrap();

        // Pre-index "will-delete"
        let del_content = sample_markdown("Delete", "Gone");
        let del_page = page_parser::parse_page(&del_content, "will-delete").unwrap();
        index.index_page(&del_page).await.unwrap();

        // FNS has: unchanged (same), will-change (different), new-page (new)
        setup_list_mock_for_dir(
            &server,
            "wiki",
            &["unchanged.md", "will-change.md", "new-page.md"],
        )
        .await;
        setup_folders_mock(&server, "wiki").await;
        setup_note_mock(&server, "unchanged.md", &unchanged_content).await;

        let new_change_content = sample_markdown("Changed", "New content");
        setup_note_mock(&server, "will-change.md", &new_change_content).await;

        let new_page_content = sample_markdown("Brand New", "Fresh");
        setup_note_mock(&server, "new-page.md", &new_page_content).await;

        let fns = FnsClient::new(
            server.uri(),
            "test-token".to_string(),
            "test-vault".to_string(),
        );
        let result = handle_sync(&fns, &index, None).await.unwrap();

        assert_eq!(result["files_changed"], 2);
        assert_eq!(result["pages_indexed"], 2);
        assert_eq!(result["pages_removed"], 1);
        assert_eq!(result["errors"].as_array().unwrap().len(), 0);

        assert!(index.get_page("unchanged").await.unwrap().is_some());
        assert!(index.get_page("will-change").await.unwrap().is_some());
        assert!(index.get_page("new-page").await.unwrap().is_some());
        assert!(index.get_page("will-delete").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_sync_multiple_links() {
        let server = MockServer::start().await;
        let index = IndexEngine::new(":memory:").await.unwrap();

        setup_list_mock_for_dir(&server, "wiki", &["hub.md", "a.md", "b.md", "c.md"]).await;
        setup_folders_mock(&server, "wiki").await;

        let hub_content = sample_markdown_with_links("Hub", &["a.md", "b.md", "c.md"]);
        setup_note_mock(&server, "hub.md", &hub_content).await;

        for name in &["a", "b", "c"] {
            let content = sample_markdown(&format!("Page {}", name), "No links");
            setup_note_mock(&server, &format!("{}.md", name), &content).await;
        }

        let fns = FnsClient::new(
            server.uri(),
            "test-token".to_string(),
            "test-vault".to_string(),
        );
        let result = handle_sync(&fns, &index, None).await.unwrap();

        assert_eq!(result["pages_indexed"], 4);
        assert_eq!(result["links_updated"], 3);
        assert_eq!(result["errors"].as_array().unwrap().len(), 0);

        assert!(index.has_link("hub", "a").await.unwrap());
        assert!(index.has_link("hub", "b").await.unwrap());
        assert!(index.has_link("hub", "c").await.unwrap());
    }

    #[tokio::test]
    async fn test_sync_recursive_nested() {
        let server = MockServer::start().await;
        let index = IndexEngine::new(":memory:").await.unwrap();

        setup_list_mock_for_dir(&server, ".", &["root.md"]).await;
        setup_folders_mock_with(&server, ".", &["wiki"]).await;

        let root_content = sample_markdown("Root", "Root page");
        setup_note_mock(&server, "root.md", &root_content).await;

        setup_list_mock_for_dir(&server, "wiki", &["wiki/hello.md"]).await;
        setup_folders_mock(&server, "wiki").await;

        let wiki_content = sample_markdown("Hello", "Wiki page");
        setup_note_mock(&server, "wiki/hello.md", &wiki_content).await;

        let fns = FnsClient::new(
            server.uri(),
            "test-token".to_string(),
            "test-vault".to_string(),
        );
        let result = handle_sync(&fns, &index, Some(".")).await.unwrap();

        assert_eq!(result["pages_indexed"], 2);
        assert_eq!(result["errors"].as_array().unwrap().len(), 0);

        assert!(index.get_page("root").await.unwrap().is_some());
        assert!(index.get_page("wiki/hello").await.unwrap().is_some());
    }

    #[tokio::test]
    async fn test_sync_skips_dot_prefixed_folders() {
        let server = MockServer::start().await;
        let index = IndexEngine::new(":memory:").await.unwrap();

        setup_list_mock_for_dir(&server, ".", &["root.md"]).await;
        setup_folders_mock_with(&server, ".", &[".archive", "wiki", ".hidden"]).await;

        let root_content = sample_markdown("Root", "Root page");
        setup_note_mock(&server, "root.md", &root_content).await;

        setup_list_mock_for_dir(&server, "wiki", &["wiki/hello.md"]).await;
        setup_folders_mock(&server, "wiki").await;

        let wiki_content = sample_markdown("Hello", "Wiki page");
        setup_note_mock(&server, "wiki/hello.md", &wiki_content).await;

        let fns = FnsClient::new(
            server.uri(),
            "test-token".to_string(),
            "test-vault".to_string(),
        );
        let result = handle_sync(&fns, &index, Some(".")).await.unwrap();

        assert_eq!(result["pages_indexed"], 2);
        assert_eq!(result["errors"].as_array().unwrap().len(), 0);

        assert!(index.get_page("root").await.unwrap().is_some());
        assert!(index.get_page("wiki/hello").await.unwrap().is_some());
    }

    #[tokio::test]
    async fn test_sync_skips_dot_prefixed_files() {
        let server = MockServer::start().await;
        let index = IndexEngine::new(":memory:").await.unwrap();

        setup_list_mock_for_dir(&server, "wiki", &[".secret.md", "readme.md"]).await;
        setup_folders_mock(&server, "wiki").await;

        let readme_content = sample_markdown("Readme", "Readme page");
        setup_note_mock(&server, "readme.md", &readme_content).await;

        let fns = FnsClient::new(
            server.uri(),
            "test-token".to_string(),
            "test-vault".to_string(),
        );
        let result = handle_sync(&fns, &index, None).await.unwrap();

        assert_eq!(result["pages_indexed"], 1);
        assert_eq!(result["errors"].as_array().unwrap().len(), 0);

        assert!(index.get_page("readme").await.unwrap().is_some());
        assert!(index.get_page(".secret").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_sync_depth_limit() {
        let server = MockServer::start().await;
        let index = IndexEngine::new(":memory:").await.unwrap();

        // d0 -> d1 -> ... -> d12 (depth limit is 10, so d11+ won't be visited)
        for i in 0..=11 {
            let dir = if i == 0 { "." } else { &format!("d{}", i) };

            if i < 11 {
                setup_list_mock_for_dir(&server, dir, &[]).await;
                let child = format!("d{}", i + 1);
                setup_folders_mock_with(&server, dir, &[&child]).await;
            } else {
                setup_list_mock_for_dir(&server, dir, &[]).await;
                setup_folders_mock(&server, dir).await;
            }
        }

        let fns = FnsClient::new(
            server.uri(),
            "test-token".to_string(),
            "test-vault".to_string(),
        );
        let result = handle_sync(&fns, &index, Some(".")).await.unwrap();

        assert_eq!(result["pages_indexed"], 0);
        assert_eq!(result["errors"].as_array().unwrap().len(), 0);
        assert_eq!(result["pages_removed"], 0);
    }

    #[tokio::test]
    async fn test_sync_removal_across_dirs() {
        let server = MockServer::start().await;
        let index = IndexEngine::new(":memory:").await.unwrap();

        let content1 = sample_markdown("Old Root", "Content");
        let page1 = page_parser::parse_page(&content1, "old-root").unwrap();
        index.index_page(&page1).await.unwrap();

        let content2 = sample_markdown("Old Wiki", "Content");
        let page2 = page_parser::parse_page(&content2, "wiki/old").unwrap();
        index.index_page(&page2).await.unwrap();

        setup_list_mock_for_dir(&server, ".", &["new-root.md"]).await;
        setup_folders_mock_with(&server, ".", &["wiki"]).await;

        let new_content = sample_markdown("New Root", "Content");
        setup_note_mock(&server, "new-root.md", &new_content).await;

        setup_list_mock_for_dir(&server, "wiki", &[]).await;
        setup_folders_mock(&server, "wiki").await;

        let fns = FnsClient::new(
            server.uri(),
            "test-token".to_string(),
            "test-vault".to_string(),
        );
        let result = handle_sync(&fns, &index, Some(".")).await.unwrap();

        assert_eq!(result["pages_indexed"], 1);
        assert_eq!(result["pages_removed"], 2);
        assert_eq!(result["errors"].as_array().unwrap().len(), 0);

        assert!(index.get_page("new-root").await.unwrap().is_some());
        assert!(index.get_page("old-root").await.unwrap().is_none());
        assert!(index.get_page("wiki/old").await.unwrap().is_none());
    }
}
