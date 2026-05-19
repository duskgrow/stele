pub mod graph_query;
pub mod handler;
pub mod maintain;
pub mod maintain_op;
pub mod page;
pub mod page_delete;
pub mod page_get;
pub mod page_list;
pub mod page_put;
pub mod registry;
pub mod reindex_op;
pub mod search;
pub mod search_op;
pub mod stats_op;
pub mod sync;
pub mod sync_op;
pub mod tags_list_op;
pub mod tags_search_op;

pub use handler::{OpExec, OpHandler, OperationContext};

pub use registry::{OperationMeta, OperationRegistry};

/// Returns true if the basename of the path starts with a dot.
pub(crate) fn is_hidden_path(path: &str) -> bool {
    path.rsplit('/').next().unwrap_or(path).starts_with('.')
}

/// Returns true if the path starts with the raw prefix "raw/".
pub(crate) fn is_raw_path(path: &str) -> bool {
    path.starts_with("raw/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_raw_path() {
        assert!(is_raw_path("raw/foo"));
        assert!(is_raw_path("raw/sub/deep"));
        assert!(!is_raw_path("wiki/foo"));
        assert!(!is_raw_path("raw"));
        assert!(!is_raw_path("rawfoo"));
    }
}
