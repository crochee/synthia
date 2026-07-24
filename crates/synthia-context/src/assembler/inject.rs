//! State-mutating methods that fill the assembler's buffers
//! between build steps.
//!
//! These take `&mut self` (NOT `self`) so callers can stream
//! input incrementally:
//!
//! ```ignore
//! let mut a = ContextAssembler::new(4096);
//! a.inject_memories(memories);
//! a.set_skill_summaries(summaries);
//! a.add_tool_results(tool_results);
//! let prompt = a.build_system_prompt();
//! ```
//!
//! The 8 methods cover: memories, skill summaries, skill
//! matches, active skill prompts, requested snippets, tool
//! results, full conversation, and single-message append.

use synthia_provider::Message;
use synthia_skill::types::SkillMatch;

use super::types::ContextAssembler;

impl ContextAssembler {
    /// Replace the entire memory buffer.
    pub fn inject_memories(&mut self, memories: Vec<String>) {
        self.memories = memories;
    }

    pub fn set_skill_summaries(&mut self, summaries: String) {
        self.skill_summaries = Some(summaries);
    }

    pub fn set_skill_matches(&mut self, matches: Vec<SkillMatch>) {
        self.skill_matches = Some(matches);
    }

    pub fn add_active_skill_prompt(&mut self, prompt: String) {
        self.active_skill_prompts.push(prompt);
    }

    pub fn add_requested_snippets(
        &mut self,
        skill_name: String,
        snippets: Vec<String>,
    ) {
        self.requested_snippets.push((skill_name, snippets));
    }

    /// Add tool result entries to be included in context assembly.
    pub fn add_tool_results(&mut self, results: Vec<String>) {
        self.tool_results.extend(results);
    }

    /// Set conversation messages for priority-based trimming.
    pub fn set_conversation(&mut self, messages: Vec<Message>) {
        self.conversation_messages = messages;
    }

    /// Add a single conversation message.
    pub fn add_message(&mut self, message: Message) {
        self.conversation_messages.push(message);
    }
}
