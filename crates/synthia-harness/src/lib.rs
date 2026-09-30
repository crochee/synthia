//! # synthia-harness
//!
//! Agent runtime + multi-agent registry + system-prompt
//! assembly.
//!
//! ## Public surface
//!
//! ### Agent contract
//!
//! - [`agent::Agent`] — async trait every agent paradigm
//!   implements. Streams [`AgentEvent`] in real time.
//! - [`agent::ReActAgent`] — canonical `Agent` implementation.
//!   The full ReAct loop is self-contained inside the `agent`
//!   module.
//! - [`agent::AgentRegistry`] — multi-agent catalog implementing
//!   [`synthia_core::registry::Registry`].
//! - [`agent::RunInbox`] — interactive steering / follow-up
//!   message injection for an in-flight run; wired onto a
//!   [`agent::ReActAgent`] via
//!   [`ReActAgent::with_run_inbox`](agent::ReActAgent::with_run_inbox).
//!
//! ### Runtime contract
//!
//! [`Agent::run`] returns a `futures::Stream` and detaches one task per
//! run, so a caller that stops polling (a dropped connection) does not
//! cancel the run. Both halves are runtime-neutral: the stream is
//! `futures`, and the detach goes through
//! [`synthia_core::spawn::Spawner`], defaulting to tokio and replaceable
//! with [`ReActAgent::with_spawner`](ReActAgent::with_spawner).
//!
//! The parts that are **not** neutral are plugins you supply: the
//! builtin tool crates (`synthia-tool-read` / `-write` / `-shell`
//! use `tokio::fs` / `tokio::process`), the provider HTTP adapters
//! (`reqwest`), and the JSONL session sink. Timers (provider retry
//! backoff, the streaming idle watchdog) also need a reactor
//! today.
//!
//! ### System prompt assembly
//!
//! The system prompt is built by a deterministic, XML-delimited
//! assembler:
//!
//! - [`PromptContext`] — `PromptContext::assemble` renders the
//!   base prompt + identity / skills / peer-agents sections in
//!   a fixed order. Industry-aligned with the
//!   Anthropic Agent SDK and OpenAI Agents SDK XML-tag
//!   conventions; sections land at the high-attention edges so
//!   the manifest is cached and the instructions reinforce the
//!   grounding. Tool schemas are deliberately **not** restated
//!   as prose — they travel on the request's `tools` field,
//!   whose empty-vs-present shape the provider owns. There is
//!   no public `Section` trait — the canonical assembly is the
//!   only shape callers need.
//! - [`agent::AgentDescriptor`] — carries
//!   identity + capability metadata (name, instructions,
//!   tools, persona, handoffs, handoff_hint, model_hint).
//!
//! ### Per-session inputs
//!
//! - [`AgentInput`] — user input (text / multi-part / history-resume).
//! - [`AgentRunConfig`] — per-session configuration consumed by
//!   the run factory inside `SessionController`. Carries the
//!   [`PromptContext`] manifest injected into every
//!   session.
//!
//! ### Events
//!
//! - [`AgentEvent`] (4-variant) / [`SystemEvent`] /
//!   [`AgentMeta`] / [`SessionEndReason`] /
//!   [`WarningKind`] — what any [`Agent::run`] streams. The
//!   stream **is** the record of a run: every state change,
//!   including the terminal `SystemEvent::SessionEnded`.
//!
//! ### Multi-agent delegation (plugin)
//!
//! Sub-agent coordination is **not** in this crate: the `task`
//! tool, its registry wiring, gates, worktree isolation, and the
//! mention router all live in the `synthia-tool-task` plugin
//! crate. They plug into the harness through
//! [`agent::ToolInterceptor`] (`ReActAgent::with_interceptor`):
//! installed, the model sees a synthetic `task(agent, prompt)`
//! tool whose calls run a child agent and commit its final text
//! as the tool result; absent, the loop has no delegation surface
//! at all. [`agent::AgentRegistry`] — the catalog the plugin
//! resolves peers from — is the piece that stays here, alongside
//! the loop and the event types child runs wrap their traces in
//! ([`AgentEvent::Agent`] + [`AgentMeta`]).

pub mod agent;
mod compaction;
mod config;
pub mod events;
mod input;
mod prompt;

pub use agent::{
    Agent,
    AgentDescriptor,
    AgentEntry,
    AgentFilter,
    AgentHandle,
    AgentRegistry,
    AgentRuntime,
    BestOfNStrategy,
    CandidateScorer,
    ChainOfThoughtStrategy,
    DEFAULT_JUDGE_PROMPT,
    DEFAULT_MAX_ITERATIONS,
    DEFAULT_SYSTEM_PROMPT,
    DetachedAgent,
    DetachedClosed,
    DetachedError,
    EventSink,
    InterceptorCall,
    KNOWN_STRATEGY_NAMES,
    LlmJudgeScorer,
    LongestAnswer,
    MpscInbox,
    ReActAgent,
    ReActStrategy,
    ReasoningStrategy,
    RunInbox,
    RunInboxHandle,
    ToolInterceptor,
    from_name,
};
pub use compaction::{
    CompactionEmitters,
    context_manager_for_compaction_with_emitters,
};
pub use config::AgentRunConfig;
pub use events::{
    AgentEvent,
    AgentMeta,
    SessionEndReason,
    SteeringSource,
    SystemEvent,
    WarningKind,
};
pub use input::AgentInput;
pub use prompt::{PromptContext, runtime_context::RuntimeContext};
