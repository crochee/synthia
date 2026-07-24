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
use synthia_agent::{AgentEvent, SessionEndReason};

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
    let event = AgentEvent::SessionStarted {
        session_id: "test-session".to_string(),
    };
    assert_eq!(repl.format_event(&event), "Session started: test-session");
}

#[test]
fn test_format_event_iteration_started_silent() {
    let repl = Repl::new(PathBuf::from("/tmp"));
    let event = AgentEvent::IterationStarted { iteration: 1 };
    assert_eq!(repl.format_event(&event), "");
}

#[test]
fn test_format_event_llm_delta() {
    let repl = Repl::new(PathBuf::from("/tmp"));
    let event = AgentEvent::LlmStreamDelta {
        content: "Hello world".to_string(),
    };
    assert_eq!(repl.format_event(&event), "Hello world");
}

#[test]
fn test_format_event_tool_call_started() {
    let repl = Repl::new(PathBuf::from("/tmp"));
    let event = AgentEvent::ToolCallStarted {
        tool_name: "read_file".to_string(),
        input: serde_json::json!({ "path": "/tmp/test" }),
    };
    let formatted = strip_ansi(&repl.format_event(&event));
    assert!(formatted.contains("[TOOL: read_file]"));
}

#[test]
fn test_format_event_tool_call_completed() {
    let repl = Repl::new(PathBuf::from("/tmp"));
    let event = AgentEvent::ToolCallCompleted {
        tool_name: "read_file".to_string(),
        output: "File contents here".to_string(),
        is_error: false,
    };
    let formatted = strip_ansi(&repl.format_event(&event));
    assert!(formatted.contains("[TOOL: read_file]"));
    assert!(formatted.contains("File contents"));
}

#[test]
fn test_format_event_tool_call_error() {
    let repl = Repl::new(PathBuf::from("/tmp"));
    let event = AgentEvent::ToolCallError {
        tool_name: "search".to_string(),
        error: "not found".to_string(),
    };
    let formatted = strip_ansi(&repl.format_event(&event));
    assert!(formatted.contains("[TOOL: search]"));
    assert!(formatted.contains("not found"));
}

#[test]
fn test_format_event_token_budget_warning() {
    let repl = Repl::new(PathBuf::from("/tmp"));
    let event = AgentEvent::TokenBudgetWarning {
        status: "near limit".to_string(),
        current_tokens: 9000,
        threshold_tokens: 10000,
    };
    assert!(repl.format_event(&event).contains("Token budget warning"));
}

#[test]
fn test_format_event_token_budget_notice_silent() {
    let repl = Repl::new(PathBuf::from("/tmp"));
    let event = AgentEvent::TokenBudgetNotice {
        status: "ok".to_string(),
        current_tokens: 5000,
        threshold_tokens: 10000,
    };
    assert_eq!(repl.format_event(&event), "");
}

#[test]
fn test_format_event_edit_conflict() {
    let repl = Repl::new(PathBuf::from("/tmp"));
    let event = AgentEvent::EditConflict {
        tool_name: "write".to_string(),
        call_id: "call-123".to_string(),
        path: "src/main.rs".to_string(),
        original_content_hash: 43981,
        current_content_hash: 57072,
    };
    let formatted = repl.format_event(&event);
    let stripped = strip_ansi(&formatted);
    assert!(stripped.contains("Edit conflict"));
    assert!(stripped.contains("src/main.rs"));
    assert!(stripped.contains("write"));
    assert!(stripped.contains("43981"));
    assert!(stripped.contains("57072"));
}

#[test]
fn test_format_event_session_ended() {
    let repl = Repl::new(PathBuf::from("/tmp"));
    let event = AgentEvent::SessionEnded {
        reason: SessionEndReason::Completed,
    };
    assert!(repl.format_event(&event).contains("Session ended"));
}

#[test]
fn test_format_event_session_ended_error() {
    let repl = Repl::new(PathBuf::from("/tmp"));
    let event = AgentEvent::SessionEnded {
        reason: SessionEndReason::Error("connection failed".to_string()),
    };
    assert!(
        repl.format_event(&event)
            .contains("error: connection failed")
    );
}

