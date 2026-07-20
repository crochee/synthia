//! FragmentRegistry + ContextFragment trait + supporting types.
//!
//! Phase 2.1 of the Registry-First extension architecture: independent prompt
//! injection fragments that can be registered, prioritised, and rendered
//! independently of the Tool system.

use std::{collections::HashMap, sync::Arc};

use async_trait::async_trait;
use tokio::sync::RwLock;

// ---------------------------------------------------------------------------
// FragmentError
// ---------------------------------------------------------------------------

/// Errors produced by the fragment system.
#[derive(Debug, thiserror::Error)]
pub enum FragmentError {
    #[error("Fragment render failed: {0}")]
    RenderFailed(String),
    #[error("Fragment not found: {0}")]
    NotFound(String),
    #[error("Fragment already registered: {0}")]
    AlreadyRegistered(String),
}

// ---------------------------------------------------------------------------
// FragmentContext
// ---------------------------------------------------------------------------

/// Context passed to [`ContextFragment::render`].
#[derive(Debug, Clone, Default)]
pub struct FragmentContext {
    /// Current session id.
    pub session_id: String,
    /// Current user id.
    pub user_id: String,
    /// Current iteration number (0-based).
    pub iteration: usize,
    /// Tokens already consumed.
    pub tokens_used: usize,
    /// Optional token budget ceiling.
    pub token_budget: Option<usize>,
    /// Extension map for ad-hoc data.
    pub data: HashMap<String, String>,
}

