use crate::ops::handler::{OpExec, OpHandler, OperationContext};
use async_trait::async_trait;
use serde_json::{Value, json};

/// Handler struct registered with inventory.
pub struct SearchHandler;

/// Executable operation with parsed arguments.
pub struct SearchOp {
    pub query: String,
    pub limit: Option<i64>,
    pub type_filter: Option<String>,
    pub sort: Option<String>,
}

#[async_trait]
impl OpExec for SearchOp {
    async fn execute(&self, ctx: &OperationContext) -> Result<Value, anyhow::Error> {
        crate::ops::search::handle_search(
            &ctx.index,
            &self.query,
            self.limit,
            self.type_filter.as_deref(),
            self.sort.as_deref(),
        )
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

impl OpHandler for SearchHandler {
    fn name(&self) -> &'static str {
        "search"
    }
    fn description(&self) -> &'static str {
        "Full-text search"
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "query": { "type": "string" },
                "limit": { "type": "integer" },
                "type_filter": { "type": "string" },
                "sort": { "type": "string", "enum": ["relevance", "date", "title"] }
            },
            "required": ["query"]
        })
    }

    fn from_mcp_args(
        &self,
        args: Option<serde_json::Map<String, Value>>,
    ) -> Result<Box<dyn OpExec>, anyhow::Error> {
        let args = args.unwrap_or_default();
        let query = args
            .get("query")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("missing required field: query"))?
            .to_string();
        let limit = args.get("limit").and_then(|v| v.as_i64());
        let type_filter = args
            .get("type_filter")
            .and_then(|v| v.as_str())
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string());
        let sort = args
            .get("sort")
            .and_then(|v| v.as_str())
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string());
        Ok(Box::new(SearchOp {
            query,
            limit,
            type_filter,
            sort,
        }))
    }

    fn cli_command(&self) -> clap::Command {
        clap::Command::new("search")
            .about("Full-text search")
            .arg(clap::Arg::new("query").required(true))
            .arg(
                clap::Arg::new("limit")
                    .long("limit")
                    .value_parser(clap::value_parser!(i64)),
            )
            .arg(clap::Arg::new("type_filter").long("type-filter"))
            .arg(
                clap::Arg::new("sort")
                    .long("sort")
                    .value_parser(["relevance", "date", "title"]),
            )
    }

    fn from_cli_matches(
        &self,
        matches: &clap::ArgMatches,
    ) -> Result<Box<dyn OpExec>, anyhow::Error> {
        let query = matches
            .get_one::<String>("query")
            .ok_or_else(|| anyhow::anyhow!("missing required argument: query"))?
            .clone();
        let limit = matches.get_one::<i64>("limit").copied();
        let type_filter = matches.get_one::<String>("type_filter").cloned();
        let sort = matches.get_one::<String>("sort").cloned();
        Ok(Box::new(SearchOp {
            query,
            limit,
            type_filter,
            sort,
        }))
    }
}

