use sha2::{Digest, Sha256};

use crate::parser::frontmatter;
use crate::types::{Error, Page, Result, TimelineEntry};

const MAX_SLUG_LEN: usize = 255;

/// Parse raw markdown into a structured `Page`.
pub fn parse_page(raw_markdown: &str, slug: &str) -> Result<Page> {
    validate_slug(slug)?;

    let (frontmatter, body) = frontmatter::parse(raw_markdown)
        .map_err(|e| Error::Parse(format!("failed to parse frontmatter for page '{slug}': {e}")))?;
    let (compiled_truth, timeline) = split_body(&body);
    let content_hash = compute_hash(raw_markdown);

    Ok(Page {
        slug: slug.to_string(),
        frontmatter,
        compiled_truth,
        timeline,
        content_hash,
        raw_content: raw_markdown.to_string(),
    })
}

fn validate_slug(slug: &str) -> Result<()> {
    if slug.is_empty() {
        return Err(Error::Parse("slug must not be empty".to_string()));
    }
    if slug.len() > MAX_SLUG_LEN {
        return Err(Error::Parse(format!(
            "slug exceeds maximum length of {MAX_SLUG_LEN} characters (got {} characters)",
            slug.len()
        )));
    }
    if slug.contains("..") {
        return Err(Error::Parse(
            "slug must not contain '..' (path traversal)".to_string(),
        ));
    }
    if slug.starts_with('/') || slug.ends_with('/') {
        return Err(Error::Parse(
            "slug must not start or end with '/'".to_string(),
        ));
    }
    if !slug
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '/' || c == '.')
    {
        return Err(Error::Parse(format!(
            "slug '{slug}' contains invalid characters: only ASCII alphanumeric, '-', '_', '/', and '.' are allowed"
        )));
    }
    Ok(())
}

/// Serialize a `Page` back to markdown with frontmatter and timeline.
pub fn serialize_page(page: &Page) -> Result<String> {
    let mut output = frontmatter::serialize(&page.frontmatter)?;

    output.push_str(&page.compiled_truth);

    if !page.timeline.is_empty() {
        output.push_str("\n---\n");
        for entry in &page.timeline {
            output.push_str(&format_timeline_entry(entry));
            output.push('\n');
        }
    }

    Ok(output)
}

fn split_body(body: &str) -> (String, Vec<TimelineEntry>) {
    let lines: Vec<&str> = body.lines().collect();
    let separator_idx = lines.iter().position(|line| line.trim() == "---");

    match separator_idx {
        Some(idx) => {
            let truth = lines[..idx].join("\n");
            let timeline_section = lines[idx + 1..].join("\n");
            let timeline = parse_timeline(&timeline_section);
            (truth.trim().to_string(), timeline)
        }
        None => (body.trim().to_string(), Vec::new()),
    }
}

fn parse_timeline(section: &str) -> Vec<TimelineEntry> {
    let mut entries = Vec::new();

    for line in section.lines() {
        let line = line.trim();
        if !line.starts_with("- ") {
            continue;
        }

        let rest = &line[2..];

        if let Some(entry) = parse_timeline_line(rest) {
            entries.push(entry);
        }
    }

    entries
}

fn parse_timeline_line(rest: &str) -> Option<TimelineEntry> {
    if rest.len() < 10 {
        return None;
    }

    let date = &rest[..10];

    if !is_valid_date(date) {
        return None;
    }

    let after_date = rest[10..].trim_start();

    if after_date.starts_with('[') {
        if let Some(close) = after_date.find(']') {
            let url = &after_date[1..close];
            let after_url = after_date[close + 1..].trim_start();

            if let Some(content) = after_url.strip_prefix(':') {
                return Some(TimelineEntry {
                    date: date.to_string(),
                    source_url: Some(url.to_string()),
                    content: content.trim().to_string(),
                    agent: None,
                });
            }
        }
    }

    if let Some(content) = after_date.strip_prefix(':') {
        return Some(TimelineEntry {
            date: date.to_string(),
            source_url: None,
            content: content.trim().to_string(),
            agent: None,
        });
    }

    None
}

fn is_valid_date(s: &str) -> bool {
    if s.len() != 10 {
        return false;
    }
    let bytes = s.as_bytes();
    bytes[4] == b'-'
        && bytes[7] == b'-'
        && bytes[0..4].iter().all(|b| b.is_ascii_digit())
        && bytes[5..7].iter().all(|b| b.is_ascii_digit())
        && bytes[8..10].iter().all(|b| b.is_ascii_digit())
}

fn format_timeline_entry(entry: &TimelineEntry) -> String {
    match &entry.source_url {
        Some(url) => format!("- {} [{}]: {}", entry.date, url, entry.content),
        None => format!("- {}: {}", entry.date, entry.content),
    }
}

