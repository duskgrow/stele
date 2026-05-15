#[cfg(test)]
use std::sync::Arc;

#[cfg(test)]
use crate::config::{Config, FnsConfig, IndexConfig, ServerConfig};
#[cfg(test)]
use crate::fns::FnsClient;
#[cfg(test)]
use crate::index::IndexEngine;
#[cfg(test)]
use crate::ops::registry::OperationRegistry;
#[cfg(test)]
use crate::types::{Frontmatter, Page, PageType, TimelineEntry};

#[cfg(test)]
pub(crate) const TEST_SCHEMA: &str = r#"
    CREATE TABLE IF NOT EXISTS pages (
        slug TEXT PRIMARY KEY,
        title TEXT NOT NULL,
        page_type TEXT NOT NULL DEFAULT '',
        vault TEXT NOT NULL DEFAULT '',
        content_hash TEXT NOT NULL DEFAULT '',
        compiled_truth TEXT NOT NULL DEFAULT '',
        raw_content TEXT NOT NULL DEFAULT '',
        timeline_json TEXT NOT NULL DEFAULT '',
        timeline_text TEXT NOT NULL DEFAULT '',
        frontmatter_json TEXT NOT NULL DEFAULT '',
        tags_json TEXT NOT NULL DEFAULT '',
        created_at TEXT NOT NULL DEFAULT '',
        updated_at TEXT NOT NULL DEFAULT ''
    );

    CREATE TABLE IF NOT EXISTS links (
        source_slug TEXT NOT NULL,
        target_slug TEXT NOT NULL,
        link_type TEXT NOT NULL,
        context_snippet TEXT,
        UNIQUE(source_slug, target_slug, link_type)
    );

    CREATE INDEX IF NOT EXISTS idx_links_source ON links(source_slug);
    CREATE INDEX IF NOT EXISTS idx_links_target ON links(target_slug);
"#;

#[cfg(test)]
pub fn sample_page(slug: &str, title: &str, page_type: PageType, content: &str) -> Page {
    Page {
        slug: slug.to_string(),
        frontmatter: Frontmatter {
            title: title.to_string(),
            page_type,
            tags: vec!["test".to_string()],
            sources: vec![],
            date: None,
            visibility: "shared".to_string(),
            created_by: None,
        },
        compiled_truth: content.to_string(),
        timeline: vec![TimelineEntry {
            date: "2024-01-01".to_string(),
            source_url: None,
            content: "Timeline entry".to_string(),
            agent: None,
        }],
        content_hash: "hash123".to_string(),
        raw_content: format!("# {title}\n\n{content}"),
    }
}

#[cfg(test)]
pub fn sample_page_with_type(slug: &str, content: &str, page_type: PageType) -> Page {
    sample_page(slug, slug, page_type, content)
}

#[cfg(test)]
pub async fn test_registry() -> OperationRegistry {
    let fns = Arc::new(FnsClient::new(
        "http://localhost".into(),
        "test-token".into(),
        "test-vault".into(),
    ).unwrap());
    let index = Arc::new(
        IndexEngine::new("sqlite::memory:")
            .await
            .expect("in-memory index"),
    );
    let config = Config {
        server: ServerConfig {
            host: "127.0.0.1".into(),
            port: 8080,
        },
        fns: FnsConfig {
            base_url: "http://localhost".into(),
            token: "test-token".into(),
            vault: "test-vault".into(),
        },
        index: IndexConfig {
            db_path: "sqlite::memory:".into(),
        },
    };
    OperationRegistry::new(fns, index, config)
}
