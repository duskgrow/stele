use anyhow::Result;
use sqlx::Row;
use sqlx::sqlite::SqlitePool;
use tracing::{info, warn};

#[derive(Debug, Clone)]
pub struct VectorHit {
    pub slug: String,
    pub distance: f64,
    pub rank: f64,
}

pub struct VectorSearch {
    enabled: bool,
    pool: SqlitePool,
}

impl VectorSearch {
    pub async fn new(pool: &SqlitePool) -> Self {
        let enabled = match Self::try_enable_vec(pool).await {
            Ok(()) => {
                info!("sqlite-vec extension loaded — vector search enabled");
                true
            }
            Err(e) => {
                warn!("sqlite-vec unavailable ({e}), vector search disabled — FTS5-only mode");
                false
            }
        };

        Self {
            enabled,
            pool: pool.clone(),
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    pub async fn store_embedding(&self, slug: &str, embedding: &[f32]) -> Result<()> {
        if !self.enabled {
            return Ok(());
        }

        let blob = f32_slice_to_le_bytes(embedding);

        sqlx::query("INSERT OR REPLACE INTO page_embeddings (slug, embedding) VALUES (?1, ?2)")
            .bind(slug)
            .bind(blob.as_slice())
            .execute(&self.pool)
            .await
            .map(|_| ())
            .map_err(|e| anyhow::anyhow!("storing embedding for {slug}: {e}"))
    }

    pub async fn remove_embedding(&self, slug: &str) -> Result<bool> {
        if !self.enabled {
            return Ok(false);
        }

        let result = sqlx::query("DELETE FROM page_embeddings WHERE slug = ?1")
            .bind(slug)
            .execute(&self.pool)
            .await
            .map_err(|e| anyhow::anyhow!("removing embedding for {slug}: {e}"))?;

        Ok(result.rows_affected() > 0)
    }

    pub async fn vector_search(&self, query_embedding: &[f32], limit: usize) -> Vec<VectorHit> {
        if !self.enabled {
            return Vec::new();
        }

        let blob = f32_slice_to_le_bytes(query_embedding);

        let rows = match sqlx::query(
            "SELECT slug, distance \
             FROM page_embeddings \
             WHERE embedding MATCH ?1 \
             ORDER BY distance \
             LIMIT ?2",
        )
        .bind(blob.as_slice())
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await
        {
            Ok(r) => r,
            Err(e) => {
                warn!("vector search query failed: {e}");
                return Vec::new();
            }
        };

        rows.into_iter()
            .enumerate()
            .map(|(i, row)| VectorHit {
                slug: row.get("slug"),
                distance: row.get("distance"),
                rank: 1.0 / (60.0 + i as f64),
            })
            .collect()
    }

    pub async fn embedding_count(&self) -> usize {
        if !self.enabled {
            return 0;
        }

        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM page_embeddings")
            .fetch_one(&self.pool)
            .await
            .unwrap_or(0) as usize
    }

    async fn try_enable_vec(pool: &SqlitePool) -> Result<()> {
        let candidates = ["vec0", "vec0.so", "sqlite_vec"];
        let mut loaded = false;

        for name in &candidates {
            if sqlx::query(&format!("SELECT load_extension('{name}')"))
                .execute(pool)
                .await
                .is_ok()
            {
                loaded = true;
                break;
            }
        }

        if !loaded {
            anyhow::bail!("could not load sqlite-vec extension");
        }

        let _ = sqlx::query("DROP TABLE IF EXISTS page_embeddings")
            .execute(pool)
            .await;

        sqlx::query(
            "CREATE VIRTUAL TABLE page_embeddings USING vec0(\
                 slug TEXT PRIMARY KEY,\
                 embedding float[1536]\
             )",
        )
        .execute(pool)
        .await
        .map_err(|e| anyhow::anyhow!("creating page_embeddings vec0 table: {e}"))?;

        Ok(())
    }
}

fn f32_slice_to_le_bytes(data: &[f32]) -> Vec<u8> {
    let mut buf = Vec::with_capacity(data.len() * 4);
    for &v in data {
        buf.extend_from_slice(&v.to_le_bytes());
    }
    buf
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedding_blob_roundtrip() {
        let vec: Vec<f32> = vec![1.0, -0.5, 0.0, 3.14];
        let blob = f32_slice_to_le_bytes(&vec);
        assert_eq!(blob.len(), 16);

        let decoded: Vec<f32> = blob
            .chunks_exact(4)
            .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
            .collect();
        assert_eq!(vec, decoded);
    }

    #[test]
    fn empty_embedding_produces_empty_blob() {
        let blob = f32_slice_to_le_bytes(&[]);
        assert!(blob.is_empty());
    }

    #[tokio::test]
    async fn vector_search_disabled_returns_empty() {
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        let vs = VectorSearch::new(&pool).await;

        assert!(!vs.is_enabled());
        assert!(vs.vector_search(&[0.0; 1536], 10).await.is_empty());
        assert!(vs.embedding_count().await == 0);
        vs.store_embedding("test", &[0.0; 1536]).await.unwrap();
        assert!(!vs.remove_embedding("test").await.unwrap());
    }
}
