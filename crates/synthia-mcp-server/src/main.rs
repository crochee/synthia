//! `synthia-mcp-server` — stdio MCP server binary.
//!
//! Spawnable by any MCP client (`pi`, a Claude Code plugin, the
//! Anthropic SDK, …) via:
//!
//! ```text
//! synthia-mcp-server --workspace /path/to/project
//! ```
//!
//! See `lib.rs` for the wire shape and the tool surface; the
//! binary is a thin shim around [`synthia_mcp_server::run`].
//!
//! ## Why the binary wraps `tokio::io::stdin()` / `stdout()`
//!
//! The library's `run` function is generic over any
//! `tokio::io::AsyncRead` / `AsyncWrite`, so it never names a
//! runtime type in its public API (§3.7). The binary itself
//! is the one place that does — and is exempt, exactly as
//! `synthia-server` is.

use std::process::ExitCode;

use synthia_mcp::ServerInfo;
use synthia_mcp_server::{ServerSurface, parse_argv};

#[tokio::main]
async fn main() -> ExitCode {
    // stderr-only logs so stdout stays a pure JSON-RPC channel.
    let _ = tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with_target(false)
        .try_init();

    let argv: Vec<String> = std::env::args().skip(1).collect();
    let config = match parse_argv(argv) {
        Ok(config) => config,
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::from(2);
        }
    };

    tracing::info!(
        server = %config.server_name,
        workspace = %config.workspace_root.display(),
        "synthia-mcp-server starting",
    );

    let surface = ServerSurface::new(&config.workspace_root);
    let info =
        ServerInfo::new(config.server_name.clone(), env!("CARGO_PKG_VERSION"));

    // The library's `run` is generic over `AsyncRead` / `AsyncWrite`.
    // The binary hands it the process's own — and keeps the runtime
    // types entirely inside `main`, where the §3.7 exemption applies.
    match synthia_mcp_server::run(
        surface,
        info,
        tokio::io::stdin(),
        tokio::io::stdout(),
    )
    .await
    {
        Ok(()) => {
            tracing::info!("stdin closed; exiting cleanly");
            ExitCode::SUCCESS
        }
        Err(error) => {
            tracing::error!(%error, "stdio pump failed");
            ExitCode::from(1)
        }
    }
}
