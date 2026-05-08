/// Error types and the crate-wide `Result` alias.
pub mod error;
/// Link graph types (`Link`, `LinkType`).
pub mod link;
/// Page types (`Page`, `Frontmatter`, `PageType`, `PageStatus`, `TimelineEntry`).
pub mod page;

pub use error::{Error, Result};
pub use link::{Link, LinkType};
pub use page::{Frontmatter, Page, PageStatus, PageType, TimelineEntry};
