//! Shared event broadcaster for agent events.
//!
//! Provides a shared broadcast channel that allows multiple
//! subscribers to receive agent events from a single source.
//!
//! Every send / subscribe / drop on this broadcaster is
//! `tracing`-instrumented so the lifetime of an `AgentEvent`
//! stream can be reconstructed from log output. The logs are
//! intentionally structured (`event.kind`,
//! `subscriber_count`, `session_id`-like fields) so they can be
//! filtered by observability tooling without free-form regex
//! matching.

use std::sync::Arc;

use synthia_agent::AgentEvent;
use tokio::sync::broadcast;

/// Default capacity for the event broadcast channel.
///
/// Sized to absorb a full LLM streaming burst for a typical agent run
/// (≈ 200 model chunks + system events). Slow SSE subscribers cause
/// the broadcaster to drop the oldest unread events (see
/// [`tokio::sync::broadcast`] semantics); the gap is logged at `warn`.
const EVENT_CHANNEL_CAPACITY: usize = 1024;

/// A shared event broadcaster that allows multiple subscribers to receive
/// agent events from a single source.
#[derive(Clone)]
pub struct EventBroadcaster {
    tx: Arc<broadcast::Sender<AgentEvent>>,
    /// Optional logical id attached to the broadcaster so logs from
    /// a controller's `persist_and_broadcast` can be correlated with
    /// the broadcaster's own send logs.
    label: Arc<String>,
}

impl EventBroadcaster {
    /// Creates a new `EventBroadcaster` with a fresh broadcast channel.
    pub fn new() -> Self {
        Self::with_label("default")
    }

    /// Creates a new `EventBroadcaster` tagged with a logical label
    /// (typically the session_id) so log lines from multiple
    /// broadcasters in a single process can be disambiguated.
    pub fn with_label(label: impl Into<String>) -> Self {
        let label = label.into();
        tracing::debug!(
            target: "synthia.event_stream",
            broadcaster = %label,
            capacity = EVENT_CHANNEL_CAPACITY,
            "Creating new EventBroadcaster"
        );
        let (tx, _rx) = broadcast::channel(EVENT_CHANNEL_CAPACITY);
        Self {
            tx: Arc::new(tx),
            label: Arc::new(label),
        }
    }

    /// Returns the broadcaster's logical label (e.g. session_id).
    pub fn label(&self) -> &str {
        &self.label
    }

    /// Subscribes to the event stream, returning a new receiver.
    ///
    /// Each subscription is logged at `debug` with the new total
    /// subscriber count so the open / close pattern of SSE clients
    /// is observable.
    pub fn subscribe(&self) -> broadcast::Receiver<AgentEvent> {
        let rx = self.tx.subscribe();
        let total = self.tx.receiver_count();
        tracing::debug!(
            target: "synthia.event_stream",
            broadcaster = %self.label,
            subscriber_count = total,
            "New subscriber attached to EventBroadcaster"
        );
        rx
    }

    /// Sends an event to all subscribers.
    ///
    /// Emits a structured `tracing::debug!` line carrying the
    /// top-level `event.kind`, an inner `SystemEvent::kind` when
    /// applicable, the payload byte size, and the resulting
    /// subscriber count. Errors are logged at `warn` so a dropped
    /// event is never silent.
    pub fn send(
        &self,
        event: AgentEvent,
    ) -> Result<usize, Box<broadcast::error::SendError<AgentEvent>>> {
        // Use the cheap O(payload) size helper rather than
        // re-serializing here — the controller path already has
        // a serialized copy for the disk write, and a second
        // serialize just for a log line is wasted work on every
        // streaming chunk.
        let byte_size = event.serialized_size();
        let outer_kind = event.kind();
        let inner_kind = match &event {
            AgentEvent::System(sys) => Some(sys.kind()),
            _ => None,
        };
        match self.tx.send(event) {
            Ok(receivers) => {
                tracing::debug!(
                    target: "synthia.event_stream",
                    broadcaster = %self.label,
                    event_kind = outer_kind,
                    system_kind = inner_kind.unwrap_or("-"),
                    payload_bytes = byte_size,
                    receivers,
                    "Broadcast AgentEvent"
                );
                Ok(receivers)
            }
            Err(e) => {
                tracing::warn!(
                    target: "synthia.event_stream",
                    broadcaster = %self.label,
                    event_kind = outer_kind,
                    system_kind = inner_kind.unwrap_or("-"),
                    error = %e,
                    "Failed to broadcast AgentEvent (no subscribers)"
                );
                Err(Box::new(e))
            }
        }
    }

    /// Returns the number of active subscribers.
    ///
    /// Logs at `trace` so volume-sensitive callers (e.g. the
    /// idle-deadline loop) do not flood the log on every poll.
    pub fn subscriber_count(&self) -> usize {
        let n = self.tx.receiver_count();
        tracing::trace!(
            target: "synthia.event_stream",
            broadcaster = %self.label,
            subscriber_count = n,
            "EventBroadcaster subscriber_count queried"
        );
        n
    }
}

impl Default for EventBroadcaster {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use synthia_agent::SystemEvent;

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

        let event =
            AgentEvent::System(synthia_agent::SystemEvent::SessionStarted {
                session_id: "test-1".to_string(),
            });
        let sent = broadcaster.send(event.clone()).unwrap();
        assert_eq!(sent, 1);

