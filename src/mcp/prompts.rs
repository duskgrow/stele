use std::sync::Arc;

use anyhow::Context;
use tracing::warn;

use crate::mcp::protocol::{PromptArgument, PromptDefinition};
use crate::storage::{BackendError, FileBackend};

/// Metadata for a prompt template stored in the vault.
#[derive(Debug, Clone)]
struct PromptMeta {
    name: String,
    description: String,
    arguments: Vec<PromptArgument>,
    /// Path in the vault, e.g. "skills/prompts/ingest.md"
    path: String,
}

pub struct PromptRegistry {
    file_backend: Arc<dyn FileBackend>,
    /// Base directory in the vault where prompt .md files live.
    prompts_dir: String,
}

impl PromptRegistry {
    pub fn new(file_backend: Arc<dyn FileBackend>) -> Self {
        Self {
            file_backend,
            prompts_dir: "prompts".to_string(),
        }
    }

    /// List all available prompts by scanning the vault directory.
    pub async fn list(&self) -> Vec<PromptDefinition> {
        let metas = self.load_prompt_metas().await;
        metas
            .into_iter()
            .map(|m| PromptDefinition {
                name: m.name,
                description: Some(m.description),
                arguments: Some(m.arguments),
            })
            .collect()
    }

    /// Get a rendered prompt by name, with variable substitution.
    pub async fn get(
        &self,
        name: &str,
        args: std::collections::HashMap<String, String>,
    ) -> Result<String, anyhow::Error> {
        let metas = self.load_prompt_metas().await;
        let meta = metas
            .iter()
            .find(|m| m.name == name)
            .ok_or_else(|| anyhow::anyhow!("Unknown prompt: {}", name))?;

        let content = self
            .file_backend
            .get(&meta.path)
            .await
            .with_context(|| format!("failed to read prompt template: {}", meta.path))?;

        let mut rendered = content;
        for (key, value) in &args {
            rendered = rendered.replace(&format!("{{{{{}}}}}", key), value);
        }

        Ok(rendered)
    }

    /// Scan the prompts directory and parse frontmatter-like metadata from
    /// each .md file. The first line is treated as the URI/description hint,
    /// and arguments are extracted from `{{variable}}` placeholders in the body.
    async fn load_prompt_metas(&self) -> Vec<PromptMeta> {
        let files = match self.file_backend.list(&self.prompts_dir).await {
            Ok(files) => files,
            Err(BackendError::NotFound(_)) => {
                warn!(dir = %self.prompts_dir, "prompts directory not found");
                return Vec::new();
            }
            Err(e) => {
                warn!(error = %e, "failed to list prompts directory");
                return Vec::new();
            }
        };

        let mut metas = Vec::new();
        for file in files {
            if file.is_dir || !file.path.ends_with(".md") {
                continue;
            }

            let name = file
                .path
                .rsplit('/')
                .next()
                .and_then(|f| f.strip_suffix(".md"))
                .unwrap_or(&file.path)
                .to_string();

            match self.file_backend.get(&file.path).await {
                Ok(content) => {
                    let (description, arguments) = parse_prompt_header(&name, &content);
                    metas.push(PromptMeta {
                        name,
                        description,
                        arguments,
                        path: file.path,
                    });
                }
                Err(e) => {
                    warn!(path = %file.path, error = %e, "failed to read prompt file");
                }
            }
        }

        metas
    }
}

/// Parse a prompt template to extract description and arguments.
///
/// Supports two formats:
/// 1. YAML frontmatter (preferred):
///    ```yaml
///    ---
///    name: ingest
///    description: 当用户提供原始资料时...
///    arguments:
///      - name: source_type
///        description: 资料类型
///        required: true
///    ---
///    ```
/// 2. Legacy heuristic: first line = URI, `## 场景` = description, `{{var}}` = arguments
fn parse_prompt_header(name: &str, content: &str) -> (String, Vec<PromptArgument>) {
    // Try YAML frontmatter first
    if let Some(result) = try_parse_frontmatter(name, content) {
        return result;
    }

    // Fallback: legacy heuristic
    parse_legacy_header(name, content)
}

/// Try to parse YAML frontmatter between `---` markers.
fn try_parse_frontmatter(_name: &str, content: &str) -> Option<(String, Vec<PromptArgument>)> {
    let trimmed = content.trim_start();
    if !trimmed.starts_with("---") {
        return None;
    }

    // Find the closing ---
    let after_first = &trimmed[3..];
    let end_pos = after_first.find("\n---")?;
    let yaml_str = &after_first[..end_pos];

    let yaml: serde_yaml::Value = serde_yaml::from_str(yaml_str).ok()?;

    let description = yaml
        .get("description")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let mut arguments = Vec::new();
    if let Some(args_seq) = yaml.get("arguments").and_then(|v| v.as_sequence()) {
        for arg in args_seq {
            let arg_name = arg.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string();
            if arg_name.is_empty() {
                continue;
            }
            let arg_desc = arg
                .get("description")
                .and_then(|v| v.as_str())
                .map(String::from);
            let required = arg
                .get("required")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);

            arguments.push(PromptArgument {
                name: arg_name,
                description: arg_desc,
                required: Some(required),
            });
        }
    }

    Some((description, arguments))
}

