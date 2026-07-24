//! One-line human-readable summary for [`ApprovalRequest`].
//!
//! The [`ApprovalRequest::action_summary`] method produces a short
//! `"{variant}: ..."` string used by Guardian UI, audit log tables,
//! and the simple heuristic [`SimpleGuardian::assess_risk`](crate::review::SimpleGuardian::assess_risk)
//! to surface the action being reviewed.

use crate::approval_request::types::ApprovalRequest;

impl ApprovalRequest {
    /// 获取操作摘要
    pub fn action_summary(&self) -> String {
        match self {
            Self::Shell { command, .. } => {
                format!("shell: {}", command.join(" "))
            }
            Self::ExecCommand { command, .. } => {
                format!("exec_command: {}", command.join(" "))
            }
            Self::ApplyPatch {
                files,
                change_count,
                ..
            } => format!(
                "apply_patch: {} files, {} changes",
                files.len(),
                change_count
            ),
            Self::NetworkAccess { target, host, .. } => {
                format!("network_access: {target} ({host})")
            }
            Self::McpToolCall {
                server, tool_name, ..
            } => format!("mcp_tool_call: {server}::{tool_name}"),
        }
    }
}
