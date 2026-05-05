use std::collections::HashSet;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::models::frontmatter::parse_page;
use crate::models::link::extract_wikilinks;
use crate::models::{Link, Page, PageType};
use crate::storage::sqlite::SqliteBackend;
use crate::storage::FileBackend;
use tracing::warn;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EnrichResult {
    pub slug: String,
    pub links_created: usize,
    pub stubs_created: usize,
    pub backlinks_updated: usize,
    pub entities_found: Vec<String>,
}

pub async fn brain_enrich(
    file_backend: &dyn FileBackend,
    sqlite: &SqliteBackend,
    slug: &str,
    depth: Option<usize>,
) -> Result<EnrichResult, anyhow::Error> {
    let max_depth = depth.unwrap_or(1);
    if max_depth == 0 {
        anyhow::bail!("depth must be >= 1");
    }

    let mut result = EnrichResult {
        slug: slug.to_string(),
        links_created: 0,
        stubs_created: 0,
        backlinks_updated: 0,
        entities_found: Vec::new(),
    };

    let first_level_targets = process_single_page(file_backend, sqlite, slug, &mut result).await?;

    if max_depth >= 2 {
        for target in first_level_targets {
            if let Err(e) = process_single_page(file_backend, sqlite, &target, &mut result).await {
                warn!(target = %target, error = %e, "failed to process page at depth 2");
            }
        }
    }

    Ok(result)
}

async fn process_single_page(
    file_backend: &dyn FileBackend,
    sqlite: &SqliteBackend,
    slug: &str,
    result: &mut EnrichResult,
) -> Result<Vec<String>, anyhow::Error> {
    let content = file_backend
        .get(slug)
        .await
        .with_context(|| format!("failed to read page {}", slug))?;

    let vault = if let Some(row) = sqlite.get_page(slug).await? {
        row.vault
    } else {
        "forge".to_string()
    };

    let page =
        parse_page(&content, slug, &vault).with_context(|| format!("parsing page {}", slug))?;

    let extracted = extract_wikilinks(&page.raw_content);
    let mut links: Vec<Link> = Vec::new();
    let mut seen_targets = HashSet::new();
    let mut targets = Vec::new();

    for ex in extracted {
        let normalized = normalize_wikilink_target(&ex.target_slug);
        if !seen_targets.insert(normalized.clone()) {
            continue;
        }

        if !ex.target_slug.contains('/') && !result.entities_found.contains(&ex.target_slug) {
            result.entities_found.push(ex.target_slug.clone());
        }

        let exists = sqlite.get_page(&normalized).await?.is_some();
        if !exists {
            let stub = create_stub_page(&normalized, &vault);
            sqlite
                .index_page(&stub)
                .await
                .with_context(|| format!("creating stub for {}", normalized))?;
            result.stubs_created += 1;
        }

        links.push(Link {
            source_slug: slug.to_string(),
            target_slug: normalized.clone(),
            link_type: "link".to_string(),
            context_snippet: Some(ex.context_snippet),
        });

        targets.push(normalized);
    }

    sqlite
        .update_links(slug, &links)
        .await
        .with_context(|| format!("updating links for {}", slug))?;

    result.links_created += links.len();
    result.backlinks_updated += links
        .iter()
        .map(|l| &l.target_slug)
        .collect::<HashSet<_>>()
        .len();



    Ok(targets)
}

fn normalize_wikilink_target(target: &str) -> String {
    if target.contains('/') {
        target.to_string()
    } else {
        format!("entities/{}", target)
    }
}

