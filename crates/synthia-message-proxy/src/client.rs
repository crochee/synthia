//! gRPC client for the multi-agent message proxy.
//!
//! [`MessageBusProxy`] speaks the same `MessageProxyService` protocol as the
//! server, but from the agent side. It connects over a Unix Domain Socket
//! and exposes the proxy's RPCs as idiomatic async methods.
//!
//! Connection management is delegated to tonic's `Channel`: once a
//! `MessageBusProxy` is constructed, every RPC transparently reconnects on
//! transport failure — callers do not need to implement retry logic.
//!
//! # Default socket path
//!
//! Reads `MESSAGE_PROXY_ADDR` from the environment, falling back to
//! `/var/run/synthia/message-proxy.sock` (matching the server's default).

use std::{
    env,
    future::Future,
    pin::Pin,
    task::{Context, Poll},
    time::{SystemTime, UNIX_EPOCH},
};

use async_trait::async_trait;
use futures::Stream;
use hyper_util::rt::TokioIo;
use thiserror::Error;
use tokio::sync::Mutex;
use tonic::transport::{Channel, Endpoint, Uri};
use tower::Service;
use uuid::Uuid;

use crate::{
    BroadcastRequest,
    BroadcastResult,
    Message,
    RegisterRequest,
    RegisterResponse,
    SendResult,
    SubscribeRequest,
    message_proxy::message_proxy_service_client::MessageProxyServiceClient,
};

/// Default UDS path used when `MESSAGE_PROXY_ADDR` is unset.
pub const DEFAULT_PROXY_ADDR: &str = "/var/run/synthia/message-proxy.sock";

