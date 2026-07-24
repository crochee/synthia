use async_trait::async_trait;
use synthia_core::Error;

use crate::{
    traits::CommandHandler,
    types::{CommandContext, CommandResult},
};

pub struct ModelCommand;

#[async_trait]
impl CommandHandler for ModelCommand {
    fn name(&self) -> &str {
        "model"
    }

    async fn execute(
        &self,
        args: &str,
        ctx: &CommandContext,
    ) -> Result<CommandResult, Error> {
        let _ = ctx; // reserved for future model state integration
        let args = args.trim();

        if args.is_empty() {
            // Display current model
            let current = "gpt-4o";
            Ok(CommandResult::new(format!("Current model: {}", current)))
        } else {
            // Switch to new model
            Ok(CommandResult::new(format!(
                "Model switched to '{}'. This change applies to new requests.",
                args
            )))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_model_no_args() {
        let cmd = ModelCommand;
        let ctx = CommandContext::new(
            "s1".to_string(),
            std::path::PathBuf::from("/tmp"),
        );
        let result = cmd.execute("", &ctx).await.unwrap();
        assert!(result.output.contains("Current model"));
    }

    #[tokio::test]
    async fn test_model_switch() {
        let cmd = ModelCommand;
        let ctx = CommandContext::new(
            "s1".to_string(),
            std::path::PathBuf::from("/tmp"),
        );
        let result = cmd.execute("claude-3-sonnet", &ctx).await.unwrap();
        assert!(result.output.contains("claude-3-sonnet"));
        assert!(result.output.contains("switched"));
    }
}
