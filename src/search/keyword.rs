use crate::types::{Error, Result};
use sqlx::SqlitePool;
use tracing::warn;

/// A single result from a full-text search.
#[derive(Debug, Clone, PartialEq)]
pub struct SearchHit {
    pub slug: String,
    pub title: String,
    pub preview: String,
    pub rank: f64,
}

/// Execute a full-text search against the FTS5 index.
pub async fn keyword_search(
    db: &SqlitePool,
    query: &str,
    limit: i64,
    type_filter: Option<&str>,
) -> Result<Vec<SearchHit>> {
    let query = query.trim();
    if query.is_empty() {
        return Ok(Vec::new());
    }

    let sanitized = sanitize_fts5_query(query);
    if sanitized.is_empty() {
        return Ok(Vec::new());
    }

    let limit = limit.clamp(0, 100);

    let results = if let Some(page_type) = type_filter {
        sqlx::query_as::<_, SearchHitRow>(
            r#"
            SELECT p.slug, p.title, p.compiled_truth,
                   bm25(pages_fts, 10, 5, 1, 1) as rank
            FROM pages_fts
            JOIN pages p ON p.rowid = pages_fts.rowid
            WHERE pages_fts MATCH ?1 AND p.page_type = ?2
            ORDER BY rank
            LIMIT ?3
            "#,
        )
        .bind(&sanitized)
        .bind(page_type)
        .bind(limit)
        .fetch_all(db)
        .await
    } else {
        sqlx::query_as::<_, SearchHitRow>(
            r#"
            SELECT p.slug, p.title, p.compiled_truth,
                   bm25(pages_fts, 10, 5, 1, 1) as rank
            FROM pages_fts
            JOIN pages p ON p.rowid = pages_fts.rowid
            WHERE pages_fts MATCH ?1
            ORDER BY rank
            LIMIT ?2
            "#,
        )
        .bind(&sanitized)
        .bind(limit)
        .fetch_all(db)
        .await
    };

    match results {
        Ok(rows) => Ok(rows
            .into_iter()
            .map(|row| SearchHit {
                slug: row.slug,
                title: row.title,
                preview: row.compiled_truth.chars().take(200).collect(),
                rank: row.rank,
            })
            .collect()),
        Err(e) => {
            let err_str = e.to_string();
            if err_str.contains("fts5") || err_str.contains("malformed") || err_str.contains("MATCH") {
                warn!("Invalid FTS5 query '{}': {}", sanitized, e);
                Ok(Vec::new())
            } else {
                Err(Error::Storage(format!("keyword_search: {e}")))
            }
        }
    }
}

fn sanitize_fts5_query(query: &str) -> String {
    let result = query.replace('\0', "");
    let result = result.trim();
    if result.is_empty() {
        return String::new();
    }

    // Only quote when FTS5 would misparse the query as an operator.
    // Hyphens get parsed as NOT ("test-foo" → "test NOT foo"), so they need quoting.
    //
    // CJK characters must NOT be quoted. With the trigram tokenizer:
    //   - Quoted "测试记录" → exact phrase match (only matches that exact sequence)
    //   - Unquoted 测试记录  → substring match   (matches any text containing those chars)
    // CJK substring search requires bare (unquoted) terms so the trigram tokenizer
    // can match partial substrings rather than enforcing exact-phrase ordering.
    let needs_quoting = result.contains('-');

    if needs_quoting {
        let inner = result.replace('"', "\"\"");
        format!("\"{inner}\"")
    } else {
        // Strip quotes from input — FTS5 bare-term syntax for regular queries.
        // This preserves AND/OR/NOT operators for power users.
        result.replace('"', "")
    }
}

