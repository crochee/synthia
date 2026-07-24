//! Per-iteration helpers for the [`StreamBuilder`] ReAct
//! loop.
//!
//! Each helper handles a single phase of the iteration
//! and returns the [`AgentEvent`]s the caller should
//! yield, plus any control-flow signal
//! (`continue` / `return` / `break`). The surrounding
//! `stream!` block in [`super::run`] is the only place
//! that actually yields — the helpers do all the
//! per-phase logic and return what should be emitted.
//!
//! This split keeps the per-iteration logic
//! unit-testable in isolation while preserving Rust's
//! `async_stream` yield semantics.
//!
//! Submodule layout:
//!
//! - [`types`]: phase outcome enums
//!   ([`CompactOutcome`], [`LlmSampleOutcome`]).
//! - [`init`]: seed context and drain steering.
//! - [`reflect`]: in-loop and end-of-session
//!   self-reflection.
//! - [`compact`]: token-budget compaction check.
//! - [`llm`]: LLM sampling with the recovery cascade.
//! - [`loop_detect`]: doom-loop detection.
//!
//! [`AgentEvent`]: crate::events::AgentEvent
//! [`StreamBuilder`]: super::types::StreamBuilder

mod compact;
mod init;
mod llm;
mod loop_detect;
mod reflect;
mod types;

pub(super) use compact::do_compact_step;
pub(super) use init::{drain_steering, seed_initial_messages};
pub(super) use llm::{
    build_tool_definitions,
    prepare_agent_ctx,
    sample_llm_and_cascade,
};
pub(super) use loop_detect::check_doom_loop;
pub(super) use reflect::{
    end_of_session_reflect,
    execute_self_reflect_tool_call,
};
pub(super) use types::{CompactOutcome, LlmSampleOutcome};
