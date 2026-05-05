use std::collections::{HashMap, HashSet};

use anyhow::{Context, Result};
use tracing::{debug, info, warn};

use crate::models::frontmatter::{compute_hash, parse_page};
use crate::models::link::{Link, extract_wikilinks};
use crate::storage::sqlite::SqliteBackend;
use crate::storage::{FileBackend, FileMeta};

const BATCH_SIZE: usize = 50;

#[derive(Debug, Clone, PartialEq)]
pub struct SyncResult {
    pub files_changed: u64,
    pub pages_indexed: u64,
    pub pages_removed: u64,
    pub links_updated: u64,
}

pub async fn brain_sync(
    file_backend: &dyn FileBackend,
    sqlite: &SqliteBackend,
    dir: &str,
    vault: &str,
) -> Result<SyncResult> {
    info!(dir, vault, "starting brain_sync");

    let fns_files = list_md_files_recursive(file_backend, dir).await?;
    info!(count = fns_files.len(), "discovered .md files in FNS");

    let fns_slugs: HashMap<String, FileMeta> = fns_files
        .into_iter()
        .filter_map(|fm| {
            let slug = path_to_slug(&fm.path)?;
            Some((slug, fm))
        })
        .collect();

    let sqlite_hashes: HashMap<String, String> =
        sqlite.list_page_hashes().await?.into_iter().collect();

    let mut files_changed: u64 = 0;
    let mut pages_indexed: u64 = 0;
    let mut links_updated: u64 = 0;

    let fns_slug_keys: HashSet<&str> = fns_slugs.keys().map(|s| s.as_str()).collect();
    let stale_slugs: Vec<String> = sqlite_hashes
        .keys()
        .filter(|slug| !fns_slug_keys.contains(slug.as_str()))
        .cloned()
        .collect();

    let slug_batches: Vec<Vec<&String>> = fns_slugs
        .keys()
        .collect::<Vec<_>>()
        .chunks(BATCH_SIZE)
        .map(|c| c.to_vec())
        .collect();

    for batch in slug_batches {
        for slug_ref in batch {
            let slug = slug_ref.as_str();
            let fm = &fns_slugs[slug];

            let content = match file_backend.get(&fm.path).await {
                Ok(c) => c,
                Err(e) => {
                    warn!(slug, path = %fm.path, error = %e, "failed to read file, skipping");
                    continue;
                }
            };

            let expected_hash = compute_hash(&content);

            let needs_reindex = match sqlite_hashes.get(slug) {
                Some(existing_hash) => existing_hash != &expected_hash,
                None => true,
            };

            if !needs_reindex {
                debug!(slug, "page unchanged, skipping");
                continue;
            }

            files_changed += 1;

            let page = match parse_page(&content, slug, vault) {
                Ok(p) => p,
                Err(e) => {
                    warn!(slug, error = %e, "failed to parse page, skipping");
                    continue;
                }
            };

            sqlite
                .index_page(&page)
                .await
                .with_context(|| format!("indexing page {slug}"))?;
            pages_indexed += 1;
            debug!(slug, "indexed page");

            let extracted = extract_wikilinks(&content);
            let links: Vec<Link> = extracted
                .into_iter()
                .map(|el| Link {
                    source_slug: slug.to_string(),
                    target_slug: el.target_slug,
                    link_type: "link".to_string(),
                    context_snippet: Some(el.context_snippet),
                })
                .collect();

            sqlite
                .update_links(slug, &links)
                .await
                .with_context(|| format!("updating links for {slug}"))?;
            links_updated += links.len() as u64;
            debug!(slug, link_count = links.len(), "updated links");
        }
    }

    let mut pages_removed: u64 = 0;
    for slug in &stale_slugs {
        debug!(slug, "removing stale page from SQLite");
        sqlite
            .remove_page(slug)
            .await
            .with_context(|| format!("removing stale page {slug}"))?;
        pages_removed += 1;
    }

    let result = SyncResult {
        files_changed,
        pages_indexed,
        pages_removed,
        links_updated,
    };

    info!(
        files_changed = result.files_changed,
        pages_indexed = result.pages_indexed,
        pages_removed = result.pages_removed,
        links_updated = result.links_updated,
        "brain_sync complete"
    );

    Ok(result)
}

async fn list_md_files_recursive(backend: &dyn FileBackend, dir: &str) -> Result<Vec<FileMeta>> {
    let mut result = Vec::new();
    let mut stack = vec![dir.to_string()];

    while let Some(current_dir) = stack.pop() {
        let entries = backend
            .list(&current_dir)
            .await
            .with_context(|| format!("listing directory {current_dir}"))?;

        for entry in entries {
            if entry.is_dir {
                stack.push(entry.path);
            } else if entry.path.ends_with(".md") {
                result.push(entry);
            }
        }
    }

    Ok(result)
}

