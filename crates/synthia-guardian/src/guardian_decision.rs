//! Guardian decision types with action-type awareness.
//!
//! This module defines the GuardianDecision enum and ActionType for
//! the hybrid Guardian layer.

use std::time::Duration;

use crate::ApprovalRequest;

/// Action type for confirmation routing
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ActionType {
    Shell,
    Network,
    Credential,
}

impl ActionType {
    /// Derive action type from an ApprovalRequest
    pub fn from_approval_request(request: &ApprovalRequest) -> Self {
        match request {
            ApprovalRequest::Shell { .. }
            | ApprovalRequest::ExecCommand { .. } => ActionType::Shell,
            ApprovalRequest::NetworkAccess { .. } => ActionType::Network,
            ApprovalRequest::ApplyPatch { .. }
            | ApprovalRequest::McpToolCall { .. } => ActionType::Credential,
        }
    }

    /// Default timeout for user confirmation
    pub fn default_timeout(&self) -> Duration {
        match self {
            ActionType::Shell => Duration::from_secs(300), // 5 min
            ActionType::Network => Duration::from_secs(60), // 1 min
            ActionType::Credential => Duration::from_secs(120), // 2 min
        }
    }

    /// Whether this action type requires blocking confirmation
    pub fn is_blocking(&self) -> bool {
        matches!(self, ActionType::Shell | ActionType::Credential)
    }
}

/// Guardian decision with action-type awareness
#[derive(Debug, Clone)]
pub enum GuardianDecision {
    Allow,
    Deny {
        reason: String,
    },
    NeedUserConfirm {
        request: Box<ApprovalRequest>,
        timeout: Duration,
        blocking: bool,
        action_type: ActionType,
    },
}

impl GuardianDecision {
    /// Returns true if the decision allows the action
    pub fn is_allowed(&self) -> bool {
        matches!(self, GuardianDecision::Allow)
    }

    /// Returns the action type if this is a confirmation request
    pub fn action_type(&self) -> Option<ActionType> {
        match self {
            GuardianDecision::NeedUserConfirm { action_type, .. } => {
                Some(*action_type)
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_guardian_decision_is_allowed() {
        assert!(GuardianDecision::Allow.is_allowed());
        assert!(
            !GuardianDecision::Deny {
                reason: "test".to_string()
            }
            .is_allowed()
        );
    }

    #[test]
    fn test_action_type_from_shell_request() {
        let shell =
            ApprovalRequest::shell("id", vec!["ls".to_string()], "/", None);
        assert_eq!(
            ActionType::from_approval_request(&shell),
            ActionType::Shell
        );
    }

    #[test]
    fn test_action_type_from_exec_command() {
        let exec = ApprovalRequest::exec_command(
            "id",
            vec!["pwd".to_string()],
            "/",
            None,
            false,
        );
        assert_eq!(ActionType::from_approval_request(&exec), ActionType::Shell);
    }

    #[test]
    fn test_action_type_from_network_access() {
        let network = ApprovalRequest::network_access(
            "id", "target", "host", "https", 443,
        );
        assert_eq!(
            ActionType::from_approval_request(&network),
            ActionType::Network
        );
    }

    #[test]
    fn test_action_type_from_apply_patch() {
        let patch = ApprovalRequest::apply_patch("id", "/", vec![], 0, "");
        assert_eq!(
            ActionType::from_approval_request(&patch),
            ActionType::Credential
        );
    }

    #[test]
    fn test_action_type_from_mcp_tool_call() {
        let mcp = ApprovalRequest::mcp_tool_call("id", "server", "tool", None);
        assert_eq!(
            ActionType::from_approval_request(&mcp),
            ActionType::Credential
        );
    }

    #[test]
    fn test_action_type_default_timeout() {
        assert_eq!(
            ActionType::Shell.default_timeout(),
            Duration::from_secs(300)
        );
        assert_eq!(
            ActionType::Network.default_timeout(),
            Duration::from_secs(60)
        );
        assert_eq!(
            ActionType::Credential.default_timeout(),
            Duration::from_secs(120)
        );
    }

    #[test]
    fn test_action_type_is_blocking() {
        assert!(ActionType::Shell.is_blocking());
        assert!(!ActionType::Network.is_blocking());
        assert!(ActionType::Credential.is_blocking());
    }

    #[test]
    fn test_need_user_confirm_has_action_type() {
        let shell =
            ApprovalRequest::shell("id", vec!["ls".to_string()], "/", None);
        let decision = GuardianDecision::NeedUserConfirm {
            request: Box::new(shell.clone()),
            timeout: Duration::from_secs(300),
            blocking: true,
            action_type: ActionType::Shell,
        };
        assert_eq!(decision.action_type(), Some(ActionType::Shell));
    }

    #[test]
    fn test_allow_has_no_action_type() {
        let decision = GuardianDecision::Allow;
        assert_eq!(decision.action_type(), None);
    }

    #[test]
    fn test_deny_has_no_action_type() {
        let decision = GuardianDecision::Deny {
            reason: "test".to_string(),
        };
        assert_eq!(decision.action_type(), None);
    }
}
