//! [`Agent`](crate::agent::Agent) and
//! [`RegistryItem`](synthia_core::registry::RegistryItem) impls
//! for [`super::ReActAgent`].
//!
//! The trait impl is intentionally tiny: the harness has nothing
//! to do but wire the per-run channel and executor onto the
//! [`ReasoningStrategy`] — every state-machine step travels in
//! through [`super::agent_runtime::ReActAgent::runtime_for_run`].

use std::{pin::Pin, sync::Arc};

use async_trait::async_trait;
use futures::{Stream, channel::mpsc};
use synthia_core::{CancelToken, registry::RegistryItem};

use super::ReActAgent;
use crate::{events::AgentEvent, input::AgentInput};

#[async_trait]
impl crate::agent::Agent for ReActAgent {
    fn descriptor(&self) -> &crate::agent::AgentDescriptor {
        &self.descriptor
    }

    fn effective_max_iterations(&self) -> usize {
        self.max_iterations
    }

    async fn run(
        &self,
        input: AgentInput,
        cancel: Arc<dyn CancelToken>,
    ) -> Pin<Box<dyn Stream<Item = AgentEvent> + Send + 'static>> {
        let (tx, rx) = mpsc::unbounded::<AgentEvent>();
        let sink = crate::agent::strategy::EventSink::new(
            Arc::new(tx),
            self.typed_sink.clone(),
        );
        let runtime =
            self.runtime_for_run(Arc::clone(&cancel), input.subagent_depth);
        // The strategy owns the loop; this adapter only wires the
        // channel and the executor. Everything the loop is made
        // of travelled in through `runtime`.
        //
        // Spawned under a panic guard: `catching_panics` returns the
        // sink so a panicking strategy can still report. It is the
        // backstop for the whole loop — the inner guards cover the
        // third-party seams (tool, interceptor, provider, context
        // manager, inbox), while a panic in the loop's *own* code
        // (bucketing, commit, hook fan-out, guards) is reachable only
        // from here. Without it the task unwinds, the sender drops, and
        // the caller's stream just ends with no `SessionEnded`.
        let strategy = Arc::clone(&self.strategy);
        Arc::clone(&self.spawner).spawn(Box::pin(async move {
            if let Err(message) = crate::agent::re_act::catching_panics(
                strategy.run(runtime, input, sink.clone()),
            )
            .await
            {
                sink.fail(format!("strategy panicked: {message}"));
            }
        }));
        Box::pin(rx)
    }
}

impl RegistryItem for ReActAgent {
    fn name(&self) -> &str {
        &self.descriptor.name
    }

    fn description(&self) -> &str {
        &self.descriptor.description
    }
}
