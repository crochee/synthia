//! Core data types for the Guardian approval system.
//!
//! Defines the [`ApprovalRequest`] enum (5 variants covering every action
//! the agent may submit to Guardian for review) and the
//! [`McpAnnotations`] helper struct used by the McpToolCall variant.

use serde::{Deserialize, Serialize};

/// 审批请求类型
#[derive(Debug, Clone, PartialEq)]
pub enum ApprovalRequest {
    Shell {
        id: String,
        command: Vec<String>,
        cwd: String,
        justification: Option<String>,
    },
    ExecCommand {
        id: String,
        command: Vec<String>,
        cwd: String,
        justification: Option<String>,
        tty: bool,
    },
    ApplyPatch {
        id: String,
        cwd: String,
        files: Vec<String>,
        change_count: usize,
        patch: String,
    },
    NetworkAccess {
        id: String,
        target: String,
        host: String,
        protocol: String,
        port: u16,
    },
    McpToolCall {
        id: String,
        server: String,
        tool_name: String,
        arguments: Option<serde_json::Value>,
        connector_id: Option<String>,
        connector_name: Option<String>,
        tool_title: Option<String>,
        tool_description: Option<String>,
        annotations: Option<McpAnnotations>,
    },
}

/// MCP 工具注解
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpAnnotations {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub destructive_hint: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub open_world_hint: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub read_only_hint: Option<bool>,
}
