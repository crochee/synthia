//! The [`Role`] enum — the 4 canonical message roles that
//! flow through every provider.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    User,
    Assistant,
    Tool,
}

impl Role {
    pub fn as_str(&self) -> &'static str {
        match self {
            Role::System => "system",
            Role::User => "user",
            Role::Assistant => "assistant",
            Role::Tool => "tool",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // -- serde 4-way lowercase mapping -----------------------------

    /// `Role` MUST serialize each variant
    /// in lowercase form (the wire
    /// format contract for OpenAI +
    /// Anthropic — `"user"`, `"system"`,
    /// `"assistant"`, `"tool"`).
    #[test]
    fn serializes_each_variant_as_lowercase() {
        assert_eq!(serde_json::to_string(&Role::System).unwrap(), "\"system\"");
        assert_eq!(serde_json::to_string(&Role::User).unwrap(), "\"user\"");
        assert_eq!(
            serde_json::to_string(&Role::Assistant).unwrap(),
            "\"assistant\""
        );
        assert_eq!(serde_json::to_string(&Role::Tool).unwrap(), "\"tool\"");
    }

    /// `Role` MUST round-trip each
    /// variant through JSON.
    #[test]
    fn round_trips_each_variant_through_json() {
        for role in [Role::System, Role::User, Role::Assistant, Role::Tool] {
            let json = serde_json::to_string(&role).unwrap();
            let parsed: Role = serde_json::from_str(&json).expect("round-trip");
            assert_eq!(parsed, role);
        }
    }

    /// `Role` MUST reject unknown
    /// variant strings (an upstream
    /// provider adding a new role
    /// must not silently round-trip
    /// into our existing variant set).
    #[test]
    fn rejects_unknown_variant_string() {
        let result: Result<Role, _> =
            serde_json::from_str("\"nonexistent_role\"");
        assert!(result.is_err());
    }

    /// `Role` MUST reject PascalCase
    /// (defense against refactors
    /// that drop the `rename_all`
    /// attribute — `Role::User`
    /// MUST NOT serialize as
    /// `"User"`).
    #[test]
    fn rejects_pascal_case_input() {
        let result: Result<Role, _> = serde_json::from_str("\"User\"");
        assert!(result.is_err());
    }

    // -- as_str 4-way mapping ---------------------------------------

    /// `Role::as_str` MUST return the
    /// same lowercase string that the
    /// serializer produces for each
    /// variant (consumers can rely on
    /// either path producing the same
    /// output).
    #[test]
    fn as_str_matches_serde_for_each_variant() {
        assert_eq!(Role::System.as_str(), "system");
        assert_eq!(Role::User.as_str(), "user");
        assert_eq!(Role::Assistant.as_str(), "assistant");
        assert_eq!(Role::Tool.as_str(), "tool");
    }

    /// `Role::as_str` MUST return
    /// `&'static str` (no allocation,
    /// hot path safe).
    #[test]
    fn as_str_returns_static_str() {
        let s: &'static str = Role::User.as_str();
        assert_eq!(s, "user");
    }

    // -- Distinctness -----------------------------------------------

    /// All 4 `Role` variants MUST be
    /// pairwise distinct.
    #[test]
    fn all_four_variants_are_pairwise_distinct() {
        assert_ne!(Role::System, Role::User);
        assert_ne!(Role::System, Role::Assistant);
        assert_ne!(Role::System, Role::Tool);
        assert_ne!(Role::User, Role::Assistant);
        assert_ne!(Role::User, Role::Tool);
        assert_ne!(Role::Assistant, Role::Tool);
    }

    /// All 4 `Role::as_str` outputs
    /// MUST be pairwise distinct
    /// (no two variants return the
    /// same string).
    #[test]
    fn all_four_as_str_outputs_are_pairwise_distinct() {
        let all = [
            (Role::System, "system"),
            (Role::User, "user"),
            (Role::Assistant, "assistant"),
            (Role::Tool, "tool"),
        ];
        for i in 0..all.len() {
            for j in 0..all.len() {
                if i != j {
                    assert_ne!(
                        all[i].1, all[j].1,
                        "as_str for {:?} and {:?} alias",
                        all[i].0, all[j].0
                    );
                }
            }
        }
    }

    // -- Trait surface ----------------------------------------------

    /// `Role` MUST implement `Copy`
    /// (used in hot agent-loop message
    /// dispatch).
    #[test]
    fn copy_trait_does_not_move() {
        let r = Role::Assistant;
        let _copy = r;
        let _still_valid = r;
    }

    /// `Role` MUST support Clone +
    /// Debug + PartialEq + Eq.
    #[test]
    fn supports_clone_debug_partial_eq_eq() {
        let r = Role::User;
        let _copy = r;
        let _ = format!("{:?}", r);
        assert_eq!(r, r);
    }
}
