//! `SessionManager` — public API for tree operations.

use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use tokio::{
    sync::{Mutex, RwLock, mpsc, oneshot},
    task::JoinHandle,
};

use crate::{
    entry::SessionEntry,
    error::{Result, SessionError},
    tree::SessionTree,
    writer_task::{TreeCmd, session_writer_task},
};

/// Public API for managing a session tree + append-only JSONL file.
pub struct SessionManager {
    tree: Arc<RwLock<SessionTree>>,
    /// Path to the JSONL file (retained for reload logic in a later round).
    #[allow(dead_code)]
    path: Arc<RwLock<PathBuf>>,
    write_tx: mpsc::Sender<TreeCmd>,
    flush_handle: Mutex<Option<JoinHandle<()>>>,
}

impl SessionManager {
    /// Open or create a session at the given path.
    ///
    /// Spawns the background writer task and initializes an empty tree.
    /// Returns immediately; caller must call `shutdown()` to drain the writer.
    pub async fn open(path: &Path) -> Result<Self> {
        let (tx, rx) = mpsc::channel(10_000);
        let path_buf = path.to_path_buf();
        let writer_path = path_buf.clone();
        let handle = tokio::spawn(session_writer_task(writer_path, rx));

        let tree = SessionTree::new(
            synthia_protocol::SessionId::new(),
            synthia_protocol::MessageId::new(),
        );

        Ok(Self {
            tree: Arc::new(RwLock::new(tree)),
            path: Arc::new(RwLock::new(path_buf)),
            write_tx: tx,
            flush_handle: Mutex::new(Some(handle)),
        })
    }

    /// Append an entry to the tree (in-memory) and dispatch to writer (durable).
    pub async fn append(&self, entry: SessionEntry) -> Result<()> {
        {
            let mut tree = self.tree.write().await;
            tree.append(entry.clone());
        }
        let (ack_tx, ack_rx) = oneshot::channel();
        self.write_tx
            .send(TreeCmd::Append { entry, ack: ack_tx })
            .await
            .map_err(|_| SessionError::WriterClosed)?;
        ack_rx.await.map_err(|_| SessionError::WriterClosed)?
    }

    /// Flush the writer's buffer to disk. Awaits the writer's ack.
    pub async fn flush(&self) -> Result<()> {
        let (ack_tx, ack_rx) = oneshot::channel();
        self.write_tx
            .send(TreeCmd::Flush { ack: ack_tx })
            .await
            .map_err(|_| SessionError::WriterClosed)?;
        ack_rx.await.map_err(|_| SessionError::WriterClosed)?
    }

    /// Shutdown the writer: drain remaining cmds + flush + join. Consumes self.
    pub async fn shutdown(self) -> Result<()> {
        let (ack_tx, ack_rx) = oneshot::channel();
        self.write_tx
            .send(TreeCmd::Shutdown { ack: ack_tx })
            .await
            .map_err(|_| SessionError::WriterClosed)?;
        ack_rx.await.map_err(|_| SessionError::WriterClosed)?;
        if let Some(handle) = self.flush_handle.lock().await.take() {
            let _ = handle.await;
        }
        Ok(())
    }

    /// Read access to the tree.
    pub async fn tree(&self) -> tokio::sync::RwLockReadGuard<'_, SessionTree> {
        self.tree.read().await
    }

    /// Update the leaf pointer (used by branch/fork).
    pub async fn set_leaf(&self, leaf: synthia_protocol::MessageId) {
        self.tree.write().await.set_leaf(leaf);
    }
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use synthia_protocol::{MessageId, SessionId};

    use super::*;
    use crate::{entry::SessionEntry, part::Part};

    #[tokio::test]
    async fn open_close() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("session.jsonl");
        let mgr = SessionManager::open(&path).await.unwrap();
        mgr.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn append_then_shutdown_drains() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("session.jsonl");
        let mgr = SessionManager::open(&path).await.unwrap();
        let m1 = MessageId::new();
        mgr.append(SessionEntry::Message {
            id: m1,
            parent_message_id: Some(MessageId::new()),
            role: "user".to_string(),
            parts: vec![Part::Text(crate::part::TextPart {
                text: "hi".to_string(),
                synthetic: false,
            })],
            time: Utc::now(),
            agent_name: None,
            model_id: None,
        })
        .await
        .unwrap();
        mgr.shutdown().await.unwrap();
        let contents = std::fs::read_to_string(&path).unwrap();
        assert!(contents.contains("\"role\":\"user\""));
        assert!(contents.contains("\"text\":\"hi\""));
    }

    #[tokio::test]
    async fn tree_leaf_updates_after_append() {
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
        {
            let tree = mgr.tree().await;
            assert_eq!(tree.leaf, m1);
            assert_eq!(tree.depth(), 2);
        }
        mgr.shutdown().await.unwrap();
    }

    #[test]
    fn tree_includes_header_dummy_leaf() {
        let tree = SessionTree::new(SessionId::new(), MessageId::new());
        assert_eq!(tree.depth(), 1);
    }
}
