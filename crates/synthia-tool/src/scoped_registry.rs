//! Scoped tool registry with RAII cleanup.
//!
//! Provides per-session tool registration with automatic cleanup
//! when the session ends via [`ScopeGuard`] drop.

use std::sync::Arc;

use dashmap::DashMap;
use synthia_core::RegistryItem;
use synthia_provider::types::ToolDefinition;

use crate::{registry::ToolEntry, traits::Tool};

/// Unique token identifying a scope.
pub type Token = Arc<()>;

/// A single scoped tool registration.
pub struct ScopedRegistration {
    /// Token identifying the scope this registration belongs to.
    pub token: Token,
    /// The registered tool.
    pub tool: Arc<dyn Tool>,
}

/// A tool registry that supports per-session scoped registrations.
///
/// Scoped registrations are automatically cleaned up when the
/// corresponding [`ScopeGuard`] is dropped.
pub struct ScopedToolRegistry {
    /// Scoped registrations keyed by tool name.
    local: DashMap<String, Vec<ScopedRegistration>>,
    /// The global registry tools snapshot for iteration.
    global_tools: Vec<ToolEntry>,
}

impl ScopedToolRegistry {
    /// Register tools in a scope. These tools override any global
    /// tools with the same name until the returned [`ScopeGuard`]
    /// is dropped.
    pub fn register_scoped(
        &self,
        tools: Vec<(String, Arc<dyn Tool>)>,
        token: Token,
    ) {
        for (name, tool) in tools {
            self.local
                .entry(name)
                .or_default()
                .push(ScopedRegistration {
                    token: token.clone(),
                    tool,
                });
        }
    }

    /// Materialize the effective tool set: global tools plus scoped
    /// overrides, with last-wins semantics (most recent scoped
    /// registration for each name wins).
    pub fn materialize(&self) -> Vec<ToolDefinition> {
        let mut result: Vec<ToolDefinition> = Vec::new();

        // Collect global tools
        for entry in &self.global_tools {
            result.push(ToolDefinition::new(
                entry.name().to_string(),
                entry.description().to_string(),
                entry.tool_instance().parameters(),
            ));
        }

        // Apply scoped overrides (last-wins)
        let mut seen: std::collections::HashMap<String, usize> =
            std::collections::HashMap::new();
        for (idx, def) in result.iter().enumerate() {
            seen.insert(def.name.clone(), idx);
        }

        for entry in self.local.iter() {
            let registrations = entry.value();
            if let Some(last) = registrations.last() {
                let name = last.tool.name();
                let def = ToolDefinition::new(
                    name.to_string(),
                    last.tool.description().to_string(),
                    last.tool.parameters(),
                );
                if let Some(idx) = seen.get(name) {
                    result[*idx] = def;
                } else {
                    seen.insert(name.to_string(), result.len());
                    result.push(def);
                }
            }
        }

        result
    }

    /// Create a new scoped registry from a snapshot of global tools.
    ///
    /// Returns the registry and a guard. When the guard is dropped,
    /// all scoped registrations associated with it are removed.
    pub fn create_scope(
        global_tools: Vec<ToolEntry>,
    ) -> (Arc<ScopedToolRegistry>, ScopeGuard) {
        let registry = Arc::new(ScopedToolRegistry {
            local: DashMap::new(),
            global_tools,
        });
        let token: Token = Arc::new(());
        let guard = ScopeGuard {
            token: token.clone(),
            registry: Arc::clone(&registry),
        };
        registry.register_scoped(vec![], token);
        (registry, guard)
    }
}

/// RAII guard that cleans up scoped registrations on drop.
pub struct ScopeGuard {
    /// Token identifying this scope.
    token: Token,
    /// Reference to the registry for cleanup.
    registry: Arc<ScopedToolRegistry>,
}

impl Drop for ScopeGuard {
    fn drop(&mut self) {
        let keys_to_clean: Vec<String> = self
            .registry
            .local
            .iter()
            .filter(|entry| {
                entry
                    .value()
                    .iter()
                    .any(|r| Arc::ptr_eq(&r.token, &self.token))
            })
            .map(|entry| entry.key().clone())
            .collect();

        for key in keys_to_clean {
            if let Some(mut regs) = self.registry.local.get_mut(&key) {
                regs.retain(|r| !Arc::ptr_eq(&r.token, &self.token));
            }
        }
    }
}

