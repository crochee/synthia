//! What a run's events owe the outside world: durable append, the
//! broadcast fan-out, and the classification each drives on.
//! Method continuation of the inner state's `ControllerInner`.

use std::sync::atomic::Ordering;

use anyhow::{Context as _, Result};
use synthia::{
    harness::{AgentEvent, SystemEvent},
    session::SessionError,
};

use super::{inner::ControllerInner, run_log::RunLog};
use crate::state::UsageMetrics;

impl ControllerInner {
    pub(super) async fn persist_and_broadcast(
        &self,
        event: &AgentEvent,
        log: &mut RunLog,
    ) -> Result<()> {
        // A wrapped event is a delegated sub-agent's trace. It never
        // joins the parent's durable log or SSE: the router gives
        // the child its own session (its own log, its own live
        // stream), and the parent keeps only the lightweight
        // enter/exit markers the router writes through `log`.
        if let AgentEvent::Agent(meta, inner) = event {
            Self::record_usage_tree(&self.usage, inner);
            if let Some(router) = self.subagent_router.as_ref() {
                router
                    .route(&self.user_id, &self.session_id, meta, inner, log)
                    .await;
            }
            return Ok(());
        }
        let outer_kind = event.kind();
        let system_kind = Self::classify_system_kind(event);
        let (payload_bytes, event_value) = Self::encode_event(event)?;
        let byte_size = payload_bytes.len();
        let event_type = event.kind();

        tracing::debug!(
            target: "synthia.session",
            session_id = %self.session_id,
            event_kind = outer_kind,
            system_kind = system_kind.unwrap_or("-"),
            event_type,
            payload_bytes = byte_size,
            subscribers = self.broadcaster.subscriber_count(),
            "persist_and_broadcast: entering"
        );

        Self::record_usage(&self.usage, event);
        // The sink's append returns the per-row stream_index
        // — used by `routes/chat.rs::resume_stream_to_sse` for
        // the cursor / snapshot frames. `EventBroadcaster`
        // continues to carry the bare `AgentEvent`; live tail
        // frames do not embed the stream_index, the wire
        // cursor + `GET /api/v1/sessions/{id}` are the source
        // of truth for resume. See
        // `2026-09-24-stream-index.md` §4.3 / §10.
        let _stream_index =
            Self::append_if_durable(event, &event_value, log).await?;
        self.broadcast_event(event, outer_kind, system_kind);
        Ok(())
    }

    /// [`Self::record_usage`] over a whole sub-agent trace: the
    /// child's usage and turn totals are real consumption of the
    /// deployment's tokens, so they belong in `/api/v1/chat/usage`
    /// even though the child's events now land in their own session.
    fn record_usage_tree(usage: &UsageMetrics, event: &AgentEvent) {
        Self::record_usage(usage, event);
        if let AgentEvent::Agent(_, inner) = event {
            Self::record_usage_tree(usage, inner);
        }
    }

    /// Fold one event into the process-wide usage counters — the
    /// **only** write site for `/api/v1/chat/usage`.
    ///
    /// `Relaxed` is deliberate: the endpoint reports a monotone
    /// running total, so there is no other memory to order these
    /// against and a lost update race would still be a bounded,
    /// self-correcting skew on a counter nobody reads causally.
    fn record_usage(usage: &UsageMetrics, event: &AgentEvent) {
        match event {
            AgentEvent::System(SystemEvent::Usage {
                input_tokens,
                output_tokens,
                ..
            }) => {
                usage
                    .tokens_in
                    .fetch_add(*input_tokens as u64, Ordering::Relaxed);
                usage
                    .tokens_out
                    .fetch_add(*output_tokens as u64, Ordering::Relaxed);
            }
            // Terminal event of every run, whatever the reason: a
            // cancelled or errored run consumed a turn too.
            AgentEvent::System(SystemEvent::SessionEnded { .. }) => {
                usage.turns.fetch_add(1, Ordering::Relaxed);
            }
            _ => {}
        }
    }

    /// Single serialization pass — `to_vec` keeps ownership
    /// of the bytes for the size measurement AND the decode
    /// recovers the `Value` the sink append needs (the
    /// previous shape ran `to_value(...).to_string().len()`
    /// (two passes) and re-serialized inside
    /// `EventBroadcaster::send` (three passes total)).
    pub(super) fn encode_event(
        event: &AgentEvent,
    ) -> Result<(Vec<u8>, serde_json::Value)> {
        let payload_bytes = serde_json::to_vec(event)
            .context("failed to serialize agent event")?;
        let event_value: serde_json::Value =
            serde_json::from_slice(&payload_bytes)
                .context("failed to decode event payload for sink append")?;
        Ok((payload_bytes, event_value))
    }

    /// Resolved kind for the structural log line, if `event`
    /// is a `System` event; `None` otherwise.
    pub(super) fn classify_system_kind(
        event: &AgentEvent,
    ) -> Option<&'static str> {
        match event {
            AgentEvent::System(sys) => Some(sys.kind()),
            _ => None,
        }
    }

    /// Append `event_value` to the run log when (and only
    /// when) the event is durable. Ephemeral events (token
    /// deltas, warnings, reasoning chunks) are streamed live
    /// but not persisted.
    ///
    /// Returns the sink's `stream_index` for the appended row
    /// — the per-session resume cursor the wire exposes in
    /// `cursor` and `snapshot` frames. `None` for non-durable
    /// events (no row was written).
    pub(super) async fn append_if_durable(
        event: &AgentEvent,
        event_value: &serde_json::Value,
        log: &mut RunLog,
    ) -> Result<Option<u64>> {
        if event.is_durable() {
            let idx =
                log.append(event_value).await.map_err(|e: SessionError| {
                    anyhow::anyhow!("failed to append event to sink: {e}")
                })?;
            Ok(Some(idx))
        } else {
            Ok(None)
        }
    }

    /// Fan the event out to the live SSE/WebSocket
    /// subscribers. A send with no subscribers is a debug
    /// log, not an error — the durable append already
    /// happened.
    pub(super) fn broadcast_event(
        &self,
        event: &AgentEvent,
        outer_kind: &'static str,
        system_kind: Option<&'static str>,
    ) {
        tracing::debug!(
            target: "synthia.session",
            session_id = %self.session_id,
            event_kind = outer_kind,
            system_kind = system_kind.unwrap_or("-"),
            "persist_and_broadcast: appended to event store; broadcasting"
        );
        if let Err(e) = self.broadcaster.send(event.clone()) {
            tracing::debug!(
                target: "synthia.session",
                session_id = %self.session_id,
                event_kind = outer_kind,
                system_kind = system_kind.unwrap_or("-"),
                error = %e,
                "No subscribers to broadcast event to"
            );
        }
    }
}
