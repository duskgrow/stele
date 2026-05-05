use std::collections::HashMap;

use anyhow::{Context, Result};
use sqlx::Row;
use sqlx::sqlite::{SqlitePool, SqlitePoolOptions};

use crate::models::{Link, Page, PageType};

pub struct SqliteBackend {
    pool: SqlitePool,
}

#[derive(Debug, Clone)]
pub struct PageRow {
    pub id: i64,
    pub slug: String,
    pub title: String,
    pub page_type: String,
    pub vault: String,
    pub content_hash: String,
    pub compiled_truth: Option<String>,
    pub timeline: Option<String>,
    pub frontmatter: String,
    pub sources: Option<String>,
    pub tags: Option<String>,
    pub related: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone)]
pub struct SearchHit {
    pub slug: String,
    pub title: String,
    pub compiled_truth_preview: String,
    pub rank: f64,
}

#[derive(Debug, Clone)]
pub struct Stats {
    pub total_pages: i64,
    pub by_type: HashMap<String, i64>,
    pub total_links: i64,
    pub orphan_pages: i64,
    pub db_size_mb: f64,
    pub last_sync: Option<String>,
}

const MIGRATION_001: &str = include_str!("../../migrations/001_init.sql");
const MIGRATION_002: &str = include_str!("../../migrations/002_fts5_vec.sql");

impl SqliteBackend {
    pub async fn new(db_path: &str) -> Result<Self> {
        let url = if db_path == ":memory:" {
            "sqlite::memory:".to_string()
        } else {
            if let Some(parent) = std::path::Path::new(db_path).parent() {
                if !parent.as_os_str().is_empty() {
                    std::fs::create_dir_all(parent)
                        .with_context(|| format!("creating parent dir for {db_path}"))?;
                }
            }
            format!("sqlite:{db_path}")
        };

        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect(&url)
            .await
            .with_context(|| format!("connecting to {url}"))?;

        Self::execute_statements(&pool, MIGRATION_001)
            .await
            .context("running migration 001_init")?;
        Self::execute_statements_lenient(&pool, MIGRATION_002)
            .await
            .context("running migration 002_fts5_vec")?;

        Ok(Self { pool })
    }

    async fn execute_statements(pool: &SqlitePool, sql: &str) -> Result<()> {
        for stmt in split_sql_statements(sql) {
            let trimmed = stmt.trim();
            if trimmed.is_empty() {
                continue;
            }
            sqlx::query(trimmed)
                .execute(pool)
                .await
                .with_context(|| format!("executing stmt: {trimmed:.80}"))?;
        }
        Ok(())
    }

    /// Like `execute_statements` but silently skips statements that fail
    /// (e.g. virtual tables requiring optional extensions like sqlite-vec).
    async fn execute_statements_lenient(pool: &SqlitePool, sql: &str) -> Result<()> {
        for stmt in split_sql_statements(sql) {
            let trimmed = stmt.trim();
            if trimmed.is_empty() {
                continue;
            }
            let _ = sqlx::query(trimmed).execute(pool).await;
        }
        Ok(())
    }

