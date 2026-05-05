use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

use crate::models::Frontmatter;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Page {
    pub slug: String,
    pub vault: String,
    pub frontmatter: Frontmatter,
    pub compiled_truth: String,
    pub timeline: Vec<TimelineEntry>,
    pub content_hash: String,
    pub raw_content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelineEntry {
    pub date: NaiveDate,
    pub source_url: Option<String>,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PageType {
    Entity,
    Concept,
    Source,
    Query,
    Synthesis,
    Comparison,
    Stub,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PageStatus {
    Seedling,
    Budding,
    Evergreen,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_frontmatter() -> Frontmatter {
        Frontmatter {
            r#type: PageType::Entity,
            title: "Test Entity".to_string(),
            tags: vec!["test".to_string(), "demo".to_string()],
            related: vec!["concepts/demo".to_string()],
            sources: vec!["2026-05-05-rss".to_string()],
            date: NaiveDate::from_ymd_opt(2026, 5, 5).unwrap(),
            status: Some(PageStatus::Seedling),
        }
    }

    #[test]
    fn page_serde_roundtrip() {
        let page = Page {
            slug: "wiki/entities/test".to_string(),
            vault: "forge".to_string(),
            frontmatter: sample_frontmatter(),
            compiled_truth: "This is the compiled truth.".to_string(),
            timeline: vec![TimelineEntry {
                date: NaiveDate::from_ymd_opt(2026, 5, 1).unwrap(),
                source_url: Some("https://example.com".to_string()),
                content: "Initial record".to_string(),
            }],
            content_hash: "abc123".to_string(),
            raw_content: "# Test\n\nContent".to_string(),
        };

        let json = serde_json::to_string(&page).unwrap();
        let restored: Page = serde_json::from_str(&json).unwrap();

        assert_eq!(page.slug, restored.slug);
        assert_eq!(page.vault, restored.vault);
        assert_eq!(page.compiled_truth, restored.compiled_truth);
        assert_eq!(page.timeline.len(), restored.timeline.len());
        assert_eq!(page.content_hash, restored.content_hash);
        assert_eq!(page.raw_content, restored.raw_content);
    }

    #[test]
    fn timeline_entry_serde_roundtrip() {
        let entry = TimelineEntry {
            date: NaiveDate::from_ymd_opt(2026, 5, 1).unwrap(),
            source_url: None,
            content: "A timeline entry".to_string(),
        };

        let json = serde_json::to_string(&entry).unwrap();
        let restored: TimelineEntry = serde_json::from_str(&json).unwrap();

        assert_eq!(entry.date, restored.date);
        assert_eq!(entry.source_url, restored.source_url);
        assert_eq!(entry.content, restored.content);
    }

    #[test]
    fn page_type_serialize_lowercase() {
        assert_eq!(
            serde_json::to_string(&PageType::Entity).unwrap(),
            "\"entity\""
        );
        assert_eq!(
            serde_json::to_string(&PageType::Concept).unwrap(),
            "\"concept\""
        );
        assert_eq!(
            serde_json::to_string(&PageType::Source).unwrap(),
            "\"source\""
        );
        assert_eq!(
            serde_json::to_string(&PageType::Query).unwrap(),
            "\"query\""
        );
        assert_eq!(
            serde_json::to_string(&PageType::Synthesis).unwrap(),
            "\"synthesis\""
        );
        assert_eq!(
            serde_json::to_string(&PageType::Comparison).unwrap(),
            "\"comparison\""
        );
    }

    #[test]
    fn page_type_deserialize_lowercase() {
        assert!(matches!(
            serde_json::from_str::<PageType>("\"entity\"").unwrap(),
            PageType::Entity
        ));
        assert!(matches!(
            serde_json::from_str::<PageType>("\"concept\"").unwrap(),
            PageType::Concept
        ));
    }

    #[test]
    fn page_status_serialize_lowercase() {
        assert_eq!(
            serde_json::to_string(&PageStatus::Seedling).unwrap(),
            "\"seedling\""
        );
        assert_eq!(
            serde_json::to_string(&PageStatus::Budding).unwrap(),
            "\"budding\""
        );
        assert_eq!(
            serde_json::to_string(&PageStatus::Evergreen).unwrap(),
            "\"evergreen\""
        );
    }

    #[test]
    fn page_status_deserialize_lowercase() {
        assert!(matches!(
            serde_json::from_str::<PageStatus>("\"seedling\"").unwrap(),
            PageStatus::Seedling
        ));
        assert!(matches!(
            serde_json::from_str::<PageStatus>("\"evergreen\"").unwrap(),
            PageStatus::Evergreen
        ));
    }
}
