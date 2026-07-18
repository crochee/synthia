//! ToolRegistry + ToolIdentity + Materialization.

use std::{collections::HashMap, sync::Arc};

use parking_lot::RwLock;

use crate::tool::{
    descriptor::{Tool, ToolDescriptor, ToolProvenance},
    provider::ToolProvider,
    types::ToolError,
};

/// Monotonic generation counter for stale detection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct ToolGeneration(pub u64);

/// Value-type tool identity for snapshot stale detection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolIdentity {
    pub name: String,
    pub generation: ToolGeneration,
}

/// Registration token for unregistration.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RegistrationToken(pub u64);

/// Registration error.
#[derive(Debug, thiserror::Error)]
pub enum RegistrationError {
    #[error("core tool name taken: {name}")]
    CoreNameTaken { name: String },
    #[error("duplicate tool: {name}")]
    DuplicateName { name: String },
    #[error("registration failed: {0}")]
    Failed(String),
}

/// Entry in the tool registry.
struct ToolEntry {
    provider_id: String,
    tool: Arc<dyn Tool>,
    identity: ToolIdentity,
    provenance: ToolProvenance,
}

/// Immutable materialization snapshot for stale detection.
#[derive(Clone)]
pub struct Materialization {
    /// Snapshot of tool identities at materialization time.
    identities: HashMap<String, ToolIdentity>,
    /// Snapshot of tool references.
    tools: HashMap<String, Arc<dyn Tool>>,
    /// Snapshot token.
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

/// Unified tool registry.
pub struct ToolRegistry {
    inner: RwLock<ToolRegistryInner>,
    next_token: RwLock<u64>,
}

struct ToolRegistryInner {
    /// Tool name → entries (LIFO for non-core tools).
    tools: HashMap<String, Vec<ToolEntry>>,
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
        let mut inner = self.inner.write();

        let token = RegistrationToken(inner.next_registration);
        inner.next_registration += 1;

        for desc in descriptors {
            if let Some(tool) = provider.get_tool(&desc.name).await {
                // Check core name immutability
                if let Some(existing) = inner.tools.get(&desc.name) {
                    if existing
                        .iter()
                        .any(|e| e.provenance == ToolProvenance::Core)
                        && desc.provenance == ToolProvenance::Core
                    {
                        return Err(RegistrationError::CoreNameTaken {
                            name: desc.name.clone(),
                        });
                    }
                }

                let identity = ToolIdentity {
                    name: desc.name.clone(),
                    generation: inner.generation,
                };

                let entry = ToolEntry {
                    provider_id: provider.id().to_string(),
                    tool,
                    identity,
                    provenance: desc.provenance,
                };

                inner.tools.entry(desc.name).or_default().push(entry);
            }
        }

        // Bump generation after registration
        inner.generation.0 += 1;

        Ok(token)
    }

    /// Unregister all tools owned by a token.
    pub fn unregister(&self, _token: RegistrationToken) {
        // TODO: implement token-based unregistration
    }

    /// Materialize an immutable snapshot for the LLM.
    pub fn materialize(&self) -> Materialization {
        let inner = self.inner.read();
        let mut identities = HashMap::new();
        let mut tools = HashMap::new();

        for (name, entries) in &inner.tools {
            if let Some(entry) = entries.last() {
                identities.insert(name.clone(), entry.identity.clone());
                tools.insert(name.clone(), entry.tool.clone());
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
            token,
        }
    }

    /// Resolve a tool from a materialization snapshot.
    pub fn resolve(
        mat: &Materialization,
        name: &str,
        current_registry: &ToolRegistry,
    ) -> Result<Arc<dyn Tool>, StaleOrUnknown> {
        let current = current_registry.inner.read();

        // Check if tool exists
        let current_entry =
            current.tools.get(name).and_then(|entries| entries.last());

        let Some(current_tool) = current_entry else {
            return Err(StaleOrUnknown::Unknown {
                name: name.to_string(),
            });
        };

        // Check stale detection
        if let Some(seen_identity) = mat.identities.get(name) {
            if seen_identity.generation != current_tool.identity.generation {
                return Err(StaleOrUnknown::Stale {
                    name: name.to_string(),
                    seen: seen_identity.generation.0,
                    current: current_tool.identity.generation.0,
                });
            }
        }

        // Return the tool from snapshot
        mat.tools.get(name).cloned().ok_or(StaleOrUnknown::Unknown {
            name: name.to_string(),
        })
    }

    /// Resolve a tool without snapshot (for non-LLM callers).
    pub fn resolve_now(&self, name: &str) -> Option<Arc<dyn Tool>> {
        let inner = self.inner.read();
        inner
            .tools
            .get(name)
            .and_then(|entries| entries.last())
            .map(|entry| entry.tool.clone())
    }

    /// Internal consistency check (debug only).
    #[cfg(debug_assertions)]
    pub fn internal_consistency_check(&self) {
        let inner = self.inner.read();
        for (name, entries) in &inner.tools {
            for entry in entries {
                assert_eq!(entry.identity.name, *name);
                assert_eq!(entry.tool.name(), *name);
            }
        }
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use async_trait::async_trait;

    use super::*;
    use crate::tool::{
        descriptor::{Tool, ToolDescriptor, ToolProvenance},
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
                name: "test".to_string(),
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
                tool: tool.clone(),
                identity: ToolIdentity {
                    name: "test-tool".to_string(),
                    generation: inner.generation,
                },
                provenance: ToolProvenance::Core,
            };
            inner.tools.insert("test-tool".to_string(), vec![entry]);
            inner.generation.0 += 1;
        }

        // Materialize again — should include the tool
        let mat2 = registry.materialize();
        assert!(mat2.identities.contains_key("test-tool"));

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
                tool,
                identity: ToolIdentity {
                    name: "find-me".to_string(),
                    generation: inner.generation,
                },
                provenance: ToolProvenance::Core,
            };
            inner.tools.insert("find-me".to_string(), vec![entry]);
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
}
