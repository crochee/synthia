//! Synthia Server binary
//!
//! Server starts with empty/default config. APIs populate config at runtime.

use std::path::PathBuf;

use anyhow::Result;
use clap::Parser;
use synthia_server::build_info;
use tokio::signal;
use tokio_util::sync::CancellationToken;
use tracing::{error, info};

#[derive(Parser, Debug)]
#[command(name = "synthia-server")]
#[command(
    about = "Synthia Agent HTTP Server",
    version = build_info::LONG_VERSION,
)]
struct Args {
    /// Working directory
    #[arg(short, long, default_value = ".")]
    directory: PathBuf,

    /// Path to the deployment configuration file.
    ///
    /// Every section read at boot comes from this file: `providers`
    /// (base_url / api_key / models), `agents` (`max_steps`,
    /// `compaction`, `strategy`), `[tools]`, `auth`, `cors`,
    /// `operations`, and `mcp_servers`. The format is
    /// chosen by extension, so `.yaml`, `.yml`, and `.toml` all work.
    ///
    /// When omitted — or when the named file is missing or does not
    /// parse — the server tries `<directory>/config.toml`, then
    /// `<directory>/.synthia/config.toml`, and finally its own defaults;
    /// the provider then falls back to
    /// `<directory>/.agents/config.toml` and the environment
    /// (`OPENAI_API_KEY` / `ANTHROPIC_API_KEY`).
    #[arg(long, value_name = "PATH")]
    config: Option<PathBuf>,

    /// Host to bind to.
    ///
    /// Precedence: this flag, then `SYNTHIA_HOST`, then `host` in the
    /// deployment config, then [`DEFAULT_HOST`] (127.0.0.1). The
    /// container images set `SYNTHIA_HOST=0.0.0.0` — a container that
    /// binds loopback is unreachable through a published port.
    #[arg(long, env = "SYNTHIA_HOST")]
    host: Option<String>,

    /// Port to bind to. Same precedence as `--host`.
    #[arg(short, long, env = "SYNTHIA_PORT")]
    port: Option<u16>,
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
    synthia::telemetry::init_tracing(&synthia::telemetry::TelemetryConfig {
        service_name: "synthia-server".to_string(),
        log_level: std::env::var("RUST_LOG").unwrap_or_else(|_| "info".into()),
    })
    .map_err(|e| anyhow::anyhow!("failed to init tracing: {e}"))?;

    let args = Args::parse();

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

    // Create and run server. The boot resolves the deployment config
    // once and returns the address it resolved from the CLI, the
    // environment, and that config — so what is logged, bound, and
    // announced is one value (R54).
    let booted = synthia_server::server::boot_server(
        args.directory.clone(),
        args.config.as_ref(),
        synthia_server::config::server::BindOverrides {
            host: args.host,
            // `args.port` is moved below; the address carries the value.
            port: args.port,
        },
    )
    .await
    .map_err(|e| anyhow::anyhow!("failed to build server: {e}"))?;
    let address = booted.address;
    info!(address = %address, "binding");
    let listener =
        tokio::net::TcpListener::bind((address.host.as_str(), address.port))
            .await
            .map_err(|e| anyhow::anyhow!("failed to bind {address}: {e}"))?;

    // Announce the probe endpoints once the router is built and
    // the listener is bound: from this point on, `/livez`
    // answers liveness (process serves HTTP) and `/readyz`
    // answers readiness (bootstrap completed before the bind, so
    // the first probe already reports ready). Orchestrators and
    // load balancers should wire these two URLs into their
    // liveness / readiness probe configuration.
    info!(
        host = %address.host,
        port = address.port,
        livez = format!("http://{address}/livez"),
        readyz = format!("http://{address}/readyz"),
        "listening; probes ready"
    );

    // Use axum server with graceful shutdown
    axum::serve(listener, booted.router)
        .with_graceful_shutdown(async move {
            cancel_token.cancelled().await;
        })
        .await?;

    info!("Server shutdown complete");

    Ok(())
}

#[cfg(test)]
mod tests {
    use clap::CommandFactory;

    use super::Args;

    /// The container images set `SYNTHIA_HOST` / `SYNTHIA_PORT`
    /// instead of passing flags. This asserts the binding on the
    /// **parsed CLI model** — not on a substring of this file's
    /// source text — so a rename of the env var or a dropped
    /// `env = …` attribute fails here.
    #[test]
    fn the_cli_binds_the_env_vars_the_images_set() {
        let command = Args::command();
        let env_of = |id: &str| {
            command
                .get_arguments()
                .find(|arg| arg.get_id() == id)
                .and_then(|arg| arg.get_env())
                .map(|value| value.to_string_lossy().into_owned())
        };
        assert_eq!(env_of("host").as_deref(), Some("SYNTHIA_HOST"));
        assert_eq!(env_of("port").as_deref(), Some("SYNTHIA_PORT"));
    }
}
