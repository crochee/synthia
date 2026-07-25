//! `tool_search` built-in tool — discover Deferred tools at runtime.
//!
//! Allows the LLM to search for tools that were not initially visible
//! (i.e., `ToolExposure::Deferred`) using simple keyword matching.

use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::tool::{
    descriptor::{
        Tool,
        ToolCategory,
        ToolDescriptor,
        ToolExposure,
        ToolProvenance,
    },
    registry::ToolRegistry,
    tool_name::ToolName,
    types::{ToolContext, ToolError, ToolInput, ToolOutput},
};

// ── Input / Output types ──────────────────────────────────────────────────

/// Input schema for `tool_search`.
#[derive(Debug, Clone, Deserialize)]
struct ToolSearchInput {
    /// Search query — matched against tool names and descriptions.
    query: String,
}

/// A single matching tool result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolSearchResult {
    /// Full tool name.
    pub name: String,
    /// Tool description.
    pub description: String,
}

// ── ToolSearchProvider ─────────────────────────────────────────────────────

/// Built-in tool that searches for `Deferred` tools in the registry.
///
/// Only tools with `ToolExposure::Deferred` are returned — `Direct` tools
/// are already visible to the LLM and `Hidden` tools are intentionally
/// excluded.
pub struct ToolSearchProvider {
    registry: Arc<ToolRegistry>,
    descriptor: ToolDescriptor,
}

impl ToolSearchProvider {
    /// Create a new `tool_search` provider backed by the given registry.
    pub fn new(registry: Arc<ToolRegistry>) -> Self {
        Self {
            registry,
            descriptor: Self::build_descriptor(),
        }
    }

    fn build_descriptor() -> ToolDescriptor {
        ToolDescriptor {
            name: ToolName::plain("tool_search"),
            description: "Search for tools that are not yet visible. \
                Returns a list of matching tool names and descriptions \
                from deferred (on-demand) tools."
                .to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "Search query — keywords to match against tool names and descriptions"
                    }
                },
                "required": ["query"]
            }),
            category: ToolCategory::Search,
            provenance: ToolProvenance::Core,
            execution_mode: crate::tool::descriptor::ExecutionMode::Parallel,
            cancel_behavior:
                crate::tool::descriptor::CancelBehavior::Cooperative,
            examples: vec![],
            permission_required: false,
            prompt_visible_provenance: true,
            is_hidden: false,
            is_user_invocable: true,
            exposure: ToolExposure::Direct,
        }
    }
}

#[async_trait]
impl Tool for ToolSearchProvider {
    fn name(&self) -> &str {
        "tool_search"
    }

    async fn execute(
        &self,
        input: ToolInput,
        _ctx: &ToolContext,
    ) -> Result<ToolOutput, ToolError> {
        let parsed: ToolSearchInput = serde_json::from_value(input.raw)
            .map_err(|e| ToolError::InvalidInput(e.to_string()))?;

        if parsed.query.trim().is_empty() {
            return Err(ToolError::InvalidInput(
                "query must not be empty".to_string(),
            ));
        }

        let mat = self.registry.materialize();
        let descs = mat.tool_descriptors_for_llm();

        let query_lower = parsed.query.to_lowercase();
        let keywords: Vec<&str> = query_lower.split_whitespace().collect();

        let mut results: Vec<ToolSearchResult> = descs
            .into_iter()
            .filter(|desc| {
                // Only return deferred tools
                mat.exposure_of(&desc.name) == Some(ToolExposure::Deferred)
            })
            .filter(|desc| {
                // Keyword matching: at least one keyword must match name or
                // description (case-insensitive substring)
                let name_lower = desc.name.full_name().to_lowercase();
                let desc_lower = desc.description.to_lowercase();
                keywords.iter().any(|kw| {
                    name_lower.contains(kw) || desc_lower.contains(kw)
                })
            })
            .map(|desc| ToolSearchResult {
                name: desc.name.full_name(),
                description: desc.description,
            })
            .collect();

        // Sort for deterministic output
        results.sort_by(|a, b| a.name.cmp(&b.name));

        let output_text = if results.is_empty() {
            format!("No deferred tools found matching '{}'.", parsed.query)
        } else {
            let mut lines = Vec::with_capacity(results.len() + 1);
            lines.push(format!(
                "Found {} deferred tool(s) matching '{}':",
                results.len(),
                parsed.query
            ));
            for r in &results {
                lines.push(format!("- {}: {}", r.name, r.description));
            }
            lines.join("\n")
        };

        Ok(ToolOutput {
            content: vec![crate::tool::types::ContentPart::Text {
                text: output_text,
            }],
            structured: Some(
                serde_json::to_value(&results)
                    .unwrap_or(serde_json::Value::Null),
            ),
            metadata: Default::default(),
            is_error: false,
        })
    }

