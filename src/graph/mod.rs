use crate::types::{Error, Link, LinkType, Result};
use sqlx::SqlitePool;
use std::collections::{HashSet, VecDeque};

pub(crate) enum LinkDirection {
    In,
    Out,
}

pub(crate) async fn query_links(
    db: &SqlitePool,
    slug: &str,
    direction: LinkDirection,
    link_type: Option<&str>,
) -> Result<Vec<Link>> {
    let (where_clause, err_label) = match direction {
        LinkDirection::Out => ("source_slug = ?1", "get_outlinks"),
        LinkDirection::In => ("target_slug = ?1", "get_backlinks"),
    };

    let rows: Vec<(String, String, String, Option<String>)> = match link_type {
        Some(lt) => {
            let sql = format!(
                "SELECT source_slug, target_slug, link_type, context_snippet FROM links WHERE {} AND link_type = ?2",
                where_clause
            );
            sqlx::query_as(&sql)
                .bind(slug)
                .bind(lt)
                .fetch_all(db)
                .await
                .map_err(|e| Error::Storage(format!("{err_label}: {e}")))?
        }
        None => {
            let sql = format!(
                "SELECT source_slug, target_slug, link_type, context_snippet FROM links WHERE {}",
                where_clause
            );
            sqlx::query_as(&sql)
                .bind(slug)
                .fetch_all(db)
                .await
                .map_err(|e| Error::Storage(format!("{err_label}: {e}")))?
        }
    };

    Ok(rows.into_iter().map(parse_link_row).collect())
}

/// Get all outgoing links from a page, optionally filtered by link type.
pub async fn get_outlinks(
    db: &SqlitePool,
    slug: &str,
    link_type: Option<&str>,
) -> Result<Vec<Link>> {
    query_links(db, slug, LinkDirection::Out, link_type).await
}

/// Get all incoming links to a page, optionally filtered by link type.
pub async fn get_backlinks(
    db: &SqlitePool,
    slug: &str,
    link_type: Option<&str>,
) -> Result<Vec<Link>> {
    query_links(db, slug, LinkDirection::In, link_type).await
}

/// BFS traversal from a starting page, returning (slug, distance) pairs.
/// The start slug itself is not included in results.
/// Cycle detection prevents infinite loops.
///
/// `direction` controls which edges to follow:
/// - `"out"` (default): follow outgoing links
/// - `"in"`: follow incoming links
/// - `"both"`: follow both directions
///
/// `link_type` filters to only edges of the given type.
pub async fn get_neighbors(
    db: &SqlitePool,
    slug: &str,
    depth: usize,
    link_type: Option<&str>,
    direction: Option<&str>,
) -> Result<Vec<(String, usize)>> {
    let dir = direction.unwrap_or("out");

    let mut visited: HashSet<String> = HashSet::new();
    let mut queue: VecDeque<(String, usize)> = VecDeque::new();
    let mut results: Vec<(String, usize)> = Vec::new();

    queue.push_back((slug.to_string(), 0));
    visited.insert(slug.to_string());

    while let Some((current, current_depth)) = queue.pop_front() {
        if current_depth > 0 {
            results.push((current.clone(), current_depth));
        }

        if current_depth >= depth {
            continue;
        }

        match dir {
            "in" => {
                let links = get_backlinks(db, &current, link_type).await?;
                for link in links {
                    if !visited.contains(&link.source_slug) {
                        visited.insert(link.source_slug.clone());
                        queue.push_back((link.source_slug, current_depth + 1));
                    }
                }
            }
            "both" => {
                let outlinks = get_outlinks(db, &current, link_type).await?;
                for link in outlinks {
                    if !visited.contains(&link.target_slug) {
                        visited.insert(link.target_slug.clone());
                        queue.push_back((link.target_slug, current_depth + 1));
                    }
                }
                let backlinks = get_backlinks(db, &current, link_type).await?;
                for link in backlinks {
                    if !visited.contains(&link.source_slug) {
                        visited.insert(link.source_slug.clone());
                        queue.push_back((link.source_slug, current_depth + 1));
                    }
                }
            }
            _ => {
                let links = get_outlinks(db, &current, link_type).await?;
                for link in links {
                    if !visited.contains(&link.target_slug) {
                        visited.insert(link.target_slug.clone());
                        queue.push_back((link.target_slug, current_depth + 1));
                    }
                }
            }
        }
    }

    Ok(results)
}

