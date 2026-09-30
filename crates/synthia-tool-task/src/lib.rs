//! # synthia-tool-task
//!
//! The `task` tool plugin: multi-agent delegation — everything a
//! parent agent needs to run child agents, and nothing the harness
//! needs to know about it.
//!
//! This crate is the concrete answer to "how do tools that need
//! the loop itself plug in?": its [`TaskDelegator`] implements
//! [`synthia_harness::ToolInterceptor`], so installing it on a
//! [`synthia_harness::ReActAgent`] gives the model a synthetic
//! `task(agent, prompt)` tool whose calls run a registered peer
//! as a child run:
//!
//! ```rust,no_run
//! use std::sync::Arc;
//!
//! use synthia_harness::{AgentRegistry, ReActAgent};
//! use synthia_tool_task::TaskDelegator;
//!
//! # async fn wrap(agent: ReActAgent) -> ReActAgent {
//! let peers = Arc::new(AgentRegistry::new());
//! // peers.put(AgentEntry::new(/* a child agent */)).await;
//! agent.with_interceptor(Arc::new(TaskDelegator::new(peers)))
//! # }
//! ```
//!
//! With no delegator installed the agent is a single-loop
//! harness with zero delegation surface — the loop code has no
//! `task` branch of its own, only the generic interceptor seam.
//!
//! ## What is here
//!
//! - [`task`] — the `task` tool: [`TaskSpec`] parsing, the wire
//!   definition, and [`TaskDelegator`] (depth-capped child runs,
//!   events forwarded as `AgentEvent::Agent` + `AgentMeta`).
//! - [`gate`] — a verification command run after a child
//!   finishes; pass / fail / timeout is surfaced in the tool
//!   result.
//! - [`worktree`] — git-worktree isolation: the child runs in a
//!   fresh detached worktree; dirty trees are committed to
//!   `synthia/agent-<ulid>` on cleanup.
//! - [`router`] / [`route_rules`] — `@agent: prompt` text-routed
//!   delegation as an alternative to the tool call.
//! - [`mention_clone`] — run a mention in a cloned context.
//! - [`pool`] — two-lane subagent admission (bounded background /
//!   unbounded foreground / nested exemption) + invocation
//!   tombstones.
//! - [`group_join`] — batched completion notification: one
//!   consolidated delivery per fan-out instead of one interruption
//!   per child, with a partial delivery when a straggler outlives
//!   the window.
//!
//! ## What is deliberately *not* here
//!
//! The [`synthia_harness::AgentRegistry`] itself (the catalog the
//! delegator resolves peers from) stays in `synthia-harness` — this
//! crate consumes it, never re-defines it.

pub mod gate;
pub mod group_join;
pub mod mention_clone;
pub mod pool;
pub mod route_rules;
pub mod router;
pub mod task;
pub mod worktree;

pub use gate::{
    CommandOutcome,
    CommandRunner,
    CommandStopReason,
    GateError,
    GateSpec,
    GateVerdict,
    StdCommandRunner,
    apply_gate_to_output,
    run_gate,
    verdict_summary,
};
pub use group_join::{
    DEFAULT_GROUP_TIMEOUT,
    DEFAULT_STRAGGLER_TIMEOUT,
    Delivery,
    GroupJoin,
    GroupOutcome,
};
pub use mention_clone::{AgentContext, MentionClone, MentionCloneHandle};
pub use pool::{
    Admission,
    AdmissionTicket,
    DEFAULT_BACKGROUND_CONCURRENCY,
    InvocationRecord,
    InvocationStatus,
    MAX_TOMBSTONES,
    PoolCounts,
    QueuedToken,
    SharedSubagentPool,
    Slot,
    SubagentPool,
};
pub use route_rules::{ConditionalRouter, RoutePatternError};
pub use router::{
    LeaderRouter,
    Mention,
    PassThroughRouter,
    Router,
    RoutingDecision,
    mentions_to_task_specs,
};
pub use task::{
    MAX_SUBAGENT_DEPTH,
    TASK_TOOL_NAME,
    TaskDelegator,
    TaskSpec,
    task_tool_definition,
    task_tool_definition_with_features,
};
pub use worktree::{
    WorktreeCleanupResult,
    WorktreeError,
    WorktreeInfo,
    WorktreeSpec,
    cleanup_worktree,
    create_worktree,
};
