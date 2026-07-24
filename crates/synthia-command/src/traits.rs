use async_trait::async_trait;
use synthia_core::Error;

use crate::types::*;

#[async_trait]
pub trait CommandHandler: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str {
        ""
    }
    async fn execute(
        &self,
        args: &str,
        ctx: &CommandContext,
    ) -> Result<CommandResult, Error>;
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestCommand;

    #[async_trait]
    impl CommandHandler for TestCommand {
        fn name(&self) -> &str {
            "test"
        }

        async fn execute(
            &self,
            _args: &str,
            _ctx: &CommandContext,
        ) -> Result<CommandResult, Error> {
            Ok(CommandResult::new("test output"))
        }
    }

    #[tokio::test]
    async fn test_command_handler() {
        let handler = TestCommand;
        assert_eq!(handler.name(), "test");
        let ctx = CommandContext::new(
            "s1".to_string(),
            std::path::PathBuf::from("/tmp"),
        );
        let result = handler.execute("", &ctx).await.unwrap();
        assert_eq!(result.output, "test output");
    }
}