inventory::submit! {
    &SearchHandler as &'static dyn OpHandler
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use std::sync::Arc;

    #[test]
    fn test_search_handler_meta() {
        let handler = SearchHandler;
        assert_eq!(handler.name(), "search");
        assert!(!handler.description().is_empty());

        let schema = handler.input_schema();
        assert!(schema.is_object());
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"].is_object());
        assert!(schema["properties"]["query"].is_object());
        assert!(schema["properties"]["sort"].is_object());
        let sort_enum = schema["properties"]["sort"]["enum"].as_array().unwrap();
        let sort_values: Vec<&str> = sort_enum.iter().map(|v| v.as_str().unwrap()).collect();
        assert_eq!(sort_values, vec!["relevance", "date", "title"]);
    }

    #[test]
    fn test_search_from_mcp_args_all_params() {
        let handler = SearchHandler;
        let mut args = serde_json::Map::new();
        args.insert(
            "query".to_string(),
            serde_json::Value::String("rust".to_string()),
        );
        args.insert("limit".to_string(), serde_json::Value::Number(5.into()));
        args.insert(
            "type_filter".to_string(),
            serde_json::Value::String("Concept".to_string()),
        );
        args.insert(
            "sort".to_string(),
            serde_json::Value::String("date".to_string()),
        );

        let exec = handler
            .from_mcp_args(Some(args))
            .expect("from_mcp_args should succeed");
        let op = exec
            .as_any()
            .downcast_ref::<SearchOp>()
            .expect("should be SearchOp");
        assert_eq!(op.query, "rust");
        assert_eq!(op.limit, Some(5));
        assert_eq!(op.type_filter, Some("Concept".to_string()));
        assert_eq!(op.sort, Some("date".to_string()));
    }

    #[test]
    fn test_search_from_mcp_args_query_only() {
        let handler = SearchHandler;
        let mut args = serde_json::Map::new();
        args.insert(
            "query".to_string(),
            serde_json::Value::String("rust".to_string()),
        );

        let exec = handler
            .from_mcp_args(Some(args))
            .expect("from_mcp_args should succeed");
        let op = exec
            .as_any()
            .downcast_ref::<SearchOp>()
            .expect("should be SearchOp");
        assert_eq!(op.query, "rust");
        assert!(op.limit.is_none());
        assert!(op.type_filter.is_none());
        assert!(op.sort.is_none());
    }

    #[test]
    fn test_search_from_mcp_args_empty_type_filter() {
        let handler = SearchHandler;
        let mut args = serde_json::Map::new();
        args.insert(
            "query".to_string(),
            serde_json::Value::String("rust".to_string()),
        );
        args.insert(
            "type_filter".to_string(),
            serde_json::Value::String("".to_string()),
        );

        let exec = handler
            .from_mcp_args(Some(args))
            .expect("from_mcp_args should succeed");
        let op = exec
            .as_any()
            .downcast_ref::<SearchOp>()
            .expect("should be SearchOp");
        assert_eq!(op.query, "rust");
        assert!(
            op.type_filter.is_none(),
            "empty type_filter should normalize to None"
        );
    }

    #[test]
    fn test_search_from_mcp_args_whitespace_type_filter() {
        let handler = SearchHandler;
        let mut args = serde_json::Map::new();
        args.insert(
            "query".to_string(),
            serde_json::Value::String("rust".to_string()),
        );
        args.insert(
            "type_filter".to_string(),
            serde_json::Value::String("   ".to_string()),
        );

        let exec = handler
            .from_mcp_args(Some(args))
            .expect("from_mcp_args should succeed");
        let op = exec
            .as_any()
            .downcast_ref::<SearchOp>()
            .expect("should be SearchOp");
        assert_eq!(op.query, "rust");
        assert!(
            op.type_filter.is_none(),
            "whitespace type_filter should normalize to None"
        );
    }

    #[test]
    fn test_search_from_mcp_args_valid_type_filter() {
        let handler = SearchHandler;
        let mut args = serde_json::Map::new();
        args.insert(
            "query".to_string(),
            serde_json::Value::String("rust".to_string()),
        );
        args.insert(
            "type_filter".to_string(),
            serde_json::Value::String("Entity".to_string()),
        );

        let exec = handler
            .from_mcp_args(Some(args))
            .expect("from_mcp_args should succeed");
        let op = exec
            .as_any()
            .downcast_ref::<SearchOp>()
            .expect("should be SearchOp");
        assert_eq!(op.query, "rust");
        assert_eq!(op.type_filter, Some("Entity".to_string()));
    }

    #[test]
    fn test_search_from_mcp_args_empty_sort() {
        let handler = SearchHandler;
        let mut args = serde_json::Map::new();
        args.insert(
            "query".to_string(),
            serde_json::Value::String("rust".to_string()),
        );
        args.insert(
            "sort".to_string(),
            serde_json::Value::String("".to_string()),
        );

        let exec = handler
            .from_mcp_args(Some(args))
            .expect("from_mcp_args should succeed");
        let op = exec
            .as_any()
            .downcast_ref::<SearchOp>()
            .expect("should be SearchOp");
        assert_eq!(op.query, "rust");
        assert!(op.sort.is_none(), "empty sort should normalize to None");
    }

    #[test]
    fn test_search_missing_query() {
        let handler = SearchHandler;
        let result = handler.from_mcp_args(None);
        assert!(result.is_err());
        let err = match result {
            Err(e) => e.to_string(),
            Ok(_) => panic!("expected error"),
        };
        assert!(
            err.contains("missing required field: query"),
            "expected query error, got: {err}"
        );
    }

    #[test]
    fn test_search_in_inventory() {
        let handlers: Vec<_> = inventory::iter::<&'static dyn OpHandler>
            .into_iter()
            .collect();
        let names: Vec<&str> = handlers.iter().map(|h| h.name()).collect();
        assert!(
            names.contains(&"search"),
            "search should be in inventory, found: {:?}",
            names
        );
    }

    #[test]
    fn test_search_from_cli_with_sort() {
        let handler = SearchHandler;
        let cmd = handler.cli_command();
        let matches = cmd
            .try_get_matches_from(["search", "rust", "--sort", "title"])
            .unwrap();

        let exec = handler
            .from_cli_matches(&matches)
            .expect("from_cli_matches should succeed");
        let op = exec
            .as_any()
            .downcast_ref::<SearchOp>()
            .expect("should be SearchOp");
        assert_eq!(op.query, "rust");
        assert_eq!(op.sort, Some("title".to_string()));
    }

    #[tokio::test]
    async fn test_search_empty_type_filter_integration() {
        use crate::index::IndexEngine;
        use crate::test_utils::*;
        use crate::types::PageType;

        let engine = IndexEngine::new(":memory:").await.unwrap();
        let page = sample_page(
            "rust-page",
            "Rust Programming Language",
            PageType::Concept,
            "Rust is great for systems programming.",
        );
        engine.index_page(&page).await.unwrap();

        let fns = test_fns("http://localhost").await;
        let index = Arc::new(engine);
        let config = Config::default();
        let ctx = OperationContext { fns, index, config };

        let op_no_filter = SearchOp {
            query: "systems".to_string(),
            limit: Some(10),
            type_filter: None,
            sort: None,
        };
        let result_no_filter = op_no_filter.execute(&ctx).await.unwrap();

        let mut args = serde_json::Map::new();
        args.insert(
            "query".to_string(),
            serde_json::Value::String("systems".to_string()),
        );
        args.insert(
            "type_filter".to_string(),
            serde_json::Value::String("".to_string()),
        );

        let handler = SearchHandler;
        let exec = handler
            .from_mcp_args(Some(args))
            .expect("from_mcp_args should succeed");
        let op_empty_filter = exec
            .as_any()
            .downcast_ref::<SearchOp>()
            .expect("should be SearchOp");
        let result_empty_filter = op_empty_filter.execute(&ctx).await.unwrap();

        assert_eq!(result_no_filter["total"], result_empty_filter["total"]);
        assert_eq!(
            result_no_filter["results"].as_array().unwrap().len(),
            result_empty_filter["results"].as_array().unwrap().len()
        );
    }
}
