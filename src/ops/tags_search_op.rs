use crate::ops::handler::{OpExec, OpHandler, OperationContext};
use async_trait::async_trait;
use serde_json::{Value, json};

/// Handler struct registered with inventory.
pub struct TagsSearchHandler;

/// Executable operation with parsed arguments.
pub struct TagsSearchOp {
    pub tags: Vec<String>,
    pub tag_match: String,
}

#[async_trait]
impl OpExec for TagsSearchOp {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    async fn execute(&self, ctx: &OperationContext) -> Result<Value, anyhow::Error> {
        let hits = ctx
            .index
            .search_by_tags(&self.tags, &self.tag_match)
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        let results: Vec<Value> = hits
            .into_iter()
            .map(|h| {
                json!({
                    "slug": h.slug,
                    "title": h.title,
                    "tags": h.tags,
                    "page_type": h.page_type
                })
            })
            .collect();
        Ok(json!({
            "tags": self.tags,
            "tag_match": self.tag_match,
            "total": results.len(),
            "results": results
        }))
    }
}

impl OpHandler for TagsSearchHandler {
    fn name(&self) -> &'static str {
        "tags.search"
    }
    fn description(&self) -> &'static str {
        "Search pages by tags"
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "tags": {
                    "type": "array",
                    "items": { "type": "string" }
                },
                "tag_match": {
                    "type": "string",
                    "enum": ["or", "and"]
                }
            },
            "required": ["tags"]
        })
    }

    fn from_mcp_args(
        &self,
        args: Option<serde_json::Map<String, Value>>,
    ) -> Result<Box<dyn OpExec>, anyhow::Error> {
        let args = args.unwrap_or_default();
        let tags = args
            .get("tags")
            .and_then(|v| v.as_array())
            .ok_or_else(|| anyhow::anyhow!("missing required field: tags"))?
            .iter()
            .enumerate()
            .map(|(idx, v)| {
                v.as_str()
                    .map(str::to_string)
                    .ok_or_else(|| anyhow::anyhow!("tags[{idx}] must be a string"))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let tag_match = args
            .get("tag_match")
            .and_then(|v| v.as_str())
            .unwrap_or("or")
            .to_string();
        if !matches!(tag_match.as_str(), "or" | "and") {
            return Err(anyhow::anyhow!("tag_match must be 'or' or 'and'"));
        }
        Ok(Box::new(TagsSearchOp { tags, tag_match }))
    }

    fn cli_command(&self) -> clap::Command {
        clap::Command::new("search")
            .about("Search pages by tags")
            .arg(clap::Arg::new("tags").required(true).num_args(1..))
            .arg(
                clap::Arg::new("tag_match")
                    .long("tag-match")
                    .default_value("or")
                    .value_parser(["or", "and"]),
            )
    }

    fn from_cli_matches(
        &self,
        matches: &clap::ArgMatches,
    ) -> Result<Box<dyn OpExec>, anyhow::Error> {
        let tags: Vec<String> = matches
            .get_many::<String>("tags")
            .ok_or_else(|| anyhow::anyhow!("missing required argument: tags"))?
            .cloned()
            .collect();
        let tag_match = matches
            .get_one::<String>("tag_match")
            .cloned()
            .unwrap_or_else(|| "or".to_string());
        Ok(Box::new(TagsSearchOp { tags, tag_match }))
    }
}

inventory::submit! {
    &TagsSearchHandler as &'static dyn OpHandler
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::index::IndexEngine;
    use crate::test_utils::*;
    use std::sync::Arc;

    #[test]
    fn test_tags_search_handler_meta() {
        let handler = TagsSearchHandler;
        assert_eq!(handler.name(), "tags.search");
        assert!(!handler.description().is_empty());
        let schema = handler.input_schema();
        assert!(schema.is_object());
        let required = schema["required"].as_array().unwrap();
        assert!(required.contains(&serde_json::Value::String("tags".to_string())));
        let tag_match_enum = schema["properties"]["tag_match"]["enum"]
            .as_array()
            .unwrap();
        let values: Vec<&str> = tag_match_enum.iter().map(|v| v.as_str().unwrap()).collect();
        assert_eq!(values, vec!["or", "and"]);
    }

    #[test]
    fn test_tags_search_from_mcp_args_or() {
        let handler = TagsSearchHandler;
        let mut args = serde_json::Map::new();
        args.insert("tags".to_string(), json!(["rust", "python"]));
        let exec = handler.from_mcp_args(Some(args)).expect("should succeed");
        let op = exec
            .as_any()
            .downcast_ref::<TagsSearchOp>()
            .expect("should be TagsSearchOp");
        assert_eq!(op.tags, vec!["rust", "python"]);
        assert_eq!(op.tag_match, "or");
    }

    #[test]
    fn test_tags_search_from_mcp_args_and() {
        let handler = TagsSearchHandler;
        let mut args = serde_json::Map::new();
        args.insert("tags".to_string(), json!(["rust", "web"]));
        args.insert("tag_match".to_string(), json!("and"));
        let exec = handler.from_mcp_args(Some(args)).expect("should succeed");
        let op = exec
            .as_any()
            .downcast_ref::<TagsSearchOp>()
            .expect("should be TagsSearchOp");
        assert_eq!(op.tags, vec!["rust", "web"]);
        assert_eq!(op.tag_match, "and");
    }

    #[test]
    fn test_tags_search_from_mcp_args_missing_tags() {
        let handler = TagsSearchHandler;
        let result = handler.from_mcp_args(None);
        assert!(result.is_err());
        let err = match result {
            Err(e) => e.to_string(),
            Ok(_) => panic!("expected error"),
        };
        assert!(
            err.contains("missing required field: tags"),
            "expected tags error, got: {err}"
        );
    }

    #[test]
    fn test_tags_search_from_mcp_args_rejects_non_string_tags() {
        let handler = TagsSearchHandler;
        let mut args = serde_json::Map::new();
        args.insert("tags".to_string(), json!(["rust", 123]));

        let result = handler.from_mcp_args(Some(args));
        assert!(result.is_err());
        let err = match result {
            Err(e) => e.to_string(),
            Ok(_) => panic!("expected error"),
        };
        assert!(
            err.contains("tags[1] must be a string"),
            "expected tag type error, got: {err}"
        );
    }

    #[test]
    fn test_tags_search_from_mcp_args_rejects_invalid_tag_match() {
        let handler = TagsSearchHandler;
        let mut args = serde_json::Map::new();
        args.insert("tags".to_string(), json!(["rust"]));
        args.insert("tag_match".to_string(), json!("xor"));

        let result = handler.from_mcp_args(Some(args));
        assert!(result.is_err());
        let err = match result {
            Err(e) => e.to_string(),
            Ok(_) => panic!("expected error"),
        };
        assert!(
            err.contains("tag_match must be 'or' or 'and'"),
            "expected tag_match error, got: {err}"
        );
    }

    #[test]
    fn test_tags_search_in_inventory() {
        let handlers: Vec<_> = inventory::iter::<&'static dyn OpHandler>
            .into_iter()
            .collect();
        let names: Vec<&str> = handlers.iter().map(|h| h.name()).collect();
        assert!(
            names.contains(&"tags.search"),
            "tags.search should be in inventory, found: {:?}",
            names
        );
    }

    #[tokio::test]
    async fn test_tags_search_integration() {
        let engine = IndexEngine::new(":memory:").await.unwrap();
        // Insert pages with specific tags
        for (slug, title, tags_json) in [
            ("page-a", "Page A", r#"["rust","web"]"#),
            ("page-b", "Page B", r#"["rust","systems"]"#),
            ("page-c", "Page C", r#"["python","ml"]"#),
        ] {
            sqlx::query(
                r#"INSERT INTO pages (slug, title, page_type, vault, content_hash, compiled_truth, raw_content,
                    timeline_json, timeline_text, frontmatter_json, tags_json, created_at, updated_at)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)"#,
            )
            .bind(slug).bind(title).bind("Concept").bind("")
            .bind(format!("h-{slug}")).bind(format!("{title} content")).bind(format!("# {title}"))
            .bind("[]").bind("")
            .bind(format!(r#"{{"title":"{title}","page_type":"Concept","tags":{tags_json},"sources":[]}}"#))
            .bind(tags_json)
            .bind("2024-01-01T00:00:00Z").bind("2024-01-01T00:00:00Z")
            .execute(engine.pool()).await.unwrap();
        }

        let fns = test_fns("http://localhost").await;
        let index = Arc::new(engine);
        let config = Config::default();
        let ctx = OperationContext { fns, index, config };

        // OR search
        let op = TagsSearchOp {
            tags: vec!["rust".to_string(), "python".to_string()],
            tag_match: "or".to_string(),
        };
        let result = op.execute(&ctx).await.unwrap();
        assert_eq!(result["total"], 3);
        assert_eq!(result["tag_match"], "or");

        // AND search
        let op_and = TagsSearchOp {
            tags: vec!["rust".to_string(), "web".to_string()],
            tag_match: "and".to_string(),
        };
        let result_and = op_and.execute(&ctx).await.unwrap();
        assert_eq!(result_and["total"], 1);
        assert_eq!(result_and["results"][0]["slug"], "page-a");
    }
}
