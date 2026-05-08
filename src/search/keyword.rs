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
    let mut result = query.replace('\0', "");
    result = result.trim().to_string();

    let mut balanced = String::new();
    let mut in_quote = false;
    for ch in result.chars() {
        if ch == '"' {
            if in_quote {
                balanced.push('"');
                in_quote = false;
            } else {
                balanced.push('"');
                in_quote = true;
            }
        } else {
            balanced.push(ch);
        }
    }
    if in_quote {
        if let Some(pos) = balanced.rfind('"') {
            balanced.remove(pos);
        }
    }

    balanced.trim().to_string()
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
                content_rowid=rowid
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
    async fn test_fts5_or() {
        let pool = setup_test_db().await;
        let page1 = sample_page(
            "page1",
            "Page One",
            PageType::Concept,
            "Rust is a systems language.",
        );
        let page2 = sample_page(
            "page2",
            "Page Two",
            PageType::Concept,
            "Python is interpreted.",
        );
        index_page(&pool, &page1).await;
        index_page(&pool, &page2).await;

        let results = keyword_search(&pool, "Rust OR Python", 10, None)
            .await
            .unwrap();
        assert_eq!(results.len(), 2);
        let slugs: Vec<String> = results.iter().map(|r| r.slug.clone()).collect();
        assert!(slugs.contains(&"page1".to_string()));
        assert!(slugs.contains(&"page2".to_string()));
    }

    #[tokio::test]
    async fn test_fts5_not() {
        let pool = setup_test_db().await;
        let page1 = sample_page(
            "page1",
            "Page One",
            PageType::Concept,
            "Rust and Python are languages.",
        );
        let page2 = sample_page(
            "page2",
            "Page Two",
            PageType::Concept,
            "Rust is a systems language.",
        );
        index_page(&pool, &page1).await;
        index_page(&pool, &page2).await;

        let results = keyword_search(&pool, "Rust NOT Python", 10, None)
            .await
            .unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].slug, "page2");
    }

    #[tokio::test]
    async fn test_fts5_phrase() {
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

        let results = keyword_search(&pool, "\"quick brown\"", 10, None)
            .await
            .unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].slug, "page1");
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
}
