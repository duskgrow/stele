use chrono::Local;
use regex::Regex;
use serde_json::{Value, json};

use crate::mcp::protocol::{JsonRpcError, ToolContent, ToolResult};
use crate::models::frontmatter::{compute_hash, parse_page};
use crate::models::link::extract_wikilinks;
use crate::storage::sqlite::SqliteBackend;
use crate::storage::{BackendError, FileBackend, FileMeta};

pub async fn brain_append(
    backend: &dyn FileBackend,
    slug: &str,
    timeline_entry: &str,
    date: Option<&str>,
) -> Result<ToolResult, JsonRpcError> {
    let content = backend.get(slug).await.map_err(map_backend_error)?;

    let date_str = date
        .map(|d| d.to_string())
        .unwrap_or_else(|| Local::now().format("%Y-%m-%d").to_string());

    let updated = append_timeline_entry(&content, &date_str, timeline_entry);

    backend
        .put(slug, &updated)
        .await
        .map_err(map_backend_error)?;

    let result_json = json!({
        "slug": slug,
        "updated": true,
        "entries_added": 1,
    });

    Ok(ToolResult {
        content: vec![ToolContent {
            content_type: "text".into(),
            text: result_json.to_string(),
        }],
        is_error: Some(false),
    })
}

fn append_timeline_entry(content: &str, date: &str, entry: &str) -> String {
    let separator_re = Regex::new(r"(?m)^---\s*$").unwrap();
    let matches: Vec<_> = separator_re.find_iter(content).collect();
    let trimmed = content.trim_end();

    if matches.len() >= 3 {
        // Timeline section exists after last ---
        format!("{trimmed}\n- {date}: {entry}")
    } else if matches.len() == 2 {
        // Has frontmatter but no timeline section
        format!("{trimmed}\n\n---\n\n- {date}: {entry}")
    } else {
        // No frontmatter at all – handle gracefully
        format!("{trimmed}\n\n---\n\n- {date}: {entry}")
    }
}

pub async fn brain_list(
    backend: &dyn FileBackend,
    dir: &str,
    recursive: bool,
) -> Result<ToolResult, JsonRpcError> {
    let items = if recursive {
        list_recursive(backend, dir).await
    } else {
        backend.list(dir).await
    }
    .map_err(map_backend_error)?;

    let entries: Vec<Value> = items
        .into_iter()
        .map(|meta| {
            json!({
                "path": meta.path,
                "is_dir": meta.is_dir,
                "size": meta.size,
                "modified": meta.modified.map(|d| d.to_rfc3339()),
            })
        })
        .collect();

    let result_json = json!({ "entries": entries });

    Ok(ToolResult {
        content: vec![ToolContent {
            content_type: "text".into(),
            text: result_json.to_string(),
        }],
        is_error: Some(false),
    })
}

async fn list_recursive(
    backend: &dyn FileBackend,
    dir: &str,
) -> Result<Vec<FileMeta>, BackendError> {
    let mut result = Vec::new();
    let mut stack = vec![dir.to_string()];

    while let Some(current_dir) = stack.pop() {
        let items = backend.list(&current_dir).await?;
        for item in items {
            let is_dir = item.is_dir;
            let path = item.path.clone();
            result.push(item);
            if is_dir {
                stack.push(path);
            }
        }
    }

    Ok(result)
}

fn map_backend_error(err: BackendError) -> JsonRpcError {
    JsonRpcError {
        code: JsonRpcError::INTERNAL_ERROR,
        message: err.to_string(),
        data: None,
    }
}

// ---------------------------------------------------------------------------
// brain_get — read a page from FileBackend and return structured JSON
// ---------------------------------------------------------------------------

