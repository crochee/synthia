//! The 2 [`SkillRegistry`] constructors.
//!
//! - [`SkillRegistry::new`] — standard constructor; loads
//!   the `SkillStateStore` from
//!   `<user_dir>/.skill-state.json` (with a default empty
//!   store on parse error), wires up the sparse / BM25 /
//!   dense indices, and leaves the embedding provider as
//!   `None`.
//! - [`SkillRegistry::new_with_provider`] — like `new` but
//!   also accepts an embedding provider and pre-initializes
//!   the dense index with the provider's model name.

use std::{
    collections::HashSet,
    sync::{Arc, atomic::AtomicIsize},
};

use indexmap::IndexMap;
use parking_lot::RwLock;
use synthia_provider::traits::ModelProvider;

use super::super::types::SkillRegistry;
use crate::{
    bm25::BM25Index,
    embedding::{DenseVectorIndex, SparseVectorIndex},
    types::{MatchConfig, SkillPaths, SkillStateStore},
};

impl SkillRegistry {
    pub fn new(paths: SkillPaths) -> Self {
        let state_path = paths.user_dir.join(".skill-state.json");
        let state_store =
            SkillStateStore::load(&state_path).unwrap_or_else(|_| {
                SkillStateStore {
                    disabled_skills: HashSet::new(),
                }
            });
        Self {
            skills: RwLock::new(IndexMap::new()),
            active_skills: RwLock::new(HashSet::new()),
            session_token_counter: AtomicIsize::new(0),
            match_config: MatchConfig::new(),
            paths,
            state_store: RwLock::new(state_store),
            vector_index: RwLock::new(SparseVectorIndex::new()),
            bm25_index: RwLock::new(BM25Index::new()),
            dense_index: Arc::new(RwLock::new(DenseVectorIndex::new(
                "unknown".to_string(),
            ))),
            embedding_provider: None,
        }
    }

    /// Create a registry with an embedding provider for dense vector matching.
    pub fn new_with_provider(
        paths: SkillPaths,
        provider: Arc<dyn ModelProvider>,
    ) -> Self {
        let model_name = provider.model_config().name.clone();
        let mut registry = Self::new(paths);
        registry.embedding_provider = Some(provider);
        *registry.dense_index.write() = DenseVectorIndex::new(model_name);
        registry
    }
}
