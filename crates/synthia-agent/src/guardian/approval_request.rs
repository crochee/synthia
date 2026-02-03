//! Guardian 审批请求类型
//!
//! 定义可提交给 Guardian 进行安全审查的审批请求类型。

use serde::{Deserialize, Serialize};
use serde_json::Value;

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
        turn_id: String,
        target: String,
        host: String,
        protocol: String,
        port: u16,
    },
    McpToolCall {
        id: String,
        server: String,
        tool_name: String,
        arguments: Option<Value>,
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
        turn_id: impl Into<String>,
        target: impl Into<String>,
        host: impl Into<String>,
        protocol: impl Into<String>,
        port: u16,
    ) -> Self {
        Self::NetworkAccess {
            id: id.into(),
            turn_id: turn_id.into(),
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
        arguments: Option<Value>,
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

    /// 转换为 JSON 表示
    pub fn to_json(&self) -> serde_json::Result<Value> {
        match self {
            Self::Shell {
                command,
                cwd,
                justification,
                ..
            } => Ok(serde_json::json!({
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
            } => Ok(serde_json::json!({
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
            } => Ok(serde_json::json!({
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
            } => Ok(serde_json::json!({
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
            } => Ok(serde_json::json!({
                "tool": "mcp_tool_call",
                "server": server,
                "tool_name": tool_name,
                "arguments": arguments,
            })),
        }
    }

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

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn test_request_id() {
        let request = ApprovalRequest::shell(
            "test-123",
            vec!["ls".to_string()],
            "/tmp",
            None,
        );
        assert_eq!(request.id(), "test-123");
    }

    #[test]
    fn test_shell_summary() {
        let request = ApprovalRequest::shell(
            "id",
            vec!["ls".to_string(), "-la".to_string()],
            "/",
            None,
        );
        assert_eq!(request.action_summary(), "shell: ls -la");
    }

    #[test]
    fn test_apply_patch_summary() {
        let request = ApprovalRequest::apply_patch(
            "id",
            "/project",
            vec!["file1.rs".to_string(), "file2.rs".to_string()],
            5,
            "patch content",
        );
        assert_eq!(request.action_summary(), "apply_patch: 2 files, 5 changes");
    }

    #[test]
    fn test_to_json() {
        let request = ApprovalRequest::shell(
            "id",
            vec!["echo".to_string(), "hello".to_string()],
            "/home",
            Some("test".to_string()),
        );
        let json = request.to_json().unwrap();

        assert_eq!(json["tool"], "shell");
        assert_eq!(json["command"], "echo hello");
        assert_eq!(json["cwd"], "/home");
        assert_eq!(json["justification"], "test");
    }

    #[test]
    fn test_exec_command_request_id() {
        let request = ApprovalRequest::exec_command(
            "exec-456",
            vec!["whoami".to_string()],
            "/",
            None,
            true,
        );
        assert_eq!(request.id(), "exec-456");
    }

    #[test]
    fn test_exec_command_summary() {
        let request = ApprovalRequest::exec_command(
            "id",
            vec!["pwd".to_string()],
            "/home",
            None,
            false,
        );
        assert_eq!(request.action_summary(), "exec_command: pwd");
    }

    #[test]
    fn test_exec_command_to_json() {
        let request = ApprovalRequest::exec_command(
            "id",
            vec!["ls".to_string()],
            "/tmp",
            Some("checking files".to_string()),
            true,
        );
        let json = request.to_json().unwrap();

        assert_eq!(json["tool"], "exec_command");
        assert_eq!(json["command"], "ls");
        assert_eq!(json["cwd"], "/tmp");
        assert_eq!(json["justification"], "checking files");
        assert_eq!(json["tty"], true);
    }

    #[test]
    fn test_apply_patch_to_json() {
        let request = ApprovalRequest::apply_patch(
            "patch-789",
            "/workspace",
            vec!["src/main.rs".to_string()],
            10,
            "diff content here",
        );
        let json = request.to_json().unwrap();

        assert_eq!(json["tool"], "apply_patch");
        assert_eq!(json["cwd"], "/workspace");
        assert_eq!(json["files"], json!(["src/main.rs"]));
        assert_eq!(json["change_count"], 10);
        assert_eq!(json["patch"], "diff content here");
    }

    #[test]
    fn test_network_access_request_id() {
        let request = ApprovalRequest::network_access(
            "net-001",
            "turn-1",
            "api.example.com",
            "api.example.com",
            "https",
            443,
        );
        assert_eq!(request.id(), "net-001");
    }

    #[test]
    fn test_network_access_summary() {
        let request = ApprovalRequest::network_access(
            "id",
            "turn-1",
            "api.example.com",
            "192.168.1.1",
            "https",
            443,
        );
        assert_eq!(
            request.action_summary(),
            "network_access: api.example.com (192.168.1.1)"
        );
    }

    #[test]
    fn test_network_access_to_json() {
        let request = ApprovalRequest::network_access(
            "id",
            "turn-1",
            "api.github.com",
            "140.82.112.0",
            "https",
            443,
        );
        let json = request.to_json().unwrap();

        assert_eq!(json["tool"], "network_access");
        assert_eq!(json["target"], "api.github.com");
        assert_eq!(json["host"], "140.82.112.0");
        assert_eq!(json["protocol"], "https");
        assert_eq!(json["port"], 443);
    }

    #[test]
    fn test_mcp_tool_call_request_id() {
        let request = ApprovalRequest::mcp_tool_call(
            "mcp-123",
            "filesystem",
            "readFile",
            None,
        );
        assert_eq!(request.id(), "mcp-123");
    }

    #[test]
    fn test_mcp_tool_call_summary() {
        let request = ApprovalRequest::mcp_tool_call(
            "id",
            "git_server",
            "getBranches",
            None,
        );
        assert_eq!(
            request.action_summary(),
            "mcp_tool_call: git_server::getBranches"
        );
    }

    #[test]
    fn test_mcp_tool_call_to_json() {
        let args = json!({"path": "/etc/config"});
        let request = ApprovalRequest::mcp_tool_call(
            "id",
            "filesystem",
            "readFile",
            Some(args.clone()),
        );
        let json = request.to_json().unwrap();

        assert_eq!(json["tool"], "mcp_tool_call");
        assert_eq!(json["server"], "filesystem");
        assert_eq!(json["tool_name"], "readFile");
        assert_eq!(json["arguments"], args);
    }

    #[test]
    fn test_mcp_tool_call_with_annotations() {
        let mut request =
            ApprovalRequest::mcp_tool_call("id", "db", "deleteRecords", None);
        // Access internal fields through the public interface
        if let ApprovalRequest::McpToolCall { annotations, .. } = &mut request {
            *annotations = Some(McpAnnotations {
                destructive_hint: Some(true),
                open_world_hint: Some(false),
                read_only_hint: Some(false),
            });
        }
        let json = request.to_json().unwrap();
        assert_eq!(json["tool"], "mcp_tool_call");
    }

    #[test]
    fn test_shell_request_id_via_pattern_matching() {
        let request = ApprovalRequest::shell(
            "shell-id",
            vec!["cmd".to_string()],
            "/",
            None,
        );
        let id = match &request {
            ApprovalRequest::Shell { id, .. } => id,
            _ => panic!("Expected Shell variant"),
        };
        assert_eq!(id, "shell-id");
    }

    #[test]
    fn test_exec_command_request_id_via_pattern_matching() {
        let request = ApprovalRequest::exec_command(
            "exec-id",
            vec!["cmd".to_string()],
            "/",
            None,
            false,
        );
        let id = match &request {
            ApprovalRequest::ExecCommand { id, .. } => id,
            _ => panic!("Expected ExecCommand variant"),
        };
        assert_eq!(id, "exec-id");
    }

    #[test]
    fn test_apply_patch_request_id_via_pattern_matching() {
        let request =
            ApprovalRequest::apply_patch("patch-id", "/", vec![], 0, "");
        let id = match &request {
            ApprovalRequest::ApplyPatch { id, .. } => id,
            _ => panic!("Expected ApplyPatch variant"),
        };
        assert_eq!(id, "patch-id");
    }

    #[test]
    fn test_network_access_request_id_via_pattern_matching() {
        let request = ApprovalRequest::network_access(
            "net-id", "turn", "t", "h", "p", 80,
        );
        let id = match &request {
            ApprovalRequest::NetworkAccess { id, .. } => id,
            _ => panic!("Expected NetworkAccess variant"),
        };
        assert_eq!(id, "net-id");
    }

    #[test]
    fn test_mcp_tool_call_request_id_via_pattern_matching() {
        let request =
            ApprovalRequest::mcp_tool_call("mcp-id", "srv", "tool", None);
        let id = match &request {
            ApprovalRequest::McpToolCall { id, .. } => id,
            _ => panic!("Expected McpToolCall variant"),
        };
        assert_eq!(id, "mcp-id");
    }

    #[test]
    fn test_id_method_consistency_across_variants() {
        let variants = [
            ApprovalRequest::shell("shell-1", vec![], "/", None),
            ApprovalRequest::exec_command("exec-1", vec![], "/", None, false),
            ApprovalRequest::apply_patch("patch-1", "/", vec![], 0, ""),
            ApprovalRequest::network_access("net-1", "t", "t", "h", "p", 80),
            ApprovalRequest::mcp_tool_call("mcp-1", "s", "t", None),
        ];

        assert_eq!(variants[0].id(), "shell-1");
        assert_eq!(variants[1].id(), "exec-1");
        assert_eq!(variants[2].id(), "patch-1");
        assert_eq!(variants[3].id(), "net-1");
        assert_eq!(variants[4].id(), "mcp-1");
    }

    #[test]
    fn test_action_summary_consistency() {
        // Verify each variant produces a non-empty summary
        let shell =
            ApprovalRequest::shell("id", vec!["echo".to_string()], "/", None);
        let exec = ApprovalRequest::exec_command(
            "id",
            vec!["echo".to_string()],
            "/",
            None,
            false,
        );
        let patch = ApprovalRequest::apply_patch(
            "id",
            "/",
            vec!["f.rs".to_string()],
            1,
            "",
        );
        let net = ApprovalRequest::network_access(
            "id", "t", "target", "host", "https", 443,
        );
        let mcp = ApprovalRequest::mcp_tool_call("id", "srv", "tool", None);

        assert!(!shell.action_summary().is_empty());
        assert!(!exec.action_summary().is_empty());
        assert!(!patch.action_summary().is_empty());
        assert!(!net.action_summary().is_empty());
        assert!(!mcp.action_summary().is_empty());
    }

    #[test]
    fn test_to_json_preserves_all_variants() {
        let shell = ApprovalRequest::shell(
            "id",
            vec!["ls".to_string()],
            "/",
            Some("just".to_string()),
        );
        let exec = ApprovalRequest::exec_command(
            "id",
            vec!["ls".to_string()],
            "/",
            Some("just".to_string()),
            true,
        );
        let patch = ApprovalRequest::apply_patch(
            "id",
            "/",
            vec!["a.rs".to_string()],
            2,
            "patch",
        );
        let net = ApprovalRequest::network_access(
            "id", "t", "target", "host", "tcp", 8080,
        );
        let mcp = ApprovalRequest::mcp_tool_call(
            "id",
            "srv",
            "tool",
            Some(json!({"k": "v"})),
        );

        assert!(shell.to_json().is_ok());
        assert!(exec.to_json().is_ok());
        assert!(patch.to_json().is_ok());
        assert!(net.to_json().is_ok());
        assert!(mcp.to_json().is_ok());
    }
}
