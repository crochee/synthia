//! The [`ResetScope`] enum — the 3 possible scopes for an L5
//! reset.

/// Represents the scope of a reset operation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ResetScope {
    /// Reset only the current conversation.
    Conversation,
    /// Reset conversation and tool state.
    ToolState,
    /// Full reset of agent state.
    Full,
}
