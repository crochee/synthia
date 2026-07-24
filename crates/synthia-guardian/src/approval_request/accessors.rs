//! Constructor methods and the `id` getter for [`ApprovalRequest`].
//!
//! Each variant has a dedicated `ApprovalRequest::{variant}(...)` constructor
//! that takes the strongly-typed payload and builds the enum case. The
//! [`ApprovalRequest::id`] getter is a `match`-based accessor that returns
//! the per-variant `id: String` field, used by audit logs and Guardian
//! UI to correlate requests across the system.

use crate::approval_request::types::ApprovalRequest;

impl ApprovalRequest {
    /// 获取请求 ID
    pub fn id(&self) -> &str {
        match self {
            Self::Shell { id, .. }
            | Self::ExecCommand { id, .. }
            | Self::ApplyPatch { id, .. }
            | Self::NetworkAccess { id, .. }
            | Self::McpToolCall { id, .. } => id,
        }
    }

    /// 创建 Shell 请求
    pub fn shell(
        id: impl Into<String>,
        command: Vec<String>,
        cwd: impl Into<String>,
        justification: Option<String>,
    ) -> Self {
        Self::Shell {
            id: id.into(),
            command,
            cwd: cwd.into(),
            justification,
        }
    }

    /// 创建 ExecCommand 请求
    pub fn exec_command(
        id: impl Into<String>,
        command: Vec<String>,
        cwd: impl Into<String>,
        justification: Option<String>,
        tty: bool,
    ) -> Self {
        Self::ExecCommand {
            id: id.into(),
            command,
            cwd: cwd.into(),
            justification,
            tty,
        }
    }

    /// 创建 ApplyPatch 请求
    pub fn apply_patch(
        id: impl Into<String>,
        cwd: impl Into<String>,
        files: Vec<String>,
        change_count: usize,
        patch: impl Into<String>,
    ) -> Self {
        Self::ApplyPatch {
            id: id.into(),
            cwd: cwd.into(),
            files,
            change_count,
            patch: patch.into(),
        }
    }

    /// 创建 NetworkAccess 请求
    pub fn network_access(
        id: impl Into<String>,
        target: impl Into<String>,
        host: impl Into<String>,
        protocol: impl Into<String>,
        port: u16,
    ) -> Self {
        Self::NetworkAccess {
            id: id.into(),
            target: target.into(),
            host: host.into(),
            protocol: protocol.into(),
            port,
        }
    }

    /// 创建 McpToolCall 请求
    pub fn mcp_tool_call(
        id: impl Into<String>,
        server: impl Into<String>,
        tool_name: impl Into<String>,
        arguments: Option<serde_json::Value>,
    ) -> Self {
        Self::McpToolCall {
            id: id.into(),
            server: server.into(),
            tool_name: tool_name.into(),
            arguments,
            connector_id: None,
            connector_name: None,
            tool_title: None,
            tool_description: None,
            annotations: None,
        }
    }
}
