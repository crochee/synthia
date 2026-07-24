//! Event stream abstraction shared by SSE and WebSocket implementations.
//!
//! Provides a shared event broadcaster that allows multiple subscribers to receive
//! agent events from a single source, enabling both SSE and WebSocket handlers to
//! consume the same event stream.

use std::sync::Arc;

use synthia_agent::types::AgentEvent;
use tokio::sync::broadcast;

/// Default capacity for the event broadcast channel.
const EVENT_CHANNEL_CAPACITY: usize = 128;

/// A shared event broadcaster that allows multiple subscribers to receive
/// agent events from a single source. Both SSE and WebSocket handlers
/// subscribe to the same broadcast channel source.
#[derive(Clone)]
pub struct EventBroadcaster {
    tx: Arc<broadcast::Sender<AgentEvent>>,
}

impl EventBroadcaster {
    /// Creates a new `EventBroadcaster` with a fresh broadcast channel.
    pub fn new() -> Self {
        let (tx, _rx) = broadcast::channel(EVENT_CHANNEL_CAPACITY);
        Self { tx: Arc::new(tx) }
    }

    /// Subscribes to the event stream, returning a new receiver.
    pub fn subscribe(&self) -> broadcast::Receiver<AgentEvent> {
        self.tx.subscribe()
    }

    /// Sends an event to all subscribers.
    pub fn send(
        &self,
        event: AgentEvent,
    ) -> Result<usize, broadcast::error::SendError<AgentEvent>> {
        self.tx.send(event)
    }

    /// Returns the number of active subscribers.
    pub fn subscriber_count(&self) -> usize {
        self.tx.receiver_count()
    }
}

impl Default for EventBroadcaster {
    fn default() -> Self {
        Self::new()
    }
}

/// SSE event stream implementation.
///
/// The `EventStream` trait that previously abstracted over SSE/WebSocket
/// transports was REMOVED on 2026-06-15 in change
/// `2026-06-15-p2-trait-cleanup` because it had 0 trait-bound usage,
/// 0 dyn dispatch, and exactly 1 real implementation
/// (`SseEventStream`). WebSocket support was not implemented.
pub struct SseEventStream;

impl SseEventStream {
    /// Converts a broadcast receiver into an SSE response.
    pub fn to_response(
        rx: broadcast::Receiver<AgentEvent>,
    ) -> impl axum::response::IntoResponse + Send {
        crate::sse::sse_event_stream(rx)
    }
}

#[cfg(test)]
mod tests {
    use synthia_agent::events::SystemEvent;

    use super::*;

    #[test]
    fn test_broadcaster_creation() {
        let broadcaster = EventBroadcaster::new();
        assert_eq!(broadcaster.subscriber_count(), 0);
    }

    #[tokio::test]
    async fn test_subscribe_and_send() {
        let broadcaster = EventBroadcaster::new();
        let mut rx = broadcaster.subscribe();

        let event = AgentEvent::System(SystemEvent::SessionStarted {
            session_id: "test-1".to_string(),
        });
        let sent = broadcaster.send(event.clone()).unwrap();
        assert_eq!(sent, 1);

        let received = rx.recv().await.unwrap();
        assert!(matches!(
            received,
            AgentEvent::System(SystemEvent::SessionStarted { .. })
        ));
    }

    #[test]
    fn test_multiple_subscribers() {
        let broadcaster = EventBroadcaster::new();
        let _rx1 = broadcaster.subscribe();
        assert_eq!(broadcaster.subscriber_count(), 1);

        let _rx2 = broadcaster.subscribe();
        assert_eq!(broadcaster.subscriber_count(), 2);
    }

    #[test]
    fn test_clone_shares_channel() {
        let broadcaster = EventBroadcaster::new();
        let cloned = broadcaster.clone();

        let _rx = broadcaster.subscribe();
        assert_eq!(cloned.subscriber_count(), 1);
    }
}
