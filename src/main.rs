use std::sync::Arc;

use clap::Parser;
use tracing::info;

use wikiops::config;
use wikiops::mcp::resources::ResourceRegistry;
use wikiops::mcp::server::McpServer;
use wikiops::mcp::transport::run_server;
use wikiops::services::tools::ToolRegistry;
use wikiops::storage::fns::FnsBackend;
use wikiops::storage::sqlite::SqliteBackend;

#[derive(Parser)]
#[command(name = "wikiops", version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(clap::Subcommand)]
enum Commands {
    /// Start the MCP server
    Serve {
        /// Path to config file
        #[arg(short, long)]
        config: Option<String>,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Serve { config } => {
            let config = config::Config::load(config.as_deref())?;

            tracing_subscriber::fmt()
                .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
                .init();

            info!("wikiops MCP server starting");

            let fns_backend = Arc::new(FnsBackend::new(
                config.storage.fns.base_url.clone(),
                config.storage.fns.api_token.clone(),
                config.storage.fns.default_vault.clone(),
            ));

            let sqlite_backend = Arc::new(SqliteBackend::new(&config.index.db_path).await?);

            info!("SQLite backend initialized with migrations");

            let tool_registry = Arc::new(
                ToolRegistry::new(sqlite_backend)
                    .with_file_backend(fns_backend.clone())
                    .with_vault(config.storage.fns.default_vault.clone()),
            );

            let resource_registry = Arc::new(ResourceRegistry::new(fns_backend.clone()));

            let mcp_server = Arc::new(McpServer::new(
                tool_registry,
                resource_registry,
                fns_backend,
            ));

            info!(
                "wikiops v{} ready — listening on {}:{}",
                env!("CARGO_PKG_VERSION"),
                config.server.host,
                config.server.port
            );

            run_server(
                mcp_server,
                config.mcp,
                &config.server.host,
                config.server.port,
            )
            .await?;
        }
    }

    Ok(())
}
