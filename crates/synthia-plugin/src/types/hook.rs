//! Hook types for the plugin hook system.
//!
//! Defines the core types for hook configuration, events, handlers, and results.

use std::fmt;

use serde::{Deserialize, Serialize};

/// Events that can trigger hook execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum HookEvent {
    /// Fired when an agent starts
    AgentStart,
    /// Fired when an agent stops
    AgentStop,
    /// Fired before a tool is used
    PreToolUse,
    /// Fired after a tool is used
    PostToolUse,
    /// Fired before a prompt is processed
    PrePrompt,
    /// Fired when a session starts
    SessionStart,
}

impl fmt::Display for HookEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HookEvent::AgentStart => write!(f, "AgentStart"),
            HookEvent::AgentStop => write!(f, "AgentStop"),
            HookEvent::PreToolUse => write!(f, "PreToolUse"),
            HookEvent::PostToolUse => write!(f, "PostToolUse"),
            HookEvent::PrePrompt => write!(f, "PrePrompt"),
            HookEvent::SessionStart => write!(f, "SessionStart"),
        }
    }
}

/// Handler types for hook execution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase", tag = "type", content = "value")]
pub enum HookHandler {
    /// Runs a command as the hook handler
    Command(String),
    /// Processes the hook as an LLM-driven prompt
    Prompt(String),
}

impl HookHandler {
    /// Returns true if this is a Command handler
    pub fn is_command(&self) -> bool {
        matches!(self, HookHandler::Command(_))
    }

    /// Returns true if this is a Prompt handler
    pub fn is_prompt(&self) -> bool {
        matches!(self, HookHandler::Prompt(_))
    }

    /// Get the underlying command string if Command variant
    pub fn command(&self) -> Option<&str> {
        match self {
            HookHandler::Command(cmd) => Some(cmd),
            _ => None,
        }
    }

    /// Get the underlying prompt string if Prompt variant
    pub fn prompt(&self) -> Option<&str> {
        match self {
            HookHandler::Prompt(p) => Some(p),
            _ => None,
        }
    }
}

/// Specification for a single hook (event-based configuration).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookSpec {
    /// The event that triggers this hook
    pub event: HookEvent,
    /// Optional regex pattern to filter events
    #[serde(default)]
    pub matcher: Option<String>,
    /// The handler to execute when the hook fires
    pub handler: HookHandler,
    /// Priority for execution order (lower = runs first)
    #[serde(default)]
    pub priority: i32,
}

impl HookSpec {
    /// Create a new hook config with defaults
    pub fn new(event: HookEvent, handler: HookHandler) -> Self {
        Self {
            event,
            matcher: None,
            handler,
            priority: 0,
        }
    }

    /// Set a regex matcher for this hook
    pub fn with_matcher(mut self, matcher: impl Into<String>) -> Self {
        self.matcher = Some(matcher.into());
        self
    }

    /// Set priority for this hook
    pub fn with_priority(mut self, priority: i32) -> Self {
        self.priority = priority;
        self
    }
}

/// Result of hook execution, controlling flow.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HookResult {
    /// Continue to next hook (no short-circuit)
    #[default]
    Continue,
    /// Stop execution, cancel remaining hooks
    Stop,
    /// Hook execution failed; behavior depends on fail_mode
    Failed,
}

impl HookResult {
    /// Returns true if this is a Continue result
    pub fn is_continue(&self) -> bool {
        matches!(self, HookResult::Continue)
    }

    /// Returns true if this is a Stop result (short-circuit)
    pub fn is_stop(&self) -> bool {
        matches!(self, HookResult::Stop)
    }

    /// Returns true if hooks should continue running
    pub fn should_continue(&self) -> bool {
        matches!(self, HookResult::Continue)
    }

    /// Returns true if this is a Failed result
    pub fn is_failed(&self) -> bool {
        matches!(self, HookResult::Failed)
    }
}

/// Fail mode for hook execution.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize,
)]
#[serde(rename_all = "lowercase")]
pub enum FailMode {
    /// Hook failure allows execution to continue (default, backward-compatible)
    #[default]
    Open,
    /// Hook failure prevents execution
    Closed,
}

impl FailMode {
    /// Returns true if failures should allow execution (fail-open)
    pub fn is_open(&self) -> bool {
        matches!(self, FailMode::Open)
    }

    /// Returns true if failures should block execution (fail-closed)
    pub fn is_closed(&self) -> bool {
        matches!(self, FailMode::Closed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hook_event_display() {
        assert_eq!(HookEvent::AgentStart.to_string(), "AgentStart");
        assert_eq!(HookEvent::PreToolUse.to_string(), "PreToolUse");
        assert_eq!(HookEvent::SessionStart.to_string(), "SessionStart");
    }

    #[test]
    fn test_hook_handler_command() {
        let cmd = HookHandler::Command("echo test".to_string());
        assert!(cmd.is_command());
        assert!(!cmd.is_prompt());
        assert_eq!(cmd.command(), Some("echo test"));
        assert_eq!(cmd.prompt(), None);
    }

    #[test]
    fn test_hook_handler_prompt() {
        let prompt = HookHandler::Prompt("Analyze this".to_string());
        assert!(!prompt.is_command());
        assert!(prompt.is_prompt());
        assert_eq!(prompt.command(), None);
        assert_eq!(prompt.prompt(), Some("Analyze this"));
    }

    #[test]
    fn test_hook_config_builder() {
        let config = HookSpec::new(
            HookEvent::AgentStart,
            HookHandler::Command("ls".into()),
        )
        .with_matcher("test-.*")
        .with_priority(5);

        assert_eq!(config.event, HookEvent::AgentStart);
        assert_eq!(config.matcher, Some("test-.*".to_string()));
        assert_eq!(config.priority, 5);
        assert!(config.handler.is_command());
    }

    #[test]
    fn test_hook_result_continue() {
        assert!(HookResult::Continue.is_continue());
        assert!(!HookResult::Continue.is_stop());
        assert!(HookResult::Continue.should_continue());
    }

    #[test]
    fn test_hook_result_stop() {
        assert!(!HookResult::Stop.is_continue());
        assert!(HookResult::Stop.is_stop());
        assert!(!HookResult::Stop.should_continue());
    }

    #[test]
    fn test_hook_result_default() {
        let default: HookResult = Default::default();
        assert_eq!(default, HookResult::Continue);
    }

    #[test]
    fn test_hook_config_serde() {
        let config = HookSpec::new(
            HookEvent::PreToolUse,
            HookHandler::Prompt("Check this".to_string()),
        )
        .with_priority(10);

        let json = serde_json::to_string(&config).unwrap();
        let parsed: HookSpec = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.event, HookEvent::PreToolUse);
        assert_eq!(parsed.priority, 10);
        assert!(parsed.handler.is_prompt());
    }

    #[test]
    fn test_hook_event_serde() {
        let json = serde_json::to_string(&HookEvent::AgentStart).unwrap();
        let parsed: HookEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, HookEvent::AgentStart);

        let json = serde_json::to_string(&HookEvent::SessionStart).unwrap();
        let parsed: HookEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, HookEvent::SessionStart);
    }
}
