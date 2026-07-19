//! Public entry points for [`StreamBuilder`].
//!
//! [`StreamBuilder::run`] is the consumer-facing API: it
//! snapshots the system prompt, allocates a fresh
//! [`BuilderSteps`], and hands off to
//! [`StreamBuilder::run_with_steps`]. The latter builds
//! the per-session `async_stream::stream!` that emits
//! [`AgentEvent`]s.
//!
//! The `stream!` block lives entirely inside
//! [`StreamBuilder::run_with_steps`] because Rust's
//! `async_stream` requires `yield` to be a statement in
//! the macro body, not a value returned from a helper.
//! The per-iteration logic is factored into helper
//! functions in [`super::iteration`] and
//! [`super::tool_execution`] that return `Vec<AgentEvent>`
//! plus control-flow signals — the `stream!` block
//! iterates over those events and acts on the control
//! signal (`continue` / `return` / `break`).
//!
//! Submodule layout:
//!
//! - [`entry`]: the public [`StreamBuilder::run`] entry
//!   point that snapshots the system prompt, builds the
//!   [`BuilderSteps`] and hands off to the per-iteration
//!   loop.
//! - [`main_loop`]: the internal
//!   [`StreamBuilder::run_with_steps`] method that
//!   contains the `async_stream::stream!` block — the
//!   per-session ReAct loop that drains steering,
//!   self-reflects, compacts, samples the LLM (with the
//!   recovery cascade), executes tools, and detects
//!   doom-loops.
//!
//! [`AgentEvent`]: crate::events::AgentEvent

mod entry;
mod helpers;
mod main_loop;
