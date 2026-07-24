use std::{path::Path, sync::Arc};

use synthia_core::registry::Registry;

use super::*;
use crate::{CommandContext, CommandHandler, CommandResult, Error};

struct HelpCommand;

#[async_trait::async_trait]
impl CommandHandler for HelpCommand {
    fn name(&self) -> &str {
        "help"
    }

    async fn execute(
        &self,
        _args: &str,
        _ctx: &CommandContext,
    ) -> Result<CommandResult, Error> {
        Ok(CommandResult::new("Available commands: help, clear"))
    }
}

#[tokio::test]
async fn test_dispatch_help() {
    let registry = CommandRegistry::new();
    registry.register_handler(Arc::new(HelpCommand));
    let ctx =
        CommandContext::new("s1".to_string(), std::path::PathBuf::from("/tmp"));
    let result = registry.dispatch("/help", &ctx).await.unwrap();
    assert!(result.is_some());
    assert!(result.unwrap().output.contains("Available commands"));
}

#[tokio::test]
async fn test_dispatch_unknown() {
    let registry = CommandRegistry::new();
    let ctx =
        CommandContext::new("s1".to_string(), std::path::PathBuf::from("/tmp"));
    let result = registry.dispatch("/unknown", &ctx).await.unwrap();
    assert!(result.is_none());
}

#[tokio::test]
async fn test_register_builtins() {
    let registry = CommandRegistry::new();
    registry.register_builtins();
    assert!(registry.len() >= 6);
    let names: Vec<String> = registry
        .list(None)
        .await
        .unwrap()
        .into_iter()
        .map(|d| d.name)
        .collect();
    assert!(names.contains(&"help".to_string()));
    assert!(names.contains(&"clear".to_string()));
    assert!(names.contains(&"model".to_string()));
    assert!(names.contains(&"session".to_string()));
    assert!(names.contains(&"skill".to_string()));
    assert!(names.contains(&"todo".to_string()));
}

#[test]
fn test_load_user_commands_from_nonexistent_dir() {
    let registry = CommandRegistry::new();
    registry.load_user_commands(Path::new("/nonexistent/path"));
    assert!(registry.is_empty());
}

#[tokio::test]
async fn test_load_user_commands_from_valid_dir() {
    use std::fs;
    let dir = tempfile::tempdir().unwrap();
    let commands_dir = dir.path().join(".agents").join("commands");
    fs::create_dir_all(&commands_dir).unwrap();

    fs::write(
        commands_dir.join("greet.toml"),
        r#"name = "greet"
description = "Greet the user"
body = "Hello, user!"
"#,
    )
    .unwrap();

    fs::write(
        commands_dir.join("hello.md"),
        r#"---
name: hello
description: Say hello
---
# Hello

This is the body content.
"#,
    )
    .unwrap();

    let registry = CommandRegistry::new();
    registry.load_user_commands(dir.path());
    assert_eq!(registry.len(), 2);
    let names: Vec<String> = registry
        .list(None)
        .await
        .unwrap()
        .into_iter()
        .map(|d| d.name)
        .collect();
    assert!(names.contains(&"greet".to_string()));
    assert!(names.contains(&"hello".to_string()));
}

#[tokio::test]
async fn test_load_markdown_command() {
    use std::fs;
    let dir = tempfile::tempdir().unwrap();
    let commands_dir = dir.path().join(".agents").join("commands");
    fs::create_dir_all(&commands_dir).unwrap();

    fs::write(
        commands_dir.join("markdown_cmd.md"),
        r#"---
name: markdown_cmd
description: A markdown command
---
This is the body of the command.
"#,
    )
    .unwrap();

    let registry = CommandRegistry::new();
    registry.load_user_commands(dir.path());
    assert_eq!(registry.len(), 1);
    let names: Vec<String> = registry
        .list(None)
        .await
        .unwrap()
        .into_iter()
        .map(|d| d.name)
        .collect();
    assert!(names.contains(&"markdown_cmd".to_string()));
}

#[tokio::test]
async fn test_load_commands_from_directory() {
    use std::fs;
    let dir = tempfile::tempdir().unwrap();
    let commands_dir = dir.path().join(".agents").join("commands");
    fs::create_dir_all(&commands_dir).unwrap();

    fs::write(
        commands_dir.join("test.toml"),
        r#"name = "test"
description = "Test command"
body = "Test body"
"#,
    )
    .unwrap();

    let commands = load_commands_from_directory(&commands_dir);
    assert_eq!(commands.len(), 1);
    assert_eq!(commands[0].name, "test");
    let result = commands[0]
        .execute(
            "",
            &CommandContext::new(
                "s1".to_string(),
                std::path::PathBuf::from("/tmp"),
            ),
        )
        .await
        .unwrap();
    assert!(result.output.contains("test"));
    assert!(result.output.contains("Test body"));
}
