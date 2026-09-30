//! In-memory implementation of the [`Memory`] trait.
//!
//! Ported from `traitclaw-core`'s `InMemoryMemory`. Zero
//! external persistence; suitable for tests, examples, and as
use std::collections::HashMap;

use async_trait::async_trait;
use parking_lot::RwLock;
use serde_json::Value;
use synthia_provider::Message;

use super::{Memory, MemoryEntry, MemoryError};

/// In-memory implementation of the [`Memory`] trait.
///
/// All data is lost when the process exits. Conversation and
/// working memory are keyed by session ID; long-term entries are
/// global.
#[derive(Debug, Default)]
pub struct InMemoryMemory {
    /// Conversation messages keyed by session ID.
    messages: RwLock<HashMap<String, Vec<Message>>>,
    /// Working memory: `session_id` → (`key` → value).
    context: RwLock<HashMap<String, HashMap<String, Value>>>,
    /// Long-term memory entries (global).
    long_term: RwLock<Vec<MemoryEntry>>,
}

impl InMemoryMemory {
    /// Create a new empty in-memory store.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl Memory for InMemoryMemory {
    async fn messages(
        &self,
        session_id: &str,
    ) -> Result<Vec<Message>, MemoryError> {
        let store = self.messages.read();
        Ok(store.get(session_id).cloned().unwrap_or_default())
    }

    async fn append(
        &self,
        session_id: &str,
        message: Message,
    ) -> Result<(), MemoryError> {
        let mut store = self.messages.write();
        store
            .entry(session_id.to_string())
            .or_default()
            .push(message);
        Ok(())
    }

    async fn get_context(
        &self,
        session_id: &str,
        key: &str,
    ) -> Result<Option<Value>, MemoryError> {
        let store = self.context.read();
        Ok(store.get(session_id).and_then(|ctx| ctx.get(key)).cloned())
    }

    async fn set_context(
        &self,
        session_id: &str,
        key: &str,
        value: Value,
    ) -> Result<(), MemoryError> {
        let mut store = self.context.write();
        store
            .entry(session_id.to_string())
            .or_default()
            .insert(key.to_string(), value);
        Ok(())
    }

    async fn recall(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<MemoryEntry>, MemoryError> {
        // Substring matching, mirroring the reference design: an
        // empty query matches everything; results truncate at
        // `limit`.
        let store = self.long_term.read();
        let results = store
            .iter()
            .filter(|entry| entry.content.contains(query))
            .take(limit)
            .cloned()
            .collect();
        Ok(results)
    }

    async fn store(&self, entry: MemoryEntry) -> Result<(), MemoryError> {
        let mut store = self.long_term.write();
        store.push(entry);
        Ok(())
    }

    async fn create_session(&self) -> Result<String, MemoryError> {
        let id = ulid::Ulid::generate().to_string();
        // Pre-create the bucket so list_sessions sees it
        // immediately.
        let mut store = self.messages.write();
        store.entry(id.clone()).or_default();
        Ok(id)
    }

    async fn list_sessions(&self) -> Result<Vec<String>, MemoryError> {
        let store = self.messages.read();
        Ok(store.keys().cloned().collect())
    }

    async fn delete_session(
        &self,
        session_id: &str,
    ) -> Result<(), MemoryError> {
        self.messages.write().remove(session_id);
        self.context.write().remove(session_id);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn user_msg(text: &str) -> Message {
        Message::user(text)
    }

    #[tokio::test]
    async fn conversation_append_then_messages_round_trips() {
        let mem = InMemoryMemory::new();
        mem.append("s1", user_msg("hello")).await.unwrap();
        mem.append("s1", Message::assistant("hi")).await.unwrap();
        let msgs = mem.messages("s1").await.unwrap();
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[0], user_msg("hello"));
        assert_eq!(msgs[1], Message::assistant("hi"));
    }

    #[tokio::test]
    async fn messages_for_unknown_session_is_empty() {
        let mem = InMemoryMemory::new();
        assert!(mem.messages("nope").await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn working_memory_is_session_scoped() {
        let mem = InMemoryMemory::new();
        mem.set_context("a", "k", serde_json::json!(1))
            .await
            .unwrap();
        mem.set_context("b", "k", serde_json::json!(2))
            .await
            .unwrap();
        assert_eq!(
            mem.get_context("a", "k").await.unwrap(),
            Some(serde_json::json!(1))
        );
        assert_eq!(mem.get_context("missing", "k").await.unwrap(), None);
        assert_eq!(mem.get_context("a", "missing").await.unwrap(), None);
    }

    #[tokio::test]
    async fn recall_matches_substring_and_truncates() {
        let mem = InMemoryMemory::new();
        for i in 0..3 {
            mem.store(MemoryEntry::now(
                format!("e{i}"),
                format!("note {i} about rust"),
            ))
            .await
            .unwrap();
        }
        mem.store(MemoryEntry::now("e9", "unrelated content"))
            .await
            .unwrap();

        let rust_hits = mem.recall("rust", 10).await.unwrap();
        assert_eq!(rust_hits.len(), 3);

        let truncated = mem.recall("note", 2).await.unwrap();
        assert_eq!(truncated.len(), 2);

        let all = mem.recall("", 100).await.unwrap();
        assert_eq!(all.len(), 4);
    }

    #[tokio::test]
    async fn session_lifecycle_create_list_delete() {
        let mem = InMemoryMemory::new();
        let s = mem.create_session().await.unwrap();
        assert_eq!(mem.list_sessions().await.unwrap(), vec![s.clone()]);

        mem.append(&s, user_msg("x")).await.unwrap();
        mem.set_context(&s, "k", serde_json::json!(true))
            .await
            .unwrap();
        mem.delete_session(&s).await.unwrap();

        assert!(mem.messages(&s).await.unwrap().is_empty());
        assert_eq!(mem.get_context(&s, "k").await.unwrap(), None);
        assert!(mem.list_sessions().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn delete_session_keeps_long_term_memory() {
        let mem = InMemoryMemory::new();
        mem.store(MemoryEntry::now("lt", "global fact"))
            .await
            .unwrap();
        mem.delete_session("any").await.unwrap();
        let hits = mem.recall("global", 5).await.unwrap();
        assert_eq!(hits.len(), 1);
    }
}
