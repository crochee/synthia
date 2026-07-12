//! Three-state extension lifecycle: `Loading` → `Active` → `Stale`.
//!
//! Extensions register themselves during the `Loading` phase (before the agent
//! loop starts running). At agent loop startup we call [`ExtensionContext::bind_core`]
//! which flushes the pending registrations into an [`ExtensionRuntime`] and
//! transitions the context to `Active`. If something invalidates the context
//! (e.g. a hot-swap of an extension's manifest), the context transitions to
//! `Stale` and subsequent operations fail with [`StaleContextError`].
//!
//! # Design rationale
//!
//! - **Single state machine, three states**: Loading is the *only* state in
//!   which `register_*` calls are accepted. Once bound, the runtime is
//!   immutable until invalidated. This matches the `P1 (prefix consistency)`
//!   principle — once a session's runtime is bound to the agent loop, the
//!   tool set should not change underfoot.
//! - **`assert_active` returns a typed error**: not `panic!`. Callers that
//!   need to ignore invalidation can match on the variant; callers that want
//!   fail-fast can `.unwrap_or_else(|e| panic!(...))`.
//! - **`invalidate` retains the last runtime** for diagnostics (the caller
//!   may want to log which extensions were bound when invalidation hit).

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use super::tool_provider::ToolProvider;

/// Stable session identifier. Reusing a `String` avoids a newtype dependency
/// and keeps the type compatible with `synthia_session::SessionId`.
pub type SessionId = String;

/// A registration that an extension can request during the `Loading` phase.
pub enum PendingRegistration {
    /// A tool provider to install.
    Tool(Arc<dyn ToolProvider>),
}

impl std::fmt::Debug for PendingRegistration {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Tool(t) => f.debug_tuple("Tool").field(&t.name()).finish(),
        }
    }
}

/// Errors returned by [`ExtensionContext`] state-machine operations.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum StaleContextError {
    /// The context is in the `Loading` state and has not been bound yet.
    #[error("ExtensionContext is in Loading state and not yet bound")]
    Loading,
    /// The context has been invalidated; see the reason for diagnostics.
    #[error("ExtensionContext is in Stale state: {0}")]
    Stale(String),
    /// The context is already bound; calling `bind_core` a second time fails.
    #[error("ExtensionContext is already bound")]
    AlreadyBound,
}

/// The three-state lifecycle of an extension context.
#[derive(Debug)]
pub enum ExtensionContext {
    /// Accepts `register_*` calls. The pending queue is flushed at `bind_core`.
    Loading {
        session_id: SessionId,
        pending: Vec<PendingRegistration>,
    },
    /// Bound to a runtime. All operations are dispatched to the runtime.
    Active {
        session_id: SessionId,
        runtime: Arc<ExtensionRuntime>,
    },
    /// Invalidated. Operations fail with [`StaleContextError::Stale`].
    Stale {
        reason: String,
        last_active: Option<Arc<ExtensionRuntime>>,
    },
}

impl ExtensionContext {
    /// Start a new context in the `Loading` state.
    pub fn new_loading(session_id: impl Into<SessionId>) -> Self {
        Self::Loading {
            session_id: session_id.into(),
            pending: Vec::new(),
        }
    }

    /// `true` if the context is in the `Loading` state.
    pub fn is_loading(&self) -> bool {
        matches!(self, Self::Loading { .. })
    }

    /// `true` if the context is in the `Active` state.
    pub fn is_active(&self) -> bool {
        matches!(self, Self::Active { .. })
    }

    /// `true` if the context is in the `Stale` state.
    pub fn is_stale(&self) -> bool {
        matches!(self, Self::Stale { .. })
    }

    /// Returns the bound session id, regardless of state.
    pub fn session_id(&self) -> &str {
        match self {
            Self::Loading { session_id, .. } => session_id,
            Self::Active { session_id, .. } => session_id,
            Self::Stale { last_active, .. } => last_active
                .as_ref()
                .map(|r| r.session_id.as_str())
                .unwrap_or(""),
        }
    }

