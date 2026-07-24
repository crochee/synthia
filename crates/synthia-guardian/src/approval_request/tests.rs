//! Unit tests for [`ApprovalRequest`].
//!
//! Covers:
//! - `id()` getter (8 tests: one per variant + 1 cross-variant consistency)
//! - constructor methods (5 tests, one per variant + cross-variant id check)
//! - `to_json()` serialization (5 tests, one per variant + 1 cross-variant
//!   check that all variants serialize successfully)
//! - `action_summary()` (3 tests: per-variant + cross-variant non-empty check)

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
    let request =
        ApprovalRequest::mcp_tool_call("id", "git_server", "getBranches", None);
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
    let request =
        ApprovalRequest::shell("shell-id", vec!["cmd".to_string()], "/", None);
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
    let request = ApprovalRequest::apply_patch("patch-id", "/", vec![], 0, "");
    let id = match &request {
        ApprovalRequest::ApplyPatch { id, .. } => id,
        _ => panic!("Expected ApplyPatch variant"),
    };
    assert_eq!(id, "patch-id");
}

#[test]
fn test_network_access_request_id_via_pattern_matching() {
    let request = ApprovalRequest::network_access("net-id", "t", "h", "p", 80);
    let id = match &request {
        ApprovalRequest::NetworkAccess { id, .. } => id,
        _ => panic!("Expected NetworkAccess variant"),
    };
    assert_eq!(id, "net-id");
}

#[test]
fn test_mcp_tool_call_request_id_via_pattern_matching() {
    let request = ApprovalRequest::mcp_tool_call("mcp-id", "srv", "tool", None);
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
        ApprovalRequest::network_access("net-1", "t", "h", "p", 80),
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
    let net =
        ApprovalRequest::network_access("id", "target", "host", "https", 443);
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
    let net =
        ApprovalRequest::network_access("id", "target", "host", "tcp", 8080);
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
