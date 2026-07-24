//! Unit tests for the REPL.
//!
//! Covers:
//! - `handle_command` parsing (every slash command variant)
//! - `format_event` for each `AgentEvent` variant
//! - `SessionState` prompt + update logic
//! - `Theme` defaults and format methods
//! - `AgentMode` `FromStr` / `Display` / `Default`
//! - `Repl` construction (command registry, workspace root)

use std::path::PathBuf;

use crossterm::style::Color;
use regex::Regex;
use serde_json::json;
use synthia_agent::{
    AgentEvent,
    SessionEndReason,
    events::{HookEvent, SystemEvent, WarningKind},
};
use synthia_provider::types::{
    ContentPart,
    TextContent,
    ToolResult as ProviderToolResult,
    ToolUse,
};

use super::types::{CommandAction, Repl, SessionState};
use crate::{
    commands::{AgentMode, CliCommand},
    theme::Theme,
};

/// Strip ANSI escape codes from a string for plain-text comparison.
fn strip_ansi(s: &str) -> String {
    let re = Regex::new(r"\x1b\[[0-9;]*m").unwrap();
    re.replace_all(s, "").to_string()
}

#[test]
fn test_handle_command_quit() {
    let repl = Repl::new(PathBuf::from("/tmp"));
    assert!(matches!(repl.handle_command("/quit"), CommandAction::Quit));
    assert!(matches!(repl.handle_command("/exit"), CommandAction::Quit));
}

#[test]
fn test_handle_command_help() {
    let repl = Repl::new(PathBuf::from("/tmp"));
    assert!(matches!(repl.handle_command("/help"), CommandAction::Help));
}

#[test]
fn test_handle_command_clear() {
    let repl = Repl::new(PathBuf::from("/tmp"));
    assert!(matches!(
        repl.handle_command("/clear"),
        CommandAction::Clear
    ));
}

#[test]
fn test_handle_command_non_slash_returns_agent_message() {
    let repl = Repl::new(PathBuf::from("/tmp"));
    match repl.handle_command("hello world") {
        CommandAction::AgentMessage(msg) => assert_eq!(msg, "hello world"),
        other => panic!("Expected AgentMessage, got {:?}", other),
    }
    match repl.handle_command("  test message  ") {
        CommandAction::AgentMessage(msg) => assert_eq!(msg, "test message"),
        other => panic!("Expected AgentMessage, got {:?}", other),
    }
}

#[test]
fn test_handle_command_unknown() {
    let repl = Repl::new(PathBuf::from("/tmp"));
    // Unknown commands print a message and return Empty
    assert!(matches!(
        repl.handle_command("/unknown"),
        CommandAction::Empty
    ));
}

#[test]
fn test_handle_command_model() {
    let repl = Repl::new(PathBuf::from("/tmp"));
    match repl.handle_command("/model gpt-4") {
        CommandAction::Execute(CliCommand::Model(Some(model))) => {
            assert_eq!(model, "gpt-4")
        }
        other => panic!("Expected Execute(Model), got {:?}", other),
    }
    match repl.handle_command("/model") {
        CommandAction::Execute(CliCommand::Model(None)) => {}
        other => panic!("Expected Execute(Model(None)), got {:?}", other),
    }
}

#[test]
fn test_handle_command_empty() {
    let repl = Repl::new(PathBuf::from("/tmp"));
    assert!(matches!(repl.handle_command(""), CommandAction::Empty));
    assert!(matches!(repl.handle_command("   "), CommandAction::Empty));
}

#[test]
fn test_handle_command_shortcuts() {
    let repl = Repl::new(PathBuf::from("/tmp"));
    assert!(matches!(repl.handle_command("/q"), CommandAction::Quit));
    assert!(matches!(repl.handle_command("/h"), CommandAction::Help));
}

#[test]
fn test_format_event_session_started() {
    let repl = Repl::new(PathBuf::from("/tmp"));
    let event = AgentEvent::System(SystemEvent::SessionStarted {
        session_id: "test-session".to_string(),
    });
    assert_eq!(repl.format_event(&event), "Session started: test-session");
}

#[test]
fn test_format_event_model_text_delta() {
    let repl = Repl::new(PathBuf::from("/tmp"));
    let event = AgentEvent::Model(ContentPart::Text(TextContent {
        text: "Hello world".to_string(),
        cache_control: None,
    }));
    assert_eq!(repl.format_event(&event), "Hello world");
}

