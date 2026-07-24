//! MemoryService trait: the unified service interface for the memory subsystem.
//!
//! Defined in Phase 2 of the architecture refactoring (see
//! `docs/superpowers/specs/2026-06-03-synthia-architecture-refactoring-design.md`).
//!
//! The trait describes the three high-level operations the agent layer needs
//! from memory:
//!
//! - `store` ingests a `MemoryEvent` (session ended, tool executed, hot flush)
//!   and is responsible for routing it into hot/cold/episodic storage as
//!   appropriate.
//! - `retrieve` returns ranked `Memory` records matching a `MemoryQuery`.
//! - `consolidate` runs the periodic maintenance pass (importance decay,
//!   summarisation, eviction).
//!
//! Concrete implementations live alongside the existing stores; this module
//! intentionally only defines the contract and the supporting query/result
//! shapes the agent layer will see.
//!
//! Note: `Memory` and `MemoryQuery` are defined here (rather than imported
//! from `memory_pipeline::data`) because `memory_pipeline` is not currently
//! wired into the public module tree. They are deliberately a strict subset of
//! the pipeline types so a future consolidation is a renaming exercise.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::types::MemoryEvent;

type Result<T> = std::result::Result<T, synthia_core::Error>;

/// A retrieved memory record returned by [`MemoryService::retrieve`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Memory {
    pub id: String,
    pub session_id: String,
    pub content: String,
    pub tags: Vec<String>,
    pub importance: f32,
    pub created_at: DateTime<Utc>,
}

impl Memory {
    pub fn new(
        session_id: impl Into<String>,
        content: impl Into<String>,
    ) -> Self {
        Self {
            id: String::new(),
            session_id: session_id.into(),
            content: content.into(),
            tags: Vec::new(),
            importance: 0.5,
            created_at: Utc::now(),
        }
    }
}

/// Filter/ranking parameters for [`MemoryService::retrieve`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryQuery {
    pub query: String,
    pub session_id: Option<String>,
    pub tags: Option<Vec<String>>,
    pub min_importance: Option<f32>,
    pub limit: usize,
}

impl Default for MemoryQuery {
    fn default() -> Self {
        Self {
            query: String::new(),
            session_id: None,
            tags: None,
            min_importance: None,
            limit: 5,
        }
    }
}

/// High-level service interface for the memory subsystem.
///
/// See the module docs for the role of each method.
#[async_trait]
pub trait MemoryService: Send + Sync {
    /// Record a memory event. The implementation decides how to route it
    /// across hot/cold/episodic stores.
    async fn store(&self, event: MemoryEvent) -> Result<()>;

    /// Retrieve memories matching the query, ordered by the implementation's
    /// ranking (typically importance + recency + relevance).
    async fn retrieve(&self, query: &MemoryQuery) -> Result<Vec<Memory>>;

    /// Run the periodic consolidation pass (importance decay, summarisation,
    /// eviction). Safe to invoke repeatedly.
    async fn consolidate(&self) -> Result<()>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_new_sets_defaults() {
        let m = Memory::new("sess-1", "hello");
        assert_eq!(m.session_id, "sess-1");
        assert_eq!(m.content, "hello");
        assert!(m.tags.is_empty());
        assert!((m.importance - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn memory_query_default_limits_to_five() {
        let q = MemoryQuery::default();
        assert_eq!(q.limit, 5);
        assert!(q.query.is_empty());
        assert!(q.session_id.is_none());
    }
}