    pub async fn index_page(&self, page: &Page) -> Result<()> {
        let page_type = page_type_str(&page.frontmatter.r#type);
        let timeline_json =
            serde_json::to_string(&page.timeline).context("serializing timeline")?;
        let frontmatter_json =
            serde_json::to_string(&page.frontmatter).context("serializing frontmatter")?;
        let sources_json =
            serde_json::to_string(&page.frontmatter.sources).context("serializing sources")?;
        let tags_json =
            serde_json::to_string(&page.frontmatter.tags).context("serializing tags")?;
        let related_json =
            serde_json::to_string(&page.frontmatter.related).context("serializing related")?;

        sqlx::query(
            r#"
            INSERT INTO pages
                (slug, title, type, vault, content_hash,
                 compiled_truth, timeline, frontmatter,
                 sources, tags, related, updated_at)
            VALUES
                (?1, ?2, ?3, ?4, ?5,
                 ?6, ?7, ?8,
                 ?9, ?10, ?11, CURRENT_TIMESTAMP)
            ON CONFLICT(slug) DO UPDATE SET
                title         = excluded.title,
                type          = excluded.type,
                vault         = excluded.vault,
                content_hash  = excluded.content_hash,
                compiled_truth = excluded.compiled_truth,
                timeline      = excluded.timeline,
                frontmatter   = excluded.frontmatter,
                sources       = excluded.sources,
                tags          = excluded.tags,
                related       = excluded.related,
                updated_at    = CURRENT_TIMESTAMP
            "#,
        )
        .bind(&page.slug)
        .bind(&page.frontmatter.title)
        .bind(page_type)
        .bind(&page.vault)
        .bind(&page.content_hash)
        .bind(&page.compiled_truth)
        .bind(&timeline_json)
        .bind(&frontmatter_json)
        .bind(&sources_json)
        .bind(&tags_json)
        .bind(&related_json)
        .execute(&self.pool)
        .await
        .context("indexing page")?;

        Ok(())
    }

    pub async fn remove_page(&self, slug: &str) -> Result<bool> {
        sqlx::query("DELETE FROM links WHERE source_slug = ?1")
            .bind(slug)
            .execute(&self.pool)
            .await
            .context("deleting outgoing links")?;

        let result = sqlx::query("DELETE FROM pages WHERE slug = ?1")
            .bind(slug)
            .execute(&self.pool)
            .await
            .context("deleting page")?;

        Ok(result.rows_affected() > 0)
    }

    pub async fn list_all_pages(&self) -> Result<Vec<PageRow>> {
        let rows = sqlx::query(
            r#"
            SELECT id, slug, title, type, vault, content_hash,
                   compiled_truth, timeline, frontmatter,
                   sources, tags, related, created_at, updated_at
            FROM pages
            ORDER BY slug
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .context("listing all pages")?;

        Ok(rows
            .into_iter()
            .map(|r| PageRow {
                id: r.get("id"),
                slug: r.get("slug"),
                title: r.get("title"),
                page_type: r.get("type"),
                vault: r.get("vault"),
                content_hash: r.get("content_hash"),
                compiled_truth: r.get("compiled_truth"),
                timeline: r.get("timeline"),
                frontmatter: r.get("frontmatter"),
                sources: r.get("sources"),
                tags: r.get("tags"),
                related: r.get("related"),
                created_at: r.get("created_at"),
                updated_at: r.get("updated_at"),
            })
            .collect())
    }

    pub async fn get_page(&self, slug: &str) -> Result<Option<PageRow>> {
        let row = sqlx::query(
            r#"
            SELECT id, slug, title, type, vault, content_hash,
                   compiled_truth, timeline, frontmatter,
                   sources, tags, related, created_at, updated_at
            FROM pages
            WHERE slug = ?1
            "#,
        )
        .bind(slug)
        .fetch_optional(&self.pool)
        .await
        .context("fetching page")?;

        Ok(row.map(|r| PageRow {
            id: r.get("id"),
            slug: r.get("slug"),
            title: r.get("title"),
            page_type: r.get("type"),
            vault: r.get("vault"),
            content_hash: r.get("content_hash"),
            compiled_truth: r.get("compiled_truth"),
            timeline: r.get("timeline"),
            frontmatter: r.get("frontmatter"),
            sources: r.get("sources"),
            tags: r.get("tags"),
            related: r.get("related"),
            created_at: r.get("created_at"),
            updated_at: r.get("updated_at"),
        }))
    }

    pub async fn list_page_hashes(&self) -> Result<Vec<(String, String)>> {
        let rows = sqlx::query("SELECT slug, content_hash FROM pages")
            .fetch_all(&self.pool)
            .await
            .context("listing page hashes")?;

        Ok(rows
            .into_iter()
            .map(|r| {
                let slug: String = r.get("slug");
                let hash: String = r.get("content_hash");
                (slug, hash)
            })
            .collect())
    }

    pub async fn search_keyword(
        &self,
        query: &str,
        limit: usize,
        type_filter: Option<&str>,
    ) -> Result<Vec<SearchHit>> {
        let rows = if let Some(filter) = type_filter {
            sqlx::query(
                r#"
                SELECT p.slug, p.title, p.compiled_truth, pfts.rank
                FROM pages_fts AS pfts
                JOIN pages AS p ON p.id = pfts.rowid
                WHERE pages_fts MATCH ?1
                  AND p.type = ?2
                ORDER BY pfts.rank
                LIMIT ?3
                "#,
            )
            .bind(query)
            .bind(filter)
            .bind(limit as i64)
            .fetch_all(&self.pool)
            .await
        } else {
            sqlx::query(
                r#"
                SELECT p.slug, p.title, p.compiled_truth, pfts.rank
                FROM pages_fts AS pfts
                JOIN pages AS p ON p.id = pfts.rowid
                WHERE pages_fts MATCH ?1
                ORDER BY pfts.rank
                LIMIT ?2
                "#,
            )
            .bind(query)
            .bind(limit as i64)
            .fetch_all(&self.pool)
            .await
        }
        .context("FTS5 search")?;

        let hits = rows
            .into_iter()
            .map(|r| {
                let full_text: Option<String> = r.get("compiled_truth");
                let preview = full_text.map(|t| truncate_str(&t, 500)).unwrap_or_default();

                SearchHit {
                    slug: r.get("slug"),
                    title: r.get("title"),
                    compiled_truth_preview: preview,
                    rank: r.get("rank"),
                }
            })
            .collect();

        Ok(hits)
    }

    pub async fn update_links(&self, source_slug: &str, links: &[Link]) -> Result<()> {
        sqlx::query("DELETE FROM links WHERE source_slug = ?1")
            .bind(source_slug)
            .execute(&self.pool)
            .await
            .context("clearing old links")?;

        for link in links {
            sqlx::query(
                r#"
                INSERT INTO links (source_slug, target_slug, link_type, context_snippet)
                VALUES (?1, ?2, ?3, ?4)
                ON CONFLICT(source_slug, target_slug, link_type) DO UPDATE SET
                    context_snippet = excluded.context_snippet
                "#,
            )
            .bind(&link.source_slug)
            .bind(&link.target_slug)
            .bind(&link.link_type)
            .bind(&link.context_snippet)
            .execute(&self.pool)
            .await
            .context("upserting link")?;
        }

        Ok(())
    }

    pub async fn get_backlinks(&self, slug: &str) -> Result<Vec<Link>> {
        let rows = sqlx::query(
            "SELECT source_slug, target_slug, link_type, context_snippet
             FROM links WHERE target_slug = ?1",
        )
        .bind(slug)
        .fetch_all(&self.pool)
        .await
        .context("fetching backlinks")?;

        Ok(rows
            .into_iter()
            .map(|r| Link {
                source_slug: r.get("source_slug"),
                target_slug: r.get("target_slug"),
                link_type: r.get("link_type"),
                context_snippet: r.get("context_snippet"),
            })
            .collect())
    }

    pub async fn list_orphan_links(&self) -> Result<Vec<Link>> {
        let rows = sqlx::query(
            r#"
            SELECT l.source_slug, l.target_slug, l.link_type, l.context_snippet
            FROM links l
            LEFT JOIN pages p ON p.slug = l.target_slug
            WHERE p.id IS NULL
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .context("listing orphan links")?;

        Ok(rows
            .into_iter()
            .map(|r| Link {
                source_slug: r.get("source_slug"),
                target_slug: r.get("target_slug"),
                link_type: r.get("link_type"),
                context_snippet: r.get("context_snippet"),
            })
            .collect())
    }

    pub async fn get_stats(&self) -> Result<Stats> {
        let total_pages: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM pages")
            .fetch_one(&self.pool)
            .await
            .context("counting pages")?;

        let total_links: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM links")
            .fetch_one(&self.pool)
            .await
            .context("counting links")?;

        let orphan_pages: i64 = sqlx::query_scalar(
            r#"
            SELECT COUNT(*) FROM pages p
            WHERE NOT EXISTS (SELECT 1 FROM links WHERE source_slug = p.slug)
              AND NOT EXISTS (SELECT 1 FROM links WHERE target_slug = p.slug)
            "#,
        )
        .fetch_one(&self.pool)
        .await
        .context("counting orphan pages")?;

        let type_rows = sqlx::query("SELECT type, COUNT(*) AS cnt FROM pages GROUP BY type")
            .fetch_all(&self.pool)
            .await
            .context("counting by type")?;

        let mut by_type = HashMap::new();
        for r in type_rows {
            let t: String = r.get("type");
            let c: i64 = r.get("cnt");
            by_type.insert(t, c);
        }

        let db_size_mb = self.db_size_mb().await.unwrap_or(0.0);

        let last_sync: Option<String> = sqlx::query_scalar("SELECT MAX(updated_at) FROM pages")
            .fetch_one(&self.pool)
            .await
            .context("fetching last sync")?;

        Ok(Stats {
            total_pages,
            by_type,
            total_links,
            orphan_pages,
            db_size_mb,
            last_sync,
        })
    }

    async fn db_size_mb(&self) -> Result<f64> {
        let page_count: i64 = sqlx::query_scalar("PRAGMA page_count")
            .fetch_one(&self.pool)
            .await?;
        let page_size: i64 = sqlx::query_scalar("PRAGMA page_size")
            .fetch_one(&self.pool)
            .await?;
        Ok((page_count as f64) * (page_size as f64) / (1024.0 * 1024.0))
    }

    /// Check if `source_slug` has a direct wikilink to `target_slug`.
    pub async fn has_direct_link(&self, source_slug: &str, target_slug: &str) -> Result<bool> {
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM links WHERE source_slug = ?1 AND target_slug = ?2)",
        )
        .bind(source_slug)
        .bind(target_slug)
        .fetch_one(&self.pool)
        .await
        .context("checking direct link")?;

        Ok(exists)
    }

    /// Get all target slugs that `slug` links to (outgoing edges).
    pub async fn get_outgoing_link_targets(&self, slug: &str) -> Result<Vec<String>> {
        let rows: Vec<String> =
            sqlx::query_scalar("SELECT target_slug FROM links WHERE source_slug = ?1")
                .bind(slug)
                .fetch_all(&self.pool)
                .await
                .context("fetching outgoing links")?;

        Ok(rows)
    }

    /// Count outgoing links from `slug` (out-degree in the link graph).
    pub async fn outgoing_degree(&self, slug: &str) -> Result<usize> {
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM links WHERE source_slug = ?1")
            .bind(slug)
            .fetch_one(&self.pool)
            .await
            .context("counting outgoing degree")?;

        Ok(count as usize)
    }

    /// Get the `sources` JSON array for a page, parsed as Vec<String>.
    pub async fn get_sources_for_page(&self, slug: &str) -> Result<Vec<String>> {
        let raw: Option<String> = sqlx::query_scalar("SELECT sources FROM pages WHERE slug = ?1")
            .bind(slug)
            .fetch_one(&self.pool)
            .await
            .context("fetching sources")?;

        match raw {
            Some(json_str) => serde_json::from_str(&json_str)
                .map_err(|e| anyhow::anyhow!("parsing sources JSON for {slug}: {e}")),
            None => Ok(Vec::new()),
        }
    }

    /// Get the page type string for a slug.
    pub async fn get_page_type(&self, slug: &str) -> Result<Option<String>> {
        let r#type: Option<String> = sqlx::query_scalar("SELECT type FROM pages WHERE slug = ?1")
            .bind(slug)
            .fetch_one(&self.pool)
            .await
            .context("fetching page type")?;

        Ok(r#type)
    }
}

fn page_type_str(pt: &PageType) -> &'static str {
    match pt {
        PageType::Entity => "entity",
        PageType::Concept => "concept",
        PageType::Source => "source",
        PageType::Query => "query",
        PageType::Synthesis => "synthesis",
        PageType::Comparison => "comparison",
        PageType::Stub => "stub",
    }
}

pub(crate) fn truncate_str(s: &str, max_chars: usize) -> String {
    if s.len() <= max_chars {
        s.to_string()
    } else {
        let end = s
            .char_indices()
            .nth(max_chars)
            .map(|(i, _)| i)
            .unwrap_or(s.len());
        format!("{}…", &s[..end])
    }
}

fn split_sql_statements(sql: &str) -> Vec<String> {
    let mut statements = Vec::new();
    let mut current = String::new();
    let mut depth = 0u32;

    for line in sql.lines() {
        let stripped_comments = if let Some(idx) = line.find("--") {
            &line[..idx]
        } else {
            line
        };

        let upper = stripped_comments.to_uppercase();
        let begins = upper.matches(" BEGIN").count() as u32;
        let ends = upper.matches("END").count() as u32;
        depth += begins;

        current.push_str(line);
        current.push('\n');

        if depth > 0 {
            depth = depth.saturating_sub(ends);
        }

        if depth == 0 && stripped_comments.trim_end().ends_with(';') {
            statements.push(std::mem::take(&mut current));
        }
    }

    if !current.trim().is_empty() {
        statements.push(current);
    }

    statements
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::Frontmatter;
    use chrono::NaiveDate;

    fn sample_page(slug: &str, title: &str, page_type: PageType) -> Page {
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
            compiled_truth: "Some compiled truth content about this page.".to_string(),
            timeline: vec![],
            content_hash: "abc123".to_string(),
            raw_content: "# Raw".to_string(),
        }
    }

