use std::path::PathBuf;

use crossterm::style::Color;
use regex::Regex;
use synthia_cli::{
    commands::{AgentMode, CliCommand},
    repl::Repl,
    theme::Theme,
};

/// Strip ANSI escape codes from a string for plain-text comparison.
fn strip_ansi(s: &str) -> String {
    let re = Regex::new(r"\x1b\[[0-9;]*m").unwrap();
    re.replace_all(s, "").to_string()
}

/// Integration tests for CLI interactive features (Task 10.17)

#[test]
fn test_cli_command_mode_parsing() {
    // Test /mode with no argument
    match CliCommand::parse("/mode") {
        CliCommand::Mode(None) => {}
        other => panic!("Expected Mode(None), got {:?}", other),
    }

    // Test /mode with each valid mode
    for mode in &["interactive", "plan", "execute", "review"] {
        match CliCommand::parse(&format!("/mode {}", mode)) {
            CliCommand::Mode(Some(m)) => assert_eq!(&m, mode),
            other => panic!("Expected Mode({}), got {:?}", mode, other),
        }
    }
}

#[test]
fn test_cli_command_status_parsing() {
    assert!(matches!(CliCommand::parse("/status"), CliCommand::Status));
}

#[test]
fn test_cli_command_compact_parsing() {
    assert!(matches!(CliCommand::parse("/compact"), CliCommand::Compact));
}

#[test]
fn test_cli_command_memory_set_parsing() {
    match CliCommand::parse("/memory set key=value") {
        CliCommand::Memory(Some(sub)) => {
            assert!(sub.starts_with("set "));
            assert_eq!(sub, "set key=value");
        }
        other => panic!("Expected Memory(set), got {:?}", other),
    }
}

#[test]
fn test_repl_handle_command_mode() {
    let repl = Repl::new(PathBuf::from("/tmp"));

    // Test mode command returns correct action
    match repl.handle_command("/mode") {
        synthia_cli::repl::CommandAction::Mode(None) => {}
        other => panic!("Expected Mode(None), got {:?}", other),
    }

    match repl.handle_command("/mode plan") {
        synthia_cli::repl::CommandAction::Mode(Some(m)) => {
            assert_eq!(m, "plan")
        }
        other => panic!("Expected Mode(plan), got {:?}", other),
    }
}

#[test]
fn test_repl_handle_command_status() {
    let repl = Repl::new(PathBuf::from("/tmp"));
    assert!(matches!(
        repl.handle_command("/status"),
        synthia_cli::repl::CommandAction::Status
    ));
}

#[test]
fn test_repl_handle_command_compact() {
    let repl = Repl::new(PathBuf::from("/tmp"));
    assert!(matches!(
        repl.handle_command("/compact"),
        synthia_cli::repl::CommandAction::Compact
    ));
}

#[test]
fn test_repl_format_event_with_themes() {
    let repl = Repl::new(PathBuf::from("/tmp"));

    // Test tool call events use theme colors
    use synthia_agent::AgentEvent;
    use synthia_provider::types::{
        ContentPart,
        TextContent,
        ToolResult,
        ToolUse,
    };

    let tool_start = AgentEvent::Model(ContentPart::ToolUse(ToolUse {
        id: "tu_1".into(),
        name: "search".into(),
        input: serde_json::json!({"query": "test"}),
    }));
    let formatted = strip_ansi(&repl.format_event(&tool_start));
    assert!(formatted.contains("[TOOL: search]"));

    let tool_complete =
        AgentEvent::Model(ContentPart::ToolResult(ToolResult {
            tool_use_id: "tu_1".into(),
            content: vec![ContentPart::Text(TextContent {
                text: "results here".into(),
                cache_control: None,
            })],
            structured_content: None,
            is_error: Some(false),
        }));
    let formatted = strip_ansi(&repl.format_event(&tool_complete));
    assert!(formatted.contains("[TOOL: search]"));
    assert!(formatted.contains("results"));
}

