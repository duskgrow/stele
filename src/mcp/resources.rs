use std::sync::Arc;

use anyhow::{Context, Result};
use tracing::warn;

use crate::mcp::protocol::ResourceDefinition;
use crate::storage::{BackendError, FileBackend, FileMeta};

pub struct ResourceRegistry {
    file_backend: Arc<dyn FileBackend>,
}

impl ResourceRegistry {
    pub fn new(file_backend: Arc<dyn FileBackend>) -> Self {
        Self { file_backend }
    }

    pub async fn list_resources(&self) -> Vec<ResourceDefinition> {
        let mut resources = Vec::new();

        match self.list_md_files_recursive("skills").await {
            Ok(files) => {
                for file in files {
                    if let Some(uri) = self.skills_path_to_uri(&file.path) {
                        let name = file
                            .path
                            .rsplit('/')
                            .next()
                            .and_then(|f| f.strip_suffix(".md"))
                            .unwrap_or(&file.path)
                            .to_string();
                        resources.push(ResourceDefinition {
                            uri,
                            name,
                            mime_type: Some("text/markdown".to_string()),
                            description: None,
                        });
                    }
                }
            }
            Err(BackendError::NotFound(_)) => {}
            Err(e) => {
                warn!(error = %e, "failed to list skills directory");
            }
        }

        match self.list_md_files_recursive("wiki").await {
            Ok(files) => {
                for file in files {
                    let uri = format!("pages://{}", file.path);
                    let name = file
                        .path
                        .rsplit('/')
                        .next()
                        .and_then(|f| f.strip_suffix(".md"))
                        .unwrap_or(&file.path)
                        .to_string();
                    resources.push(ResourceDefinition {
                        uri,
                        name,
                        mime_type: Some("text/markdown".to_string()),
                        description: None,
                    });
                }
            }
            Err(BackendError::NotFound(_)) => {}
            Err(e) => {
                warn!(error = %e, "failed to list wiki directory");
            }
        }

        resources.push(ResourceDefinition {
            uri: "log://latest".to_string(),
            name: "Latest Log".to_string(),
            mime_type: Some("text/markdown".to_string()),
            description: Some("Last 50 lines of wiki/log.md".to_string()),
        });

        resources
    }

    pub async fn read_resource(&self, uri: &str) -> Result<String> {
        if let Some(rel_path) = uri.strip_prefix("skills://") {
            let file_path = format!("skills/{}.md", rel_path);
            self.file_backend
                .get(&file_path)
                .await
                .with_context(|| format!("failed to read skills resource: {uri}"))
        } else if let Some(slug) = uri.strip_prefix("pages://") {
            self.file_backend
                .get(slug)
                .await
                .with_context(|| format!("failed to read pages resource: {uri}"))
        } else if uri == "log://latest" {
            let content = self
                .file_backend
                .get("wiki/log.md")
                .await
                .with_context(|| "failed to read wiki/log.md")?;
            let lines: Vec<&str> = content.lines().collect();
            let start = lines.len().saturating_sub(50);
            Ok(lines[start..].join("\n"))
        } else {
            anyhow::bail!("unsupported resource URI scheme: {uri}")
        }
    }

    fn skills_path_to_uri(&self, path: &str) -> Option<String> {
        let stripped = path.strip_prefix("skills/")?;
        let without_ext = stripped.strip_suffix(".md")?;
        Some(format!("skills://{}", without_ext))
    }

