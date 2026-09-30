//! [`synthia_tool_task`] — the `task` tool plugin: multi-agent
//! delegation by running registered peers as sub-agents (gates,
//! worktree isolation, `@agent:` routing, subagent admission, batched
//! completion notification).
//!
//! Installs on the harness through the interceptor seam:
//! `agent.with_interceptor(Arc::new(TaskDelegator::new(peers)))`.

pub use synthia_tool_task::*;
