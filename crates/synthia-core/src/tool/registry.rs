//! ToolRegistry + ToolIdentity + Materialization + RegistrationScope.

use std::{collections::HashMap, sync::Arc};

use parking_lot::RwLock;
use tracing;

use crate::tool::{
    descriptor::{Tool, ToolDescriptor, ToolExposure, ToolProvenance},
    provider::ToolProvider,
    tool_name::ToolName,
};

/// Monotonic generation counter for stale detection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct ToolGeneration(pub u64);

/// Value-type tool identity for snapshot stale detection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolIdentity {
    pub name: ToolName,
    pub generation: ToolGeneration,
}

/// Registration token for unregistration.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RegistrationToken(pub u64);

/// Registration error.
#[derive(Debug, thiserror::Error)]
pub enum RegistrationError {
    #[error("core tool name taken: {name}")]
    CoreNameTaken { name: ToolName },
    #[error("duplicate tool: {name}")]
    DuplicateName { name: ToolName },
    #[error("registration failed: {0}")]
    Failed(String),
}

/// Entry in the tool registry.
struct ToolEntry {
    #[expect(dead_code)]
    // used for provider identification; consumed by future deregistration
    provider_id: String,
    /// Token that owns this registration — used for scoped unregistration.
    provider_token: RegistrationToken,
    tool: Arc<dyn Tool>,
    identity: ToolIdentity,
    provenance: ToolProvenance,
}

/// Immutable materialization snapshot for stale detection.
#[derive(Clone)]
pub struct Materialization {
    /// Snapshot of tool identities at materialization time.
    identities: HashMap<ToolName, ToolIdentity>,
    /// Snapshot of tool references.
    tools: HashMap<ToolName, Arc<dyn Tool>>,
    /// Exposure level per tool — used by `tool_descriptors_for_llm`.
    exposure_map: HashMap<ToolName, ToolExposure>,
    /// Snapshot token.
    #[expect(dead_code)]
    // used for identity comparison; consumed by stale detection
    token: u64,
}

/// Stale or unknown tool resolution error.
#[derive(Debug, Clone)]
pub enum StaleOrUnknown {
    Stale {
        name: String,
        seen: u64,
        current: u64,
    },
    Unknown {
        name: String,
    },
}

impl Materialization {
    /// Return tool descriptors suitable for sending to the LLM.
    ///
    /// - `Direct` tools: full descriptor including parameters schema.
    /// - `Deferred` tools: only name + description (parameters set to empty object).
    /// - `Hidden` tools are never included (filtered out during materialization).
    pub fn tool_descriptors_for_llm(&self) -> Vec<ToolDescriptor> {
        self.tools
            .values()
            .map(|tool| {
                let desc = tool.descriptor();
                match self.exposure_map.get(&desc.name) {
                    Some(ToolExposure::Deferred) => ToolDescriptor {
                        parameters: serde_json::Value::Object(
                            Default::default(),
                        ),
                        ..desc.clone()
                    },
                    _ => desc.clone(),
                }
            })
            .collect()
    }

    /// Return the exposure level of a tool in this snapshot.
    pub fn exposure_of(&self, name: &ToolName) -> Option<ToolExposure> {
        self.exposure_map.get(name).copied()
    }
}

/// Unified tool registry.
pub struct ToolRegistry {
    inner: RwLock<ToolRegistryInner>,
    next_token: RwLock<u64>,
}

struct ToolRegistryInner {
    /// Tool name → entries (LIFO for non-core tools).
    tools: HashMap<ToolName, Vec<ToolEntry>>,
    /// Monotonic generation counter.
    generation: ToolGeneration,
    /// Next registration token.
    next_registration: u64,
}

impl ToolRegistry {
    /// Create an empty registry.
    pub fn new() -> Self {
        Self {
            inner: RwLock::new(ToolRegistryInner {
                tools: HashMap::new(),
                generation: ToolGeneration(1),
                next_registration: 1,
            }),
            next_token: RwLock::new(1),
        }
    }