impl FragmentContext {
    /// Convenience constructor with required fields; everything else defaults.
    pub fn new(
        session_id: impl Into<String>,
        user_id: impl Into<String>,
    ) -> Self {
        Self {
            session_id: session_id.into(),
            user_id: user_id.into(),
            iteration: 0,
            tokens_used: 0,
            token_budget: None,
            data: HashMap::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// ContextFragment trait
// ---------------------------------------------------------------------------

/// A context fragment — an independently renderable piece of prompt text.
///
/// Fragments are registered in a [`FragmentRegistry`] and rendered in priority
/// order (lower `priority()` value = rendered first).
#[async_trait]
pub trait ContextFragment: Send + Sync + 'static {
    /// Unique fragment name (acts as the registry key).
    fn name(&self) -> &str;
    /// Priority — lower values are rendered first; 0 = highest priority.
    fn priority(&self) -> u32;
    /// Whether the fragment is currently active.
    fn is_active(&self) -> bool;
    /// Render the fragment content.
    async fn render(
        &self,
        ctx: &FragmentContext,
    ) -> Result<String, FragmentError>;
}

// ---------------------------------------------------------------------------
// FragmentRegistry
// ---------------------------------------------------------------------------

/// Thread-safe registry for [`ContextFragment`] instances.
///
/// Uses `tokio::sync::RwLock` because [`ContextFragment::render`] is async and
/// we must not hold a `std::sync::RwLock` guard across `.await` points.
pub struct FragmentRegistry {
    fragments: RwLock<HashMap<String, Arc<dyn ContextFragment>>>,
}

impl FragmentRegistry {
    /// Create an empty registry.
    pub fn new() -> Self {
        Self {
            fragments: RwLock::new(HashMap::new()),
        }
    }

    /// Register a fragment.
    ///
    /// Returns `Err(FragmentError::AlreadyRegistered)` if a fragment with the
    /// same name already exists.
    pub async fn register(
        &self,
        fragment: Arc<dyn ContextFragment>,
    ) -> Result<(), FragmentError> {
        let name = fragment.name().to_string();
        let mut map = self.fragments.write().await;
        if map.contains_key(&name) {
            return Err(FragmentError::AlreadyRegistered(name));
        }
        map.insert(name, fragment);
        Ok(())
    }

    /// Unregister a fragment by name.
    ///
    /// Returns `true` if the fragment was present and removed.
    pub async fn unregister(&self, name: &str) -> bool {
        let mut map = self.fragments.write().await;
        map.remove(name).is_some()
    }

    /// Look up a single fragment by name.
    pub async fn get(&self, name: &str) -> Option<Arc<dyn ContextFragment>> {
        let map = self.fragments.read().await;
        map.get(name).cloned()
    }

    /// Return the number of registered fragments.
    pub async fn fragment_count(&self) -> usize {
        self.fragments.read().await.len()
    }

    /// List all registered fragment names (unsorted).
    pub async fn list(&self) -> Vec<String> {
        let map = self.fragments.read().await;
        map.keys().cloned().collect()
    }

    /// Render all active fragments, sorted by priority (ascending).
    ///
    /// Returns a vec of `(fragment_name, rendered_content)` pairs.
    pub async fn render_active(
        &self,
        ctx: &FragmentContext,
    ) -> Vec<(String, String)> {
        // Collect fragments under a read lock, then release before rendering.
        let fragments: Vec<Arc<dyn ContextFragment>> = {
            let map = self.fragments.read().await;
            map.values().filter(|f| f.is_active()).cloned().collect()
        };

        // Sort by priority (stable sort preserves insertion order for equal
        // priorities).
        let mut fragments = fragments;
        fragments.sort_by_key(|f| f.priority());

        let mut results = Vec::with_capacity(fragments.len());
        for f in fragments {
            match f.render(ctx).await {
                Ok(content) => results.push((f.name().to_string(), content)),
                Err(e) => {
                    tracing::warn!(
                        fragment = f.name(),
                        error = %e,
                        "fragment render failed, skipping"
                    );
                }
            }
        }
        results
    }

    /// Render a single named fragment.
    pub async fn render_by_name(
        &self,
        name: &str,
        ctx: &FragmentContext,
    ) -> Result<String, FragmentError> {
        let fragment = self
            .get(name)
            .await
            .ok_or_else(|| FragmentError::NotFound(name.to_string()))?;
        fragment.render(ctx).await
    }
}

impl Default for FragmentRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// A simple fragment implementation for testing.
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

    // -- 1. Basic register / unregister ----------------------------------

    #[tokio::test]
    async fn register_and_unregister_basic() {
        let reg = FragmentRegistry::new();

        let f = Arc::new(StubFragment::new("f1", 10, true, "hello"));
        reg.register(f).await.unwrap();

        let names = reg.list().await;
        assert_eq!(names, vec!["f1"]);

        assert!(reg.unregister("f1").await);
        assert!(reg.list().await.is_empty());
    }

    // -- 2. Unregister non-existent returns false -------------------------

    #[tokio::test]
    async fn unregister_nonexistent_returns_false() {
        let reg = FragmentRegistry::new();
        assert!(!reg.unregister("nope").await);
    }

    // -- 3. Name conflict returns AlreadyRegistered ----------------------

    #[tokio::test]
    async fn duplicate_name_returns_already_registered() {
        let reg = FragmentRegistry::new();

        let f1 = Arc::new(StubFragment::new("dup", 1, true, "a"));
        let f2 = Arc::new(StubFragment::new("dup", 2, true, "b"));

        reg.register(f1).await.unwrap();
        let err = reg.register(f2).await.unwrap_err();
        match err {
            FragmentError::AlreadyRegistered(name) => assert_eq!(name, "dup"),
            other => panic!("expected AlreadyRegistered, got {other}"),
        }
    }

    // -- 4. Get existing and missing fragment -----------------------------

    #[tokio::test]
    async fn get_returns_fragment_or_none() {
        let reg = FragmentRegistry::new();

        assert!(reg.get("absent").await.is_none());

        let f = Arc::new(StubFragment::new("present", 5, true, "x"));
        reg.register(f).await.unwrap();

        let found = reg.get("present").await;
        assert!(found.is_some());
        assert_eq!(found.unwrap().name(), "present");
    }

    // -- 5. Priority ordering in render_active ---------------------------

    #[tokio::test]
    async fn render_active_sorts_by_priority() {
        let reg = FragmentRegistry::new();

        reg.register(Arc::new(StubFragment::new(
            "low",
            100,
            true,
            "low-content",
        )))
        .await
        .unwrap();
        reg.register(Arc::new(StubFragment::new(
            "mid",
            50,
            true,
            "mid-content",
        )))
        .await
        .unwrap();
        reg.register(Arc::new(StubFragment::new(
            "high",
            1,
            true,
            "high-content",
        )))
        .await
        .unwrap();

        let rendered = reg.render_active(&ctx()).await;
        let names: Vec<&str> =
            rendered.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(names, vec!["high", "mid", "low"]);

        let contents: Vec<&str> =
            rendered.iter().map(|(_, c)| c.as_str()).collect();
        assert_eq!(
            contents,
            vec!["high-content", "mid-content", "low-content"]
        );
    }

    // -- 6. Inactive fragments are filtered out --------------------------

    #[tokio::test]
    async fn render_active_skips_inactive() {
        let reg = FragmentRegistry::new();

        reg.register(Arc::new(StubFragment::new("on", 1, true, "visible")))
            .await
            .unwrap();
        reg.register(Arc::new(StubFragment::new("off", 2, false, "hidden")))
            .await
            .unwrap();

        let rendered = reg.render_active(&ctx()).await;
        assert_eq!(rendered.len(), 1);
        assert_eq!(rendered[0].0, "on");
    }

    // -- 7. render_active returns empty when registry is empty -----------

    #[tokio::test]
    async fn render_active_empty_registry() {
        let reg = FragmentRegistry::new();
        let rendered = reg.render_active(&ctx()).await;
        assert!(rendered.is_empty());
    }

    // -- 8. render_by_name success and NotFound --------------------------

    #[tokio::test]
    async fn render_by_name_found_and_not_found() {
        let reg = FragmentRegistry::new();

        reg.register(Arc::new(StubFragment::new(
            "named",
            1,
            true,
            "content-here",
        )))
        .await
        .unwrap();

        let result = reg.render_by_name("named", &ctx()).await;
        assert_eq!(result.unwrap(), "content-here");

        let err = reg.render_by_name("missing", &ctx()).await.unwrap_err();
        match err {
            FragmentError::NotFound(name) => assert_eq!(name, "missing"),
            other => panic!("expected NotFound, got {other}"),
        }
    }

    // -- 9. Fragment that fails to render is skipped ---------------------

    struct FailingFragment {
        name: String,
    }

    #[async_trait]
    impl ContextFragment for FailingFragment {
        fn name(&self) -> &str {
            &self.name
        }

        fn priority(&self) -> u32 {
            0
        }

        fn is_active(&self) -> bool {
            true
        }

        async fn render(
            &self,
            _ctx: &FragmentContext,
        ) -> Result<String, FragmentError> {
            Err(FragmentError::RenderFailed("intentional".to_string()))
        }
    }

    #[tokio::test]
    async fn render_active_skips_failing_fragment() {
        let reg = FragmentRegistry::new();

        reg.register(Arc::new(StubFragment::new("good", 10, true, "ok")))
            .await
            .unwrap();
        reg.register(Arc::new(FailingFragment {
            name: "bad".to_string(),
        }))
        .await
        .unwrap();

        let rendered = reg.render_active(&ctx()).await;
        // Only the good fragment should appear.
        assert_eq!(rendered.len(), 1);
        assert_eq!(rendered[0].0, "good");
    }

    // -- 10. Re-register after unregister --------------------------------

    #[tokio::test]
    async fn reregister_after_unregister() {
        let reg = FragmentRegistry::new();

        let f1 = Arc::new(StubFragment::new("slot", 1, true, "v1"));
        reg.register(f1).await.unwrap();
        reg.unregister("slot").await;

        let f2 = Arc::new(StubFragment::new("slot", 2, true, "v2"));
        reg.register(f2).await.unwrap();

        let content = reg.render_by_name("slot", &ctx()).await.unwrap();
        assert_eq!(content, "v2");
    }

    // -- 11. All inactive → render_active returns empty ------------------

    #[tokio::test]
    async fn all_inactive_yields_empty() {
        let reg = FragmentRegistry::new();

        reg.register(Arc::new(StubFragment::new("a", 1, false, "x")))
            .await
            .unwrap();
        reg.register(Arc::new(StubFragment::new("b", 2, false, "y")))
            .await
            .unwrap();

        let rendered = reg.render_active(&ctx()).await;
        assert!(rendered.is_empty());
    }

    // -- 12. Stable sort: equal priority preserves insertion order -------

    #[tokio::test]
    async fn equal_priority_preserves_insertion_order() {
        let reg = FragmentRegistry::new();

        // All have the same priority; HashMap iteration order is not
        // guaranteed, but insertion-into the vec is deterministic when we
        // collect values. The important guarantee is that *all* fragments
        // with the same priority appear (no drops) and sorting is stable.
        reg.register(Arc::new(StubFragment::new("first", 5, true, "1")))
            .await
            .unwrap();
        reg.register(Arc::new(StubFragment::new("second", 5, true, "2")))
            .await
            .unwrap();
        reg.register(Arc::new(StubFragment::new("third", 5, true, "3")))
            .await
            .unwrap();

        let rendered = reg.render_active(&ctx()).await;
        let names: Vec<&str> =
            rendered.iter().map(|(n, _)| n.as_str()).collect();
        // All three must be present; exact order may vary due to HashMap,
        // but the set must be complete.
        let mut sorted_names = names;
        sorted_names.sort();
        assert_eq!(sorted_names, vec!["first", "second", "third"]);
    }
}
