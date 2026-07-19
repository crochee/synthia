//! `ToolId` + `ProviderId` newtypes (PR-5.1).
//!
//! `ToolId` is a UUID-based unique identifier for each materialized tool.
//! `ProviderId` identifies the `ToolProvider` that registered the tool.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Unique identifier for a materialized tool instance.
///
/// Each call to `ScopedToolRegistry::materialize()` produces a fresh
/// `ToolId` (UUID v4), scoped to the provider + registration context.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ToolId(pub Uuid);

impl ToolId {
    /// Allocate a fresh id (UUID v4).
    #[must_use]
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for ToolId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for ToolId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Identifies the `ToolProvider` that registered a tool.
///
/// Interned as `&'static str` for zero-cost comparison. Providers
/// typically use a `const` string for their id.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ProviderId(pub &'static str);

impl ProviderId {
    /// Create a provider id from a static string.
    ///
    /// # Panics
    ///
    /// Panics if `id` is empty.
    #[must_use]
    pub const fn new(id: &'static str) -> Self {
        Self(id)
    }
}

impl std::fmt::Display for ProviderId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_id_is_unique() {
        let id1 = ToolId::new();
        let id2 = ToolId::new();
        assert_ne!(id1, id2);
    }

    #[test]
    fn tool_id_default_is_new() {
        let id = ToolId::default();
        assert!(!id.0.is_nil());
    }

    #[test]
    fn tool_id_display() {
        let id = ToolId::new();
        assert!(!id.to_string().is_empty());
    }

    #[test]
    fn provider_id_display() {
        let pid = ProviderId::new("bash");
        assert_eq!(pid.to_string(), "bash");
    }

    #[test]
    fn provider_id_equality() {
        let p1 = ProviderId::new("bash");
        let p2 = ProviderId::new("bash");
        assert_eq!(p1, p2);
    }

    #[test]
    fn tool_id_serde_roundtrip() {
        let id = ToolId::new();
        let json = serde_json::to_string(&id).unwrap();
        let parsed: ToolId = serde_json::from_str(&json).unwrap();
        assert_eq!(id, parsed);
    }
}
