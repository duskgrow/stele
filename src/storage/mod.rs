pub mod fns;
pub mod sqlite;

use chrono::{DateTime, Utc};

/// Metadata for a file or directory entry.
#[derive(Debug, Clone)]
pub struct FileMeta {
    pub path: String,
    pub is_dir: bool,
    pub size: u64,
    pub modified: Option<DateTime<Utc>>,
}

/// Detailed stat info for a single file.
#[derive(Debug, Clone)]
pub struct FileStat {
    pub size: u64,
    pub modified: DateTime<Utc>,
    pub content_hash: String,
}

/// Errors from storage backend operations.
#[derive(thiserror::Error, Debug)]
pub enum BackendError {
    #[error("not found: {0}")]
    NotFound(String),
    #[error("conflict: {0}")]
    Conflict(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("api error: {status} {message}")]
    Api { status: u16, message: String },
    #[error("auth error: {0}")]
    Auth(String),
}

#[async_trait::async_trait]
pub trait FileBackend: Send + Sync {
    /// Read file content.
    async fn get(&self, path: &str) -> Result<String, BackendError>;

    /// Write/overwrite file content.
    async fn put(&self, path: &str, content: &str) -> Result<(), BackendError>;

    /// Append content to end of file.
    async fn append(&self, path: &str, content: &str) -> Result<(), BackendError>;

    /// Delete file (soft delete: move to .archive/).
    async fn delete(&self, path: &str) -> Result<(), BackendError>;

    /// List files in a directory.
    async fn list(&self, dir: &str) -> Result<Vec<FileMeta>, BackendError>;

    /// Check if a file exists.
    async fn exists(&self, path: &str) -> Result<bool, BackendError>;

    /// Get file metadata (mtime, size, hash).
    async fn stat(&self, path: &str) -> Result<FileStat, BackendError>;
}
