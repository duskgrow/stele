use async_trait::async_trait;
use serde_json::{Value, json};
use crate::ops::handler::{OpHandler, OpExec, OperationContext};

/// Handler struct registered with inventory.
pub struct MaintainHandler;

/// Executable operation with parsed arguments.
pub struct MaintainOp {
    pub scope: Option<String>,
}

#[async_trait]
impl OpExec for MaintainOp {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    async fn execute(&self, ctx: &OperationContext) -> Result<Value, anyhow::Error> {
        crate::ops::maintain::handle_maintain(&ctx.index, self.scope.as_deref())
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }
}

impl OpHandler for MaintainHandler {
    fn name(&self) -> &'static str { "maintain" }
    fn description(&self) -> &'static str { "Run maintenance tasks (lint, orphans, backlinks, full)" }
    
    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "scope": {
                    "type": "string",
                    "enum": ["lint", "orphans", "backlinks", "full"]
                }
            }
        })
    }
    
    fn from_mcp_args(&self, args: Option<serde_json::Map<String, Value>>) -> Result<Box<dyn OpExec>, anyhow::Error> {
        let args = args.unwrap_or_default();
        let scope = args.get("scope")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        Ok(Box::new(MaintainOp { scope }))
    }
    
    fn cli_command(&self) -> clap::Command {
        clap::Command::new("maintain")
            .about("Run maintenance tasks")
            .arg(clap::Arg::new("scope").long("scope").value_parser(["lint", "orphans", "backlinks", "full"]))
    }
    
    fn from_cli_matches(&self, matches: &clap::ArgMatches) -> Result<Box<dyn OpExec>, anyhow::Error> {
        let scope = matches.get_one::<String>("scope").cloned();
        Ok(Box::new(MaintainOp { scope }))
    }
}

inventory::submit! {
    &MaintainHandler as &'static dyn OpHandler
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_maintain_handler_meta() {
        let handler = MaintainHandler;
        assert_eq!(handler.name(), "maintain");
        assert!(!handler.description().is_empty());

        let schema = handler.input_schema();
        assert!(schema.is_object());
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"].is_object());
        assert!(schema["properties"]["scope"].is_object());
    }

    #[test]
    fn test_maintain_from_mcp_args_with_scope() {
        let handler = MaintainHandler;
        let mut args = serde_json::Map::new();
        args.insert("scope".to_string(), serde_json::Value::String("lint".to_string()));
        
        let exec = handler.from_mcp_args(Some(args)).expect("from_mcp_args should succeed");
        let _ = exec;
    }

    #[test]
    fn test_maintain_from_mcp_args_without_scope() {
        let handler = MaintainHandler;
        let exec = handler.from_mcp_args(None).expect("from_mcp_args should succeed without scope");
        let _ = exec;
    }

    #[test]
    fn test_maintain_in_inventory() {
        let handlers: Vec<_> = inventory::iter::<&'static dyn OpHandler>
            .into_iter()
            .collect();
        let names: Vec<&str> = handlers.iter().map(|h| h.name()).collect();
        assert!(names.contains(&"maintain"), "maintain should be in inventory, found: {:?}", names);
    }
}
