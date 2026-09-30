//! The run-stream factory seam: how the controller obtains an agent
//! run's event stream, and the production impl over
//! [`synthia::harness::Agent`].

use std::{pin::Pin, sync::Arc};

use futures::{Stream, StreamExt};
use synthia::{
    harness::{Agent, AgentEvent, AgentInput, AgentRunConfig, ReActAgent},
    tool_task::TaskDelegator,
};
use tokio_util::sync::CancellationToken;

/// Factory abstraction so the controller can be unit-tested without
/// starting a real agent run.
pub trait RunStreamFactory: Send + Sync + 'static {
    fn run_stream(
        &self,
        config: AgentRunConfig,
        input: AgentInput,
        cancel: Arc<CancellationToken>,
    ) -> Pin<Box<dyn Stream<Item = AgentEvent> + Send + 'static>>;
}

/// Production implementation that delegates to
/// [`synthia::harness::Agent`].
#[derive(Debug, Clone, Default)]
pub struct AgentRunStreamFactory;

impl RunStreamFactory for AgentRunStreamFactory {
    fn run_stream(
        &self,
        config: AgentRunConfig,
        input: AgentInput,
        cancel: Arc<CancellationToken>,
    ) -> Pin<Box<dyn Stream<Item = AgentEvent> + Send + 'static>> {
        // Resolve the descriptor through the dispatcher when
        // the caller supplied an `agent_resolver` + `agent_name`.
        // The legacy path (no resolver) keeps using the
        // `system_prompt` as the base instructions.
        let provider = Arc::clone(&config.provider);
        let tool_registry = Arc::clone(&config.tool_registry);
        let workspace_root = config.workspace_root.clone();
        let system_prompt = config.system_prompt.clone();
        let prompt_context = config.prompt_context.clone();
        let resolver = config.agent_resolver.clone();
        let resolved_descriptor = if let (Some(r), Some(n)) =
            (resolver.as_ref(), config.agent_name.as_ref())
        {
            match r(n.clone()) {
                Some(d) => Some(d),
                None => {
                    tracing::warn!(
                        agent_name = %n,
                        "agent_resolver returned None; falling back to default descriptor"
                    );
                    None
                }
            }
        } else {
            None
        };
        // Multi-agent orchestration: panel / role fields were
        // removed from `AgentDescriptor`, so the run factory
        // always builds a single `ReActAgent`. A caller wanting
        // multi-agent orchestration composes its own fan-out
        // on top of the returned event stream — there is no
        // built-in panel coordinator in the agent runtime.

        // Build the agent with the assembled prompt context
        // (skills + peer agents + tool manifest) so the system
        // prompt that reaches the LLM carries the full
        // industry-aligned manifest, not just the base
        // instructions.
        let base = match resolved_descriptor {
            Some(descriptor) => ReActAgent::with_descriptor(
                provider,
                tool_registry,
                workspace_root,
                descriptor,
                prompt_context,
            )
            .with_steering(Arc::clone(&config.steering))
            .with_max_iterations(
                config
                    .max_iterations
                    .unwrap_or(synthia::harness::DEFAULT_MAX_ITERATIONS),
            ),
            None => ReActAgent::with_prompt_context(
                provider,
                tool_registry,
                workspace_root,
                system_prompt,
                prompt_context,
            )
            .with_steering(Arc::clone(&config.steering))
            .with_max_iterations(
                config
                    .max_iterations
                    .unwrap_or(synthia::harness::DEFAULT_MAX_ITERATIONS),
            ),
        };

        // R34: install the deployment's `[tools]` surface policy —
        // groups + the `max_visible` cap — on the run's agent. `None`
        // leaves the R33 projection untouched.
        let base = match config.tool_surface {
            Some(policy) => base.with_tool_surface(policy),
            None => base,
        };

        // R50: install the deployment's reasoning loop. The factory is
        // the only place a run's agent is assembled, so this is the one
        // hop between `agents.<name>.strategy` and the loop that runs.
        let base = match config.strategy {
            Some(strategy) => base.with_strategy(strategy),
            None => base,
        };

        // R58: and the agent's own allow/deny list, which is what makes
        // `denied_tools` an enforced control rather than an
        // advertisement hint.
        let base = match config.tool_restriction {
            Some(restriction) => {
                base.with_tool_restriction((*restriction).clone())
            }
            None => base,
        };

        // R6-A/B: forward the typed-event sink (when the
        // controller created one) so the loop's structural
        // boundary events reach the drain task.
        let base = match config.typed_event_sink {
            Some(sink) => base.with_typed_event_sink(sink),
            None => base,
        };

        // R22: the caller composes the context manager (compaction
        // or a custom chain) and hands it in; the
        // factory just installs it. `None` keeps the loop default.
        let base = match config.context_manager.as_ref() {
            Some(manager) => base.with_context_manager(Arc::clone(manager)),
            None => base,
        };

        // Sub-agent delegation: when the controller resolved this
        // run through the multi-agent registry, install the `task`
        // tool plugin so its model can delegate. Registered peers
        // become the delegable set.
        let agent = match config.agent_registry.as_ref() {
            Some(registry) => Arc::new(base.with_interceptor(Arc::new(
                TaskDelegator::new(Arc::clone(registry)),
            ))),
            None => Arc::new(base),
        };
        // `Agent::run` is `async` (via `#[async_trait]`) so we
        // bridge the future into a stream by awaiting it once
        // and yielding each event. The agent surfaces errors
        // through `AgentEvent::System(SessionEnded{Error})`,
        // so the returned stream carries no `Result`.
        Box::pin(async_stream::stream! {
            let mut inner = agent.run(input, cancel).await;
            while let Some(item) = inner.next().await {
                yield item;
            }
        })
    }
}
