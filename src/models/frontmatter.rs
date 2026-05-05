use chrono::NaiveDate;
use regex::Regex;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::sync::LazyLock;

use crate::models::{Page, PageStatus, PageType, TimelineEntry};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Frontmatter {
    pub r#type: PageType,
    pub title: String,
    pub tags: Vec<String>,
    pub related: Vec<String>,
    pub sources: Vec<String>,
    pub date: NaiveDate,
    pub status: Option<PageStatus>,
}

// Regex for --- separator lines (frontmatter start/end, timeline separator)
static SEPARATOR_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?m)^---\s*$").unwrap());

// Regex for timeline entries: "- YYYY-MM-DD: content"
static TIMELINE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^-\s*(\d{4}-\d{2}-\d{2}):\s*(.*)$").unwrap());

// Regex for markdown links: [text](url)
static LINK_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\[([^\]]*)\]\(([^)]*)\)").unwrap());

/// Parse a complete markdown page into Page struct.
///
/// Splitting rules (v1 spec §4.1):
/// - First `---` to second `---` = YAML frontmatter
/// - After second `---` to last `---` (or EOF) = Compiled Truth
/// - After last `---` = Timeline (bullet list)
/// - If no second `---` after frontmatter: entire body = Compiled Truth, no Timeline
pub fn parse_page(raw_content: &str, slug: &str, vault: &str) -> anyhow::Result<Page> {
    let (fm_raw, ct, tl_raw) = split_sections(raw_content);

    let frontmatter = match fm_raw {
        Some(yaml) => parse_frontmatter(yaml)?,
        None => {
            anyhow::bail!("No frontmatter found: page must contain at least two '---' separators")
        }
    };

    let timeline = tl_raw.map(parse_timeline).unwrap_or_default();
    let content_hash = compute_hash(raw_content);

    Ok(Page {
        slug: slug.to_string(),
        vault: vault.to_string(),
        frontmatter,
        compiled_truth: ct.trim().to_string(),
        timeline,
        content_hash,
        raw_content: raw_content.to_string(),
    })
}

/// Parse YAML frontmatter string into Frontmatter struct.
pub fn parse_frontmatter(yaml: &str) -> anyhow::Result<Frontmatter> {
    serde_yaml::from_str(yaml.trim()).map_err(Into::into)
}

/// Parse timeline text into a vector of TimelineEntry.
///
/// Each line should match `- YYYY-MM-DD: content`.
/// Malformed lines and lines with invalid dates are skipped.
/// Markdown links `[text](url)` in the content are extracted as `source_url`.
pub fn parse_timeline(text: &str) -> Vec<TimelineEntry> {
    let mut entries = Vec::new();

    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let Some(caps) = TIMELINE_RE.captures(line) else {
            continue;
        };

        let date_str = caps.get(1).map(|m| m.as_str()).unwrap_or("");
        let content_str = caps.get(2).map(|m| m.as_str()).unwrap_or("");

        let Ok(date) = NaiveDate::parse_from_str(date_str, "%Y-%m-%d") else {
            continue;
        };

        let mut source_url = None;
        let mut cleaned_content = content_str.to_string();

        if let Some(link_caps) = LINK_RE.captures(content_str) {
            source_url = link_caps.get(2).map(|m| m.as_str().to_string());
            if let Some(link_text) = link_caps.get(1) {
                cleaned_content = LINK_RE.replace(content_str, link_text.as_str()).to_string();
            }
        }

        entries.push(TimelineEntry {
            date,
            source_url,
            content: cleaned_content.trim().to_string(),
        });
    }

    entries
}

/// Compute SHA256 hash of raw content, prefixed with "sha256:".
pub fn compute_hash(content: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    format!("sha256:{:x}", hasher.finalize())
}