fn path_to_slug(path: &str) -> Option<String> {
    let stripped = path
        .strip_suffix(".md")
        .or_else(|| path.strip_suffix(".MD"))?;
    let slug = stripped.trim();
    if slug.is_empty() {
        return None;
    }
    Some(slug.to_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Mutex;

    use crate::storage::{BackendError, FileStat};
    use chrono::Utc;

    struct MockFileBackend {
        files: Mutex<HashMap<String, String>>,
    }

    impl MockFileBackend {
        fn new(files: Vec<(&str, &str)>) -> Self {
            Self {
                files: Mutex::new(
                    files
                        .into_iter()
                        .map(|(k, v)| (k.to_string(), v.to_string()))
                        .collect(),
                ),
            }
        }
    }

    #[async_trait::async_trait]
    impl FileBackend for MockFileBackend {
        async fn get(&self, path: &str) -> Result<String, BackendError> {
            self.files
                .lock()
                .unwrap()
                .get(path)
                .cloned()
                .ok_or_else(|| BackendError::NotFound(path.to_string()))
        }

        async fn put(&self, _path: &str, _content: &str) -> Result<(), BackendError> {
            unimplemented!()
        }

        async fn append(&self, _path: &str, _content: &str) -> Result<(), BackendError> {
            unimplemented!()
        }

        async fn delete(&self, _path: &str) -> Result<(), BackendError> {
            unimplemented!()
        }

        async fn list(&self, dir: &str) -> Result<Vec<FileMeta>, BackendError> {
            let files = self.files.lock().unwrap();
            let prefix = if dir.is_empty() {
                String::new()
            } else {
                format!("{dir}/")
            };
            let mut seen_dirs = std::collections::HashSet::new();
            let mut result = Vec::new();

            for path in files.keys() {
                if let Some(rest) = path.strip_prefix(&prefix) {
                    if let Some(slash_idx) = rest.find('/') {
                        let dirname = &rest[..slash_idx];
                        let full_dir = format!("{prefix}{dirname}");
                        if seen_dirs.insert(full_dir.clone()) {
                            result.push(FileMeta {
                                path: full_dir,
                                is_dir: true,
                                size: 0,
                                modified: None,
                            });
                        }
                    } else {
                        result.push(FileMeta {
                            path: path.clone(),
                            is_dir: false,
                            size: files[path].len() as u64,
                            modified: None,
                        });
                    }
                }
            }
            Ok(result)
        }

        async fn exists(&self, path: &str) -> Result<bool, BackendError> {
            Ok(self.files.lock().unwrap().contains_key(path))
        }

        async fn stat(&self, path: &str) -> Result<FileStat, BackendError> {
            let content = self.get(path).await?;
            Ok(FileStat {
                size: content.len() as u64,
                modified: Utc::now(),
                content_hash: compute_hash(&content),
            })
        }
    }

    fn sample_md(slug: &str) -> String {
        format!(
            r#"---
type: entity
title: {slug}
tags: [test]
related: []
sources: []
date: 2026-05-05
---

Compiled truth for {slug}."#
        )
    }

    #[test]
    fn path_to_slug_strips_md_extension() {
        assert_eq!(path_to_slug("wiki/test.md"), Some("wiki/test".into()));
        assert_eq!(
            path_to_slug("deep/nested/path.md"),
            Some("deep/nested/path".into())
        );
    }

    #[test]
    fn path_to_slug_handles_uppercase_extension() {
        assert_eq!(path_to_slug("wiki/test.MD"), Some("wiki/test".into()));
    }

    #[test]
    fn path_to_slug_returns_none_for_non_md() {
        assert_eq!(path_to_slug("wiki/test.txt"), None);
        assert_eq!(path_to_slug("wiki/test"), None);
    }

    #[test]
    fn path_to_slug_returns_none_for_empty() {
        assert_eq!(path_to_slug(".md"), None);
    }

    #[tokio::test]
    async fn brain_sync_indexes_new_files() {
        let fns = MockFileBackend::new(vec![
            ("wiki/alpha.md", &sample_md("alpha")),
            ("wiki/beta.md", &sample_md("beta")),
        ]);
        let sqlite = SqliteBackend::new(":memory:").await.unwrap();

        let result = brain_sync(&fns, &sqlite, "wiki", "forge").await.unwrap();

        assert_eq!(result.files_changed, 2);
        assert_eq!(result.pages_indexed, 2);
        assert_eq!(result.pages_removed, 0);

        let alpha = sqlite.get_page("wiki/alpha").await.unwrap();
        assert!(alpha.is_some());
        let beta = sqlite.get_page("wiki/beta").await.unwrap();
        assert!(beta.is_some());
    }

    #[tokio::test]
    async fn brain_sync_skips_unchanged_files() {
        let md = sample_md("stable");
        let fns = MockFileBackend::new(vec![("wiki/stable.md", &md)]);
        let sqlite = SqliteBackend::new(":memory:").await.unwrap();

        let r1 = brain_sync(&fns, &sqlite, "wiki", "forge").await.unwrap();
        assert_eq!(r1.pages_indexed, 1);

        let r2 = brain_sync(&fns, &sqlite, "wiki", "forge").await.unwrap();
        assert_eq!(r2.files_changed, 0);
        assert_eq!(r2.pages_indexed, 0);
    }

    #[tokio::test]
    async fn brain_sync_reindexes_changed_files() {
        let fns = MockFileBackend::new(vec![("wiki/page.md", &sample_md("v1"))]);
        let sqlite = SqliteBackend::new(":memory:").await.unwrap();

        brain_sync(&fns, &sqlite, "wiki", "forge").await.unwrap();

        let fns2 = MockFileBackend::new(vec![("wiki/page.md", &sample_md("v2"))]);
        let result = brain_sync(&fns2, &sqlite, "wiki", "forge").await.unwrap();

        assert_eq!(result.files_changed, 1);
        assert_eq!(result.pages_indexed, 1);
    }

    #[tokio::test]
    async fn brain_sync_removes_stale_pages() {
        let fns = MockFileBackend::new(vec![("wiki/keep.md", &sample_md("keep"))]);
        let sqlite = SqliteBackend::new(":memory:").await.unwrap();

        let stale_page = parse_page(&sample_md("stale"), "wiki/stale", "forge").unwrap();
        sqlite.index_page(&stale_page).await.unwrap();

        let result = brain_sync(&fns, &sqlite, "wiki", "forge").await.unwrap();

        assert_eq!(result.pages_removed, 1);
        assert!(sqlite.get_page("wiki/stale").await.unwrap().is_none());
        assert!(sqlite.get_page("wiki/keep").await.unwrap().is_some());
    }

    #[tokio::test]
    async fn brain_sync_extracts_links() {
        let content = r#"---
type: entity
title: Source
tags: []
related: []
sources: []
date: 2026-05-05
---

See [[target-page]] for more info."#;

        let fns = MockFileBackend::new(vec![("wiki/source.md", content)]);
        let sqlite = SqliteBackend::new(":memory:").await.unwrap();

        let result = brain_sync(&fns, &sqlite, "wiki", "forge").await.unwrap();
        assert!(result.links_updated > 0);

        let backlinks = sqlite.get_backlinks("target-page").await.unwrap();
        assert_eq!(backlinks.len(), 1);
        assert_eq!(backlinks[0].source_slug, "wiki/source");
        assert_eq!(backlinks[0].target_slug, "target-page");
    }

    #[tokio::test]
    async fn brain_sync_handles_empty_directory() {
        let fns = MockFileBackend::new(vec![]);
        let sqlite = SqliteBackend::new(":memory:").await.unwrap();

        let result = brain_sync(&fns, &sqlite, "wiki", "forge").await.unwrap();

        assert_eq!(result.files_changed, 0);
        assert_eq!(result.pages_indexed, 0);
        assert_eq!(result.pages_removed, 0);
        assert_eq!(result.links_updated, 0);
    }

    #[tokio::test]
    async fn brain_sync_skips_non_md_files() {
        let fns = MockFileBackend::new(vec![
            ("wiki/note.md", &sample_md("note")),
            ("wiki/image.png", "binary-data"),
        ]);
        let sqlite = SqliteBackend::new(":memory:").await.unwrap();

        let result = brain_sync(&fns, &sqlite, "wiki", "forge").await.unwrap();

        assert_eq!(result.files_changed, 1);
        assert_eq!(result.pages_indexed, 1);
    }

    #[tokio::test]
    async fn brain_sync_handles_nested_directories() {
        let fns = MockFileBackend::new(vec![
            ("wiki/a/alpha.md", &sample_md("alpha")),
            ("wiki/b/beta.md", &sample_md("beta")),
        ]);
        let sqlite = SqliteBackend::new(":memory:").await.unwrap();

        let result = brain_sync(&fns, &sqlite, "wiki", "forge").await.unwrap();

        assert_eq!(result.files_changed, 2);
        assert_eq!(result.pages_indexed, 2);
    }

    #[tokio::test]
    async fn list_md_files_recursive_finds_nested() {
        let fns = MockFileBackend::new(vec![
            ("wiki/a/deep.md", "content"),
            ("wiki/b/file.md", "content"),
            ("wiki/b/file.txt", "content"),
            ("wiki/top.md", "content"),
        ]);

        let files = list_md_files_recursive(&fns, "wiki").await.unwrap();
        assert_eq!(files.len(), 3);
        let paths: Vec<&str> = files.iter().map(|f| f.path.as_str()).collect();
        assert!(paths.contains(&"wiki/a/deep.md"));
        assert!(paths.contains(&"wiki/b/file.md"));
        assert!(paths.contains(&"wiki/top.md"));
    }

    #[test]
    fn sync_result_fields() {
        let r = SyncResult {
            files_changed: 5,
            pages_indexed: 3,
            pages_removed: 1,
            links_updated: 7,
        };
        assert_eq!(r.files_changed, 5);
        assert_eq!(r.pages_indexed, 3);
        assert_eq!(r.pages_removed, 1);
        assert_eq!(r.links_updated, 7);
    }
}
