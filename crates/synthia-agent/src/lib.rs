//! # synthia-agent
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
//!
//! ### System prompt assembly
//!
//! The system prompt is built by a deterministic, XML-delimited
//! assembler:
//!
//! - [`prompt`] — `PromptContext::assemble` renders the
//!   base prompt + identity / tool / skill / agent / rules
//!   sections in a fixed order. Industry-aligned with the
//!   Anthropic Agent SDK and OpenAI Agents SDK XML-tag
//!   conventions; sections land at the high-attention edges so
//!   the manifest is cached and the rules reinforce the
//!   grounding. There is no public `Section` trait — the
//!   canonical assembly is the only shape callers need.
//! - [`agent::descriptor`] — `AgentDescriptor` carries
//!   identity + capability metadata (name, instructions,
//!   tools, persona, handoffs, handoff_hint, model_hint).
//!
//! ### Per-session inputs
//!
//! - [`AgentInput`] — user input (text / multi-part / history-resume).
//! - [`AgentRunConfig`] — per-session configuration consumed by
//!   the run factory inside `SessionController`. Carries the
//!   [`prompt::PromptContext`] manifest injected into every
//!   session.
//!
//! ### Events
//!
//! - [`AgentEvent`] (4-variant) / [`SystemEvent`] /
//!   [`AgentMeta`] / [`SessionEndReason`] / [`AgentOutput`] /
//!   [`WarningKind`] — events emitted by any [`Agent::run`].
//!
//! ### Multi-agent delegation
//!
//! Multi-agent coordination is delegated to the `TaskTool` layer:
//! a parent agent invokes `TaskTool::invoke` once per sub-agent
//! inside a single `react_step` and `tokio::join!`s their runs.
//! The legacy `coordinator::Coordinator` actor was retired in v1.3
//! because no production caller ever instantiated one — see the
//! design notes in `mvp-agent-design.md §11.1`.

pub mod agent;
pub mod config;
pub mod events;
pub mod input;
pub mod prompt;

pub use agent::{
    Agent,
    AgentDescriptor,
    AgentEntry,
    AgentFilter,
    AgentRegistry,
    ReActAgent,
};
pub use config::AgentRunConfig;
pub use events::{
    AgentEvent,
    AgentMeta,
    AgentOutput,
    SessionEndReason,
    SystemEvent,
    WarningKind,
};
pub use input::AgentInput;
pub use prompt::PromptContext;
