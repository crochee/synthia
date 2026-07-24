//! MCP Proxy — manages MCP server lifecycle.
//!
//! # Architecture
//!
//! The proxy supports 4 transport types (Stdio, SSE, HTTP, WS).
//! Stdio servers are spawned as child processes; network servers
//! are validated at start time (HTTP probe or WS handshake) and
//! the live connection is not retained.
//!
//! # Module Layout
//!
//! - [`error`]: [`error::McpProxyError`] (8 variants).
//! - [`handle`]: The 2 internal handle types
//!   ([`handle::ServerHandle`] enum + [`handle::NetworkHandle`]).
//! - [`core`]: [`core::McpProxy`] struct + `Default` + `Drop`
//!   + the 3 constructor methods (`new` / `default` /
//!     `with_startup_timeout`).
//! - [`start`]: The 5 start methods (1 dispatcher +
//!   `start_stdio_server` / `start_sse_server` /
//!   `start_http_server` / `start_ws_server`).
//! - [`stop`]: The 5 stop/status methods (`stop_server` /
//!   `stop_all` / `shutdown` / `running_servers` /
//!   `is_running`).
//! - [`tests`]: 10 unit tests covering creation, config
//!   validation, unknown server stop, transport
//!   discrimination, and the async paths.
//!
//! # Lifetime Caveat
//!
//! [`core::McpProxy`] does **not** run async cleanup in
//! [`Drop`] (Rust does not support async in `Drop`). Callers
//! **must** call [`core::McpProxy::shutdown`] explicitly.

mod core;
mod error;
mod handle;
mod start;
mod stop;

#[cfg(test)]
mod tests;

pub use core::McpProxy;

pub use error::McpProxyError;
