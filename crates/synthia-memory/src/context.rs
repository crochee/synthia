use std::{collections::HashMap, sync::Arc};

use tokio::sync::RwLock;

use crate::{compaction::CompactionEngine, types::CompactionReport};

/// A session message for context memory.
#[derive(Debug, Clone)]
pub struct ContextMessage {
    pub role: String,
    pub content: String,
}

pub struct ContextMemory {
    sessions: Arc<RwLock<HashMap<String, Vec<ContextMessage>>>>,
    compaction_engine: CompactionEngine,
}

impl ContextMemory {
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
            compaction_engine: CompactionEngine::new(),
        }
    }

    /// Store messages for a session.
    pub async fn set_context(
        &self,
        session_id: &str,
        messages: Vec<ContextMessage>,
    ) -> Result<(), synthia_core::Error> {
        let mut sessions = self.sessions.write().await;
        sessions.insert(session_id.to_string(), messages);
        Ok(())
    }

    /// Get all messages for a session.
    pub async fn get_context(
        &self,
        session_id: &str,
    ) -> Result<Vec<ContextMessage>, synthia_core::Error> {
        let sessions = self.sessions.read().await;
        Ok(sessions.get(session_id).cloned().unwrap_or_default())
    }

    /// Compact the context for a session and return a CompactionReport.
    pub async fn compact_context(
        &self,
        session_id: &str,
    ) -> Result<CompactionReport, synthia_core::Error> {
        let mut sessions = self.sessions.write().await;

        let messages = sessions.remove(session_id).unwrap_or_default();

        let entries: Vec<String> = messages
            .iter()
            .map(|m| format!("{}: {}", m.role, m.content))
            .collect();

        let report = self.compaction_engine.compact_full(&entries);

        // Store compacted result back
        if !entries.is_empty() && report.stage > 0 {
            let compacted_messages = entries
                .into_iter()
                .map(|e| {
                    if let Some(colon_pos) = e.find(": ") {
                        let role = e[..colon_pos].to_string();
                        let content = e[colon_pos + 2..].to_string();
                        ContextMessage { role, content }
                    } else {
                        ContextMessage {
                            role: "unknown".to_string(),
                            content: e,
                        }
                    }
                })
                .collect();
            sessions.insert(session_id.to_string(), compacted_messages);
        }

        Ok(report)
    }

    /// Get the number of active sessions.
    pub async fn session_count(&self) -> usize {
        self.sessions.read().await.len()
    }

    /// Clear a specific session.
    pub async fn clear_session(&self, session_id: &str) {
        self.sessions.write().await.remove(session_id);
    }
}

impl Default for ContextMemory {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_messages(count: usize) -> Vec<ContextMessage> {
        (0..count)
            .map(|i| ContextMessage {
                role: if i % 2 == 0 { "user" } else { "assistant" }.to_string(),
                content: format!("Message content {}", i),
            })
            .collect()
    }

    #[tokio::test]
    async fn test_set_and_get_context() {
        let mem = ContextMemory::new();
        let msgs = make_messages(3);
        mem.set_context("sess-1", msgs.clone()).await.unwrap();

        let retrieved = mem.get_context("sess-1").await.unwrap();
        assert_eq!(retrieved.len(), 3);
        assert_eq!(retrieved[0].role, "user");
    }

    #[tokio::test]
    async fn test_get_nonexistent_session() {
        let mem = ContextMemory::new();
        let retrieved = mem.get_context("nonexistent").await.unwrap();
        assert!(retrieved.is_empty());
    }

    #[tokio::test]
    async fn test_compact_context() {
        let mem = ContextMemory::new();
        let msgs = make_messages(20);
        mem.set_context("sess-1", msgs).await.unwrap();

        let report = mem.compact_context("sess-1").await.unwrap();
        assert!(report.tokens_before > 0);
        assert!(report.stage >= 1);
        // After compaction, should have fewer tokens
        assert!(report.tokens_after < report.tokens_before);
    }

    #[tokio::test]
    async fn test_compact_empty_session() {
        let mem = ContextMemory::new();
        let report = mem.compact_context("empty").await.unwrap();
        assert_eq!(report.tokens_before, 0);
        assert_eq!(report.stage, 0);
    }

    #[tokio::test]
    async fn test_session_count() {
        let mem = ContextMemory::new();
        assert_eq!(mem.session_count().await, 0);

        mem.set_context("s1", make_messages(1)).await.unwrap();
        mem.set_context("s2", make_messages(1)).await.unwrap();
        assert_eq!(mem.session_count().await, 2);
    }

    #[tokio::test]
    async fn test_clear_session() {
        let mem = ContextMemory::new();
        mem.set_context("sess-1", make_messages(3)).await.unwrap();
        assert_eq!(mem.session_count().await, 1);

        mem.clear_session("sess-1").await;
        assert_eq!(mem.session_count().await, 0);
    }

    #[tokio::test]
    async fn test_compact_and_verify_stored() {
        let mem = ContextMemory::new();
        mem.set_context("sess-1", make_messages(20)).await.unwrap();

        let report = mem.compact_context("sess-1").await.unwrap();
        assert!(report.stage > 0);

        // Context should still exist after compaction
        let msgs = mem.get_context("sess-1").await.unwrap();
        assert!(!msgs.is_empty());
    }
}
