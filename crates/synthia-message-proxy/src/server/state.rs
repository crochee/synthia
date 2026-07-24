//! The [`ProxyState`] struct + 2 methods (`register` /
//! `lookup`), plus the `AGENT_CHANNEL_CAPACITY` constant
//! and the `AgentSender` type alias.
//!
//! `ProxyState` is the shared, in-memory routing table
//! of agent IDs → broadcast channels. It's a
//! [`DashMap<String, broadcast::Sender<Message>>`] wrapped
//! in `Arc`, so it's cheap to clone.

use std::sync::Arc;

use dashmap::DashMap;
use tokio::sync::broadcast;

use crate::Message;

/// Capacity of the per-agent broadcast channel. Sized
/// to absorb short bursts without back-pressuring senders;
/// on overflow, slow subscribers observe `Lagged` and miss
/// the dropped messages.
pub(crate) const AGENT_CHANNEL_CAPACITY: usize = 256;

pub(crate) type AgentSender = broadcast::Sender<Message>;

/// Shared, in-memory state of the proxy. Cheap to clone
/// via the inner `Arc`.
#[derive(Clone, Default)]
pub(crate) struct ProxyState {
    pub(crate) agents: Arc<DashMap<String, AgentSender>>,
}

impl ProxyState {
    /// Idempotent: re-registering an agent replaces the
    /// prior sender. Any subscriber to the previous sender
    /// will simply stop receiving new messages — its
    /// `Receiver` returns `Closed` once the old `Sender`
    /// is fully dropped.
    pub(crate) fn register(&self, agent_id: &str) -> AgentSender {
        let (tx, _) = broadcast::channel(AGENT_CHANNEL_CAPACITY);
        self.agents.insert(agent_id.to_owned(), tx.clone());
        tx
    }

    pub(crate) fn lookup(&self, agent_id: &str) -> Option<AgentSender> {
        self.agents.get(agent_id).map(|entry| entry.value().clone())
    }
}