#[test]
fn test_theme_custom_colors() {
    let theme = Theme::new(
        Color::Magenta,
        Color::Blue,
        Color::Yellow,
        Color::Cyan,
        Color::Red,
    );

    assert_eq!(theme.tool_call_color, Color::Magenta);
    assert_eq!(theme.text_color, Color::Blue);
    assert_eq!(theme.error_color, Color::Yellow);
    assert_eq!(theme.success_color, Color::Cyan);
    assert_eq!(theme.prompt_color, Color::Red);
}

#[test]
fn test_syntax_highlighting_rust_code() {
    let theme = Theme::default();
    let code = "fn main() {\n    let x = \"hello\";\n    // comment\n}";
    let highlighted = synthia_cli::repl::highlight_rust_code(code, &theme);

    // Verify code is preserved
    assert!(highlighted.contains("fn"));
    assert!(highlighted.contains("main"));
    assert!(highlighted.contains("let"));
    assert!(highlighted.contains("hello"));
}

#[test]
fn test_code_block_formatting() {
    let theme = Theme::default();
    let text = "Here is code:\n```rust\nfn test() {}\n```\nDone.";
    let formatted = strip_ansi(
        &synthia_cli::repl::format_with_syntax_highlighting(text, &theme),
    );

    assert!(formatted.contains("```rust"));
    assert!(formatted.contains("fn"));
    assert!(formatted.contains("test"));
    assert!(formatted.contains("Here is code:"));
    assert!(formatted.contains("Done."));
}

#[test]
fn test_agent_mode_variants() {
    // Test all mode variants
    assert_eq!(AgentMode::Interactive.to_string(), "interactive");
    assert_eq!(AgentMode::Plan.to_string(), "plan");
    assert_eq!(AgentMode::Execute.to_string(), "execute");
    assert_eq!(AgentMode::Review.to_string(), "review");

    // Test from_str
    assert_eq!(
        "interactive".parse::<AgentMode>(),
        Ok(AgentMode::Interactive)
    );
    assert_eq!("plan".parse::<AgentMode>(), Ok(AgentMode::Plan));
    assert_eq!("execute".parse::<AgentMode>(), Ok(AgentMode::Execute));
    assert_eq!("review".parse::<AgentMode>(), Ok(AgentMode::Review));
    assert_eq!("auto".parse::<AgentMode>(), Ok(AgentMode::Execute));
    assert_eq!("default".parse::<AgentMode>(), Ok(AgentMode::Interactive));
    assert_eq!("invalid".parse::<AgentMode>(), Err(()));
}

#[test]
fn test_session_prompt_format() {
    use synthia_cli::repl::SessionState;
    let state = SessionState::new();
    let prompt = state.prompt();

    assert!(prompt.contains("iter:"));
    assert!(prompt.contains("tools:"));
    assert!(prompt.ends_with("> "));
}

#[test]
fn test_repl_creation() {
    let repl = Repl::new(PathBuf::from("/tmp/test"));
    assert_eq!(repl.workspace_root, PathBuf::from("/tmp/test"));
}

#[test]
fn test_cli_command_unknown() {
    match CliCommand::parse("/unknowncommand") {
        CliCommand::Unknown(cmd) => assert_eq!(cmd, "unknowncommand"),
        other => panic!("Expected Unknown, got {:?}", other),
    }
}

#[test]
fn test_cli_command_message() {
    match CliCommand::parse("hello world") {
        CliCommand::Message(msg) => assert_eq!(msg, "hello world"),
        other => panic!("Expected Message, got {:?}", other),
    }
}

#[test]
fn test_cli_command_shortcuts() {
    assert!(matches!(CliCommand::parse("/q"), CliCommand::Quit));
    assert!(matches!(CliCommand::parse("/h"), CliCommand::Help));
    assert!(matches!(CliCommand::parse("/m"), CliCommand::Model(None)));
    assert!(matches!(
        CliCommand::parse("/p"),
        CliCommand::Provider(None)
    ));
    assert!(matches!(CliCommand::parse("/t"), CliCommand::Tools));
}
