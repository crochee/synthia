//! The [`MessageProxyServiceImpl`] gRPC service impl —
//! the 4 RPC handlers from `MessageProxyService`:
//!
//! - [`MessageProxyService::send`] — point-to-point:
//!   validates `id` + `to`, looks up the recipient in
//!   [`super::state::ProxyState`], returns a
//!   "no active subscriber" error if `tx.send` fails.
//! - [`MessageProxyService::broadcast`] — one-to-many:
//!   skips self-recipient, looks up each recipient,
//!   counts deliveries, returns the last error if no
//!   deliveries succeeded.
//! - [`MessageProxyService::register`] — idempotent:
//!   inserts / replaces the broadcast `Sender` in
//!   `ProxyState`.
//! - [`MessageProxyService::subscribe`] — returns a
//!   `Pin<Box<dyn Stream<...>>>` that yields each
//!   `Message` from the broadcast `Receiver`, plus a
//!   `warn!` on `Lagged(skipped)` and breaks on
//!   `Closed`.

use std::pin::Pin;

use async_stream::stream;
use async_trait::async_trait;
use futures::Stream;
use tokio::sync::broadcast;
use tonic::{Request, Response, Status};
use tracing::{debug, info, warn};
use uuid::Uuid;

use super::{rpc::unix_millis, state::ProxyState};
use crate::{
    BroadcastRequest,
    BroadcastResult,
    Message,
    RegisterRequest,
    RegisterResponse,
    SendResult,
    SubscribeRequest,
    message_proxy::message_proxy_service_server::MessageProxyService,
};

pub(crate) struct MessageProxyServiceImpl {
    pub(crate) state: ProxyState,
}

#[async_trait]
impl MessageProxyService for MessageProxyServiceImpl {
    type SubscribeStream =
        Pin<Box<dyn Stream<Item = Result<Message, Status>> + Send + 'static>>;

    async fn send(
        &self,
        request: Request<Message>,
    ) -> Result<Response<SendResult>, Status> {
        let message = request.into_inner();

        if message.id.is_empty() {
            return Ok(Response::new(SendResult {
                success: false,
                error: "message id is required".to_string(),
            }));
        }
        if message.to.is_empty() {
            return Ok(Response::new(SendResult {
                success: false,
                error: "recipient `to` is required".to_string(),
            }));
        }

        let Some(recipient) = self.state.lookup(&message.to) else {
            debug!(to = %message.to, "Send: recipient not registered");
            return Ok(Response::new(SendResult {
                success: false,
                error: format!("recipient `{}` is not registered", message.to),
            }));
        };

        // `Sender::send` only errors when there are zero active receivers,
        // i.e. the agent registered but is not currently subscribed. The
        // returned `usize` (number of receivers reached) is intentionally
        // ignored — `SendResult` only reports success/failure.
        let recipient_label = message.to.clone();
        match recipient.send(message) {
            Ok(_receivers) => Ok(Response::new(SendResult {
                success: true,
                error: String::new(),
            })),
            Err(_) => Ok(Response::new(SendResult {
                success: false,
                error: format!(
                    "recipient `{}` has no active subscriber",
                    recipient_label
                ),
            })),
        }
    }

    async fn broadcast(
        &self,
        request: Request<BroadcastRequest>,
    ) -> Result<Response<BroadcastResult>, Status> {
        let req = request.into_inner();

        if req.recipients.is_empty() {
            return Ok(Response::new(BroadcastResult {
                success: false,
                delivered_count: 0,
                error: "recipients list is empty".to_string(),
            }));
        }

        let timestamp = unix_millis();
        let from = req.from;
        let payload = req.payload;
        let mut delivered: i32 = 0;
        let mut last_error = String::new();

        for recipient in req.recipients {
            if recipient == from {
                debug!(
                    from = %from,
                    recipient = %recipient,
                    "Broadcast: skipping self"
                );
                continue;
            }
            let Some(tx) = self.state.lookup(&recipient) else {
                last_error =
                    format!("recipient `{}` is not registered", recipient);
                continue;
            };
            let msg = Message {
                id: Uuid::new_v4().to_string(),
                from: from.clone(),
                to: recipient.clone(),
                payload: payload.clone(),
                timestamp,
            };
            if tx.send(msg).is_ok() {
                delivered += 1;
            } else {
                last_error = format!(
                    "recipient `{}` has no active subscriber",
                    recipient
                );
            }
        }

        let success = delivered > 0;
        Ok(Response::new(BroadcastResult {
            success,
            delivered_count: delivered,
            error: if success { String::new() } else { last_error },
        }))
    }

    async fn register(
        &self,
        request: Request<RegisterRequest>,
    ) -> Result<Response<RegisterResponse>, Status> {
        let req = request.into_inner();
        if req.agent_id.is_empty() {
            return Ok(Response::new(RegisterResponse {
                success: false,
                error: "agent_id is required".to_string(),
            }));
        }
        let _ = self.state.register(&req.agent_id);
        info!(agent_id = %req.agent_id, "Agent registered");
        Ok(Response::new(RegisterResponse {
            success: true,
            error: String::new(),
        }))
    }

    async fn subscribe(
        &self,
        request: Request<SubscribeRequest>,
    ) -> Result<Response<Self::SubscribeStream>, Status> {
        let req = request.into_inner();
        if req.agent_id.is_empty() {
            return Err(Status::invalid_argument("agent_id is required"));
        }

        let Some(tx) = self.state.lookup(&req.agent_id) else {
            return Err(Status::failed_precondition(format!(
                "agent `{}` must be registered before subscribing",
                req.agent_id
            )));
        };

        let mut rx = tx.subscribe();
        info!(agent_id = %req.agent_id, "Agent subscribed");

        let output = stream! {
            loop {
                match rx.recv().await {
                    Ok(msg) => yield Ok(msg),
                    Err(broadcast::error::RecvError::Lagged(skipped)) => {
                        warn!(skipped, "subscriber lagged; dropped messages");
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
        };

        Ok(Response::new(Box::pin(output) as Self::SubscribeStream))
    }
}
