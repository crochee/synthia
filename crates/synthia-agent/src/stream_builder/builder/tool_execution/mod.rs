//! Per-iteration tool execution with the L3-L5 recovery
//! cascade and the L1 truncation hook.
//!
//! Split out of [`super::run`] because the tool phase is
//! the most complex phase of the ReAct loop.
//!
//! Submodule layout:
//!
//! - [`types`]: the [`ToolExecuteOutcome`] enum that the
//!   caller pattern-matches on.
//! - [`execute`]: the [`execute_and_emit`] entry point
//!   and the [`collect_tool_calls`] hook helper.

mod execute;
mod types;

pub(super) use execute::execute_and_emit;
pub(super) use types::ToolExecuteOutcome;
