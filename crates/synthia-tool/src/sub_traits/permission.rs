//! `ToolPermission` trait — a thin abstraction over permission decisions.
//!
//! Provides a simplified interface for tool-level permission checks,
//! distinct from the full `PermissionChecker` in `synthia-permission`.
//! The `ToolPermission` trait is designed for injection into the
//! `ToolExecution::execute()` path, enabling per-tool permission gating
//! without coupling to the full policy engine.

use serde::Serialize;
use uuid::Uuid;

/// A permission decision returned by a [`ToolPermission`] check.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionDecision {
    /// The tool invocation is allowed.
    Allow,
    /// The tool invocation is denied with a reason.
    Deny(String),
    /// The tool invocation requires user approval.
    Ask,
}

/// Context provided to a [`ToolPermission::check`] call.
#[derive(Debug, Clone)]
pub struct PermissionContext {
    /// Name of the tool being invoked.
    pub tool_name: String,
    /// Raw JSON arguments for the tool invocation.
    pub arguments: serde_json::Value,
    /// Unique identifier for the agent run.
    pub agent_run_id: Uuid,
    /// Optional user identifier for user-scoped permissions.
    pub user_id: Option<String>,
}

impl PermissionContext {
    /// Create a new permission context.
    pub fn new(
        tool_name: String,
        arguments: serde_json::Value,
        agent_run_id: Uuid,
    ) -> Self {
        Self {
            tool_name,
            arguments,
            agent_run_id,
            user_id: None,
        }
    }

    /// Attach a user identifier.
    pub fn with_user_id(mut self, user_id: String) -> Self {
        self.user_id = Some(user_id);
        self
    }
}

/// Trait for tool-level permission decisions.
///
/// Implementations can range from a simple "always allow" to a
/// sophisticated policy engine that consults user preferences,
/// tool categories, and argument analysis.
///
/// This trait is intentionally simple (1 method) to keep the
/// permission decision path O(1) and side-effect-free.
pub trait ToolPermission: Send + Sync + 'static {
    /// Check whether a tool invocation is permitted.
    fn check(&self, ctx: &PermissionContext) -> PermissionDecision;
}

/// Default permission implementation that always allows.
///
/// Suitable for trusted contexts (e.g. tests, headless mode)
/// where all tool invocations are pre-approved.
pub struct PermissionAlwaysAllow;

impl ToolPermission for PermissionAlwaysAllow {
    fn check(&self, _ctx: &PermissionContext) -> PermissionDecision {
        PermissionDecision::Allow
    }
}

/// Permission implementation that always denies.
///
/// Suitable for locked-down contexts where no tool
/// invocations are allowed.
pub struct PermissionAlwaysDeny;

impl ToolPermission for PermissionAlwaysDeny {
    fn check(&self, _ctx: &PermissionContext) -> PermissionDecision {
        PermissionDecision::Deny("all tool invocations denied".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn always_allow_permits_everything() {
        let perm = PermissionAlwaysAllow;
        let ctx = PermissionContext::new(
            "bash".to_string(),
            serde_json::json!({"command": "rm -rf /"}),
            Uuid::new_v4(),
        );
        assert_eq!(perm.check(&ctx), PermissionDecision::Allow);
    }

    #[test]
    fn always_deny_blocks_everything() {
        let perm = PermissionAlwaysDeny;
        let ctx = PermissionContext::new(
            "read".to_string(),
            serde_json::json!({"path": "/tmp/test"}),
            Uuid::new_v4(),
        );
        assert!(matches!(perm.check(&ctx), PermissionDecision::Deny(_)));
    }

    #[test]
    fn permission_context_builder() {
        let ctx = PermissionContext::new(
            "write".to_string(),
            serde_json::json!({}),
            Uuid::new_v4(),
        )
        .with_user_id("alice".to_string());
        assert_eq!(ctx.tool_name, "write");
        assert_eq!(ctx.user_id.as_deref(), Some("alice"));
    }
}
