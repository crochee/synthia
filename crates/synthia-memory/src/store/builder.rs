//! The [`MemoryStoreImpl`] struct + 3 constructors.

use std::path::PathBuf;

use crate::{
    cold::ColdMemory,
    cold_jsonl::ColdJsonlMemory,
    context::ContextMemory,
    episodic_jsonl::EpisodicJsonlMemory,
    hot::HotMemory,
};

/// Unified MemoryStore implementation combining all
/// four memory layers.
pub struct MemoryStoreImpl {
    pub(super) hot: HotMemory,
    pub(super) cold_jsonl: ColdJsonlMemory,
    /// Optional SQLite-backed cold memory for advanced
    /// retrieval (BM25/Semantic/Hybrid).
    pub(super) cold_sqlite: Option<ColdMemory>,
    pub(super) episodic_jsonl: EpisodicJsonlMemory,
    pub(super) context: ContextMemory,
}

impl MemoryStoreImpl {
    pub fn new(base_dir: PathBuf) -> Self {
        let memory_dir = base_dir.join(".agents").join("memory");
        Self {
            hot: HotMemory::new(base_dir.clone()),
            cold_jsonl: ColdJsonlMemory::new(memory_dir.clone()),
            cold_sqlite: None,
            episodic_jsonl: EpisodicJsonlMemory::new(memory_dir.clone()),
            context: ContextMemory::new(),
        }
    }

    /// Create with explicit paths (useful for testing).
    pub fn with_paths(
        hot_dir: PathBuf,
        cold_path: PathBuf,
        episodic_path: PathBuf,
    ) -> Self {
        Self {
            hot: HotMemory::new(hot_dir),
            cold_jsonl: ColdJsonlMemory::with_path(cold_path),
            cold_sqlite: None,
            episodic_jsonl: EpisodicJsonlMemory::with_path(episodic_path),
            context: ContextMemory::new(),
        }
    }

    /// Create with SQLite-backed cold memory for advanced
    /// retrieval modes.
    pub async fn with_sqlite_cold(
        base_dir: PathBuf,
    ) -> Result<Self, synthia_core::Error> {
        let memory_dir = base_dir.join(".agents").join("memory");
        let cold_sqlite = ColdMemory::new(memory_dir.clone()).await?;
        Ok(Self {
            hot: HotMemory::new(base_dir),
            cold_jsonl: ColdJsonlMemory::new(memory_dir.clone()),
            cold_sqlite: Some(cold_sqlite),
            episodic_jsonl: EpisodicJsonlMemory::new(memory_dir),
            context: ContextMemory::new(),
        })
    }
}
