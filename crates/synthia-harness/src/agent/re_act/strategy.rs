//! The default [`ReasoningStrategy`].
//!
//! [`ReasoningStrategy`]: crate::agent::ReasoningStrategy

use async_trait::async_trait;

use super::loop_::ReActLoop;
use crate::{
    agent::strategy::{AgentRuntime, EventSink, ReasoningStrategy},
    input::AgentInput,
};

/// Think → act → observe, with tools: the loop this crate has
/// always run, expressed as a [`ReasoningStrategy`] so it can be
/// swapped without rebuilding the agent.
///
/// It is the default, and it is the reference implementation: a
/// custom strategy can copy its shape (prepare → sample → execute
/// tools → repeat) while reusing everything in [`AgentRuntime`].
#[derive(Debug, Default)]
pub struct ReActStrategy;

#[async_trait]
impl ReasoningStrategy for ReActStrategy {
    fn name(&self) -> &str {
        "react"
    }

    async fn run(
        &self,
        runtime: AgentRuntime,
        input: AgentInput,
        sink: EventSink,
    ) {
        ReActLoop::from_runtime(runtime, sink).drive(input).await;
    }
}
