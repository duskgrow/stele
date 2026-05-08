use crate::types::{Link, LinkType};

/// Extract all wikilinks from markdown content.
///
/// Parses `[[target]]`, `[[type::target]]`, and `[[target|display]]` syntax.
/// Skips links inside fenced code blocks (```) and inline code (`).
/// source_slug is left empty — the caller fills it in.
pub fn extract_links(content: &str) -> Vec<Link> {
    let bytes = content.as_bytes();
    let len = bytes.len();
    let mut links = Vec::new();
    let mut i = 0;

    let mut in_fenced_block = false;
    let mut in_inline_code = false;

    while i < len {
        if bytes[i] == b'`' {
            if !in_inline_code && is_fence_start(bytes, i, len) {
                in_fenced_block = !in_fenced_block;
                while i < len && bytes[i] != b'\n' {
                    i += 1;
                }
                continue;
            }

            // Single backtick toggles inline code (only outside fenced blocks)
            if !in_fenced_block {
                in_inline_code = !in_inline_code;
                i += 1;
                continue;
            }
        }

        if in_fenced_block || in_inline_code {
            i += 1;
            continue;
        }

        if i + 1 < len && bytes[i] == b'[' && bytes[i + 1] == b'[' {
            let start = i + 2;
            let mut close_pos = None;

            let mut j = start;
            while j + 1 < len {
                if bytes[j] == b']' && bytes[j + 1] == b']' {
                    close_pos = Some(j);
                    break;
                }
                // Nested `[[` before `]]` → abort (no nested wikilinks)
                if bytes[j] == b'[' && j + 1 < len && bytes[j + 1] == b'[' {
                    break;
                }
                j += 1;
            }

            if let Some(end) = close_pos {
                let inner = &content[start..end];
                if let Some(link) = parse_wikilink(inner) {
                    links.push(link);
                }
                i = end + 2;
            } else {
                i += 1;
            }
        } else {
            i += 1;
        }
    }

    links
}

pub fn extract_links_for_page(content: &str, source_slug: &str) -> Vec<Link> {
    let mut links = extract_links(content);
    for link in &mut links {
        link.source_slug = source_slug.to_string();
    }
    links.retain(|link| link.target_slug != source_slug);
    links
}

fn is_fence_start(bytes: &[u8], i: usize, _len: usize) -> bool {
    let mut count = 0;
    let mut pos = i;
    while pos < bytes.len() && bytes[pos] == b'`' {
        count += 1;
        pos += 1;
    }
    if count < 3 {
        return false;
    }

    if i > 0 && bytes[i - 1] != b'\n' {
        return false;
    }

    let mut check = i;
    while check > 0 {
        check -= 1;
        if bytes[check] == b'\n' {
            break;
        }
        if !bytes[check].is_ascii_whitespace() {
            return false;
        }
    }

    true
}

