use crate::types::{Error, Frontmatter, PageType, Result};
use serde::Deserialize;
use serde_json::Value;

impl Default for Frontmatter {
    fn default() -> Self {
        Frontmatter {
            title: String::new(),
            page_type: PageType::Entity,
            tags: Vec::new(),
            sources: Vec::new(),
            date: None,
            visibility: "shared".to_string(),
            created_by: None,
        }
    }
}

fn default_page_type() -> PageType {
    PageType::Entity
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
    sources: Vec<String>,
    #[serde(default)]
    date: Option<String>,
    #[serde(default = "default_shared")]
    visibility: String,
    #[serde(default)]
    created_by: Option<String>,
}

fn default_shared() -> String {
    "shared".to_string()
}

impl From<FrontmatterHelper> for Frontmatter {
    fn from(h: FrontmatterHelper) -> Self {
        Frontmatter {
            title: h.title,
            page_type: h.page_type,
            tags: h.tags,
            sources: h.sources,
            date: h.date,
            visibility: h.visibility,
            created_by: h.created_by,
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

/// Merge partial frontmatter updates into an existing frontmatter.
///
/// Uses top-level key replacement semantics: each key in `updates` replaces
/// the corresponding field in `existing`. Unknown keys are silently ignored.
/// Enum fields (`page_type`) are validated; `title: null` is an error.
pub fn merge_frontmatter(existing: &Frontmatter, updates: &Value) -> Result<Frontmatter> {
    let obj = match updates.as_object() {
        Some(o) => o,
        None => return Ok(existing.clone()),
    };

    let mut result = existing.clone();

    for (key, value) in obj {
        match key.as_str() {
            "title" => {
                if value.is_null() {
                    return Err(Error::Parse("title cannot be null".to_string()));
                }
                result.title = serde_json::from_value(value.clone())?;
            }
            "page_type" => {
                result.page_type = serde_json::from_value(value.clone()).map_err(|e| {
                    Error::Parse(format!("page_type: {e}"))
                })?;
            }
            "tags" => {
                result.tags = serde_json::from_value(value.clone())?;
            }
            "sources" => {
                result.sources = serde_json::from_value(value.clone())?;
            }
            "date" => {
                result.date = serde_json::from_value(value.clone())?;
            }
            "visibility" => {
                result.visibility = serde_json::from_value(value.clone())?;
            }
            "created_by" => {
                result.created_by = serde_json::from_value(value.clone())?;
            }
            _ => {}
        }
    }

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_valid() {
        let raw = "---\ntitle: Test Page\npage_type: Concept\ntags:\n  - rust\n  - types\nsources:\n  - https://example.com\ndate: '2024-01-01'\nvisibility: private\ncreated_by: alice\n---\n# Body starts here\n\nSome content.\n";

        let (fm, body) = parse(raw).unwrap();

        assert_eq!(fm.title, "Test Page");
        assert_eq!(fm.page_type, PageType::Concept);
        assert_eq!(fm.tags, vec!["rust", "types"]);
        assert_eq!(fm.sources, vec!["https://example.com"]);
        assert_eq!(fm.date, Some("2024-01-01".to_string()));
        assert_eq!(fm.visibility, "private");
        assert_eq!(fm.created_by, Some("alice".to_string()));
        assert_eq!(body, "# Body starts here\n\nSome content.\n");
    }

    #[test]
    fn test_parse_minimal() {
        let raw = "---\ntitle: Minimal Page\n---\nJust the body.\n";

        let (fm, body) = parse(raw).unwrap();

        assert_eq!(fm.title, "Minimal Page");
        assert_eq!(fm.page_type, PageType::Entity);
        assert!(fm.tags.is_empty());
        assert!(fm.sources.is_empty());
        assert_eq!(fm.date, None);
        assert_eq!(fm.visibility, "shared");
        assert_eq!(fm.created_by, None);
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
        assert_eq!(fm.page_type, PageType::Entity);
        assert!(fm.tags.is_empty());
        assert_eq!(fm.date, None);
        assert_eq!(fm.visibility, "shared");
        assert_eq!(fm.created_by, None);
        assert_eq!(body, "Body after empty frontmatter.\n");
    }

    #[test]
    fn test_parse_unknown_fields() {
        let raw = "---\ntitle: With Extras\npage_type: Entity\nunknown_field: 42\nanother_one:\n  nested: true\nvisibility: public\n---\nBody here.\n";

        let (fm, body) = parse(raw).unwrap();

        assert_eq!(fm.title, "With Extras");
        assert_eq!(fm.page_type, PageType::Entity);
        assert_eq!(fm.visibility, "public");
        assert_eq!(body, "Body here.\n");
    }

    #[test]
    fn test_roundtrip() {
        let raw = "---\ntitle: Roundtrip Test\npage_type: Synthesis\ntags:\n  - foo\nsources:\n  - baz\ndate: '2024-06-01'\nvisibility: private\ncreated_by: bob\n---\n# Body\n";

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
            sources: vec![],
            date: Some("2024-12-25".to_string()),
            visibility: "shared".to_string(),
            created_by: None,
        };

        let result = serialize(&fm).unwrap();

        assert!(result.starts_with("---\n"));
        assert!(result.ends_with("---\n"));
        assert!(result.contains("title: Serialize Me"));
        assert!(result.contains("page_type: Query"));
        assert!(result.contains("visibility: shared"));
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
    fn test_empty_sources_defaults_to_empty_vec() {
        let raw = "---\ntitle: Test\nsources: []\n---\nBody\n";
        let (fm, _) = parse(raw).unwrap();
        assert!(fm.sources.is_empty());
    }

    #[test]
    fn test_unicode_in_title_and_body() {
        let raw = "---\ntitle: \u{6d4b}\u{8bd5}\n---\n\u{8fd9}\u{662f}\u{4e00}\u{4e2a}\u{6d4b}\u{8bd5}\n";
        let (fm, body) = parse(&raw).unwrap();
        assert_eq!(fm.title, "\u{6d4b}\u{8bd5}");
        assert_eq!(body, "\u{8fd9}\u{662f}\u{4e00}\u{4e2a}\u{6d4b}\u{8bd5}\n");
    }

    fn sample_frontmatter() -> Frontmatter {
        Frontmatter {
            title: "Original Title".to_string(),
            page_type: PageType::Concept,
            tags: vec!["rust".to_string(), "types".to_string()],
            sources: vec!["https://example.com".to_string()],
            date: Some("2024-01-01".to_string()),
            visibility: "shared".to_string(),
            created_by: None,
        }
    }

    #[test]
    fn merge_empty_updates_returns_existing() {
        let existing = sample_frontmatter();
        let updates = serde_json::json!({});
        let result = merge_frontmatter(&existing, &updates).unwrap();
        assert_eq!(result, existing);
    }

    #[test]
    fn merge_title_only_rest_preserved() {
        let existing = sample_frontmatter();
        let updates = serde_json::json!({"title": "New Title"});
        let result = merge_frontmatter(&existing, &updates).unwrap();
        assert_eq!(result.title, "New Title");
        assert_eq!(result.page_type, PageType::Concept);
        assert_eq!(result.tags, vec!["rust", "types"]);
        assert_eq!(result.sources, vec!["https://example.com"]);
        assert_eq!(result.date, Some("2024-01-01".to_string()));
        assert_eq!(result.visibility, "shared");
        assert_eq!(result.created_by, None);
    }

    #[test]
    fn merge_tags_replaces_not_appends() {
        let existing = sample_frontmatter();
        let updates = serde_json::json!({"tags": ["new"]});
        let result = merge_frontmatter(&existing, &updates).unwrap();
        assert_eq!(result.tags, vec!["new"]);
        assert_eq!(result.title, "Original Title");
        assert_eq!(result.page_type, PageType::Concept);
    }

    #[test]
    fn merge_visibility_from_string() {
        let existing = sample_frontmatter();
        let updates = serde_json::json!({"visibility": "private"});
        let result = merge_frontmatter(&existing, &updates).unwrap();
        assert_eq!(result.visibility, "private");
    }

    #[test]
    fn merge_created_by_from_string() {
        let existing = sample_frontmatter();
        let updates = serde_json::json!({"created_by": "alice"});
        let result = merge_frontmatter(&existing, &updates).unwrap();
        assert_eq!(result.created_by, Some("alice".to_string()));
    }

    #[test]
    fn merge_null_title_errors() {
        let existing = sample_frontmatter();
        let updates = serde_json::json!({"title": null});
        let result = merge_frontmatter(&existing, &updates);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("title"), "error should mention title: {err}");
    }

    #[test]
    fn merge_unknown_field_ignored() {
        let existing = sample_frontmatter();
        let updates = serde_json::json!({"unknown_field": "x", "another": 42});
        let result = merge_frontmatter(&existing, &updates).unwrap();
        assert_eq!(result, existing);
    }

    #[test]
    fn merge_invalid_page_type_errors() {
        let existing = sample_frontmatter();
        let updates = serde_json::json!({"page_type": "InvalidType"});
        let result = merge_frontmatter(&existing, &updates);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("page_type"), "error should mention page_type: {err}");
    }

    #[test]
    fn merge_all_fields_at_once() {
        let existing = sample_frontmatter();
        let updates = serde_json::json!({
            "title": "All Updated",
            "page_type": "Synthesis",
            "tags": ["brand", "new"],
            "sources": ["https://new.com"],
            "date": "2025-06-01",
            "visibility": "private",
            "created_by": "bob"
        });
        let result = merge_frontmatter(&existing, &updates).unwrap();
        assert_eq!(result.title, "All Updated");
        assert_eq!(result.page_type, PageType::Synthesis);
        assert_eq!(result.tags, vec!["brand", "new"]);
        assert_eq!(result.sources, vec!["https://new.com"]);
        assert_eq!(result.date, Some("2025-06-01".to_string()));
        assert_eq!(result.visibility, "private");
        assert_eq!(result.created_by, Some("bob".to_string()));
    }

    #[test]
    fn merge_mixed_known_and_unknown_fields() {
        let existing = sample_frontmatter();
        let updates = serde_json::json!({
            "title": "Mixed",
            "unknown_field": "ignored",
            "visibility": "public"
        });
        let result = merge_frontmatter(&existing, &updates).unwrap();
        assert_eq!(result.title, "Mixed");
        assert_eq!(result.visibility, "public");
        assert_eq!(result.page_type, PageType::Concept);
        assert_eq!(result.tags, vec!["rust", "types"]);
    }

    #[test]
    fn stub_page_type_maps_to_entity() {
        let raw = "---\ntitle: Old Stub\npage_type: Stub\n---\nBody.\n";
        let (fm, body) = parse(raw).unwrap();
        assert_eq!(fm.title, "Old Stub");
        assert_eq!(fm.page_type, PageType::Entity);
        assert_eq!(body, "Body.\n");
    }

    #[test]
    fn old_status_field_silently_ignored_yaml() {
        let raw = "---\ntitle: With Status\npage_type: Entity\nstatus: Seedling\n---\nBody.\n";
        let (fm, body) = parse(raw).unwrap();
        assert_eq!(fm.title, "With Status");
        assert_eq!(fm.page_type, PageType::Entity);
        assert_eq!(body, "Body.\n");
    }

    #[test]
    fn old_related_field_silently_ignored_yaml() {
        let raw = "---\ntitle: With Related\npage_type: Entity\nrelated:\n  - foo\n  - bar\n---\nBody.\n";
        let (fm, body) = parse(raw).unwrap();
        assert_eq!(fm.title, "With Related");
        assert_eq!(fm.page_type, PageType::Entity);
        assert_eq!(body, "Body.\n");
    }

    #[test]
    fn all_old_fields_combined_ignored_yaml() {
        let raw = "---\ntitle: All Old\npage_type: Stub\nstatus: Seedling\nrelated:\n  - foo\n  - bar\n---\nBody.\n";
        let (fm, body) = parse(raw).unwrap();
        assert_eq!(fm.title, "All Old");
        assert_eq!(fm.page_type, PageType::Entity);
        assert_eq!(body, "Body.\n");
    }
}
