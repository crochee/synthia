//! Phase outcome enums for the per-iteration helpers.

/// Outcome of the per-iteration compact check.
pub(crate) enum CompactOutcome {
    /// No budget configured or well below threshold.
    None,
    /// Over the warning threshold but not the
    /// must-compact threshold. Caller yields a
    /// `TokenBudgetWarning` event.
    Warning,
    /// Over the must-compact threshold. Caller yields
    /// `ContextCompacted` + `TokenBudgetWarning`, then
    /// `continue`s the loop.
    MustCompact {
        old_tokens: usize,
        new_tokens: usize,
    },
}

/// Outcome of the LLM sampling + recovery cascade phase.
pub(crate) enum LlmSampleOutcome {
    /// LLM sampling succeeded. Caller yields the
    /// contained events (text deltas) before continuing
    /// to the post-LLM phase.
    Done {
        sampling: synthia_provider::types::SamplingResult,
        events: Vec<crate::events::AgentEvent>,
    },
    /// Recovery cascade recovered. Caller yields the
    /// contained events and `continue`s the loop.
    Continue {
        events: Vec<crate::events::AgentEvent>,
    },
    /// Recovery cascade exhausted. Caller yields the
    /// contained events and `return`s from the stream.
    Terminate {
        events: Vec<crate::events::AgentEvent>,
    },
}
