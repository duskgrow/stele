use serde::{Deserialize, Serialize};

/// A knowledge page with frontmatter, compiled truth, timeline, and links.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Page {
    pub slug: String,
    pub frontmatter: Frontmatter,
    pub compiled_truth: String,
    pub timeline: Vec<TimelineEntry>,
    pub content_hash: String,
    pub raw_content: String,
}

/// YAML frontmatter parsed from the top of a markdown page.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Frontmatter {
    pub title: String,
    pub page_type: PageType,
    pub tags: Vec<String>,
    pub sources: Vec<String>,
    pub date: Option<String>,
    #[serde(default = "default_shared")]
    pub visibility: String,
    #[serde(default)]
    pub created_by: Option<String>,
}

fn default_shared() -> String {
    "shared".to_string()
}

/// Classification of a page's role in the knowledge base.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub enum PageType {
    Entity,
    Concept,
    Source,
    Query,
    Synthesis,
    Comparison,
}

impl std::fmt::Display for PageType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Entity => write!(f, "Entity"),
            Self::Concept => write!(f, "Concept"),
            Self::Source => write!(f, "Source"),
            Self::Query => write!(f, "Query"),
            Self::Synthesis => write!(f, "Synthesis"),
            Self::Comparison => write!(f, "Comparison"),
        }
    }
}

impl PageType {
    pub const NAMES: &'static [&'static str] = &[
        "Entity",
        "Concept",
        "Source",
        "Query",
        "Synthesis",
        "Comparison",
    ];
}

/// A single dated entry in a page's timeline section.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TimelineEntry {
    pub date: String,
    pub source_url: Option<String>,
    pub content: String,
    pub agent: Option<String>,
}

