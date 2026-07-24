//! Per-variant JSON serialization for [`ApprovalRequest`].
//!
//! The [`ApprovalRequest::to_json`] method produces a `serde_json::Value`
//! representation used by audit logs and the LLM-driven Guardian reviewer
//! prompts. The shape is intentionally narrow (only the fields the
//! Guardian needs to make a decision), excluding the `id` field to keep
//! log volume bounded.

use serde_json::{Value, json};

use crate::approval_request::types::ApprovalRequest;

impl ApprovalRequest {
    /// 转换为 JSON 表示
    pub fn to_json(&self) -> serde_json::Result<Value> {
        match self {
            Self::Shell {
                command,
                cwd,
                justification,
                ..
            } => Ok(json!({
                "tool": "shell",
                "command": command.join(" "),
                "cwd": cwd,
                "justification": justification,
            })),
            Self::ExecCommand {
                command,
                cwd,
                justification,
                tty,
                ..
            } => Ok(json!({
                "tool": "exec_command",
                "command": command.join(" "),
                "cwd": cwd,
                "justification": justification,
                "tty": tty,
            })),
            Self::ApplyPatch {
                cwd,
                files,
                change_count,
                patch,
                ..
            } => Ok(json!({
                "tool": "apply_patch",
                "cwd": cwd,
                "files": files,
                "change_count": change_count,
                "patch": patch,
            })),
            Self::NetworkAccess {
                target,
                host,
                protocol,
                port,
                ..
            } => Ok(json!({
                "tool": "network_access",
                "target": target,
                "host": host,
                "protocol": protocol,
                "port": port,
            })),
            Self::McpToolCall {
                server,
                tool_name,
                arguments,
                ..
            } => Ok(json!({
                "tool": "mcp_tool_call",
                "server": server,
                "tool_name": tool_name,
                "arguments": arguments,
            })),
        }
    }
}
