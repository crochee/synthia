//! Typed-event emission helpers used by [`super::ReActLoop`].
//!
//! This module owns the four typed-event wrappers
//! (`emit_iteration_start` / `emit_iteration_end` /
//! `emit_step_start` / `emit_step_end`) plus the `StepAction`
//! enum the step-boundary events carry. The wrappers are
//! deliberately tiny (one line each over `emit_typed`); they
//! exist so the loop body in [`super`] does not need to know
//! the names of the typed-event constructors. The `StepAction`
//! enum lives here because its only purpose is to feed
//! `emit_step_end`.

use synthia_session::{
    TypedEventRecord,
    iteration_end,
    iteration_start,
    step_end,
    step_start,
};

use super::ReActLoop;

impl ReActLoop {
    pub(in crate::agent::re_act) fn emit_typed(
        &self,
        event: synthia_session::SessionEvent,
    ) {
        if let Some(sink) = &self.typed_sink {
            sink.record(TypedEventRecord::new(event));
        }
    }

    pub(super) fn emit_iteration_start(&self, idx: u32) {
        self.emit_typed(iteration_start(idx));
    }

    pub(super) fn emit_iteration_end(&self, idx: u32, action: &'static str) {
        self.emit_typed(iteration_end(idx, action));
    }

    pub(super) fn emit_step_start(&self, turn_idx: u32, step_idx: u32) {
        self.emit_typed(step_start(turn_idx, step_idx));
    }

    pub(super) fn emit_step_end(
        &self,
        turn_idx: u32,
        step_idx: u32,
        action: StepAction,
    ) {
        self.emit_typed(step_end(turn_idx, step_idx, action.as_str()));
    }
}

/// How one ReAct step concluded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StepAction {
    ToolCall,
    FinalAnswer,
}

impl StepAction {
    const fn as_str(self) -> &'static str {
        match self {
            Self::ToolCall => "tool_call",
            Self::FinalAnswer => "final_answer",
        }
    }
}
