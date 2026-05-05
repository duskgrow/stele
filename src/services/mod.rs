pub mod enrich;
pub mod ingest;
pub mod maintain;
pub mod sync;
pub mod tools;

use crate::storage::sqlite::SqliteBackend;
use serde_json::{Value, json};

pub async fn brain_stats(backend: &SqliteBackend) -> anyhow::Result<Value> {
    let stats = backend.get_stats().await?;
    Ok(json!({
        "total_pages": stats.total_pages,
        "by_type": stats.by_type,
        "total_links": stats.total_links,
        "orphan_pages": stats.orphan_pages,
        "db_size_mb": stats.db_size_mb,
        "last_sync": stats.last_sync,
    }))
}
