//! Read-side assembly — `assemble_sections` /
//! `build_system_prompt` / `section_by_name` /
//! `system_snapshot` / private `estimate_total_tokens`.
//!
//! [`ContextAssembler::assemble_sections`] is the only
//! mutating entry point in this module (it builds a fresh
//! `Vec<Section>` every call), but `&self` is enough — no
//! state on the assembler is touched.
//!
//! [`ContextAssembler::system_snapshot`] is the byte-level
//! snapshot used by `PrefixTracker` to detect
//! cache-affecting changes. Two calls with no intervening
//! mutations MUST return byte-identical output.

use synthia_provider::{Message, Role};

use super::types::ContextAssembler;
use crate::{
    injector::Section,
    traits::{estimate_message_tokens, extract_message_text},
};

impl ContextAssembler {
    /// Assemble context sections from all sources.
    ///
    /// Returns a vector of Sections ordered by assembly order, each with
    /// its assigned priority. Injectors are called to contribute their data.
    pub fn assemble_sections(&self) -> Vec<Section> {
        let mut sections = Vec::new();

        // Rendered fragments from FragmentRegistry (highest priority,
        // replaces legacy system prompt when available).
        for (name, content) in &self.rendered_fragments {
            sections.push(Section::new(
                format!("Fragment: {}", name),
                content.clone(),
                self.priorities.system_prompt(),
            ));
        }

        // System prompt (highest priority) — skipped when fragments
        // are present, as FragmentRegistry supersedes this path.
        if self.rendered_fragments.is_empty() {
            if let Some(prompt) = &self.system_prompt {
                sections.push(Section::new(
                    "System Prompt",
                    prompt.clone(),
                    self.priorities.system_prompt(),
                ));
            }
        }

        // Injected system prompts from injectors
        for injector in &self.injectors {
            if let Some(injected_prompt) = injector.inject_system_prompt() {
                sections.push(Section::new(
                    format!("Injector: {}", injector.name()),
                    injected_prompt,
                    self.priorities.system_prompt(),
                ));
            }
        }

        // Memories (from direct injection and injectors)
        let mut all_memories = self.memories.clone();
        for injector in &self.injectors {
            for (title, content) in injector.inject_memories() {
                all_memories.push(format!("**{}**: {}", title, content));
            }
        }
        if !all_memories.is_empty() {
            sections.push(Section::new(
                "Memories",
                format!("# Memories\n\n{}", all_memories.join("\n")),
                self.priorities.injected_memories(),
            ));
        }

        // Tool results
        if !self.tool_results.is_empty() {
            sections.push(Section::new(
                "Tool Results",
                format!("# Tool Results\n\n{}", self.tool_results.join("\n")),
                self.priorities.tool_results(),
            ));
        }

        // Skill summaries
        if let Some(skills) = &self.skill_summaries {
            sections.push(Section::new(
                "Skill Documentation",
                skills.clone(),
                self.priorities.skill_docs(),
            ));
        }

        // Active skill prompts (Level 1 content for activated skills)
        for prompt in &self.active_skill_prompts {
            sections.push(Section::new(
                "Active Skill",
                prompt.clone(),
                self.priorities.skill_docs(),
            ));
        }

        // Snippet requests (Level 2 content)
        for (skill_name, snippet_names) in &self.requested_snippets {
            sections.push(Section::new(
                format!("Skill Snippet: {}", skill_name),
                format!(
                    "Requested snippets for {}: {:?}",
                    skill_name, snippet_names
                ),
                self.priorities.skill_docs(),
            ));
        }

        // Workspace info
        if let Some(info) = &self.workspace_info {
            sections.push(Section::new(
                "Workspace Info",
                info.clone(),
                self.priorities.workspace_info(),
            ));
        }

        // Conversation messages
        if !self.conversation_messages.is_empty() {
            let conversation_text: Vec<String> = self
                .conversation_messages
                .iter()
                .map(|m| {
                    let role = match m.role {
                        Role::User => "user",
                        Role::Assistant => "assistant",
                        Role::System => "system",
                        Role::Tool => "tool",
                    };
                    format!("{}: {}", role, extract_message_text(m))
                })
                .collect();
            sections.push(Section::new(
                "Conversation",
                conversation_text.join("\n"),
                self.priorities.user_messages(),
            ));
        }

        sections
    }

    /// Build the complete system prompt string from all sections.
    pub fn build_system_prompt(&self) -> String {
        let sections = self.assemble_sections();
        if sections.is_empty() {
            return String::new();
        }
        sections
            .iter()
            .map(|s| s.content.clone())
            .collect::<Vec<_>>()
            .join("\n\n")
    }

    /// Look up a section by its title.
    ///
    /// Returns the first section whose `title` equals `name` (case-sensitive).
    /// Used by post-assembly inspection (e.g. self-reflection that needs to
    /// read a specific system section without re-assembling).
    pub fn section_by_name(&self, name: &str) -> Option<Section> {
        let sections = self.assemble_sections();
        sections.into_iter().find(|s| s.title == name)
    }

    /// Return the byte-level snapshot of the system prefix.
    ///
    /// The snapshot is used by `PrefixTracker` to detect cache-affecting
    /// changes across LLM calls. Two calls with no intervening mutations
    /// MUST return byte-identical output. Implementation delegates to
    /// `build_system_prompt()` and serializes to UTF-8 bytes.
    pub fn system_snapshot(&self) -> Vec<u8> {
        self.build_system_prompt().into_bytes()
    }

    /// Calculate total estimated tokens for messages.
    /// Uses the provider's TokenCounter if available, otherwise falls back to estimation heuristics.
    pub(super) fn estimate_total_tokens(&self, messages: &[Message]) -> usize {
        match &self.token_counter {
            Some(counter) => {
                messages.iter().map(|m| counter.count_message(m)).sum()
            }
            None => messages.iter().map(estimate_message_tokens).sum(),
        }
    }
}
