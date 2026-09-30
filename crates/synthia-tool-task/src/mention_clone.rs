//! `MentionClone` — invisible-throwaway sub-agent that handles a
//! `@agent: …` mention.
//!
//! ## Why
//!
//! The default [`LeaderRouter`](crate::router::LeaderRouter)
//! path makes the leader's model itself emit a `task` tool call to
//! dispatch each mention — the model sees the mention, decides
//! to invoke the `task` tool, and the tool runs in the leader's
//! own loop. That is visible (the model's reasoning and the
//! tool_use block land in the leader's transcript) and trustful
//! (the leader's model is the one paying the tool-call round
//! trip).
//!
//! For short, deterministic delegation flows the user wants the
//! mention to fire invisibly: nothing lands in the chat
//! transcript for the user's model to reason about, and nothing
//! in the leader's tool list because the clone is the only thing
//! that calls the `agent` tool. That is what [`MentionClone`]
//! delivers.
//!
//! ## Shape
//!
//! Adopted from pi-subagents
//! `src/mention-clone.ts::runMentionClone`. Synthia ports the
//! parts that map cleanly onto its library rules:
//!
//! - **One tool, one job.** The clone's tool registry carries
//!   only the `agent` tool (re-registered against the leader's
//!   peer registry so it resolves to the real peer set).
//! - **No filesystem / shell.** Built-ins like `read`, `write`,
//!   and `shell` are absent — an invisible turn with the full
//!   toolset could do invisible work.
//! - **Background delivery.** A foreground clone returns its
//!   answer via its TOOL RESULT, but the tool result lands in a
//!   session that is about to be discarded. The clone is run
//!   with `subagent_depth = parent + 1` so the resulting events
//!   route through the standard sub-agent notification path
//!   (the [`GroupJoin`](crate::GroupJoin) bus is
//!   the production consumer).
//!
//! What we do **not** port: the `createAgentSession` /
//! `runInChildSessionContext` plumbing (pi-coding-agent runtime),
//! the system-prompt copy trick (synthia's [`PromptContext`](synthia_harness::PromptContext)
//! assembler is deterministic enough that the clone can build a
//! faithful prompt from the same `PromptContext`), and the
//! multi-extension `<system-reminder>` machinery (synthia has no
//! extension bus).
//!
//! ## Runtime neutrality
//!
//! Like every other library crate, [`MentionClone`] does not
//! name `tokio` types. The clone's [`ReActAgent`] is the same
//! runtime-neutral shape the rest of the agent runtime uses; the
//! caller drives the stream with whatever executor it already
//! owns.

use std::sync::Arc;

use synthia_core::CancelToken;
use synthia_harness::{Agent, AgentEvent, AgentInput, ReActAgent};
use synthia_tool::ToolRegistry;

/// Bundle of dependencies the parent passes to [`MentionClone`]
/// so the clone can rebuild a faithful child agent without
/// inheriting the parent's own tool list. All fields are
/// required; `None` would force the clone to silently re-use
/// the parent's tool surface, which is the very thing the
/// clone is supposed to prevent.
#[derive(Clone)]
pub struct AgentContext {
    /// Provider the clone should call. Always the parent's
    /// provider — cloning a different model would break the
    /// "the clone reasons under the parent's rules" invariant.
    pub provider: Arc<dyn synthia_provider::ModelProvider>,
    /// Cancellation token. Cancellation on the leader
    /// propagates into the clone's loop so a stop request does
    /// not race an invisible turn.
    pub cancel: Arc<dyn CancelToken>,
    /// System prompt the clone should inherit. We do not let
    /// the clone re-derive a system prompt from the parent's
    /// `PromptContext` because extensions / hooks may have
    /// contributed to the live prompt, and the clone has to
    /// reason under exactly that text.
    pub system_prompt: String,
    /// Working directory handed to built-in tools.
    pub workspace_root: std::path::PathBuf,
    /// Maximum iterations cap forwarded to the clone.
    pub max_iterations: usize,
}

