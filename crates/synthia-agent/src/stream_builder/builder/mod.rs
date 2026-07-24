//! The `builder` submodule of `stream_builder`.
//!
//! Wires the per-step primitives in
//! [`super::steps`] (sample / tool_execute / compact / reflect)
//! into a single async stream of [`AgentEvent`]s.
//!
//! Submodule layout:
//!
//! - [`types`]: the 3 struct definitions ([`AgentBuilder`],
//!   [`StreamBuilder`], [`BuilderSteps`]) plus the
//!   `BuilderSteps::new` factory. No business logic — just
//!   shapes and constructors.
//! - [`construct`]: the [`StreamBuilder::from_config`] entry
//!   point that allocates a fresh
//!   [`ContextAssembler`] + [`HookBuilder`] and
//!   initialises the prefix tracker.
//! - [`setters`]: the builder-pattern accessors and
//!   `with_*` setters ([`StreamBuilder::with_prefix_tracker`],
//!   [`StreamBuilder::on_prefix_event`],
//!   [`StreamBuilder::with_initial_state`],
//!   [`StreamBuilder::context`], [`StreamBuilder::hooks`],
//!   [`StreamBuilder::hooks_mut`]).
//! - [`run`]: the public [`StreamBuilder::run`] entry
//!   point that snapshots the system prompt, builds the
//!   [`BuilderSteps`] and hands off to the per-iteration
//!   loop. Split into submodules:
//!   - [`run::entry`]: the public [`StreamBuilder::run`]
//!     entry point.
//!   - [`run::main_loop`]: the internal
//!     [`StreamBuilder::run_with_steps`] method
//!     containing the `async_stream::stream!` block.
//! - [`iteration`]: the per-iteration body of the
//!   `StreamBuilder` ReAct loop — drain steering,
//!   self-reflection check, compact check, LLM sampling
//!   (with the LLM recovery cascade), doom-loop detection.
//!   Tool execution is split out to [`tool_execution`]
//!   because its recovery cascade is the most complex
//!   piece in the loop.
//! - [`tool_execution`]: the per-iteration tool-execution
//!   pass — split into [`tool_execution::types`]
//!   (the [`ToolExecuteOutcome`](tool_execution::ToolExecuteOutcome) enum) and
//!   [`tool_execution::execute`] (`before_tool` hook processing,
//!   the `StepToolExecute::execute` call, the tool-execution
//!   recovery cascade (L3-L5), and the L1 truncation
//!   service that yields the `ToolCallCompleted` events).
//! - [`tests`]: the 2 doom-loop unit tests that verify
//!   the unified `LoopDetectorSet` triggers after 3
//!   identical calls and does NOT trigger on alternating
//!   args.
//!
//! [`AgentEvent`]: crate::events::AgentEvent
//! [`ContextAssembler`]: synthia_context::assembler::ContextAssembler
//! [`HookBuilder`]: super::hook_builder::HookBuilder

mod construct;
mod iteration;
mod run;
mod setters;
mod tests;
mod tool_execution;
mod types;

pub use types::{AgentBuilder, BuilderSteps, StreamBuilder};