    async fn in_memory_backend() -> SqliteBackend {
        SqliteBackend::new(":memory:")
            .await
            .expect("creating in-memory backend")
    }

    #[tokio::test]
    async fn test_index_and_get_page() {
        let backend = in_memory_backend().await;
        let page = sample_page("wiki/test", "Test Page", PageType::Entity);

        backend.index_page(&page).await.unwrap();

        let row = backend.get_page("wiki/test").await.unwrap();
        assert!(row.is_some());
        let row = row.unwrap();
        assert_eq!(row.slug, "wiki/test");
        assert_eq!(row.title, "Test Page");
        assert_eq!(row.page_type, "entity");
        assert_eq!(row.vault, "forge");
        assert_eq!(row.content_hash, "abc123");
        assert_eq!(
            row.compiled_truth.as_deref(),
            Some("Some compiled truth content about this page.")
        );
    }

    #[tokio::test]
    async fn test_index_upsert() {
        let backend = in_memory_backend().await;
        let mut page = sample_page("wiki/upsert", "Original", PageType::Concept);
        backend.index_page(&page).await.unwrap();

        page.frontmatter.title = "Updated".to_string();
        page.content_hash = "def456".to_string();
        backend.index_page(&page).await.unwrap();

        let row = backend.get_page("wiki/upsert").await.unwrap().unwrap();
        assert_eq!(row.title, "Updated");
        assert_eq!(row.content_hash, "def456");
    }

