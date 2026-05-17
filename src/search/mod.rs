/// Keyword full-text search over the SQLite FTS5 index.
pub mod keyword;
pub use keyword::{SearchHit, keyword_search};