fn create_stub_page(slug: &str, vault: &str) -> Page {
    use crate::models::Frontmatter;
    use crate::models::PageStatus;
    use chrono::NaiveDate;

    let frontmatter = Frontmatter {
        r#type: PageType::Stub,
        title: slug.to_string(),
        tags: vec![],
        related: vec![],
        sources: vec![],
        date: NaiveDate::from_ymd_opt(2026, 5, 5)
            .unwrap_or(NaiveDate::from_ymd_opt(2000, 1, 1).unwrap()),
        status: Some(PageStatus::Seedling),
    };

    Page {
        slug: slug.to_string(),
        vault: vault.to_string(),
        frontmatter,
        compiled_truth: String::new(),
        timeline: vec![],
        content_hash: "stub".to_string(),
        raw_content: String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::sqlite::SqliteBackend;
    use crate::storage::{BackendError, FileMeta, FileStat};
    use std::collections::HashMap;
    use std::sync::Mutex;

    struct MockBackend {
        files: Mutex<HashMap<String, String>>,
    }

    impl MockBackend {
        fn new() -> Self {
            Self {
                files: Mutex::new(HashMap::new()),
            }
        }

        fn with_file(path: &str, content: &str) -> Self {
            let mut files = HashMap::new();
            files.insert(path.to_string(), content.to_string());
            Self {
                files: Mutex::new(files),
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

        async fn put(&self, _path: &str, _content: &str) -> Result<(), BackendError> {
            unimplemented!()
        }

        async fn append(&self, _path: &str, _content: &str) -> Result<(), BackendError> {
            unimplemented!()
        }

        async fn delete(&self, _path: &str) -> Result<(), BackendError> {
            unimplemented!()
        }

        async fn list(&self, _dir: &str) -> Result<Vec<FileMeta>, BackendError> {
            Ok(vec![])
        }

        async fn exists(&self, path: &str) -> Result<bool, BackendError> {
            let files = self.files.lock().unwrap();
            Ok(files.contains_key(path))
        }

        async fn stat(&self, _path: &str) -> Result<FileStat, BackendError> {
            unimplemented!()
        }
    }

    fn sample_page_content(slug: &str) -> String {
        format!(
            r#"---
type: source
title: {slug}
tags: []
related: []
sources: []
date: 2026-05-05
---
# Compiled Truth

See [[fns]] and [[concepts/llm]] for more info."#
        )
    }

    #[tokio::test]
    async fn brain_enrich_extracts_links_and_creates_stubs() {
        let backend = MockBackend::with_file("wiki/test.md", &sample_page_content("test"));
        let sqlite = SqliteBackend::new(":memory:").await.unwrap();

        let result = brain_enrich(&backend, &sqlite, "wiki/test.md", Some(1))
            .await
            .unwrap();

        assert_eq!(result.slug, "wiki/test.md");
        assert_eq!(result.links_created, 2);
        assert_eq!(result.stubs_created, 2);
        assert_eq!(result.backlinks_updated, 2);
        assert_eq!(result.entities_found, vec!["fns"]);

        let backlinks = sqlite.get_backlinks("entities/fns").await.unwrap();
        assert_eq!(backlinks.len(), 1);
        assert_eq!(backlinks[0].source_slug, "wiki/test.md");

        let backlinks2 = sqlite.get_backlinks("concepts/llm").await.unwrap();
        assert_eq!(backlinks2.len(), 1);
    }

    #[tokio::test]
    async fn brain_enrich_skips_existing_pages() {
        let content = r#"---
type: entity
title: Existing
tags: []
related: []
sources: []
date: 2026-05-05
---
See [[existing-page]] for more."#;

        let backend = MockBackend::with_file("wiki/test.md", content);
        let sqlite = SqliteBackend::new(":memory:").await.unwrap();

        let existing = create_stub_page("entities/existing-page", "forge");
        sqlite.index_page(&existing).await.unwrap();

        let result = brain_enrich(&backend, &sqlite, "wiki/test.md", Some(1))
            .await
            .unwrap();

        assert_eq!(result.links_created, 1);
        assert_eq!(result.stubs_created, 0);
    }

    #[tokio::test]
    async fn brain_enrich_handles_depth_two() {
        let content_a = r#"---
type: entity
title: A
tags: []
related: []
sources: []
date: 2026-05-05
---
See [[b]] for more."#;

        let content_b = r#"---
type: entity
title: B
tags: []
related: []
sources: []
date: 2026-05-05
---
See [[c]] for more."#;

        let backend = MockBackend::new();
        {
            let mut files = backend.files.lock().unwrap();
            files.insert("wiki/a.md".to_string(), content_a.to_string());
            files.insert("entities/b".to_string(), content_b.to_string());
        }
        let sqlite = SqliteBackend::new(":memory:").await.unwrap();

        let result = brain_enrich(&backend, &sqlite, "wiki/a.md", Some(2))
            .await
            .unwrap();

        assert!(result.links_created >= 2);
        assert!(result.stubs_created >= 2);
    }

    #[tokio::test]
    async fn brain_enrich_propagates_read_error_for_root() {
        let backend = MockBackend::new();
        let sqlite = SqliteBackend::new(":memory:").await.unwrap();

        let result = brain_enrich(&backend, &sqlite, "missing.md", Some(1)).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn brain_enrich_source_type_extracts_sources() {
        let content = r#"---
type: source
title: Source
tags: []
related: []
sources: ["2026-05-05-rss"]
date: 2026-05-05
---
See [[entity-page]] for more."#;

        let backend = MockBackend::with_file("wiki/source.md", content);
        let sqlite = SqliteBackend::new(":memory:").await.unwrap();

        let result = brain_enrich(&backend, &sqlite, "wiki/source.md", Some(1))
            .await
            .unwrap();

        assert_eq!(result.links_created, 1);
        assert_eq!(result.entities_found, vec!["entity-page"]);
    }
}