#[test]
fn test_format_event_tool_call_started() {
    let repl = Repl::new(PathBuf::from("/tmp"));
    let event = AgentEvent::Model(ContentPart::ToolUse(ToolUse {
        id: "call-1".to_string(),
        name: "read_file".to_string(),
        input: json!({ "path": "/tmp/test" }),
    }));
    let formatted = strip_ansi(&repl.format_event(&event));
    assert!(formatted.contains("[TOOL: read_file]"));
}

#[test]
fn test_format_event_tool_call_completed() {
    let repl = Repl::new(PathBuf::from("/tmp"));
    let event = AgentEvent::Model(ContentPart::ToolResult(
        ProviderToolResult::new("call-1", "File contents here"),
    ));
    let formatted = strip_ansi(&repl.format_event(&event));
    assert!(formatted.contains("[TOOL: call-1]"));
    assert!(formatted.contains("File contents"));
}

#[test]
fn test_format_event_tool_call_error() {
    let repl = Repl::new(PathBuf::from("/tmp"));
    let event = AgentEvent::Model(ContentPart::ToolResult(
        ProviderToolResult::error("call-1", "not found"),
    ));
    let formatted = strip_ansi(&repl.format_event(&event));
    assert!(formatted.contains("[TOOL: call-1]"));
    assert!(formatted.contains("not found"));
    assert!(formatted.contains("error"));
}

#[test]
fn test_format_event_token_budget_warning() {
    let repl = Repl::new(PathBuf::from("/tmp"));
    let event = AgentEvent::System(SystemEvent::Warning {
        kind: WarningKind::TokenBudget,
        message: "near limit".to_string(),
        iteration: None,
    });
    assert!(repl.format_event(&event).contains("Token budget"));
}

#[test]
fn test_format_event_token_budget_usage_silent() {
    // Usage is non-silent in the new event model, but the per-line
    // format does not change session state — just verify it renders
    // without panicking.
    let repl = Repl::new(PathBuf::from("/tmp"));
    let event = AgentEvent::System(SystemEvent::Usage {
        input_tokens: 5000,
        output_tokens: 1000,
        cache_read_tokens: None,
        cache_creation_tokens: None,
    });
    let formatted = repl.format_event(&event);
    assert!(formatted.contains("Usage"));
}

#[test]
fn test_format_event_edit_conflict() {
    let repl = Repl::new(PathBuf::from("/tmp"));
    let event = AgentEvent::System(SystemEvent::Warning {
        kind: WarningKind::EditConflict,
        message: "edit conflict on src/main.rs (write: 43981 -> 57072)"
            .to_string(),
        iteration: None,
    });
    let formatted = repl.format_event(&event);
    let stripped = strip_ansi(&formatted);
    assert!(stripped.contains("EditConflict"));
    assert!(stripped.contains("src/main.rs"));
    assert!(stripped.contains("43981"));
    assert!(stripped.contains("57072"));
}

#[test]
fn test_format_event_session_ended() {
    let repl = Repl::new(PathBuf::from("/tmp"));
    let event = AgentEvent::System(SystemEvent::SessionEnded {
        reason: SessionEndReason::Completed,
    });
    assert!(repl.format_event(&event).contains("Session ended"));
}

#[test]
fn test_format_event_session_ended_error() {
    let repl = Repl::new(PathBuf::from("/tmp"));
    let event = AgentEvent::System(SystemEvent::SessionEnded {
        reason: SessionEndReason::Error("connection failed".to_string()),
    });
    assert!(
        repl.format_event(&event)
            .contains("error: connection failed")
    );
}

#[test]
fn test_format_event_guardian_warning() {
    let repl = Repl::new(PathBuf::from("/tmp"));
    let event = AgentEvent::System(SystemEvent::Warning {
        kind: WarningKind::Guardian,
        message: "safety violation".to_string(),
        iteration: Some(3),
    });
    let formatted = strip_ansi(&repl.format_event(&event));
    assert!(formatted.contains("Guardian"));
    assert!(formatted.contains("safety violation"));
}

#[test]
fn test_format_event_guardian_confirmation_request() {
    let repl = Repl::new(PathBuf::from("/tmp"));
    let event = AgentEvent::Hook(HookEvent::ConfirmRequest {
        tool_use_id: "call-1".to_string(),
        tool_name: "shell_exec".to_string(),
        reason: "destructive operation".to_string(),
    });
    let formatted = strip_ansi(&repl.format_event(&event));
    assert!(formatted.contains("Guardian confirmation required"));
    assert!(formatted.contains("shell_exec"));
    assert!(formatted.contains("destructive operation"));
}