    /// Register all tools from a provider.
    pub async fn register_provider(
        &self,
        provider: Arc<dyn ToolProvider>,
    ) -> Result<RegistrationToken, RegistrationError> {
        let descriptors = provider.list_tools().await;

        // Resolve tools outside the lock to avoid holding it across await points.
        let resolved: Vec<(ToolDescriptor, Arc<dyn Tool>)> = {
            let mut resolved = Vec::with_capacity(descriptors.len());
            for desc in &descriptors {
                if let Some(tool) =
                    provider.get_tool(&desc.name.full_name()).await
                {
                    resolved.push((desc.clone(), tool));
                }
            }
            resolved
        };

        let mut inner = self.inner.write();

        let token = RegistrationToken(inner.next_registration);
        inner.next_registration += 1;

        for (desc, tool) in resolved {
            // Check core name immutability
            if let Some(existing) = inner.tools.get(&desc.name)
                && existing
                    .iter()
                    .any(|e| e.provenance == ToolProvenance::Core)
                && desc.provenance == ToolProvenance::Core
            {
                return Err(RegistrationError::CoreNameTaken {
                    name: desc.name.clone(),
                });
            }

            let identity = ToolIdentity {
                name: desc.name.clone(),
                generation: inner.generation,
            };

            let entry = ToolEntry {
                provider_id: provider.id().to_string(),
                provider_token: token.clone(),
                tool,
                identity,
                provenance: desc.provenance,
            };

            inner.tools.entry(desc.name).or_default().push(entry);
        }

        // Bump generation after registration
        inner.generation.0 += 1;

        Ok(token)
    }

    /// Unregister all tools owned by a token.
    pub fn unregister(&self, token: RegistrationToken) {
        self.unregister_by_token(token);
    }

    /// Remove all `ToolEntry` instances that were registered with the given token.
    ///
    /// After removal, bumps the generation counter so that stale materialization
    /// snapshots correctly detect the change.
    pub fn unregister_by_token(&self, token: RegistrationToken) {
        let mut inner = self.inner.write();
        let mut removed_count = 0usize;

        // Drain entries matching the token from each tool bucket.
        inner.tools.retain(|_name, entries| {
            let before = entries.len();
            entries.retain(|e| e.provider_token != token);
            let removed = before - entries.len();
            removed_count += removed;
            !entries.is_empty()
        });

        if removed_count > 0 {
            inner.generation.0 += 1;
            tracing::info!(
                token = token.0,
                count = removed_count,
                "unregistered tools by token"
            );
        } else {
            tracing::debug!(
                token = token.0,
                "unregister_by_token: no matching entries"
            );
        }
    }

    /// Materialize an immutable snapshot for the LLM.
    ///
    /// - `Hidden` tools are excluded entirely (not visible to LLM).
    /// - `Deferred` tools are included with only name + description.
    /// - `Direct` tools are fully included.
    pub fn materialize(&self) -> Materialization {
        let inner = self.inner.read();
        let mut identities = HashMap::new();
        let mut tools = HashMap::new();
        let mut exposure_map = HashMap::new();

        for (name, entries) in &inner.tools {
            if let Some(entry) = entries.last() {
                let exposure = entry.tool.descriptor().exposure;
                // Hidden tools are excluded from materialization
                if exposure == ToolExposure::Hidden {
                    continue;
                }
                identities.insert(name.clone(), entry.identity.clone());
                tools.insert(name.clone(), entry.tool.clone());
                exposure_map.insert(name.clone(), exposure);
            }
        }

        let token = {
            let mut next = self.next_token.write();
            let t = *next;
            *next += 1;
            t
        };

        Materialization {
            identities,
            tools,
            exposure_map,
            token,
        }
    }

    /// Resolve a tool from a materialization snapshot.
    pub fn resolve(
        mat: &Materialization,
        name: &str,
        current_registry: &ToolRegistry,
    ) -> Result<Arc<dyn Tool>, StaleOrUnknown> {
        let key =
            ToolName::parse(name).unwrap_or_else(|| ToolName::plain(name));
        let current = current_registry.inner.read();

        // Check if tool exists
        let current_entry =
            current.tools.get(&key).and_then(|entries| entries.last());

        let Some(current_tool) = current_entry else {
            return Err(StaleOrUnknown::Unknown {
                name: name.to_string(),
            });
        };

        // Check stale detection
        if let Some(seen_identity) = mat.identities.get(&key)
            && seen_identity.generation != current_tool.identity.generation
        {
            return Err(StaleOrUnknown::Stale {
                name: name.to_string(),
                seen: seen_identity.generation.0,
                current: current_tool.identity.generation.0,
            });
        }

        // Return the tool from snapshot
        mat.tools.get(&key).cloned().ok_or(StaleOrUnknown::Unknown {
            name: name.to_string(),
        })
    }

    /// Resolve a tool without snapshot (for non-LLM callers).
    pub fn resolve_now(&self, name: &str) -> Option<Arc<dyn Tool>> {
        let key =
            ToolName::parse(name).unwrap_or_else(|| ToolName::plain(name));
        let inner = self.inner.read();
        inner
            .tools
            .get(&key)
            .and_then(|entries| entries.last())
            .map(|entry| entry.tool.clone())
    }