    /// Register a tool provider. Only valid in the `Loading` state.
    ///
    /// Returns the registration index. Callers that need to undo the
    /// registration (e.g. on rollback) can keep the index for diagnostics.
    pub fn register_tool(
        &mut self,
        provider: Arc<dyn ToolProvider>,
    ) -> Result<usize, StaleContextError> {
        match self {
            Self::Loading { pending, .. } => {
                let idx = pending.len();
                pending.push(PendingRegistration::Tool(provider));
                Ok(idx)
            }
            Self::Active { .. } => Err(StaleContextError::AlreadyBound),
            Self::Stale { reason, .. } => {
                Err(StaleContextError::Stale(reason.clone()))
            }
        }
    }

    /// Number of pending registrations (Loading state only).
    pub fn pending_count(&self) -> usize {
        match self {
            Self::Loading { pending, .. } => pending.len(),
            _ => 0,
        }
    }

    /// Bind the pending queue into a runtime. Transitions to `Active`.
    ///
    /// Emits a `tracing::info_span!` named `extension.bind_core` with
    /// `session_id` and `provider_count` so OTel consumers can observe
    /// the lifecycle transition (P9 observability requirement). The
    /// span is a no-op without the `otel` feature.
    pub fn bind_core(self) -> Result<Self, StaleContextError> {
        match self {
            Self::Loading {
                session_id,
                pending,
            } => {
                let provider_count = pending.len();
                let _span = tracing::info_span!(
                    target: "synthia.extension",
                    "extension.bind_core",
                    session_id = session_id.as_str(),
                    provider_count = provider_count,
                )
                .entered();
                let runtime = Arc::new(ExtensionRuntime::from_pending(
                    &session_id,
                    pending,
                ));
                Ok(Self::Active {
                    session_id,
                    runtime,
                })
            }
            Self::Active { .. } => Err(StaleContextError::AlreadyBound),
            Self::Stale { reason, .. } => Err(StaleContextError::Stale(reason)),
        }
    }

    /// Returns a reference to the bound runtime.
    pub fn assert_active(
        &self,
    ) -> Result<&Arc<ExtensionRuntime>, StaleContextError> {
        match self {
            Self::Active { runtime, .. } => Ok(runtime),
            Self::Loading { .. } => Err(StaleContextError::Loading),
            Self::Stale { reason, .. } => {
                Err(StaleContextError::Stale(reason.clone()))
            }
        }
    }

    /// Invalidate the context, transitioning to `Stale`. Retains the last
    /// runtime (if any) for diagnostics.
    ///
    /// Emits a `tracing::info_span!` named `extension.invalidate` with
    /// `from_state` and `retained_runtime = true|false` so OTel
    /// consumers can observe the lifecycle transition (P9
    /// observability requirement).
    pub fn invalidate(&mut self, reason: impl Into<String>) {
        let last_active = match self {
            Self::Active { runtime, .. } => Some(runtime.clone()),
            _ => None,
        };
        let from_state = match self {
            Self::Loading { .. } => "loading",
            Self::Active { .. } => "active",
            Self::Stale { .. } => "stale",
        };
        let _span = tracing::info_span!(
            target: "synthia.extension",
            "extension.invalidate",
            from_state = from_state,
            retained_runtime = last_active.is_some(),
        )
        .entered();
        *self = Self::Stale {
            reason: reason.into(),
            last_active,
        };
    }
}

/// The runtime view exposed to the agent loop once an [`ExtensionContext`] is
/// bound. The runtime is the only thing the agent loop can see; the
/// `ExtensionContext` state machine wraps the runtime for safe lifecycle
/// transitions.
pub struct ExtensionRuntime {
    pub session_id: SessionId,
    pub tool_providers: Vec<Arc<dyn ToolProvider>>,
}

impl std::fmt::Debug for ExtensionRuntime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ExtensionRuntime")
            .field("session_id", &self.session_id)
            .field(
                "tool_providers",
                &self
                    .tool_providers
                    .iter()
                    .map(|p| p.name().to_string())
                    .collect::<Vec<_>>(),
            )
            .finish()
    }
}