#[test]
fn test_format_event_tool_call_skipped() {
    let repl = Repl::new(PathBuf::from("/tmp"));
    let event = AgentEvent::Hook(HookEvent::ConfirmResponse {
        approved: false,
        tool_use_id: "call-write".to_string(),
    });
    let formatted = strip_ansi(&repl.format_event(&event));
    assert!(formatted.contains("call-write"));
    assert!(formatted.contains("denied"));
}

#[test]
fn test_format_event_llm_error() {
    let repl = Repl::new(PathBuf::from("/tmp"));
    let event = AgentEvent::System(SystemEvent::SessionEnded {
        reason: SessionEndReason::Error("timeout".to_string()),
    });
    let formatted = strip_ansi(&repl.format_event(&event));
    assert!(formatted.contains("error: timeout"));
}

#[test]
fn test_format_event_hook_error() {
    let repl = Repl::new(PathBuf::from("/tmp"));
    let event = AgentEvent::System(SystemEvent::Warning {
        kind: WarningKind::Hook,
        message: "pre_request: failed to connect".to_string(),
        iteration: None,
    });
    let formatted = strip_ansi(&repl.format_event(&event));
    assert!(formatted.contains("Hook"));
    assert!(formatted.contains("pre_request"));
    assert!(formatted.contains("failed to connect"));
}

#[test]
fn test_format_event_other_events_silent() {
    let repl = Repl::new(PathBuf::from("/tmp"));

    let events = [
        AgentEvent::Model(ContentPart::Image(
            synthia_provider::types::ImageContent {
                data: "x".into(),
                mime_type: "image/png".into(),
                detail: None,
            },
        )),
        AgentEvent::Model(ContentPart::Audio(
            synthia_provider::types::AudioContent {
                data: "x".into(),
                mime_type: "audio/wav".into(),
                format: None,
            },
        )),
        AgentEvent::Hook(HookEvent::Custom {
            kind: "my_extension".to_string(),
            data: json!({}),
        }),
    ];

    // Image and Audio ContentParts render as empty strings. Custom
    // hook events render a label. We assert that Image/Audio are
    // silent, and Custom renders a label.
    assert_eq!(repl.format_event(&events[0]), "");
    assert_eq!(repl.format_event(&events[1]), "");
    let custom = repl.format_event(&events[2]);
    assert!(custom.contains("my_extension"));
}

#[test]
fn test_repl_creation_with_command_registry() {
    let repl = Repl::new(PathBuf::from("/tmp"));
    assert!(
        repl.command_registry.len() >= 6,
        "Should have built-in commands registered"
    );
}

#[test]
fn test_repl_workspace_root() {
    let path = PathBuf::from("/tmp/test_workspace");
    let repl = Repl::new(path.clone());
    assert_eq!(repl.workspace_root, path);
}

#[test]
fn test_handle_command_session_variants() {
    let repl = Repl::new(PathBuf::from("/tmp"));

    match repl.handle_command("/session") {
        CommandAction::Execute(CliCommand::Session(None)) => {}
        other => panic!("Expected Session(None), got {:?}", other),
    }

    match repl.handle_command("/session new") {
        CommandAction::Execute(CliCommand::Session(Some(arg))) => {
            assert_eq!(arg, "new")
        }
        other => panic!("Expected Session(new), got {:?}", other),
    }

    match repl.handle_command("/session list") {
        CommandAction::Execute(CliCommand::SessionList) => {}
        other => panic!("Expected SessionList, got {:?}", other),
    }

    match repl.handle_command("/session switch abc-123") {
        CommandAction::Execute(CliCommand::SessionSwitch(id)) => {
            assert_eq!(id, "abc-123")
        }
        other => panic!("Expected SessionSwitch, got {:?}", other),
    }
}

#[test]
fn test_handle_command_tools() {
    let repl = Repl::new(PathBuf::from("/tmp"));
    assert!(matches!(
        repl.handle_command("/tools"),
        CommandAction::Execute(CliCommand::Tools)
    ));
}

#[test]
fn test_handle_command_provider() {
    let repl = Repl::new(PathBuf::from("/tmp"));
    match repl.handle_command("/provider openai") {
        CommandAction::Execute(CliCommand::Provider(Some(name))) => {
            assert_eq!(name, "openai")
        }
        other => panic!("Expected Provider(openai), got {:?}", other),
    }
}