/// Find all pages with zero inbound links.
pub async fn find_orphans(db: &SqlitePool) -> Result<Vec<String>> {
    let rows: Vec<(String,)> = sqlx::query_as(
        "SELECT slug FROM pages p WHERE NOT EXISTS (SELECT 1 FROM links l WHERE l.target_slug = p.slug)",
    )
    .fetch_all(db)
    .await
    .map_err(|e| Error::Storage(format!("find_orphans: {e}")))?;

    Ok(rows.into_iter().map(|r| r.0).collect())
}

/// Get all unique link types present in the graph.
pub async fn get_link_types(db: &SqlitePool) -> Result<Vec<String>> {
    let rows: Vec<(String,)> =
        sqlx::query_as("SELECT DISTINCT link_type FROM links ORDER BY link_type")
            .fetch_all(db)
            .await
            .map_err(|e| Error::Storage(format!("get_link_types: {e}")))?;

    Ok(rows.into_iter().map(|r| r.0).collect())
}

/// Find pages that both slug_a and slug_b link to.
pub async fn shared_links(db: &SqlitePool, slug_a: &str, slug_b: &str) -> Result<Vec<String>> {
    let rows: Vec<(String,)> = sqlx::query_as(
        r#"
        SELECT target_slug FROM links WHERE source_slug = ?1
        INTERSECT
        SELECT target_slug FROM links WHERE source_slug = ?2
        ORDER BY target_slug
        "#,
    )
    .bind(slug_a)
    .bind(slug_b)
    .fetch_all(db)
    .await
    .map_err(|e| Error::Storage(format!("shared_links: {e}")))?;

    Ok(rows.into_iter().map(|r| r.0).collect())
}