    /// Register all tools from a provider and return a `RegistrationScope`.
    ///
    /// When the scope is dropped, all tools registered by this call are
    /// automatically unregistered from the registry.
    pub async fn register_scoped(
        self: &Arc<Self>,
        provider: Arc<dyn ToolProvider>,
    ) -> Result<RegistrationScope, RegistrationError> {
        let token = self.register_provider(provider).await?;

        let tool_names = {
            let inner = self.inner.read();
            inner
                .tools
                .iter()
                .filter_map(|(name, entries)| {
                    entries
                        .iter()
                        .any(|e| e.provider_token == token)
                        .then_some(name.clone())
                })
                .collect()
        };

        Ok(RegistrationScope {
            token,
            registry: Arc::downgrade(self),
            tool_names,
            namespace: None,
        })
    }

    /// Register all tools from a provider with a namespace and return a `RegistrationScope`.
    ///
    /// The namespace is stored in the scope for later use when `ToolName` migration
    /// completes. Actual namespacing of tool names is not applied yet.
    pub async fn register_scoped_with_namespace(
        self: &Arc<Self>,
        provider: Arc<dyn ToolProvider>,
        namespace: &str,
    ) -> Result<RegistrationScope, RegistrationError> {
        let token = self.register_provider(provider).await?;

        let tool_names = {
            let inner = self.inner.read();
            inner
                .tools
                .iter()
                .filter_map(|(name, entries)| {
                    entries
                        .iter()
                        .any(|e| e.provider_token == token)
                        .then_some(name.clone())
                })
                .collect()
        };

        Ok(RegistrationScope {
            token,
            registry: Arc::downgrade(self),
            tool_names,
            namespace: Some(namespace.to_string()),
        })
    }

    /// Return the number of registered tools (LIFO top-only count).
    pub fn tool_count(&self) -> usize {
        let inner = self.inner.read();
        inner.tools.len()
    }

