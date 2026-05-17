use crate::ops::handler::{OpExec, OpHandler, OperationContext};
use async_trait::async_trait;
use serde_json::{Value, json};

/// Handler struct registered with inventory.
pub struct PageListHandler;

/// Executable operation with parsed arguments.
pub struct PageListOp {
    pub dir: Option<String>,
}

#[async_trait]
impl OpExec for PageListOp {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    async fn execute(&self, ctx: &OperationContext) -> Result<Value, anyhow::Error> {
        crate::ops::page::handle_page_list(&ctx.fns, self.dir.as_deref())
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }
}

impl OpHandler for PageListHandler {
    fn name(&self) -> &'static str {
        "page.list"
    }
    fn description(&self) -> &'static str {
        "List pages"
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "dir": { "type": "string" }
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
        Ok(Box::new(PageListOp { dir }))
    }

    fn cli_command(&self) -> clap::Command {
        clap::Command::new("list")
            .about("List pages")
            .arg(clap::Arg::new("dir").required(false))
    }

    fn from_cli_matches(
        &self,
        matches: &clap::ArgMatches,
    ) -> Result<Box<dyn OpExec>, anyhow::Error> {
        let dir = matches.get_one::<String>("dir").cloned();
        Ok(Box::new(PageListOp { dir }))
    }
}

inventory::submit! {
    &PageListHandler as &'static dyn OpHandler
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_page_list_handler_meta() {
        let handler = PageListHandler;
        assert_eq!(handler.name(), "page.list");
        assert!(!handler.description().is_empty());

        let schema = handler.input_schema();
        assert!(schema.is_object());
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"].is_object());
        assert!(schema["properties"]["dir"].is_object());
    }

    #[test]
    fn test_page_list_from_mcp_args_with_dir() {
        let handler = PageListHandler;
        let mut args = serde_json::Map::new();
        args.insert(
            "dir".to_string(),
            serde_json::Value::String("notes".to_string()),
        );

        let exec = handler
            .from_mcp_args(Some(args))
            .expect("from_mcp_args should succeed");
        let _ = exec;
    }

    #[test]
    fn test_page_list_from_mcp_args_without_dir() {
        let handler = PageListHandler;

        let exec = handler
            .from_mcp_args(None)
            .expect("from_mcp_args should succeed without dir");
        let _ = exec;
    }

    #[test]
    fn test_page_list_in_inventory() {
        let handlers: Vec<_> = inventory::iter::<&'static dyn OpHandler>
            .into_iter()
            .collect();
        let names: Vec<&str> = handlers.iter().map(|h| h.name()).collect();
        assert!(
            names.contains(&"page.list"),
            "page.list should be in inventory, found: {:?}",
            names
        );
    }
}
