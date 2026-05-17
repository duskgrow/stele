use crate::ops::handler::{OpExec, OpHandler, OperationContext};
use async_trait::async_trait;
use serde_json::{Value, json};

/// Handler struct registered with inventory.
pub struct ReindexHandler;

/// Executable operation with parsed arguments.
pub struct ReindexOp;

#[async_trait]
impl OpExec for ReindexOp {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    async fn execute(&self, ctx: &OperationContext) -> Result<Value, anyhow::Error> {
        crate::ops::maintain::handle_reindex(&ctx.fns, &ctx.index)
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }
}

impl OpHandler for ReindexHandler {
    fn name(&self) -> &'static str {
        "reindex"
    }
    fn description(&self) -> &'static str {
        "Rebuild the full-text search index"
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
        Ok(Box::new(ReindexOp))
    }

    fn cli_command(&self) -> clap::Command {
        clap::Command::new("reindex").about("Rebuild search index")
    }

    fn from_cli_matches(
        &self,
        _matches: &clap::ArgMatches,
    ) -> Result<Box<dyn OpExec>, anyhow::Error> {
        Ok(Box::new(ReindexOp))
    }
}

inventory::submit! {
    &ReindexHandler as &'static dyn OpHandler
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_reindex_handler_meta() {
        let handler = ReindexHandler;
        assert_eq!(handler.name(), "reindex");
        assert!(!handler.description().is_empty());

        let schema = handler.input_schema();
        assert!(schema.is_object());
        assert_eq!(schema["type"], "object");
    }

    #[test]
    fn test_reindex_from_mcp_args() {
        let handler = ReindexHandler;
        let exec = handler
            .from_mcp_args(None)
            .expect("from_mcp_args should succeed");
        let _ = exec;
    }

    #[test]
    fn test_reindex_in_inventory() {
        let handlers: Vec<_> = inventory::iter::<&'static dyn OpHandler>
            .into_iter()
            .collect();
        let names: Vec<&str> = handlers.iter().map(|h| h.name()).collect();
        assert!(
            names.contains(&"reindex"),
            "reindex should be in inventory, found: {:?}",
            names
        );
    }
}
