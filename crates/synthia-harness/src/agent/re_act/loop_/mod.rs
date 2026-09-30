//! The ReAct state machine.
//!
//! [`ReActLoop`] is the per-session driver bound to one run.
//! The main loop body ([`drive`]) lives in [`drive`]; every
//! other concern lives in its own focused sibling module.
//!
//! ## Layout
//!
//! | Module | Concern |
//! |---|---|
//! | [`drive`] | `drive()` orchestrator + the four named phases (`drive_setup` / `stamp_request_header` / `drive_one_iteration` / `drive_finalize`) |
//! | [`drive_hooks`] | The cross-cutting helpers `drive` calls from more than one phase: `hooks_on_agent_start` / `hooks_on_agent_end` / `cancelled` |
//! | [`steps`]   | prepare, sample_once, commit_assistant, fail_truncated_tool_batch, apply_context, inject_hints, snapshot_runtime_context, finalize |
//! | [`bucket`]  | `execute_tools` orchestrator + Parallel / Sequential bucketing, semaphore concurrency, sequential aborts, commit-all-results |
//! | [`seams`]   | `execute_tool_inner` orchestrator + the four steering seams (restriction, hook veto, guard pipeline, observation) |
//! | [`route`]   | `dispatch_tool_call` orchestrator + the three routing steps (interceptor claim, registry lookup, stream drain) |
//! | [`dispatch`] | Shared seam helpers: `WireToolResult`, `lookup_execution_mode`, `tool_definitions` memo, `append_tool_result_hints`, `notify_error_hooks` |
//! | [`commit`]  | `commit_tool_result` + `last_assistant_text` |
//! | [`inbox`]   | drain_steering, take_follow_ups, append_injected |
//! | [`events`]  | emit_typed wrappers + `StepAction` (typed-event boundary) |
//!
//! Cross-cutting helpers (the typed-event boundary, the stream
//! sink wiring) live in their respective modules and thread
//! through every step. The provider-chunk → event translator
//! lives in [`super::stream`].
//!
//! ## The harness contract
//!
//! With **zero** interceptors, the loop has no `task` tool,
//! no delegation, no multi-agent machinery — it is a
//! single-loop harness. Every external capability is
//! composed in through the registry, the strategy seam, or
//! [`ToolInterceptor`].
//!
//! [`ToolInterceptor`]: crate::agent::ToolInterceptor

use std::{path::PathBuf, sync::Arc, time::Instant};

use parking_lot::Mutex;
use synthia_context::AgentState;
use synthia_provider::{
    Message,
    ToolDefinition,
    ToolUse,
    traits::ModelProvider,
};
use synthia_session::TypedEventSink;
use synthia_steering::Steering;
use synthia_tool::ToolRegistry;

use crate::{
    agent::{
        RunInbox,
        descriptor::AgentDescriptor,
        interceptor::ToolInterceptor,
        strategy::{AgentRuntime, EventSink},
    },
    prompt::PromptContext,
};

mod bucket;
mod commit;
mod dispatch;
mod drive;
mod drive_hooks;
mod events;
mod inbox;
mod route;
mod seams;
mod steps;

