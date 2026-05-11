use std::sync::Arc;

use tracing::info;

use stele::config::Config;
use stele::fns::FnsClient;
use stele::index::IndexEngine;
use stele::ops::OperationRegistry;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
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

    stele::cli::run_cli(registry)
        .await
        .map_err(|e| anyhow::anyhow!("cli error: {e}"))?;

    Ok(())
}
