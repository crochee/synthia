//! Constructor + builder methods for [`ContextAssembler`].
//!
//! All 7 `with_*` builders follow the same pattern: take
//! ownership of `self`, mutate one field, return `self`.
//! This module is the single source for "how to construct
//! a ContextAssembler" — the action submodules
//! ([`super::inject`], [`super::assemble`],
//! [`super::pipeline`]) only mutate fields, they never
//! create a new instance.

use synthia_provider::TokenCounter;

use super::types::{ContextAssembler, SectionPriorities};
use crate::{injector::ContextInjector, protector::ProtectionZone};

impl ContextAssembler {
    /// Create a new assembler with the given `max_tokens`
    /// budget. All other buffers start empty.
    pub fn new(max_tokens: usize) -> Self {
        Self {
            system_prompt: None,
            memories: Vec::new(),
            max_tokens,
            protection_zone: ProtectionZone::default(),
            skill_summaries: None,
            skill_matches: None,
            active_skill_prompts: Vec::new(),
            requested_snippets: Vec::new(),
            token_counter: None,
            injectors: Vec::new(),
            workspace_info: None,
            tool_results: Vec::new(),
            conversation_messages: Vec::new(),
            priorities: SectionPriorities::default(),
        }
    }

    /// Set the system prompt (highest-priority section).
    pub fn with_system_prompt(mut self, prompt: String) -> Self {
        self.system_prompt = Some(prompt);
        self
    }

    /// Override the protection zone (recent-message guard
    /// used by [`super::pipeline::ContextAssembler::prepare`]).
    pub fn with_protection_zone(mut self, zone: ProtectionZone) -> Self {
        self.protection_zone = zone;
        self
    }

    /// Set a custom [`TokenCounter`] (provider-specific). If
    /// unset, [`super::assemble::ContextAssembler::estimate_total_tokens`]
    /// falls back to the `estimate_message_tokens` heuristic.
    pub fn with_token_counter(
        mut self,
        counter: Box<dyn TokenCounter>,
    ) -> Self {
        self.token_counter = Some(counter);
        self
    }

    /// Add a context injector to the assembler.
    pub fn with_injector(mut self, injector: Box<dyn ContextInjector>) -> Self {
        self.injectors.push(injector);
        self
    }

    /// Add multiple context injectors at once.
    pub fn with_injectors(
        mut self,
        injectors: Vec<Box<dyn ContextInjector>>,
    ) -> Self {
        self.injectors.extend(injectors);
        self
    }

    /// Set workspace info content.
    pub fn with_workspace_info(mut self, info: String) -> Self {
        self.workspace_info = Some(info);
        self
    }

    /// Set priority configuration overrides.
    pub fn with_priorities(mut self, priorities: SectionPriorities) -> Self {
        self.priorities = priorities;
        self
    }
}
