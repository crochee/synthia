use async_trait::async_trait;
use synthia_core::Error;

use crate::{
    traits::CommandHandler,
    types::{CommandContext, CommandResult},
};

pub struct ClearCommand;

#[async_trait]
impl CommandHandler for ClearCommand {
    fn name(&self) -> &str {
        "clear"
    }

    async fn execute(
        &self,
        _args: &str,
        _ctx: &CommandContext,
    ) -> Result<CommandResult, Error> {
        Ok(CommandResult::new(
            "Session context cleared. Starting a new conversation.",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_clear() {
        let cmd = ClearCommand;
        let ctx = CommandContext::new(
            "s1".to_string(),
            std::path::PathBuf::from("/tmp"),
        );
        let result = cmd.execute("", &ctx).await.unwrap();
        assert!(result.output.contains("cleared"));
    }
}