pub(in crate::agent::re_act) use dispatch::WireToolResult;
/// [`super::agent::ReActAgent`]'s shared deps plus per-call
/// state (sender, cancellation). Encapsulates the full
/// state machine so the [`Agent::run`](crate::agent::Agent::run)
/// implementation stays trivial.
pub(crate) struct ReActLoop {
    pub(super) provider: Arc<dyn ModelProvider>,
    pub(super) tool_registry: Arc<ToolRegistry>,
    /// R34 surface policy cloned from the owning
    /// [`super::agent::ReActAgent`] (`None` = the unfiltered
    /// R33 projection).
    pub(super) tool_surface: Option<Arc<synthia_tool::ToolSurfacePolicy>>,
    /// R58 per-agent allow/deny list, cloned from the owning
    /// agent.
    pub(super) tool_restriction: Option<Arc<synthia_tool::ToolRestriction>>,
    /// Memo for the composed tool list.
    pub(super) tool_projection: Mutex<Option<ToolProjectionMemo>>,
    pub(super) workspace_root: PathBuf,
    pub(super) descriptor: AgentDescriptor,
    pub(super) prompt_context: Arc<PromptContext>,
    pub(super) context_manager: Arc<dyn synthia_context::ContextManager>,
    pub(super) interceptors: Vec<Arc<dyn ToolInterceptor>>,
    pub(super) steering: Arc<Steering>,
    pub(super) depth: usize,
    pub(super) cancel: Arc<dyn synthia_core::CancelToken>,
    pub(super) sink: crate::agent::re_act::stream::StreamSink,
    pub(super) started_at: Instant,
    pub(super) max_iterations: usize,
    pub(super) typed_sink: Option<TypedEventSink>,
    pub(super) inbox: Option<Arc<dyn RunInbox>>,
    pub(super) touched: Mutex<synthia_context::CompactionDetails>,
    /// Whether a schema-valid `structured_output` submission was
    /// committed. Per-run, like [`Self::touched`]: the loop is built by
    /// [`ReActLoop::from_runtime`] and consumed by
    /// [`ReActLoop::drive`], so this dies with the run.
    ///
    /// Recorded here rather than recovered from the transcript because
    /// the transcript is lossy for exactly this question: history stores
    /// a tool result as its *content* plus the call id
    /// ([`commit::commit_tool_result`]), and the tool name / `is_error`
    /// flag that decide validity exist only on the wire event.
    pub(super) structured_output: Mutex<bool>,
    pub(super) clock: synthia_core::SharedClock,
    pub(super) last_runtime_snapshot: Option<String>,
}

/// Result of one LLM sampling pass. Defined here (not in
/// [`steps`]) because both [`steps`] (producer) and [`drive`]
/// (consumer) share it; keeping the type in `mod.rs` makes
/// the dispatch visible to both without forcing one to
/// import from the other.
pub(crate) struct SampleOutcome {
    pub(super) assistant_text: String,
    pub(super) tool_uses: Vec<ToolUse>,
    pub(super) stop_reason: Option<String>,
    /// The assistant turn's content parts, in the order the provider
    /// produced them.
    ///
    /// This — not the two fields above — is what the history is built
    /// from. `assistant_text` and `tool_uses` are flattened *views*
    /// used for control flow (does this turn call a tool? how long was
    /// the answer?), and rebuilding a message from them would lose the
    /// interleaving: a turn whose reasoning precedes its tool call
    /// would be committed as `[text, tool_use]`, and a turn that was
    /// *only* reasoning would commit nothing at all.
    pub(super) parts: Vec<synthia_provider::ContentPart>,
}

impl SampleOutcome {
    pub(super) fn has_tool_calls(&self) -> bool {
        !self.tool_uses.is_empty()
    }
}

pub(crate) struct ToolProjectionMemo {
    pub(super) registry_version: u64,
    pub(super) promoted: Vec<String>,
    pub(super) defs: Arc<Vec<ToolDefinition>>,
}

impl ReActLoop {
    pub(super) fn from_runtime(runtime: AgentRuntime, sink: EventSink) -> Self {
        let AgentRuntime {
            provider,
            tool_registry,
            tool_surface,
            tool_restriction,
            workspace_root,
            descriptor,
            prompt_context,
            context_manager,
            interceptors,
            steering,
            max_iterations,
            typed_sink,
            inbox,
            clock,
            spawner: _,
            cancel,
            subagent_depth,
        } = runtime;
        let (tx, typed_from_sink) = sink.into_parts();
        Self {
            provider,
            tool_registry,
            tool_surface,
            tool_restriction,
            workspace_root,
            descriptor,
            prompt_context,
            context_manager,
            interceptors,
            steering,
            depth: subagent_depth,
            cancel,
            sink: crate::agent::re_act::stream::StreamSink {
                tx,
                typed_sink: typed_from_sink,
            },
            tool_projection: Mutex::new(None),
            started_at: Instant::now(),
            max_iterations,
            typed_sink,
            inbox,
            touched: Mutex::new(synthia_context::CompactionDetails::default()),
            structured_output: Mutex::new(false),
            clock,
            last_runtime_snapshot: None,
        }
    }

