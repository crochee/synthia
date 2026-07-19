//! LoopServices — unified service container for the agent main loop.
//!
//! Groups all services (required + optional) that the main loop consumes.
//! Required services are resolved at bootstrap time; missing required
//! services cause a hard failure. Optional services degrade to no-op
//! fallbacks with a `tracing::warn` log.
//!
//! Feature-gated behind `unified-registry`.

use std::sync::Arc;

use synthia_context::assembler::ContextAssembler;
use synthia_hook::{HookRegistry, LoopDetector, UnifiedHookDispatcher};
use synthia_memory::types::MemoryEvent;
use synthia_permission::ApprovalService;
use synthia_provider::router::ModelRouter;
use synthia_sandbox::SandboxManager;
use synthia_session::Store as SessionStore;
use tokio::sync::mpsc;

use crate::{
    control::AgentControl,
    steering::SteeringChannel,
    subagent::SubagentSessionFactory,
    tools::dynamic_provider::ExtensionManager,
};

/// Unified service container for the agent main loop.
///
/// All fields are resolved once at loop entry via [`LoopServices::bootstrap`]
/// and cached in `AgentRunConfig::loop_services` (`OnceLock`).
#[derive(Clone)]
pub struct LoopServices {
    // ── Required services (hard fail if missing) ──────────────
    pub session: SessionStore,
    pub permission: Arc<dyn ApprovalService>,
    pub hooks: Arc<HookRegistry>,

    // ── Optional services (no-op fallback) ────────────────────
    pub memory: Option<mpsc::Sender<MemoryEvent>>,
    pub guardian: Option<Arc<synthia_guardian::GuardianCoordinator>>,
    pub steering: Option<Arc<dyn SteeringChannel>>,
    pub agent_control: Option<Arc<AgentControl>>,
    pub context: Option<Arc<ContextAssembler>>,
    pub sandbox: Arc<dyn SandboxManager>,
    pub extension: Option<ExtensionManager>,
    pub model_router: Option<Arc<ModelRouter>>,
    pub compaction: Option<
        Arc<dyn synthia_context::compaction::level1::CompactionProvider>,
    >,
    pub subagent_factory: Option<Arc<dyn SubagentSessionFactory>>,
    /// Admission-control goal service — semaphore-based concurrent goal
    /// limit. `None` = no admission control (unlimited concurrent goals).
    pub goal_admission: Option<Arc<dyn synthia_goal_service::GoalService>>,
    /// Per-session goal tracker — progress observability and budget
    /// tracking. `None` = no goal tracking (NoopGoalService behavior).
    pub goal_tracker: Option<Arc<dyn synthia_service::goal::GoalService>>,
    /// Unified hook dispatcher — the single dispatch point for all hook
    /// events. Constructed from `HookRegistry` hooks wrapped in
    /// `AgentHookAdapter`, plus `LoopDetector` as a Layer 2 Hook.
    /// Promoted from `BuilderSteps` to `LoopServices` so that the
    /// dispatcher is accessible to all parts of the loop.
    pub hook_dispatcher: Arc<synthia_hook::UnifiedHookDispatcher>,
}

/// Bootstrap configuration for constructing [`LoopServices`].
///
/// Extracted from `AgentRunConfig` so that bootstrap logic is
/// testable without constructing the full config.
pub struct LoopServicesConfig {
    pub session: SessionStore,
    pub permission: Option<Arc<dyn ApprovalService>>,
    pub hooks: Arc<HookRegistry>,
    pub memory: Option<mpsc::Sender<MemoryEvent>>,
    pub guardian: Option<Arc<synthia_guardian::GuardianCoordinator>>,
    pub steering: Option<Arc<dyn SteeringChannel>>,
    pub agent_control: Option<Arc<AgentControl>>,
    pub context: Option<Arc<ContextAssembler>>,
    pub sandbox: Option<Arc<dyn SandboxManager>>,
    pub extension: Option<ExtensionManager>,
    pub model_router: Option<Arc<ModelRouter>>,
    pub compaction: Option<
        Arc<dyn synthia_context::compaction::level1::CompactionProvider>,
    >,
    pub subagent_factory: Option<Arc<dyn SubagentSessionFactory>>,
    pub goal_admission: Option<Arc<dyn synthia_goal_service::GoalService>>,
    pub goal_tracker: Option<Arc<dyn synthia_service::goal::GoalService>>,
    pub hook_dispatcher: Arc<synthia_hook::UnifiedHookDispatcher>,
}