fn compute_hash(content: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    let result = hasher.finalize();
    hex_encode(&result)
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::wikilink;
    use crate::types::PageType;

    fn sample_markdown() -> &'static str {
        "\
---
title: Test Page
page_type: Concept
tags:
  - rust
  - types
related:
  - other-page
sources:
  - https://example.com
date: '2024-01-01'
status: Budding
---
This is the [[compiled-truth]] about the topic.

It references [[other-page]] too.
---
- 2024-01-01: First entry
- 2024-06-15 [https://source.com]: Second entry with source
"
    }

    fn sample_markdown_no_timeline() -> &'static str {
        "\
---
title: No Timeline
page_type: Entity
tags: []
related: []
sources: []
status: Seedling
---
Just the compiled truth.
"
    }

    fn sample_markdown_empty_truth() -> &'static str {
        "\
---
title: Empty Truth
page_type: Stub
tags: []
related: []
sources: []
status: Seedling
---
---
- 2024-01-01: Timeline only entry
"
    }

    #[test]
    fn test_full_roundtrip() {
        let raw = sample_markdown();
        let page = parse_page(raw, "test-page").unwrap();
        let serialized = serialize_page(&page).unwrap();
        let page2 = parse_page(&serialized, "test-page").unwrap();
        let serialized2 = serialize_page(&page2).unwrap();

        assert_eq!(page.slug, page2.slug);
        assert_eq!(page.frontmatter, page2.frontmatter);
        assert_eq!(page.compiled_truth, page2.compiled_truth);
        assert_eq!(page.timeline, page2.timeline);
        assert_eq!(serialized, serialized2);
    }

    #[test]
    fn test_no_timeline() {
        let raw = sample_markdown_no_timeline();
        let page = parse_page(raw, "no-timeline").unwrap();

        assert_eq!(page.compiled_truth, "Just the compiled truth.");
        assert!(page.timeline.is_empty());
    }

    #[test]
    fn test_empty_compiled_truth() {
        let raw = sample_markdown_empty_truth();
        let page = parse_page(raw, "empty-truth").unwrap();

        assert_eq!(page.compiled_truth, "");
        assert_eq!(page.timeline.len(), 1);
        assert_eq!(page.timeline[0].date, "2024-01-01");
        assert_eq!(page.timeline[0].content, "Timeline only entry");
    }

    #[test]
    fn test_extracts_wikilinks() {
        let raw = sample_markdown();
        let page = parse_page(raw, "test-page").unwrap();

        let links = wikilink::extract_links(&page.compiled_truth);
        assert_eq!(links.len(), 2);
        assert_eq!(links[0].target_slug, "compiled-truth");
        assert_eq!(links[1].target_slug, "other-page");
    }

    #[test]
    fn test_content_hash_deterministic() {
        let raw = sample_markdown();
        let page1 = parse_page(raw, "test-page").unwrap();
        let page2 = parse_page(raw, "test-page").unwrap();

        assert_eq!(page1.content_hash, page2.content_hash);
        assert_eq!(page1.content_hash.len(), 64);
    }

    #[test]
    fn test_timeline_entries() {
        let raw = sample_markdown();
        let page = parse_page(raw, "test-page").unwrap();

        assert_eq!(page.timeline.len(), 2);

        assert_eq!(page.timeline[0].date, "2024-01-01");
        assert_eq!(page.timeline[0].source_url, None);
        assert_eq!(page.timeline[0].content, "First entry");
        assert_eq!(page.timeline[0].agent, None);

        assert_eq!(page.timeline[1].date, "2024-06-15");
        assert_eq!(
            page.timeline[1].source_url,
            Some("https://source.com".to_string())
        );
        assert_eq!(page.timeline[1].content, "Second entry with source");
        assert_eq!(page.timeline[1].agent, None);
    }

    #[test]
    fn test_timeline_no_url() {
        let raw = "\
---
title: Test
page_type: Stub
tags: []
related: []
sources: []
status: Seedling
---
Truth.
---
- 2024-03-20: No URL entry
";
        let page = parse_page(raw, "no-url").unwrap();

        assert_eq!(page.timeline.len(), 1);
        assert_eq!(page.timeline[0].date, "2024-03-20");
        assert_eq!(page.timeline[0].source_url, None);
        assert_eq!(page.timeline[0].content, "No URL entry");
    }

    #[test]
    fn test_serialize_preserves_content() {
        let raw = sample_markdown();
        let page = parse_page(raw, "test-page").unwrap();
        let serialized = serialize_page(&page).unwrap();

        let page2 = parse_page(&serialized, "test-page").unwrap();
        assert_eq!(page.compiled_truth, page2.compiled_truth);
        assert_eq!(page.timeline.len(), page2.timeline.len());

        for (a, b) in page.timeline.iter().zip(page2.timeline.iter()) {
            assert_eq!(a.date, b.date);
            assert_eq!(a.source_url, b.source_url);
            assert_eq!(a.content, b.content);
        }
    }

    #[test]
    fn test_different_content_different_hash() {
        let raw1 = sample_markdown();
        let raw2 = sample_markdown_no_timeline();
        let page1 = parse_page(raw1, "a").unwrap();
        let page2 = parse_page(raw2, "b").unwrap();

        assert_ne!(page1.content_hash, page2.content_hash);
    }

    #[test]
    fn test_slug_preserved() {
        let raw = sample_markdown_no_timeline();
        let page = parse_page(raw, "my-slug").unwrap();
        assert_eq!(page.slug, "my-slug");
    }

    #[test]
    fn test_raw_content_preserved() {
        let raw = sample_markdown();
        let page = parse_page(raw, "test-page").unwrap();
        assert_eq!(page.raw_content, raw);
    }

    #[test]
    fn test_frontmatter_parsed() {
        let raw = sample_markdown();
        let page = parse_page(raw, "test-page").unwrap();

        assert_eq!(page.frontmatter.title, "Test Page");
        assert_eq!(page.frontmatter.page_type, PageType::Concept);
        assert_eq!(page.frontmatter.tags, vec!["rust", "types"]);
        assert_eq!(page.frontmatter.related, vec!["other-page"]);
        assert_eq!(page.frontmatter.sources, vec!["https://example.com"]);
        assert_eq!(page.frontmatter.date, Some("2024-01-01".to_string()));
        assert_eq!(page.frontmatter.status, crate::types::PageStatus::Budding);
    }

    #[test]
    fn test_serialize_roundtrip_preserves_hash() {
        let raw = sample_markdown();
        let page = parse_page(raw, "test-page").unwrap();

        let page2 = parse_page(raw, "test-page").unwrap();
        assert_eq!(page.content_hash, page2.content_hash);
    }

    #[test]
    fn test_empty_content_parses() {
        let raw = "---\ntitle: Empty\n---\n";
        let page = parse_page(raw, "empty-page").unwrap();
        assert_eq!(page.compiled_truth, "");
        assert!(page.timeline.is_empty());
    }

    #[test]
    fn test_empty_slug_rejected() {
        let raw = "---\ntitle: Test\n---\ncontent\n";
        let result = parse_page(raw, "");
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("slug must not be empty"), "expected empty slug error, got: {err}");
    }

    #[test]
    fn test_long_slug_rejected() {
        let raw = "---\ntitle: Test\n---\ncontent\n";
        let long_slug = "a".repeat(256);
        let result = parse_page(raw, &long_slug);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("exceeds maximum length"), "expected long slug error, got: {err}");
    }

    #[test]
    fn test_unicode_slug_rejected() {
        let raw = "---\ntitle: Test\n---\ncontent\n";
        let result = parse_page(raw, "unicode-\u{00e9}");
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("invalid characters"), "expected invalid slug error, got: {err}");
    }

    #[test]
    fn test_valid_slug_boundary_255() {
        let raw = "---\ntitle: Test\n---\ncontent\n";
        let slug_255 = "a".repeat(255);
        let result = parse_page(raw, &slug_255);
        assert!(result.is_ok(), "slug of exactly 255 chars should be valid");
    }

    #[test]
    fn test_path_slug_accepted() {
        let raw = "---\ntitle: Test\n---\ncontent\n";
        let result = parse_page(raw, "wiki/entities/hello.md");
        assert!(result.is_ok(), "path-style slug should be accepted");
        assert_eq!(result.unwrap().slug, "wiki/entities/hello.md");
    }

    #[test]
    fn test_path_traversal_rejected() {
        let raw = "---\ntitle: Test\n---\ncontent\n";
        let result = parse_page(raw, "../etc/passwd");
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("path traversal"),
            "expected path traversal error, got: {err}"
        );
    }

    #[test]
    fn test_leading_slash_rejected() {
        let raw = "---\ntitle: Test\n---\ncontent\n";
        let result = parse_page(raw, "/hello");
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("must not start or end with '/'"),
            "expected leading slash error, got: {err}"
        );
    }

    #[test]
    fn test_trailing_slash_rejected() {
        let raw = "---\ntitle: Test\n---\ncontent\n";
        let result = parse_page(raw, "hello/");
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("must not start or end with '/'"),
            "expected trailing slash error, got: {err}"
        );
    }
}