/// Tool scope layer used by [`LayeredToolRegistry`].
///
/// Priority is `Project > User > Session > Global` — i.e. a tool
/// registered in a higher-priority scope shadows the same-named tool
/// in lower-priority scopes during materialization.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ToolScope {
    /// Process-wide default. Always present.
    Global,
    /// Per-session override. Lives as long as the session.
    Session,
    /// Per-user override (e.g. `~/.config/synthia/tools.toml`).
    User,
    /// Per-project override (e.g. `.synthia/tools.toml`).
    Project,
}

impl ToolScope {
    /// Higher value wins during materialization. The numeric
    /// values are spaced so callers can debug `materialize` with
    /// `source_scope.priority()` in OTel spans.
    pub fn priority(self) -> u8 {
        match self {
            ToolScope::Project => 40,
            ToolScope::User => 30,
            ToolScope::Session => 20,
            ToolScope::Global => 10,
        }
    }
}

impl std::fmt::Display for ToolScope {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ToolScope::Global => f.write_str("Global"),
            ToolScope::Session => f.write_str("Session"),
            ToolScope::User => f.write_str("User"),
            ToolScope::Project => f.write_str("Project"),
        }
    }
}

/// Layered tool registry spanning [`ToolScope::Global`],
/// [`ToolScope::Session`], [`ToolScope::User`], and
/// [`ToolScope::Project`].
///
/// `materialize(session_id)` returns the effective set of tools for a
/// given session: tools in higher-priority scopes shadow tools in
/// lower-priority scopes (per-name last-wins within a single layer).
///
/// Distinct from [`ScopedToolRegistry`] which is RAII-token based; this
/// registry is intended for long-lived layered configuration (process
/// lifetime) and is the registry used by the orchestrator when
/// assembling the per-turn tool set.
type LayerMap =
    parking_lot::RwLock<std::collections::HashMap<String, Arc<dyn Tool>>>;

pub struct LayeredToolRegistry {
    /// Per-scope tool maps.
    layers: DashMap<ToolScope, LayerMap>,
    /// Session-scoped overrides keyed by session id.
    session_tools: DashMap<String, LayerMap>,
}

impl Default for LayeredToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl LayeredToolRegistry {
    pub fn new() -> Self {
        Self {
            layers: DashMap::new(),
            session_tools: DashMap::new(),
        }
    }

    /// Register a tool in a non-session scope (`Global` / `User` /
    /// `Project`).
    pub fn register_in_scope(
        &self,
        scope: ToolScope,
        name: impl Into<String>,
        tool: Arc<dyn Tool>,
    ) {
        let entry = self.layers.entry(scope).or_default();
        entry.write().insert(name.into(), tool);
    }

    /// Register a tool override for a specific session.
    pub fn register_session(
        &self,
        session_id: &str,
        name: impl Into<String>,
        tool: Arc<dyn Tool>,
    ) {
        let entry = self
            .session_tools
            .entry(session_id.to_string())
            .or_default();
        entry.write().insert(name.into(), tool);
    }

