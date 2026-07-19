//! `ToolProvenance` enum (PR-5.3).
//!
//! Distinguishes the origin of a tool for audit purposes:
//! builtin (shipped with Synthia), plugin (from an extension), or
//! ephemeral (short-lived / session-scoped).

use serde::{Deserialize, Serialize};

/// The origin of a tool.
///
/// Recorded in [`Materialization::provenance`](crate::Materialization::provenance)
/// so that audits can trace where each tool came from.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ToolProvenance {
    /// Built into Synthia (e.g., bash, read, write).
    Builtin,
    /// Provided by a plugin/extension.
    Plugin {
        /// The extension id that registered this tool.
        extension_id: String,
    },
    /// Ephemeral / session-scoped tool (e.g., dynamically generated).
    Ephemeral {
        /// The source id that created this tool.
        source_id: String,
    },
}

impl std::fmt::Display for ToolProvenance {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Builtin => write!(f, "builtin"),
            Self::Plugin { extension_id } => {
                write!(f, "plugin:{extension_id}")
            }
            Self::Ephemeral { source_id } => {
                write!(f, "ephemeral:{source_id}")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_display() {
        assert_eq!(ToolProvenance::Builtin.to_string(), "builtin");
    }

    #[test]
    fn plugin_display() {
        let prov = ToolProvenance::Plugin {
            extension_id: "my-ext".into(),
        };
        assert_eq!(prov.to_string(), "plugin:my-ext");
    }

    #[test]
    fn ephemeral_display() {
        let prov = ToolProvenance::Ephemeral {
            source_id: "capsule-1".into(),
        };
        assert_eq!(prov.to_string(), "ephemeral:capsule-1");
    }

    #[test]
    fn serde_roundtrip() {
        let variants = [
            ToolProvenance::Builtin,
            ToolProvenance::Plugin {
                extension_id: "ext".into(),
            },
            ToolProvenance::Ephemeral {
                source_id: "src".into(),
            },
        ];
        for prov in &variants {
            let json = serde_json::to_string(prov).unwrap();
            let parsed: ToolProvenance = serde_json::from_str(&json).unwrap();
            assert_eq!(*prov, parsed);
        }
    }
}
