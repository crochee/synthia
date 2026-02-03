//! Synthia Server binary
//!
//! Server starts with empty/default config. APIs populate config at runtime.

use std::path::PathBuf;

use anyhow::Result;
use clap::Parser;
use synthia_server::{ServerConfig, build_agent, run_server};
use tokio::signal;
use tokio_util::sync::CancellationToken;
use tracing::{error, info};

const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Parser, Debug)]
#[command(name = "synthia-server")]
#[command(about = "Synthia Agent HTTP Server", version = VERSION)]
struct Args {
    /// Working directory
    #[arg(short, long, default_value = ".")]
    directory: PathBuf,

    /// Host to bind to
    #[arg(long, default_value = "127.0.0.1")]
    host: String,

    /// Port to bind to
    #[arg(short, long, default_value = "8080")]
    port: u16,
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenv::dotenv().ok();

    let args = Args::parse();

    // Start with default config; APIs will populate it at runtime
    let config = ServerConfig {
        host: args.host.clone(),
        port: args.port,
        ..Default::default()
    };

    info!("Starting Synthia server on {}:{}", config.host, config.port);

    // Build agent (config is API-managed, not file-based)
    let state = build_agent(&args.directory, &config).await?;

    // Create cancellation token for graceful shutdown
    let cancel_token = CancellationToken::new();
    let cancel_token_clone = cancel_token.clone();

    // Spawn task to handle shutdown signals
    tokio::spawn(async move {
        match signal::ctrl_c().await {
            Ok(()) => {
                info!("Received Ctrl+C, initiating graceful shutdown...");
                cancel_token_clone.cancel();
            }
            Err(e) => {
                error!("Failed to listen for shutdown signal: {}", e);
            }
        }
    });

    // Run server with graceful shutdown
    run_server(state, cancel_token).await?;

    info!("Server shutdown complete");

    Ok(())
}
