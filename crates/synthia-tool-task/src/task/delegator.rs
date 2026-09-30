//! The `task` tool as a harness plugin.
//!
//! Wraps an [`AgentRegistry`] of peer agents and implements
//! [`ToolInterceptor`]: installed on a
//! [`ReActAgent`](synthia_harness::ReActAgent) via
//! `with_interceptor`, the model sees a `task(agent, prompt)`
//! tool; calls resolve the named peer, run it as a
//! depth-capped child, forward its events, and commit its
//! final text as the tool result. Not installed, the agent
//! has no `task` tool at all.

use std::sync::Arc;

use synthia_core::CancelToken;
use synthia_harness::{
    AgentEvent,
    AgentRegistry,
    InterceptorCall,
    ToolInterceptor,
};
use synthia_provider::ToolDefinition;
use tracing::{info, warn};

use super::{
    runner::run_subagent,
    schema::task_tool_definition,
    spec::{MAX_SUBAGENT_DEPTH, TASK_TOOL_NAME, TaskSpec},
};

#[derive(Clone)]
pub struct TaskDelegator {
    peers: Arc<AgentRegistry>,
}

impl TaskDelegator {
    /// Delegator resolving peers from `registry`.
    #[must_use]
    pub fn new(peers: Arc<AgentRegistry>) -> Self {
        Self { peers }
    }

    /// The peer catalog this delegator resolves names from.
    #[must_use]
    pub fn peers(&self) -> &Arc<AgentRegistry> {
        &self.peers
    }

    async fn execute_call(
        &self,
        call: &synthia_provider::ToolUse,
        cancel: Arc<dyn CancelToken>,
        depth: usize,
        emit: &(dyn Fn(AgentEvent) + Send + Sync),
    ) -> synthia_tool::ToolOutput {
        let spec = match TaskSpec::from_value(&call.input) {
            Ok(spec) => spec,
            Err(message) => return synthia_tool::ToolOutput::error(message),
        };

        if depth >= MAX_SUBAGENT_DEPTH {
            warn!(
                tool_use_id = %call.id,
                depth,
                "task_delegator: sub-agent depth limit reached"
            );
            return synthia_tool::ToolOutput::error(format!(
                "cannot delegate: maximum sub-agent depth ({}) reached; \
                 the `task` tool is disabled at this nesting level",
                MAX_SUBAGENT_DEPTH
            ));
        }

        let peer = match self.peers.resolve_sync(&spec.agent) {
            Some(peer) => peer,
            None => {
                let names = self.peers.names().join(", ");
                return synthia_tool::ToolOutput::error(format!(
                    "unknown peer agent `{}`; registered agents: {}",
                    spec.agent,
                    if names.is_empty() {
                        "(none)".to_string()
                    } else {
                        names
                    }
                ));
            }
        };

        info!(
            tool_name = %call.name,
            tool_use_id = %call.id,
            peer = %spec.agent,
            depth = depth + 1,
            "task_delegator: spawning sub-agent"
        );

        // `parent_session_id` is empty at this layer (the server
        // assigns session ids); delegation to nested children
        // preserves depth bookkeeping regardless.
        let parent_session_id = String::new();
        run_subagent(peer, spec, &parent_session_id, depth, cancel, emit).await
    }
}

impl ToolInterceptor for TaskDelegator {
    fn definitions(&self) -> Vec<ToolDefinition> {
        vec![task_tool_definition()]
    }

    fn claims(&self, name: &str) -> bool {
        name == TASK_TOOL_NAME
    }

    fn execute<'a>(
        &'a self,
        call: InterceptorCall<'a>,
    ) -> futures::future::BoxFuture<'a, synthia_tool::ToolOutput> {
        Box::pin(async move {
            self.execute_call(
                call.call,
                Arc::clone(&call.cancel),
                call.depth,
                call.emit,
            )
            .await
        })
    }
}
