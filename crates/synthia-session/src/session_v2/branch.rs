//! `branch()` + `branch_with_summary()` + `fork()` + `build_context()`
//! — non-destructive tree operations.

use synthia_protocol::{MessageId, SessionId};

use crate::session_v2::{
    entry::SessionEntry,
    error::Result,
    manager::SessionManager,
};

impl SessionManager {
    /// Move the leaf pointer back to `target`. Original messages remain in the tree.
    pub async fn branch(&self, target: MessageId) -> Result<()> {
        self.set_leaf(target).await;
        Ok(())
    }

    /// Branch + write a summary entry for the abandoned branch.
    pub async fn branch_with_summary(
        &self,
        target: MessageId,
        summary: String,
    ) -> Result<MessageId> {
        self.branch(target).await?;
        let summary_id = MessageId::new();
        self.append(SessionEntry::BranchSummary {
            id: summary_id,
            parent_message_id: Some(target),
            from_message_id: target,
            summary,
            from_hook: false,
        })
        .await?;
        Ok(summary_id)
    }

    /// Fork the session at `at_message_id`, creating a new SessionId.
    /// Returns the new SessionId.
    pub async fn fork(&self, at_message_id: MessageId) -> Result<SessionId> {
        let new_sid = SessionId::new();
        self.append(SessionEntry::Fork {
            id: MessageId::new(),
            parent_session_id: new_sid,
            forked_at_message_id: at_message_id,
        })
        .await?;
        Ok(new_sid)
    }

    /// Build context for the next turn: walks root → leaf, preserving compaction summaries.
    pub async fn build_context(
        &self,
    ) -> Result<Vec<(MessageId, SessionEntry)>> {
        let tree = self.tree().await;
        Ok(tree
            .paths_from_root
            .iter()
            .filter_map(|id| {
                tree.entries
                    .get(&crate::session_v2::tree::MessageKey(*id))
                    .cloned()
                    .map(|e| (*id, e))
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use chrono::Utc;

    use super::*;
    use crate::session_v2::{entry::SessionEntry, part::Part};

    #[tokio::test]
    async fn branch_moves_leaf() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("session.jsonl");
        let mgr = SessionManager::open(&path).await.unwrap();
        let m1 = MessageId::new();
        mgr.append(SessionEntry::Message {
            id: m1,
            parent_message_id: Some(MessageId::new()),
            role: "user".to_string(),
            parts: vec![],
            time: Utc::now(),
            agent_name: None,
            model_id: None,
        })
        .await
        .unwrap();
        mgr.branch(m1).await.unwrap();
        {
            let tree = mgr.tree().await;
            assert_eq!(tree.leaf, m1);
        }
        mgr.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn branch_with_summary_appends_entry() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("session.jsonl");
        let mgr = SessionManager::open(&path).await.unwrap();
        let m1 = MessageId::new();
        mgr.append(SessionEntry::Message {
            id: m1,
            parent_message_id: Some(MessageId::new()),
            role: "user".to_string(),
            parts: vec![],
            time: Utc::now(),
            agent_name: None,
            model_id: None,
        })
        .await
        .unwrap();
        let summary_id = mgr
            .branch_with_summary(m1, "abandoned branch".to_string())
            .await
            .unwrap();
        {
            let tree = mgr.tree().await;
            assert!(tree.entries.contains_key(
                &crate::session_v2::tree::MessageKey(summary_id)
            ));
        }
        mgr.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn fork_creates_new_session_id() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("session.jsonl");
        let mgr = SessionManager::open(&path).await.unwrap();
        let m1 = MessageId::new();
        mgr.append(SessionEntry::Message {
            id: m1,
            parent_message_id: Some(MessageId::new()),
            role: "user".to_string(),
            parts: vec![],
            time: Utc::now(),
            agent_name: None,
            model_id: None,
        })
        .await
        .unwrap();
        let new_sid = mgr.fork(m1).await.unwrap();
        {
            let tree = mgr.tree().await;
            let has_fork = tree.entries.values().any(|e| {
                matches!(e, SessionEntry::Fork { parent_session_id, .. } if *parent_session_id == new_sid)
            });
            assert!(has_fork, "expected Fork entry referencing new SessionId");
        }
        mgr.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn build_context_walks_root_to_leaf() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("session.jsonl");
        let mgr = SessionManager::open(&path).await.unwrap();
        let m1 = MessageId::new();
        mgr.append(SessionEntry::Message {
            id: m1,
            parent_message_id: None,
            role: "user".to_string(),
            parts: vec![Part::Text(crate::session_v2::part::TextPart {
                text: "hello".to_string(),
                synthetic: false,
            })],
            time: Utc::now(),
            agent_name: None,
            model_id: None,
        })
        .await
        .unwrap();
        let ctx = mgr.build_context().await.unwrap();
        assert!(!ctx.is_empty(), "expected at least m1 in context, got 0");
        assert_eq!(ctx.len(), 1, "expected exactly m1, got {}", ctx.len());
        let (_, entry) = &ctx[0];
        match entry {
            SessionEntry::Message { id, .. } => assert_eq!(*id, m1),
            _ => panic!("expected Message entry"),
        }
        mgr.shutdown().await.unwrap();
    }
}
