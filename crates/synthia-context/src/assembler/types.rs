//! Data definitions for [`ContextAssembler`].
//!
//! [`SectionPriorities`] is a small override config — six
//! optional `u8` slots, one per section type. Each is `None`
//! by default, and the resolver methods fall back to the
//! constants in [`crate::injector::priorities`].
//!
//! [`ContextAssembler`] holds the runtime buffers. Fields are
//! `pub(super)` so the action submodules ([`super::construct`],
//! [`super::inject`], [`super::assemble`],
//! [`super::pipeline`], [`super::trim`]) can manipulate
//! them directly without exposing a mutable accessor for
//! every field.

use synthia_provider::{Message, TokenCounter};
use synthia_skill::types::SkillMatch;

use crate::{injector::ContextInjector, protector::ProtectionZone};

/// Priority configuration overrides for context sections.
///
/// Allows customizing the default priority values used during context trimming.
#[derive(Debug, Clone, Default)]
pub struct SectionPriorities {
    pub system_prompt: Option<u8>,
    pub user_messages: Option<u8>,
    pub tool_results: Option<u8>,
    pub injected_memories: Option<u8>,
    pub skill_docs: Option<u8>,
    pub workspace_info: Option<u8>,
}

impl SectionPriorities {
    /// Resolve the priority for system prompt, using override or default.
    pub fn system_prompt(&self) -> u8 {
        self.system_prompt
            .unwrap_or(crate::injector::priorities::SYSTEM_PROMPT)
    }

    /// Resolve the priority for user messages, using override or default.
    pub fn user_messages(&self) -> u8 {
        self.user_messages
            .unwrap_or(crate::injector::priorities::USER_MESSAGES)
    }

    /// Resolve the priority for tool results, using override or default.
    pub fn tool_results(&self) -> u8 {
        self.tool_results
            .unwrap_or(crate::injector::priorities::TOOL_RESULTS)
    }

    /// Resolve the priority for injected memories, using override or default.
    pub fn injected_memories(&self) -> u8 {
        self.injected_memories
            .unwrap_or(crate::injector::priorities::INJECTED_MEMORIES)
    }

    /// Resolve the priority for skill docs, using override or default.
    pub fn skill_docs(&self) -> u8 {
        self.skill_docs
            .unwrap_or(crate::injector::priorities::SKILL_DOCS)
    }

    /// Resolve the priority for workspace info, using override or default.
    pub fn workspace_info(&self) -> u8 {
        self.workspace_info
            .unwrap_or(crate::injector::priorities::WORKSPACE_INFO)
    }
}

/// Layered context assembler.
///
/// Accumulates input from builders, state setters, and
/// injectors, then produces a deterministic `Vec<Section>`
/// (or a serialized system prompt) on demand.
///
/// # Deprecation
///
/// This struct is deprecated in favor of `FragmentRegistry::render_active()`.
/// The Registry-First architecture uses `FragmentRegistry` for modular context
/// injection. When `ExtensionRegistry` is available, fragment rendering replaces
/// this assembler's system prompt path. See `ExtensionRegistry` for the new
/// architecture.
#[deprecated(
    since = "0.1.0",
    note = "Use FragmentRegistry::render_active() instead. See ExtensionRegistry for the Registry-First architecture."
)]
pub struct ContextAssembler {
    pub(super) system_prompt: Option<String>,
    pub(super) memories: Vec<String>,
    pub(super) max_tokens: usize,
    pub(super) protection_zone: ProtectionZone,
    pub(super) skill_summaries: Option<String>,
    pub(super) skill_matches: Option<Vec<SkillMatch>>,
    pub(super) active_skill_prompts: Vec<String>,
    pub(super) requested_snippets: Vec<(String, Vec<String>)>,
    pub(super) token_counter: Option<Box<dyn TokenCounter>>,
    pub(super) injectors: Vec<Box<dyn ContextInjector>>,
    pub(super) workspace_info: Option<String>,
    pub(super) tool_results: Vec<String>,
    pub(super) conversation_messages: Vec<Message>,
    pub(super) priorities: SectionPriorities,
    /// Pre-rendered fragment content from `FragmentRegistry::render_active()`.
    /// When non-empty, these sections are included in the assembled output,
    /// delegating context injection to the FragmentRegistry.
    pub(super) rendered_fragments: Vec<(String, String)>,
}
