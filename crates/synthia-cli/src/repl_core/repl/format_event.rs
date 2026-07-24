//! Event formatting and stream rendering.
//!
//! [`Repl::format_event`] turns an [`AgentEvent`] into a
//! presentable string; [`Repl::render_event_stream`] drains a
//! stream of events, updating session state and printing each
//! formatted line (or streaming LLM deltas without newlines).

use std::io::{self, Write};

use futures::StreamExt;
use synthia_agent::{AgentEvent, SessionEndReason};

use super::{syntax::format_with_syntax_highlighting, types::Repl};

impl Repl {
    /// Format an agent event for display in the REPL with theme colors and code highlighting.
    ///
    /// Returns the formatted string representation of the event.
    pub fn format_event(&self, event: &AgentEvent) -> String {
        let state = self.state.read();
        let theme = &state.theme;

        match event {
            AgentEvent::SessionStarted { session_id } => {
                format!("Session started: {}", session_id)
            }
            AgentEvent::IterationStarted { .. } => String::new(),
            AgentEvent::LlmStreamDelta { content } => content.clone(),
            AgentEvent::ToolCallStarted { tool_name, .. } => {
                // Task 10.15: Colored tool call indicator
                format!(
                    "[{}: {}]...",
                    theme.format_tool_call("TOOL"),
                    tool_name
                )
            }
            AgentEvent::ToolCallCompleted {
                tool_name,
                output,
                is_error,
            } => {
                let preview = output.chars().take(60).collect::<String>();
                if *is_error {
                    format!(
                        "[{}: {}] {} (error)",
                        theme.format_tool_call("TOOL"),
                        tool_name,
                        theme.format_error(&preview)
                    )
                } else {
                    format!(
                        "[{}: {}] {}",
                        theme.format_tool_call("TOOL"),
                        tool_name,
                        preview
                    )
                }
            }
            AgentEvent::ToolCallSkipped { tool_name, reason } => {
                format!(
                    "[{}: {}] skipped: {}",
                    theme.format_tool_call("TOOL"),
                    tool_name,
                    theme.format_error(reason)
                )
            }
            AgentEvent::ToolCallError { tool_name, error } => {
                format!(
                    "[{}: {}] {}",
                    theme.format_tool_call("TOOL"),
                    tool_name,
                    theme.format_error(error)
                )
            }
            AgentEvent::LlmResponseComplete { content, .. } => {
                format_with_syntax_highlighting(content, theme)
            }
            AgentEvent::ContextCompacted {
                old_tokens,
                new_tokens,
            } => {
                format!(
                    "Context compacted: {} -> {} tokens (reduced by {:.0}%)",
                    old_tokens,
                    new_tokens,
                    ((*old_tokens - *new_tokens) as f64 / *old_tokens as f64)
                        * 100.0
                )
            }
            AgentEvent::TokenBudgetWarning { status, .. } => {
                format!("Token budget warning: {}", status)
            }
            AgentEvent::TokenBudgetNotice { .. } => String::new(),
            AgentEvent::EditConflict {
                tool_name,
                path,
                original_content_hash,
                current_content_hash,
                ..
            } => {
                format!(
                    "{}Edit conflict on {} (tool: {}){} - File was modified since read. Original hash: {}, Current hash: {}",
                    theme.format_error("⚠ "),
                    path,
                    tool_name,
                    theme.format_error(" ⚠"),
                    original_content_hash,
                    current_content_hash
                )
            }
            AgentEvent::SessionEnded { reason } => {
                let reason_str = match reason {
                    SessionEndReason::Completed => "completed".to_string(),
                    SessionEndReason::Cancelled => "cancelled".to_string(),
                    SessionEndReason::Error(e) => format!("error: {}", e),
                    SessionEndReason::TokenBudgetExceeded => {
                        "token budget exceeded".to_string()
                    }
                    SessionEndReason::MaxIterationsReached => {
                        "max iterations reached".to_string()
                    }
                    SessionEndReason::GuardianBlocked => {
                        "blocked by guardian".to_string()
                    }
                    SessionEndReason::LoopDetected => {
                        "loop detected".to_string()
                    }
                    SessionEndReason::CircuitBreakerOpen => {
                        "circuit breaker open".to_string()
                    }
                };
                format!("Session ended: {}", reason_str)
            }
            AgentEvent::GuardianWarning { reason, .. } => {
                format!("Guardian: {}", theme.format_error(reason))
            }
            AgentEvent::GuardianConfirmationRequest { tool_name, reason } => {
                format!(
                    "Guardian confirmation required for {}: {}",
                    theme.format_tool_call(tool_name),
                    theme.format_prompt(reason)
                )
            }
            AgentEvent::LlmError { error } => {
                format!("LLM error: {}", theme.format_error(error))
            }
            AgentEvent::HookError {
                hook_name, error, ..
            } => {
                format!(
                    "Hook error: {}: {}",
                    hook_name,
                    theme.format_error(error)
                )
            }
            AgentEvent::Thinking { text, iteration } => {
                format!("[thinking #{}] {}", iteration, text)
            }
            // All other events are silent in the REPL
            _ => String::new(),
        }
    }

    /// Render a stream of agent events to the terminal.
    pub(super) async fn render_event_stream(
        &self,
        stream: &mut (impl futures::Stream<Item = AgentEvent> + Unpin),
    ) {
        while let Some(event) = stream.next().await {
            // Update session state from events
            self.state.write().update(&event);

            let formatted = self.format_event(&event);
            if formatted.is_empty() {
                continue;
            }

            // For LLM deltas and reasoning deltas, stream directly without newline
            if matches!(event, AgentEvent::LlmStreamDelta { .. })
                || matches!(event, AgentEvent::LlmReasoningDelta { .. })
            {
                print!("{}", formatted);
                io::stdout().flush().ok();
                continue;
            }

            println!("{}", formatted);
        }
        io::stdout().flush().ok();
    }
}
