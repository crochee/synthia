//! The [`AgentsMdSection`] struct itself, plus its public constructors
//! and the [`PromptSection`] impl that wires the walk → merge → format
//! pipeline together.

use std::sync::Arc;

use anyhow::Result;

use super::{
    config::AgentsMdConfig,
    pipeline::format_merged,
    walk::walk_ancestors,
};
use crate::prompt::{PromptContext, PromptSection, SectionCaching};

/// `PromptSection` that injects hierarchically-discovered `AGENTS.md`
/// files into the system prompt.
#[derive(Debug, Clone)]
pub struct AgentsMdSection {
    config: Arc<AgentsMdConfig>,
}

impl AgentsMdSection {
    /// Construct a section with the default configuration.
    pub fn new() -> Self {
        Self {
            config: Arc::new(AgentsMdConfig::default()),
        }
    }

    /// Construct a section with a custom configuration.
    pub fn with_config(config: AgentsMdConfig) -> Self {
        Self {
            config: Arc::new(config),
        }
    }

    /// Borrow the current configuration.
    pub fn config(&self) -> &AgentsMdConfig {
        &self.config
    }
}

impl Default for AgentsMdSection {
    fn default() -> Self {
        Self::new()
    }
}

impl PromptSection for AgentsMdSection {
    fn name(&self) -> &str {
        "agents_md"
    }

    fn caching(&self) -> SectionCaching {
        SectionCaching::SessionCached
    }

    fn build(&self, ctx: &PromptContext<'_>) -> Result<String> {
        if !self.config.enabled {
            return Ok(String::new());
        }

        let discovered =
            walk_ancestors(ctx.workspace_dir, &self.config.filenames);
        if discovered.is_empty() {
            return Ok(String::new());
        }

        let merged = super::pipeline::merge_within_limit(
            discovered,
            self.config.max_chars_per_file,
            self.config.max_chars_total,
        );

        for file in &merged {
            tracing::debug!(
                path = %file.path.display(),
                chars = file.char_count(),
                "agents_md loaded"
            );
        }

        Ok(format_merged(&merged, self.config.max_chars_total))
    }
}
