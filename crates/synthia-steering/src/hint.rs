//! [`Hint`] — advisory context injections that steer the model
//! without hard vetoes.
//!
//! Where a [`Guard`](crate::Guard) *blocks*, a hint *nudges*: it
//! appends a short reminder into the conversation when the run's
//! [`AgentState`] crosses a threshold (iteration count, context
//! utilisation, truncation). The agent loop consults hints right
//! before each LLM call and at tool-result commit time.
//!
//! Injection points are honoured by the loop (unlike traitclaw,
//! where every hint lands at the tail regardless of its declared
//! point):
//!
//! - [`InjectionPoint::BeforeNextLlmCall`] / [`InjectionPoint::RecencyZone`]
//!   → a `[reminder]`-prefixed user message right before the next
//!   sampling pass (in a ReAct loop the recency zone *is* the
//!   message tail, so the two coincide).
//! - [`InjectionPoint::SystemPrompt`] → appended to the system
//!   message once at session start.
//! - [`InjectionPoint::AppendToToolResult`] → appended to the
//!   matching tool result's content at commit time.

use synthia_context::AgentState;

/// Where the loop should place a hint's message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InjectionPoint {
    /// Append to the system prompt (evaluated once, at session
    /// start).
    SystemPrompt,
    /// Insert immediately before the next LLM call.
    BeforeNextLlmCall,
    /// Insert at the tail of the conversation (highest-attention
    /// zone). In a ReAct loop this is implemented identically to
    /// [`InjectionPoint::BeforeNextLlmCall`].
    RecencyZone,
    /// Append to the result of every call to `tool_name`.
    AppendToToolResult { tool_name: String },
}

/// Urgency of a hint. `Low` hints are dropped when the context is
/// already crowded (utilisation ≥ 80%); `Normal` and `Critical`
/// always inject.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum HintPriority {
    /// Best-effort: skipped under context pressure.
    Low,
    /// Always injected.
    Normal,
    /// Always injected (reserved for budget-exhaustion class
    /// reminders).
    Critical,
}

/// The message a hint produces.
#[derive(Debug, Clone)]
pub struct HintMessage {
    /// Reminder body (without the `[reminder]` prefix — the loop
    /// adds it).
    pub content: String,
    pub priority: HintPriority,
}

/// Advisory, state-triggered context injection.
pub trait Hint: Send + Sync {
    /// Stable hint name for logs.
    fn name(&self) -> &str;

    /// Decide whether to fire for this state. MUST be pure (no
    /// mutation, no I/O).
    fn should_trigger(&self, state: &AgentState) -> bool;

    /// Produce the message to inject. Only called when
    /// [`Hint::should_trigger`] returned `true`.
    fn generate(&self, state: &AgentState) -> HintMessage;

    /// Where the loop should place the message.
    fn injection_point(&self) -> InjectionPoint;
}

/// Never fires. The neutral element so callers never hold an
/// `Option<Hint>`.
pub struct NoopHint;

impl Hint for NoopHint {
    fn name(&self) -> &str {
        "noop"
    }

    fn should_trigger(&self, _state: &AgentState) -> bool {
        false
    }

    fn generate(&self, _state: &AgentState) -> HintMessage {
        HintMessage {
            content: String::new(),
            priority: HintPriority::Low,
        }
    }

    fn injection_point(&self) -> InjectionPoint {
        InjectionPoint::BeforeNextLlmCall
    }
}

/// Periodically remind long-running agents to converge.
///
/// Fires every `every` iterations (from the first multiple
/// onwards): at iteration counts where a healthy run should be
/// wrapping up, the reminder tells the model to either finish or
/// make genuinely new progress.
pub struct IterationReminderHint {
    every: usize,
}

impl IterationReminderHint {
    /// Fire every `every` iterations (`>= 2`; 1 would fire on
    /// every pass).
    pub fn new(every: usize) -> Self {
        Self {
            every: every.max(2),
        }
    }
}

impl Hint for IterationReminderHint {
    fn name(&self) -> &str {
        "iteration_reminder"
    }

    fn should_trigger(&self, state: &AgentState) -> bool {
        state.iteration_count >= self.every
            && state.iteration_count.is_multiple_of(self.every)
    }