/// Legacy heuristic parser for prompts without frontmatter.
fn parse_legacy_header(name: &str, content: &str) -> (String, Vec<PromptArgument>) {
    let lines: Vec<&str> = content.lines().collect();

    // Extract description from first "## 场景" section
    let description = lines
        .iter()
        .skip(1) // skip the brain:// URI line
        .find(|l| l.starts_with("## 场景"))
        .and_then(|l| {
            let idx = lines.iter().position(|x| x == l).unwrap_or(0);
            lines[idx + 1..]
                .iter()
                .find(|l| !l.trim().is_empty())
                .map(|l| l.trim().to_string())
        })
        .unwrap_or_else(|| format!("Prompt: {}", name));

    // Extract {{variable}} placeholders as arguments
    let mut arguments = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for line in &lines {
        let mut remaining = *line;
        while let Some(start) = remaining.find("{{") {
            if let Some(end) = remaining[start + 2..].find("}}") {
                let var_name = remaining[start + 2..start + 2 + end].trim().to_string();
                if !var_name.is_empty() && seen.insert(var_name.clone()) {
                    arguments.push(PromptArgument {
                        name: var_name,
                        description: None,
                        required: Some(false),
                    });
                }
                remaining = &remaining[start + 2 + end + 2..];
            } else {
                break;
            }
        }
    }

    (description, arguments)
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- YAML frontmatter tests ---

    #[test]
    fn frontmatter_extracts_description() {
        let content = "---\nname: ingest\ndescription: 当用户提供原始资料时，将其转化为结构化的 Wiki 页面。\narguments:\n  - name: source_type\n    description: 资料类型\n    required: true\n---\n\n## 可用工具\n...\n";
        let (desc, _) = parse_prompt_header("ingest", content);
        assert_eq!(desc, "当用户提供原始资料时，将其转化为结构化的 Wiki 页面。");
    }

    #[test]
    fn frontmatter_extracts_arguments() {
        let content = "---\nname: query\ndescription: 检索知识\narguments:\n  - name: question\n    description: 用户的问题\n    required: true\n  - name: depth\n    description: 搜索深度\n    required: false\n---\n\nBody\n";
        let (_, args) = parse_prompt_header("query", content);
        assert_eq!(args.len(), 2);
        assert_eq!(args[0].name, "question");
        assert_eq!(args[0].description.as_deref(), Some("用户的问题"));
        assert_eq!(args[0].required, Some(true));
        assert_eq!(args[1].name, "depth");
        assert_eq!(args[1].required, Some(false));
    }

    #[test]
    fn frontmatter_no_arguments_field() {
        let content = "---\nname: test\ndescription: A test prompt\n---\n\nBody\n";
        let (desc, args) = parse_prompt_header("test", content);
        assert_eq!(desc, "A test prompt");
        assert!(args.is_empty());
    }

    #[test]
    fn frontmatter_empty_description() {
        let content = "---\nname: test\n---\n\nBody\n";
        let (desc, _) = parse_prompt_header("test", content);
        assert_eq!(desc, "");
    }

    // --- Legacy heuristic tests ---

    #[test]
    fn legacy_extracts_description() {
        let content = "brain://ingest\n\n## 场景\n用户提供了原始资料，需要将其转化为结构化的 Wiki 知识。\n\n## 执行流程\n...\n";
        let (desc, _) = parse_prompt_header("ingest", content);
        assert_eq!(
            desc,
            "用户提供了原始资料，需要将其转化为结构化的 Wiki 知识。"
        );
    }

    #[test]
    fn legacy_extracts_arguments() {
        let content = "brain://query\n\n## 场景\n...\n\n根据 {{depth}} 参数选择策略...\n\n如果没有 {{missing_topic}} 的信息\n";
        let (_, args) = parse_prompt_header("query", content);
        assert_eq!(args.len(), 2);
        assert_eq!(args[0].name, "depth");
        assert_eq!(args[1].name, "missing_topic");
    }

    #[test]
    fn legacy_no_scene_section() {
        let content = "brain://test\n\nSome description here.\n";
        let (desc, _) = parse_prompt_header("test", content);
        assert_eq!(desc, "Prompt: test");
    }

    #[test]
    fn legacy_no_arguments() {
        let content = "brain://test\n\n## 场景\nSimple prompt.\n";
        let (_, args) = parse_prompt_header("test", content);
        assert!(args.is_empty());
    }

    #[test]
    fn legacy_deduplicates_arguments() {
        let content = "brain://test\n\n{{x}} and {{x}} and {{y}}\n";
        let (_, args) = parse_prompt_header("test", content);
        assert_eq!(args.len(), 2);
    }
}
