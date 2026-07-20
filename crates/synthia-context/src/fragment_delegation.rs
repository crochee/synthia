//! FragmentRegistry delegation layer — replaces ContextAssembler for
//! Registry-First architecture.

use synthia_core::tool::{FragmentContext, FragmentError, FragmentRegistry};

/// Build a system prompt from FragmentRegistry, replacing ContextAssembler.
///
/// This is the new path for the Registry-First architecture.
pub async fn build_system_prompt_from_fragments(
    registry: &FragmentRegistry,
    ctx: &FragmentContext,
) -> Result<String, FragmentError> {
    let rendered = registry.render_active(ctx).await;
    if rendered.is_empty() {
        Ok(String::new())
    } else {
        Ok(rendered
            .iter()
            .map(|(_, content)| content.as_str())
            .collect::<Vec<_>>()
            .join("\n\n"))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use async_trait::async_trait;
    use synthia_core::tool::{
        ContextFragment,
        FragmentContext,
        FragmentError,
        FragmentRegistry,
    };

    use super::build_system_prompt_from_fragments;

    /// A simple fragment for testing.
    struct StubFragment {
        name: String,
        priority: u32,
        active: bool,
        content: String,
    }

    impl StubFragment {
        fn new(name: &str, priority: u32, active: bool, content: &str) -> Self {
            Self {
                name: name.to_string(),
                priority,
                active,
                content: content.to_string(),
            }
        }
    }

    #[async_trait]
    impl ContextFragment for StubFragment {
        fn name(&self) -> &str {
            &self.name
        }

        fn priority(&self) -> u32 {
            self.priority
        }

        fn is_active(&self) -> bool {
            self.active
        }

        async fn render(
            &self,
            _ctx: &FragmentContext,
        ) -> Result<String, FragmentError> {
            Ok(self.content.clone())
        }
    }

    fn ctx() -> FragmentContext {
        FragmentContext::new("test-session", "test-user")
    }

    #[tokio::test]
    async fn empty_registry_returns_empty_string() {
        let registry = FragmentRegistry::new();
        let result = build_system_prompt_from_fragments(&registry, &ctx())
            .await
            .unwrap();
        assert!(result.is_empty());
    }

    #[tokio::test]
    async fn single_fragment_returns_its_content() {
        let registry = FragmentRegistry::new();
        registry
            .register(Arc::new(StubFragment::new(
                "sys",
                0,
                true,
                "system prompt",
            )))
            .await
            .unwrap();

        let result = build_system_prompt_from_fragments(&registry, &ctx())
            .await
            .unwrap();
        assert_eq!(result, "system prompt");
    }

    #[tokio::test]
    async fn multiple_fragments_joined_by_double_newline() {
        let registry = FragmentRegistry::new();
        registry
            .register(Arc::new(StubFragment::new(
                "high",
                0,
                true,
                "high-priority",
            )))
            .await
            .unwrap();
        registry
            .register(Arc::new(StubFragment::new(
                "low",
                100,
                true,
                "low-priority",
            )))
            .await
            .unwrap();

        let result = build_system_prompt_from_fragments(&registry, &ctx())
            .await
            .unwrap();
        assert_eq!(result, "high-priority\n\nlow-priority");
    }

    #[tokio::test]
    async fn inactive_fragments_are_excluded() {
        let registry = FragmentRegistry::new();
        registry
            .register(Arc::new(StubFragment::new("on", 0, true, "visible")))
            .await
            .unwrap();
        registry
            .register(Arc::new(StubFragment::new("off", 1, false, "hidden")))
            .await
            .unwrap();

        let result = build_system_prompt_from_fragments(&registry, &ctx())
            .await
            .unwrap();
        assert_eq!(result, "visible");
    }
}
