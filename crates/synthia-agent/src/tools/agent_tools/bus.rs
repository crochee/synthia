//! Message bus infrastructure for inter-agent communication.
//!
//! Provides:
//! - [`AgentMessage`]: the wire-level message type exchanged between agents
//! - [`MessageBus`]: trait abstracting the transport
//! - [`InMemoryMessageBus`]: a `DashMap`-backed in-process implementation
//!
//! The bus is intentionally transport-agnostic so a remote (network) bus
//! can be swapped in without changing call-sites.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::sync::mpsc;
use uuid::Uuid;

/// Error type for send operations
#[derive(Debug, Error)]
pub enum SendError {
    #[error("Agent '{0}' not found")]
    AgentNotFound(String),
    #[error("Agent '{0}' has no inbox channel")]
    NoInboxChannel(String),
    #[error("Send failed: {0}")]
    SendFailed(String),
}

/// Error type for receive operations
#[derive(Debug, Error)]
pub enum ReceiveError {
    #[error("Agent '{0}' not found")]
    AgentNotFound(String),
    #[error("Receive failed: {0}")]
    ReceiveFailed(String),
}

/// Represents a message between agents
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentMessage {
    pub id: String,
    pub from: String,
    pub to: String,
    pub content: String,
    pub timestamp: DateTime<Utc>,
}

impl AgentMessage {
    pub fn new(from: String, to: String, content: String) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            from,
            to,
            content,
            timestamp: Utc::now(),
        }
    }
}

/// Trait for message bus implementations
#[async_trait]
pub trait MessageBus: Send + Sync {
    fn register_agent(&self, agent_id: &str) -> Result<(), SendError>;
    fn unregister_agent(&self, agent_id: &str);
    async fn send(&self, message: AgentMessage) -> Result<(), SendError>;
    async fn receive(
        &self,
        agent_id: &str,
    ) -> Result<Option<AgentMessage>, ReceiveError>;
}

/// In-memory message bus implementation
pub struct InMemoryMessageBus {
    /// Maps agent ID to sender channels for other agents to send to
    outboxes: DashMap<String, mpsc::Sender<AgentMessage>>,
    /// Maps agent ID to receiver channels for this agent to receive from
    inboxes: DashMap<String, mpsc::Receiver<AgentMessage>>,
}

impl InMemoryMessageBus {
    pub fn new() -> Self {
        Self {
            outboxes: DashMap::new(),
            inboxes: DashMap::new(),
        }
    }
}

impl Default for InMemoryMessageBus {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl MessageBus for InMemoryMessageBus {
    fn register_agent(&self, agent_id: &str) -> Result<(), SendError> {
        // Check if agent is already registered
        if self.outboxes.contains_key(agent_id) {
            return Ok(());
        }

        let (tx, rx) = mpsc::channel(100);
        self.outboxes.insert(agent_id.to_string(), tx);
        self.inboxes.insert(agent_id.to_string(), rx);
        Ok(())
    }

    fn unregister_agent(&self, agent_id: &str) {
        self.outboxes.remove(agent_id);
        self.inboxes.remove(agent_id);
    }

    async fn send(&self, message: AgentMessage) -> Result<(), SendError> {
        let agent_id = &message.to;

        let tx = self
            .outboxes
            .get(agent_id)
            .ok_or_else(|| SendError::AgentNotFound(agent_id.clone()))?;

        tx.send(message)
            .await
            .map_err(|e| SendError::SendFailed(e.to_string()))
    }

    async fn receive(
        &self,
        agent_id: &str,
    ) -> Result<Option<AgentMessage>, ReceiveError> {
        let mut rx = self
            .inboxes
            .get_mut(agent_id)
            .ok_or_else(|| ReceiveError::AgentNotFound(agent_id.to_string()))?;

        Ok(rx.recv().await)
    }
}