    #[tokio::test]
    async fn test_remove_page() {
        let backend = in_memory_backend().await;
        let page = sample_page("wiki/remove-me", "ToRemove", PageType::Source);
        backend.index_page(&page).await.unwrap();

        let removed = backend.remove_page("wiki/remove-me").await.unwrap();
        assert!(removed);

        let row = backend.get_page("wiki/remove-me").await.unwrap();
        assert!(row.is_none());
    }

    #[tokio::test]
    async fn test_remove_nonexistent() {
        let backend = in_memory_backend().await;
        let removed = backend.remove_page("no-such-slug").await.unwrap();
        assert!(!removed);
    }

    #[tokio::test]
    async fn test_search_keyword_roundtrip() {
        let backend = in_memory_backend().await;

        backend
            .index_page(&sample_page(
                "wiki/quantum",
                "Quantum Computing",
                PageType::Entity,
            ))
            .await
            .unwrap();
        backend
            .index_page(&sample_page(
                "wiki/classical",
                "Classical Computing",
                PageType::Entity,
            ))
            .await
            .unwrap();

        let hits = backend.search_keyword("quantum", 10, None).await.unwrap();
        assert!(!hits.is_empty());
        assert!(hits.iter().any(|h| h.slug == "wiki/quantum"));
    }

