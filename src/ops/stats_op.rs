use crate::ops::handler::{OpExec, OpHandler, OperationContext};
use async_trait::async_trait;
use serde_json::{Value, json};

/// Handler struct registered with inventory.
pub struct StatsHandler;

/// Executable operation with parsed arguments.
pub struct StatsOp;

#[async_trait]
impl OpExec for StatsOp {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    async fn execute(&self, ctx: &OperationContext) -> Result<Value, anyhow::Error> {
        crate::ops::search::handle_stats(&ctx.index)
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }
}

impl OpHandler for StatsHandler {
    fn name(&self) -> &'static str {
        "stats"
    }
    fn description(&self) -> &'static str {
        "Get index statistics"
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
        Ok(Box::new(StatsOp))
    }

    fn cli_command(&self) -> clap::Command {
        clap::Command::new("stats").about("Get index statistics")
    }

    fn from_cli_matches(
        &self,
        _matches: &clap::ArgMatches,
    ) -> Result<Box<dyn OpExec>, anyhow::Error> {
        Ok(Box::new(StatsOp))
    }
}

inventory::submit! {
    &StatsHandler as &'static dyn OpHandler
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stats_handler_meta() {
        let handler = StatsHandler;
        assert_eq!(handler.name(), "stats");
        assert!(!handler.description().is_empty());

        let schema = handler.input_schema();
        assert!(schema.is_object());
        assert_eq!(schema["type"], "object");
    }

    #[test]
    fn test_stats_from_mcp_args() {
        let handler = StatsHandler;
        let exec = handler
            .from_mcp_args(None)
            .expect("from_mcp_args should succeed");
        let _ = exec;
    }

    #[test]
    fn test_stats_in_inventory() {
        let handlers: Vec<_> = inventory::iter::<&'static dyn OpHandler>
            .into_iter()
            .collect();
        let names: Vec<&str> = handlers.iter().map(|h| h.name()).collect();
        assert!(
            names.contains(&"stats"),
            "stats should be in inventory, found: {:?}",
            names
        );
    }
}
