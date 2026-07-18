//! Bridge implementations: legacy [`Tool`] → sub-traits.
//!
//! Provides blanket implementations so that any type implementing the
//! legacy [`crate::Tool`] trait automatically satisfies the three
//! sub-traits ([`ToolDefinition`], [`ToolExecution`], [`ToolLifecycle`]).
//!
//! This avoids having to modify every existing tool implementation —
//! they continue to implement `Tool` and get the sub-traits for free.

use async_trait::async_trait;

use crate::{
    sub_traits::{
        ToolCategory,
        ToolDefinition,
        ToolExecution,
        ToolLifecycle,
        ToolMetadataSnapshot,
    },
    traits::Tool,
    types::ToolInput,
};

/// Blanket `ToolDefinition` for any `Tool`.
///
/// Maps `Tool::name` → `ToolDefinition::name`, `Tool::description` →
/// `ToolDefinition::description`, `Tool::parameters` →
/// `ToolDefinition::parameters_schema`. Category defaults to
/// [`ToolCategory::Utility`].
impl<T: Tool + 'static> ToolDefinition for T {
    fn name(&self) -> &str {
        <Self as Tool>::name(self)
    }

    fn description(&self) -> &str {
        <Self as Tool>::description(self)
    }

    fn parameters_schema(&self) -> serde_json::Value {
        <Self as Tool>::parameters(self)
    }

    fn category(&self) -> ToolCategory {
        // Legacy tools don't declare a category. Default to Utility.
        // Tools that need a specific category should implement
        // ToolDefinition directly.
        ToolCategory::Utility
    }

    fn to_metadata(&self) -> ToolMetadataSnapshot {
        ToolMetadataSnapshot {
            name: self.name().to_string(),
            description: self.description().to_string(),
            category: self.category(),
            parameters_schema: self.parameters_schema(),
            version: format!("{}", self.version()),
        }
    }
}

/// Blanket `ToolExecution` for any `Tool`.
///
/// Maps `Tool::call` → `ToolExecution::execute`. Validation and dry-run
/// default to "always valid". Cost defaults to 0.
#[async_trait]
impl<T: Tool + 'static> ToolExecution for T {
    async fn execute(&self, input: ToolInput) -> crate::types::ToolOutput {
        <Self as Tool>::call(self, input).await
    }

    fn validate(&self, args: &serde_json::Value) -> Result<(), String> {
        // Legacy tools don't have a separate validation step.
        // Basic schema check: ensure args is an object.
        if !args.is_object() {
            return Err("tool arguments must be a JSON object".to_string());
        }
        Ok(())
    }
}

/// Blanket `ToolLifecycle` for any `Tool`.
///
/// All lifecycle hooks default to no-op. Version defaults to `"0.1.0"`.
impl<T: Tool + 'static> ToolLifecycle for T {}

#[cfg(test)]
mod tests {
    use async_trait::async_trait;

    use crate::{
        sub_traits::{
            ToolCategory,
            ToolDefinition,
            ToolExecution,
            ToolLifecycle,
        },
        traits::Tool,
        types::{ToolExecutionContext, ToolInput, ToolOutput},
    };

    /// Minimal test tool for verifying blanket implementations.
    struct TestTool;

    #[async_trait]
    impl Tool for TestTool {
        fn name(&self) -> &str {
            "test"
        }

        fn description(&self) -> &str {
            "A test tool"
        }

        fn parameters(&self) -> serde_json::Value {
            serde_json::json!({"type": "object"})
        }

        async fn call(&self, _input: ToolInput) -> ToolOutput {
            ToolOutput::text("ok")
        }
    }

    #[test]
    fn blanket_tool_definition() {
        let tool = TestTool;
        assert_eq!(ToolDefinition::name(&tool), "test");
        assert_eq!(ToolDefinition::description(&tool), "A test tool");
        assert!(ToolDefinition::parameters_schema(&tool).is_object());
        assert_eq!(ToolDefinition::category(&tool), ToolCategory::Utility);
        let meta = tool.to_metadata();
        assert_eq!(meta.name, "test");
    }

    #[tokio::test]
    async fn blanket_tool_execution() {
        let tool = TestTool;
        let input = ToolInput {
            name: "test".to_string(),
            input: serde_json::json!({}),
            context: ToolExecutionContext::new(
                "s1".to_string(),
                std::path::PathBuf::from("/tmp"),
            ),
        };
        let output = ToolExecution::execute(&tool, input).await;
        assert!(!output.is_error.unwrap_or(false));
    }

    #[test]
    fn blanket_tool_lifecycle() {
        let tool = TestTool;
        assert!(ToolLifecycle::on_register(&tool).is_ok());
        assert!(ToolLifecycle::on_unregister(&tool).is_ok());
        assert!(ToolLifecycle::health_check(&tool).is_ok());
        assert_eq!(tool.version(), semver::Version::new(0, 1, 0));
        assert_eq!(tool.schema_version(), 1);
    }

    /// Verify `ToolV1` supertrait is satisfied by any `Tool` impl.
    #[test]
    fn tool_v1_supertrait_satisfied() {
        fn assert_tool_v1<T: crate::ToolV1>() {}
        assert_tool_v1::<TestTool>();
    }
}
