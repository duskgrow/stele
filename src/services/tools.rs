use std::sync::Arc;

use serde_json::{Value, json};

use super::ingest::{brain_append, brain_delete, brain_get, brain_list, brain_put};
use super::maintain::brain_maintain;
use super::sync::brain_sync;
use crate::mcp::protocol::JsonRpcError;
use crate::search::hybrid::brain_query;
use crate::search::keyword::keyword_search;
use crate::storage::FileBackend;
use crate::storage::sqlite::SqliteBackend;

pub struct ToolRegistry {
    db: Arc<SqliteBackend>,
    file_backend: Option<Arc<dyn FileBackend>>,
    vault: String,
}

impl ToolRegistry {
    pub fn new(db: Arc<SqliteBackend>) -> Self {
        Self {
            db,
            file_backend: None,
            vault: "forge".into(),
        }
    }

    pub fn with_file_backend(mut self, fb: Arc<dyn FileBackend>) -> Self {
        self.file_backend = Some(fb);
        self
    }

    pub fn with_vault(mut self, vault: String) -> Self {
        self.vault = vault;
        self
    }

    pub fn list_tools(&self) -> Vec<Value> {
        vec![
            json!({
                "name": "brain_search",
                "description": "Search the brain knowledge base using FTS5 keyword search",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "query": {
                            "type": "string",
                            "description": "Search query with optional FTS5 syntax (AND, OR, NOT, quoted phrases)"
                        },
                        "limit": {
                            "type": "integer",
                            "default": 20,
                            "maximum": 100,
                            "description": "Maximum number of results to return"
                        },
                        "type_filter": {
                            "type": "string",
                            "description": "Optional filter by page type (entity, concept, source, query, synthesis, comparison)"
                        }
                    },
                    "required": ["query"]
                }
            }),
            json!({
                "name": "brain_stats",
                "description": "Return knowledge base statistics",
                "inputSchema": { "type": "object", "properties": {} }
            }),
            json!({
                "name": "brain_maintain",
                "description": "Run maintenance checks: lint, orphan detection, backlink verification.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "scope": {
                            "type": "string",
                            "enum": ["lint", "orphans", "backlinks", "full"],
                            "default": "full",
                            "description": "Which maintenance check to run"
                        }
                    }
                }
            }),
            json!({
                "name": "brain_append",
                "description": "Append a timeline entry to a brain document",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "slug": { "type": "string" },
                        "vault": { "type": "string", "default": "forge" },
                        "timeline_entry": { "type": "string", "description": "Timeline entry text, without date prefix" },
                        "date": { "type": "string", "description": "Entry date, defaults to today" }
                    },
                    "required": ["slug", "timeline_entry"]
                }
            }),
            json!({
                "name": "brain_list",
                "description": "List files in a brain directory",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "dir": { "type": "string" },
                        "vault": { "type": "string", "default": "forge" },
                        "recursive": { "type": "boolean", "default": false }
                    },
                    "required": ["dir"]
                }
            }),
            json!({
                "name": "brain_sync",
                "description": "Full reconciliation: list FNS files, diff against SQLite, re-index changed pages, remove stale pages, and update links",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "dir": {
                            "type": "string",
                            "description": "Root directory to sync (e.g. 'wiki')",
                            "default": "wiki"
                        }
                    }
                }
            }),
            json!({
                "name": "brain_enrich",
                "description": "Extract wikilinks from a page, update the link graph, and create stubs for missing targets",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "slug": { "type": "string", "description": "Page slug to enrich" },
                        "depth": { "type": "integer", "default": 1, "description": "Recursion depth (1=current page only, 2=also process linked pages)" }
                    },
                    "required": ["slug"]
                }
            }),
            json!({
                "name": "brain_query",
                "description": "Hybrid search with RRF fusion: keyword + vector + graph signals (direct links, source overlap, common neighbors, type affinity)",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "query": { "type": "string", "description": "Search query text" },
                        "limit": { "type": "integer", "default": 10, "maximum": 100, "description": "Maximum results to return" },
                        "from_slugs": { "type": "array", "items": { "type": "string" }, "description": "Known relevant page slugs for graph signal boost" }
                    },
                    "required": ["query"]
                }
            }),
            json!({
                "name": "brain_get",
                "description": "Retrieve a brain document by slug",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "slug": { "type": "string", "description": "Page path, e.g. 'wiki/entities/fns'" },
                        "vault": { "type": "string", "default": "forge" }
                    },
                    "required": ["slug"]
                }
            }),
            json!({
                "name": "brain_put",
                "description": "Store or update a brain document (creates or overwrites)",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "slug": { "type": "string", "description": "Page path, e.g. 'wiki/entities/fns'" },
                        "vault": { "type": "string", "default": "forge" },
                        "content": { "type": "string", "description": "Full Markdown content with frontmatter" },
                        "etag": { "type": "string", "description": "Optimistic lock: expected content_hash, omit to skip check" }
                    },
                    "required": ["slug", "content"]
                }
            }),
            json!({
                "name": "brain_delete",
                "description": "Delete a brain document by slug",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "slug": { "type": "string", "description": "Page path to delete" },
                        "vault": { "type": "string", "default": "forge" }
                    },
                    "required": ["slug"]
                }
            }),
        ]
    }

    pub async fn call(&self, name: &str, arguments: Value) -> Result<Value, JsonRpcError> {
        match name {
            "brain_search" => self.execute_brain_search(arguments).await,
            "brain_stats" => self.execute_brain_stats().await,
            "brain_maintain" => self.execute_brain_maintain(arguments).await,
            "brain_append" => self.execute_brain_append(arguments).await,
            "brain_list" => self.execute_brain_list(arguments).await,
            "brain_sync" => self.execute_brain_sync(arguments).await,
            "brain_query" => self.execute_brain_query(arguments).await,
            "brain_get" => self.execute_brain_get(arguments).await,
            "brain_put" => self.execute_brain_put(arguments).await,
            "brain_delete" => self.execute_brain_delete(arguments).await,
            "brain_enrich" => self.execute_brain_enrich(arguments).await,
            _ => Err(JsonRpcError {
                code: JsonRpcError::INVALID_PARAMS,
                message: format!("Unknown tool: {}", name),
                data: None,
            }),
        }
    }

    async fn execute_brain_search(&self, arguments: Value) -> Result<Value, JsonRpcError> {
        let query = arguments
            .get("query")
            .and_then(|v| v.as_str())
            .ok_or_else(|| JsonRpcError {
                code: JsonRpcError::INVALID_PARAMS,
                message: "Missing required 'query' parameter".into(),
                data: None,
            })?;

        let limit = arguments
            .get("limit")
            .and_then(|v| v.as_u64())
            .unwrap_or(20) as usize;
        let limit = limit.min(100);

        let type_filter = arguments.get("type_filter").and_then(|v| v.as_str());

        let hits = keyword_search(&self.db, query, limit, type_filter)
            .await
            .map_err(|e| JsonRpcError {
                code: JsonRpcError::INTERNAL_ERROR,
                message: format!("Search failed: {}", e),
                data: None,
            })?;

        let results: Vec<Value> = hits
            .into_iter()
            .map(|hit| {
                json!({
                    "slug": hit.slug,
                    "title": hit.title,
                    "preview": hit.compiled_truth_preview,
                    "rank": hit.rank
                })
            })
            .collect();

        Ok(json!({
            "query": query,
            "total": results.len(),
            "results": results
        }))
    }

    async fn execute_brain_stats(&self) -> Result<Value, JsonRpcError> {
        let stats = self.db.get_stats().await.map_err(|e| JsonRpcError {
            code: JsonRpcError::INTERNAL_ERROR,
            message: format!("Database error: {}", e),
            data: None,
        })?;

        Ok(json!({
            "total_pages": stats.total_pages,
            "by_type": stats.by_type,
            "total_links": stats.total_links,
            "orphan_pages": stats.orphan_pages,
            "db_size_mb": stats.db_size_mb,
            "last_sync": stats.last_sync,
        }))
    }

    async fn execute_brain_maintain(&self, arguments: Value) -> Result<Value, JsonRpcError> {
        let scope = arguments
            .get("scope")
            .and_then(|v| v.as_str())
            .unwrap_or("full");

        let result = brain_maintain(&self.db, scope)
            .await
            .map_err(|e| JsonRpcError {
                code: JsonRpcError::INTERNAL_ERROR,
                message: format!("Maintain error: {}", e),
                data: None,
            })?;

        serde_json::to_value(result).map_err(|e| JsonRpcError {
            code: JsonRpcError::INTERNAL_ERROR,
            message: format!("Serialization error: {}", e),
            data: None,
        })
    }

    async fn execute_brain_append(&self, arguments: Value) -> Result<Value, JsonRpcError> {
        let fb = self.file_backend.as_ref().ok_or_else(|| JsonRpcError {
            code: JsonRpcError::INTERNAL_ERROR,
            message: "FileBackend not configured".into(),
            data: None,
        })?;

        let slug = arguments
            .get("slug")
            .and_then(|v| v.as_str())
            .ok_or_else(|| JsonRpcError {
                code: JsonRpcError::INVALID_PARAMS,
                message: "Missing required 'slug' parameter".into(),
                data: None,
            })?;

        let timeline_entry = arguments
            .get("timeline_entry")
            .and_then(|v| v.as_str())
            .ok_or_else(|| JsonRpcError {
                code: JsonRpcError::INVALID_PARAMS,
                message: "Missing required 'timeline_entry' parameter".into(),
                data: None,
            })?;

        let date = arguments.get("date").and_then(|v| v.as_str());

        let result = brain_append(fb.as_ref(), slug, timeline_entry, date).await?;
        serde_json::to_value(result).map_err(|e| JsonRpcError {
            code: JsonRpcError::INTERNAL_ERROR,
            message: format!("Serialization error: {}", e),
            data: None,
        })
    }

    async fn execute_brain_list(&self, arguments: Value) -> Result<Value, JsonRpcError> {
        let fb = self.file_backend.as_ref().ok_or_else(|| JsonRpcError {
            code: JsonRpcError::INTERNAL_ERROR,
            message: "FileBackend not configured".into(),
            data: None,
        })?;

        let dir = arguments
            .get("dir")
            .and_then(|v| v.as_str())
            .ok_or_else(|| JsonRpcError {
                code: JsonRpcError::INVALID_PARAMS,
                message: "Missing required 'dir' parameter".into(),
                data: None,
            })?;

        let recursive = arguments
            .get("recursive")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let result = brain_list(fb.as_ref(), dir, recursive).await?;
        serde_json::to_value(result).map_err(|e| JsonRpcError {
            code: JsonRpcError::INTERNAL_ERROR,
            message: format!("Serialization error: {}", e),
            data: None,
        })
    }

    async fn execute_brain_sync(&self, arguments: Value) -> Result<Value, JsonRpcError> {
        let fb = self.file_backend.as_ref().ok_or_else(|| JsonRpcError {
            code: JsonRpcError::INTERNAL_ERROR,
            message: "FileBackend not configured".into(),
            data: None,
        })?;

        let dir = arguments
            .get("dir")
            .and_then(|v| v.as_str())
            .unwrap_or("wiki");

        let result = brain_sync(fb.as_ref(), &self.db, dir, &self.vault)
            .await
            .map_err(|e| JsonRpcError {
                code: JsonRpcError::INTERNAL_ERROR,
                message: format!("Sync error: {}", e),
                data: None,
            })?;

        Ok(json!({
            "files_changed": result.files_changed,
            "pages_indexed": result.pages_indexed,
            "pages_removed": result.pages_removed,
            "links_updated": result.links_updated,
        }))
    }

    async fn execute_brain_query(&self, arguments: Value) -> Result<Value, JsonRpcError> {
        let query = arguments
            .get("query")
            .and_then(|v| v.as_str())
            .ok_or_else(|| JsonRpcError {
                code: JsonRpcError::INVALID_PARAMS,
                message: "Missing required 'query' parameter".into(),
                data: None,
            })?;

        let limit = arguments
            .get("limit")
            .and_then(|v| v.as_u64())
            .unwrap_or(10) as usize;
        let limit = limit.min(100);

        let from_slugs: Vec<String> = arguments
            .get("from_slugs")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();

        brain_query(&self.db, None, query, limit, &from_slugs)
            .await
            .map_err(|e| JsonRpcError {
                code: JsonRpcError::INTERNAL_ERROR,
                message: format!("brain_query failed: {}", e),
                data: None,
            })
    }

    async fn execute_brain_get(&self, arguments: Value) -> Result<Value, JsonRpcError> {
        let fb = self.file_backend.as_ref().ok_or_else(|| JsonRpcError {
            code: JsonRpcError::INTERNAL_ERROR,
            message: "FileBackend not configured".into(),
            data: None,
        })?;

        let result = brain_get(fb.as_ref(), &self.db, arguments).await?;
        serde_json::to_value(result).map_err(|e| JsonRpcError {
            code: JsonRpcError::INTERNAL_ERROR,
            message: format!("Serialization error: {}", e),
            data: None,
        })
    }

    async fn execute_brain_put(&self, arguments: Value) -> Result<Value, JsonRpcError> {
        let fb = self.file_backend.as_ref().ok_or_else(|| JsonRpcError {
            code: JsonRpcError::INTERNAL_ERROR,
            message: "FileBackend not configured".into(),
            data: None,
        })?;

        let result = brain_put(fb.as_ref(), &self.db, arguments).await?;
        serde_json::to_value(result).map_err(|e| JsonRpcError {
            code: JsonRpcError::INTERNAL_ERROR,
            message: format!("Serialization error: {}", e),
            data: None,
        })
    }

    async fn execute_brain_delete(&self, arguments: Value) -> Result<Value, JsonRpcError> {
        let fb = self.file_backend.as_ref().ok_or_else(|| JsonRpcError {
            code: JsonRpcError::INTERNAL_ERROR,
            message: "FileBackend not configured".into(),
            data: None,
        })?;

        let result = brain_delete(fb.as_ref(), &self.db, arguments).await?;
        serde_json::to_value(result).map_err(|e| JsonRpcError {
            code: JsonRpcError::INTERNAL_ERROR,
            message: format!("Serialization error: {}", e),
            data: None,
        })
    }

    async fn execute_brain_enrich(&self, arguments: Value) -> Result<Value, JsonRpcError> {
        let fb = self.file_backend.as_ref().ok_or_else(|| JsonRpcError {
            code: JsonRpcError::INTERNAL_ERROR,
            message: "FileBackend not configured".into(),
            data: None,
        })?;

        let slug = arguments
            .get("slug")
            .and_then(|v| v.as_str())
            .ok_or_else(|| JsonRpcError {
                code: JsonRpcError::INVALID_PARAMS,
                message: "Missing required 'slug' parameter".into(),
                data: None,
            })?;

        let depth = arguments
            .get("depth")
            .and_then(|v| v.as_u64())
            .map(|d| d as usize);

        let result =
            super::enrich::brain_enrich(fb.as_ref(), &self.db, slug, depth)
                .await
                .map_err(|e| JsonRpcError {
                    code: JsonRpcError::INTERNAL_ERROR,
                    message: format!("brain_enrich failed: {}", e),
                    data: None,
                })?;

        serde_json::to_value(result).map_err(|e| JsonRpcError {
            code: JsonRpcError::INTERNAL_ERROR,
            message: format!("Serialization error: {}", e),
            data: None,
        })
    }
}
