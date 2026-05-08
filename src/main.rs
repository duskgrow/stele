use std::sync::Arc;

use clap::Parser;
use tracing::info;

use stele::cli::{Commands, SteleCli};
use stele::config::Config;
use stele::fns::FnsClient;
use stele::index::IndexEngine;
use stele::ops::OperationRegistry;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = match SteleCli::try_parse() {
        Ok(c) => c,
        Err(e) => {
            let code = if e.use_stderr() { 2 } else { 0 };
            eprintln!("{}", e);
            std::process::exit(code);
        }
    };

    let config = Config::load().map_err(|e| anyhow::anyhow!("config load failed: {e}"))?;

    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    info!("stele v{} starting", env!("CARGO_PKG_VERSION"));
    info!("config loaded");

    let fns = FnsClient::new(
        config.fns.base_url.clone(),
        config.fns.token.clone(),
        config.fns.vault.clone(),
    );
    info!("FNS client created");

    let index = IndexEngine::new(&config.index.db_path).await?;
    info!("index engine initialized");

    let registry = Arc::new(OperationRegistry::new(
        Arc::new(fns),
        Arc::new(index),
        config.clone(),
    ));

    match cli.command {
        Commands::Serve { transport, port } => {
            if transport == "stdio" {
                info!("starting MCP server on stdio transport");
                tokio::select! {
                    result = stele::mcp::stdio::run_stdio(registry) => {
                        result.map_err(|e| anyhow::anyhow!("stdio server error: {e}"))?;
                    }
                    _ = tokio::signal::ctrl_c() => {
                        info!("Received SIGINT, shutting down gracefully");
                    }
                }
            } else if transport == "http" {
                let host = &config.server.host;
                info!("starting MCP HTTP server on {}:{}", host, port);
                tokio::select! {
                    result = stele::mcp::http::run_http(registry, host, port) => {
                        result.map_err(|e| anyhow::anyhow!("http server error: {e}"))?;
                    }
                    _ = tokio::signal::ctrl_c() => {
                        info!("Received SIGINT, shutting down gracefully");
                    }
                }
            } else {
                anyhow::bail!("unknown transport: {}", transport);
            }
        }
        _ => {
            stele::cli::run_cli(registry)
                .await
                .map_err(|e| anyhow::anyhow!("cli error: {e}"))?;
        }
    }

    Ok(())
}