#[derive(sqlx::FromRow)]
struct SearchHitRow {
    slug: String,
    title: String,
    compiled_truth: String,
    rank: f64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Page, PageType};
    use sqlx::sqlite::SqlitePoolOptions;

    async fn setup_test_db() -> SqlitePool {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect(":memory:")
            .await
            .unwrap();

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
            .execute(&pool)
            .await
            .unwrap();

        pool
    }

    use crate::test_utils::*;

    async fn index_page(pool: &SqlitePool, page: &Page) {
        let now = "2024-01-01T00:00:00Z";
        let timeline_json = serde_json::to_string(&page.timeline).unwrap();
        let timeline_text = page
            .timeline
            .iter()
            .map(|e| e.content.as_str())
            .collect::<Vec<_>>()
            .join(" ");
        let frontmatter_json = serde_json::to_string(&page.frontmatter).unwrap();
        let tags_json = serde_json::to_string(&page.frontmatter.tags).unwrap();

        let page_type_str = page.frontmatter.page_type.to_string();

        sqlx::query(
            r#"
            INSERT INTO pages (
                slug, title, page_type, vault, content_hash, compiled_truth, raw_content,
                timeline_json, timeline_text, frontmatter_json, tags_json, created_at, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
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
        .bind(now)
        .bind(now)
        .execute(pool)
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn test_find_by_title() {
        let pool = setup_test_db().await;
        let page = sample_page(
            "rust-page",
            "Rust Programming Language",
            PageType::Concept,
            "Rust is great for systems programming.",
        );
        index_page(&pool, &page).await;

        let results = keyword_search(&pool, "Programming", 10, None).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].slug, "rust-page");
        assert_eq!(results[0].title, "Rust Programming Language");
    }

    #[tokio::test]
    async fn test_find_by_body() {
        let pool = setup_test_db().await;
        let page = sample_page(
            "rust-page",
            "Rust",
            PageType::Concept,
            "Rust is great for systems programming.",
        );
        index_page(&pool, &page).await;

        let results = keyword_search(&pool, "systems", 10, None).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].slug, "rust-page");
    }

    #[tokio::test]
    async fn test_type_filter() {
        let pool = setup_test_db().await;
        let page1 = sample_page(
            "entity-page",
            "Entity Page",
            PageType::Entity,
            "This is an entity page.",
        );
        let page2 = sample_page(
            "concept-page",
            "Concept Page",
            PageType::Concept,
            "This is a concept page.",
        );
        index_page(&pool, &page1).await;
        index_page(&pool, &page2).await;

        let results = keyword_search(&pool, "page", 10, Some("Entity")).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].slug, "entity-page");
    }

    #[tokio::test]
    async fn test_limit() {
        let pool = setup_test_db().await;
        for i in 0..5 {
            let page = sample_page(
                &format!("page-{i}"),
                &format!("Page {i}"),
                PageType::Concept,
                "Common content word.",
            );
            index_page(&pool, &page).await;
        }

        let results = keyword_search(&pool, "Common", 2, None).await.unwrap();
        assert_eq!(results.len(), 2);
    }

    #[tokio::test]
    async fn test_empty_query() {
        let pool = setup_test_db().await;
        let page = sample_page("test", "Test", PageType::Concept, "Content");
        index_page(&pool, &page).await;

        let results = keyword_search(&pool, "", 10, None).await.unwrap();
        assert!(results.is_empty());

        let results = keyword_search(&pool, "   ", 10, None).await.unwrap();
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn test_no_match() {
        let pool = setup_test_db().await;
        let page = sample_page("test", "Test", PageType::Concept, "Content");
        index_page(&pool, &page).await;

        let results = keyword_search(&pool, "nonexistent", 10, None).await.unwrap();
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn test_cjk_search() {
        let pool = setup_test_db().await;
        let page = sample_page(
            "test-page",
            "测试页面",
            PageType::Concept,
            "这是一条测试记录，用于验证 stele MCP 接口的完整读写链路。",
        );
        index_page(&pool, &page).await;

        // "用于验证" is an exact substring of compiled_truth
        let results = keyword_search(&pool, "用于验证", 10, None).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].slug, "test-page");

        // "接口的" is also an exact substring
        let results = keyword_search(&pool, "接口的", 10, None).await.unwrap();
        assert_eq!(results.len(), 1);
    }

    #[tokio::test]
    async fn test_hyphen_in_query() {
        let pool = setup_test_db().await;
        let page = sample_page(
            "test-stele-verify",
            "Test Stele Verify",
            PageType::Concept,
            "Content about test-stele-verify integration.",
        );
        index_page(&pool, &page).await;

        let results = keyword_search(&pool, "test-stele-verify", 10, None)
            .await
            .unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].slug, "test-stele-verify");
    }

    #[tokio::test]
    async fn test_fts5_and() {
        let pool = setup_test_db().await;
        let page1 = sample_page(
            "page1",
            "Page One",
            PageType::Concept,
            "Rust and Python are programming languages.",
        );
        let page2 = sample_page(
            "page2",
            "Page Two",
            PageType::Concept,
            "Rust is a systems language.",
        );
        index_page(&pool, &page1).await;
        index_page(&pool, &page2).await;

        let results = keyword_search(&pool, "Rust AND Python", 10, None)
            .await
            .unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].slug, "page1");
    }

    #[tokio::test]
    async fn test_fts5_multi_word_and() {
        let pool = setup_test_db().await;
        let page1 = sample_page(
            "page1",
            "Page One",
            PageType::Concept,
            "The quick brown fox jumps.",
        );
        let page2 = sample_page(
            "page2",
            "Page Two",
            PageType::Concept,
            "The quick fox is brown.",
        );
        index_page(&pool, &page1).await;
        index_page(&pool, &page2).await;

        // Multi-word query: AND semantics — both pages contain "quick" and "brown"
        let results = keyword_search(&pool, "quick brown", 10, None)
            .await
            .unwrap();
        assert_eq!(results.len(), 2);
    }

    #[tokio::test]
    async fn test_title_ranked_higher() {
        let pool = setup_test_db().await;
        let page_body = sample_page(
            "body-match",
            "Some Page",
            PageType::Concept,
            "The keyword appears only in the body content here.",
        );
        let page_title = sample_page(
            "title-match",
            "The keyword Page",
            PageType::Concept,
            "Some other content.",
        );
        index_page(&pool, &page_body).await;
        index_page(&pool, &page_title).await;

        let results = keyword_search(&pool, "keyword", 10, None).await.unwrap();
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].slug, "title-match");
        assert_eq!(results[1].slug, "body-match");
    }

    #[tokio::test]
    async fn test_preview_truncation() {
        let pool = setup_test_db().await;
        let long_content = "word ".repeat(100);
        let page = sample_page("long", "Long Page", PageType::Concept, &long_content.trim());
        index_page(&pool, &page).await;

        let results = keyword_search(&pool, "word", 10, None).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].preview.len(), 200);
    }

    #[tokio::test]
    async fn test_limit_capped_at_100() {
        let pool = setup_test_db().await;
        for i in 0..5 {
            let page = sample_page(
                &format!("page-{i}"),
                &format!("Page {i}"),
                PageType::Concept,
                "Common content word.",
            );
            index_page(&pool, &page).await;
        }

        let results = keyword_search(&pool, "Common", 200, None).await.unwrap();
        assert_eq!(results.len(), 5);
    }

    #[test]
    fn test_sanitize_cjk_not_quoted() {
        assert_eq!(sanitize_fts5_query("测试记录"), "测试记录");
        assert_eq!(sanitize_fts5_query("接口验证"), "接口验证");
        assert_eq!(sanitize_fts5_query("世界"), "世界");
    }

    #[test]
    fn test_sanitize_hyphen_quoted() {
        assert_eq!(sanitize_fts5_query("test-stele-verify"), "\"test-stele-verify\"");
    }

    #[test]
    fn test_sanitize_english_operators_preserved() {
        assert_eq!(sanitize_fts5_query("Rust AND Python"), "Rust AND Python");
        assert_eq!(sanitize_fts5_query("Rust OR Python"), "Rust OR Python");
        assert_eq!(sanitize_fts5_query("NOT deprecated"), "NOT deprecated");
    }

    #[tokio::test]
    async fn test_cjk_substring_search() {
        let pool = setup_test_db().await;
        let page = sample_page(
            "cjk-page",
            "全量验证",
            PageType::Concept,
            "这是全量验证的测试记录",
        );
        index_page(&pool, &page).await;

        let results = keyword_search(&pool, "测试记录", 10, None).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].slug, "cjk-page");
    }

    #[tokio::test]
    async fn test_cjk_exact_match() {
        let pool = setup_test_db().await;
        let page = sample_page(
            "cjk-exact",
            "接口页面",
            PageType::Concept,
            "接口验证是必要的步骤",
        );
        index_page(&pool, &page).await;

        let results = keyword_search(&pool, "接口验证", 10, None).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].slug, "cjk-exact");
    }

    #[tokio::test]
    async fn test_mixed_cjk_english_search() {
        let pool = setup_test_db().await;
        let page = sample_page(
            "mixed-page",
            "Mixed Content",
            PageType::Concept,
            "hello世界欢迎你",
        );
        index_page(&pool, &page).await;

        let results = keyword_search(&pool, "世界欢", 10, None).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].slug, "mixed-page");
    }

    #[tokio::test]
    async fn test_cjk_full_content_match() {
        let pool = setup_test_db().await;
        let page = sample_page(
            "full-cjk",
            "Full CJK",
            PageType::Concept,
            "verification of the system is complete",
        );
        index_page(&pool, &page).await;

        let results = keyword_search(&pool, "verification", 10, None).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].slug, "full-cjk");
    }

    #[tokio::test]
    async fn test_fts5_or() {
        let pool = setup_test_db().await;
        let page1 = sample_page("p1", "Page One", PageType::Concept, "Rust is fast.");
        let page2 = sample_page("p2", "Page Two", PageType::Concept, "Python is flexible.");
        index_page(&pool, &page1).await;
        index_page(&pool, &page2).await;

        let results = keyword_search(&pool, "Rust OR Python", 10, None).await.unwrap();
        assert_eq!(results.len(), 2);
    }
}
