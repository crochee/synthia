//! 4 loading + index-rebuild methods on
//! [`super::SkillRegistry`].
//!
//! - [`SkillRegistry::load_builtins`] — load all builtin
//!   skills from [`crate::builtin::BuiltinLoader`], insert
//!   into the skills map, then rebuild the vector +
//!   BM25 indices.
//! - [`SkillRegistry::load_from_paths`] — walk each path,
//!   find subdirectories that contain a `SKILL.md`, and
//!   call `load_skill` on each (skipping parse errors with a
//!   warn). Rebuilds both indices when done.
//! - [`SkillRegistry::rebuild_vector_index_internal`] —
//!   rebuilds the sparse vector index from
//!   `name + description` of every loaded skill.
//! - [`SkillRegistry::rebuild_bm25_index_internal`] —
//!   rebuilds the BM25 index from the full skill list.
//! - [`SkillRegistry::load_skill`] — single-skill loader:
//!   parses `SKILL.md` frontmatter + body, builds the
//!   `Skill` struct, skips if disabled in the state store,
//!   inserts, then spawns an async embedding-generation
//!   task (no-op if no provider is configured).

use std::{path::Path, sync::Arc};

use synthia_core::Error;

use super::SkillRegistry;
use crate::{
    bm25::BM25Index,
    loader::SkillLoader,
    types::{Skill, SkillLevel, SkillSource, SkillState, SkillTokenCount},
};

impl SkillRegistry {
    pub fn load_builtins(&self) -> Result<(), Error> {
        let builtins = crate::builtin::BuiltinLoader::load_builtins()?;
        for skill in builtins {
            let name = skill.metadata.name.clone();
            self.skills.write().insert(name, skill);
        }
        self.rebuild_vector_index_internal();
        self.rebuild_bm25_index_internal();
        Ok(())
    }

    pub fn load_from_paths(&self, paths: &[&Path]) -> Result<(), Error> {
        for path in paths {
            let entries = match std::fs::read_dir(path) {
                Ok(e) => e,
                Err(_) => continue,
            };
            for entry in entries.flatten() {
                let entry_path = entry.path();
                if entry_path.is_dir() && entry_path.join("SKILL.md").exists() {
                    match self.load_skill(&entry_path) {
                        Ok(_) => {}
                        Err(e) => {
                            tracing::warn!(path = ?entry_path, error = ?e, "Skipping invalid skill");
                        }
                    }
                }
            }
        }
        self.rebuild_vector_index_internal();
        self.rebuild_bm25_index_internal();
        Ok(())
    }

    pub(in crate::registry) fn rebuild_vector_index_internal(&self) {
        let skills = self.skills.read();
        let texts: Vec<(String, String)> = skills
            .iter()
            .map(|(_, s)| {
                (
                    s.metadata.name.clone(),
                    format!("{} {}", s.metadata.name, s.metadata.description),
                )
            })
            .collect();
        let mut index = self.vector_index.write();
        index.build_from_texts(&texts);
    }

    pub(in crate::registry) fn rebuild_bm25_index_internal(&self) {
        let skills = self.skills.read();
        let skill_list: Vec<_> = skills.values().cloned().collect();
        let index = BM25Index::build(&skill_list);
        *self.bm25_index.write() = index;
    }

    pub(crate) fn load_skill(&self, path: &Path) -> Result<(), Error> {
        let skill_md = path.join("SKILL.md");
        let metadata = SkillLoader::parse_frontmatter(&skill_md)?;
        let body = SkillLoader::parse_body(&skill_md)?;

        let warnings = SkillLoader::validate_optional_fields(&metadata);
        for w in &warnings {
            tracing::warn!(skill = %metadata.name, "{}", w);
        }

        let source = SkillSource::from_path(
            path,
            &self.paths.builtin_dir,
            &self.paths.project_dir,
            &self.paths.user_dir,
        );

        let level0_text =
            format!("{}: {}", metadata.name, metadata.description);
        let level1_text = body.clone();
        let token_count = SkillTokenCount {
            level0: level0_text.len() / 4,
            level1: level1_text.len() / 4,
        };

        let skill = Skill {
            metadata,
            body,
            source,
            level: SkillLevel::Level0,
            token_count,
            state: SkillState::Loaded,
        };

        let name = skill.metadata.name.clone();
        let state_store = self.state_store.read();
        if state_store.disabled_skills.contains(&name) {
            return Ok(());
        }
        drop(state_store);

        let mut skills = self.skills.write();
        skills.insert(name.clone(), skill);
        drop(skills);

        // Trigger async embedding generation if a provider is configured (non-blocking)
        self.spawn_embedding_generation(&name, path);

        Ok(())
    }

    /// Spawn a background task to generate and store the embedding for a skill.
    /// The skill is available immediately; embedding is added when ready.
    fn spawn_embedding_generation(&self, skill_name: &str, path: &Path) {
        let provider = match &self.embedding_provider {
            Some(p) => Arc::clone(p),
            None => return,
        };

        let dense_index = Arc::clone(&self.dense_index);
        let skill_name = skill_name.to_string();
        let path = path.to_path_buf();

        tokio::spawn(async move {
            // Parse the skill content to generate embedding text
            let content = match Self::skill_embedding_text(&path) {
                Ok(text) => text,
                Err(e) => {
                    tracing::warn!(skill = %skill_name, error = ?e, "Failed to read skill content for embedding");
                    return;
                }
            };

            match provider.embed(vec![content]).await {
                Ok(mut embeddings) => {
                    if let Some(embedding) = embeddings.pop() {
                        dense_index.write().insert(&skill_name, embedding);
                        tracing::debug!(skill = %skill_name, "Dense embedding generated");
                    }
                }
                Err(e) => {
                    tracing::warn!(skill = %skill_name, error = ?e, "Failed to generate dense embedding");
                }
            }
        });
    }

    /// Generate the text to embed for a skill (name + description + triggers + tags).
    fn skill_embedding_text(path: &Path) -> Result<String, Error> {
        let skill_md = path.join("SKILL.md");
        let metadata = SkillLoader::parse_frontmatter(&skill_md)?;

        let mut parts = vec![metadata.name, metadata.description];
        parts.extend(metadata.triggers);
        parts.extend(metadata.tags);

        Ok(parts.join(" "))
    }
}
