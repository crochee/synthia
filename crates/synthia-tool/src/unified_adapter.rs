//! Adapters wrapping legacy [`Tool`](crate::traits::Tool) implementations
//! into the new unified [`synthia_core::tool::descriptor::Tool`] trait.
//!
//! Feature-gated behind `unified-registry`.

use std::sync::{Arc, OnceLock};

use async_trait::async_trait;
use synthia_core::tool::{
    descriptor::{
        CancelBehavior,
        ExecutionMode,
        Tool,
        ToolCategory,
        ToolDescriptor,
        ToolProvenance,
    },
    types::{ContentPart, ToolContext, ToolError, ToolInput, ToolOutput},
};

/// Generic adapter: wraps any `dyn legacy_Tool` into the new
/// `synthia_core::tool::descriptor::Tool` trait.
pub struct LegacyToolAdapter {
    inner: Arc<dyn crate::traits::Tool>,
    category: ToolCategory,
    provenance: ToolProvenance,
    descriptor: OnceLock<ToolDescriptor>,
}

impl LegacyToolAdapter {
    /// Create an adapter for a core built-in tool.
    pub fn core(tool: Arc<dyn crate::traits::Tool>) -> Self {
        Self {
            inner: tool,
            category: ToolCategory::Utility,
            provenance: ToolProvenance::Core,
            descriptor: OnceLock::new(),
        }
    }

    /// Create an adapter with explicit category and provenance.
    pub fn with_category(
        tool: Arc<dyn crate::traits::Tool>,
        category: ToolCategory,
        provenance: ToolProvenance,
    ) -> Self {
        Self {
            inner: tool,
            category,
            provenance,
            descriptor: OnceLock::new(),
        }
    }

    fn build_descriptor(&self) -> ToolDescriptor {
        let inner = self.inner.as_ref();
        ToolDescriptor {
            name: inner.name().to_string(),
            description: inner.description().to_string(),
            parameters: inner.parameters(),
            category: self.category,
            provenance: self.provenance.clone(),
            execution_mode: if inner.is_concurrency_safe() {
                ExecutionMode::Parallel
            } else {
                ExecutionMode::Sequential
            },
            cancel_behavior: CancelBehavior::Cooperative,
            examples: vec![],
            permission_required: inner.requires_permission(),
            prompt_visible_provenance: true,
            is_hidden: inner.is_hidden(),
            is_user_invocable: inner.is_user_invocable(),
        }
    }
}

#[async_trait]
impl Tool for LegacyToolAdapter {
    fn name(&self) -> &str {
        self.inner.name()
    }

    async fn execute(
        &self,
        input: ToolInput,
        ctx: &ToolContext,
    ) -> Result<ToolOutput, ToolError> {
        let legacy_input = crate::types::ToolInput {
            name: input.name.clone(),
            input: input.raw,
            context: crate::types::ToolExecutionContext::new(
                ctx.session_id.clone(),
                ctx.workspace_root.clone(),
            ),
        };

        let legacy_output = self.inner.call(legacy_input).await;

        // Convert legacy output to new output
        let is_error = legacy_output.is_error.unwrap_or(false);
        let content: Vec<ContentPart> = legacy_output
            .content
            .into_iter()
            .map(|part| match part {
                synthia_provider::types::ContentPart::Text(t) => {
                    ContentPart::Text { text: t.text }
                }
                synthia_provider::types::ContentPart::Image(img) => {
                    ContentPart::Image {
                        url: img.data,
                        mime_type: img.mime_type,
                    }
                }
                // Other content types are dropped in the
                // unified adapter — they are not relevant
                // to tool output conversion.
                _ => ContentPart::Text {
                    text: String::new(),
                },
            })
            .collect();

        Ok(ToolOutput {
            content,
            structured: None,
            metadata: Default::default(),
            is_error,
        })
    }

    fn descriptor(&self) -> &ToolDescriptor {
        self.descriptor.get_or_init(|| self.build_descriptor())
    }
}

// ── Convenience constructors for built-in tools ──────────

/// Create a `LegacyToolAdapter` for a filesystem read tool.
pub fn adapt_read(tool: Arc<dyn crate::traits::Tool>) -> LegacyToolAdapter {
    LegacyToolAdapter::with_category(
        tool,
        ToolCategory::Filesystem,
        ToolProvenance::Core,
    )
}

/// Create a `LegacyToolAdapter` for a filesystem write tool.
pub fn adapt_write(tool: Arc<dyn crate::traits::Tool>) -> LegacyToolAdapter {
    LegacyToolAdapter::with_category(
        tool,
        ToolCategory::Filesystem,
        ToolProvenance::Core,
    )
}

/// Create a `LegacyToolAdapter` for a search/grep tool.
pub fn adapt_grep(tool: Arc<dyn crate::traits::Tool>) -> LegacyToolAdapter {
    LegacyToolAdapter::with_category(
        tool,
        ToolCategory::Search,
        ToolProvenance::Core,
    )
}

/// Create a `LegacyToolAdapter` for an edit tool.
pub fn adapt_edit(tool: Arc<dyn crate::traits::Tool>) -> LegacyToolAdapter {
    LegacyToolAdapter::with_category(
        tool,
        ToolCategory::Edit,
        ToolProvenance::Core,
    )
}

/// Create a `LegacyToolAdapter` for a shell/bash tool.
pub fn adapt_shell(tool: Arc<dyn crate::traits::Tool>) -> LegacyToolAdapter {
    LegacyToolAdapter::with_category(
        tool,
        ToolCategory::Shell,
        ToolProvenance::Core,
    )
}

/// Create a `LegacyToolAdapter` for a utility tool (glob, list, etc).
pub fn adapt_utility(tool: Arc<dyn crate::traits::Tool>) -> LegacyToolAdapter {
    LegacyToolAdapter::with_category(
        tool,
        ToolCategory::Utility,
        ToolProvenance::Core,
    )
}

#[cfg(test)]
mod tests {
    use synthia_core::tool::capability::ToolCapabilities;

    use super::*;

    #[test]
    fn adapter_builds_descriptor() {
        let read_tool = Arc::new(crate::builtin::ReadTool::default());
        let adapter = adapt_read(read_tool);
        let desc = adapter.descriptor();
        assert_eq!(desc.name, "read");
        assert_eq!(desc.category, ToolCategory::Filesystem);
        assert_eq!(desc.provenance, ToolProvenance::Core);
        assert!(desc.is_user_invocable);
        assert!(!desc.is_hidden);
    }

    #[tokio::test]
    async fn adapter_execute_converts_output() {
        let read_tool = Arc::new(crate::builtin::ReadTool::default());
        let adapter = adapt_read(read_tool);
        // The execute should succeed even with invalid input
        // (ReadTool handles errors gracefully)
        let input = ToolInput {
            raw: serde_json::json!({
                "file_path": "/nonexistent/path"
            }),
            name: "read".to_string(),
            session_id: "test".to_string(),
            workspace_root: std::path::PathBuf::from("/tmp"),
        };
        let ctx = ToolContext {
            capabilities: ToolCapabilities::default(),
            session_id: "test".to_string(),
            workspace_root: std::path::PathBuf::from("/tmp"),
            caller_agent: "test".to_string(),
        };
        let result = adapter.execute(input, &ctx).await;
        // Should return Ok (tool handles errors internally)
        assert!(result.is_ok());
    }
}
