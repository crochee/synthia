//! Approval policy types — controls when the agent asks for user permission.

use serde::{Deserialize, Serialize};

use crate::id::SessionId;

/// Approval policy (mirrors codex `codex-rs/protocol/src/protocol.rs:807-855`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "policy", rename_all = "snake_case")]
#[non_exhaustive]
pub enum AskForApproval {
    /// Never ask; trust all commands.
    Never,
    /// Ask unless the command is in the trusted-command list.
    UnlessTrusted,
    /// Ask only when the command fails.
    OnFailure,
    /// Always ask (default for new sessions).
    OnRequest,
    /// Granular per-category policy.
    Granular(GranularApprovalConfig),
}

/// Granular per-category approval policy.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct GranularApprovalConfig {
    pub sandbox_approval: bool,
    pub rules: bool,
    pub skill_approval: bool,
    pub request_permissions: bool,
    pub mcp_elicitations: bool,
}

/// Decision for an approval request.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "decision", rename_all = "snake_case")]
#[non_exhaustive]
pub enum PermissionDecision {
    Approved,
    ApprovedForSession,
    ApprovedAlways,
    Denied { reason: String },
    Abort,
}

/// Per-tool approval requirement (mirrors codex `ExecApprovalRequirement`).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "requirement", rename_all = "snake_case")]
pub enum ExecApprovalRequirement {
    Skip,
    Forbidden,
    NeedsApproval,
}

/// Approval request from server to client.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct ApprovalRequest {
    pub session_id: SessionId,
    pub tool_name: String,
    pub args_summary: String,
    pub reason: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn granular_default_all_false() {
        let g = GranularApprovalConfig::default();
        assert!(!g.sandbox_approval);
        assert!(!g.rules);
        assert!(!g.skill_approval);
        assert!(!g.request_permissions);
        assert!(!g.mcp_elicitations);
    }

    #[test]
    fn decision_serde_roundtrip() {
        let d = PermissionDecision::Denied {
            reason: "policy".to_string(),
        };
        let json = serde_json::to_string(&d).unwrap();
        let parsed: PermissionDecision = serde_json::from_str(&json).unwrap();
        match parsed {
            PermissionDecision::Denied { reason } => {
                assert_eq!(reason, "policy")
            }
            _ => panic!("wrong variant"),
        }
    }
}
