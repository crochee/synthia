//! `ToolVisibility` enum (PR-5.1).

use serde::{Deserialize, Serialize};

/// Visibility of a materialized tool.
///
/// Controls when and how the tool appears in the tool list.
#[derive(
    Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize,
)]
pub enum ToolVisibility {
    /// Always visible in the tool list.
    #[default]
    Always,
    /// Dynamically visible based on a schedule or condition.
    Dynamic {
        /// Schedule expression (e.g., cron-like or conditional).
        schedule: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_always() {
        assert_eq!(ToolVisibility::default(), ToolVisibility::Always);
    }

    #[test]
    fn serde_roundtrip_always() {
        let vis = ToolVisibility::Always;
        let json = serde_json::to_string(&vis).unwrap();
        let parsed: ToolVisibility = serde_json::from_str(&json).unwrap();
        assert_eq!(vis, parsed);
    }

    #[test]
    fn serde_roundtrip_dynamic() {
        let vis = ToolVisibility::Dynamic {
            schedule: "0 * * * *".into(),
        };
        let json = serde_json::to_string(&vis).unwrap();
        let parsed: ToolVisibility = serde_json::from_str(&json).unwrap();
        assert_eq!(vis, parsed);
    }
}
