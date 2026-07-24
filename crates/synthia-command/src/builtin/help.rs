use async_trait::async_trait;
use synthia_core::Error;

use crate::{
    traits::CommandHandler,
    types::{CommandContext, CommandResult},
};

pub struct HelpCommand;

#[async_trait]
impl CommandHandler for HelpCommand {
    fn name(&self) -> &str {
        "help"
    }

    async fn execute(
        &self,
        args: &str,
        _ctx: &CommandContext,
    ) -> Result<CommandResult, Error> {
        let output = if args.is_empty() {
            "Available commands:\n\
                 /help [command]    - Show this help or details for a specific command\n\
                 /clear             - Clear the current session context\n\
                 /model [name]      - View or switch the active LLM model\n\
                 /session <action>  - Manage sessions (list/save/load/delete)\n\
                 /skill <action>    - Manage skills (list/enable/disable)\n\
                 /todo [action]     - Manage task tracking (list/add/complete)"
                .to_string()
        } else {
            match args.trim() {
                "help" => "Usage: /help [command]\nShow available commands or details for a specific command.".to_string(),
                "clear" => "Usage: /clear\nReset the current session context and clear message history.".to_string(),
                "model" => "Usage: /model [name]\nView the current model or switch to a new one.".to_string(),
                "session" => "Usage: /session <list|save|load|delete>\nManage session lifecycle.".to_string(),
                "skill" => "Usage: /skill <list|enable|disable>\nManage skills for the current session.".to_string(),
                "todo" => "Usage: /todo [list|add|complete]\nManage task tracking list.".to_string(),
                other => format!("Unknown command: {other}"),
            }
        };

        Ok(CommandResult::new(output))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_help_no_args() {
        let cmd = HelpCommand;
        let ctx = CommandContext::new(
            "s1".to_string(),
            std::path::PathBuf::from("/tmp"),
        );
        let result = cmd.execute("", &ctx).await.unwrap();
        assert!(result.output.contains("Available commands"));
        assert!(result.output.contains("/help"));
        assert!(result.output.contains("/clear"));
    }

    #[tokio::test]
    async fn test_help_with_known_command() {
        let cmd = HelpCommand;
        let ctx = CommandContext::new(
            "s1".to_string(),
            std::path::PathBuf::from("/tmp"),
        );
        let result = cmd.execute("clear", &ctx).await.unwrap();
        assert!(result.output.contains("/clear"));
    }

    #[tokio::test]
    async fn test_help_with_unknown_command() {
        let cmd = HelpCommand;
        let ctx = CommandContext::new(
            "s1".to_string(),
            std::path::PathBuf::from("/tmp"),
        );
        let result = cmd.execute("unknown", &ctx).await.unwrap();
        assert!(result.output.contains("Unknown command"));
    }
}
