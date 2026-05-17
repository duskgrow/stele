use crate::ops::handler::{OpExec, OpHandler, OperationContext};
use async_trait::async_trait;
use serde_json::{Value, json};

/// Handler struct registered with inventory.
pub struct GraphQueryHandler;

/// Executable operation with parsed arguments.
pub struct GraphQueryOp {
    pub slug: String,
    pub depth: Option<usize>,
    pub link_type: Option<String>,
    pub direction: Option<String>,
}

#[async_trait]
impl OpExec for GraphQueryOp {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    async fn execute(&self, ctx: &OperationContext) -> Result<Value, anyhow::Error> {
        crate::ops::search::handle_graph_query(
            &ctx.index,
            &self.slug,
            self.depth,
            self.link_type.as_deref(),
            self.direction.as_deref(),
        )
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))
    }
}

impl OpHandler for GraphQueryHandler {
    fn name(&self) -> &'static str {
        "graph.query"
    }
    fn description(&self) -> &'static str {
        "Query the page graph to a given depth"
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "slug": { "type": "string" },
                "depth": { "type": "integer" },
                "link_type": { "type": "string" },
                "direction": { "type": "string", "enum": ["in", "out", "both"] }
            },
            "required": ["slug"]
        })
    }

    fn from_mcp_args(
        &self,
        args: Option<serde_json::Map<String, Value>>,
    ) -> Result<Box<dyn OpExec>, anyhow::Error> {
        let args = args.unwrap_or_default();
        let slug = args
            .get("slug")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("missing required field: slug"))?
            .to_string();
        let depth = args
            .get("depth")
            .and_then(|v| v.as_u64())
            .map(|d| d as usize);
        let link_type = args
            .get("link_type")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let direction = args
            .get("direction")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        Ok(Box::new(GraphQueryOp {
            slug,
            depth,
            link_type,
            direction,
        }))
    }

    fn cli_command(&self) -> clap::Command {
        clap::Command::new("query")
            .about("Query page graph")
            .arg(clap::Arg::new("slug").required(true))
            .arg(
                clap::Arg::new("depth")
                    .long("depth")
                    .value_parser(clap::value_parser!(usize)),
            )
            .arg(clap::Arg::new("link_type").long("link-type"))
            .arg(
                clap::Arg::new("direction")
                    .long("direction")
                    .value_parser(["in", "out", "both"]),
            )
    }

    fn from_cli_matches(
        &self,
        matches: &clap::ArgMatches,
    ) -> Result<Box<dyn OpExec>, anyhow::Error> {
        let slug = matches
            .get_one::<String>("slug")
            .ok_or_else(|| anyhow::anyhow!("missing required argument: slug"))?
            .clone();
        let depth = matches.get_one::<usize>("depth").copied();
        let link_type = matches.get_one::<String>("link_type").cloned();
        let direction = matches.get_one::<String>("direction").cloned();
        Ok(Box::new(GraphQueryOp {
            slug,
            depth,
            link_type,
            direction,
        }))
    }
}

inventory::submit! {
    &GraphQueryHandler as &'static dyn OpHandler
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_graph_query_handler_meta() {
        let handler = GraphQueryHandler;
        assert_eq!(handler.name(), "graph.query");
        assert!(!handler.description().is_empty());

        let schema = handler.input_schema();
        assert!(schema.is_object());
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"].is_object());
        assert!(schema["properties"]["slug"].is_object());
        assert!(schema["properties"]["depth"].is_object());
    }

    #[test]
    fn test_graph_query_from_mcp_args_with_depth() {
        let handler = GraphQueryHandler;
        let mut args = serde_json::Map::new();
        args.insert(
            "slug".to_string(),
            serde_json::Value::String("test/page".to_string()),
        );
        args.insert("depth".to_string(), serde_json::Value::Number(2.into()));

        let exec = handler
            .from_mcp_args(Some(args))
            .expect("from_mcp_args should succeed");
        let _ = exec;
    }

    #[test]
    fn test_graph_query_from_mcp_args_slug_only() {
        let handler = GraphQueryHandler;
        let mut args = serde_json::Map::new();
        args.insert(
            "slug".to_string(),
            serde_json::Value::String("test/page".to_string()),
        );

        let exec = handler
            .from_mcp_args(Some(args))
            .expect("from_mcp_args should succeed");
        let _ = exec;
    }

    #[test]
    fn test_graph_query_missing_slug() {
        let handler = GraphQueryHandler;
        let result = handler.from_mcp_args(None);
        assert!(result.is_err());
        let err = match result {
            Err(e) => e.to_string(),
            Ok(_) => panic!("expected error"),
        };
        assert!(
            err.contains("missing required field: slug"),
            "expected slug error, got: {err}"
        );
    }

    #[test]
    fn test_graph_query_in_inventory() {
        let handlers: Vec<_> = inventory::iter::<&'static dyn OpHandler>
            .into_iter()
            .collect();
        let names: Vec<&str> = handlers.iter().map(|h| h.name()).collect();
        assert!(
            names.contains(&"graph.query"),
            "graph.query should be in inventory, found: {:?}",
            names
        );
    }

    #[test]
    fn test_graph_query_from_mcp_args_with_filters() {
        let handler = GraphQueryHandler;
        let mut args = serde_json::Map::new();
        args.insert(
            "slug".to_string(),
            serde_json::Value::String("test/page".to_string()),
        );
        args.insert("depth".to_string(), serde_json::Value::Number(2.into()));
        args.insert(
            "link_type".to_string(),
            serde_json::Value::String("cites".to_string()),
        );
        args.insert(
            "direction".to_string(),
            serde_json::Value::String("both".to_string()),
        );

        let exec = handler
            .from_mcp_args(Some(args))
            .expect("from_mcp_args should succeed");
        let _ = exec;
    }

    #[test]
    fn test_graph_query_schema_has_new_params() {
        let handler = GraphQueryHandler;
        let schema = handler.input_schema();
        assert!(schema["properties"]["link_type"].is_object());
        assert!(schema["properties"]["direction"].is_object());
    }
}