    async fn list_md_files_recursive(&self, dir: &str) -> Result<Vec<FileMeta>, BackendError> {
        let mut result = Vec::new();
        let mut stack = vec![dir.to_string()];

        while let Some(current_dir) = stack.pop() {
            let entries = match self.file_backend.list(&current_dir).await {
                Ok(entries) => entries,
                Err(BackendError::NotFound(_)) => continue,
                Err(e) => return Err(e),
            };

            for entry in entries {
                if entry.is_dir {
                    stack.push(entry.path);
                } else if entry.path.ends_with(".md") {
                    result.push(entry);
                }
            }
        }

        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    struct DummyBackend {
        files: std::collections::HashMap<String, String>,
    }

    impl DummyBackend {
        fn new() -> Self {
            Self {
                files: std::collections::HashMap::new(),
            }
        }

        fn with_file(mut self, path: &str, content: &str) -> Self {
            self.files.insert(path.to_string(), content.to_string());
            self
        }
    }

    #[async_trait::async_trait]
    impl FileBackend for DummyBackend {
        async fn get(&self, path: &str) -> Result<String, BackendError> {
            self.files
                .get(path)
                .cloned()
                .ok_or_else(|| BackendError::NotFound(path.to_string()))
        }

        async fn put(&self, _path: &str, _content: &str) -> Result<(), BackendError> {
            Ok(())
        }

        async fn append(&self, _path: &str, _content: &str) -> Result<(), BackendError> {
            Ok(())
        }

        async fn delete(&self, _path: &str) -> Result<(), BackendError> {
            Ok(())
        }

        async fn list(&self, dir: &str) -> Result<Vec<FileMeta>, BackendError> {
            let mut entries = Vec::new();
            let prefix = if dir.ends_with('/') {
                dir.to_string()
            } else {
                format!("{}/", dir)
            };

            for path in self.files.keys() {
                if let Some(rest) = path.strip_prefix(&prefix) {
                    let first_segment = rest.split('/').next().unwrap_or(rest);
                    let entry_path = format!("{}{}", prefix, first_segment);
                    if !entries.iter().any(|e: &FileMeta| e.path == entry_path) {
                        let is_dir = rest.contains('/');
                        entries.push(FileMeta {
                            path: entry_path,
                            is_dir,
                            size: 0,
                            modified: None,
                        });
                    }
                }
            }

            if entries.is_empty() {
                return Err(BackendError::NotFound(dir.to_string()));
            }

            Ok(entries)
        }

        async fn exists(&self, path: &str) -> Result<bool, BackendError> {
            Ok(self.files.contains_key(path))
        }

        async fn stat(&self, path: &str) -> Result<crate::storage::FileStat, BackendError> {
            let content = self.get(path).await?;
            Ok(crate::storage::FileStat {
                size: content.len() as u64,
                modified: Utc::now(),
                content_hash: String::new(),
            })
        }
    }

    #[tokio::test]
    async fn list_resources_includes_skills() {
        let backend = Arc::new(DummyBackend::new().with_file("skills/dev/rust.md", "# Rust"));
        let registry = ResourceRegistry::new(backend);
        let resources = registry.list_resources().await;

        assert!(resources.iter().any(|r| r.uri == "skills://dev/rust"));
        assert!(resources.iter().any(|r| r.uri == "log://latest"));
    }

    #[tokio::test]
    async fn list_resources_includes_pages() {
        let backend = Arc::new(DummyBackend::new().with_file("wiki/index.md", "# Index"));
        let registry = ResourceRegistry::new(backend);
        let resources = registry.list_resources().await;

        assert!(resources.iter().any(|r| r.uri == "pages://wiki/index.md"));
    }

    #[tokio::test]
    async fn read_resource_skills() {
        let backend =
            Arc::new(DummyBackend::new().with_file("skills/dev/rust.md", "# Rust Skills"));
        let registry = ResourceRegistry::new(backend);
        let content = registry.read_resource("skills://dev/rust").await.unwrap();

        assert_eq!(content, "# Rust Skills");
    }

    #[tokio::test]
    async fn read_resource_pages() {
        let backend = Arc::new(DummyBackend::new().with_file("wiki/index.md", "# Wiki Index"));
        let registry = ResourceRegistry::new(backend);
        let content = registry
            .read_resource("pages://wiki/index.md")
            .await
            .unwrap();

        assert_eq!(content, "# Wiki Index");
    }

    #[tokio::test]
    async fn read_resource_log_latest_returns_last_50_lines() {
        let lines: Vec<String> = (1..=60).map(|i| format!("line {}", i)).collect();
        let content = lines.join("\n");
        let backend = Arc::new(DummyBackend::new().with_file("wiki/log.md", &content));
        let registry = ResourceRegistry::new(backend);
        let result = registry.read_resource("log://latest").await.unwrap();

        assert!(result.contains("line 60"));
        assert_eq!(result.lines().next().unwrap(), "line 11");
        assert_eq!(result.lines().count(), 50);
    }

    #[tokio::test]
    async fn read_resource_unknown_scheme_fails() {
        let backend = Arc::new(DummyBackend::new());
        let registry = ResourceRegistry::new(backend);
        let result = registry.read_resource("unknown://foo").await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn read_resource_not_found_fails() {
        let backend = Arc::new(DummyBackend::new());
        let registry = ResourceRegistry::new(backend);
        let result = registry.read_resource("skills://missing/file").await;

        assert!(result.is_err());
    }
}