/// Input for appending a timeline entry via `page.put`.
///
/// The date is system-generated (today) and not provided by the caller.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TimelineAppendInput {
    pub content: String,
    pub agent: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_frontmatter() -> Frontmatter {
        Frontmatter {
            title: "Test Page".to_string(),
            page_type: PageType::Concept,
            tags: vec!["rust".to_string(), "types".to_string()],
            sources: vec!["https://example.com".to_string()],
            date: Some("2024-01-01".to_string()),
            visibility: "shared".to_string(),
            created_by: None,
        }
    }

    fn sample_timeline() -> Vec<TimelineEntry> {
        vec![
            TimelineEntry {
                date: "2024-01-01".to_string(),
                source_url: Some("https://example.com".to_string()),
                content: "First entry".to_string(),
                agent: Some("agent-1".to_string()),
            },
            TimelineEntry {
                date: "2024-06-15".to_string(),
                source_url: None,
                content: "Second entry".to_string(),
                agent: None,
            },
        ]
    }

    fn sample_page() -> Page {
        Page {
            slug: "test-page".to_string(),
            frontmatter: sample_frontmatter(),
            compiled_truth: "This is the truth.".to_string(),
            timeline: sample_timeline(),
            content_hash: "abc123".to_string(),
            raw_content: "# Test Page\n\nContent here.".to_string(),
        }
    }

    #[test]
    fn page_serde_roundtrip_json() {
        let page = sample_page();
        let json = serde_json::to_string(&page).expect("serialize");
        let deserialized: Page = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(page, deserialized);
    }

    #[test]
    fn page_serde_roundtrip_yaml() {
        let page = sample_page();
        let yaml = serde_yaml::to_string(&page).expect("serialize");
        let deserialized: Page = serde_yaml::from_str(&yaml).expect("deserialize");
        assert_eq!(page, deserialized);
    }

    #[test]
    fn frontmatter_serde_roundtrip_json() {
        let fm = sample_frontmatter();
        let json = serde_json::to_string(&fm).expect("serialize");
        let deserialized: Frontmatter = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(fm, deserialized);
    }

    #[test]
    fn frontmatter_serde_roundtrip_yaml() {
        let fm = sample_frontmatter();
        let yaml = serde_yaml::to_string(&fm).expect("serialize");
        let deserialized: Frontmatter = serde_yaml::from_str(&yaml).expect("deserialize");
        assert_eq!(fm, deserialized);
    }

    #[test]
    fn timeline_entry_serde_roundtrip_json() {
        let entry = TimelineEntry {
            date: "2024-03-20".to_string(),
            source_url: Some("https://rust-lang.org".to_string()),
            content: "Rust 1.75 released".to_string(),
            agent: Some("release-bot".to_string()),
        };
        let json = serde_json::to_string(&entry).expect("serialize");
        let deserialized: TimelineEntry = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(entry, deserialized);
    }

    #[test]
    fn timeline_entry_serde_roundtrip_yaml() {
        let entry = TimelineEntry {
            date: "2024-03-20".to_string(),
            source_url: None,
            content: "Some content".to_string(),
            agent: None,
        };
        let yaml = serde_yaml::to_string(&entry).expect("serialize");
        let deserialized: TimelineEntry = serde_yaml::from_str(&yaml).expect("deserialize");
        assert_eq!(entry, deserialized);
    }

    #[test]
    fn page_type_variants_serialize_correctly() {
        assert_eq!(
            serde_json::to_string(&PageType::Entity).unwrap(),
            "\"Entity\""
        );
        assert_eq!(
            serde_json::to_string(&PageType::Concept).unwrap(),
            "\"Concept\""
        );
        assert_eq!(
            serde_json::to_string(&PageType::Source).unwrap(),
            "\"Source\""
        );
        assert_eq!(
            serde_json::to_string(&PageType::Query).unwrap(),
            "\"Query\""
        );
        assert_eq!(
            serde_json::to_string(&PageType::Synthesis).unwrap(),
            "\"Synthesis\""
        );
        assert_eq!(
            serde_json::to_string(&PageType::Comparison).unwrap(),
            "\"Comparison\""
        );
    }

    #[test]
    fn page_type_deserializes_correctly() {
        assert_eq!(
            serde_json::from_str::<PageType>("\"Entity\"").unwrap(),
            PageType::Entity
        );
        assert_eq!(
            serde_json::from_str::<PageType>("\"Concept\"").unwrap(),
            PageType::Concept
        );
    }

    #[test]
    fn visibility_defaults_to_shared() {
        let fm: Frontmatter =
            serde_json::from_str(r#"{"title":"T","page_type":"Entity","tags":[],"sources":[]}"#)
                .unwrap();
        assert_eq!(fm.visibility, "shared");
    }

    #[test]
    fn created_by_defaults_to_none() {
        let fm: Frontmatter =
            serde_json::from_str(r#"{"title":"T","page_type":"Entity","tags":[],"sources":[]}"#)
                .unwrap();
        assert_eq!(fm.created_by, None);
    }

    #[test]
    fn old_status_field_silently_ignored_json() {
        let fm: Frontmatter = serde_json::from_str(
            r#"{"title":"T","page_type":"Entity","tags":[],"sources":[],"status":"Seedling"}"#,
        )
        .unwrap();
        assert_eq!(fm.title, "T");
        assert_eq!(fm.page_type, PageType::Entity);
    }

    #[test]
    fn old_related_field_silently_ignored_json() {
        let fm: Frontmatter = serde_json::from_str(
            r#"{"title":"T","page_type":"Entity","tags":[],"sources":[],"related":["foo","bar"]}"#,
        )
        .unwrap();
        assert_eq!(fm.title, "T");
        assert_eq!(fm.page_type, PageType::Entity);
    }

    #[test]
    fn all_old_fields_combined_ignored_json() {
        let fm: Frontmatter = serde_json::from_str(
            r#"{"title":"T","page_type":"Entity","tags":[],"sources":[],"status":"Seedling","related":["foo","bar"]}"#
        ).unwrap();
        assert_eq!(fm.title, "T");
        assert_eq!(fm.page_type, PageType::Entity);
    }
}