#[test]
fn test_format_event_guardian_warning() {
    let repl = Repl::new(PathBuf::from("/tmp"));
    let event = AgentEvent::GuardianWarning {
        reason: "safety violation".to_string(),
        iteration: 3,
    };
    let formatted = strip_ansi(&repl.format_event(&event));
    assert!(formatted.contains("Guardian:"));
    assert!(formatted.contains("safety violation"));
}

#[test]
fn test_format_event_guardian_confirmation_request() {
    let repl = Repl::new(PathBuf::from("/tmp"));
    let event = AgentEvent::GuardianConfirmationRequest {
        tool_name: "shell_exec".to_string(),
        reason: "destructive operation".to_string(),
    };
    let formatted = strip_ansi(&repl.format_event(&event));
    assert!(formatted.contains("Guardian confirmation required"));
    assert!(formatted.contains("shell_exec"));
    assert!(formatted.contains("destructive operation"));
}

#[test]
fn test_format_event_tool_call_skipped() {
    let repl = Repl::new(PathBuf::from("/tmp"));
    let event = AgentEvent::ToolCallSkipped {
        tool_name: "write_file".to_string(),
        reason: "guardian policy".to_string(),
    };
    let formatted = strip_ansi(&repl.format_event(&event));
    assert!(formatted.contains("[TOOL: write_file]"));
    assert!(formatted.contains("skipped"));
    assert!(formatted.contains("guardian policy"));
}

#[test]
fn test_format_event_llm_error() {
    let repl = Repl::new(PathBuf::from("/tmp"));
    let event = AgentEvent::LlmError {
        error: "timeout".to_string(),
    };
    let formatted = strip_ansi(&repl.format_event(&event));
    assert_eq!(formatted, "LLM error: timeout");
}

#[test]
fn test_format_event_hook_error() {
    let repl = Repl::new(PathBuf::from("/tmp"));
    let event = AgentEvent::HookError {
        hook_name: "pre_request".to_string(),
        error: "failed to connect".to_string(),
        hook_type: "pre".to_string(),
    };
    let formatted = strip_ansi(&repl.format_event(&event));
    assert_eq!(formatted, "Hook error: pre_request: failed to connect");
}

#[test]
fn test_format_event_other_events_silent() {
    let repl = Repl::new(PathBuf::from("/tmp"));

    let events = vec![
        AgentEvent::Warning {
            message: "some warning".to_string(),
        },
        AgentEvent::Progress {
            message: "loading".to_string(),
            step: 1,
            total: 10,
        },
        AgentEvent::Finish {
            output: "done".to_string(),
        },
        AgentEvent::Checkpoint {
            session_id: "s1".to_string(),
            step: 5,
        },
        AgentEvent::StateChange {
            from: "idle".to_string(),
            to: "running".to_string(),
        },
    ];

    for event in events {
        assert_eq!(
            repl.format_event(&event),
            "",
            "Event {:?} should be silent",
            event
        );
    }
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
    let event = AgentEvent::ToolCallCompleted {
        tool_name: "search".to_string(),
        output: long_output.clone(),
        is_error: false,
    };
    let formatted = strip_ansi(&repl.format_event(&event));
    // Should be truncated to 60 chars
    assert!(formatted.len() < 120);
    assert!(formatted.contains("[TOOL: search]"));
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
    state.update(&AgentEvent::IterationStarted { iteration: 5 });
    assert_eq!(state.iteration_count, 5);

    state.update(&AgentEvent::ToolCallStarted {
        tool_name: "test".to_string(),
        input: serde_json::json!({}),
    });
    assert_eq!(state.tool_call_count, 1);

    state.update(&AgentEvent::ToolCallStarted {
        tool_name: "test2".to_string(),
        input: serde_json::json!({}),
    });
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
    let event = AgentEvent::ContextCompacted {
        old_tokens: 10000,
        new_tokens: 5000,
    };
    let formatted = repl.format_event(&event);
    assert!(formatted.contains("Context compacted"));
    assert!(formatted.contains("50"));
}