    /// Materialize the effective tool set for `session_id`.
    ///
    /// Walk scopes from lowest to highest priority, last-wins per
    /// (scope, name). Returns one entry per unique tool name with the
    /// scope that owns it.
    pub fn materialize(
        &self,
        session_id: &str,
    ) -> Vec<(String, Arc<dyn Tool>, ToolScope)> {
        // Insertion order: Global < User < Project (Session is
        // resolved from the session_tools map). This guarantees that
        // a later-inserted entry with the same name shadows earlier
        // ones.
        let mut result: std::collections::HashMap<
            String,
            (Arc<dyn Tool>, ToolScope),
        > = std::collections::HashMap::new();

        let mut static_scopes =
            vec![ToolScope::Global, ToolScope::User, ToolScope::Project];
        static_scopes.sort_by_key(|s| s.priority());

        for scope in static_scopes {
            if let Some(layer) = self.layers.get(&scope) {
                let layer_read = layer.read();
                for (name, tool) in layer_read.iter() {
                    result.insert(name.clone(), (tool.clone(), scope));
                }
            }
        }

        if let Some(session_layer) = self.session_tools.get(session_id) {
            let session_read = session_layer.read();
            for (name, tool) in session_read.iter() {
                result.insert(name.clone(), (tool.clone(), ToolScope::Session));
            }
        }

        result
            .into_iter()
            .map(|(name, (tool, scope))| (name, tool, scope))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use async_trait::async_trait;

    use crate::{
        registry::ToolEntry,
        scoped_registry::ScopedToolRegistry,
        traits::Tool,
        types::{ToolInput, ToolOutput},
    };

    struct TestTool {
        name: String,
        description: String,
    }

    impl TestTool {
        fn new(name: &str, description: &str) -> Self {
            Self {
                name: name.to_string(),
                description: description.to_string(),
            }
        }
    }

    #[async_trait]
    impl Tool for TestTool {
        fn name(&self) -> &str {
            &self.name
        }

        fn description(&self) -> &str {
            &self.description
        }

        fn parameters(&self) -> serde_json::Value {
            serde_json::json!({})
        }

        async fn call(&self, _input: ToolInput) -> ToolOutput {
            ToolOutput::text("ok")
        }
    }

    fn make_entry(name: &str, description: &str) -> ToolEntry {
        ToolEntry::new(Arc::new(TestTool::new(name, description)))
    }

    #[test]
    fn test_scoped_registry_global_only() {
        let global_tools =
            vec![make_entry("tool1", "desc1"), make_entry("tool2", "desc2")];
        let (registry, _guard) = ScopedToolRegistry::create_scope(global_tools);

        let tools = registry.materialize();
        assert_eq!(tools.len(), 2);
    }

    #[test]
    fn test_scoped_registry_override() {
        let global_tools =
            vec![make_entry("test_tool", "original description")];

        let (registry, guard) = ScopedToolRegistry::create_scope(global_tools);

        // Verify original is present
        let tools = registry.materialize();
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].description, "original description");

        // Override with scoped registration
        registry.register_scoped(
            vec![(
                "test_tool".to_string(),
                Arc::new(TestTool::new("test_tool", "scoped description")),
            )],
            guard.token.clone(),
        );

        let tools = registry.materialize();
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].description, "scoped description");
    }

    #[test]
    fn test_scope_guard_cleanup() {
        let global_tools = vec![make_entry("test_tool", "original")];

        let (registry, guard) = ScopedToolRegistry::create_scope(global_tools);

        registry.register_scoped(
            vec![(
                "test_tool".to_string(),
                Arc::new(TestTool::new("test_tool", "scoped")),
            )],
            guard.token.clone(),
        );

        assert_eq!(registry.materialize().len(), 1);

        drop(guard);

        // After dropping guard, scoped tools should be removed
        let tools = registry.materialize();
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].description, "original");
    }

    #[test]
    fn test_multiple_scopes() {
        let global_tools = vec![make_entry("tool_a", "global_a")];

        let (registry, guard1) =
            ScopedToolRegistry::create_scope(global_tools.clone());
        let (_registry2, _guard2) =
            ScopedToolRegistry::create_scope(global_tools);

        registry.register_scoped(
            vec![(
                "tool_a".to_string(),
                Arc::new(TestTool::new("tool_a", "scoped1")),
            )],
            guard1.token.clone(),
        );

        // guard2's scope should not affect registry
        let tools = registry.materialize();
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].description, "scoped1");

        drop(guard1);

        let tools = registry.materialize();
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].description, "global_a");
    }

    #[test]
    fn test_scoped_adds_new_tool() {
        let global_tools = vec![make_entry("tool_a", "global_a")];

        let (registry, guard) = ScopedToolRegistry::create_scope(global_tools);

        // Add a completely new tool via scoped registration
        registry.register_scoped(
            vec![(
                "tool_b".to_string(),
                Arc::new(TestTool::new("tool_b", "new_tool")),
            )],
            guard.token.clone(),
        );

        let tools = registry.materialize();
        assert_eq!(tools.len(), 2);
        let tool_b = tools.iter().find(|t| t.name == "tool_b").unwrap();
        assert_eq!(tool_b.description, "new_tool");
    }
}

// Tests for `ToolScope` + `LayeredToolRegistry` (Phase 1, Task 1.2).
#[cfg(test)]
mod layered_tests {
    use std::sync::Arc;

    use async_trait::async_trait;

    use crate::{
        scoped_registry::{LayeredToolRegistry, ToolScope},
        traits::Tool,
        types::{ToolInput, ToolOutput},
    };

