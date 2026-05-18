use sha2::{Digest, Sha256};

use crate::parser::frontmatter;
use crate::types::{Error, Page, Result, TimelineEntry};

const MAX_SLUG_LEN: usize = 255;

/// Parse raw markdown into a structured `Page`.
pub fn parse_page(raw_markdown: &str, slug: &str) -> Result<Page> {
    validate_slug(slug)?;

    let (frontmatter, body) = frontmatter::parse(raw_markdown).map_err(|e| {
        Error::Parse(format!(
            "failed to parse frontmatter for page '{slug}': {e}"
        ))
    })?;
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

pub fn validate_slug(slug: &str) -> Result<()> {
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

/// Normalize a slug by stripping a trailing `.md` extension if present.
///
/// Returns an error if stripping `.md` would produce an empty string.
pub fn normalize_slug(slug: &str) -> Result<String> {
    match slug.strip_suffix(".md") {
        Some("") => Err(Error::Parse(
            "normalizing slug would produce an empty string".to_string(),
        )),
        Some(stripped) => Ok(stripped.to_string()),
        None => Ok(slug.to_string()),
    }
}

/// Convert a normalized slug to an FNS path by ensuring it ends with `.md`.
///
/// If the slug already ends with `.md`, it is returned unchanged.
pub fn to_fns_path(slug: &str) -> String {
    if slug.ends_with(".md") {
        slug.to_string()
    } else {
        format!("{slug}.md")
    }
}

/// Serialize a `Page` back to markdown with frontmatter and timeline.
pub fn serialize_page(page: &Page) -> Result<String> {
    let mut output = frontmatter::serialize(&page.frontmatter)?;

    output.push_str(&page.compiled_truth);
    output.push_str("\n\n---\n");

    for entry in &page.timeline {
        output.push_str(&format_timeline_entry(entry));
        output.push('\n');
    }

    Ok(output)
}

fn split_body(body: &str) -> (String, Vec<TimelineEntry>) {
    let lines: Vec<&str> = body.lines().collect();
    let mut last_separator_idx = None;
    let mut in_code_fence = false;
    let mut fence_char = b'\0';
    let mut fence_count = 0usize;

    for (idx, line) in lines.iter().enumerate() {
        if in_code_fence {
            if let Some((c, count)) = parse_fence(line) {
                if c == fence_char && count >= fence_count {
                    in_code_fence = false;
                }
            }
            continue;
        }

        if let Some((c, count)) = parse_fence(line) {
            in_code_fence = true;
            fence_char = c;
            fence_count = count;
            continue;
        }

        if line.trim() == "---" {
            last_separator_idx = Some(idx);
        }
    }

    match last_separator_idx {
        Some(idx) => {
            let truth = lines[..idx].join("\n");
            let timeline_section = lines[idx + 1..].join("\n");
            let timeline = parse_timeline(&timeline_section);
            (truth.trim().to_string(), timeline)
        }
        None => (body.trim().to_string(), Vec::new()),
    }
}

fn parse_fence(line: &str) -> Option<(u8, usize)> {
    let trimmed = line.trim_start();
    let bytes = trimmed.as_bytes();

    if bytes.len() < 3 {
        return None;
    }

    let c = bytes[0];
    if c != b'`' && c != b'~' {
        return None;
    }

    let count = bytes.iter().take_while(|&&b| b == c).count();
    if count < 3 {
        return None;
    }

    let rest = &trimmed[count..];
    if rest.contains(c as char) {
        return None;
    }

    Some((c, count))
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
            let bracket_content = &after_date[1..close];
            let after_bracket = after_date[close + 1..].trim_start();

            if let Some(content) = after_bracket.strip_prefix(':') {
                let is_url = bracket_content.starts_with("http://")
                    || bracket_content.starts_with("https://");

                if is_url {
                    return Some(TimelineEntry {
                        date: date.to_string(),
                        source_url: Some(bracket_content.to_string()),
                        content: content.trim().to_string(),
                        agent: None,
                    });
                } else {
                    return Some(TimelineEntry {
                        date: date.to_string(),
                        source_url: None,
                        content: content.trim().to_string(),
                        agent: Some(bracket_content.to_string()),
                    });
                }
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
    match (&entry.source_url, &entry.agent) {
        (Some(url), _) => format!("- {} [{}]: {}", entry.date, url, entry.content),
        (None, Some(agent)) => format!("- {} [{}]: {}", entry.date, agent, entry.content),
        (None, None) => format!("- {}: {}", entry.date, entry.content),
    }
}

pub(crate) fn compute_hash(content: &str) -> String {
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
sources:
  - https://example.com
date: '2024-01-01'
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
sources: []
---
Just the compiled truth.
"
    }

    fn sample_markdown_empty_truth() -> &'static str {
        "\
---
title: Empty Truth
page_type: Entity
tags: []
sources: []
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
page_type: Entity
tags: []
sources: []
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
        assert_eq!(page.frontmatter.sources, vec!["https://example.com"]);
        assert_eq!(page.frontmatter.date, Some("2024-01-01".to_string()));
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
        assert!(
            err.contains("slug must not be empty"),
            "expected empty slug error, got: {err}"
        );
    }

    #[test]
    fn test_long_slug_rejected() {
        let raw = "---\ntitle: Test\n---\ncontent\n";
        let long_slug = "a".repeat(256);
        let result = parse_page(raw, &long_slug);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("exceeds maximum length"),
            "expected long slug error, got: {err}"
        );
    }

    #[test]
    fn test_unicode_slug_rejected() {
        let raw = "---\ntitle: Test\n---\ncontent\n";
        let result = parse_page(raw, "unicode-\u{00e9}");
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("invalid characters"),
            "expected invalid slug error, got: {err}"
        );
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

    #[test]
    fn test_normalize_slug() {
        assert_eq!(normalize_slug("wiki/hello.md").unwrap(), "wiki/hello");
        assert_eq!(normalize_slug("wiki/hello").unwrap(), "wiki/hello");
        assert_eq!(normalize_slug("readme.md.md").unwrap(), "readme.md");
        assert_eq!(
            normalize_slug("wiki/v2.0-notes.md").unwrap(),
            "wiki/v2.0-notes"
        );
    }

    #[test]
    fn test_normalize_slug_errors() {
        assert!(normalize_slug(".md").is_err());
    }

    #[test]
    fn test_to_fns_path() {
        assert_eq!(to_fns_path("wiki/hello"), "wiki/hello.md");
        assert_eq!(to_fns_path("wiki/hello.md"), "wiki/hello.md");
    }

    #[test]
    fn test_validate_slug_is_pub() {
        assert!(validate_slug("valid-slug").is_ok());
        assert!(validate_slug("").is_err());
    }

    #[test]
    fn test_split_body_last_separator_wins() {
        let raw = "\
---
title: Test
page_type: Concept
tags: []
sources: []
---
Content above the hr.

---

More content below the hr.

---

- 2024-01-01: Timeline entry
";
        let page = parse_page(raw, "last-sep").unwrap();

        assert!(
            page.compiled_truth.contains("Content above the hr."),
            "compiled_truth should contain content above first hr"
        );
        assert!(
            page.compiled_truth.contains("More content below the hr."),
            "compiled_truth should contain content below first hr but above separator"
        );
        assert_eq!(page.timeline.len(), 1);
        assert_eq!(page.timeline[0].date, "2024-01-01");
    }

    #[test]
    fn test_split_body_ignores_separator_in_backtick_fence() {
        let raw = "\
---
title: Fenced
page_type: Concept
tags: []
sources: []
---
Some content.

```markdown
---
```

Real separator here.

---

- 2024-06-15: Real timeline
";
        let page = parse_page(raw, "fenced-bt").unwrap();

        assert!(
            page.compiled_truth.contains("```markdown"),
            "compiled_truth should contain the code fence"
        );
        assert!(
            page.compiled_truth.contains("Real separator here."),
            "compiled_truth should contain text between fence and real separator"
        );
        assert_eq!(page.timeline.len(), 1);
        assert_eq!(page.timeline[0].date, "2024-06-15");
    }

    #[test]
    fn test_split_body_ignores_separator_in_tilde_fence() {
        let raw = "\
---
title: Tilde Fence
page_type: Concept
tags: []
sources: []
---
Content.

~~~
---
~~~

After fence.

---

- 2024-07-20: Tilde test
";
        let page = parse_page(raw, "fenced-tilde").unwrap();

        assert!(
            page.compiled_truth.contains("After fence."),
            "compiled_truth should contain text between fence and real separator"
        );
        assert_eq!(page.timeline.len(), 1);
        assert_eq!(page.timeline[0].date, "2024-07-20");
    }

    #[test]
    fn test_body_no_separator_all_truth() {
        let raw = "\
---
title: No Sep
page_type: Concept
tags: []
sources: []
---
Just some content without any separator.
";
        let page = parse_page(raw, "no-sep").unwrap();

        assert_eq!(
            page.compiled_truth,
            "Just some content without any separator."
        );
        assert!(page.timeline.is_empty());
    }

    #[test]
    fn test_serialize_always_emits_separator() {
        let raw = "\
---
title: Empty Timeline
page_type: Concept
tags: []
sources: []
---
Some truth.
";
        let page = parse_page(raw, "always-sep").unwrap();
        assert!(page.timeline.is_empty());

        let serialized = serialize_page(&page).unwrap();
        assert!(
            serialized.ends_with("Some truth.\n\n---\n"),
            "serialized page should end with --- separator after compiled_truth, got:\n{serialized}"
        );
    }

    #[test]
    fn test_serialize_roundtrip_empty_timeline() {
        let raw = "\
---
title: RT Empty
page_type: Concept
tags: []
sources: []
---
Some truth.
";
        let page = parse_page(raw, "rt-empty").unwrap();
        let serialized = serialize_page(&page).unwrap();
        let page2 = parse_page(&serialized, "rt-empty").unwrap();
        let serialized2 = serialize_page(&page2).unwrap();

        assert_eq!(page.compiled_truth, page2.compiled_truth);
        assert_eq!(page.timeline.len(), page2.timeline.len());
        assert_eq!(serialized, serialized2);
    }

    #[test]
    fn test_split_body_no_separator_with_code_fence() {
        let raw = "\
---
title: Fence No Sep
page_type: Concept
tags: []
sources: []
---
```rust
fn main() {}
```
";
        let page = parse_page(raw, "fence-no-sep").unwrap();

        assert!(page.compiled_truth.contains("```rust"));
        assert!(page.timeline.is_empty());
    }

    #[test]
    fn test_unclosed_code_fence_treats_rest_as_inside() {
        let raw = "\
---
title: Unclosed
page_type: Concept
tags: []
sources: []
---
Before fence.

```
---
- 2024-01-01: Not a timeline
";
        let page = parse_page(raw, "unclosed").unwrap();

        assert!(
            page.timeline.is_empty(),
            "unclosed fence should hide --- and timeline entries"
        );
        assert!(
            page.compiled_truth.contains("Before fence."),
            "compiled_truth should contain text before the fence"
        );
    }

    #[test]
    fn test_timeline_agent_parsed() {
        let raw = "\
---
title: Agent Test
page_type: Concept
tags: []
sources: []
---
---
- 2026-05-09 [claude]: update
";
        let page = parse_page(raw, "agent-test").unwrap();

        assert_eq!(page.timeline.len(), 1);
        assert_eq!(page.timeline[0].date, "2026-05-09");
        assert_eq!(page.timeline[0].content, "update");
        assert_eq!(page.timeline[0].agent, Some("claude".to_string()));
        assert_eq!(page.timeline[0].source_url, None);
    }

    #[test]
    fn test_timeline_source_url_still_parsed() {
        let raw = "\
---
title: URL Test
page_type: Concept
tags: []
sources: []
---
---
- 2024-06-15 [https://source.com]: entry
";
        let page = parse_page(raw, "url-test").unwrap();

        assert_eq!(page.timeline.len(), 1);
        assert_eq!(page.timeline[0].date, "2024-06-15");
        assert_eq!(page.timeline[0].content, "entry");
        assert_eq!(
            page.timeline[0].source_url,
            Some("https://source.com".to_string())
        );
        assert_eq!(page.timeline[0].agent, None);
    }

    #[test]
    fn test_format_timeline_entry_with_agent() {
        let entry = TimelineEntry {
            date: "2026-05-09".to_string(),
            content: "update".to_string(),
            agent: Some("claude".to_string()),
            source_url: None,
        };
        assert_eq!(
            format_timeline_entry(&entry),
            "- 2026-05-09 [claude]: update"
        );
    }

    #[test]
    fn test_format_timeline_entry_with_source_url() {
        let entry = TimelineEntry {
            date: "2024-06-15".to_string(),
            content: "entry".to_string(),
            agent: None,
            source_url: Some("https://source.com".to_string()),
        };
        assert_eq!(
            format_timeline_entry(&entry),
            "- 2024-06-15 [https://source.com]: entry"
        );
    }

    #[test]
    fn test_agent_roundtrip() {
        let raw = "\
---
title: Roundtrip
page_type: Concept
tags: []
sources: []
---
---
- 2026-05-09 [claude]: update
";
        let page = parse_page(raw, "roundtrip").unwrap();
        let serialized = serialize_page(&page).unwrap();
        let page2 = parse_page(&serialized, "roundtrip").unwrap();

        assert_eq!(page.timeline.len(), 1);
        assert_eq!(page2.timeline.len(), 1);
        assert_eq!(page.timeline[0].agent, page2.timeline[0].agent);
        assert_eq!(page.timeline[0].agent, Some("claude".to_string()));
    }
}