    /// Internal consistency check (debug only).
    #[cfg(debug_assertions)]
    pub fn internal_consistency_check(&self) {
        let inner = self.inner.read();
        for (name, entries) in &inner.tools {
            for entry in entries {
                assert_eq!(entry.identity.name, *name);
                assert_eq!(entry.tool.name(), name.name());
            }
        }
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// RAII scope that automatically unregisters tools when dropped.
///
/// Created by [`ToolRegistry::register_scoped`] or
/// [`ToolRegistry::register_scoped_with_namespace`]. When the scope goes out of
/// scope, all tools that were registered under its token are removed from the
/// registry. If the registry itself has already been dropped, cleanup is a
/// no-op.
#[derive(Debug)]
pub struct RegistrationScope {
    token: RegistrationToken,
    registry: std::sync::Weak<ToolRegistry>,
    /// Tool names registered in this scope (for diagnostics / future namespace use).
    tool_names: Vec<ToolName>,
    /// Optional namespace (stored for when ToolName migration completes).
    namespace: Option<String>,
}

impl RegistrationScope {
    /// The registration token for this scope.
    pub fn token(&self) -> &RegistrationToken {
        &self.token
    }

    /// The tool names registered in this scope.
    pub fn tool_names(&self) -> &[ToolName] {
        &self.tool_names
    }

    /// The namespace for this scope, if any.
    pub fn namespace(&self) -> Option<&str> {
        self.namespace.as_deref()
    }

    /// Manually unregister all tools and invalidate the scope.
    ///
    /// After calling this, the `Drop` impl becomes a no-op.
    pub fn close(mut self) {
        self.cleanup();
    }

    /// Perform cleanup: upgrade `Weak` → `Arc`, call `unregister_by_token`.
    fn cleanup(&mut self) {
        if let Some(registry) = self.registry.upgrade() {
            tracing::info!(
                token = self.token.0,
                tool_count = self.tool_names.len(),
                namespace = self.namespace.as_deref().unwrap_or(""),
                "RegistrationScope dropped — unregistering tools"
            );
            registry.unregister_by_token(self.token.clone());
        } else {
            tracing::debug!(
                token = self.token.0,
                "RegistrationScope dropped but registry already gone — no-op"
            );
        }
        // Clear tool_names so a subsequent drop (impossible in safe code, but
        // defensive) does not try again.
        self.tool_names.clear();
    }
}

impl Drop for RegistrationScope {
    fn drop(&mut self) {
        self.cleanup();
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use async_trait::async_trait;

    use super::*;
    use crate::tool::{
        descriptor::{Tool, ToolDescriptor, ToolProvenance},
        tool_name::ToolName,
        types::{ToolContext, ToolError, ToolInput, ToolOutput},
    };

    /// Minimal tool for stale detection testing.
    struct TestTool {
        tool_name: String,
    }

    #[async_trait]
    impl Tool for TestTool {
        fn name(&self) -> &str {
            &self.tool_name
        }

        async fn execute(
            &self,
            _input: ToolInput,
            _ctx: &ToolContext,
        ) -> Result<ToolOutput, ToolError> {
            Ok(ToolOutput::default())
        }

        fn descriptor(&self) -> &ToolDescriptor {
            // Return a static descriptor — simplified for testing.
            static DESC: std::sync::OnceLock<ToolDescriptor> =
                std::sync::OnceLock::new();
            DESC.get_or_init(|| ToolDescriptor {
                name: ToolName::plain("test"),
                description: "test tool".to_string(),
                parameters: serde_json::Value::Null,
                category: crate::tool::descriptor::ToolCategory::Utility,
                provenance: ToolProvenance::Core,
                execution_mode:
                    crate::tool::descriptor::ExecutionMode::Parallel,
                cancel_behavior:
                    crate::tool::descriptor::CancelBehavior::Cooperative,
                examples: vec![],
                permission_required: false,
                prompt_visible_provenance: true,
                is_hidden: false,
                is_user_invocable: true,
                exposure: crate::tool::descriptor::ToolExposure::Direct,
            })
        }
    }

    #[test]
    fn materialization_stale_detection() {
        let registry = ToolRegistry::new();

        // Materialize when empty
        let mat1 = registry.materialize();
        assert!(mat1.identities.is_empty());

        // Register a tool via a simple provider
        // (We use resolve_now to check directly)
        let tool: Arc<dyn Tool> = Arc::new(TestTool {
            tool_name: "test-tool".to_string(),
        });

        // Direct insert into inner for testing
        {
            let mut inner = registry.inner.write();
            let entry = ToolEntry {
                provider_id: "test".to_string(),
                provider_token: RegistrationToken(1),
                tool: tool.clone(),
                identity: ToolIdentity {
                    name: ToolName::plain("test-tool"),
                    generation: inner.generation,
                },
                provenance: ToolProvenance::Core,
            };
            inner
                .tools
                .insert(ToolName::plain("test-tool"), vec![entry]);
            inner.generation.0 += 1;
        }

        // Materialize again — should include the tool
        let mat2 = registry.materialize();
        assert!(mat2.identities.contains_key(&ToolName::plain("test-tool")));

        // Resolve with matching materialization — should succeed
        let result = ToolRegistry::resolve(&mat2, "test-tool", &registry);
        assert!(result.is_ok());

        // Resolve with stale materialization (mat1) — should be stale
        let result = ToolRegistry::resolve(&mat1, "test-tool", &registry);
        match result {
            Err(StaleOrUnknown::Stale { name, .. }) => {
                assert_eq!(name, "test-tool");
            }
            Err(StaleOrUnknown::Unknown { name }) => {
                assert_eq!(name, "test-tool");
                // Also valid: mat1 doesn't know about this tool at all
            }
            Ok(_) => panic!("Expected stale or unknown error"),
        }
    }

    #[test]
    fn resolve_now_finds_tool() {
        let registry = ToolRegistry::new();
        let tool: Arc<dyn Tool> = Arc::new(TestTool {
            tool_name: "find-me".to_string(),
        });

        {
            let mut inner = registry.inner.write();
            let entry = ToolEntry {
                provider_id: "test".to_string(),
                provider_token: RegistrationToken(1),
                tool,
                identity: ToolIdentity {
                    name: ToolName::plain("find-me"),
                    generation: inner.generation,
                },
                provenance: ToolProvenance::Core,
            };
            inner.tools.insert(ToolName::plain("find-me"), vec![entry]);
        }

        let found = registry.resolve_now("find-me");
        assert!(found.is_some());
        assert_eq!(found.unwrap().name(), "find-me");

        let missing = registry.resolve_now("nonexistent");
        assert!(missing.is_none());
    }

    #[test]
    #[cfg(debug_assertions)]
    fn internal_consistency_check() {
        let registry = ToolRegistry::new();
        // Empty registry should pass
        registry.internal_consistency_check();
    }

    // ── Helper: a simple ToolProvider for testing ──────────────────────────

    /// A minimal `ToolProvider` that exposes one tool.
    struct SimpleProvider {
        id: String,
        tool_name: String,
    }

    #[async_trait]
    impl ToolProvider for SimpleProvider {
        fn id(&self) -> &str {
            &self.id
        }

        async fn list_tools(&self) -> Vec<ToolDescriptor> {
            vec![ToolDescriptor {
                name: ToolName::plain(&self.tool_name),
                description: "simple test tool".to_string(),
                parameters: serde_json::Value::Null,
                category: crate::tool::descriptor::ToolCategory::Utility,
                provenance: ToolProvenance::Plugin {
                    id: "test".to_string(),
                },
                execution_mode:
                    crate::tool::descriptor::ExecutionMode::Parallel,
                cancel_behavior:
                    crate::tool::descriptor::CancelBehavior::Cooperative,
                examples: vec![],
                permission_required: false,
                prompt_visible_provenance: true,
                is_hidden: false,
                is_user_invocable: true,
                exposure: crate::tool::descriptor::ToolExposure::Direct,
            }]
        }

        async fn get_tool(&self, name: &str) -> Option<Arc<dyn Tool>> {
            if name == self.tool_name {
                Some(Arc::new(TestTool {
                    tool_name: self.tool_name.clone(),
                }))
            } else {
                None
            }
        }
    }

    /// A provider that exposes two tools.
    struct MultiProvider {
        id: String,
        tool_names: Vec<String>,
    }

    #[async_trait]
    impl ToolProvider for MultiProvider {
        fn id(&self) -> &str {
            &self.id
        }

        async fn list_tools(&self) -> Vec<ToolDescriptor> {
            self.tool_names
                .iter()
                .map(|name| ToolDescriptor {
                    name: ToolName::plain(name),
                    description: "multi test tool".to_string(),
                    parameters: serde_json::Value::Null,
                    category: crate::tool::descriptor::ToolCategory::Utility,
                    provenance: ToolProvenance::Plugin {
                        id: "test".to_string(),
                    },
                    execution_mode:
                        crate::tool::descriptor::ExecutionMode::Parallel,
                    cancel_behavior:
                        crate::tool::descriptor::CancelBehavior::Cooperative,
                    examples: vec![],
                    permission_required: false,
                    prompt_visible_provenance: true,
                    is_hidden: false,
                    is_user_invocable: true,
                    exposure: crate::tool::descriptor::ToolExposure::Direct,
                })
                .collect()
        }

        async fn get_tool(&self, name: &str) -> Option<Arc<dyn Tool>> {
            if self.tool_names.iter().any(|n| n == name) {
                Some(Arc::new(TestTool {
                    tool_name: name.to_string(),
                }))
            } else {
                None
            }
        }
    }

    // ── unregister_by_token tests ──────────────────────────────────────────

    #[tokio::test]
    async fn unregister_by_token_removes_tools() {
        let registry = ToolRegistry::new();

        let provider: Arc<dyn ToolProvider> = Arc::new(SimpleProvider {
            id: "p1".to_string(),
            tool_name: "tool-a".to_string(),
        });

        let token = registry.register_provider(provider).await.unwrap();

        // Tool should be resolvable
        assert!(registry.resolve_now("tool-a").is_some());

        // Unregister by token
        registry.unregister_by_token(token);

        // Tool should no longer be resolvable
        assert!(registry.resolve_now("tool-a").is_none());
    }

    #[tokio::test]
    async fn unregister_by_token_only_removes_matching() {
        let registry = ToolRegistry::new();

        let p1: Arc<dyn ToolProvider> = Arc::new(SimpleProvider {
            id: "p1".to_string(),
            tool_name: "tool-a".to_string(),
        });
        let p2: Arc<dyn ToolProvider> = Arc::new(SimpleProvider {
            id: "p2".to_string(),
            tool_name: "tool-b".to_string(),
        });

        let token1 = registry.register_provider(p1).await.unwrap();
        let _token2 = registry.register_provider(p2).await.unwrap();

        // Both tools should be resolvable
        assert!(registry.resolve_now("tool-a").is_some());
        assert!(registry.resolve_now("tool-b").is_some());

        // Unregister only token1
        registry.unregister_by_token(token1);

        // tool-a gone, tool-b still present
        assert!(registry.resolve_now("tool-a").is_none());
        assert!(registry.resolve_now("tool-b").is_some());
    }

    #[tokio::test]
    async fn unregister_by_token_with_overlapping_names() {
        let registry = ToolRegistry::new();

        // Register same tool name from two different providers (LIFO)
        let p1: Arc<dyn ToolProvider> = Arc::new(SimpleProvider {
            id: "p1".to_string(),
            tool_name: "shared".to_string(),
        });
        let p2: Arc<dyn ToolProvider> = Arc::new(SimpleProvider {
            id: "p2".to_string(),
            tool_name: "shared".to_string(),
        });

        let token1 = registry.register_provider(p1).await.unwrap();
        let _token2 = registry.register_provider(p2).await.unwrap();

        // "shared" should resolve (to p2's, LIFO)
        assert!(registry.resolve_now("shared").is_some());

        // Unregister token1 — p1's entry is removed, but p2's remains
        registry.unregister_by_token(token1);

        // "shared" should still resolve (to p2's entry)
        assert!(registry.resolve_now("shared").is_some());
    }

    #[tokio::test]
    async fn unregister_by_token_bumps_generation() {
        let registry = ToolRegistry::new();

        let provider: Arc<dyn ToolProvider> = Arc::new(SimpleProvider {
            id: "p1".to_string(),
            tool_name: "tool-a".to_string(),
        });

        let token = registry.register_provider(provider).await.unwrap();

        // Materialize after registration
        let mat_after_reg = registry.materialize();
        assert!(
            mat_after_reg
                .identities
                .contains_key(&ToolName::plain("tool-a"))
        );

        // Unregister
        registry.unregister_by_token(token);

        // Materialize after unregistration — tool should be gone
        let mat_after_unreg = registry.materialize();
        assert!(
            !mat_after_unreg
                .identities
                .contains_key(&ToolName::plain("tool-a"))
        );

        // Old materialization should detect stale/unknown
        let result = ToolRegistry::resolve(&mat_after_reg, "tool-a", &registry);
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn unregister_noop_for_unknown_token() {
        let registry = ToolRegistry::new();

        let provider: Arc<dyn ToolProvider> = Arc::new(SimpleProvider {
            id: "p1".to_string(),
            tool_name: "tool-a".to_string(),
        });

        let _token = registry.register_provider(provider).await.unwrap();

        // Unregister with a token that was never used
        registry.unregister_by_token(RegistrationToken(999));

        // Original tool should still be present
        assert!(registry.resolve_now("tool-a").is_some());
    }

    // ── RegistrationScope tests ────────────────────────────────────────────

    #[tokio::test]
    async fn scoped_registration_auto_cleanup_on_drop() {
        let registry = Arc::new(ToolRegistry::new());

        let provider: Arc<dyn ToolProvider> = Arc::new(SimpleProvider {
            id: "scoped1".to_string(),
            tool_name: "scoped-tool".to_string(),
        });

        {
            let scope = registry.register_scoped(provider).await.unwrap();
            assert_eq!(scope.tool_names(), &[ToolName::plain("scoped-tool")]);
            // Tool should be resolvable inside the scope
            assert!(registry.resolve_now("scoped-tool").is_some());
        } // scope dropped here

        // After scope drop, tool should be gone
        assert!(registry.resolve_now("scoped-tool").is_none());
    }

    #[tokio::test]
    async fn scoped_registration_with_namespace() {
        let registry = Arc::new(ToolRegistry::new());

        let provider: Arc<dyn ToolProvider> = Arc::new(SimpleProvider {
            id: "ns1".to_string(),
            tool_name: "ns-tool".to_string(),
        });

        let scope = registry
            .register_scoped_with_namespace(provider, "my-namespace")
            .await
            .unwrap();

        assert_eq!(scope.namespace(), Some("my-namespace"));
        assert_eq!(scope.tool_names(), &[ToolName::plain("ns-tool")]);
        assert!(registry.resolve_now("ns-tool").is_some());

        // Drop the scope
        drop(scope);
        assert!(registry.resolve_now("ns-tool").is_none());
    }

    #[tokio::test]
    async fn scoped_registration_multiple_scopes() {
        let registry = Arc::new(ToolRegistry::new());

        let p1: Arc<dyn ToolProvider> = Arc::new(SimpleProvider {
            id: "s1".to_string(),
            tool_name: "tool-x".to_string(),
        });
        let p2: Arc<dyn ToolProvider> = Arc::new(SimpleProvider {
            id: "s2".to_string(),
            tool_name: "tool-y".to_string(),
        });

        let scope1 = registry.register_scoped(p1).await.unwrap();
        let scope2 = registry.register_scoped(p2).await.unwrap();

        assert!(registry.resolve_now("tool-x").is_some());
        assert!(registry.resolve_now("tool-y").is_some());

        // Drop scope1 — only tool-x should be removed
        drop(scope1);
        assert!(registry.resolve_now("tool-x").is_none());
        assert!(registry.resolve_now("tool-y").is_some());

        // Drop scope2 — tool-y removed too
        drop(scope2);
        assert!(registry.resolve_now("tool-y").is_none());
    }

    #[tokio::test]
    async fn scoped_registration_noop_when_registry_dropped_first() {
        let registry = Arc::new(ToolRegistry::new());

        let provider: Arc<dyn ToolProvider> = Arc::new(SimpleProvider {
            id: "ephemeral".to_string(),
            tool_name: "ephemeral-tool".to_string(),
        });

        let scope = registry.register_scoped(provider).await.unwrap();
        assert!(registry.resolve_now("ephemeral-tool").is_some());

        // Drop the registry first
        drop(registry);

        // Now drop the scope — should not panic (Weak::upgrade returns None)
        drop(scope);
    }

    #[tokio::test]
    async fn scoped_registration_close_method() {
        let registry = Arc::new(ToolRegistry::new());

        let provider: Arc<dyn ToolProvider> = Arc::new(SimpleProvider {
            id: "close-test".to_string(),
            tool_name: "close-tool".to_string(),
        });

        let scope = registry.register_scoped(provider).await.unwrap();
        assert!(registry.resolve_now("close-tool").is_some());

        // Explicit close
        scope.close();

        assert!(registry.resolve_now("close-tool").is_none());
    }

    #[tokio::test]
    async fn scoped_registration_token_access() {
        let registry = Arc::new(ToolRegistry::new());

        let provider: Arc<dyn ToolProvider> = Arc::new(SimpleProvider {
            id: "token-test".to_string(),
            tool_name: "token-tool".to_string(),
        });

        let scope = registry.register_scoped(provider).await.unwrap();
        // Token should be a valid non-zero value
        assert_ne!(scope.token().0, 0);
    }

    #[tokio::test]
    async fn scoped_multi_provider_cleanup() {
        let registry = Arc::new(ToolRegistry::new());

        let provider: Arc<dyn ToolProvider> = Arc::new(MultiProvider {
            id: "multi".to_string(),
            tool_names: vec!["alpha".to_string(), "beta".to_string()],
        });

        {
            let scope = registry.register_scoped(provider).await.unwrap();
            let mut names = scope.tool_names().to_vec();
            names.sort();
            assert_eq!(
                names,
                vec![ToolName::plain("alpha"), ToolName::plain("beta")]
            );

            assert!(registry.resolve_now("alpha").is_some());
            assert!(registry.resolve_now("beta").is_some());
        }

        // Both tools should be gone after scope drop
        assert!(registry.resolve_now("alpha").is_none());
        assert!(registry.resolve_now("beta").is_none());
    }

    #[tokio::test]
    async fn unregister_delegates_to_unregister_by_token() {
        let registry = ToolRegistry::new();

        let provider: Arc<dyn ToolProvider> = Arc::new(SimpleProvider {
            id: "p1".to_string(),
            tool_name: "delegated".to_string(),
        });

        let token = registry.register_provider(provider).await.unwrap();
        assert!(registry.resolve_now("delegated").is_some());

        // The public `unregister` method should work identically
        registry.unregister(token);
        assert!(registry.resolve_now("delegated").is_none());
    }

    // ── ToolExposure tests ─────────────────────────────────────────────────

    /// A tool with configurable exposure for testing.
    struct ExposureTestTool {
        tool_name: String,
        exposure: crate::tool::descriptor::ToolExposure,
        descriptor: std::sync::OnceLock<ToolDescriptor>,
    }

    impl ExposureTestTool {
        fn new(
            name: &str,
            exposure: crate::tool::descriptor::ToolExposure,
        ) -> Self {
            Self {
                tool_name: name.to_string(),
                exposure,
                descriptor: std::sync::OnceLock::new(),
            }
        }
    }

    #[async_trait]
    impl Tool for ExposureTestTool {
        fn name(&self) -> &str {
            &self.tool_name
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
                name: ToolName::plain(&self.tool_name),
                description: format!("{} tool", self.tool_name),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "arg1": { "type": "string" }
                    }
                }),
                category: crate::tool::descriptor::ToolCategory::Utility,
                provenance: ToolProvenance::Core,
                execution_mode:
                    crate::tool::descriptor::ExecutionMode::Parallel,
                cancel_behavior:
                    crate::tool::descriptor::CancelBehavior::Cooperative,
                examples: vec![],
                permission_required: false,
                prompt_visible_provenance: true,
                is_hidden: false,
                is_user_invocable: true,
                exposure: self.exposure,
            })
        }
    }

    /// Helper: insert a tool directly into the registry for exposure testing.
    fn insert_tool_with_exposure(
        registry: &ToolRegistry,
        name: &str,
        exposure: crate::tool::descriptor::ToolExposure,
    ) {
        let tool: Arc<dyn Tool> =
            Arc::new(ExposureTestTool::new(name, exposure));
        let mut inner = registry.inner.write();
        let token = RegistrationToken(inner.next_registration);
        inner.next_registration += 1;
        let entry = ToolEntry {
            provider_id: "exposure-test".to_string(),
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

    #[test]
    fn direct_tool_full_in_materialization() {
        let registry = ToolRegistry::new();
        insert_tool_with_exposure(
            &registry,
            "direct-tool",
            ToolExposure::Direct,
        );

        let mat = registry.materialize();

        // Direct tool should appear in identities
        assert!(mat.identities.contains_key(&ToolName::plain("direct-tool")));

        // Exposure should be Direct
        assert_eq!(
            mat.exposure_of(&ToolName::plain("direct-tool")),
            Some(ToolExposure::Direct)
        );

        // Full descriptor available in tool_descriptors_for_llm
        let descs = mat.tool_descriptors_for_llm();
        let desc = descs
            .iter()
            .find(|d| d.name == ToolName::plain("direct-tool"))
            .unwrap();
        // Direct tools should have their full parameters schema
        assert!(desc.parameters.is_object());
        assert!(
            desc.parameters
                .as_object()
                .unwrap()
                .contains_key("properties")
        );
    }

    #[test]
    fn deferred_tool_appears_in_materialization_with_minimal_info() {
        let registry = ToolRegistry::new();
        insert_tool_with_exposure(
            &registry,
            "deferred-tool",
            ToolExposure::Deferred,
        );

        let mat = registry.materialize();

        // Deferred tool should appear in identities
        assert!(
            mat.identities
                .contains_key(&ToolName::plain("deferred-tool"))
        );

        // Exposure should be Deferred
        assert_eq!(
            mat.exposure_of(&ToolName::plain("deferred-tool")),
            Some(ToolExposure::Deferred)
        );

        // In LLM descriptors, parameters should be empty object
        let descs = mat.tool_descriptors_for_llm();
        let desc = descs
            .iter()
            .find(|d| d.name == ToolName::plain("deferred-tool"))
            .unwrap();
        assert!(desc.parameters.is_object());
        // Deferred tools should have empty parameters (no properties key)
        assert!(
            !desc
                .parameters
                .as_object()
                .unwrap()
                .contains_key("properties")
        );

        // Name and description should still be present
        assert_eq!(desc.name, ToolName::plain("deferred-tool"));
        assert!(!desc.description.is_empty());
    }

    #[test]
    fn hidden_tool_not_in_materialization() {
        let registry = ToolRegistry::new();
        insert_tool_with_exposure(
            &registry,
            "hidden-tool",
            ToolExposure::Hidden,
        );

        let mat = registry.materialize();

        // Hidden tool should NOT appear in materialization
        assert!(!mat.identities.contains_key(&ToolName::plain("hidden-tool")));
        assert!(!mat.tools.contains_key(&ToolName::plain("hidden-tool")));

        // Not in LLM descriptors either
        let descs = mat.tool_descriptors_for_llm();
        assert!(
            descs
                .iter()
                .all(|d| d.name != ToolName::plain("hidden-tool"))
        );
    }

    #[test]
    fn hidden_tool_still_resolvable_now() {
        let registry = ToolRegistry::new();
        insert_tool_with_exposure(
            &registry,
            "hidden-tool",
            ToolExposure::Hidden,
        );

        // Hidden tools should still be resolvable via resolve_now (programmatic access)
        let tool = registry.resolve_now("hidden-tool");
        assert!(tool.is_some());
        assert_eq!(tool.unwrap().name(), "hidden-tool");
    }

    #[test]
    fn mixed_exposure_tools_in_materialization() {
        let registry = ToolRegistry::new();
        insert_tool_with_exposure(
            &registry,
            "direct-tool",
            ToolExposure::Direct,
        );
        insert_tool_with_exposure(
            &registry,
            "deferred-tool",
            ToolExposure::Deferred,
        );
        insert_tool_with_exposure(
            &registry,
            "hidden-tool",
            ToolExposure::Hidden,
        );

        let mat = registry.materialize();

        // Only Direct and Deferred should appear
        assert!(mat.identities.contains_key(&ToolName::plain("direct-tool")));
        assert!(
            mat.identities
                .contains_key(&ToolName::plain("deferred-tool"))
        );
        assert!(!mat.identities.contains_key(&ToolName::plain("hidden-tool")));

        // Exposure map should reflect correct levels
        assert_eq!(
            mat.exposure_of(&ToolName::plain("direct-tool")),
            Some(ToolExposure::Direct)
        );
        assert_eq!(
            mat.exposure_of(&ToolName::plain("deferred-tool")),
            Some(ToolExposure::Deferred)
        );
        assert_eq!(mat.exposure_of(&ToolName::plain("hidden-tool")), None);

        // LLM descriptors should have 2 tools
        let descs = mat.tool_descriptors_for_llm();
        assert_eq!(descs.len(), 2);

        // Direct tool has full schema
        let direct_desc = descs
            .iter()
            .find(|d| d.name == ToolName::plain("direct-tool"))
            .unwrap();
        assert!(
            direct_desc
                .parameters
                .as_object()
                .unwrap()
                .contains_key("properties")
        );

        // Deferred tool has empty parameters
        let deferred_desc = descs
            .iter()
            .find(|d| d.name == ToolName::plain("deferred-tool"))
            .unwrap();
        assert!(
            !deferred_desc
                .parameters
                .as_object()
                .unwrap()
                .contains_key("properties")
        );
    }

    #[test]
    fn exposure_default_is_direct() {
        // Verify that ToolExposure::default() is Direct
        assert_eq!(ToolExposure::default(), ToolExposure::Direct);
    }
}