    struct DummyTool {
        name: String,
    }

    impl DummyTool {
        fn new(name: &str) -> Self {
            Self {
                name: name.to_string(),
            }
        }
    }

    #[async_trait]
    impl Tool for DummyTool {
        fn name(&self) -> &str {
            &self.name
        }

        fn description(&self) -> &str {
            "dummy"
        }

        fn parameters(&self) -> serde_json::Value {
            serde_json::json!({})
        }

        async fn call(&self, _input: ToolInput) -> ToolOutput {
            ToolOutput::text("ok")
        }
    }

    #[test]
    fn tool_scope_priority_order() {
        assert!(ToolScope::Project.priority() > ToolScope::User.priority());
        assert!(ToolScope::User.priority() > ToolScope::Session.priority());
        assert!(ToolScope::Session.priority() > ToolScope::Global.priority());
    }

    #[test]
    fn tool_scope_display() {
        assert_eq!(ToolScope::Global.to_string(), "Global");
        assert_eq!(ToolScope::Session.to_string(), "Session");
        assert_eq!(ToolScope::User.to_string(), "User");
        assert_eq!(ToolScope::Project.to_string(), "Project");
    }

    #[test]
    fn layered_registry_project_overrides_user_and_global() {
        let registry = LayeredToolRegistry::new();
        registry.register_in_scope(
            ToolScope::Global,
            "read",
            Arc::new(DummyTool::new("read_global")),
        );
        registry.register_in_scope(
            ToolScope::User,
            "read",
            Arc::new(DummyTool::new("read_user")),
        );
        registry.register_in_scope(
            ToolScope::Project,
            "read",
            Arc::new(DummyTool::new("read_project")),
        );

        let tools = registry.materialize("session-1");
        let reads: Vec<_> =
            tools.iter().filter(|(n, _, _)| n == "read").collect();
        assert_eq!(reads.len(), 1, "Project should win and shadow others");
        assert_eq!(reads[0].2, ToolScope::Project);
        assert_eq!(reads[0].0, "read");
    }

    #[test]
    fn layered_registry_user_overrides_global() {
        let registry = LayeredToolRegistry::new();
        registry.register_in_scope(
            ToolScope::Global,
            "read",
            Arc::new(DummyTool::new("read_global")),
        );
        registry.register_in_scope(
            ToolScope::User,
            "read",
            Arc::new(DummyTool::new("read_user")),
        );

        let tools = registry.materialize("session-1");
        let reads: Vec<_> =
            tools.iter().filter(|(n, _, _)| n == "read").collect();
        assert_eq!(reads.len(), 1);
        assert_eq!(reads[0].2, ToolScope::User);
    }

    #[test]
    fn layered_registry_session_isolated_per_session() {
        let registry = LayeredToolRegistry::new();
        registry.register_in_scope(
            ToolScope::Global,
            "global_tool",
            Arc::new(DummyTool::new("global_tool")),
        );

        registry.register_session(
            "session-1",
            "session_tool",
            Arc::new(DummyTool::new("session_tool")),
        );

        // session-1 sees the global tool AND its session override
        let tools_s1 = registry.materialize("session-1");
        let names_s1: std::collections::HashSet<_> =
            tools_s1.iter().map(|(n, _, _)| n.clone()).collect();
        assert!(names_s1.contains("global_tool"));
        assert!(names_s1.contains("session_tool"));

        // session-2 sees only the global tool
        let tools_s2 = registry.materialize("session-2");
        let names_s2: std::collections::HashSet<_> =
            tools_s2.iter().map(|(n, _, _)| n.clone()).collect();
        assert!(names_s2.contains("global_tool"));
        assert!(!names_s2.contains("session_tool"));
    }

    #[test]
    fn layered_registry_session_overrides_global() {
        let registry = LayeredToolRegistry::new();
        registry.register_in_scope(
            ToolScope::Global,
            "read",
            Arc::new(DummyTool::new("read_global")),
        );
        registry.register_session(
            "session-1",
            "read",
            Arc::new(DummyTool::new("read_session")),
        );

        let tools = registry.materialize("session-1");
        let reads: Vec<_> =
            tools.iter().filter(|(n, _, _)| n == "read").collect();
        assert_eq!(reads.len(), 1);
        assert_eq!(reads[0].2, ToolScope::Session);
    }
}
