use async_trait::async_trait;
use serde_json::{Value, json};
use crate::ops::handler::{OpHandler, OpExec, OperationContext};

pub struct GraphBacklinksHandler;

pub struct GraphBacklinksOp {
    pub slug: String,
}

#[async_trait]
impl OpExec for GraphBacklinksOp {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    async fn execute(&self, ctx: &OperationContext) -> Result<Value, anyhow::Error> {
        crate::ops::search::handle_graph_backlinks(&ctx.index, &self.slug)
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }
}

impl OpHandler for GraphBacklinksHandler {
    fn name(&self) -> &'static str { "graph.backlinks" }
    fn description(&self) -> &'static str { "Find pages linking to a slug" }
    
    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "slug": { "type": "string" }
            },
            "required": ["slug"]
        })
    }
    
    fn from_mcp_args(&self, args: Option<serde_json::Map<String, Value>>) -> Result<Box<dyn OpExec>, anyhow::Error> {
        let args = args.unwrap_or_default();
        let slug = args.get("slug")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("missing required field: slug"))?
            .to_string();
        Ok(Box::new(GraphBacklinksOp { slug }))
    }
    
    fn cli_command(&self) -> clap::Command {
        clap::Command::new("backlinks")
            .about("Find pages linking to a slug")
            .arg(clap::Arg::new("slug").required(true))
    }
    
    fn from_cli_matches(&self, matches: &clap::ArgMatches) -> Result<Box<dyn OpExec>, anyhow::Error> {
        let slug = matches.get_one::<String>("slug")
            .ok_or_else(|| anyhow::anyhow!("missing required argument: slug"))?
            .clone();
        Ok(Box::new(GraphBacklinksOp { slug }))
    }
}

inventory::submit! {
    &GraphBacklinksHandler as &'static dyn OpHandler
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_graph_backlinks_handler_meta() {
        let handler = GraphBacklinksHandler;
        assert_eq!(handler.name(), "graph.backlinks");
        assert!(!handler.description().is_empty());

        let schema = handler.input_schema();
        assert!(schema.is_object());
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"].is_object());
        assert!(schema["properties"]["slug"].is_object());
    }

    #[test]
    fn test_graph_backlinks_from_mcp_args() {
        let handler = GraphBacklinksHandler;
        let mut args = serde_json::Map::new();
        args.insert("slug".to_string(), serde_json::Value::String("test/page".to_string()));
        
        let exec = handler.from_mcp_args(Some(args)).expect("from_mcp_args should succeed");
        let _ = exec;
    }

    #[test]
    fn test_graph_backlinks_missing_slug() {
        let handler = GraphBacklinksHandler;
        let result = handler.from_mcp_args(None);
        assert!(result.is_err());
        let err = match result {
            Err(e) => e.to_string(),
            Ok(_) => panic!("expected error"),
        };
        assert!(err.contains("missing required field: slug"), "expected slug error, got: {err}");
    }

    #[test]
    fn test_graph_backlinks_in_inventory() {
        let handlers: Vec<_> = inventory::iter::<&'static dyn OpHandler>
            .into_iter()
            .collect();
        let names: Vec<&str> = handlers.iter().map(|h| h.name()).collect();
        assert!(names.contains(&"graph.backlinks"), "graph.backlinks should be in inventory, found: {:?}", names);
    }
}
