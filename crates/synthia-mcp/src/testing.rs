//! [`InMemoryTransport`] — scripted MCP transport for tests.
//!
//! Public (not `#[cfg(test)]`) on purpose: a lib consumer that
//! embeds synthia also needs to test *its* MCP wiring without
//! spawning processes. Record every request; return the scripted
//! result for each method.

use std::{
    collections::HashMap,
    sync::{
        Arc,
        Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

use async_trait::async_trait;
use serde_json::{Value, json};
use synthia_core::Error;

use crate::{McpTransport, SharedTransport, supervisor::TransportFactory};

/// One recorded call.
#[derive(Clone, Debug, PartialEq)]
pub struct RecordedCall {
    pub method: String,
    pub params: Value,
    /// True for `notify` (no response expected).
    pub is_notification: bool,
}

/// Scripted, in-process [`McpTransport`].
pub struct InMemoryTransport {
    responses: parking_lot::RwLock<HashMap<String, Value>>,
    calls: Mutex<Vec<RecordedCall>>,
    /// When set, the next `request` fails with this message
    /// instead of returning a scripted result.
    fail_with: Mutex<Option<String>>,
    /// When false, EVERY request fails — a persistent outage
    /// (the counterpart of [`Self::fail_next`]'s one-shot
    /// failure). Flip with [`Self::set_alive`].
    alive: AtomicBool,
}

impl Default for InMemoryTransport {
    fn default() -> Self {
        Self {
            responses: parking_lot::RwLock::new(HashMap::new()),
            calls: Mutex::new(Vec::new()),
            fail_with: Mutex::new(None),
            alive: AtomicBool::new(true),
        }
    }
}

impl InMemoryTransport {
    /// Empty transport (every method errors until scripted).
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Script `method` to return `result`. Returns `self` for
    /// chaining.
    #[must_use]
    pub fn on(self, method: &str, result: Value) -> Self {
        self.set_script(method, result);
        self
    }

    /// Replace the script for `method` after construction —
    /// the seam for re-sync tests (a server whose `tools/list`
    /// answer changes between generations).
    pub fn set_script(&self, method: &str, result: Value) {
        self.responses.write().insert(method.to_string(), result);
    }

    /// Whether the transport currently accepts requests.
    #[must_use]
    pub fn is_alive(&self) -> bool {
        self.alive.load(Ordering::SeqCst)
    }

    /// Flip the outage switch. While `false`, EVERY `request`
    /// fails (persistent failure, unlike one-shot
    /// [`Self::fail_next`]); `true` revives the transport.
    /// This is how supervisor reconnect tests script an
    /// outage-and-recovery.
    pub fn set_alive(&self, alive: bool) {
        self.alive.store(alive, Ordering::SeqCst);
    }

    /// Script the standard handshake + one `echo` tool.
    #[must_use]
    pub fn with_echo_server(self) -> Self {
        self.on(
            "initialize",
            json!({
                "protocolVersion": crate::MCP_PROTOCOL_VERSION,
                "capabilities": {},
                "serverInfo": {"name": "in-memory-test", "version": "1"}
            }),
        )
        .on(
            "tools/list",
            json!({
                "tools": [{
                    "name": "echo",
                    "description": "Echo the input back",
                    "inputSchema": {
                        "type": "object",
                        "properties": {"text": {"type": "string"}},
                        "required": ["text"]
                    }
                }]
            }),
        )
        .on(
            "tools/call",
            json!({
                "content": [{"type": "text", "text": "echoed"}],
                "isError": false
            }),
        )
    }

    /// Make the next `request` fail (transport-level).
    pub fn fail_next(&self, message: impl Into<String>) {
        *self.fail_with.lock().expect("fail mutex") = Some(message.into());
    }

    /// Every call recorded so far, in order.
    #[must_use]
    pub fn recorded(&self) -> Vec<RecordedCall> {
        self.calls.lock().expect("calls mutex").clone()
    }

    /// Methods recorded so far, in order.
    #[must_use]
    pub fn methods(&self) -> Vec<String> {
        self.recorded().into_iter().map(|c| c.method).collect()
    }

    /// Wrap in an `Arc` for the client constructors.
    #[must_use]
    pub fn shared(self) -> Arc<dyn McpTransport> {
        Arc::new(self)
    }
}

#[async_trait]
impl McpTransport for InMemoryTransport {
    async fn request(
        &self,
        _id: u64,
        method: &str,
        params: Value,
    ) -> Result<Value, Error> {
        self.calls.lock().expect("calls mutex").push(RecordedCall {
            method: method.to_string(),
            params,
            is_notification: false,
        });
        if !self.is_alive() {
            return Err(Error::ToolExecution {
                message: "InMemoryTransport: transport down".to_string(),
            });
        }
        if let Some(message) = self.fail_with.lock().expect("fail mutex").take()
        {
            return Err(Error::ToolExecution { message });
        }
        self.responses.read().get(method).cloned().ok_or_else(|| {
            Error::ToolExecution {
                message: format!("InMemoryTransport: no script for `{method}`"),
            }
        })
    }

    async fn notify(&self, method: &str, params: Value) -> Result<(), Error> {
        self.calls.lock().expect("calls mutex").push(RecordedCall {
            method: method.to_string(),
            params,
            is_notification: true,
        });
        Ok(())
    }

    fn describe(&self) -> String {
        "in-memory".to_string()
    }
}

/// [`TransportFactory`] handing out clones of one shared
/// [`InMemoryTransport`].
///
/// Every "connection attempt" lands on the same scripted wire, so
/// flipping [`InMemoryTransport::set_alive`] scripts an outage and
/// recovery for supervisor tests without a process or a socket.
pub struct InMemoryTransportFactory {
    transport: Arc<InMemoryTransport>,
}

impl InMemoryTransportFactory {
    /// Wrap `transport`; every `connect` shares it.
    #[must_use]
    pub fn new(transport: Arc<InMemoryTransport>) -> Self {
        Self { transport }
    }

    /// The shared transport (for scripting / assertions).
    #[must_use]
    pub fn transport(&self) -> &Arc<InMemoryTransport> {
        &self.transport
    }
}

#[async_trait]
impl TransportFactory for InMemoryTransportFactory {
    async fn connect(&self) -> Result<SharedTransport, Error> {
        Ok(Arc::clone(&self.transport) as SharedTransport)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn unscripted_method_errors() {
        let t = InMemoryTransport::new();
        let err = t.request(1, "tools/list", json!({})).await.unwrap_err();
        assert!(err.to_string().contains("no script"));
    }

    #[tokio::test]
    async fn records_requests_and_notifications() {
        let t = InMemoryTransport::new().with_echo_server();
        let _ = t.request(1, "initialize", json!({})).await.unwrap();
        t.notify("notifications/initialized", json!({}))
            .await
            .unwrap();
        assert_eq!(
            t.methods(),
            vec!["initialize", "notifications/initialized"]
        );
        assert!(t.recorded()[1].is_notification);
    }

    #[tokio::test]
    async fn fail_next_surfaces_transport_error() {
        let t = InMemoryTransport::new().with_echo_server();
        t.fail_next("boom");
        let err = t.request(1, "initialize", json!({})).await.unwrap_err();
        assert!(err.to_string().contains("boom"));
        // The failure is one-shot.
        assert!(t.request(2, "initialize", json!({})).await.is_ok());
    }
}
