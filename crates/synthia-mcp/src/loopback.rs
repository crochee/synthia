//! [`LoopbackTransport`] — an in-process MCP transport pair.
//!
//! `loopback_pair()` returns a client-side [`McpTransport`] and the
//! matching [`LoopbackServer`] inbox. Requests travel over an
//! unbounded `futures` channel; responses return through per-id
//! `oneshot` slots the server pump completes with
//! [`LoopbackServer::respond`]. No task, timer, or runtime is
//! created here — the caller drives the server side (usually with
//! [`crate::server::serve`]).
//!
//! This is the seam an **in-process MCP server** plugs into: a
//! process that wants to expose its own surface over the protocol
//! builds the pair, hands the client side to a plain [`McpClient`](crate::McpClient),
//! and pumps the server side itself — the whole existing client
//! stack (naming, generations, registration) then applies to the
//! process's own tools with zero process spawn or network.

use std::{collections::HashMap, sync::Arc};

use futures::channel::{mpsc, oneshot};
use parking_lot::Mutex;
use serde_json::Value;
use synthia_core::Error;

use crate::{
    McpTransport,
    notification_envelope,
    request_envelope,
    unwrap_response,
};

/// State shared between the client side and the server pump.
struct Shared {
    /// Envelopes on their way to the server pump.
    tx: mpsc::UnboundedSender<Value>,
    /// One open reply slot per in-flight request id.
    pending: Mutex<HashMap<u64, oneshot::Sender<Value>>>,
}

/// The client side of a loopback pair. Safe to clone-share (the
/// [`McpTransport`] contract wants `Arc<dyn …>` callers).
pub struct LoopbackTransport {
    shared: Arc<Shared>,
    label: String,
}

#[async_trait::async_trait]
impl McpTransport for LoopbackTransport {
    async fn request(
        &self,
        id: u64,
        method: &str,
        params: Value,
    ) -> Result<Value, Error> {
        let (resp_tx, resp_rx) = oneshot::channel();
        self.shared.pending.lock().insert(id, resp_tx);
        let send = self
            .shared
            .tx
            .unbounded_send(request_envelope(id, method, &params));
        if send.is_err() {
            self.shared.pending.lock().remove(&id);
            return Err(Error::ToolExecution {
                message: "loopback server is gone".to_string(),
            });
        }
        let response = match resp_rx.await {
            Ok(response) => response,
            Err(_) => {
                // The pump dropped the slot without answering —
                // treat it as the transport closing, not a protocol
                // error the caller could act on.
                return Err(Error::ToolExecution {
                    message: "loopback server dropped the request".to_string(),
                });
            }
        };
        unwrap_response(response)
    }

    async fn notify(&self, method: &str, params: Value) -> Result<(), Error> {
        self.shared
            .tx
            .unbounded_send(notification_envelope(method, &params))
            .map_err(|_| Error::ToolExecution {
                message: "loopback server is gone".to_string(),
            })
    }

    fn describe(&self) -> String {
        format!("loopback: {}", self.label)
    }
}

/// The server side of a loopback pair: envelopes in, responses out.
pub struct LoopbackServer {
    rx: mpsc::UnboundedReceiver<Value>,
    shared: Arc<Shared>,
}

impl LoopbackServer {
    /// Await the next envelope (request or notification) from the
    /// client side. `None` once every transport handle is dropped.
    pub async fn next_envelope(&mut self) -> Option<Value> {
        use futures::StreamExt;
        self.rx.next().await
    }

    /// Complete the pending request `response` addresses, by its
    /// `id`. A response for an unknown id (already answered, or a
    /// client that went away) is dropped — the client side already
    /// reported the failure to its caller.
    pub fn respond(&self, response: Value) {
        if let Some(id) = response.get("id").and_then(Value::as_u64)
            && let Some(slot) = self.shared.pending.lock().remove(&id)
        {
            let _ = slot.send(response);
        }
    }
}

/// Build a connected pair. The client side is the [`McpTransport`]
/// every existing consumer takes; the server side is the inbox a
/// pump (such as [`crate::server::serve`]) drains.
#[must_use]
pub fn loopback_pair(
    label: impl Into<String>,
) -> (Arc<LoopbackTransport>, LoopbackServer) {
    let (tx, rx) = mpsc::unbounded();
    let shared = Arc::new(Shared {
        tx,
        pending: Mutex::new(HashMap::new()),
    });
    let client = Arc::new(LoopbackTransport {
        shared: Arc::clone(&shared),
        label: label.into(),
    });
    let server = LoopbackServer { rx, shared };
    (client, server)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The pair carries a request to the pump and its response
    /// back to the awaiting client, matched by id.
    #[tokio::test]
    async fn round_trips_one_request() {
        let (transport, mut server) = loopback_pair("test");
        let call = {
            let transport = Arc::clone(&transport);
            tokio::spawn(async move {
                use crate::McpTransport as _;
                transport.request(7, "ping", serde_json::json!({})).await
            })
        };
        let envelope =
            server.next_envelope().await.expect("envelope must arrive");
        assert_eq!(envelope["id"], 7);
        assert_eq!(envelope["method"], "ping");
        server.respond(serde_json::json!({
            "jsonrpc": "2.0",
            "id": 7,
            "result": {"pong": true},
        }));
        let result = call.await.expect("join").expect("response");
        assert_eq!(result["pong"], true);
    }

    /// A server error envelope surfaces as `Error::ToolExecution`
    /// with the code and message embedded.
    #[tokio::test]
    async fn surfaces_error_envelopes() {
        let (client, mut server) = loopback_pair("test");
        let handle = {
            let client = Arc::clone(&client);
            tokio::spawn(async move {
                use crate::McpTransport as _;
                client.request(1, "boom", serde_json::json!({})).await
            })
        };
        let _ = server.next_envelope().await;
        server.respond(serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "error": {"code": -32601, "message": "no such method"},
        }));
        let err = handle.await.expect("join").expect_err("must error");
        assert!(
            err.to_string().contains("-32601"),
            "error must carry the code: {err}"
        );
    }
}
