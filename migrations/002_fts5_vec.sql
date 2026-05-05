-- 向量搜索表 (sqlite-vec)
CREATE VIRTUAL TABLE page_embeddings USING vec0(
    embedding float[1536]
);

-- 搜索缓存表
CREATE TABLE search_cache (
    query_hash TEXT PRIMARY KEY,
    query TEXT NOT NULL,
    results JSON,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP
);
