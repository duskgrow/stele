use regex::Regex;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Link {
    pub source_slug: String,
    pub target_slug: String,
    pub link_type: String,
    pub context_snippet: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ExtractedLink {
    pub target_slug: String,
    pub display_text: Option<String>,
    pub context_snippet: String,
}

/// Extract all [[wikilinks]] from content, ignoring those in code blocks
pub fn extract_wikilinks(content: &str) -> Vec<ExtractedLink> {
    let wikilink_re = Regex::new(r"\[\[([^\]\n]+?)\]\]").unwrap();

    let mut in_code_block = false;
    let mut result = Vec::new();

    for line in content.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") {
            in_code_block = !in_code_block;
            continue;
        }

        if in_code_block {
            continue;
        }

        for mat in wikilink_re.find_iter(line) {
            let full_match = mat.as_str();
            let inner = &full_match[2..full_match.len() - 2];

            let (target, display) = if let Some(pipe_idx) = inner.find('|') {
                let target = inner[..pipe_idx].trim();
                let display = inner[pipe_idx + 1..].trim();
                (target, Some(display.to_string()))
            } else {
                (inner.trim(), None)
            };

            let target_slug = target.to_lowercase();
            if target_slug.is_empty() {
                continue;
            }

            let start = mat.start();
            let end = mat.end();
            let context_start = start.saturating_sub(25);
            let context_end = (end + 25).min(line.len());
            let context_snippet = line[context_start..context_end].to_string();

            result.push(ExtractedLink {
                target_slug,
                display_text: display,
                context_snippet,
            });
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn link_serde_roundtrip() {
        let link = Link {
            source_slug: "wiki/entities/fns".to_string(),
            target_slug: "wiki/concepts/llm-wiki".to_string(),
            link_type: "works_at".to_string(),
            context_snippet: Some("Context around the link".to_string()),
        };

        let json = serde_json::to_string(&link).unwrap();
        let restored: Link = serde_json::from_str(&json).unwrap();

        assert_eq!(link.source_slug, restored.source_slug);
        assert_eq!(link.target_slug, restored.target_slug);
        assert_eq!(link.link_type, restored.link_type);
        assert_eq!(link.context_snippet, restored.context_snippet);
    }

    #[test]
    fn link_with_none_context() {
        let link = Link {
            source_slug: "a".to_string(),
            target_slug: "b".to_string(),
            link_type: "link".to_string(),
            context_snippet: None,
        };

        let json = serde_json::to_string(&link).unwrap();
        let restored: Link = serde_json::from_str(&json).unwrap();

        assert_eq!(link.context_snippet, restored.context_snippet);
    }

    #[test]
    fn extract_simple_wikilink() {
        let content = "See [[hello]] for more info.";
        let links = extract_wikilinks(content);
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].target_slug, "hello");
        assert_eq!(links[0].display_text, None);
        assert!(links[0].context_snippet.contains("[[hello]]"));
    }

    #[test]
    fn extract_wikilink_with_display_text() {
        let content = "Check out [[hello|Hello World]] now.";
        let links = extract_wikilinks(content);
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].target_slug, "hello");
        assert_eq!(links[0].display_text, Some("Hello World".to_string()));
    }

    #[test]
    fn extract_wikilink_with_path() {
        let content = "See [[folder/slug]] for details.";
        let links = extract_wikilinks(content);
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].target_slug, "folder/slug");
        assert_eq!(links[0].display_text, None);
    }

    #[test]
    fn extract_multiple_wikilinks() {
        let content = "Read [[alpha]] and [[beta|Beta Page]] here.";
        let links = extract_wikilinks(content);
        assert_eq!(links.len(), 2);
        assert_eq!(links[0].target_slug, "alpha");
        assert_eq!(links[1].target_slug, "beta");
        assert_eq!(links[1].display_text, Some("Beta Page".to_string()));
    }

    #[test]
    fn normalize_slug_to_lowercase() {
        let content = "See [[HelloWorld]] and [[UPPER]]";
        let links = extract_wikilinks(content);
        assert_eq!(links.len(), 2);
        assert_eq!(links[0].target_slug, "helloworld");
        assert_eq!(links[1].target_slug, "upper");
    }

    #[test]
    fn trim_whitespace_in_slug() {
        let content = "See [[  spaced  ]] and [[ a | b ]]";
        let links = extract_wikilinks(content);
        assert_eq!(links.len(), 2);
        assert_eq!(links[0].target_slug, "spaced");
        assert_eq!(links[1].target_slug, "a");
        assert_eq!(links[1].display_text, Some("b".to_string()));
    }

    #[test]
    fn ignore_wikilinks_in_code_blocks() {
        let content = r#"Some text with [[link1]].
```
This is a code block with [[ignored]].
```
More text with [[link2]]."#;
        let links = extract_wikilinks(content);
        assert_eq!(links.len(), 2);
        assert_eq!(links[0].target_slug, "link1");
        assert_eq!(links[1].target_slug, "link2");
    }

    #[test]
    fn ignore_wikilinks_in_multiline_code_blocks() {
        let content = r#"Start [[outer1]].
```rust
fn main() {
    let x = [[inner]];
}
```
End [[outer2]]."#;
        let links = extract_wikilinks(content);
        assert_eq!(links.len(), 2);
        assert_eq!(links[0].target_slug, "outer1");
        assert_eq!(links[1].target_slug, "outer2");
    }

    #[test]
    fn no_wikilinks_returns_empty() {
        let content = "Just plain text without any links.";
        let links = extract_wikilinks(content);
        assert!(links.is_empty());
    }

    #[test]
    fn empty_slug_is_ignored() {
        let content = "See [[ ]] and [[|display]] here.";
        let links = extract_wikilinks(content);
        assert!(links.is_empty());
    }

    #[test]
    fn context_snippet_length() {
        let content =
            "Before this we have some text and then [[target]] and after we have more text to pad.";
        let links = extract_wikilinks(content);
        assert_eq!(links.len(), 1);
        let snippet = &links[0].context_snippet;
        assert!(snippet.contains("[[target]]"));
        assert!(snippet.len() <= 60);
    }

    #[test]
    fn wikilink_at_line_boundary_context() {
        let content = "[[start]] of the line";
        let links = extract_wikilinks(content);
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].target_slug, "start");
        assert_eq!(links[0].context_snippet, "[[start]] of the line");
    }

    #[test]
    fn indented_code_block_recognized() {
        let content = r#"Text [[link1]].
   ```
   code with [[ignored]]
   ```
More [[link2]]."#;
        let links = extract_wikilinks(content);
        assert_eq!(links.len(), 2);
        assert_eq!(links[0].target_slug, "link1");
        assert_eq!(links[1].target_slug, "link2");
    }
}
