use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;

use crate::config::Config;
use crate::fns::FnsClient;
use crate::index::IndexEngine;

/// Shared context passed to all operation executors.
pub struct OperationContext {
    pub fns: Arc<FnsClient>,
    pub index: Arc<IndexEngine>,
    pub config: Config,
}

/// Trait for operation executors - the actual work unit.
#[async_trait]
pub trait OpExec: Send + Sync {
    async fn execute(&self, ctx: &OperationContext) -> Result<Value, anyhow::Error>;
    fn as_any(&self) -> &dyn std::any::Any;
}

/// Trait for operation handlers - provides metadata, parsing, and CLI integration.
/// Each op implements this trait and registers via `inventory::submit!`.
pub trait OpHandler: Send + Sync + 'static {
    /// MCP tool name (e.g., "page.get")
    fn name(&self) -> &'static str;
    /// Human-readable description
    fn description(&self) -> &'static str;
    /// JSON Schema for MCP input parameters
    fn input_schema(&self) -> Value;
    /// Parse MCP JSON arguments into an executable op
    #[allow(clippy::wrong_self_convention)]
    fn from_mcp_args(
        &self,
        args: Option<serde_json::Map<String, Value>>,
    ) -> Result<Box<dyn OpExec>, anyhow::Error>;
    /// Build clap::Command for CLI subcommand
    fn cli_command(&self) -> clap::Command;
    /// Parse clap ArgMatches into executable op
    #[allow(clippy::wrong_self_convention)]
    fn from_cli_matches(
        &self,
        matches: &clap::ArgMatches,
    ) -> Result<Box<dyn OpExec>, anyhow::Error>;
}

// Register &'static dyn OpHandler with inventory for auto-collection.
// References are used because Box::new is not const and cannot appear in
// the static context that inventory::submit! expands to.
inventory::collect!(&'static dyn OpHandler);

#[cfg(test)]
mod tests {
    use super::*;

    /// Dummy OpExec for testing.
    struct DummyExec;

    #[async_trait]
    impl OpExec for DummyExec {
        async fn execute(&self, _ctx: &OperationContext) -> Result<Value, anyhow::Error> {
            Ok(serde_json::json!({"ok": true}))
        }

        fn as_any(&self) -> &dyn std::any::Any {
            self
        }
    }

    /// Dummy OpHandler for testing.
    struct DummyHandler;

    impl OpHandler for DummyHandler {
        fn name(&self) -> &'static str {
            "test.dummy"
        }

        fn description(&self) -> &'static str {
            "A dummy handler for testing"
        }

        fn input_schema(&self) -> Value {
            serde_json::json!({
                "type": "object",
                "properties": {
                    "x": { "type": "string" }
                }
            })
        }

        fn from_mcp_args(
            &self,
            _args: Option<serde_json::Map<String, Value>>,
        ) -> Result<Box<dyn OpExec>, anyhow::Error> {
            Ok(Box::new(DummyExec))
        }

        fn cli_command(&self) -> clap::Command {
            clap::Command::new("dummy").about("Dummy command")
        }

        fn from_cli_matches(
            &self,
            _matches: &clap::ArgMatches,
        ) -> Result<Box<dyn OpExec>, anyhow::Error> {
            Ok(Box::new(DummyExec))
        }
    }

    /// Test that OperationContext has the expected fields.
    #[tokio::test]
    async fn test_operation_context_fields() {
        use crate::test_utils::*;
        let server = wiremock::MockServer::start().await;
        let fns = test_fns(&server.uri()).await;
        let index = test_index().await;
        let config = Config::default();

        let ctx = OperationContext { fns, index, config };

        // Verify all fields are accessible
        let _ = &ctx.fns;
        let _ = &ctx.index;
        let _ = &ctx.config;
    }

    #[test]
    fn test_inventory_collection() {
        let handlers: Vec<_> = inventory::iter::<&'static dyn OpHandler>
            .into_iter()
            .collect();
        // The collect!() declaration compiles and iter works — handlers will be
        // populated as concrete ops register via inventory::submit! in Tasks 3-13.
        let _ = handlers;
    }

    /// Test that OpHandler trait methods work correctly.
    #[test]
    fn test_op_handler_trait_methods() {
        let handler = DummyHandler;

        assert_eq!(handler.name(), "test.dummy");
        assert_eq!(handler.description(), "A dummy handler for testing");

        let schema = handler.input_schema();
        assert!(schema.is_object());
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"].is_object());
        assert!(schema["properties"]["x"].is_object());

        // Test from_mcp_args
        let exec = handler
            .from_mcp_args(None)
            .expect("from_mcp_args should succeed");
        // We can't easily call execute without a real OperationContext, but we can verify it returns Ok
        let _ = exec;

        // Test cli_command
        let cmd = handler.cli_command();
        assert_eq!(cmd.get_name(), "dummy");

        // Test from_cli_matches
        let matches = cmd.try_get_matches_from(["dummy"]).unwrap();
        let exec = handler
            .from_cli_matches(&matches)
            .expect("from_cli_matches should succeed");
        let _ = exec;
    }
}