/// Split raw markdown into (frontmatter, compiled_truth, timeline) sections.
fn split_sections(raw: &str) -> (Option<&str>, &str, Option<&str>) {
    let matches: Vec<_> = SEPARATOR_RE.find_iter(raw).collect();

    if matches.len() < 2 {
        return (None, raw, None);
    }

    let frontmatter = raw[matches[0].end()..matches[1].start()].trim();
    let after_frontmatter = matches[1].end();

    if matches.len() == 2 {
        let ct = raw[after_frontmatter..].trim();
        return (Some(frontmatter), ct, None);
    }

    let last_sep = &matches[matches.len() - 1];
    let ct = raw[after_frontmatter..last_sep.start()].trim();
    let tl = raw[last_sep.end()..].trim();

    (Some(frontmatter), ct, Some(tl))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frontmatter_serde_roundtrip() {
        let fm = Frontmatter {
            r#type: PageType::Concept,
            title: "Test Concept".to_string(),
            tags: vec!["rust".to_string(), "serde".to_string()],
            related: vec!["entities/fns".to_string()],
            sources: vec![],
            date: NaiveDate::from_ymd_opt(2026, 5, 5).unwrap(),
            status: Some(PageStatus::Budding),
        };

        let json = serde_json::to_string(&fm).unwrap();
        let restored: Frontmatter = serde_json::from_str(&json).unwrap();

        assert_eq!(fm.title, restored.title);
        assert_eq!(fm.tags, restored.tags);
        assert_eq!(fm.related, restored.related);
        assert_eq!(fm.sources, restored.sources);
        assert_eq!(fm.date, restored.date);
        assert_eq!(fm.status, restored.status);
    }

    #[test]
    fn frontmatter_with_none_status() {
        let fm = Frontmatter {
            r#type: PageType::Source,
            title: "Raw Source".to_string(),
            tags: vec![],
            related: vec![],
            sources: vec!["2026-05-05-rss".to_string()],
            date: NaiveDate::from_ymd_opt(2026, 5, 5).unwrap(),
            status: None,
        };

        let json = serde_json::to_string(&fm).unwrap();
        let restored: Frontmatter = serde_json::from_str(&json).unwrap();

        assert_eq!(fm.status, restored.status);
    }

    #[test]
    fn compute_hash_basic() {
        let h1 = compute_hash("hello");
        let h2 = compute_hash("hello");
        let h3 = compute_hash("world");

        assert!(h1.starts_with("sha256:"));
        assert_eq!(h1, h2);
        assert_ne!(h1, h3);
        assert_eq!(h1.len(), 7 + 64);
    }

    #[test]
    fn compute_hash_empty() {
        let h = compute_hash("");
        assert!(h.starts_with("sha256:"));
        assert_eq!(h.len(), 7 + 64);
    }

    #[test]
    fn parse_frontmatter_full() {
        let yaml = r#"
type: entity
title: Test Entity
tags: [rust, ai]
related: [concepts/llm]
sources: [2026-05-05-rss]
date: 2026-05-05
status: budding
"#;
        let fm = parse_frontmatter(yaml).unwrap();
        assert!(matches!(fm.r#type, PageType::Entity));
        assert_eq!(fm.title, "Test Entity");
        assert_eq!(fm.tags, vec!["rust", "ai"]);
        assert_eq!(fm.related, vec!["concepts/llm"]);
        assert_eq!(fm.sources, vec!["2026-05-05-rss"]);
        assert_eq!(fm.date, NaiveDate::from_ymd_opt(2026, 5, 5).unwrap());
        assert_eq!(fm.status, Some(PageStatus::Budding));
    }

    #[test]
    fn parse_frontmatter_missing_optional() {
        let yaml = r#"
type: concept
title: Minimal
tags: []
related: []
sources: []
date: 2026-01-01
"#;
        let fm = parse_frontmatter(yaml).unwrap();
        assert_eq!(fm.status, None);
        assert_eq!(fm.tags, Vec::<String>::new());
    }

    #[test]
    fn parse_timeline_basic() {
        let text = r#"
- 2026-05-01: Initial record
- 2026-05-03: Update with more info
"#;
        let entries = parse_timeline(text);
        assert_eq!(entries.len(), 2);
        assert_eq!(
            entries[0].date,
            NaiveDate::from_ymd_opt(2026, 5, 1).unwrap()
        );
        assert_eq!(entries[0].content, "Initial record");
        assert_eq!(entries[0].source_url, None);
        assert_eq!(
            entries[1].date,
            NaiveDate::from_ymd_opt(2026, 5, 3).unwrap()
        );
        assert_eq!(entries[1].content, "Update with more info");
    }

    #[test]
    fn parse_timeline_with_links() {
        let text = "- 2026-05-01: [Source](https://example.com) Initial record";
        let entries = parse_timeline(text);
        assert_eq!(entries.len(), 1);
        assert_eq!(
            entries[0].source_url,
            Some("https://example.com".to_string())
        );
        assert_eq!(entries[0].content, "Source Initial record");
    }

    #[test]
    fn parse_timeline_malformed_date_skipped() {
        let text = r#"
- 2026-05-01: Good entry
- not-a-date: Bad entry
- 2026-13-45: Invalid date
- 2026-05-02: Another good entry
"#;
        let entries = parse_timeline(text);
        assert_eq!(entries.len(), 2);
        assert_eq!(
            entries[0].date,
            NaiveDate::from_ymd_opt(2026, 5, 1).unwrap()
        );
        assert_eq!(
            entries[1].date,
            NaiveDate::from_ymd_opt(2026, 5, 2).unwrap()
        );
    }

    #[test]
    fn parse_timeline_empty_and_whitespace() {
        let text = "\n\n  \n";
        let entries = parse_timeline(text);
        assert!(entries.is_empty());
    }

    #[test]
    fn parse_timeline_no_entries() {
        let text = "Just some text without timeline format.";
        let entries = parse_timeline(text);
        assert!(entries.is_empty());
    }

    fn sample_full_page() -> &'static str {
        r#"---
type: entity
title: Test Entity
tags: [rust]
related: [concepts/demo]
sources: []
date: 2026-05-05
status: seedling
---
# Compiled Truth

This is the best understanding.

---
- 2026-05-01: [Source](https://example.com) Initial record
- 2026-05-03: Update
"#
    }

    #[test]
    fn parse_page_full() {
        let page = parse_page(sample_full_page(), "wiki/entities/test", "forge").unwrap();

        assert_eq!(page.slug, "wiki/entities/test");
        assert_eq!(page.vault, "forge");
        assert!(matches!(page.frontmatter.r#type, PageType::Entity));
        assert_eq!(page.frontmatter.title, "Test Entity");
        assert_eq!(
            page.compiled_truth,
            "# Compiled Truth\n\nThis is the best understanding."
        );
        assert_eq!(page.timeline.len(), 2);
        assert_eq!(
            page.timeline[0].date,
            NaiveDate::from_ymd_opt(2026, 5, 1).unwrap()
        );
        assert_eq!(
            page.timeline[0].source_url,
            Some("https://example.com".to_string())
        );
        assert_eq!(page.timeline[0].content, "Source Initial record");
        assert_eq!(page.timeline[1].content, "Update");
        assert!(page.content_hash.starts_with("sha256:"));
        assert!(!page.raw_content.is_empty());
    }

    #[test]
    fn parse_page_no_timeline() {
        let raw = r#"---
type: concept
title: No Timeline
tags: []
related: []
sources: []
date: 2026-05-05
---
# Just compiled truth

No timeline here.
"#;
        let page = parse_page(raw, "wiki/concepts/no-tl", "forge").unwrap();
        assert_eq!(
            page.compiled_truth,
            "# Just compiled truth\n\nNo timeline here."
        );
        assert!(page.timeline.is_empty());
    }

    #[test]
    fn parse_page_empty_ct() {
        let raw = r#"---
type: concept
title: Empty CT
tags: []
related: []
sources: []
date: 2026-05-05
---
---
- 2026-05-01: Entry
"#;
        let page = parse_page(raw, "wiki/concepts/empty-ct", "forge").unwrap();
        assert!(page.compiled_truth.is_empty());
        assert_eq!(page.timeline.len(), 1);
    }

    #[test]
    fn parse_page_multiple_separators_in_ct() {
        let raw = r#"---
type: entity
title: With Rules
tags: []
related: []
sources: []
date: 2026-05-05
---
# Title

Some text.

---

More text after a horizontal rule.

---
- 2026-05-01: Entry
"#;
        let page = parse_page(raw, "wiki/entities/rules", "forge").unwrap();
        assert!(page.compiled_truth.contains("Some text."));
        assert!(
            page.compiled_truth
                .contains("More text after a horizontal rule.")
        );
        assert_eq!(page.timeline.len(), 1);
    }

    #[test]
    fn parse_page_no_frontmatter_fails() {
        let raw = "# Just a markdown file\n\nNo frontmatter.\n";
        let result = parse_page(raw, "wiki/no-fm", "forge");
        assert!(result.is_err());
    }

    #[test]
    fn parse_page_missing_optional_fields() {
        let raw = r#"---
type: source
title: Raw Source
tags: []
related: []
sources: [2026-05-05-rss]
date: 2026-05-05
---
The raw content.
"#;
        let page = parse_page(raw, "wiki/sources/raw", "forge").unwrap();
        assert_eq!(page.frontmatter.status, None);
        assert_eq!(page.frontmatter.tags, Vec::<String>::new());
    }
}
