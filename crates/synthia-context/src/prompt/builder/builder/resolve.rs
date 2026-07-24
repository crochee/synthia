//! Section-iteration core: [`PromptBuilder::resolve`] produces
//! [`ResolvedPrompt`] (with `prefix_hash` / `static_hash`), and
//! [`PromptBuilder::validate_prefix_stability`] re-hashes the
//! cached-only subset of the section list to detect regressions
//! in the static prefix.
//!
//! Both functions rely on `std::collections::hash_map::DefaultHasher`
//! (the std hasher; not `ahash`) because the resulting hash is
//! serialised to the on-disk session and must remain stable across
//! Rust toolchain versions.

use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
};

use super::{
    super::state::{PromptState, ResolvedPrompt},
    core::PromptBuilder,
};
use crate::prompt::{
    PromptContext,
    PromptSection,
    section_trait::SectionCaching,
};

impl PromptBuilder {
    /// Walk every section, classify it as static (Cached) vs
    /// dynamic (everything else), and emit a [`ResolvedPrompt`]
    /// with the joined static / dynamic halves plus the
    /// `prefix_hash` (full prompt hash) and `static_hash` (Cached
    /// sections only) for KV-cache stability tracking.
    pub fn resolve(
        &self,
        ctx: &PromptContext<'_>,
        state: &mut PromptState,
    ) -> anyhow::Result<ResolvedPrompt> {
        let mut static_content = String::new();
        let mut dynamic_content = String::new();
        let mut sections_used = Vec::new();
        let mut static_hasher = DefaultHasher::new();
        let mut full_hasher = DefaultHasher::new();

        for section in &self.sections {
            let caching = section.caching();

            let part = if caching == SectionCaching::Uncached {
                section.build(ctx)?
            } else if let Some(cached) = state.get(section.name(), caching) {
                cached
            } else {
                let value = section.build(ctx)?;
                state.insert(
                    section.name().to_string(),
                    value.clone(),
                    caching,
                );
                value
            };

            if part.trim().is_empty() {
                continue;
            }

            sections_used.push(section.name().to_string());

            let trimmed = part.trim_end();
            trimmed.hash(&mut full_hasher);

            if caching == SectionCaching::Cached {
                trimmed.hash(&mut static_hasher);
            }

            if caching == SectionCaching::Uncached
                || caching == SectionCaching::SessionCached
                || caching == SectionCaching::Volatile
            {
                if !dynamic_content.is_empty() {
                    dynamic_content.push_str("\n\n");
                }
                dynamic_content.push_str(trimmed);
            } else {
                if !static_content.is_empty() {
                    static_content.push_str("\n\n");
                }
                static_content.push_str(trimmed);
            }
        }

        let prefix_hash = format!("{:x}", full_hasher.finish());
        let static_hash = format!("{:x}", static_hasher.finish());

        Ok(ResolvedPrompt {
            static_content,
            dynamic_content,
            sections_used,
            prefix_hash,
            static_hash,
        })
    }

    /// Re-hash the Cached-only sections and compare against a
    /// previously recorded `static_hash`. Returns `true` when the
    /// static prefix is unchanged (KV-cache safe). Non-Cached
    /// sections are intentionally ignored — they don't affect the
    /// cache key.
    pub fn validate_prefix_stability(
        &self,
        ctx: &PromptContext<'_>,
        state: &PromptState,
        previous_static_hash: Option<&str>,
    ) -> anyhow::Result<bool> {
        let Some(previous) = previous_static_hash else {
            return Ok(true);
        };

        let mut static_hasher = DefaultHasher::new();

        for section in &self.sections {
            let caching = section.caching();
            if caching != SectionCaching::Cached {
                continue;
            }

            let part = if let Some(cached) = state.get(section.name(), caching)
            {
                cached
            } else {
                section.build(ctx)?
            };

            if part.trim().is_empty() {
                continue;
            }

            let trimmed = part.trim_end();
            trimmed.hash(&mut static_hasher);
        }

        let current_static_hash = format!("{:x}", static_hasher.finish());
        Ok(current_static_hash == previous)
    }
}