    #[tokio::test]
    async fn test_search_keyword_compiled_truth() {
        let backend = in_memory_backend().await;

        let mut page = sample_page("wiki/ai", "AI Research", PageType::Concept);
        page.compiled_truth =
            "Neural networks are the backbone of modern deep learning systems.".to_string();
        backend.index_page(&page).await.unwrap();

        let hits = backend.search_keyword("neural", 10, None).await.unwrap();
        assert!(!hits.is_empty());
        assert_eq!(hits[0].slug, "wiki/ai");
        assert!(hits[0].compiled_truth_preview.contains("Neural"));
    }

    #[tokio::test]
    async fn test_search_keyword_limit() {
        let backend = in_memory_backend().await;

        for i in 0..5 {
            let page = sample_page(
                &format!("wiki/page{i}"),
                &format!("Search Result {i}"),
                PageType::Entity,
            );
            backend.index_page(&page).await.unwrap();
        }

        let hits = backend.search_keyword("search", 2, None).await.unwrap();
        assert!(hits.len() <= 2);
    }

    #[tokio::test]
    async fn test_search_no_results() {
        let backend = in_memory_backend().await;
        let page = sample_page("wiki/only", "Only Page", PageType::Entity);
        backend.index_page(&page).await.unwrap();

        let hits = backend
            .search_keyword("zzzznonexistent", 10, None)
            .await
            .unwrap();
        assert!(hits.is_empty());
    }