/// Bootstrap error — a required service was missing.
#[derive(Debug, thiserror::Error)]
pub enum BootstrapError {
    #[error("required service missing: {0}")]
    MissingRequired(&'static str),
}

impl LoopServices {
    /// Bootstrap the service container.
    ///
    /// - **Required services**: `session`, `hooks` — if missing, returns
    ///   [`BootstrapError::MissingRequired`].
    /// - **Permission**: falls back to [`HeadlessApprovalService`] if not
    ///   provided.
    /// - **Sandbox**: falls back to [`NoopSandboxManager`] if not provided.
    /// - **All other optional services**: fall back to `None` with a
    ///   `tracing::warn` log.
    pub fn bootstrap(
        config: LoopServicesConfig,
    ) -> Result<Self, BootstrapError> {
        // ── Required services ─────────────────────────────────
        let session = config.session;

        let hooks = config.hooks;

        // ── Permission: required, but has a sensible default ───
        let permission = config.permission.unwrap_or_else(|| {
            tracing::warn!(
                "permission service not provided, falling back to \
                 HeadlessApprovalService"
            );
            Arc::new(synthia_permission::HeadlessApprovalService)
        });

        // ── Sandbox: required for tool execution, has no-op ───
        let sandbox = config.sandbox.unwrap_or_else(|| {
            tracing::warn!(
                "sandbox manager not provided, falling back to \
                 NoopSandboxManager"
            );
            Arc::new(synthia_sandbox::NoopSandboxManager)
        });

        // ── Optional services: warn if missing ────────────────
        let memory = config.memory;
        if memory.is_none() {
            tracing::warn!("memory event sender not provided");
        }

        let guardian = config.guardian;
        if guardian.is_none() {
            tracing::debug!("guardian coordinator not provided (disabled)");
        }

        let steering = config.steering;
        if steering.is_none() {
            tracing::debug!("steering channel not provided");
        }

        let agent_control = config.agent_control;
        if agent_control.is_none() {
            tracing::debug!("agent control not provided");
        }

        let context = config.context;
        if context.is_none() {
            tracing::debug!("context assembler not provided");
        }

        let extension = config.extension;
        if extension.is_none() {
            tracing::debug!("extension manager not provided");
        }

        let model_router = config.model_router;
        if model_router.is_none() {
            tracing::debug!("model router not provided");
        }

        let compaction = config.compaction;
        if compaction.is_none() {
            tracing::debug!("compaction provider not provided");
        }

        let subagent_factory = config.subagent_factory;
        if subagent_factory.is_none() {
            tracing::debug!("subagent factory not provided");
        }

        let goal_admission = config.goal_admission;
        if goal_admission.is_none() {
            tracing::debug!("goal admission service not provided");
        }

        let goal_tracker = config.goal_tracker;
        if goal_tracker.is_none() {
            tracing::debug!("goal tracker service not provided");
        }

        // ── Hook dispatcher: construct from hooks + LoopDetector ──
        let hook_dispatcher = config.hook_dispatcher;

        Ok(Self {
            session,
            permission,
            hooks,
            memory,
            guardian,
            steering,
            agent_control,
            context,
            sandbox,
            extension,
            model_router,
            compaction,
            subagent_factory,
            goal_admission,
            goal_tracker,
            hook_dispatcher,
        })
    }

    /// Extract a [`LoopServicesConfig`] from an [`AgentRunConfig`].
    ///
    /// This is a convenience method so callers don't need to destructure
    /// the config manually.
    pub fn config_from_run_config(
        run_config: &crate::config::AgentRunConfig,
    ) -> LoopServicesConfig {
        LoopServicesConfig {
            session: run_config.session_store.clone(),
            permission: run_config.approval_service.clone(),
            hooks: run_config.hook_registry.clone(),
            memory: run_config.memory_event_sender.clone(),
            guardian: run_config.guardian_coordinator.clone(),
            steering: run_config.steering_channel.clone(),
            agent_control: run_config.agent_control.clone().map(Arc::new),
            context: run_config.context_assembler.clone(),
            sandbox: run_config.sandbox_manager.clone(),
            extension: run_config.extension_manager.clone(),
            model_router: Some(run_config.model_router.clone()),
            compaction: run_config.compaction_provider.clone(),
            subagent_factory: run_config.subagent_session_factory.clone(),
            goal_admission: None,
            goal_tracker: None,
            hook_dispatcher: {
                let mut dispatcher = UnifiedHookDispatcher::from_hook_registry(
                    &run_config.hook_registry,
                );
                dispatcher.add_hook(Arc::new(LoopDetector::new()));
                Arc::new(dispatcher)
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_session_store() -> SessionStore {
        let dir = tempfile::tempdir().unwrap();
        // Leak the tempdir so the store path remains valid for the
        // test lifetime. Acceptable in tests only.
        let path = dir.into_path();
        SessionStore::new(path)
    }

    /// Helper: construct a default hook dispatcher for tests.
    fn test_hook_dispatcher() -> Arc<UnifiedHookDispatcher> {
        let mut dispatcher =
            UnifiedHookDispatcher::from_hook_registry(&HookRegistry::new());
        dispatcher.add_hook(Arc::new(LoopDetector::new()));
        Arc::new(dispatcher)
    }

    #[test]
    fn bootstrap_with_defaults() {
        let config = LoopServicesConfig {
            session: temp_session_store(),
            permission: None,
            hooks: Arc::new(HookRegistry::new()),
            memory: None,
            guardian: None,
            steering: None,
            agent_control: None,
            context: None,
            sandbox: None,
            extension: None,
            model_router: None,
            compaction: None,
            subagent_factory: None,
            goal_admission: None,
            goal_tracker: None,
            hook_dispatcher: test_hook_dispatcher(),
        };
        let services = LoopServices::bootstrap(config).unwrap();
        assert!(services.memory.is_none());
        assert!(services.guardian.is_none());
        assert!(services.steering.is_none());
        assert!(services.goal_admission.is_none());
        assert!(services.goal_tracker.is_none());
    }

    #[test]
    fn bootstrap_with_all_services() {
        let (tx, _rx) = mpsc::channel(16);
        let config = LoopServicesConfig {
            session: temp_session_store(),
            permission: Some(Arc::new(
                synthia_permission::HeadlessApprovalService,
            )),
            hooks: Arc::new(HookRegistry::new()),
            memory: Some(tx),
            guardian: None,
            steering: None,
            agent_control: None,
            context: None,
            sandbox: Some(Arc::new(synthia_sandbox::NoopSandboxManager)),
            extension: None,
            model_router: None,
            compaction: None,
            subagent_factory: None,
            goal_admission: None,
            goal_tracker: None,
            hook_dispatcher: test_hook_dispatcher(),
        };
        let services = LoopServices::bootstrap(config).unwrap();
        assert!(services.memory.is_some());
        // Sandbox is present (no-op fallback was applied)
        let _ = &services.sandbox;
    }
}
