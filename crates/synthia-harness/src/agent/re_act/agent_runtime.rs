//! Runtime-wiring setters on [`super::ReActAgent`].
//!
//! Every setter in this file rewires the loop: tool registry,
//! steering, context manager, typed event sink, run inbox, clock,
//! executor, strategy, surface / restriction policy, interceptors,
//! and the LLM-backed compaction policy. They do not change the
//! agent's descriptor — that lives in
//! [`super::agent_descriptor`].
//!
//! Two private helpers belong here too:
//!
//! - [`ReActAgent::effective_context_manager`] — the manager one run
//!   actually uses. Reads `compaction_settings + typed_sink +
//!   compaction_checkpoint`, auto-builds a `CompactionCheckpoint` from
//!   the sink when both policy + sink are present and no external
//!   checkpoint takes precedence, then asks
//!   [`crate::compaction::resolve`] for the context manager. Resolving
//!   per run (instead of in the setters) is what makes setter **order**
//!   unable to change the outcome.
//! - [`ReActAgent::runtime_for_run`] — assembles one
//!   [`AgentRuntime`] per dispatched session, the bag the
//!   `ReasoningStrategy::run` actually receives.

use std::{path::PathBuf, sync::Arc};

use synthia_context::ContextManager;
use synthia_session::TypedEventSink;
use synthia_steering::Steering;
use synthia_tool::{ToolRegistry, ToolRestriction, ToolSurfacePolicy};

use super::ReActAgent;
use crate::agent::{
    RunInbox,
    descriptor::AgentDescriptor,
    interceptor::ToolInterceptor,
    strategy::{AgentRuntime, ReasoningStrategy},
};

impl ReActAgent {
    /// Replace the working directory handed to built-in tools as
    /// `Context::working_dir`. Defaults to an empty path.
    #[must_use]
    pub fn with_workspace(mut self, root: impl Into<PathBuf>) -> Self {
        self.workspace_root = root.into();
        self
    }

    /// Replace the tool registry. The new registry is what the
    /// harness advertises and dispatches against from the next
    /// `run` onward.
    #[must_use]
    pub fn with_tool_registry(mut self, registry: Arc<ToolRegistry>) -> Self {
        self.tool_registry = registry;
        self
    }

    /// Replace the context-window manager used to prune the
    /// message list before each LLM call.
    ///
    /// This is the manager the run uses **unless** the deployment
    /// installed an enabled + valid compaction policy
    /// ([`Self::with_compaction_settings`]), which replaces it with the
    /// provider-backed summariser. Setter order is irrelevant: the
    /// decision is made per run.
    pub fn with_context_manager(
        mut self,
        manager: Arc<dyn ContextManager>,
    ) -> Self {
        self.context_manager = manager;
        self
    }

    /// Borrow the context-window manager this agent was assembled
    /// with, before the compaction policy is applied. See
    /// [`Self::with_context_manager`] for which manager a run actually
    /// gets.
    pub fn context_manager(&self) -> &Arc<dyn ContextManager> {
        &self.context_manager
    }

    /// Install a synthetic-tool plugin the loop dispatches ahead
    /// of the registry.
    #[must_use]
    pub fn with_interceptor(
        mut self,
        interceptor: Arc<dyn ToolInterceptor>,
    ) -> Self {
        self.interceptors.push(interceptor);
        self
    }

    /// The installed synthetic-tool plugins, in installation order.
    pub fn interceptors(&self) -> &[Arc<dyn ToolInterceptor>] {
        &self.interceptors
    }

    /// Override the per-run iteration cap. The value is clamped
    /// to `[1, 4096]`.
    pub fn with_max_iterations(mut self, max_iterations: usize) -> Self {
        self.max_iterations = max_iterations.clamp(1, 4096);
        self
    }

    /// Borrow the configured iteration cap.
    pub fn max_iterations(&self) -> usize {
        self.max_iterations
    }

    /// Install a deployment-level [`ToolSurfacePolicy`].
    pub fn with_tool_surface(mut self, policy: ToolSurfacePolicy) -> Self {
        self.tool_surface = Some(Arc::new(policy));
        self
    }