#[test]
fn test_format_event_tool_call_completed_long_output_truncated() {
    let repl = Repl::new(PathBuf::from("/tmp"));
    let long_output = "x".repeat(200);
    let event = AgentEvent::Model(ContentPart::ToolResult(
        ProviderToolResult::new("call-search", &long_output),
    ));
    let formatted = strip_ansi(&repl.format_event(&event));
    // Should be truncated to 60 chars
    assert!(formatted.len() < 120);
    assert!(formatted.contains("[TOOL: call-search]"));
}

#[test]
fn test_mode_command_parsing() {
    let repl = Repl::new(PathBuf::from("/tmp"));
    match repl.handle_command("/mode") {
        CommandAction::Mode(None) => {}
        other => panic!("Expected Mode(None), got {:?}", other),
    }
    match repl.handle_command("/mode plan") {
        CommandAction::Mode(Some(m)) => assert_eq!(m, "plan"),
        other => panic!("Expected Mode(plan), got {:?}", other),
    }
}

#[test]
fn test_status_command_parsing() {
    let repl = Repl::new(PathBuf::from("/tmp"));
    assert!(matches!(
        repl.handle_command("/status"),
        CommandAction::Status
    ));
}

#[test]
fn test_compact_command_parsing() {
    let repl = Repl::new(PathBuf::from("/tmp"));
    assert!(matches!(
        repl.handle_command("/compact"),
        CommandAction::Compact
    ));
}

#[test]
fn test_prompt_generation() {
    let state = SessionState::new();
    let prompt = state.prompt();
    assert!(prompt.contains("iter:0"));
    assert!(prompt.contains("tools:0"));
    assert!(prompt.ends_with("> "));
}

#[test]
fn test_session_state_update() {
    let mut state = SessionState::new();

    // IterationStarted is no longer a wire event — the REPL derives
    // iteration count from session/agent lifecycle instead.

    state.update(&AgentEvent::Model(ContentPart::ToolUse(ToolUse {
        id: "call-test".to_string(),
        name: "test".to_string(),
        input: json!({}),
    })));
    assert_eq!(state.tool_call_count, 1);

    state.update(&AgentEvent::Model(ContentPart::ToolUse(ToolUse {
        id: "call-test2".to_string(),
        name: "test2".to_string(),
        input: json!({}),
    })));
    assert_eq!(state.tool_call_count, 2);
}

#[test]
fn test_theme_defaults() {
    let theme = Theme::default();
    assert_eq!(theme.tool_call_color, Color::Cyan);
    assert_eq!(theme.text_color, Color::White);
    assert_eq!(theme.error_color, Color::Red);
    assert_eq!(theme.success_color, Color::Green);
    assert_eq!(theme.prompt_color, Color::Yellow);
}

#[test]
fn test_theme_format_methods() {
    let theme = Theme::default();
    assert!(theme.format_tool_call("TOOL").contains("TOOL"));
    assert!(theme.format_error("error").contains("error"));
    assert!(theme.format_success("ok").contains("ok"));
    assert!(theme.format_prompt("prompt").contains("prompt"));
    assert!(theme.format_text("text").contains("text"));
}

#[test]
fn test_agent_mode_from_str() {
    assert_eq!(
        "interactive".parse::<AgentMode>(),
        Ok(AgentMode::Interactive)
    );
    assert_eq!("plan".parse::<AgentMode>(), Ok(AgentMode::Plan));
    assert_eq!("execute".parse::<AgentMode>(), Ok(AgentMode::Execute));
    assert_eq!("review".parse::<AgentMode>(), Ok(AgentMode::Review));
    assert_eq!("auto".parse::<AgentMode>(), Ok(AgentMode::Execute));
    assert_eq!("unknown".parse::<AgentMode>(), Err(()));
}

#[test]
fn test_agent_mode_display() {
    assert_eq!(AgentMode::Interactive.to_string(), "interactive");
    assert_eq!(AgentMode::Plan.to_string(), "plan");
    assert_eq!(AgentMode::Execute.to_string(), "execute");
    assert_eq!(AgentMode::Review.to_string(), "review");
}

#[test]
fn test_agent_mode_default() {
    assert_eq!(AgentMode::default(), AgentMode::Interactive);
}

#[test]
fn test_context_compacted_display() {
    let repl = Repl::new(PathBuf::from("/tmp"));
    let event = AgentEvent::System(SystemEvent::Warning {
        kind: WarningKind::ContextCompaction,
        message: "Context compacted 10000 -> 5000".to_string(),
        iteration: None,
    });
    let formatted = repl.format_event(&event);
    assert!(formatted.contains("ContextCompaction"));
    assert!(formatted.contains("5000"));
}
