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

    /// Path to an LLM provider configuration file.
    ///
    /// When present and the file exists, the YAML/TOML is parsed and its
    /// provider entries (base_url, api_key, default_model) are bridged
    /// into the runtime `WorkspaceConfig` consumed by `synthia-provider`.
    /// When omitted, the server falls back to `<directory>/.agents/config.toml`
    /// (existing behavior).
    ///
    /// Supported formats: YAML (`.yaml`, `.yml`) and TOML (`.toml`).
    #[arg(long, value_name = "PATH")]
    config: Option<PathBuf>,

    /// Host to bind to
    #[arg(long, default_value = "127.0.0.1")]
    host: String,

    /// Port to bind to
    #[arg(short, long, default_value = "8080")]
    port: u16,
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();

    // Initialize tracing via `synthia-telemetry` so the OTLP
    // pipeline (SYNTHIA_OTLP_ENDPOINT, etc.) and structured
    // layers from that crate are wired up. Falls back to
    // console tracing when OTLP is unset, matching the
    // `tracing_subscriber::fmt` behaviour the previous
    // inlined setup provided.
    synthia_telemetry::init_tracing(&synthia_telemetry::TelemetryConfig {
        service_name: "synthia-server".to_string(),
        log_level: std::env::var("RUST_LOG").unwrap_or_else(|_| "info".into()),
    })
    .map_err(|e| anyhow::anyhow!("failed to init tracing: {e}"))?;

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
    let app = synthia_server::server::create_server(
        args.directory.clone(),
        args.config.as_ref(),
    )
    .await
    .map_err(|e| anyhow::anyhow!("failed to build server: {e}"))?;
    let listener =
        tokio::net::TcpListener::bind(format!("{}:{}", args.host, args.port))
            .await?;

    // Announce the probe endpoints once the router is built and
    // the listener is bound: from this point on, `/livez`
    // answers liveness (process serves HTTP) and `/readyz`
    // answers readiness (bootstrap completed before the bind, so
    // the first probe already reports ready). Orchestrators and
    // load balancers should wire these two URLs into their
    // liveness / readiness probe configuration.
    info!(
        host = %args.host,
        port = args.port,
        livez = format!("http://{}:{}/livez", args.host, args.port),
        readyz = format!("http://{}:{}/readyz", args.host, args.port),
        "listening; probes ready"
    );

    // Use axum server with graceful shutdown
    axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            cancel_token.cancelled().await;
        })
        .await?;

    info!("Server shutdown complete");

    Ok(())
}