pub async fn brain_get(
    backend: &dyn FileBackend,
    sqlite: &SqliteBackend,
    args: Value,
) -> Result<ToolResult, JsonRpcError> {
    let slug = args
        .get("slug")
        .and_then(|v| v.as_str())
        .ok_or_else(|| JsonRpcError {
            code: JsonRpcError::INVALID_PARAMS,
            message: "Missing required 'slug' parameter".into(),
            data: None,
        })?;

    let vault = args
        .get("vault")
        .and_then(|v| v.as_str())
        .unwrap_or("forge");

    let content = backend.get(slug).await.map_err(map_backend_error)?;
    let content_hash = compute_hash(&content);

    let page = parse_page(&content, slug, vault).map_err(|e| JsonRpcError {
        code: JsonRpcError::INTERNAL_ERROR,
        message: format!("Failed to parse page: {e}"),
        data: None,
    })?;

    let frontmatter_json = serde_json::to_value(&page.frontmatter).map_err(|e| JsonRpcError {
        code: JsonRpcError::INTERNAL_ERROR,
        message: format!("Failed to serialize frontmatter: {e}"),
        data: None,
    })?;

    let timeline_json: Vec<Value> = page
        .timeline
        .iter()
        .map(|entry| {
            json!({
                "date": entry.date.to_string(),
                "source_url": entry.source_url,
                "content": entry.content,
            })
        })
        .collect();

    let backlinks = sqlite.get_backlinks(slug).await.map_err(|e| JsonRpcError {
        code: JsonRpcError::INTERNAL_ERROR,
        message: format!("Failed to fetch backlinks: {e}"),
        data: None,
    })?;

    let backlink_slugs: Vec<String> = backlinks.into_iter().map(|l| l.source_slug).collect();

    let result = json!({
        "slug": slug,
        "vault": vault,
        "title": page.frontmatter.title,
        "type": serde_json::to_value(&page.frontmatter.r#type).unwrap_or(json!("unknown")),
        "frontmatter": frontmatter_json,
        "compiled_truth": page.compiled_truth,
        "timeline": timeline_json,
        "content_hash": content_hash,
        "etag": content_hash,
        "related_pages": page.frontmatter.related,
        "backlinks": backlink_slugs,
    });

    Ok(ToolResult {
        content: vec![ToolContent {
            content_type: "text".into(),
            text: result.to_string(),
        }],
        is_error: Some(false),
    })
}

// ---------------------------------------------------------------------------
// brain_put — write a page to FileBackend + index in SQLite
// ---------------------------------------------------------------------------

pub async fn brain_put(
    backend: &dyn FileBackend,
    sqlite: &SqliteBackend,
    args: Value,
) -> Result<ToolResult, JsonRpcError> {
    let slug = args
        .get("slug")
        .and_then(|v| v.as_str())
        .ok_or_else(|| JsonRpcError {
            code: JsonRpcError::INVALID_PARAMS,
            message: "Missing required 'slug' parameter".into(),
            data: None,
        })?;

    let vault = args
        .get("vault")
        .and_then(|v| v.as_str())
        .unwrap_or("forge");

    let content = args
        .get("content")
        .and_then(|v| v.as_str())
        .ok_or_else(|| JsonRpcError {
            code: JsonRpcError::INVALID_PARAMS,
            message: "Missing required 'content' parameter".into(),
            data: None,
        })?;

    let etag = args.get("etag").and_then(|v| v.as_str());

    // Optimistic lock: verify current hash matches provided etag
    if let Some(expected_etag) = etag {
        match backend.get(slug).await {
            Ok(existing) => {
                let current_hash = compute_hash(&existing);
                if current_hash != expected_etag {
                    return Err(JsonRpcError {
                        code: 409,
                        message: format!(
                            "Conflict: etag mismatch (expected {expected_etag}, got {current_hash})"
                        ),
                        data: None,
                    });
                }
            }
            Err(BackendError::NotFound(_)) => {
                // New file — no conflict possible
            }
            Err(e) => return Err(map_backend_error(e)),
        }
    }

    // Check if file already exists (for the created flag)
    let existed = backend.exists(slug).await.unwrap_or(false);

    // Write to FileBackend
    backend.put(slug, content).await.map_err(map_backend_error)?;

    // Parse and index in SQLite
    let new_hash = compute_hash(content);
    let page = parse_page(content, slug, vault).map_err(|e| JsonRpcError {
        code: JsonRpcError::INTERNAL_ERROR,
        message: format!("Failed to parse page: {e}"),
        data: None,
    })?;

    sqlite.index_page(&page).await.map_err(|e| JsonRpcError {
        code: JsonRpcError::INTERNAL_ERROR,
        message: format!("Failed to index page: {e}"),
        data: None,
    })?;

    // Extract wikilinks and update links table
    let extracted = extract_wikilinks(content);
    let links: Vec<crate::models::Link> = extracted
        .into_iter()
        .map(|el| crate::models::Link {
            source_slug: slug.to_string(),
            target_slug: el.target_slug,
            link_type: "link".to_string(),
            context_snippet: Some(el.context_snippet),
        })
        .collect();

    let links_extracted = links.len();
    sqlite.update_links(slug, &links).await.map_err(|e| JsonRpcError {
        code: JsonRpcError::INTERNAL_ERROR,
        message: format!("Failed to update links: {e}"),
        data: None,
    })?;

    let result = json!({
        "slug": slug,
        "vault": vault,
        "title": page.frontmatter.title,
        "content_hash": new_hash,
        "etag": new_hash,
        "created": !existed,
        "indexed": true,
        "links_extracted": links_extracted,
    });

    Ok(ToolResult {
        content: vec![ToolContent {
            content_type: "text".into(),
            text: result.to_string(),
        }],
        is_error: Some(false),
    })
}

// ---------------------------------------------------------------------------
// brain_delete — remove a page from FileBackend + SQLite
// ---------------------------------------------------------------------------

pub async fn brain_delete(
    backend: &dyn FileBackend,
    sqlite: &SqliteBackend,
    args: Value,
) -> Result<ToolResult, JsonRpcError> {
    let slug = args
        .get("slug")
        .and_then(|v| v.as_str())
        .ok_or_else(|| JsonRpcError {
            code: JsonRpcError::INVALID_PARAMS,
            message: "Missing required 'slug' parameter".into(),
            data: None,
        })?;

    // Soft delete: read content → write to .archive/ → delete original
    let content = backend.get(slug).await.map_err(map_backend_error)?;
    let archive_path = format!(".archive/{}", slug);
    backend
        .put(&archive_path, &content)
        .await
        .map_err(map_backend_error)?;
    backend.delete(slug).await.map_err(map_backend_error)?;

    // Remove from SQLite (pages + links)
    let removed = sqlite.remove_page(slug).await.map_err(|e| JsonRpcError {
        code: JsonRpcError::INTERNAL_ERROR,
        message: format!("Failed to remove page from index: {e}"),
        data: None,
    })?;

    let result = json!({
        "slug": slug,
        "archived_to": archive_path,
        "deleted": true,
        "was_indexed": removed,
    });

    Ok(ToolResult {
        content: vec![ToolContent {
            content_type: "text".into(),
            text: result.to_string(),
        }],
        is_error: Some(false),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Mutex;

    struct MockBackend {
        files: Mutex<HashMap<String, String>>,
        lists: Mutex<HashMap<String, Vec<FileMeta>>>,
    }

    impl MockBackend {
        fn new() -> Self {
            Self {
                files: Mutex::new(HashMap::new()),
                lists: Mutex::new(HashMap::new()),
            }
        }

        fn with_file(path: &str, content: &str) -> Self {
            let mut files = HashMap::new();
            files.insert(path.to_string(), content.to_string());
            Self {
                files: Mutex::new(files),
                lists: Mutex::new(HashMap::new()),
            }
        }

        fn with_list(dir: &str, items: Vec<FileMeta>) -> Self {
            let mut lists = HashMap::new();
            lists.insert(dir.to_string(), items);
            Self {
                files: Mutex::new(HashMap::new()),
                lists: Mutex::new(lists),
            }
        }
    }

    #[async_trait::async_trait]
    impl FileBackend for MockBackend {
        async fn get(&self, path: &str) -> Result<String, BackendError> {
            let files = self.files.lock().unwrap();
            files
                .get(path)
                .cloned()
                .ok_or_else(|| BackendError::NotFound(path.to_string()))
        }

        async fn put(&self, path: &str, content: &str) -> Result<(), BackendError> {
            let mut files = self.files.lock().unwrap();
            files.insert(path.to_string(), content.to_string());
            Ok(())
        }

        async fn append(&self, _path: &str, _content: &str) -> Result<(), BackendError> {
            unimplemented!()
        }

        async fn delete(&self, _path: &str) -> Result<(), BackendError> {
            unimplemented!()
        }

        async fn list(&self, dir: &str) -> Result<Vec<FileMeta>, BackendError> {
            let lists = self.lists.lock().unwrap();
            Ok(lists.get(dir).cloned().unwrap_or_default())
        }

        async fn exists(&self, path: &str) -> Result<bool, BackendError> {
            let files = self.files.lock().unwrap();
            Ok(files.contains_key(path))
        }

        async fn stat(&self, _path: &str) -> Result<crate::storage::FileStat, BackendError> {
            unimplemented!()
        }
    }

    #[test]
    fn append_timeline_entry_with_existing_timeline() {
        let content = r#"---
type: entity
title: Test
---
# Compiled Truth

Some content.

---
- 2026-05-01: First entry
"#;
        let updated = append_timeline_entry(content, "2026-05-02", "Second entry");
        assert!(updated.contains("- 2026-05-01: First entry"));
        assert!(updated.contains("- 2026-05-02: Second entry"));
    }

    #[test]
    fn append_timeline_entry_without_timeline() {
        let content = r#"---
type: entity
title: Test
---
# Compiled Truth

Some content.
"#;
        let updated = append_timeline_entry(content, "2026-05-02", "New entry");
        assert!(updated.contains("---\n\n- 2026-05-02: New entry"));
    }

    #[test]
    fn append_timeline_entry_no_frontmatter() {
        let content = "# Just markdown\n\nNo frontmatter.";
        let updated = append_timeline_entry(content, "2026-05-02", "Entry");
        assert!(updated.contains("---\n\n- 2026-05-02: Entry"));
    }

    #[tokio::test]
    async fn brain_append_adds_entry_with_date() {
        let content = r#"---
type: entity
title: Test
---
# Compiled Truth

Content.
"#;
        let backend = MockBackend::with_file("test.md", content);
        let result = brain_append(
            &backend,
            "test.md",
            "New timeline entry",
            Some("2026-05-05"),
        )
        .await
        .unwrap();

        assert_eq!(result.is_error, Some(false));
        let text = &result.content[0].text;
        assert!(text.contains("test.md"));

        let updated = backend.get("test.md").await.unwrap();
        assert!(updated.contains("- 2026-05-05: New timeline entry"));
    }

    #[tokio::test]
    async fn brain_append_adds_entry_with_default_date() {
        let content = r#"---
type: entity
title: Test
---
Content.
"#;
        let backend = MockBackend::with_file("test.md", content);
        let result = brain_append(&backend, "test.md", "Entry with default date", None)
            .await
            .unwrap();

        assert_eq!(result.is_error, Some(false));
        let updated = backend.get("test.md").await.unwrap();
        let today = Local::now().format("%Y-%m-%d").to_string();
        assert!(updated.contains(&format!("- {today}: Entry with default date")));
    }

    #[tokio::test]
    async fn brain_append_propagates_get_error() {
        let backend = MockBackend::new();
        let result = brain_append(&backend, "missing.md", "Entry", Some("2026-05-05")).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn brain_list_returns_entries() {
        let items = vec![
            FileMeta {
                path: "dir/file1.md".into(),
                is_dir: false,
                size: 100,
                modified: None,
            },
            FileMeta {
                path: "dir/file2.md".into(),
                is_dir: false,
                size: 200,
                modified: None,
            },
        ];
        let backend = MockBackend::with_list("dir", items);
        let result = brain_list(&backend, "dir", false).await.unwrap();

        assert_eq!(result.is_error, Some(false));
        let text = &result.content[0].text;
        assert!(text.contains("file1.md"));
        assert!(text.contains("file2.md"));
    }

    #[tokio::test]
    async fn brain_list_recursive_flattens_dirs() {
        let items = vec![
            FileMeta {
                path: "dir/subdir".into(),
                is_dir: true,
                size: 0,
                modified: None,
            },
            FileMeta {
                path: "dir/file1.md".into(),
                is_dir: false,
                size: 100,
                modified: None,
            },
        ];
        let sub_items = vec![FileMeta {
            path: "dir/subdir/nested.md".into(),
            is_dir: false,
            size: 50,
            modified: None,
        }];

        let backend = MockBackend::new();
        {
            let mut lists = backend.lists.lock().unwrap();
            lists.insert("dir".to_string(), items);
            lists.insert("dir/subdir".to_string(), sub_items);
        }

        let result = brain_list(&backend, "dir", true).await.unwrap();
        let text = &result.content[0].text;
        assert!(text.contains("file1.md"));
        assert!(text.contains("subdir"));
        assert!(text.contains("nested.md"));
    }

    #[tokio::test]
    async fn brain_list_empty_directory() {
        let backend = MockBackend::with_list("empty", vec![]);
        let result = brain_list(&backend, "empty", false).await.unwrap();
        let text = &result.content[0].text;
        assert!(text.contains("\"entries\":[]"));
    }
}