        let received = rx.recv().await.unwrap();
        assert!(matches!(
            received,
            AgentEvent::System(
                synthia_agent::SystemEvent::SessionStarted { .. }
            )
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

    #[test]
    fn test_with_label_sets_log_label() {
        let broadcaster = EventBroadcaster::with_label("alice/s1");
        assert_eq!(broadcaster.label(), "alice/s1");
    }

    #[test]
    fn test_kind_label_on_send_path() {
        // Smoke check that AgentEvent::kind() returns the expected
        // string tokens for the two variants we log specifically
        // (Model + System).
        let text = AgentEvent::text_delta("hi");
        assert_eq!(text.kind(), "Model");

        let sys = AgentEvent::System(SystemEvent::SessionEnded {
            reason: synthia_agent::SessionEndReason::Completed,
        });
        assert_eq!(sys.kind(), "System");
        assert!(matches!(
            sys,
            AgentEvent::System(_)
                | AgentEvent::Model(_)
                | AgentEvent::ModelDone(_)
                | AgentEvent::Agent(..)
        ));
    }

    /// Direct test for the broadcast contract that the
    /// `a2a::executor::execute` loop relies on: when the
    /// receiver falls behind and the channel overwrites a
    /// ring-buffer of pending messages, `recv()` returns
    /// `RecvError::Lagged(skipped)`. The executor's fix
    /// requires `Lagged` to be a recoverable signal (the
    /// receiver must `continue`, not break), and
    /// `RecvError::Closed` to be the only terminal
    /// condition.
    #[tokio::test]
    async fn test_recv_error_lagged_reports_skipped_count() {
        let broadcaster = EventBroadcaster::new();
        let mut rx = broadcaster.subscribe();

        // Push past `EVENT_CHANNEL_CAPACITY` events WITHOUT
        // consuming the receiver. The next `recv()` MUST
        // surface a `Lagged` error.
        let event = AgentEvent::System(SystemEvent::SessionStarted {
            session_id: "lag-test".to_string(),
        });
        for _ in 0..(super::EVENT_CHANNEL_CAPACITY + 8) {
            // `send` errors once the channel is full *for
            // new senders*, but a single live receiver plus
            // active producer keeps writing into the
            // ring buffer; the receiver sees them as
            // Lagged.
            let _ = broadcaster.send(event.clone());
        }

        let result = rx.recv().await;
        match result {
            Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                assert!(
                    skipped > 0,
                    "Lagged must report at least one skipped event, got {skipped}"
                );
            }
            other => panic!(
                "expected Err(Lagged(_)) when receiver falls behind, got {other:?}"
            ),
        }

        // After Lagged, the receiver must still receive
        // subsequent messages (i.e. it's recoverable).
        // This is the contract the executor relies on.
        let next = rx.recv().await;
        assert!(
            next.is_ok(),
            "receiver must remain connected after Lagged; got {next:?}"
        );
    }

    /// Late subscribers (those that join AFTER events have
    /// already been broadcast) MUST NOT see the historical
    /// events on the wire — the broadcast channel is a
    /// live fan-out, not a queue of pending messages.
    /// `tokio::sync::broadcast` documents this explicitly:
    /// "Receivers only see messages sent after they
    /// subscribed."
    ///
    /// This contract is what allows `wrapper::subscribe_to_task`
    /// to safely short-circuit to the TaskStore snapshot
    /// when a brand-new SSE client attaches after the
    /// executor has cleared the active execution: the
    /// client MUST NOT see half-streamed in-flight events
    /// because the broadcast channel no longer retains
    /// them. If you ever need replay, build it from the
    /// durable session event store, not the broadcast
    /// channel.
    #[tokio::test]
    async fn test_late_subscriber_does_not_see_past_events() {
        let broadcaster = EventBroadcaster::new();
        let event = AgentEvent::System(SystemEvent::SessionStarted {
            session_id: "historic".to_string(),
        });

        // Send 5 events BEFORE any subscriber attaches.
        // `tokio::sync::broadcast::Sender::send` returns
        // `Err(SendError(value))` when there are no
        // receivers — which is exactly our setup. The
        // broadcaster layer wraps and re-emits that as
        // `Box<SendError<AgentEvent>>`, so we discard
        // it. The important point is: the channel does
        // not buffer undelivered events for future
        // subscribers.
        for _ in 0..5 {
            let _ = broadcaster.send(event.clone());
        }

        // Late subscriber attaches AFTER the events were
        // broadcast. recv() MUST NOT see any of those
        // historic events.
        let mut rx = broadcaster.subscribe();

        // Race the recv against a short sleep so the test
        // does not hang if the channel does happen to
        // surface a phantom event.
        let outcome = tokio::time::timeout(
            std::time::Duration::from_millis(50),
            rx.recv(),
        )
        .await;
        match outcome {
            // The only acceptable outcome: timeout fires
            // because no event was sent after subscribe().
            Err(_elapsed) => {}
            Ok(Err(tokio::sync::broadcast::error::RecvError::Lagged(_))) => {
                // Also acceptable: if the late subscriber
                // was attached while the ring buffer was
                // already full, the next recv can return
                // Lagged(0) and that's a recoverable
                // signal too. We treat it as "did not
                // receive a historical event".
            }
            Ok(Ok(event)) => panic!(
                "late subscriber must NOT see events sent before subscribe(); got {event:?}"
            ),
            Ok(Err(other)) => {
                panic!("unexpected recv error for late subscriber: {other:?}")
            }
        }
    }
}
