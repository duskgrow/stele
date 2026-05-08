use crate::types::{Error, Frontmatter, PageStatus, PageType, Result};
use serde::Deserialize;

impl Default for Frontmatter {
    fn default() -> Self {
        Frontmatter {
            title: String::new(),
            page_type: PageType::Stub,
            tags: Vec::new(),
            related: Vec::new(),
            sources: Vec::new(),
            date: None,
            status: PageStatus::Seedling,
        }
    }
}

fn default_page_type() -> PageType {
    PageType::Stub
}

fn default_page_status() -> PageStatus {
    PageStatus::Seedling
}

#[derive(Deserialize)]
struct FrontmatterHelper {
    #[serde(default)]
    title: String,
    #[serde(default = "default_page_type")]
    page_type: PageType,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    related: Vec<String>,
    #[serde(default)]
    sources: Vec<String>,
    #[serde(default)]
    date: Option<String>,
    #[serde(default = "default_page_status")]
    status: PageStatus,
}

impl From<FrontmatterHelper> for Frontmatter {
    fn from(h: FrontmatterHelper) -> Self {
        Frontmatter {
            title: h.title,
            page_type: h.page_type,
            tags: h.tags,
            related: h.related,
            sources: h.sources,
            date: h.date,
            status: h.status,
        }
    }
}

/// Parse YAML frontmatter from raw markdown content.
///
/// Expects the content to start with `---\n`, followed by YAML,
/// followed by `---\n`. Returns the parsed Frontmatter and the body content.
pub fn parse(raw: &str) -> Result<(Frontmatter, String)> {
    if !raw.starts_with("---\n") {
        return Err(Error::Parse("missing opening frontmatter delimiter".to_string()));
    }

    let after_open = &raw[4..];

    if let Some(body) = after_open.strip_prefix("---\n") {
        return Ok((Frontmatter::default(), body.to_string()));
    }

    match after_open.find("\n---\n") {
        Some(idx) => {
            let yaml_str = &after_open[..idx];
            let body = &after_open[idx + 5..];

            let frontmatter = if yaml_str.trim().is_empty() {
                Frontmatter::default()
            } else {
                let helper: FrontmatterHelper = serde_yaml::from_str(yaml_str)?;
                helper.into()
            };

            Ok((frontmatter, body.to_string()))
        }
        None => Err(Error::Parse("missing closing frontmatter delimiter".to_string())),
    }
}