    fn descriptor(&self) -> &ToolDescriptor {
        &self.descriptor
    }
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::tool::{
        descriptor::{
            CancelBehavior,
            ExecutionMode,
            Tool as ToolTrait,
            ToolExposure,
        },
        registry::{RegistrationToken, ToolEntry, ToolIdentity},
        tool_name::ToolName,
        types::ToolInput,
    };

    /// Helper tool that can be configured with a specific exposure.
    struct StubTool {
        name: String,
        exposure: ToolExposure,
        descriptor: std::sync::OnceLock<ToolDescriptor>,
    }

    impl StubTool {
        fn new(name: &str, exposure: ToolExposure) -> Self {
            Self {
                name: name.to_string(),
                exposure,
                descriptor: std::sync::OnceLock::new(),
            }
        }
    }

    #[async_trait]
    impl ToolTrait for StubTool {
        fn name(&self) -> &str {
            &self.name
        }

        async fn execute(
            &self,
            _input: ToolInput,
            _ctx: &ToolContext,
        ) -> Result<ToolOutput, ToolError> {
            Ok(ToolOutput::default())
        }

        fn descriptor(&self) -> &ToolDescriptor {
            self.descriptor.get_or_init(|| ToolDescriptor {
                name: ToolName::plain(&self.name),
                description: format!("{} description", self.name),
                parameters: serde_json::json!({"type": "object", "properties": {"arg1": {"type": "string"}}}),
                category: ToolCategory::Utility,
                provenance: ToolProvenance::Core,
                execution_mode: ExecutionMode::Parallel,
                cancel_behavior: CancelBehavior::Cooperative,
                examples: vec![],
                permission_required: false,
                prompt_visible_provenance: true,
                is_hidden: false,
                is_user_invocable: true,
                exposure: self.exposure,
            })
        }
    }

    /// Insert a tool directly into the registry for testing.
    fn insert_tool(
        registry: &ToolRegistry,
        name: &str,
        exposure: ToolExposure,
    ) {
        let tool: Arc<dyn ToolTrait> = Arc::new(StubTool::new(name, exposure));
        let mut inner = registry.inner.write();
        let token = RegistrationToken(inner.next_registration);
        inner.next_registration += 1;
        let entry = ToolEntry {
            provider_id: "test".to_string(),
            provider_token: token,
            tool,
            identity: ToolIdentity {
                name: ToolName::plain(name),
                generation: inner.generation,
            },
            provenance: ToolProvenance::Core,
        };
        inner.tools.insert(ToolName::plain(name), vec![entry]);
        inner.generation.0 += 1;
    }

    fn make_input(query: &str) -> ToolInput {
        ToolInput {
            raw: serde_json::json!({ "query": query }),
            name: "tool_search".to_string(),
            session_id: "test-session".to_string(),
            workspace_root: std::path::PathBuf::from("/tmp"),
        }
    }

    fn make_ctx() -> ToolContext {
        ToolContext {
            capabilities: crate::tool::capability::ToolCapabilities::default(),
            session_id: "test-session".to_string(),
            workspace_root: std::path::PathBuf::from("/tmp"),
            caller_agent: "test".to_string(),
        }
    }

    #[test]
    fn tool_name_is_tool_search() {
        let registry = Arc::new(ToolRegistry::new());
        let tool = ToolSearchProvider::new(registry);
        assert_eq!(tool.name(), "tool_search");
    }

