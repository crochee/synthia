//! The [`Memory`](crate::memory::Memory) impl: conversation
//! delegation, working memory, long-term recall/store, and the
//! session registry.

use async_trait::async_trait;
use rusqlite::params;
use serde_json::Value;
use synthia_core::Clock;
use synthia_provider::Message;
use ulid::Ulid;

use super::{
    decode::rfc3339,
    error::{SqliteMemoryError, query_err},
    store::SqliteMemory,
};
use crate::memory::{Memory, MemoryEntry, MemoryError};
#[async_trait]
impl Memory for SqliteMemory {
    async fn messages(
        &self,
        session_id: &str,
    ) -> Result<Vec<Message>, MemoryError> {
        self.inner.messages(session_id).await
    }

    async fn append(
        &self,
        session_id: &str,
        message: Message,
    ) -> Result<(), MemoryError> {
        self.inner.append(session_id, message).await
    }

    async fn get_context(
        &self,
        session_id: &str,
        key: &str,
    ) -> Result<Option<Value>, MemoryError> {
        let conn = self.conn.lock();
        let raw = conn.query_row(
            "SELECT value FROM working_memory \
             WHERE session_id = ?1 AND key = ?2",
            params![session_id, key],
            |row| row.get::<_, String>(0),
        );
        let raw = match raw {
            Ok(raw) => Some(raw),
            Err(rusqlite::Error::QueryReturnedNoRows) => None,
            Err(error) => {
                return Err(query_err("read working memory", error).into());
            }
        };
        match raw {
            Some(raw) => {
                serde_json::from_str(&raw).map(Some).map_err(|error| {
                    MemoryError::Sqlite(SqliteMemoryError::Decode {
                        reason: format!("working memory {key:?}: {error}"),
                    })
                })
            }
            None => Ok(None),
        }
    }

    async fn set_context(
        &self,
        session_id: &str,
        key: &str,
        value: Value,
    ) -> Result<(), MemoryError> {
        self.writable()?;
        let encoded = value.to_string();
        let conn = self.conn.lock();
        conn.execute(
            "INSERT INTO working_memory (session_id, key, value) \
             VALUES (?1, ?2, ?3) \
             ON CONFLICT(session_id, key) DO UPDATE SET value = excluded.value",
            params![session_id, key, encoded],
        )
        .map_err(|error| query_err("write working memory", error))?;
        Ok(())
    }

    async fn recall(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<MemoryEntry>, MemoryError> {
        self.search(query, limit).map_err(MemoryError::from)
    }

    async fn store(&self, entry: MemoryEntry) -> Result<(), MemoryError> {
        self.store_entry(&entry).map_err(MemoryError::from)
    }

    async fn create_session(&self) -> Result<String, MemoryError> {
        self.writable()?;
        let id = Ulid::generate().to_string();
        let conn = self.conn.lock();
        conn.execute(
            "INSERT INTO sessions (id, created_at) VALUES (?1, ?2)",
            params![id, rfc3339(self.clock.now())],
        )
        .map_err(|error| query_err("create session", error))?;
        Ok(id)
    }

    async fn list_sessions(&self) -> Result<Vec<String>, MemoryError> {
        let conn = self.conn.lock();
        let mut stmt = conn
            .prepare(
                "SELECT id FROM sessions ORDER BY created_at DESC, id DESC",
            )
            .map_err(|error| query_err("list sessions", error))?;
        let rows = stmt
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(|error| query_err("list sessions", error))?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row.map_err(|error| query_err("list sessions", error))?);
        }
        Ok(out)
    }

    async fn delete_session(
        &self,
        session_id: &str,
    ) -> Result<(), MemoryError> {
        self.writable()?;
        {
            // The guard must not be held across the await below:
            // `Connection` is not `Send`.
            let conn = self.conn.lock();
            conn.execute(
                "DELETE FROM working_memory WHERE session_id = ?1",
                params![session_id],
            )
            .map_err(|error| query_err("delete working memory", error))?;
            conn.execute(
                "DELETE FROM sessions WHERE id = ?1",
                params![session_id],
            )
            .map_err(|error| query_err("delete session", error))?;
        }
        // The conversation tier is the sink's: deleting it is the
        // sink's business, exactly as in `FileMemory`.
        self.inner.delete_session(session_id).await
    }
}
