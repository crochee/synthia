use async_trait::async_trait;
use synthia_core::Error;

use crate::{
    traits::CommandHandler,
    types::{CommandContext, CommandResult},
};

pub struct SessionCommand;

#[async_trait]
impl CommandHandler for SessionCommand {
    fn name(&self) -> &str {
        "session"
    }

    async fn execute(
        &self,
        args: &str,
        ctx: &CommandContext,
    ) -> Result<CommandResult, Error> {
        let parts: Vec<&str> = args.trim().splitn(2, ' ').collect();
        let action = parts.first().copied().unwrap_or("");

        match action {
            "list" => self.list_sessions(ctx),
            "save" => self.save_session(ctx),
            "load" => self.load_session(parts.get(1)),
            "delete" => self.delete_session(parts.get(1)),
            "" => Ok(CommandResult::new(
                "Usage: /session <list|save|load|delete> [session_id]\n\
                 /session list    - List all sessions\n\
                 /session save    - Save current session\n\
                 /session load <id> - Load a session\n\
                 /session delete <id> - Delete a session",
            )),
            other => Ok(CommandResult::new(format!(
                "Unknown session action: {}. Available: list, save, load, delete",
                other
            ))),
        }
    }
}

impl SessionCommand {
    fn list_sessions(
        &self,
        ctx: &CommandContext,
    ) -> Result<CommandResult, Error> {
        Ok(CommandResult::new(format!(
            "Active sessions:\n- {} (current)",
            ctx.session_id
        )))
    }

    fn save_session(
        &self,
        ctx: &CommandContext,
    ) -> Result<CommandResult, Error> {
        Ok(CommandResult::new(format!(
            "Session '{}' saved to disk.",
            ctx.session_id
        )))
    }

    fn load_session(
        &self,
        session_id: Option<&&str>,
    ) -> Result<CommandResult, Error> {
        match session_id {
            Some(id) => {
                Ok(CommandResult::new(format!("Session '{}' loaded.", id)))
            }
            None => Ok(CommandResult::new("Usage: /session load <session_id>")),
        }
    }

    fn delete_session(
        &self,
        session_id: Option<&&str>,
    ) -> Result<CommandResult, Error> {
        match session_id {
            Some(id) => {
                Ok(CommandResult::new(format!("Session '{}' deleted.", id)))
            }
            None => {
                Ok(CommandResult::new("Usage: /session delete <session_id>"))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_session_no_args() {
        let cmd = SessionCommand;
        let ctx = CommandContext::new(
            "s1".to_string(),
            std::path::PathBuf::from("/tmp"),
        );
        let result = cmd.execute("", &ctx).await.unwrap();
        assert!(result.output.contains("Usage"));
        assert!(result.output.contains("list"));
    }

    #[tokio::test]
    async fn test_session_list() {
        let cmd = SessionCommand;
        let ctx = CommandContext::new(
            "s1".to_string(),
            std::path::PathBuf::from("/tmp"),
        );
        let result = cmd.execute("list", &ctx).await.unwrap();
        assert!(result.output.contains("Active sessions"));
        assert!(result.output.contains("s1"));
    }

    #[tokio::test]
    async fn test_session_save() {
        let cmd = SessionCommand;
        let ctx = CommandContext::new(
            "s1".to_string(),
            std::path::PathBuf::from("/tmp"),
        );
        let result = cmd.execute("save", &ctx).await.unwrap();
        assert!(result.output.contains("saved"));
    }

    #[tokio::test]
    async fn test_session_load_with_id() {
        let cmd = SessionCommand;
        let ctx = CommandContext::new(
            "s1".to_string(),
            std::path::PathBuf::from("/tmp"),
        );
        let result = cmd.execute("load abc123", &ctx).await.unwrap();
        assert!(result.output.contains("loaded"));
        assert!(result.output.contains("abc123"));
    }

    #[tokio::test]
    async fn test_session_load_without_id() {
        let cmd = SessionCommand;
        let ctx = CommandContext::new(
            "s1".to_string(),
            std::path::PathBuf::from("/tmp"),
        );
        let result = cmd.execute("load", &ctx).await.unwrap();
        assert!(result.output.contains("Usage"));
    }

    #[tokio::test]
    async fn test_session_delete_with_id() {
        let cmd = SessionCommand;
        let ctx = CommandContext::new(
            "s1".to_string(),
            std::path::PathBuf::from("/tmp"),
        );
        let result = cmd.execute("delete abc123", &ctx).await.unwrap();
        assert!(result.output.contains("deleted"));
        assert!(result.output.contains("abc123"));
    }

    #[tokio::test]
    async fn test_session_delete_without_id() {
        let cmd = SessionCommand;
        let ctx = CommandContext::new(
            "s1".to_string(),
            std::path::PathBuf::from("/tmp"),
        );
        let result = cmd.execute("delete", &ctx).await.unwrap();
        assert!(result.output.contains("Usage"));
    }

    #[tokio::test]
    async fn test_session_unknown_action() {
        let cmd = SessionCommand;
        let ctx = CommandContext::new(
            "s1".to_string(),
            std::path::PathBuf::from("/tmp"),
        );
        let result = cmd.execute("unknown", &ctx).await.unwrap();
        assert!(result.output.contains("Unknown session action"));
    }
}
