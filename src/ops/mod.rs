/// Page operation handlers (get, put, delete, list).
pub mod page;
/// Maintenance and reindex handlers.
pub mod maintain;
/// Operation registry and dispatch.
pub mod registry;
/// Search, graph, and stats operation handlers.
pub mod search;
/// FNS synchronization handler.
pub mod sync;

pub use registry::{Operation, OperationMeta, OperationRegistry};