    /// Install this agent's own allow/deny list (R58).
    #[must_use]
    pub fn with_tool_restriction(
        mut self,
        restriction: ToolRestriction,
    ) -> Self {
        self.tool_restriction = Some(Arc::new(restriction));
        self
    }

    /// Install the typed-event sink (R6-A).
    pub fn with_typed_event_sink(mut self, sink: TypedEventSink) -> Self {
        self.typed_sink = Some(sink);
        self
    }

    /// Install the steering bundle.
    pub fn with_steering(mut self, steering: Arc<Steering>) -> Self {
        self.steering = steering;
        self
    }

    /// Borrow the current steering bundle.
    pub fn steering(&self) -> &Steering {
        &self.steering
    }

    /// Install an interactive [`RunInbox`] (pi agent-loop queue parity).
    pub fn with_run_inbox(mut self, inbox: Arc<dyn RunInbox>) -> Self {
        self.run_inbox = Some(inbox);
        self
    }

    /// Borrow the configured run inbox, if any.
    pub fn run_inbox(&self) -> Option<&Arc<dyn RunInbox>> {
        self.run_inbox.as_ref()
    }

    /// Inject a clock used to stamp the runtime-context snapshot.
    /// Default: [`synthia_core::SystemClock`].
    pub fn with_clock(mut self, clock: synthia_core::SharedClock) -> Self {
        self.clock = clock;
        self
    }

    /// Borrow the configured clock.
    pub fn clock(&self) -> &synthia_core::SharedClock {
        &self.clock
    }

    /// Install the executor that detached runs go to.
    #[must_use]
    pub fn with_spawner(
        mut self,
        spawner: Arc<dyn synthia_core::spawn::Spawner>,
    ) -> Self {
        self.spawner = spawner;
        self
    }

    /// Borrow the configured spawner.
    pub fn spawner(&self) -> &Arc<dyn synthia_core::spawn::Spawner> {
        &self.spawner
    }

    /// Install the reasoning loop.
    #[must_use]
    pub fn with_strategy(
        mut self,
        strategy: Arc<dyn ReasoningStrategy>,
    ) -> Self {
        self.strategy = strategy;
        self
    }

    /// The installed strategy.
    pub fn strategy(&self) -> &Arc<dyn ReasoningStrategy> {
        &self.strategy
    }

    /// Install an LLM-backed compaction policy.
    ///
    /// - `None` / disabled / invalid → the manager installed with
    ///   [`Self::with_context_manager`] (default
    ///   [`synthia_context::TruncatingContextManager`]) is kept
    ///   (with a `warn!` log for the invalid case).
    /// - enabled + valid → that manager is **replaced** by the
    ///   provider-backed [`synthia_context::SummarizingContextManager`].
    ///
    /// If a typed event sink has also been installed (see
    /// [`Self::with_typed_event_sink`]) AND no externally-owned
    /// checkpoint has been installed (see
    /// [`Self::with_compaction_checkpoint`]), every run auto-builds a
    /// fresh [`synthia_session::CompactionCheckpoint`] from the sink
    /// and wires the emitters into the summarising manager — so a run
    /// that compacts writes `compaction_start` / `compaction_summary` /
    /// `compaction_end` rows through the durable channel.
    ///
    /// The decision is made **per run**, not here, so the order of
    /// this call relative to [`Self::with_context_manager`] and
    /// [`Self::with_typed_event_sink`] does not matter: a policy that
    /// is enabled + valid always wins over the caller's manager, and
    /// one that is disabled or invalid always leaves it in place.
    #[must_use]
    pub fn with_compaction_settings(
        mut self,
        settings: synthia_context::CompactionSettings,
    ) -> Self {
        self.compaction_settings = Some(settings);
        self
    }

