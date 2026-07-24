//! `AgentDependencies` builder.
//!
//! Wires up the optional service-layer dependencies that an agent can be
//! constructed with: circuit breaker, loop detector, tool/hook registries,
//! and the high-level `DefaultContextService`, `Store`, and
//! `MemoryService` trait objects.
//!
//! Concrete types are stored behind `Arc<T>` (e.g. `ToolRegistry`,
//! `HookRegistry`, `CircuitBreaker`, `LoopDetectorSet`) while service-style
//! abstractions are stored as `Arc<T>` for `DefaultContextService`/`Store`
//! and `Arc<dyn Trait>` for `MemoryService` (still has multiple impls).
//!
//! `PersistenceService` trait REMOVED 2026-06-15 in change
//! `2026-06-15-p2-trait-cleanup`. `Store` (the concrete impl) is the
//! sole persistence backend and is stored as `Arc<SessionStore>`
//! (where `SessionStore` is a type alias for `synthia_session::Store`).

use std::sync::Arc;

use synthia_context::DefaultContextService;
use synthia_guardian::LoopDetectorSet;
use synthia_hook::HookRegistry;
use synthia_memory::service::MemoryService;
use synthia_session::Store as SessionStore;
use synthia_tool::ToolRegistry;

/// Collection of optional service dependencies an agent can be assembled with.
///
/// Each field starts as `None` and is populated via the corresponding
/// `with_*` builder method. The struct itself doubles as the builder and the
/// assembled value: call [`AgentDependencies::build`] once all desired
/// services have been wired up to obtain the final, ready-to-use instance.
pub struct AgentDependencies {
    /// Loop detector set for generic-repeat, ping-pong, poll-no-progress,
    /// and global circuit patterns.
    pub loop_detector: Option<Arc<LoopDetectorSet>>,
    /// Registry of tools the agent can invoke.
    pub tool_registry: Option<Arc<ToolRegistry>>,
    /// Registry of lifecycle hooks the agent fires.
    pub hook_registry: Option<Arc<HookRegistry>>,
    /// High-level context assembly / compaction service.
    ///
    /// Stored as the concrete `DefaultContextService` because the
    /// `ContextService` trait was REMOVED on 2026-06-15 (change
    /// `2026-06-15-p2-trait-cleanup`).
    pub context_service: Option<Arc<DefaultContextService>>,
    /// Persistence backend for session metadata, messages, and checkpoints.
    ///
    /// Stored as the concrete `SessionStore` (`synthia_session::Store`).
    /// The `PersistenceService` trait was REMOVED on 2026-06-15 in change
    /// `2026-06-15-p2-trait-cleanup`.
    pub persistence_service: Option<Arc<SessionStore>>,
    /// Memory subsystem facade (hot/cold/episodic stores, retrieval, consolidation).
    pub memory_service: Option<Arc<dyn MemoryService>>,
}

impl Default for AgentDependencies {
    fn default() -> Self {
        Self::new()
    }
}

impl AgentDependencies {
    /// Create an empty builder with every dependency unset.
    pub fn new() -> Self {
        Self {
            loop_detector: None,
            tool_registry: None,
            hook_registry: None,
            context_service: None,
            persistence_service: None,
            memory_service: None,
        }
    }

    /// Attach a loop detector set.
    pub fn with_loop_detector(
        mut self,
        loop_detector: Arc<LoopDetectorSet>,
    ) -> Self {
        self.loop_detector = Some(loop_detector);
        self
    }

    /// Attach a tool registry.
    pub fn with_tool_registry(
        mut self,
        tool_registry: Arc<ToolRegistry>,
    ) -> Self {
        self.tool_registry = Some(tool_registry);
        self
    }

    /// Attach a hook registry.
    pub fn with_hook_registry(
        mut self,
        hook_registry: Arc<HookRegistry>,
    ) -> Self {
        self.hook_registry = Some(hook_registry);
        self
    }

    /// Attach a context service implementation.
    pub fn with_context_service(
        mut self,
        context_service: Arc<DefaultContextService>,
    ) -> Self {
        self.context_service = Some(context_service);
        self
    }

    /// Attach a persistence service implementation.
    pub fn with_persistence_service(
        mut self,
        persistence_service: Arc<SessionStore>,
    ) -> Self {
        self.persistence_service = Some(persistence_service);
        self
    }

    /// Attach a memory service implementation.
    pub fn with_memory_service(
        mut self,
        memory_service: Arc<dyn MemoryService>,
    ) -> Self {
        self.memory_service = Some(memory_service);
        self
    }

    /// Finalize the builder and return the assembled dependencies.
    ///
    /// All fields remain optional; callers are expected to check for `None`
    /// (or to have populated them beforehand). The method consumes `self`
    /// to make ownership transfer explicit at the build site.
    pub fn build(self) -> Self {
        self
    }
}
