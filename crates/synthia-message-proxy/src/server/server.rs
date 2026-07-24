//! The [`MessageProxyServer`] struct + 2 methods (`new` /
//! `serve`).
//!
//! [`MessageProxyServer`] holds the socket path and the
//! shared [`super::state::ProxyState`] used by all gRPC
//! connections. `serve` removes any stale socket file
//! (otherwise `bind` fails with `AddressInUse`), then
//! binds a `tokio::net::UnixListener`, wraps it in
//! [`tokio_stream::wrappers::UnixListenerStream`], and
//! serves the [`super::service::MessageProxyServiceImpl`]
//! until the listener is shut down.

use std::path::Path;

use tokio_stream::wrappers::UnixListenerStream;
use tonic::transport::Server;
use tracing::info;

use super::{service::MessageProxyServiceImpl, state::ProxyState};
use crate::message_proxy::message_proxy_service_server::MessageProxyServiceServer;

/// The `MessageProxy` gRPC server. Holds the socket path
/// and the shared agent registry used by all gRPC
/// connections.
pub struct MessageProxyServer {
    state: ProxyState,
    addr: String,
}

impl MessageProxyServer {
    /// Build a new server bound to the given Unix Domain
    /// Socket path.
    pub fn new(addr: String) -> Self {
        Self {
            state: ProxyState::default(),
            addr,
        }
    }

    /// Bind the UDS listener, register the gRPC service,
    /// and serve requests until the listener is shut down.
    pub async fn serve(self) -> Result<(), Box<dyn std::error::Error>> {
        let path = Path::new(&self.addr);
        // Remove any stale socket file left behind by a previous process;
        // otherwise `bind` would fail with `AddressInUse`.
        if path.exists() {
            std::fs::remove_file(path)?;
        }

        let listener = tokio::net::UnixListener::bind(&self.addr)?;
        info!(addr = %self.addr, "MessageProxy listening");

        let svc = MessageProxyServiceServer::new(MessageProxyServiceImpl {
            state: self.state,
        });

        Server::builder()
            .add_service(svc)
            .serve_with_incoming(UnixListenerStream::new(listener))
            .await?;
        Ok(())
    }
}
