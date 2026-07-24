//! The [`MemoryRetriever`] struct + 4 constructors
//! (`new` / `with_recency_weight` /
//! `with_importance_weight` /
//! `with_semantic_retriever`).
//!
//! The 2 search methods are in [`super::search`].

use crate::{
    cold::ColdMemory,
    embedding::SemanticRetriever,
    episodic::EpisodicMemory,
    hot::HotMemory,
};

/// Searches across multiple memory layers with recency-weighted and importance-weighted ranking.
pub struct MemoryRetriever {
    pub(super) hot: HotMemory,
    pub(super) cold: ColdMemory,
    pub(super) episodic: EpisodicMemory,
    pub(super) recency_weight: f64,
    pub(super) importance_weight: f64,
    pub(super) semantic_retriever: Option<SemanticRetriever>,
}

impl MemoryRetriever {
    pub fn new(
        hot: HotMemory,
        cold: ColdMemory,
        episodic: EpisodicMemory,
    ) -> Self {
        Self {
            hot,
            cold,
            episodic,
            recency_weight: 0.3,
            importance_weight: 0.4,
            semantic_retriever: None,
        }
    }

    pub fn with_recency_weight(mut self, weight: f64) -> Self {
        self.recency_weight = weight.clamp(0.0, 1.0);
        self
    }

    pub fn with_importance_weight(mut self, weight: f64) -> Self {
        self.importance_weight = weight.clamp(0.0, 1.0);
        self
    }

    pub fn with_semantic_retriever(
        mut self,
        retriever: SemanticRetriever,
    ) -> Self {
        self.semantic_retriever = Some(retriever);
        self
    }
}