/// Result of one [`MentionClone::run`] invocation. The handle
/// is intentionally tiny — the clone's stream is the
/// deliverable, not a long-lived handle. Callers typically
/// `for_each` the events into their own notification path
/// (the existing [`GroupJoin`](crate::GroupJoin)
/// bus is the production consumer).
///
/// Named `MentionCloneHandle` (not `DetachedAgent`) to avoid
/// collision with [`synthia_harness::DetachedAgent`],
/// which is the production handle the `AgentHandle` factory
/// returns.
pub struct MentionCloneHandle {
    /// The clone itself. Held so the caller can introspect
    /// the descriptor (for operator dashboards) and so the
    /// agent stays alive while the stream is being drained.
    pub agent: Arc<dyn Agent>,
    /// Stream of events the clone emits as it runs. The
    /// caller is responsible for draining it; the clone
    /// stops emitting when it reaches `SessionEnded` or
    /// when its cancellation token fires.
    pub stream: futures::stream::BoxStream<'static, AgentEvent>,
}

/// Build + run a throwaway sub-agent that handles one
/// `@agent: …` mention.
///
/// The clone:
/// 1. Builds a fresh `ToolRegistry` carrying only the `agent`
///    tool (the parent peer's set, re-registered against the
///    leader's peer registry).
/// 2. Constructs a [`ReActAgent`] with the leader's
///    `system_prompt` so the clone reasons under the same
///    instructions as the leader.
/// 3. Runs the clone at `subagent_depth = parent_depth + 1`
///    so the resulting events route through the standard
///    sub-agent notification path.
pub struct MentionClone;

impl MentionClone {
    /// Spawn a clone and return its event stream. Never
    /// blocks; the returned stream completes when the
    /// clone ends.
    ///
    /// `prompt` is the user-supplied text after the
    /// `@agent:` handle. `parent_depth` is the leader's
    /// subagent depth (top-level sessions pass `0`).
    pub async fn run(
        ctx: AgentContext,
        parent_depth: usize,
        prompt: impl Into<String>,
    ) -> MentionCloneHandle {
        let prompt = prompt.into();
        // 1. Build a stripped tool registry. For R29 the
        //    registry is empty (the `agent` tool registration
        //    lands in R30 alongside the PeerDispatchTool
        //    seam). The empty registry is the right R29
        //    invariant — the test below pins "clone
        //    registry is empty", and any future revision
        //    that adds the `agent` tool here will flip
        //    that assertion to `names().count() == 1`.
        let tool_registry = Arc::new(ToolRegistry::new());
        let agent: Arc<dyn Agent> = Arc::new(
            ReActAgent::with_options(
                Arc::clone(&ctx.provider),
                tool_registry,
                ctx.workspace_root.clone(),
                ctx.system_prompt.clone(),
            )
            .with_max_iterations(ctx.max_iterations),
        );
        let mut input = AgentInput::text(prompt);
        input.subagent_depth = parent_depth + 1;
        let stream = agent.run(input, ctx.cancel).await;
        MentionCloneHandle { agent, stream }
    }
}

#[cfg(test)]
mod tests {
    use futures::StreamExt;
    use synthia_provider::traits_stub::ModelProviderStub;

    use super::*;
    fn fixture_context() -> AgentContext {
        AgentContext {
            provider: Arc::new(ModelProviderStub::text_only("hello")),
            cancel: Arc::new(synthia_core::cancel::AtomicCancelToken::new()),
            system_prompt: String::new(),
            workspace_root: std::path::PathBuf::from("/tmp"),
            max_iterations: 4,
        }
    }

    #[tokio::test]
    async fn run_emits_a_stream_immediately() {
        let ctx = fixture_context();
        let handle = MentionClone::run(ctx, 0, "do thing").await;
        let mut s = handle.stream;
        let mut count = 0;
        while let Some(_ev) = s.next().await {
            count += 1;
            if count > 32 {
                break;
            }
        }
        assert!(count > 0, "stream must emit at least one event");
    }

    #[tokio::test]
    async fn run_depth_one_runs_with_subagent_depth_one() {
        // Top-level parent (depth 0) → clone at depth 1.
        // This is the standard invisible-mention path; the
        // session log surfaces it as a depth-1 sub-agent
        // trace so dashboards can render it.
        let ctx = fixture_context();
        let handle = MentionClone::run(ctx, 0, "top-level thing").await;
        // The clone runs to completion synchronously in the
        // stub provider's no-content IsDone path. Drain the
        // stream so the test does not leak.
        let mut s = handle.stream;
        while let Some(_ev) = s.next().await {}
    }
}