    fn generate(&self, state: &AgentState) -> HintMessage {
        HintMessage {
            content: format!(
                "You have been working for {} iterations. If the task is \
                 essentially complete, stop calling tools and give your \
                 final answer now; if not, make sure each further step is \
                 new progress rather than a repeat of earlier work",
                state.iteration_count
            ),
            priority: HintPriority::Normal,
        }
    }

    fn injection_point(&self) -> InjectionPoint {
        InjectionPoint::RecencyZone
    }
}

/// Warn when the context window is running out.
///
/// Fires once the post-`prepare` token estimate crosses
/// `threshold` of the window, nudging the model toward
/// summarising instead of re-reading large outputs.
pub struct ContextBudgetHint {
    threshold: f32,
}

impl ContextBudgetHint {
    /// Fire at this fraction of context utilisation
    /// (`0.0 < threshold <= 1.0`, clamped).
    pub fn new(threshold: f32) -> Self {
        Self {
            threshold: threshold.clamp(0.05, 1.0),
        }
    }
}

impl Hint for ContextBudgetHint {
    fn name(&self) -> &str {
        "context_budget"
    }

    fn should_trigger(&self, state: &AgentState) -> bool {
        state.context_utilization() >= self.threshold
    }

    fn generate(&self, state: &AgentState) -> HintMessage {
        let percent = (state.context_utilization() * 100.0).round() as u32;
        HintMessage {
            content: format!(
                "Roughly {percent}% of the context window is in use. Avoid \
                 re-reading large tool outputs you have already seen; rely \
                 on what is already in the conversation, summarise \
                 intermediate findings, and move toward your final answer"
            ),
            priority: HintPriority::Critical,
        }
    }

    fn injection_point(&self) -> InjectionPoint {
        InjectionPoint::RecencyZone
    }
}

/// Note that the last tool output was truncated, so the model
/// does not burn iterations re-fetching the missing tail.
pub struct TruncationHint;

impl Hint for TruncationHint {
    fn name(&self) -> &str {
        "truncation"
    }

    fn should_trigger(&self, state: &AgentState) -> bool {
        state.last_truncated
    }

    fn generate(&self, _state: &AgentState) -> HintMessage {
        HintMessage {
            content: "The most recent tool output was truncated to fit the \
                      context budget. Do not re-run the same tool to recover \
                      the missing tail — work with what you have, or query a \
                      narrower range"
                .to_string(),
            priority: HintPriority::Normal,
        }
    }

    fn injection_point(&self) -> InjectionPoint {
        InjectionPoint::BeforeNextLlmCall
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// NoopHint MUST never trigger.
    #[test]
    fn noop_hint_never_triggers() {
        let mut state = AgentState::with_window(1000);
        state.iteration_count = 100;
        assert!(!NoopHint.should_trigger(&state));
    }

    /// IterationReminderHint MUST fire on multiples of `every`
    /// (and never before the first multiple).
    #[test]
    fn iteration_reminder_fires_on_multiples() {
        let hint = IterationReminderHint::new(4);
        for iteration in [1usize, 2, 3, 5, 7] {
            let mut state = AgentState::with_window(1000);
            state.iteration_count = iteration;
            assert!(!hint.should_trigger(&state), "iteration {iteration}");
        }
        let mut state = AgentState::with_window(1000);
        state.iteration_count = 8;
        assert!(hint.should_trigger(&state));
        let msg = hint.generate(&state);
        assert!(msg.content.contains("8 iterations"));
        assert_eq!(msg.priority, HintPriority::Normal);
    }

    /// ContextBudgetHint MUST fire at/above the threshold and
    /// quote the utilisation.
    #[test]
    fn budget_hint_fires_above_threshold() {
        let hint = ContextBudgetHint::new(0.8);
        let mut state = AgentState::with_window(1000);
        state.estimated_tokens = 500;
        assert!(!hint.should_trigger(&state));
        state.estimated_tokens = 850;
        assert!(hint.should_trigger(&state));
        assert_eq!(hint.generate(&state).priority, HintPriority::Critical);
        assert!(hint.generate(&state).content.contains("85%"));
    }

    /// TruncationHint MUST track `last_truncated`.
    #[test]
    fn truncation_hint_tracks_flag() {
        let mut state = AgentState::with_window(1000);
        assert!(!TruncationHint.should_trigger(&state));
        state.last_truncated = true;
        assert!(TruncationHint.should_trigger(&state));
        assert_eq!(
            TruncationHint.injection_point(),
            InjectionPoint::BeforeNextLlmCall
        );
    }
}
