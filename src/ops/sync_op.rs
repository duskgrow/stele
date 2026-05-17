use crate::ops::handler::{OpExec, OpHandler, OperationContext};
use async_trait::async_trait;
use serde_json::{Value, json};

/// Handler struct registered with inventory.
pub struct SyncHandler;

/// Executable operation with parsed arguments.
pub struct SyncOp {
    pub dir: Option<String>,
}

#[async_trait]
impl OpExec for SyncOp {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    async fn execute(&self, ctx: &OperationContext) -> Result<Value, anyhow::Error> {
        crate::ops::sync::handle_sync(&ctx.fns, &ctx.index, self.dir.as_deref())
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }
}

impl OpHandler for SyncHandler {
    fn name(&self) -> &'static str {
        "sync"
    }
    fn description(&self) -> &'static str {
        "Sync pages from FNS vault"
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "dir": { "type": "string", "description": "Directory to sync (default: root)" }
            }
        })
    }

    fn from_mcp_args(
        &self,
        args: Option<serde_json::Map<String, Value>>,
    ) -> Result<Box<dyn OpExec>, anyhow::Error> {
        let args = args.unwrap_or_default();
        let dir = args
            .get("dir")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        Ok(Box::new(SyncOp { dir }))
    }

    fn cli_command(&self) -> clap::Command {
        clap::Command::new("sync")
            .about("Sync from FNS vault")
            .arg(clap::Arg::new("dir").long("dir"))
    }

    fn from_cli_matches(
        &self,
        matches: &clap::ArgMatches,
    ) -> Result<Box<dyn OpExec>, anyhow::Error> {
        let dir = matches.get_one::<String>("dir").cloned();
        Ok(Box::new(SyncOp { dir }))
    }
}

inventory::submit! {
    &SyncHandler as &'static dyn OpHandler
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sync_handler_meta() {
        let handler = SyncHandler;
        assert_eq!(handler.name(), "sync");
        assert!(!handler.description().is_empty());

        let schema = handler.input_schema();
        assert!(schema.is_object());
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"].is_object());
        assert!(schema["properties"]["dir"].is_object());
    }

    #[test]
    fn test_sync_from_mcp_args_with_dir() {
        let handler = SyncHandler;
        let mut args = serde_json::Map::new();
        args.insert(
            "dir".to_string(),
            serde_json::Value::String("/notes".to_string()),
        );

        let exec = handler
            .from_mcp_args(Some(args))
            .expect("from_mcp_args should succeed");
        let _ = exec;
    }

    #[test]
    fn test_sync_from_mcp_args_without_dir() {
        let handler = SyncHandler;
        let exec = handler
            .from_mcp_args(None)
            .expect("from_mcp_args should succeed without dir");
        let _ = exec;
    }

    #[test]
    fn test_sync_in_inventory() {
        let handlers: Vec<_> = inventory::iter::<&'static dyn OpHandler>
            .into_iter()
            .collect();
        let names: Vec<&str> = handlers.iter().map(|h| h.name()).collect();
        assert!(
            names.contains(&"sync"),
            "sync should be in inventory, found: {:?}",
            names
        );
    }
}