/// Errors that can surface from any `MessageBus` operation. Wraps both
/// transport-level failures (broken channel) and application-level failures
/// reported by the server in its result messages.
#[derive(Debug, Error)]
pub enum ProxyError {
    #[error("transport error: {0}")]
    Transport(#[from] tonic::transport::Error),
    #[error("gRPC error: {0}")]
    Status(#[from] tonic::Status),
    #[error("server returned failure: {0}")]
    Server(String),
}

/// Async contract for talking to a `MessageProxy` over gRPC. Implemented by
/// [`MessageBusProxy`]; test doubles can implement it directly.
#[async_trait]
pub trait MessageBus: Send + Sync {
    /// Register an agent with the proxy. Idempotent on the server side:
    /// re-registering an existing agent replaces its prior subscriber sender.
    async fn register(&self, agent_id: &str) -> Result<(), ProxyError>;

    /// Send a unicast message from `from` to `to`.
    async fn send(
        &self,
        from: &str,
        to: &str,
        payload: Vec<u8>,
    ) -> Result<(), ProxyError>;

    /// Send a single payload from `from` to every entry in `recipients`.
    /// Returns the number of recipients the server confirmed as delivered.
    async fn broadcast(
        &self,
        from: &str,
        recipients: Vec<String>,
        payload: Vec<u8>,
    ) -> Result<u32, ProxyError>;

    /// Subscribe to inbound messages for `agent_id`. The returned stream
    /// terminates if the server closes the underlying RPC.
    async fn subscribe(
        &self,
        agent_id: &str,
    ) -> Result<
        Box<dyn Stream<Item = Result<Message, tonic::Status>> + Send + Unpin>,
        ProxyError,
    >;
}

/// gRPC client for the `MessageProxy` service.
///
/// The inner `Channel` reconnects transparently on transport failure, so a
/// stale connection (e.g. server restart) is recovered on the next RPC
/// without any explicit reconnection logic in the caller.
pub struct MessageBusProxy {
    client: Mutex<MessageProxyServiceClient<Channel>>,
    agent_id: String,
}

impl MessageBusProxy {
    /// Connect to the proxy at `MESSAGE_PROXY_ADDR` (or the default UDS
    /// path). The agent's identifier is stored on the struct and used as the
    /// default `from` field for outgoing messages.
    pub async fn connect(agent_id: String) -> Result<Self, ProxyError> {
        let addr = env::var("MESSAGE_PROXY_ADDR")
            .unwrap_or_else(|_| DEFAULT_PROXY_ADDR.to_string());
        Self::connect_to(agent_id, addr).await
    }

    /// Connect to a specific UDS path. The path is converted to a
    /// `unix://localhost` URL and a `Channel` is built in lazy mode: the
    /// actual handshake happens on the first RPC, so this constructor does
    /// not fail if the server is not yet running.
    ///
    /// The `localhost` authority is required because the `unix` scheme is a
    /// non-special URI scheme and cannot have an empty authority. The
    /// authority is ignored by the UDS connector — only the path is used.
    pub async fn connect_to(
        agent_id: String,
        addr: String,
    ) -> Result<Self, ProxyError> {
        let url = format!("unix://localhost{addr}");
        let endpoint = Endpoint::from_shared(url)?;
        let connector = UdsConnector::new(addr);
        let channel = endpoint.connect_with_connector_lazy(connector);
        let client = MessageProxyServiceClient::new(channel);
        Ok(Self {
            client: Mutex::new(client),
            agent_id,
        })
    }

    /// Convenience constructor for tests that need a pre-connected channel
    /// (e.g. against an in-process test server). `connect_lazy` is used by
    /// default; the channel is shared and reconnect-safe.
    pub fn from_channel(agent_id: String, channel: Channel) -> Self {
        let client = MessageProxyServiceClient::new(channel);
        Self {
            client: Mutex::new(client),
            agent_id,
        }
    }

    /// The agent identifier this client was constructed with.
    pub fn agent_id(&self) -> &str {
        &self.agent_id
    }
}

#[async_trait]
impl MessageBus for MessageBusProxy {
    async fn register(&self, agent_id: &str) -> Result<(), ProxyError> {
        let req = RegisterRequest {
            agent_id: agent_id.to_string(),
        };
        let mut client = self.client.lock().await;
        let resp: RegisterResponse = client.register(req).await?.into_inner();
        if resp.success {
            Ok(())
        } else {
            Err(ProxyError::Server(resp.error))
        }
    }

    async fn send(
        &self,
        from: &str,
        to: &str,
        payload: Vec<u8>,
    ) -> Result<(), ProxyError> {
        let msg = Message {
            id: Uuid::new_v4().to_string(),
            from: from.to_string(),
            to: to.to_string(),
            payload,
            timestamp: unix_millis(),
        };
        let mut client = self.client.lock().await;
        let resp: SendResult = client.send(msg).await?.into_inner();
        if resp.success {
            Ok(())
        } else {
            Err(ProxyError::Server(resp.error))
        }
    }

    async fn broadcast(
        &self,
        from: &str,
        recipients: Vec<String>,
        payload: Vec<u8>,
    ) -> Result<u32, ProxyError> {
        let req = BroadcastRequest {
            from: from.to_string(),
            recipients,
            payload,
        };
        let mut client = self.client.lock().await;
        let resp: BroadcastResult = client.broadcast(req).await?.into_inner();
        if resp.success {
            Ok(resp.delivered_count as u32)
        } else {
            Err(ProxyError::Server(resp.error))
        }
    }

    async fn subscribe(
        &self,
        agent_id: &str,
    ) -> Result<
        Box<dyn Stream<Item = Result<Message, tonic::Status>> + Send + Unpin>,
        ProxyError,
    > {
        let req = SubscribeRequest {
            agent_id: agent_id.to_string(),
        };
        let mut client = self.client.lock().await;
        let stream = client.subscribe(req).await?.into_inner();
        Ok(Box::new(stream))
    }
}

fn unix_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Tower [`Service`] that dials a fixed Unix Domain Socket path on every
/// call. Used in place of tonic's default TCP `HttpConnector` because the
/// `MessageProxy` speaks gRPC over UDS only.
///
/// The response is wrapped in `hyper_util::rt::TokioIo` so it satisfies
/// hyper's `rt::Read + rt::Write + Unpin` bounds (raw `tokio::net::UnixStream`
/// only implements the futures-codec traits).
#[derive(Clone)]
struct UdsConnector {
    path: String,
}

impl UdsConnector {
    fn new(path: String) -> Self {
        Self { path }
    }
}

type UdsFuture = Pin<
    Box<
        dyn Future<
                Output = Result<
                    TokioIo<tokio::net::UnixStream>,
                    std::io::Error,
                >,
            > + Send,
    >,
>;

impl Service<Uri> for UdsConnector {
    type Error = std::io::Error;
    type Future = UdsFuture;
    type Response = TokioIo<tokio::net::UnixStream>;

    fn poll_ready(
        &mut self,
        _cx: &mut Context<'_>,
    ) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, _uri: Uri) -> Self::Future {
        let path = self.path.clone();
        Box::pin(async move {
            let stream = tokio::net::UnixStream::connect(&path).await?;
            Ok(TokioIo::new(stream))
        })
    }
}
