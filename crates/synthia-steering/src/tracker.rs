//! [`Tracker`] — synchronous observation of run progress plus the
//! concurrency recommendation consumed by the tool batch
//! scheduler.
//!
//! The tracker is the *read* side of the run state: the agent loop
//! is the single writer of [`AgentState`] (it bumps counters,
//! records fingerprints, accumulates usage), and the tracker is
//! notified at every boundary so it can log, meter, and adapt.
//! This split fixes traitclaw's wiring gap where
//! `Tracker::on_tool_call` was declared but never invoked, and its
//! interior-mutability counters that leaked state across runs.

use serde_json::Value;
use synthia_context::AgentState;
use synthia_provider::TokenUsage;

/// Run-progress observer.
pub trait Tracker: Send + Sync {
    /// Called at the top of every ReAct iteration.
    fn on_iteration(&self, state: &AgentState);

    /// Called after every dispatched tool call (including ones
    /// later denied by a guard — the observation happens at the
    /// dispatch boundary).
    fn on_tool_call(&self, name: &str, arguments: &Value, state: &AgentState);

    /// Called after every provider response with its reported
    /// usage (already accumulated into
    /// [`AgentState::total_tokens`] by the loop).
    fn on_llm_response(&self, usage: &TokenUsage, state: &AgentState);

    /// Upper bound on concurrent tool executions the batch
    /// scheduler should use for the upcoming round.
    fn recommended_concurrency(&self, state: &AgentState) -> usize;
}

/// Neutral tracker: observes nothing, never limits concurrency.
pub struct NoopTracker;

impl Tracker for NoopTracker {
    fn on_iteration(&self, _state: &AgentState) {}

    fn on_tool_call(
        &self,
        _name: &str,
        _arguments: &Value,
        _state: &AgentState,
    ) {
    }

    fn on_llm_response(&self, _usage: &TokenUsage, _state: &AgentState) {}

    fn recommended_concurrency(&self, _state: &AgentState) -> usize {
        usize::MAX
    }
}

/// Context-pressure-aware concurrency.
///
/// Full concurrency while the window is roomy; steps down as
/// utilisation climbs so a crowded context does not also carry a
/// fan-out of in-flight tool results that will need re-budgeting:
///
/// | utilisation | concurrency |
/// |---|---|
/// | < 75% | `max_concurrency` |
/// | 75%–90% | 2 |
/// | ≥ 90% | 1 (serialise) |
pub struct AdaptiveTracker {
    max_concurrency: usize,
}

impl AdaptiveTracker {
    /// Cap concurrency at `max_concurrency` while the context is
    /// roomy.
    pub fn new(max_concurrency: usize) -> Self {
        Self {
            max_concurrency: max_concurrency.max(1),
        }
    }
}

impl Tracker for AdaptiveTracker {
    fn on_iteration(&self, state: &AgentState) {
        tracing::debug!(
            iteration = state.iteration_count,
            estimated_tokens = state.estimated_tokens,
            total_tokens = state.total_tokens,
            tool_calls = state.tool_call_count,
            "tracker: iteration boundary"
        );
    }

    fn on_tool_call(&self, name: &str, arguments: &Value, state: &AgentState) {
        tracing::debug!(
            tool = name,
            tool_call_count = state.tool_call_count,
            "tracker: tool dispatched"
        );
        let _ = arguments;
    }

    fn on_llm_response(&self, usage: &TokenUsage, state: &AgentState) {
        tracing::debug!(
            prompt_tokens = usage.prompt_tokens,
            completion_tokens = usage.completion_tokens,
            total_tokens = state.total_tokens,
            "tracker: llm response"
        );
    }

    fn recommended_concurrency(&self, state: &AgentState) -> usize {
        let utilisation = state.context_utilization();
        if utilisation >= 0.9 {
            1
        } else if utilisation >= 0.75 {
            2
        } else {
            self.max_concurrency
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn state(utilisation: f32) -> AgentState {
        let mut s = AgentState::with_window(1000);
        s.estimated_tokens = (utilisation * 1000.0) as usize;
        s
    }

    /// NoopTracker MUST never limit concurrency.
    #[test]
    fn noop_tracker_never_limits() {
        assert_eq!(
            NoopTracker.recommended_concurrency(&state(1.0)),
            usize::MAX
        );
    }

    /// AdaptiveTracker MUST step down by utilisation band.
    #[test]
    fn adaptive_tracker_bands() {
        let tracker = AdaptiveTracker::new(8);
        assert_eq!(tracker.recommended_concurrency(&state(0.5)), 8);
        assert_eq!(tracker.recommended_concurrency(&state(0.8)), 2);
        assert_eq!(tracker.recommended_concurrency(&state(0.95)), 1);
    }

    /// Observation callbacks MUST be callable without panicking
    /// (they are fire-and-forget seams).
    #[test]
    fn observation_callbacks_are_safe() {
        let tracker = AdaptiveTracker::new(4);
        let mut s = state(0.1);
        s.record_tool_call("read".to_string());
        tracker.on_iteration(&s);
        tracker.on_tool_call("read", &json!({"file_path": "a"}), &s);
        tracker.on_llm_response(
            &TokenUsage {
                prompt_tokens: 10,
                completion_tokens: 5,
                total_tokens: 15,
                reasoning_tokens: None,
                cached_prompt_tokens: None,
                cache_read_tokens: None,
                cache_write_tokens: None,
            },
            &s,
        );
    }
}
