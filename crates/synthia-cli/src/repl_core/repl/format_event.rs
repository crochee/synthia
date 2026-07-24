//! Event formatting and stream rendering.
//!
//! [`Repl::format_event`] turns an [`AgentEvent`] into a presentable
//! string; [`Repl::render_event_stream`] drains a stream of events,
//! updating session state and printing each formatted line (or
//! streaming model chunks without newlines).

use std::io::{self, Write};

use futures::StreamExt;
use synthia_agent::{
    AgentEvent,
    events::{HookEvent, SessionEndReason, SystemEvent, WarningKind},
};
use synthia_provider::{ContentPart, TextContent};

use super::{syntax::format_with_syntax_highlighting, types::Repl};

impl Repl {
    /// Format an agent event for display in the REPL with theme colors and code highlighting.
    ///
    /// Returns the formatted string representation of the event.
    pub fn format_event(&self, event: &AgentEvent) -> String {
        let state = self.state.read();
        let theme = &state.theme;

        match event {
            AgentEvent::System(SystemEvent::SessionStarted { session_id }) => {
                format!("Session started: {}", session_id)
            }

            AgentEvent::Model(ContentPart::Text(TextContent {
                text, ..
            })) => text.clone(),

            AgentEvent::Model(ContentPart::Reasoning(r)) => {
                format!("[thinking] {}", r.text)
            }

            AgentEvent::Model(ContentPart::ToolUse(tu)) => {
                format!("[{}: {}]...", theme.format_tool_call("TOOL"), tu.name)
            }

            AgentEvent::Model(ContentPart::ToolResult(tr)) => {
                let text: String = tr
                    .content
                    .iter()
                    .filter_map(|c| match c {
                        ContentPart::Text(TextContent { text, .. }) => {
                            Some(text.clone())
                        }
                        _ => None,
                    })
                    .collect::<Vec<String>>()
                    .join("");
                let preview: String = text.chars().take(60).collect();
                let err = tr.is_error.unwrap_or(false);
                if err {
                    format!(
                        "[{}: {}] {} (error)",
                        theme.format_tool_call("TOOL"),
                        tr.tool_use_id,
                        theme.format_error(&preview)
                    )
                } else {
                    format!(
                        "[{}: {}] {}",
                        theme.format_tool_call("TOOL"),
                        tr.tool_use_id,
                        preview
                    )
                }
            }

            AgentEvent::Model(ContentPart::Image(_))
            | AgentEvent::Model(ContentPart::Audio(_))
            | AgentEvent::Model(ContentPart::Resource(_)) => String::new(),

            AgentEvent::ModelDone(sampling) => {
                format_with_syntax_highlighting(&sampling.text, theme)
            }

            AgentEvent::System(SystemEvent::SessionEnded { reason }) => {
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

            AgentEvent::System(SystemEvent::SessionInterrupted { reason }) => {
                format!("Session interrupted: {}", reason)
            }

            AgentEvent::System(SystemEvent::Warning {
                kind,
                message,
                iteration,
            }) => {
                let prefix = match kind {
                    WarningKind::Guardian => "Guardian",
                    WarningKind::Loop => "Loop",
                    WarningKind::TokenBudget => "Token budget",
                    WarningKind::ContextCompaction => "ContextCompaction",
                    WarningKind::Hook => "Hook",
                    WarningKind::EditConflict => "EditConflict",
                };
                match iteration {
                    Some(it) => {
                        format!(
                            "[{} #{}] {}",
                            prefix,
                            it,
                            theme.format_error(message)
                        )
                    }
                    None => {
                        format!("[{}] {}", prefix, theme.format_error(message))
                    }
                }
            }

            AgentEvent::System(SystemEvent::Progress {
                message,
                step,
                total,
            }) => {
                format!("Progress ({}/{}): {}", step, total, message)
            }

            AgentEvent::System(SystemEvent::Recovery {
                level_number,
                tool_name,
                message,
                iteration,
            }) => {
                let tool = tool_name.as_deref().unwrap_or("global");
                let iter = iteration
                    .map(|i| i.to_string())
                    .unwrap_or_else(|| "-".to_string());
                format!(
                    "[recovery L{} iter={} tool={}] {}",
                    level_number, iter, tool, message
                )
            }

            AgentEvent::System(SystemEvent::Usage {
                input_tokens,
                output_tokens,
                cache_read_tokens,
                cache_creation_tokens,
            }) => {
                format!(
                    "Usage: in={} out={} cache_read={:?} cache_create={:?}",
                    input_tokens,
                    output_tokens,
                    cache_read_tokens,
                    cache_creation_tokens
                )
            }

            AgentEvent::Hook(HookEvent::Message { priority, message }) => {
                format!("[hook pri={}] {}", priority, message)
            }

            AgentEvent::Hook(HookEvent::ConfirmRequest {
                tool_name,
                reason,
                ..
            }) => {
                format!(
                    "Guardian confirmation required for {}: {}",
                    theme.format_tool_call(tool_name),
                    theme.format_prompt(reason)
                )
            }

            AgentEvent::Hook(HookEvent::ConfirmResponse {
                approved,
                tool_use_id,
            }) => {
                format!(
                    "Guardian response for {}: {}",
                    tool_use_id,
                    if *approved { "approved" } else { "denied" }
                )
            }

            AgentEvent::Hook(HookEvent::Custom { kind, .. }) => {
                format!("[custom] {}", kind)
            }

            AgentEvent::Agent(meta, inner) => {
                format!(
                    "[subagent {}->{} depth={}] {}",
                    meta.parent_session_id,
                    meta.child_session_id,
                    meta.parent_depth,
                    self.format_event(inner)
                )
            }
        }
    }

    /// Render a stream of agent events to the terminal.
    pub(super) async fn render_event_stream(
        &self,
        stream: &mut (impl futures::Stream<Item = AgentEvent> + Unpin),
    ) {
        let mut stdout = io::stdout();
        while let Some(event) = stream.next().await {
            // Update session state from events
            self.state.write().update(&event);

            let formatted = self.format_event(&event);
            if formatted.is_empty() {
                continue;
            }

            // For streaming model chunks, write directly without
            // newline so the delta renders inline. For LLM reasoning
            // chunks same treatment.
            let is_streaming = matches!(
                event,
                AgentEvent::Model(ContentPart::Text(_))
                    | AgentEvent::Model(ContentPart::Reasoning(_))
            );
            if is_streaming {
                print!("{}", formatted);
                let _ = stdout.flush();
                continue;
            }

            println!("{}", formatted);
            let _ = stdout.flush();
        }
        let _ = stdout.flush();
    }
}
