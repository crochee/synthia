//! Type definitions for the skill registry: the [`SkillRegistry`] struct
//! itself (shared state), the [`SkillFilter`] used by the
//! `Registry<Skill>` trait, and the `RegistryItem` impl for [`Skill`].
//!
//! Field visibility is `pub(super)` so that the impl blocks scattered
//! across the [`lifecycle`], [`query`], and [`registry_trait`] submodules
//! can manipulate registry state directly while keeping the public
//! surface narrow.
//!
//! [`Skill`]: crate::types::Skill

use std::{
    collections::HashSet,
    sync::{Arc, atomic::AtomicIsize},
};

use indexmap::IndexMap;
use parking_lot::RwLock;
use synthia_core::registry::RegistryItem;
use synthia_provider::traits::ModelProvider;

use crate::{
    bm25::BM25Index,
    embedding::{DenseVectorIndex, SparseVectorIndex},
    types::{
        MatchConfig,
        SkillPaths,
        SkillSource,
        SkillState,
        SkillStateStore,
    },
};

/// Filter passed to `Registry<Skill>::list`.
///
/// Combines three orthogonal filters: source, tag-membership, and
/// enabled-state. All three are AND-ed together when applied.
#[derive(Clone, Debug, Default)]
pub struct SkillFilter {
    pub source: Option<SkillSource>,
    pub tags: Vec<String>,
    pub enabled_only: bool,
}

impl SkillFilter {
    pub fn matches_skill(&self, skill: &crate::types::Skill) -> bool {
        if let Some(ref source) = self.source
            && skill.source != *source
        {
            return false;
        }

        if self.enabled_only && skill.state == SkillState::Disabled {
            return false;
        }

        if !self.tags.is_empty() {
            let has_tag = self
                .tags
                .iter()
                .any(|tag| skill.metadata.tags.contains(tag));
            if !has_tag {
                return false;
            }
        }

        true
    }
}

impl RegistryItem for crate::types::Skill {
    fn name(&self) -> &str {
        &self.metadata.name
    }

    fn description(&self) -> &str {
        &self.metadata.description
    }
}

/// The central skill registry.
///
/// Holds the loaded skills, per-skill state (active / disabled), the
/// three retrieval indices (sparse TF-IDF, BM25, dense), the
/// embedding provider used for dense matching, and the configured
/// [`MatchConfig`] knobs.
///
/// Field visibility is `pub(super)` so that the impl blocks in sibling
/// submodules can read and write registry state without going through
/// public accessor methods.
pub struct SkillRegistry {
    pub(super) skills: RwLock<IndexMap<String, crate::types::Skill>>,
    pub(super) active_skills: RwLock<HashSet<String>>,
    pub(super) session_token_counter: AtomicIsize,
    pub(super) match_config: MatchConfig,
    pub(super) paths: SkillPaths,
    pub(super) state_store: RwLock<SkillStateStore>,
    pub(super) vector_index: RwLock<SparseVectorIndex>,
    pub(super) bm25_index: RwLock<BM25Index>,
    pub(super) dense_index: Arc<RwLock<DenseVectorIndex>>,
    pub(super) embedding_provider: Option<Arc<dyn ModelProvider>>,
}
