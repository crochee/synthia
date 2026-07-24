//! Prefix source abstraction.
//!
//! Sources track content that contributes to the LLM prompt prefix (system
//! prompt, tool schemas, skill list). Each source has a baseline epoch and
//! reports deltas via [`Source::update`].

use std::hash::Hasher;

use ahash::AHasher;

mod epoch;
mod skill_list;
mod system_prompt;
mod tool_schemas;

pub use epoch::SourceEpoch;
pub use skill_list::SkillListSource;
pub use system_prompt::SystemPromptSource;
pub use tool_schemas::ToolSchemasSource;

/// Stable, type-safe identifier for a prefix source.
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct SourceId(pub &'static str);

/// Content snapshot from a source, used for baseline/current comparison.
#[derive(Debug, Clone)]
pub struct SourceContent(pub Vec<u8>);

impl SourceContent {
    /// Deterministic hash using `ahash::AHasher::default()`.
    pub fn hash(&self) -> u64 {
        let mut hasher = AHasher::default();
        hasher.write(&self.0);
        hasher.finish()
    }

    /// Construct from a string slice.
    pub fn from_text(s: &str) -> Self {
        Self(s.as_bytes().to_vec())
    }
}

/// Delta reported by [`Source::update()`].
#[derive(Debug, Clone)]
pub enum SourceDelta {
    /// Content changed; carries the new content.
    Changed(SourceContent),
    /// Content identical to last recorded state.
    Unchanged,
    /// Source no longer contributes to the prefix.
    Removed,
}

/// A source of prefix-affecting content.
///
/// Implementors track a specific category of prefix content (system prompt
/// text, tool schemas, skill list) and report changes via
/// [`update`](Source::update).
pub trait Source: Send + Sync {
    /// Stable identifier for this source.
    fn id(&self) -> SourceId;
    /// Initial epoch content.
    fn baseline(&self) -> SourceContent;
    /// Delta since the last `baseline()` or `update()` call.
    fn update(&mut self) -> SourceDelta;
}

#[cfg(test)]
mod tests;
