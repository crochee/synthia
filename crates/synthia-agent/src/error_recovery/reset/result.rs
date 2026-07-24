//! The [`ResetResult`] struct + its 2 constructors.

use super::scope::ResetScope;

/// Result of a reset attempt.
#[derive(Debug, Clone)]
pub struct ResetResult {
    /// Whether the reset was successful.
    pub success: bool,
    /// The scope of the reset.
    pub scope: ResetScope,
    /// Description of what was reset.
    pub description: String,
}

impl ResetResult {
    /// Creates a successful reset result.
    pub fn success(scope: ResetScope, description: impl Into<String>) -> Self {
        Self {
            success: true,
            scope,
            description: description.into(),
        }
    }

    /// Creates a failed reset result.
    pub fn failed(scope: ResetScope, reason: impl Into<String>) -> Self {
        Self {
            success: false,
            scope,
            description: reason.into(),
        }
    }
}
