use crate::ops::handler::{OpExec, OpHandler, OperationContext};
use async_trait::async_trait;
use serde_json::{Value, json};

/// Handler struct registered with inventory.
pub struct TagsListHandler;

/// Executable operation with parsed arguments.
pub struct TagsListOp;

#[async_trait]
impl OpExec for TagsListOp {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    async fn execute(&self, ctx: &OperationContext) -> Result<Value, anyhow::Error> {
        let tags = ctx
            .index
            .list_tags()
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        Ok(json!({"tags": tags}))
    }
}

impl OpHandler for TagsListHandler {
    fn name(&self) -> &'static str {
        "tags.list"
    }
    fn description(&self) -> &'static str {
        "List all unique tags"
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object"
        })
    }

    fn from_mcp_args(
        &self,
        _args: Option<serde_json::Map<String, Value>>,
    ) -> Result<Box<dyn OpExec>, anyhow::Error> {
        Ok(Box::new(TagsListOp))
    }

    fn cli_command(&self) -> clap::Command {
        clap::Command::new("list").about("List all unique tags")
    }

    fn from_cli_matches(
        &self,
        _matches: &clap::ArgMatches,
    ) -> Result<Box<dyn OpExec>, anyhow::Error> {
        Ok(Box::new(TagsListOp))
    }
}

inventory::submit! {
    &TagsListHandler as &'static dyn OpHandler
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::index::IndexEngine;
    use crate::test_utils::*;
    use std::sync::Arc;

    #[test]
    fn test_tags_list_handler_meta() {
        let handler = TagsListHandler;
        assert_eq!(handler.name(), "tags.list");
        assert!(!handler.description().is_empty());
        let schema = handler.input_schema();
        assert!(schema.is_object());
        assert_eq!(schema["type"], "object");
    }

    #[test]
    fn test_tags_list_from_mcp_args() {
        let handler = TagsListHandler;
        let exec = handler
            .from_mcp_args(None)
            .expect("from_mcp_args should succeed");
        let _ = exec;
    }

    #[test]
    fn test_tags_list_in_inventory() {
        let handlers: Vec<_> = inventory::iter::<&'static dyn OpHandler>
            .into_iter()
            .collect();
        let names: Vec<&str> = handlers.iter().map(|h| h.name()).collect();
        assert!(
            names.contains(&"tags.list"),
            "tags.list should be in inventory, found: {:?}",
            names
        );
    }

    #[tokio::test]
    async fn test_tags_list_integration() {
        let engine = IndexEngine::new(":memory:").await.unwrap();
        sqlx::query(
            r#"INSERT INTO pages (slug, title, page_type, vault, content_hash, compiled_truth, raw_content,
                timeline_json, timeline_text, frontmatter_json, tags_json, created_at, updated_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)"#,
        )
        .bind("test-page")
        .bind("Test Page")
        .bind("Concept")
        .bind("")
        .bind("h1")
        .bind("Test content")
        .bind("# Test")
        .bind("[]")
        .bind("")
        .bind(r#"{"title":"Test Page","page_type":"Concept","tags":["rust","test"],"sources":[]}"#)
        .bind(r#"["rust","test"]"#)
        .bind("2024-01-01T00:00:00Z")
        .bind("2024-01-01T00:00:00Z")
        .execute(engine.pool())
        .await
        .unwrap();

        let fns = test_fns("http://localhost").await;
        let index = Arc::new(engine);
        let config = Config::default();
        let ctx = OperationContext { fns, index, config };

        let op = TagsListOp;
        let result = op.execute(&ctx).await.unwrap();

        let tags = result["tags"].as_array().unwrap();
        assert_eq!(tags.len(), 2);
        assert!(tags.contains(&serde_json::Value::String("rust".to_string())));
        assert!(tags.contains(&serde_json::Value::String("test".to_string())));
    }
}