    /// Install an externally owned compaction checkpoint.
    ///
    /// The checkpoint carries its own sink and ledger, so
    /// [`Self::with_typed_event_sink`] and the auto-built checkpoint
    /// logic of [`Self::with_compaction_settings`] are not consulted
    /// for it. Holding the handle lets a caller flush parked
    /// records after the run. An external checkpoint takes precedence
    /// over the auto-built one whichever setter ran first.
    #[must_use]
    pub fn with_compaction_checkpoint(
        mut self,
        checkpoint: Arc<synthia_session::CompactionCheckpoint>,
    ) -> Self {
        self.compaction_checkpoint = Some(checkpoint);
        self
    }

    /// The installed compaction policy, if any.
    pub fn compaction_settings(
        &self,
    ) -> Option<&synthia_context::CompactionSettings> {
        self.compaction_settings.as_ref()
    }

    /// The installed externally owned compaction checkpoint, if
    /// any.
    pub fn compaction_checkpoint(
        &self,
    ) -> Option<&Arc<synthia_session::CompactionCheckpoint>> {
        self.compaction_checkpoint.as_ref()
    }

    /// The compaction emitters this agent's runs install on the
    /// summarising manager: an auto-built
    /// [`synthia_session::CompactionCheckpoint`] over the typed sink,
    /// when the deployment supplied one of each. An externally owned
    /// checkpoint ([`Self::with_compaction_checkpoint`]) carries its
    /// own sink and ledger, so nothing is built for it.
    fn compaction_emitters(
        &self,
    ) -> Option<crate::compaction::CompactionEmitters> {
        match (
            self.compaction_settings.as_ref(),
            self.typed_sink.as_ref(),
            self.compaction_checkpoint.as_ref(),
        ) {
            (Some(_), Some(sink), None) => {
                let checkpoint = synthia_session::CompactionCheckpoint::new(
                    sink.clone(),
                    Arc::new(synthia_session::SurfaceLedger::new()),
                );
                Some(crate::compaction::CompactionEmitters::new(
                    checkpoint.record_callback(),
                    checkpoint.lifecycle_callback(),
                ))
            }
            _ => None,
        }
    }

    /// The context manager one run actually uses.
    ///
    /// Resolved here rather than in the setters so **setter order
    /// cannot change the outcome**: the compaction policy decides
    /// between the caller's own manager
    /// ([`Self::with_context_manager`]) and a provider-backed
    /// summariser, and a caller that installs them in either order
    /// gets the same decision.
    fn effective_context_manager(&self) -> Arc<dyn ContextManager> {
        crate::compaction::resolve(
            self.compaction_settings,
            Arc::clone(&self.context_manager),
            &self.provider,
            self.compaction_emitters(),
        )
    }

    /// The tool definitions this agent would send to the model for
    /// `messages`, without running a session.
    #[must_use]
    pub fn projected_tool_definitions(
        &self,
        messages: &[synthia_provider::Message],
    ) -> Vec<synthia_provider::ToolDefinition> {
        let descriptors = self.tool_registry.descriptors_cached();
        let called = super::promoted_tool_names(&descriptors, messages);
        super::compose_tool_definitions(
            &self.tool_registry,
            self.tool_surface.as_deref(),
            self.tool_restriction.as_deref(),
            &self.interceptors,
            &called,
        )
    }

    /// Assemble this run's [`AgentRuntime`].
    pub(super) fn runtime_for_run(
        &self,
        cancel: Arc<dyn synthia_core::CancelToken>,
        subagent_depth: usize,
    ) -> AgentRuntime {
        AgentRuntime {
            provider: Arc::clone(&self.provider),
            tool_registry: Arc::clone(&self.tool_registry),
            tool_surface: self.tool_surface.clone(),
            tool_restriction: self.tool_restriction.clone(),
            workspace_root: self.workspace_root.clone(),
            descriptor: AgentDescriptor::clone(&self.descriptor),
            prompt_context: Arc::clone(&self.prompt_context),
            context_manager: self.effective_context_manager(),
            interceptors: self.interceptors.clone(),
            steering: Arc::clone(&self.steering),
            max_iterations: self.max_iterations,
            typed_sink: self.typed_sink.clone(),
            inbox: self.run_inbox.clone(),
            clock: self.clock.clone(),
            spawner: Arc::clone(&self.spawner),
            cancel,
            subagent_depth,
        }
    }
}