pub(crate) fn parse_link_row(row: (String, String, String, Option<String>)) -> Link {
    let (source_slug, target_slug, link_type_str, context_snippet) = row;
    Link {
        source_slug,
        target_slug,
        link_type: if link_type_str == "plain" {
            LinkType::Plain
        } else {
            LinkType::Custom(link_type_str)
        },
        context_snippet,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::TEST_SCHEMA;
    use sqlx::sqlite::SqlitePoolOptions;

    async fn setup_test_db() -> SqlitePool {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect(":memory:")
            .await
            .unwrap();

        sqlx::raw_sql(TEST_SCHEMA).execute(&pool).await.unwrap();

        pool
    }

    async fn insert_page(pool: &SqlitePool, slug: &str) {
        sqlx::query("INSERT INTO pages (slug, title) VALUES (?1, ?2)")
            .bind(slug)
            .bind(format!("Title for {}", slug))
            .execute(pool)
            .await
            .unwrap();
    }

    async fn insert_link(pool: &SqlitePool, source: &str, target: &str, link_type: &str) {
        sqlx::query("INSERT INTO links (source_slug, target_slug, link_type) VALUES (?1, ?2, ?3)")
            .bind(source)
            .bind(target)
            .bind(link_type)
            .execute(pool)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn test_outlinks() {
        let pool = setup_test_db().await;
        insert_page(&pool, "page-a").await;
        insert_page(&pool, "page-b").await;
        insert_page(&pool, "page-c").await;
        insert_link(&pool, "page-a", "page-b", "plain").await;
        insert_link(&pool, "page-a", "page-c", "cites").await;

        let outlinks = get_outlinks(&pool, "page-a", None).await.unwrap();
        assert_eq!(outlinks.len(), 2);
        assert!(
            outlinks
                .iter()
                .any(|l| l.target_slug == "page-b" && l.link_type == LinkType::Plain)
        );
        assert!(
            outlinks.iter().any(|l| l.target_slug == "page-c"
                && l.link_type == LinkType::Custom("cites".to_string()))
        );
    }

    #[tokio::test]
    async fn test_backlinks() {
        let pool = setup_test_db().await;
        insert_page(&pool, "page-a").await;
        insert_page(&pool, "page-b").await;
        insert_page(&pool, "page-c").await;
        insert_link(&pool, "page-a", "page-b", "plain").await;
        insert_link(&pool, "page-c", "page-b", "references").await;

        let backlinks = get_backlinks(&pool, "page-b", None).await.unwrap();
        assert_eq!(backlinks.len(), 2);
        assert!(
            backlinks
                .iter()
                .any(|l| l.source_slug == "page-a" && l.link_type == LinkType::Plain)
        );
        assert!(backlinks.iter().any(|l| l.source_slug == "page-c"
            && l.link_type == LinkType::Custom("references".to_string())));
    }

    #[tokio::test]
    async fn test_bfs_with_cycles() {
        let pool = setup_test_db().await;
        insert_page(&pool, "a").await;
        insert_page(&pool, "b").await;
        insert_page(&pool, "c").await;
        insert_link(&pool, "a", "b", "plain").await;
        insert_link(&pool, "b", "c", "plain").await;
        insert_link(&pool, "c", "a", "plain").await;

        let neighbors = get_neighbors(&pool, "a", 5, None, None).await.unwrap();
        assert_eq!(neighbors.len(), 2);
        let slugs: Vec<String> = neighbors.iter().map(|(s, _)| s.clone()).collect();
        assert!(slugs.contains(&"b".to_string()));
        assert!(slugs.contains(&"c".to_string()));
    }

    #[tokio::test]
    async fn test_bfs_depth_limit() {
        let pool = setup_test_db().await;
        insert_page(&pool, "a").await;
        insert_page(&pool, "b").await;
        insert_page(&pool, "c").await;
        insert_link(&pool, "a", "b", "plain").await;
        insert_link(&pool, "b", "c", "plain").await;

        let neighbors = get_neighbors(&pool, "a", 1, None, None).await.unwrap();
        assert_eq!(neighbors.len(), 1);
        assert_eq!(neighbors[0].0, "b");
        assert_eq!(neighbors[0].1, 1);
    }

    #[tokio::test]
    async fn test_find_orphans() {
        let pool = setup_test_db().await;
        insert_page(&pool, "page-a").await;
        insert_page(&pool, "page-b").await;
        insert_page(&pool, "page-c").await;
        insert_link(&pool, "page-a", "page-b", "plain").await;

        let orphans = find_orphans(&pool).await.unwrap();
        assert_eq!(orphans.len(), 2);
        assert!(orphans.contains(&"page-a".to_string()));
        assert!(orphans.contains(&"page-c".to_string()));
    }

    #[tokio::test]
    async fn test_get_link_types() {
        let pool = setup_test_db().await;
        insert_page(&pool, "page-a").await;
        insert_page(&pool, "page-b").await;
        insert_link(&pool, "page-a", "page-b", "plain").await;
        insert_link(&pool, "page-b", "page-a", "cites").await;
        insert_link(&pool, "page-a", "page-b", "references").await;

        let types = get_link_types(&pool).await.unwrap();
        assert_eq!(types.len(), 3);
        assert!(types.contains(&"plain".to_string()));
        assert!(types.contains(&"cites".to_string()));
        assert!(types.contains(&"references".to_string()));
    }

    #[tokio::test]
    async fn test_shared_links() {
        let pool = setup_test_db().await;
        insert_page(&pool, "a").await;
        insert_page(&pool, "b").await;
        insert_page(&pool, "c").await;
        insert_page(&pool, "d").await;
        insert_link(&pool, "a", "c", "plain").await;
        insert_link(&pool, "a", "d", "plain").await;
        insert_link(&pool, "b", "c", "plain").await;

        let shared = shared_links(&pool, "a", "b").await.unwrap();
        assert_eq!(shared.len(), 1);
        assert_eq!(shared[0], "c");
    }

    #[tokio::test]
    async fn test_empty_graph() {
        let pool = setup_test_db().await;

        let outlinks = get_outlinks(&pool, "nonexistent", None).await.unwrap();
        assert!(outlinks.is_empty());

        let backlinks = get_backlinks(&pool, "nonexistent", None).await.unwrap();
        assert!(backlinks.is_empty());

        let neighbors = get_neighbors(&pool, "nonexistent", 3, None, None)
            .await
            .unwrap();
        assert!(neighbors.is_empty());

        let orphans = find_orphans(&pool).await.unwrap();
        assert!(orphans.is_empty());

        let types = get_link_types(&pool).await.unwrap();
        assert!(types.is_empty());

        let shared = shared_links(&pool, "a", "b").await.unwrap();
        assert!(shared.is_empty());
    }

    #[tokio::test]
    async fn test_no_links() {
        let pool = setup_test_db().await;
        insert_page(&pool, "page-a").await;

        let outlinks = get_outlinks(&pool, "page-a", None).await.unwrap();
        assert!(outlinks.is_empty());
    }

    #[tokio::test]
    async fn test_outlinks_link_type_filter() {
        let pool = setup_test_db().await;
        insert_page(&pool, "page-a").await;
        insert_page(&pool, "page-b").await;
        insert_page(&pool, "page-c").await;
        insert_link(&pool, "page-a", "page-b", "plain").await;
        insert_link(&pool, "page-a", "page-c", "cites").await;

        let plain = get_outlinks(&pool, "page-a", Some("plain")).await.unwrap();
        assert_eq!(plain.len(), 1);
        assert_eq!(plain[0].target_slug, "page-b");

        let cites = get_outlinks(&pool, "page-a", Some("cites")).await.unwrap();
        assert_eq!(cites.len(), 1);
        assert_eq!(cites[0].target_slug, "page-c");

        let none = get_outlinks(&pool, "page-a", Some("missing"))
            .await
            .unwrap();
        assert!(none.is_empty());
    }

    #[tokio::test]
    async fn test_backlinks_link_type_filter() {
        let pool = setup_test_db().await;
        insert_page(&pool, "page-a").await;
        insert_page(&pool, "page-b").await;
        insert_page(&pool, "page-c").await;
        insert_link(&pool, "page-a", "page-b", "plain").await;
        insert_link(&pool, "page-c", "page-b", "references").await;

        let plain = get_backlinks(&pool, "page-b", Some("plain")).await.unwrap();
        assert_eq!(plain.len(), 1);
        assert_eq!(plain[0].source_slug, "page-a");

        let refs = get_backlinks(&pool, "page-b", Some("references"))
            .await
            .unwrap();
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].source_slug, "page-c");
    }

    #[tokio::test]
    async fn test_neighbors_direction_in() {
        let pool = setup_test_db().await;
        insert_page(&pool, "a").await;
        insert_page(&pool, "b").await;
        insert_page(&pool, "c").await;
        insert_link(&pool, "b", "a", "plain").await;
        insert_link(&pool, "c", "a", "plain").await;

        let neighbors = get_neighbors(&pool, "a", 1, None, Some("in"))
            .await
            .unwrap();
        assert_eq!(neighbors.len(), 2);
        let slugs: Vec<String> = neighbors.iter().map(|(s, _)| s.clone()).collect();
        assert!(slugs.contains(&"b".to_string()));
        assert!(slugs.contains(&"c".to_string()));
    }

    #[tokio::test]
    async fn test_neighbors_direction_both() {
        let pool = setup_test_db().await;
        insert_page(&pool, "a").await;
        insert_page(&pool, "b").await;
        insert_page(&pool, "c").await;
        insert_link(&pool, "a", "b", "plain").await;
        insert_link(&pool, "c", "a", "plain").await;

        let neighbors = get_neighbors(&pool, "a", 1, None, Some("both"))
            .await
            .unwrap();
        assert_eq!(neighbors.len(), 2);
        let slugs: Vec<String> = neighbors.iter().map(|(s, _)| s.clone()).collect();
        assert!(slugs.contains(&"b".to_string()));
        assert!(slugs.contains(&"c".to_string()));
    }

    #[tokio::test]
    async fn test_neighbors_link_type_filter() {
        let pool = setup_test_db().await;
        insert_page(&pool, "a").await;
        insert_page(&pool, "b").await;
        insert_page(&pool, "c").await;
        insert_link(&pool, "a", "b", "plain").await;
        insert_link(&pool, "a", "c", "cites").await;

        let plain = get_neighbors(&pool, "a", 1, Some("plain"), None)
            .await
            .unwrap();
        assert_eq!(plain.len(), 1);
        assert_eq!(plain[0].0, "b");

        let cites = get_neighbors(&pool, "a", 1, Some("cites"), None)
            .await
            .unwrap();
        assert_eq!(cites.len(), 1);
        assert_eq!(cites[0].0, "c");
    }

    #[tokio::test]
    async fn test_neighbors_direction_in_depth_two() {
        let pool = setup_test_db().await;
        insert_page(&pool, "a").await;
        insert_page(&pool, "b").await;
        insert_page(&pool, "c").await;
        insert_link(&pool, "b", "a", "plain").await;
        insert_link(&pool, "c", "b", "plain").await;

        let neighbors = get_neighbors(&pool, "a", 2, None, Some("in"))
            .await
            .unwrap();
        assert_eq!(neighbors.len(), 2);
        let by_dist: std::collections::HashMap<String, usize> = neighbors.into_iter().collect();
        assert_eq!(by_dist.get("b"), Some(&1));
        assert_eq!(by_dist.get("c"), Some(&2));
    }
}
