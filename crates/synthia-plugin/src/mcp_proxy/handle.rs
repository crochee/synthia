//! The 2 internal handle types that live inside the
//! [`super::core::McpProxy::servers`] map:
//!
//! - [`ServerHandle`] — the enum that distinguishes a
//!   live stdio `Child` from a validated-but-not-retained
//!   network connection.
//! - [`NetworkHandle`] — the inert struct for the network
//!   variant (currently only records the `Transport` for
//!   diagnostics in `stop_server`).

use tokio::process::Child;

use crate::types::Transport;

#[derive(Debug)]
pub enum ServerHandle {
    /// Stdio child process
    Stdio(Child),
    /// Network connection handle (placeholder for HTTP/WS)
    Network(NetworkHandle),
}

/// Network connection handle for HTTP/SSE/WS transports.
///
/// Network transports are currently validated at start time (HTTP probe)
/// but the live connection is not retained. The handle only records the
/// transport type for diagnostics in `stop_server`.
#[derive(Debug)]
pub struct NetworkHandle {
    /// The transport type
    pub transport: Transport,
}

impl NetworkHandle {
    pub fn new(transport: Transport) -> Self {
        Self { transport }
    }
}