    #[test]
    fn descriptor_has_correct_category() {
        let registry = Arc::new(ToolRegistry::new());
        let tool = ToolSearchProvider::new(registry);
        assert_eq!(tool.descriptor().category, ToolCategory::Search);
    }

    #[test]
    fn descriptor_has_direct_exposure() {
        let registry = Arc::new(ToolRegistry::new());
        let tool = ToolSearchProvider::new(registry);
        assert_eq!(tool.descriptor().exposure, ToolExposure::Direct);
    }

    #[tokio::test]
    async fn finds_deferred_tool_by_name() {
        let registry = Arc::new(ToolRegistry::new());
        insert_tool(&registry, "database_query", ToolExposure::Deferred);
        insert_tool(&registry, "file_read", ToolExposure::Direct);

        let tool = ToolSearchProvider::new(Arc::clone(&registry));
        let result = tool
            .execute(make_input("database"), &make_ctx())
            .await
            .unwrap();

        assert!(!result.is_error);
        let structured = result.structured.unwrap();
        let results: Vec<ToolSearchResult> =
            serde_json::from_value(structured).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "database_query");
    }

    #[tokio::test]
    async fn finds_deferred_tool_by_description() {
        let registry = Arc::new(ToolRegistry::new());
        insert_tool(&registry, "my_tool", ToolExposure::Deferred);
        // The stub creates description "my_tool description"

        let tool = ToolSearchProvider::new(Arc::clone(&registry));
        let result = tool
            .execute(make_input("description"), &make_ctx())
            .await
            .unwrap();

        assert!(!result.is_error);
        let structured = result.structured.unwrap();
        let results: Vec<ToolSearchResult> =
            serde_json::from_value(structured).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "my_tool");
    }

    #[tokio::test]
    async fn excludes_direct_tools() {
        let registry = Arc::new(ToolRegistry::new());
        insert_tool(&registry, "bash", ToolExposure::Direct);
        insert_tool(&registry, "file_read", ToolExposure::Direct);

        let tool = ToolSearchProvider::new(Arc::clone(&registry));
        let result =
            tool.execute(make_input("bash"), &make_ctx()).await.unwrap();

        assert!(!result.is_error);
        let structured = result.structured.unwrap();
        let results: Vec<ToolSearchResult> =
            serde_json::from_value(structured).unwrap();
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn excludes_hidden_tools() {
        let registry = Arc::new(ToolRegistry::new());
        // Hidden tools won't even appear in materialization
        insert_tool(&registry, "internal_admin", ToolExposure::Hidden);

        let tool = ToolSearchProvider::new(Arc::clone(&registry));
        let result = tool
            .execute(make_input("admin"), &make_ctx())
            .await
            .unwrap();

        assert!(!result.is_error);
        let structured = result.structured.unwrap();
        let results: Vec<ToolSearchResult> =
            serde_json::from_value(structured).unwrap();
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn multiple_keywords_or_logic() {
        let registry = Arc::new(ToolRegistry::new());
        insert_tool(&registry, "database_query", ToolExposure::Deferred);
        insert_tool(&registry, "cache_invalidate", ToolExposure::Deferred);

        let tool = ToolSearchProvider::new(Arc::clone(&registry));
        let result = tool
            .execute(make_input("database cache"), &make_ctx())
            .await
            .unwrap();

        assert!(!result.is_error);
        let structured = result.structured.unwrap();
        let results: Vec<ToolSearchResult> =
            serde_json::from_value(structured).unwrap();
        assert_eq!(results.len(), 2);
    }

    #[tokio::test]
    async fn case_insensitive_search() {
        let registry = Arc::new(ToolRegistry::new());
        insert_tool(&registry, "Database_Query", ToolExposure::Deferred);

        let tool = ToolSearchProvider::new(Arc::clone(&registry));
        let result = tool
            .execute(make_input("DATABASE"), &make_ctx())
            .await
            .unwrap();

        assert!(!result.is_error);
        let structured = result.structured.unwrap();
        let results: Vec<ToolSearchResult> =
            serde_json::from_value(structured).unwrap();
        assert_eq!(results.len(), 1);
    }

    #[tokio::test]
    async fn no_results_returns_message() {
        let registry = Arc::new(ToolRegistry::new());
        let tool = ToolSearchProvider::new(registry);
        let result = tool
            .execute(make_input("nonexistent"), &make_ctx())
            .await
            .unwrap();

        assert!(!result.is_error);
        let text = match &result.content[0] {
            crate::tool::types::ContentPart::Text { text } => text.clone(),
            _ => panic!("expected text content"),
        };
        assert!(text.contains("No deferred tools found"));
    }

    #[tokio::test]
    async fn empty_query_returns_error() {
        let registry = Arc::new(ToolRegistry::new());
        let tool = ToolSearchProvider::new(registry);

        let result =
            tool.execute(make_input(""), &make_ctx()).await.unwrap_err();

        match result {
            ToolError::InvalidInput(msg) => {
                assert!(msg.contains("empty"));
            }
            other => panic!("expected InvalidInput, got: {other}"),
        }
    }

    #[tokio::test]
    async fn whitespace_only_query_returns_error() {
        let registry = Arc::new(ToolRegistry::new());
        let tool = ToolSearchProvider::new(registry);

        let result = tool
            .execute(make_input("   "), &make_ctx())
            .await
            .unwrap_err();

        match result {
            ToolError::InvalidInput(msg) => {
                assert!(msg.contains("empty"));
            }
            other => panic!("expected InvalidInput, got: {other}"),
        }
    }

    #[tokio::test]
    async fn invalid_json_input_returns_error() {
        let registry = Arc::new(ToolRegistry::new());
        let tool = ToolSearchProvider::new(registry);

        let input = ToolInput {
            raw: serde_json::json!("not an object"),
            name: "tool_search".to_string(),
            session_id: "test".to_string(),
            workspace_root: std::path::PathBuf::from("/tmp"),
        };

        let result = tool.execute(input, &make_ctx()).await.unwrap_err();
        match result {
            ToolError::InvalidInput(_) => {}
            other => panic!("expected InvalidInput, got: {other}"),
        }
    }

    #[tokio::test]
    async fn results_are_sorted_by_name() {
        let registry = Arc::new(ToolRegistry::new());
        insert_tool(&registry, "zzz_last", ToolExposure::Deferred);
        insert_tool(&registry, "aaa_first", ToolExposure::Deferred);
        insert_tool(&registry, "mmm_middle", ToolExposure::Deferred);

        let tool = ToolSearchProvider::new(Arc::clone(&registry));
        let result = tool
            .execute(make_input("description"), &make_ctx())
            .await
            .unwrap();

        let structured = result.structured.unwrap();
        let results: Vec<ToolSearchResult> =
            serde_json::from_value(structured).unwrap();
        assert_eq!(results.len(), 3);
        assert_eq!(results[0].name, "aaa_first");
        assert_eq!(results[1].name, "mmm_middle");
        assert_eq!(results[2].name, "zzz_last");
    }

    #[tokio::test]
    async fn structured_output_contains_results() {
        let registry = Arc::new(ToolRegistry::new());
        insert_tool(&registry, "my_tool", ToolExposure::Deferred);

        let tool = ToolSearchProvider::new(Arc::clone(&registry));
        let result = tool
            .execute(make_input("my_tool"), &make_ctx())
            .await
            .unwrap();

        let structured = result.structured.unwrap();
        let results: Vec<ToolSearchResult> =
            serde_json::from_value(structured).unwrap();
        assert_eq!(results[0].name, "my_tool");
        assert!(results[0].description.contains("my_tool"));
    }

    #[tokio::test]
    async fn text_output_formatted_correctly() {
        let registry = Arc::new(ToolRegistry::new());
        insert_tool(&registry, "database_query", ToolExposure::Deferred);

        let tool = ToolSearchProvider::new(Arc::clone(&registry));
        let result = tool
            .execute(make_input("database"), &make_ctx())
            .await
            .unwrap();

        let text = match &result.content[0] {
            crate::tool::types::ContentPart::Text { text } => text.clone(),
            _ => panic!("expected text content"),
        };
        assert!(text.contains("Found 1 deferred tool(s)"));
        assert!(text.contains("- database_query:"));
    }
}
