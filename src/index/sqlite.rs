use crate::types::{Error, Frontmatter, Link, LinkType, Page, Result, TimelineEntry};
use chrono::Utc;
use sqlx::{Pool, Sqlite, sqlite::SqlitePoolOptions};
use std::collections::HashMap;
use std::path::Path;
use tracing::info;

/// SQLite-backed engine for indexing pages, links, and full-text search.
pub struct IndexEngine {
    pool: Pool<Sqlite>,
}

/// Aggregate statistics about the indexed knowledge base.
#[derive(Debug, Clone, PartialEq)]
pub struct IndexStats {
    pub total_pages: i64,
    pub pages_by_type: HashMap<String, i64>,
    pub total_links: i64,
    pub orphan_count: i64,
}

impl IndexEngine {
    /// Access the underlying SQLite connection pool.
    pub fn pool(&self) -> &Pool<Sqlite> {
        &self.pool
    }

    /// Open (or create) the SQLite database and run migrations.
    pub async fn new(db_path: &str) -> Result<Self> {
        // Ensure parent directory exists so SQLite can create the file.
        if !db_path.contains("memory") {
            if let Some(parent) = Path::new(db_path).parent() {
                if !parent.as_os_str().is_empty() && !parent.exists() {
                    info!("db directory missing, creating: {}", parent.display());
                    std::fs::create_dir_all(parent)
                        .map_err(|e| Error::Storage(format!("create db dir: {e}")))?;
                }
            }
        }

        let max_conn = if db_path.contains("memory") { 1 } else { 5 };
        let pool = SqlitePoolOptions::new()
            .max_connections(max_conn)
            .connect(db_path)
            .await
            .map_err(|e| Error::Storage(format!("connect: {e}")))?;

        let engine = Self { pool };
        engine.init().await?;
        Ok(engine)
    }

