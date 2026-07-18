//! EventBus trait — unified event publishing/subscription.
//!
//! This is a skeleton for Phase 5 (EventBus unification).
//! Full implementation deferred to a follow-up change.

use async_trait::async_trait;

/// Event bus trait for publishing and subscribing to agent events.
///
/// Will replace 3 parallel channels:
/// - `agent/events/emitter.rs` (`mpsc::UnboundedSender`)
/// - `server/event_stream.rs` (`broadcast::Sender(128)`)
/// - `orchestrator/lib.rs` (`broadcast::Sender(256)`)
#[async_trait]
pub trait EventBus: Send + Sync {
    /// Publish an event. Ephemeral events broadcast directly;
    /// durable events go through the persistent actor.
    async fn publish(&self, event: AgentEvent) -> Result<(), EventBusError>;

    /// Subscribe to events matching a filter.
    fn subscribe(&self) -> tokio_stream::wrappers::BroadcastStream<AgentEvent>;
}

/// Agent event placeholder.
/// Full definition deferred to Phase 5.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AgentEvent {
    pub kind: String,
    pub payload: serde_json::Value,
}

/// Event bus error.
#[derive(Debug, thiserror::Error)]
pub enum EventBusError {
    #[error("publish failed: {0}")]
    PublishFailed(String),
    #[error("subscriber lagged")]
    Lagged,
}
