use crate::ops::handler::{OpExec, OpHandler, OperationContext};
use async_trait::async_trait;
use serde_json::{Value, json};

/// Handler struct registered with inventory.
pub struct PageGetHandler;

/// Executable operation with parsed arguments.
pub struct PageGetOp {
    pub slug: String,
}

#[async_trait]
impl OpExec for PageGetOp {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    async fn execute(&self, ctx: &OperationContext) -> Result<Value, anyhow::Error> {
        crate::ops::page::handle_page_get(&ctx.fns, &ctx.index, &self.slug)
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }
}

impl OpHandler for PageGetHandler {
    fn name(&self) -> &'static str {
        "page.get"
    }
    fn description(&self) -> &'static str {
        "Retrieve a page by slug"
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "slug": { "type": "string" }
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
        Ok(Box::new(PageGetOp { slug }))
    }

    fn cli_command(&self) -> clap::Command {
        clap::Command::new("get")
            .about("Get a page by slug")
            .arg(clap::Arg::new("slug").required(true))
    }

    fn from_cli_matches(
        &self,
        matches: &clap::ArgMatches,
    ) -> Result<Box<dyn OpExec>, anyhow::Error> {
        let slug = matches
            .get_one::<String>("slug")
            .ok_or_else(|| anyhow::anyhow!("missing required argument: slug"))?
            .clone();
        Ok(Box::new(PageGetOp { slug }))
    }
}

inventory::submit! {
    &PageGetHandler as &'static dyn OpHandler
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_page_get_handler_meta() {
        let handler = PageGetHandler;
        assert_eq!(handler.name(), "page.get");
        assert!(!handler.description().is_empty());

        let schema = handler.input_schema();
        assert!(schema.is_object());
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"].is_object());
        assert!(schema["properties"]["slug"].is_object());
    }

    #[test]
    fn test_page_get_from_mcp_args() {
        let handler = PageGetHandler;
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
    fn test_page_get_missing_slug() {
        let handler = PageGetHandler;
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
    fn test_page_get_in_inventory() {
        let handlers: Vec<_> = inventory::iter::<&'static dyn OpHandler>
            .into_iter()
            .collect();
        let names: Vec<&str> = handlers.iter().map(|h| h.name()).collect();
        assert!(
            names.contains(&"page.get"),
            "page.get should be in inventory, found: {:?}",
            names
        );
    }
}
