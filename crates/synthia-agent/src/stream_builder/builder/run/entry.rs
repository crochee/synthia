use std::{pin::Pin, sync::Arc};

use super::super::types::{BuilderSteps, StreamBuilder};
use crate::{
    config::AgentRunConfig,
    events::AgentEvent,
    tracing::record_llm_cache_tokens,
};

impl StreamBuilder {
    /// Run an agent session and produce a stream of
    /// [`AgentEvent`]s.
    ///
    /// The system prompt snapshot is taken ONCE at the
    /// top of this method — the system prompt is
    /// typically immutable during a session, so
    /// cache-hit detection does not need per-iteration
    /// re-snapshots. Per-iteration token history is
    /// captured via the LLM call boundary.
    ///
    /// [`AgentEvent`]: crate::events::AgentEvent
    pub fn run(
        &self,
        run_config: AgentRunConfig,
    ) -> Pin<Box<dyn futures::Stream<Item = AgentEvent> + Send>> {
        let steps = BuilderSteps::new(&run_config, self.hooks.clone());
        // Capture prefix snapshot Arc-clones so the stream is `'static`.
        let prefix_tracker = self.prefix_tracker.clone();
        let on_prefix_event = self.on_prefix_event.clone();
        // Always wire an `on_usage` callback that emits cache token
        // counters via the `metrics` facade (no-op when no recorder is
        // installed). This gives operators KV cache hit ratio
        // observability without requiring the `otel` / `observability`
        // feature on this crate.
        let on_usage: Option<
            Arc<dyn Fn(synthia_provider::TokenUsage) + Send + Sync + 'static>,
        > = Some(Arc::new(|usage: synthia_provider::TokenUsage| {
            record_llm_cache_tokens(&usage);
        }));
        let initial_system_snapshot: Vec<u8> = self.context.system_snapshot();
        self.run_with_steps(
            run_config,
            steps,
            self.initial_state.clone(),
            prefix_tracker,
            on_prefix_event,
            on_usage,
            initial_system_snapshot,
        )
    }
}
