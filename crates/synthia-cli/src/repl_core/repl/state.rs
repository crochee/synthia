//! [`SessionState`] methods.
//!
//! Kept separate from the data definitions in [`super::types`] so
//! the state-update logic is easy to audit in one place.

use synthia_agent::{AgentEvent, TokenUsage};
use synthia_provider::types::ContentPart;

use super::types::SessionState;

impl SessionState {
    /// Create a new session state with default values.
    pub fn new() -> Self {
        Self {
            iteration_count: 0,
            tool_call_count: 0,
            token_usage: TokenUsage {
                prompt_tokens: 0,
                completion_tokens: 0,
                total_tokens: 0,
                cached_prompt_tokens: None,
                cache_read_tokens: None,
                cache_write_tokens: None,
            },
            mode: crate::commands::AgentMode::default(),
            theme: crate::theme::Theme::default(),
        }
    }

    /// Build the dynamic prompt string (Task 10.16).
    pub fn prompt(&self) -> String {
        format!(
            "\n[iter:{}|tools:{}]> ",
            self.iteration_count, self.tool_call_count
        )
    }

    /// Update state from an agent event.
    pub fn update(&mut self, event: &AgentEvent) {
        match event {
            // IterationStarted is no longer a wire event — the REPL derives
            // iteration count from session/agent lifecycle instead.
            AgentEvent::Model(ContentPart::ToolUse(_)) => {
                self.tool_call_count += 1;
            }
            AgentEvent::ModelDone(sampling) => {
                self.token_usage = sampling.usage.clone();
            }
            _ => {}
        }
    }
}