/// Serialize a Frontmatter struct to a YAML frontmatter block with `---` delimiters.
///
/// Returns a string in the form `---\n<yaml>---\n` (no body content).
pub fn serialize(frontmatter: &Frontmatter) -> Result<String> {
    let yaml = serde_yaml::to_string(frontmatter)?;
    Ok(format!("---\n{yaml}---\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_valid() {
        let raw = "---\ntitle: Test Page\npage_type: Concept\ntags:\n  - rust\n  - types\nrelated:\n  - other-page\nsources:\n  - https://example.com\ndate: '2024-01-01'\nstatus: Budding\n---\n# Body starts here\n\nSome content.\n";

        let (fm, body) = parse(raw).unwrap();

        assert_eq!(fm.title, "Test Page");
        assert_eq!(fm.page_type, PageType::Concept);
        assert_eq!(fm.tags, vec!["rust", "types"]);
        assert_eq!(fm.related, vec!["other-page"]);
        assert_eq!(fm.sources, vec!["https://example.com"]);
        assert_eq!(fm.date, Some("2024-01-01".to_string()));
        assert_eq!(fm.status, PageStatus::Budding);
        assert_eq!(body, "# Body starts here\n\nSome content.\n");
    }

    #[test]
    fn test_parse_minimal() {
        let raw = "---\ntitle: Minimal Page\n---\nJust the body.\n";

        let (fm, body) = parse(raw).unwrap();

        assert_eq!(fm.title, "Minimal Page");
        assert_eq!(fm.page_type, PageType::Stub);
        assert!(fm.tags.is_empty());
        assert!(fm.related.is_empty());
        assert!(fm.sources.is_empty());
        assert_eq!(fm.date, None);
        assert_eq!(fm.status, PageStatus::Seedling);
        assert_eq!(body, "Just the body.\n");
    }

    #[test]
    fn test_parse_no_frontmatter() {
        let raw = "# No frontmatter here\n\nJust markdown.\n";

        let result = parse(raw);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("missing opening frontmatter delimiter"), "expected opening delimiter error, got: {err}");
    }

    #[test]
    fn test_parse_unclosed() {
        let raw = "---\ntitle: Unclosed\n\nThis is not a closing delimiter.\n";

        let result = parse(raw);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("missing closing frontmatter delimiter"), "expected closing delimiter error, got: {err}");
    }

    #[test]
    fn test_parse_empty_frontmatter() {
        let raw = "---\n---\nBody after empty frontmatter.\n";

        let (fm, body) = parse(raw).unwrap();

        assert_eq!(fm.title, "");
        assert_eq!(fm.page_type, PageType::Stub);
        assert!(fm.tags.is_empty());
        assert_eq!(fm.date, None);
        assert_eq!(fm.status, PageStatus::Seedling);
        assert_eq!(body, "Body after empty frontmatter.\n");
    }

    #[test]
    fn test_parse_unknown_fields() {
        let raw = "---\ntitle: With Extras\npage_type: Entity\nunknown_field: 42\nanother_one:\n  nested: true\nstatus: Evergreen\n---\nBody here.\n";

        let (fm, body) = parse(raw).unwrap();

        assert_eq!(fm.title, "With Extras");
        assert_eq!(fm.page_type, PageType::Entity);
        assert_eq!(fm.status, PageStatus::Evergreen);
        assert_eq!(body, "Body here.\n");
    }

    #[test]
    fn test_roundtrip() {
        let raw = "---\ntitle: Roundtrip Test\npage_type: Synthesis\ntags:\n  - foo\nrelated:\n  - bar\nsources:\n  - baz\ndate: '2024-06-01'\nstatus: Evergreen\n---\n# Body\n";

        let (fm1, _) = parse(raw).unwrap();
        let serialized = serialize(&fm1).unwrap();
        let (fm2, body2) = parse(&serialized).unwrap();

        assert_eq!(fm1, fm2);
        assert_eq!(body2, "");
    }

    #[test]
    fn test_serialize() {
        let fm = Frontmatter {
            title: "Serialize Me".to_string(),
            page_type: PageType::Query,
            tags: vec!["tag1".to_string(), "tag2".to_string()],
            related: vec!["rel1".to_string()],
            sources: vec![],
            date: Some("2024-12-25".to_string()),
            status: PageStatus::Budding,
        };

        let result = serialize(&fm).unwrap();

        assert!(result.starts_with("---\n"));
        assert!(result.ends_with("---\n"));
        assert!(result.contains("title: Serialize Me"));
        assert!(result.contains("page_type: Query"));
        assert!(result.contains("status: Budding"));
    }

    #[test]
    fn test_body_preserved() {
        let body = "# Heading\n\nParagraph with **bold** and *italic*.\n\n- list item 1\n- list item 2\n\n```rust\nlet x = 42;\n```\n";
        let raw = format!("---\ntitle: Body Test\n---\n{body}");

        let (_, parsed_body) = parse(&raw).unwrap();
        assert_eq!(parsed_body, body);
    }

    #[test]
    fn test_empty_title_defaults_to_empty_string() {
        let raw = "---\ntitle: \"\"\n---\nBody\n";
        let (fm, body) = parse(raw).unwrap();
        assert_eq!(fm.title, "");
        assert_eq!(body, "Body\n");
    }

    #[test]
    fn test_missing_title_defaults_to_empty_string() {
        let raw = "---\npage_type: Concept\n---\nBody\n";
        let (fm, body) = parse(raw).unwrap();
        assert_eq!(fm.title, "");
        assert_eq!(body, "Body\n");
    }

    #[test]
    fn test_empty_tags_defaults_to_empty_vec() {
        let raw = "---\ntitle: Test\ntags: []\n---\nBody\n";
        let (fm, _) = parse(raw).unwrap();
        assert!(fm.tags.is_empty());
    }

    #[test]
    fn test_missing_tags_defaults_to_empty_vec() {
        let raw = "---\ntitle: Test\n---\nBody\n";
        let (fm, _) = parse(raw).unwrap();
        assert!(fm.tags.is_empty());
    }

    #[test]
    fn test_empty_related_defaults_to_empty_vec() {
        let raw = "---\ntitle: Test\nrelated: []\n---\nBody\n";
        let (fm, _) = parse(raw).unwrap();
        assert!(fm.related.is_empty());
    }

    #[test]
    fn test_empty_sources_defaults_to_empty_vec() {
        let raw = "---\ntitle: Test\nsources: []\n---\nBody\n";
        let (fm, _) = parse(raw).unwrap();
        assert!(fm.sources.is_empty());
    }

    #[test]
    fn test_unicode_in_title_and_body() {
        let raw = "---\ntitle: \u{6d4b}\u{8bd5}\n---\n\u{8fd9}\u{662f}\u{4e00}\u{4e2a}\u{6d4b}\u{8bd5}\n";
        let (fm, body) = parse(raw).unwrap();
        assert_eq!(fm.title, "\u{6d4b}\u{8bd5}");
        assert_eq!(body, "\u{8fd9}\u{662f}\u{4e00}\u{4e2a}\u{6d4b}\u{8bd5}\n");
    }
}
