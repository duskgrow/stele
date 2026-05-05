use crate::storage::sqlite::{SearchHit, SqliteBackend};

/// Perform a keyword search using SQLite FTS5 MATCH syntax.
///
/// Supports FTS5 query syntax: AND, OR, NOT, quoted phrases.
///
/// # Arguments
/// * `db` - The SQLite backend to query
/// * `query` - The FTS5 search query string
/// * `limit` - Maximum number of results to return
/// * `type_filter` - Optional page type filter (e.g., "entity", "concept")
pub async fn keyword_search(
    db: &SqliteBackend,
    query: &str,
    limit: usize,
    type_filter: Option<&str>,
) -> Result<Vec<SearchHit>, anyhow::Error> {
    db.search_keyword(query, limit, type_filter).await
}