    #[tokio::test]
    async fn test_update_and_get_backlinks() {
        let backend = in_memory_backend().await;

        backend
            .index_page(&sample_page("wiki/a", "Page A", PageType::Entity))
            .await
            .unwrap();
        backend
            .index_page(&sample_page("wiki/b", "Page B", PageType::Concept))
            .await
            .unwrap();

        let links = vec![Link {
            source_slug: "wiki/a".to_string(),
            target_slug: "wiki/b".to_string(),
            link_type: "link".to_string(),
            context_snippet: Some("see Page B".to_string()),
        }];
        backend.update_links("wiki/a", &links).await.unwrap();

        let backlinks = backend.get_backlinks("wiki/b").await.unwrap();
        assert_eq!(backlinks.len(), 1);
        assert_eq!(backlinks[0].source_slug, "wiki/a");
        assert_eq!(backlinks[0].target_slug, "wiki/b");
    }

    #[tokio::test]
    async fn test_update_links_replaces_old() {
        let backend = in_memory_backend().await;

        backend
            .index_page(&sample_page("wiki/src", "Source", PageType::Entity))
            .await
            .unwrap();
        backend
            .index_page(&sample_page("wiki/tgt1", "Target 1", PageType::Entity))
            .await
            .unwrap();
        backend
            .index_page(&sample_page("wiki/tgt2", "Target 2", PageType::Entity))
            .await
            .unwrap();

        backend
            .update_links(
                "wiki/src",
                &[Link {
                    source_slug: "wiki/src".to_string(),
                    target_slug: "wiki/tgt1".to_string(),
                    link_type: "link".to_string(),
                    context_snippet: None,
                }],
            )
            .await
            .unwrap();

        backend
            .update_links(
                "wiki/src",
                &[Link {
                    source_slug: "wiki/src".to_string(),
                    target_slug: "wiki/tgt2".to_string(),
                    link_type: "works_at".to_string(),
                    context_snippet: Some("employed here".to_string()),
                }],
            )
            .await
            .unwrap();

        let bl1 = backend.get_backlinks("wiki/tgt1").await.unwrap();
        assert!(bl1.is_empty());

        let bl2 = backend.get_backlinks("wiki/tgt2").await.unwrap();
        assert_eq!(bl2.len(), 1);
        assert_eq!(bl2[0].link_type, "works_at");
    }

