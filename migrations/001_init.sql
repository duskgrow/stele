PRAGMA journal_mode=WAL;
PRAGMA foreign_keys = ON;

-- 页面主表
CREATE TABLE IF NOT EXISTS pages (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    slug TEXT NOT NULL UNIQUE,
    title TEXT NOT NULL,
    type TEXT NOT NULL,
    vault TEXT NOT NULL DEFAULT 'forge',
    content_hash TEXT NOT NULL,
    compiled_truth TEXT,
    timeline TEXT,
    frontmatter TEXT NOT NULL,
    sources JSON,
    tags JSON,
    related JSON,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME DEFAULT CURRENT_TIMESTAMP
);

-- 全文搜索虚拟表
CREATE VIRTUAL TABLE IF NOT EXISTS pages_fts USING fts5(
    slug,
    title,
    compiled_truth,
    timeline,
    content='pages',
    content_rowid='id'
);

-- 全文搜索触发器：自动同步
CREATE TRIGGER IF NOT EXISTS pages_fts_insert AFTER INSERT ON pages BEGIN
    INSERT INTO pages_fts(rowid, slug, title, compiled_truth, timeline)
    VALUES (new.id, new.slug, new.title, new.compiled_truth, new.timeline);
END;

CREATE TRIGGER IF NOT EXISTS pages_fts_delete AFTER DELETE ON pages BEGIN
    INSERT INTO pages_fts(pages_fts, rowid, slug, title, compiled_truth, timeline)
    VALUES ('delete', old.id, old.slug, old.title, old.compiled_truth, old.timeline);
END;

CREATE TRIGGER IF NOT EXISTS pages_fts_update AFTER UPDATE ON pages BEGIN
    INSERT INTO pages_fts(pages_fts, rowid, slug, title, compiled_truth, timeline)
    VALUES ('delete', old.id, old.slug, old.title, old.compiled_truth, old.timeline);
    INSERT INTO pages_fts(rowid, slug, title, compiled_truth, timeline)
    VALUES (new.id, new.slug, new.title, new.compiled_truth, new.timeline);
END;

-- 链接图谱表
CREATE TABLE IF NOT EXISTS links (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    source_slug TEXT NOT NULL,
    target_slug TEXT NOT NULL,
    link_type TEXT DEFAULT 'link',
    context_snippet TEXT,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    UNIQUE(source_slug, target_slug, link_type)
);

CREATE INDEX IF NOT EXISTS idx_links_source ON links(source_slug);
CREATE INDEX IF NOT EXISTS idx_links_target ON links(target_slug);
