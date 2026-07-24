//! Synthia Server binary
//!
//! Server starts with empty/default config. APIs populate config at runtime.

use std::path::PathBuf;

use anyhow::Result;
use clap::Parser;
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

    info!("Starting Synthia server on {}:{}", args.host, args.port);

    // Create cancellation token for graceful shutdown
    let cancel_token = CancellationToken::new();
    let cancel_token_for_shutdown = cancel_token.clone();

    // Spawn task to handle shutdown signals
    tokio::spawn(async move {
        match signal::ctrl_c().await {
            Ok(()) => {
                info!("Received Ctrl+C, initiating graceful shutdown...");
                cancel_token_for_shutdown.cancel();
            }
            Err(e) => {
                error!("Failed to listen for shutdown signal: {}", e);
            }
        }
    });

    // Create and run server
    let app =
        synthia_server::server::create_server(args.directory.clone()).await;
    let listener =
        tokio::net::TcpListener::bind(format!("{}:{}", args.host, args.port))
            .await?;

    // Use axum server with graceful shutdown
    axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            cancel_token.cancelled().await;
        })
        .await?;

    info!("Server shutdown complete");

    Ok(())
}