impl ExtensionRuntime {
    /// Build a runtime from the pending queue. Currently only `Tool`
    /// registrations are honored; `Hook` registrations are forward-compat
    /// (Phase 5 will wire them).
    fn from_pending(
        session_id: &str,
        pending: Vec<PendingRegistration>,
    ) -> Self {
        let mut tool_providers = Vec::new();
        for p in pending {
            match p {
                PendingRegistration::Tool(t) => tool_providers.push(t),
            }
        }
        Self {
            session_id: session_id.to_string(),
            tool_providers,
        }
    }
}

/// Serialize/Deserialize stub for diagnostic snapshots. The runtime itself
/// is not serializable (it holds `Arc<dyn ToolProvider>`), so this only
/// captures the lifecycle state.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ExtensionContextSnapshot {
    Loading {
        session_id: SessionId,
        pending_count: usize,
    },
    Active {
        session_id: SessionId,
        tool_provider_count: usize,
    },
    Stale {
        reason: String,
        last_active_provider_count: Option<usize>,
    },
}

impl From<&ExtensionContext> for ExtensionContextSnapshot {
    fn from(ctx: &ExtensionContext) -> Self {
        match ctx {
            ExtensionContext::Loading {
                session_id,
                pending,
            } => Self::Loading {
                session_id: session_id.clone(),
                pending_count: pending.len(),
            },
            ExtensionContext::Active {
                session_id,
                runtime,
            } => Self::Active {
                session_id: session_id.clone(),
                tool_provider_count: runtime.tool_providers.len(),
            },
            ExtensionContext::Stale {
                reason,
                last_active,
            } => Self::Stale {
                reason: reason.clone(),
                last_active_provider_count: last_active
                    .as_ref()
                    .map(|r| r.tool_providers.len()),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use async_trait::async_trait;

    use super::*;
    use crate::tools::dynamic_provider::tool_provider::{
        SchemaRef,
        ToolDefinition,
        ToolProvider,
    };

    struct StubProvider {
        name: &'static str,
        tools: Vec<String>,
    }

    #[async_trait]
    impl ToolProvider for StubProvider {
        fn name(&self) -> &'static str {
            self.name
        }

        fn list_tools(&self) -> Vec<ToolDefinition> {
            self.tools
                .iter()
                .map(|t| ToolDefinition {
                    name: t.clone(),
                    description: format!("{} tool", t),
                    parameters: SchemaRef::Inline(serde_json::json!({
                        "type": "object",
                        "properties": {},
                    })),
                    deprecated: None,
                })
                .collect()
        }
    }

    fn stub(
        name: &'static str,
        tools: Vec<&'static str>,
    ) -> Arc<dyn ToolProvider> {
        Arc::new(StubProvider {
            name,
            tools: tools.into_iter().map(String::from).collect(),
        })
    }

    #[test]
    fn new_loading_starts_in_loading_state() {
        let ctx = ExtensionContext::new_loading("s1");
        assert!(ctx.is_loading());
        assert!(!ctx.is_active());
        assert!(!ctx.is_stale());
        assert_eq!(ctx.session_id(), "s1");
        assert_eq!(ctx.pending_count(), 0);
    }

    #[test]
    fn register_tool_accumulates_during_loading() {
        let mut ctx = ExtensionContext::new_loading("s1");
        let p1 = stub("p1", vec!["t1"]);
        let p2 = stub("p2", vec!["t2", "t3"]);
        assert_eq!(ctx.register_tool(p1).unwrap(), 0);
        assert_eq!(ctx.register_tool(p2).unwrap(), 1);
        assert_eq!(ctx.pending_count(), 2);
    }

    #[test]
    fn assert_active_fails_while_loading() {
        let ctx = ExtensionContext::new_loading("s1");
        let err = ctx.assert_active().unwrap_err();
        assert_eq!(err, StaleContextError::Loading);
    }

    #[test]
    fn assert_active_fails_when_stale() {
        let mut ctx = ExtensionContext::new_loading("s1");
        ctx.invalidate("hot-swap");
        let err = ctx.assert_active().unwrap_err();
        assert!(matches!(err, StaleContextError::Stale(r) if r == "hot-swap"));
    }

    #[test]
    fn bind_core_transitions_to_active_and_flushes_pending() {
        let mut ctx = ExtensionContext::new_loading("s1");
        ctx.register_tool(stub("p1", vec!["t1"])).unwrap();
        ctx.register_tool(stub("p2", vec!["t2", "t3"])).unwrap();

        let ctx = ctx.bind_core().unwrap();
        assert!(ctx.is_active());
        assert!(!ctx.is_loading());

        let runtime = ctx.assert_active().unwrap();
        assert_eq!(runtime.session_id, "s1");
        assert_eq!(runtime.tool_providers.len(), 2);
        assert_eq!(runtime.tool_providers[0].name(), "p1");
        assert_eq!(runtime.tool_providers[1].name(), "p2");
    }

    #[test]
    fn bind_core_with_empty_pending_still_binds() {
        let ctx = ExtensionContext::new_loading("s1");
        let ctx = ctx.bind_core().unwrap();
        assert!(ctx.is_active());
        assert_eq!(ctx.assert_active().unwrap().tool_providers.len(), 0);
    }

    #[test]
    fn double_bind_fails() {
        let ctx = ExtensionContext::new_loading("s1");
        let ctx = ctx.bind_core().unwrap();
        let err = ctx.bind_core().unwrap_err();
        assert_eq!(err, StaleContextError::AlreadyBound);
    }

    #[test]
    fn register_tool_after_bind_fails() {
        let mut ctx = ExtensionContext::new_loading("s1");
        ctx = ctx.bind_core().unwrap();
        let err = ctx.register_tool(stub("late", vec![])).unwrap_err();
        assert_eq!(err, StaleContextError::AlreadyBound);
    }

    #[test]
    fn invalidate_retains_last_active_runtime_for_diagnostics() {
        let mut ctx = ExtensionContext::new_loading("s1");
        ctx.register_tool(stub("p1", vec!["t1"])).unwrap();
        ctx = ctx.bind_core().unwrap();
        let original = ctx.assert_active().unwrap().clone();

        ctx.invalidate("hot-swap");
        assert!(ctx.is_stale());

        let snap = ExtensionContextSnapshot::from(&ctx);
        assert_eq!(
            snap,
            ExtensionContextSnapshot::Stale {
                reason: "hot-swap".to_string(),
                last_active_provider_count: Some(1),
            }
        );
        // The retained runtime still reflects the bound state.
        if let ExtensionContext::Stale { last_active, .. } = &ctx {
            assert_eq!(
                last_active.as_ref().unwrap().session_id,
                original.session_id
            );
        } else {
            panic!("expected Stale");
        }
    }

    #[test]
    fn invalidate_loading_state_has_no_last_active() {
        let mut ctx = ExtensionContext::new_loading("s1");
        ctx.invalidate("config-error");
        assert!(ctx.is_stale());
        if let ExtensionContext::Stale { last_active, .. } = &ctx {
            assert!(last_active.is_none());
        } else {
            panic!("expected Stale");
        }
    }

    #[test]
    fn register_tool_after_invalidate_fails_with_stale_error() {
        let mut ctx = ExtensionContext::new_loading("s1");
        ctx.invalidate("oops");
        let err = ctx.register_tool(stub("p", vec![])).unwrap_err();
        assert!(matches!(err, StaleContextError::Stale(r) if r == "oops"));
    }

    #[test]
    fn snapshot_round_trip_serializes_lifecycle_state() {
        let mut ctx = ExtensionContext::new_loading("s1");
        ctx.register_tool(stub("p1", vec!["t1"])).unwrap();
        let snap = ExtensionContextSnapshot::from(&ctx);
        let json = serde_json::to_string(&snap).unwrap();
        let back: ExtensionContextSnapshot =
            serde_json::from_str(&json).unwrap();
        assert_eq!(
            back,
            ExtensionContextSnapshot::Loading {
                session_id: "s1".to_string(),
                pending_count: 1,
            }
        );
    }

    // --- Phase 3.4: state machine concurrency / stress tests ---

    #[test]
    fn invalidate_loading_emits_no_retained_runtime_field() {
        // The snapshot must reflect that no runtime was bound.
        let mut ctx = ExtensionContext::new_loading("s1");
        ctx.invalidate("config-error");
        let snap = ExtensionContextSnapshot::from(&ctx);
        assert_eq!(
            snap,
            ExtensionContextSnapshot::Stale {
                reason: "config-error".to_string(),
                last_active_provider_count: None,
            }
        );
    }

    #[test]
    fn snapshot_after_bind_reflects_provider_count() {
        let mut ctx = ExtensionContext::new_loading("s1");
        ctx.register_tool(stub("p1", vec!["t1"])).unwrap();
        ctx.register_tool(stub("p2", vec!["t2", "t3"])).unwrap();
        ctx = ctx.bind_core().unwrap();
        let snap = ExtensionContextSnapshot::from(&ctx);
        assert_eq!(
            snap,
            ExtensionContextSnapshot::Active {
                session_id: "s1".to_string(),
                tool_provider_count: 2,
            }
        );
    }

    #[test]
    fn register_tool_in_order_preserves_provider_order() {
        // bind_core must flush in registration order — orchestrators
        // may rely on this for deterministic tool enumeration.
        let mut ctx = ExtensionContext::new_loading("s1");
        for i in 0..8 {
            let name: &'static str = match i {
                0 => "p0",
                1 => "p1",
                2 => "p2",
                3 => "p3",
                4 => "p4",
                5 => "p5",
                6 => "p6",
                _ => "p7",
            };
            ctx.register_tool(stub(name, vec![])).unwrap();
        }
        let ctx = ctx.bind_core().unwrap();
        let runtime = ctx.assert_active().unwrap();
        let names: Vec<String> = runtime
            .tool_providers
            .iter()
            .map(|p| p.name().to_string())
            .collect();
        assert_eq!(
            names,
            vec!["p0", "p1", "p2", "p3", "p4", "p5", "p6", "p7",]
                .into_iter()
                .map(String::from)
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn session_id_returns_empty_when_stale_without_prior_active() {
        let mut ctx = ExtensionContext::new_loading("");
        ctx.invalidate("never-bound");
        assert_eq!(ctx.session_id(), "");
    }

    #[test]
    fn session_id_returns_loading_session_id() {
        let ctx = ExtensionContext::new_loading("loading-id");
        assert_eq!(ctx.session_id(), "loading-id");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn concurrent_register_via_mutex_is_safe() {
        // ExtensionContext is not internally synchronized — callers
        // wrap it in a Mutex for concurrent registration. This test
        // exercises the typical pattern.
        use std::sync::Mutex;

        let ctx = std::sync::Arc::new(Mutex::new(
            ExtensionContext::new_loading("s1"),
        ));
        let mut handles = Vec::new();
        for i in 0..32 {
            let ctx = ctx.clone();
            handles.push(tokio::spawn(async move {
                let name: &'static str = match i % 4 {
                    0 => "p0",
                    1 => "p1",
                    2 => "p2",
                    _ => "p3",
                };
                let mut guard = ctx.lock().unwrap();
                guard.register_tool(stub(name, vec![])).unwrap();
            }));
        }
        for h in handles {
            h.await.unwrap();
        }
        let ctx = Arc::try_unwrap(ctx).unwrap().into_inner().unwrap();
        let ctx = ctx.bind_core().unwrap();
        // 32 registrations across 4 unique provider names.
        let runtime = ctx.assert_active().unwrap();
        assert_eq!(runtime.tool_providers.len(), 32);
    }
}