fn parse_wikilink(inner: &str) -> Option<Link> {
    let inner = inner.trim();

    if inner.is_empty() {
        return None;
    }

    let (body, display) = match inner.find('|') {
        Some(pos) => {
            let d = inner[pos + 1..].trim();
            let d = if d.is_empty() { None } else { Some(d.to_string()) };
            (&inner[..pos], d)
        }
        None => (inner, None),
    };

    let body = body.trim();

    if body.is_empty() {
        return None;
    }

    let (link_type, target_slug) = match body.find("::") {
        Some(pos) => {
            let type_str = body[..pos].trim();
            let target = body[pos + 2..].trim();

            if type_str.is_empty() || target.is_empty() {
                return None;
            }

            (LinkType::Custom(type_str.to_string()), target.to_string())
        }
        None => (LinkType::Plain, body.to_string()),
    };

    if target_slug.is_empty() {
        return None;
    }

    Some(Link {
        source_slug: String::new(),
        target_slug,
        link_type,
        context_snippet: display,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plain_link() {
        let links = extract_links("Check out [[my-page]] for details.");
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].target_slug, "my-page");
        assert_eq!(links[0].link_type, LinkType::Plain);
        assert_eq!(links[0].context_snippet, None);
        assert_eq!(links[0].source_slug, "");
    }

    #[test]
    fn test_typed_link() {
        let links = extract_links("See [[cites::paper-2024]] for the reference.");
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].target_slug, "paper-2024");
        assert_eq!(links[0].link_type, LinkType::Custom("cites".to_string()));
        assert_eq!(links[0].context_snippet, None);
    }

    #[test]
    fn test_aliased_link() {
        let links = extract_links("[[my-page|display text]] is here.");
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].target_slug, "my-page");
        assert_eq!(links[0].link_type, LinkType::Plain);
        assert_eq!(links[0].context_snippet, Some("display text".to_string()));
    }

    #[test]
    fn test_typed_aliased_link() {
        let links = extract_links("[[cites::paper-2024|The Paper]]");
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].target_slug, "paper-2024");
        assert_eq!(links[0].link_type, LinkType::Custom("cites".to_string()));
        assert_eq!(links[0].context_snippet, Some("The Paper".to_string()));
    }

    #[test]
    fn test_multiple_links() {
        let links = extract_links("First [[page-a]] and second [[page-b]] and third [[type::page-c]].");
        assert_eq!(links.len(), 3);
        assert_eq!(links[0].target_slug, "page-a");
        assert_eq!(links[1].target_slug, "page-b");
        assert_eq!(links[2].target_slug, "page-c");
        assert_eq!(links[2].link_type, LinkType::Custom("type".to_string()));
    }

    #[test]
    fn test_skip_fenced_code_block() {
        let content = "Before [[real-link]]\n\n```\n[[not-a-link]]\n```\n\nAfter [[also-real]]";
        let links = extract_links(content);
        assert_eq!(links.len(), 2);
        assert_eq!(links[0].target_slug, "real-link");
        assert_eq!(links[1].target_slug, "also-real");
    }

    #[test]
    fn test_skip_fenced_code_block_with_language() {
        let content = "Before [[real-link]]\n\n```rust\n[[not-a-link]]\n```\n\nAfter";
        let links = extract_links(content);
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].target_slug, "real-link");
    }

    #[test]
    fn test_skip_inline_code() {
        let content = "Use `[[not-a-link]]` and also [[real-link]].";
        let links = extract_links(content);
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].target_slug, "real-link");
    }

    #[test]
    fn test_malformed_empty() {
        let links = extract_links("Nothing [[]] here.");
        assert_eq!(links.len(), 0);
    }

    #[test]
    fn test_malformed_unclosed() {
        let links = extract_links("Broken [[target and more text.");
        assert_eq!(links.len(), 0);
    }

    #[test]
    fn test_no_links() {
        let links = extract_links("Just plain text with no wikilinks.");
        assert_eq!(links.len(), 0);
    }

    #[test]
    fn test_links_with_spaces() {
        let links = extract_links("[[target name]]");
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].target_slug, "target name");
    }

    #[test]
    fn test_complex_content() {
        let content = r#"# My Document

Here is a [[plain-link]] and a [[type::typed-link|With Display]].

```markdown
[[inside-code-block]]
```

And `[[inline-code]]` should be skipped.

But [[another-link|display]] should work.

Some `code` and [[final-link]] at the end."#;

        let links = extract_links(content);
        assert_eq!(links.len(), 4);
        assert_eq!(links[0].target_slug, "plain-link");
        assert_eq!(links[0].link_type, LinkType::Plain);
        assert_eq!(links[1].target_slug, "typed-link");
        assert_eq!(links[1].link_type, LinkType::Custom("type".to_string()));
        assert_eq!(links[1].context_snippet, Some("With Display".to_string()));
        assert_eq!(links[2].target_slug, "another-link");
        assert_eq!(links[2].context_snippet, Some("display".to_string()));
        assert_eq!(links[3].target_slug, "final-link");
    }

    #[test]
    fn test_link_at_start_and_end() {
        let links = extract_links("[[first]] middle [[last]]");
        assert_eq!(links.len(), 2);
        assert_eq!(links[0].target_slug, "first");
        assert_eq!(links[1].target_slug, "last");
    }

    #[test]
    fn test_adjacent_links() {
        let links = extract_links("[[a]][[b]]");
        assert_eq!(links.len(), 2);
        assert_eq!(links[0].target_slug, "a");
        assert_eq!(links[1].target_slug, "b");
    }

    #[test]
    fn test_type_with_display_alias() {
        let links = extract_links("[[authors::smith2024|Smith et al.]]");
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].target_slug, "smith2024");
        assert_eq!(links[0].link_type, LinkType::Custom("authors".to_string()));
        assert_eq!(links[0].context_snippet, Some("Smith et al.".to_string()));
    }

    #[test]
    fn test_whitespace_trimming() {
        let links = extract_links("[[  spaced-target  ]]");
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].target_slug, "spaced-target");
    }

    #[test]
    fn test_type_with_spaces() {
        let links = extract_links("[[ my type :: my target ]]");
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].target_slug, "my target");
        assert_eq!(links[0].link_type, LinkType::Custom("my type".to_string()));
    }

    #[test]
    fn test_malformed_empty_type() {
        let links = extract_links("[[::target]]");
        assert_eq!(links.len(), 0);
    }

    #[test]
    fn test_malformed_empty_target_after_colons() {
        let links = extract_links("[[type::]]");
        assert_eq!(links.len(), 0);
    }

    #[test]
    fn test_triple_brackets() {
        let links = extract_links("[[[target]]]");
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].target_slug, "[target");
    }

    #[test]
    fn test_fenced_block_not_closed() {
        let content = "Before [[link1]]\n\n```\n[[not-a-link]]\n[[also-not]]";
        let links = extract_links(content);
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].target_slug, "link1");
    }

    #[test]
    fn test_multiple_fenced_blocks() {
        let content = "[[before]]\n```\n[[skip1]]\n```\n[[middle]]\n```\n[[skip2]]\n```\n[[after]]";
        let links = extract_links(content);
        assert_eq!(links.len(), 3);
        assert_eq!(links[0].target_slug, "before");
        assert_eq!(links[1].target_slug, "middle");
        assert_eq!(links[2].target_slug, "after");
    }

    #[test]
    fn test_inline_code_adjacent_to_link() {
        let content = "`code`[[link]]";
        let links = extract_links(content);
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].target_slug, "link");
    }

    #[test]
    fn test_display_only_pipe() {
        let links = extract_links("[[target|]]");
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].target_slug, "target");
        assert_eq!(links[0].context_snippet, None);
    }

    #[test]
    fn test_extract_links_for_page_filters_self_links() {
        let content = "See [[self-page]] and [[other-page]] for more.";
        let links = extract_links_for_page(content, "self-page");
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].target_slug, "other-page");
        assert_eq!(links[0].source_slug, "self-page");
    }

    #[test]
    fn test_extract_links_for_page_allows_all_when_no_self_link() {
        let content = "See [[page-a]] and [[page-b]] for more.";
        let links = extract_links_for_page(content, "source-page");
        assert_eq!(links.len(), 2);
        assert_eq!(links[0].target_slug, "page-a");
        assert_eq!(links[1].target_slug, "page-b");
        assert_eq!(links[0].source_slug, "source-page");
        assert_eq!(links[1].source_slug, "source-page");
    }

    #[test]
    fn test_extract_links_for_page_filters_all_self_links() {
        let content = "See [[me]] and [[me|alias]] and [[other]].";
        let links = extract_links_for_page(content, "me");
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].target_slug, "other");
    }
}