    async fn init(&self) -> Result<()> {
        let schema = r#"
            CREATE TABLE IF NOT EXISTS pages (
                slug TEXT PRIMARY KEY,
                title TEXT NOT NULL,
                page_type TEXT NOT NULL,
                vault TEXT NOT NULL DEFAULT '',
                content_hash TEXT NOT NULL,
                compiled_truth TEXT NOT NULL,
                raw_content TEXT NOT NULL,
                timeline_json TEXT NOT NULL,
                timeline_text TEXT NOT NULL DEFAULT '',
                frontmatter_json TEXT NOT NULL,
                tags_json TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
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

            CREATE VIRTUAL TABLE IF NOT EXISTS pages_fts USING fts5(
                slug,
                title,
                compiled_truth,
                timeline_text,
                content=pages,
                content_rowid=rowid,
                tokenize='trigram'
            );

            CREATE TRIGGER IF NOT EXISTS pages_fts_insert AFTER INSERT ON pages BEGIN
                INSERT INTO pages_fts(rowid, slug, title, compiled_truth, timeline_text)
                VALUES (new.rowid, new.slug, new.title, new.compiled_truth, new.timeline_text);
            END;

            CREATE TRIGGER IF NOT EXISTS pages_fts_delete AFTER DELETE ON pages BEGIN
                INSERT INTO pages_fts(pages_fts, rowid, slug, title, compiled_truth, timeline_text)
                VALUES ('delete', old.rowid, old.slug, old.title, old.compiled_truth, old.timeline_text);
            END;

            CREATE TRIGGER IF NOT EXISTS pages_fts_update AFTER UPDATE ON pages BEGIN
                INSERT INTO pages_fts(pages_fts, rowid, slug, title, compiled_truth, timeline_text)
                VALUES ('delete', old.rowid, old.slug, old.title, old.compiled_truth, old.timeline_text);
                INSERT INTO pages_fts(rowid, slug, title, compiled_truth, timeline_text)
                VALUES (new.rowid, new.slug, new.title, new.compiled_truth, new.timeline_text);
            END;
        "#;

        sqlx::raw_sql(schema)
            .execute(&self.pool)
            .await
            .map_err(|e| Error::Storage(format!("schema init: {e}")))?;

        for sql in [
            "ALTER TABLE pages ADD COLUMN visibility TEXT NOT NULL DEFAULT 'shared'",
            "ALTER TABLE pages ADD COLUMN created_by TEXT",
        ] {
            if let Err(e) = sqlx::query(sql).execute(&self.pool).await {
                let err_str = e.to_string().to_lowercase();
                if !err_str.contains("duplicate column name") {
                    return Err(Error::Storage(format!("schema migration: {e}")));
                }
            }
        }

        sqlx::query(
            "UPDATE pages SET slug = SUBSTR(slug, 1, LENGTH(slug) - 3) WHERE slug LIKE '%.md'",
        )
        .execute(&self.pool)
        .await
        .map_err(|e| Error::Storage(format!("migrate pages slug: {e}")))?;

        sqlx::query(
            "UPDATE links SET source_slug = SUBSTR(source_slug, 1, LENGTH(source_slug) - 3) WHERE source_slug LIKE '%.md'",
        )
        .execute(&self.pool)
        .await
        .map_err(|e| Error::Storage(format!("migrate links source_slug: {e}")))?;

        sqlx::query(
            "UPDATE links SET target_slug = SUBSTR(target_slug, 1, LENGTH(target_slug) - 3) WHERE target_slug LIKE '%.md'",
        )
        .execute(&self.pool)
        .await
        .map_err(|e| Error::Storage(format!("migrate links target_slug: {e}")))?;

        sqlx::query("INSERT INTO pages_fts(pages_fts) VALUES('rebuild')")
            .execute(&self.pool)
            .await
            .map_err(|e| Error::Storage(format!("fts rebuild: {e}")))?;

        Ok(())
    }

    /// Insert or update a page in the index.
    pub async fn index_page(&self, page: &Page) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        let timeline_json = serde_json::to_string(&page.timeline)?;
        let timeline_text = page
            .timeline
            .iter()
            .map(|e| e.content.as_str())
            .collect::<Vec<_>>()
            .join(" ");
        let frontmatter_json = serde_json::to_string(&page.frontmatter)?;
        let tags_json = serde_json::to_string(&page.frontmatter.tags)?;

        let page_type_str = page.frontmatter.page_type.to_string();

        sqlx::query(
            r#"
            INSERT INTO pages (
                slug, title, page_type, vault, content_hash, compiled_truth, raw_content,
                timeline_json, timeline_text, frontmatter_json, tags_json, created_at, updated_at,
                visibility, created_by
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)
            ON CONFLICT(slug) DO UPDATE SET
                title = excluded.title,
                page_type = excluded.page_type,
                vault = excluded.vault,
                content_hash = excluded.content_hash,
                compiled_truth = excluded.compiled_truth,
                raw_content = excluded.raw_content,
                timeline_json = excluded.timeline_json,
                timeline_text = excluded.timeline_text,
                frontmatter_json = excluded.frontmatter_json,
                tags_json = excluded.tags_json,
                updated_at = excluded.updated_at,
                visibility = excluded.visibility,
                created_by = excluded.created_by
            "#,
        )
        .bind(&page.slug)
        .bind(&page.frontmatter.title)
        .bind(page_type_str)
        .bind("")
        .bind(&page.content_hash)
        .bind(&page.compiled_truth)
        .bind(&page.raw_content)
        .bind(&timeline_json)
        .bind(&timeline_text)
        .bind(&frontmatter_json)
        .bind(&tags_json)
        .bind(&now)
        .bind(&now)
        .bind(&page.frontmatter.visibility)
        .bind(&page.frontmatter.created_by)
        .execute(&self.pool)
        .await
        .map_err(|e| Error::Storage(format!("index_page: {e}")))?;

        Ok(())
    }