    /// Drive one session end-to-end. The body lives in
    /// [`drive`]; this is a one-line façade so the public
    /// surface stays in `mod.rs`.
    pub(super) async fn drive(
        self,
        input: crate::input::AgentInput,
    ) -> crate::events::AgentOutput {
        drive::drive(self, input).await
    }
}

// --- Step method declarations; bodies live in the submodules ---
//
// Each delegation is one line: the orchestrator (in
// `drive.rs`) calls `self.method(...)`; the method lives in
// the focused sibling that owns the concern. This block is
// the entire "loop's delegation surface" — keep it flat, no
// control flow, no state.

impl ReActLoop {
    pub(super) fn prepare(
        &self,
        input: &crate::input::AgentInput,
    ) -> Vec<Message> {
        steps::prepare(self, input)
    }

    pub(super) async fn sample_once(
        &self,
        messages: &[Message],
        iteration: usize,
        agent_state: &mut AgentState,
    ) -> Result<super::SampleOutcome, crate::events::SessionEndReason> {
        steps::sample_once(self, messages, iteration, agent_state).await
    }

    pub(super) fn commit_assistant(
        &self,
        messages: &mut Vec<Message>,
        outcome: &super::SampleOutcome,
    ) {
        steps::commit_assistant(messages, outcome)
    }

    pub(super) fn fail_truncated_tool_batch(
        &self,
        messages: &mut Vec<Message>,
        calls: &[ToolUse],
    ) {
        steps::fail_truncated_tool_batch(self, messages, calls)
    }

    pub(super) async fn execute_tools(
        &self,
        messages: &mut Vec<Message>,
        calls: &[ToolUse],
        agent_state: &mut AgentState,
    ) {
        bucket::execute_tools(self, messages, calls, agent_state).await
    }

    pub(super) async fn apply_context(
        &self,
        messages: &mut Vec<Message>,
        state: &mut AgentState,
    ) -> Result<(), crate::events::SessionEndReason> {
        steps::apply_context(self, messages, state).await
    }

    pub(super) async fn inject_hints(
        &self,
        messages: &mut Vec<Message>,
        state: &AgentState,
        system_hinted: &mut std::collections::HashSet<String>,
        iteration: usize,
    ) {
        steps::inject_hints(self, messages, state, system_hinted, iteration)
            .await
    }

    pub(super) fn snapshot_runtime_context(
        &mut self,
        messages: &mut Vec<Message>,
    ) {
        steps::snapshot_runtime_context(self, messages)
    }

    pub(super) fn tool_definitions(
        &self,
        messages: &[Message],
    ) -> Arc<Vec<ToolDefinition>> {
        dispatch::tool_definitions(self, messages)
    }

    pub(super) async fn dispatch_tool_call(
        &self,
        call: &ToolUse,
        tool_name: &str,
        call_id: &str,
    ) -> synthia_tool::ToolOutput {
        route::dispatch_tool_call(self, call, tool_name, call_id).await
    }

    pub(super) fn commit_tool_result(
        &self,
        messages: &mut Vec<Message>,
        tr: WireToolResult,
    ) {
        commit::commit_tool_result(self, messages, tr)
    }

    pub(super) fn finalize(
        &self,
        reason: crate::events::SessionEndReason,
        messages: &[Message],
    ) -> crate::events::AgentOutput {
        steps::finalize(self, reason, messages)
    }

    pub(super) async fn drain_steering(&self, messages: &mut Vec<Message>) {
        inbox::drain_steering(self, messages).await
    }

    pub(super) async fn take_follow_ups(&self) -> Vec<Message> {
        inbox::take_follow_ups(self).await
    }

    pub(super) fn append_injected(
        &self,
        messages: &mut Vec<Message>,
        source: crate::events::SteeringSource,
        drained: Vec<Message>,
    ) {
        inbox::append_injected(self, messages, source, drained)
    }
}