    #[tokio::test]
    async fn test_list_orphan_links() {
        let backend = in_memory_backend().await;

        backend
            .index_page(&sample_page("wiki/exists", "Exists", PageType::Entity))
            .await
            .unwrap();

        backend
            .update_links(
                "wiki/exists",
                &[Link {
                    source_slug: "wiki/exists".to_string(),
                    target_slug: "wiki/missing".to_string(),
                    link_type: "link".to_string(),
                    context_snippet: None,
                }],
            )
            .await
            .unwrap();

        let orphans = backend.list_orphan_links().await.unwrap();
        assert_eq!(orphans.len(), 1);
        assert_eq!(orphans[0].target_slug, "wiki/missing");
    }

    #[tokio::test]
    async fn test_list_orphan_links_none() {
        let backend = in_memory_backend().await;

        backend
            .index_page(&sample_page("wiki/a", "A", PageType::Entity))
            .await
            .unwrap();
        backend
            .index_page(&sample_page("wiki/b", "B", PageType::Entity))
            .await
            .unwrap();

        backend
            .update_links(
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

        let orphans = backend.list_orphan_links().await.unwrap();
        assert!(orphans.is_empty());
    }

    #[tokio::test]
    async fn test_get_stats_empty() {
        let backend = in_memory_backend().await;
        let stats = backend.get_stats().await.unwrap();

        assert_eq!(stats.total_pages, 0);
        assert_eq!(stats.total_links, 0);
        assert_eq!(stats.orphan_pages, 0);
        assert!(stats.by_type.is_empty());
        assert!(stats.last_sync.is_none());
    }

    #[tokio::test]
    async fn test_get_stats_with_data() {
        let backend = in_memory_backend().await;

        backend
            .index_page(&sample_page("wiki/e1", "Entity 1", PageType::Entity))
            .await
            .unwrap();
        backend
            .index_page(&sample_page("wiki/e2", "Entity 2", PageType::Entity))
            .await
            .unwrap();
        backend
            .index_page(&sample_page("wiki/c1", "Concept 1", PageType::Concept))
            .await
            .unwrap();

        backend
            .update_links(
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

        let stats = backend.get_stats().await.unwrap();
        assert_eq!(stats.total_pages, 3);
        assert_eq!(stats.total_links, 1);
        assert_eq!(stats.orphan_pages, 1);
        assert_eq!(stats.by_type.get("entity"), Some(&2));
        assert_eq!(stats.by_type.get("concept"), Some(&1));
        assert!(stats.last_sync.is_some());
    }

    #[tokio::test]
    async fn test_get_page_not_found() {
        let backend = in_memory_backend().await;
        let row = backend.get_page("no/such/page").await.unwrap();
        assert!(row.is_none());
    }

    #[tokio::test]
    async fn test_backlinks_empty() {
        let backend = in_memory_backend().await;
        let backlinks = backend.get_backlinks("wiki/lonely").await.unwrap();
        assert!(backlinks.is_empty());
    }

    #[tokio::test]
    async fn test_remove_page_cleans_links() {
        let backend = in_memory_backend().await;

        backend
            .index_page(&sample_page("wiki/src", "Source", PageType::Entity))
            .await
            .unwrap();
        backend
            .index_page(&sample_page("wiki/tgt", "Target", PageType::Entity))
            .await
            .unwrap();

        backend
            .update_links(
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

        backend.remove_page("wiki/src").await.unwrap();

        let backlinks = backend.get_backlinks("wiki/tgt").await.unwrap();
        assert!(backlinks.is_empty());
    }
}