    /// Remove a page and all its links from the index.
    pub async fn remove_page(&self, slug: &str) -> Result<()> {
        sqlx::query("DELETE FROM links WHERE source_slug = ?1 OR target_slug = ?1")
            .bind(slug)
            .execute(&self.pool)
            .await
            .map_err(|e| Error::Storage(format!("remove_page links: {e}")))?;

        sqlx::query("DELETE FROM pages WHERE slug = ?1")
            .bind(slug)
            .execute(&self.pool)
            .await
            .map_err(|e| Error::Storage(format!("remove_page: {e}")))?;

        Ok(())
    }

    /// Retrieve a page by slug, if it exists.
    pub async fn get_page(&self, slug: &str) -> Result<Option<Page>> {
        let row: Option<(String, String, String, String, String, String)> = sqlx::query_as(
            r#"
            SELECT slug, frontmatter_json, compiled_truth, timeline_json, content_hash, raw_content
            FROM pages WHERE slug = ?1
            "#,
        )
        .bind(slug)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| Error::Storage(format!("get_page: {e}")))?;

        match row {
            Some((
                slug,
                frontmatter_json,
                compiled_truth,
                timeline_json,
                content_hash,
                raw_content,
            )) => {
                let frontmatter: Frontmatter = serde_json::from_str(&frontmatter_json)?;
                let timeline: Vec<TimelineEntry> = serde_json::from_str(&timeline_json)?;
                Ok(Some(Page {
                    slug,
                    frontmatter,
                    compiled_truth,
                    timeline,
                    content_hash,
                    raw_content,
                }))
            }
            None => Ok(None),
        }
    }

    /// Replace all outgoing links for a page.
    pub async fn update_links(&self, slug: &str, links: &[Link]) -> Result<()> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| Error::Storage(format!("tx begin: {e}")))?;

        sqlx::query("DELETE FROM links WHERE source_slug = ?1")
            .bind(slug)
            .execute(&mut *tx)
            .await
            .map_err(|e| Error::Storage(format!("delete links: {e}")))?;

        for link in links {
            let link_type_str = match &link.link_type {
                LinkType::Plain => "plain",
                LinkType::Custom(s) => s.as_str(),
            };
            sqlx::query(
                "INSERT INTO links (source_slug, target_slug, link_type, context_snippet) VALUES (?1, ?2, ?3, ?4)",
            )
            .bind(slug)
            .bind(&link.target_slug)
            .bind(link_type_str)
            .bind(&link.context_snippet)
            .execute(&mut *tx)
            .await
            .map_err(|e| Error::Storage(format!("insert link: {e}")))?;
        }

        tx.commit()
            .await
            .map_err(|e| Error::Storage(format!("tx commit: {e}")))?;
        Ok(())
    }

    /// Check whether a direct link exists from source to target.
    pub async fn has_link(&self, source: &str, target: &str) -> Result<bool> {
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM links WHERE source_slug = ?1 AND target_slug = ?2",
        )
        .bind(source)
        .bind(target)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| Error::Storage(format!("has_link: {e}")))?;

        Ok(count > 0)
    }

    /// Compute aggregate statistics about the indexed data.
    pub async fn get_stats(&self) -> Result<IndexStats> {
        let total_pages: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM pages")
            .fetch_one(&self.pool)
            .await
            .map_err(|e| Error::Storage(format!("stats pages: {e}")))?;

        let type_rows: Vec<(String, i64)> =
            sqlx::query_as("SELECT page_type, COUNT(*) FROM pages GROUP BY page_type")
                .fetch_all(&self.pool)
                .await
                .map_err(|e| Error::Storage(format!("stats types: {e}")))?;

        let pages_by_type: HashMap<String, i64> = type_rows.into_iter().collect();

        let total_links: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM links")
            .fetch_one(&self.pool)
            .await
            .map_err(|e| Error::Storage(format!("stats links: {e}")))?;

        let orphan_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM pages p WHERE NOT EXISTS (SELECT 1 FROM links l WHERE l.target_slug = p.slug)",
        )
        .fetch_one(&self.pool)
        .await
        .map_err(|e| Error::Storage(format!("stats orphan: {e}")))?;

        Ok(IndexStats {
            total_pages,
            pages_by_type,
            total_links,
            orphan_count,
        })
    }

    /// List every page slug currently in the index.
    pub async fn list_slugs(&self) -> Result<Vec<String>> {
        let rows: Vec<(String,)> = sqlx::query_as("SELECT slug FROM pages")
            .fetch_all(&self.pool)
            .await
            .map_err(|e| Error::Storage(format!("list_slugs: {e}")))?;

        Ok(rows.into_iter().map(|r| r.0).collect())
    }

    /// Run an FTS5 full-text search and return matching slugs.
    pub async fn search_fts(&self, query: &str) -> Result<Vec<String>> {
        let rows: Vec<(String,)> =
            sqlx::query_as("SELECT slug FROM pages_fts WHERE pages_fts MATCH ?1 ORDER BY rank")
                .bind(query)
                .fetch_all(&self.pool)
                .await
                .map_err(|e| Error::Storage(format!("fts search: {e}")))?;

        Ok(rows.into_iter().map(|r| r.0).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph;
    use crate::test_utils::*;
    use crate::types::PageType;

    #[tokio::test]
    async fn test_schema_creation() {
        let engine = IndexEngine::new(":memory:").await.unwrap();

        let tables: Vec<(String,)> =
            sqlx::query_as("SELECT name FROM sqlite_master WHERE type='table'")
                .fetch_all(&engine.pool)
                .await
                .unwrap();

        let table_names: Vec<String> = tables.into_iter().map(|t| t.0).collect();
        assert!(table_names.contains(&"pages".to_string()));
        assert!(table_names.contains(&"links".to_string()));
        assert!(table_names.contains(&"pages_fts".to_string()));
    }

    #[tokio::test]
    async fn test_visibility_and_created_by_columns_exist() {
        let engine = IndexEngine::new(":memory:").await.unwrap();

        let columns: Vec<(String,)> = sqlx::query_as(
            "SELECT name FROM pragma_table_info('pages') WHERE name IN ('visibility', 'created_by')",
        )
        .fetch_all(&engine.pool)
        .await
        .unwrap();

        let col_names: Vec<String> = columns.into_iter().map(|c| c.0).collect();
        assert!(col_names.contains(&"visibility".to_string()));
        assert!(col_names.contains(&"created_by".to_string()));
    }

    #[tokio::test]
    async fn test_schema_migration_idempotent() {
        let engine = IndexEngine::new(":memory:").await.unwrap();
        engine.init().await.unwrap();
        engine.init().await.unwrap();
        engine.init().await.unwrap();

        let columns: Vec<(String,)> = sqlx::query_as(
            "SELECT name FROM pragma_table_info('pages') WHERE name IN ('visibility', 'created_by')",
        )
        .fetch_all(&engine.pool)
        .await
        .unwrap();

        let col_names: Vec<String> = columns.into_iter().map(|c| c.0).collect();
        assert!(col_names.contains(&"visibility".to_string()));
        assert!(col_names.contains(&"created_by".to_string()));
    }

    #[tokio::test]
    async fn test_slug_migration_strips_md_suffix() {
        let engine = IndexEngine::new(":memory:").await.unwrap();

        // Seed pages with .md suffix using raw SQL
        sqlx::query(
            r#"
            INSERT INTO pages (slug, title, page_type, vault, content_hash, compiled_truth, raw_content,
                timeline_json, timeline_text, frontmatter_json, tags_json, created_at, updated_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
            "#,
        )
        .bind("hello.md")
        .bind("Hello")
        .bind("Concept")
        .bind("")
        .bind("hash1")
        .bind("Hello content")
        .bind("# Hello")
        .bind("[]")
        .bind("")
        .bind(r#"{"title":"Hello","page_type":"Concept","tags":[],"related":[],"sources":[],"status":"Budding"}"#)
        .bind("[]")
        .bind("2024-01-01T00:00:00Z")
        .bind("2024-01-01T00:00:00Z")
        .execute(&engine.pool)
        .await
        .unwrap();

        sqlx::query(
            r#"
            INSERT INTO pages (slug, title, page_type, vault, content_hash, compiled_truth, raw_content,
                timeline_json, timeline_text, frontmatter_json, tags_json, created_at, updated_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
            "#,
        )
        .bind("world.md")
        .bind("World")
        .bind("Concept")
        .bind("")
        .bind("hash2")
        .bind("World content")
        .bind("# World")
        .bind("[]")
        .bind("")
        .bind(r#"{"title":"World","page_type":"Concept","tags":[],"related":[],"sources":[],"status":"Budding"}"#)
        .bind("[]")
        .bind("2024-01-01T00:00:00Z")
        .bind("2024-01-01T00:00:00Z")
        .execute(&engine.pool)
        .await
        .unwrap();

        // Seed a page without .md suffix (should be untouched)
        sqlx::query(
            r#"
            INSERT INTO pages (slug, title, page_type, vault, content_hash, compiled_truth, raw_content,
                timeline_json, timeline_text, frontmatter_json, tags_json, created_at, updated_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
            "#,
        )
        .bind("no-suffix")
        .bind("No Suffix")
        .bind("Concept")
        .bind("")
        .bind("hash3")
        .bind("No suffix content")
        .bind("# No Suffix")
        .bind("[]")
        .bind("")
        .bind(r#"{"title":"No Suffix","page_type":"Concept","tags":[],"related":[],"sources":[],"status":"Budding"}"#)
        .bind("[]")
        .bind("2024-01-01T00:00:00Z")
        .bind("2024-01-01T00:00:00Z")
        .execute(&engine.pool)
        .await
        .unwrap();

        // Seed links with .md suffixes
        sqlx::query("INSERT INTO links (source_slug, target_slug, link_type, context_snippet) VALUES (?1, ?2, ?3, ?4)")
            .bind("hello.md")
            .bind("world.md")
            .bind("plain")
            .bind("see also")
            .execute(&engine.pool)
            .await
            .unwrap();

        // Run init again to trigger migration
        engine.init().await.unwrap();

        // Verify pages slugs are stripped
        let slugs: Vec<(String,)> = sqlx::query_as("SELECT slug FROM pages ORDER BY slug")
            .fetch_all(&engine.pool)
            .await
            .unwrap();
        let slug_names: Vec<String> = slugs.into_iter().map(|r| r.0).collect();
        assert_eq!(slug_names, vec!["hello", "no-suffix", "world"]);

        // Verify link slugs are stripped
        let links: Vec<(String, String)> =
            sqlx::query_as("SELECT source_slug, target_slug FROM links")
                .fetch_all(&engine.pool)
                .await
                .unwrap();
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].0, "hello");
        assert_eq!(links[0].1, "world");

        // Verify FTS still works after rebuild
        let fts_results: Vec<(String,)> =
            sqlx::query_as("SELECT slug FROM pages_fts WHERE pages_fts MATCH ?1")
                .bind("Hello")
                .fetch_all(&engine.pool)
                .await
                .unwrap();
        let fts_slugs: Vec<String> = fts_results.into_iter().map(|r| r.0).collect();
        assert!(fts_slugs.contains(&"hello".to_string()));
    }

    #[tokio::test]
    async fn test_slug_migration_idempotent() {
        let engine = IndexEngine::new(":memory:").await.unwrap();

        // Seed a page with .md suffix
        sqlx::query(
            r#"
            INSERT INTO pages (slug, title, page_type, vault, content_hash, compiled_truth, raw_content,
                timeline_json, timeline_text, frontmatter_json, tags_json, created_at, updated_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
            "#,
        )
        .bind("test.md")
        .bind("Test")
        .bind("Concept")
        .bind("")
        .bind("hash1")
        .bind("Test content")
        .bind("# Test")
        .bind("[]")
        .bind("")
        .bind(r#"{"title":"Test","page_type":"Concept","tags":[],"related":[],"sources":[],"status":"Budding"}"#)
        .bind("[]")
        .bind("2024-01-01T00:00:00Z")
        .bind("2024-01-01T00:00:00Z")
        .execute(&engine.pool)
        .await
        .unwrap();

        sqlx::query("INSERT INTO links (source_slug, target_slug, link_type, context_snippet) VALUES (?1, ?2, ?3, ?4)")
            .bind("test.md")
            .bind("test.md")
            .bind("plain")
            .bind("self")
            .execute(&engine.pool)
            .await
            .unwrap();

        // First migration
        engine.init().await.unwrap();

        let slugs_before: Vec<(String,)> = sqlx::query_as("SELECT slug FROM pages")
            .fetch_all(&engine.pool)
            .await
            .unwrap();

        // Second migration (idempotency check)
        engine.init().await.unwrap();

        let slugs_after: Vec<(String,)> = sqlx::query_as("SELECT slug FROM pages")
            .fetch_all(&engine.pool)
            .await
            .unwrap();

        assert_eq!(slugs_before, slugs_after);

        // Verify links are still correct
        let links: Vec<(String, String)> =
            sqlx::query_as("SELECT source_slug, target_slug FROM links")
                .fetch_all(&engine.pool)
                .await
                .unwrap();
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].0, "test");
        assert_eq!(links[0].1, "test");
    }

    #[tokio::test]
    async fn test_page_roundtrip() {
        let engine = IndexEngine::new(":memory:").await.unwrap();
        let page = sample_page("test-page", "Test Page", PageType::Concept, "Truth content");

        engine.index_page(&page).await.unwrap();
        let retrieved = engine.get_page("test-page").await.unwrap();

        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap(), page);
    }

    #[tokio::test]
    async fn test_page_not_found() {
        let engine = IndexEngine::new(":memory:").await.unwrap();
        let result = engine.get_page("nonexistent").await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_remove_page() {
        let engine = IndexEngine::new(":memory:").await.unwrap();
        let page = sample_page("remove-me", "Remove Me", PageType::Entity, "Content");

        engine.index_page(&page).await.unwrap();
        assert!(engine.get_page("remove-me").await.unwrap().is_some());

        engine.remove_page("remove-me").await.unwrap();
        assert!(engine.get_page("remove-me").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_link_operations() {
        let engine = IndexEngine::new(":memory:").await.unwrap();

        let page_a = sample_page("page-a", "Page A", PageType::Entity, "A content");
        let page_b = sample_page("page-b", "Page B", PageType::Concept, "B content");
        let page_c = sample_page("page-c", "Page C", PageType::Source, "C content");

        engine.index_page(&page_a).await.unwrap();
        engine.index_page(&page_b).await.unwrap();
        engine.index_page(&page_c).await.unwrap();

        let links = vec![
            Link {
                source_slug: "page-a".to_string(),
                target_slug: "page-b".to_string(),
                link_type: LinkType::Plain,
                context_snippet: Some("see also".to_string()),
            },
            Link {
                source_slug: "page-a".to_string(),
                target_slug: "page-c".to_string(),
                link_type: LinkType::Custom("cites".to_string()),
                context_snippet: None,
            },
        ];

        engine.update_links("page-a", &links).await.unwrap();

        let outgoing = graph::get_outlinks(engine.pool(), "page-a", None)
            .await
            .unwrap();
        assert_eq!(outgoing.len(), 2);

        let backlinks_b = graph::get_backlinks(engine.pool(), "page-b", None)
            .await
            .unwrap();
        assert_eq!(backlinks_b.len(), 1);
        assert_eq!(backlinks_b[0].source_slug, "page-a");
        assert_eq!(backlinks_b[0].link_type, LinkType::Plain);

        let backlinks_c = graph::get_backlinks(engine.pool(), "page-c", None)
            .await
            .unwrap();
        assert_eq!(backlinks_c.len(), 1);
        assert_eq!(
            backlinks_c[0].link_type,
            LinkType::Custom("cites".to_string())
        );
    }

    #[tokio::test]
    async fn test_has_link() {
        let engine = IndexEngine::new(":memory:").await.unwrap();

        let page_a = sample_page("page-a", "Page A", PageType::Entity, "A");
        let page_b = sample_page("page-b", "Page B", PageType::Concept, "B");
        engine.index_page(&page_a).await.unwrap();
        engine.index_page(&page_b).await.unwrap();

        let links = vec![Link {
            source_slug: "page-a".to_string(),
            target_slug: "page-b".to_string(),
            link_type: LinkType::Plain,
            context_snippet: None,
        }];
        engine.update_links("page-a", &links).await.unwrap();

        assert!(engine.has_link("page-a", "page-b").await.unwrap());
        assert!(!engine.has_link("page-b", "page-a").await.unwrap());
        assert!(!engine.has_link("page-a", "page-c").await.unwrap());
    }

    #[tokio::test]
    async fn test_fts_search() {
        let engine = IndexEngine::new(":memory:").await.unwrap();

        let page1 = sample_page(
            "page-1",
            "Rust Programming",
            PageType::Concept,
            "Rust is a systems language",
        );
        let page2 = sample_page(
            "page-2",
            "Python Guide",
            PageType::Concept,
            "Python is interpreted",
        );

        engine.index_page(&page1).await.unwrap();
        engine.index_page(&page2).await.unwrap();

        let results = engine.search_fts("Rust").await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0], "page-1");

        let results = engine.search_fts("systems").await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0], "page-1");

        let results = engine.search_fts("programming").await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0], "page-1");
    }

    #[tokio::test]
    async fn test_get_stats() {
        let engine = IndexEngine::new(":memory:").await.unwrap();

        let page1 = sample_page("page-1", "Page 1", PageType::Entity, "Content 1");
        let page2 = sample_page("page-2", "Page 2", PageType::Concept, "Content 2");
        let page3 = sample_page("page-3", "Page 3", PageType::Concept, "Content 3");

        engine.index_page(&page1).await.unwrap();
        engine.index_page(&page2).await.unwrap();
        engine.index_page(&page3).await.unwrap();

        let links = vec![
            Link {
                source_slug: "page-1".to_string(),
                target_slug: "page-2".to_string(),
                link_type: LinkType::Plain,
                context_snippet: None,
            },
            Link {
                source_slug: "page-1".to_string(),
                target_slug: "page-3".to_string(),
                link_type: LinkType::Plain,
                context_snippet: None,
            },
        ];
        engine.update_links("page-1", &links).await.unwrap();

        let stats = engine.get_stats().await.unwrap();
        assert_eq!(stats.total_pages, 3);
        assert_eq!(stats.total_links, 2);
        assert_eq!(stats.pages_by_type.get("Entity"), Some(&1));
        assert_eq!(stats.pages_by_type.get("Concept"), Some(&2));
    }

    #[tokio::test]
    async fn test_orphan_count() {
        let engine = IndexEngine::new(":memory:").await.unwrap();

        let page1 = sample_page("page-1", "Page 1", PageType::Entity, "Content 1");
        let page2 = sample_page("page-2", "Page 2", PageType::Concept, "Content 2");
        let page3 = sample_page("page-3", "Page 3", PageType::Entity, "Content 3");

        engine.index_page(&page1).await.unwrap();
        engine.index_page(&page2).await.unwrap();
        engine.index_page(&page3).await.unwrap();

        let links = vec![Link {
            source_slug: "page-1".to_string(),
            target_slug: "page-2".to_string(),
            link_type: LinkType::Plain,
            context_snippet: None,
        }];
        engine.update_links("page-1", &links).await.unwrap();

        let stats = engine.get_stats().await.unwrap();
        assert_eq!(stats.orphan_count, 2);
    }
}
