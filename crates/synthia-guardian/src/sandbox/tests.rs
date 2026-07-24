use std::path::{Path, PathBuf};

use super::*;
use crate::SecuritySeverity;

#[test]
fn test_command_allowed_when_disabled() {
    let executor = SandboxExecutor::new(SandboxConfig::default());
    assert!(executor.is_command_allowed("rm -rf /"));
}

#[test]
fn test_blocked_command_when_enabled() {
    let config = SandboxConfig {
        enabled: true,
        ..SandboxConfig::default()
    };
    let executor = SandboxExecutor::new(config);

    assert!(!executor.is_command_allowed("rm -rf /"));
    assert!(!executor.is_command_allowed("mkfs.ext4 /dev/sda1"));
    assert!(!executor.is_command_allowed(":(){:|:&};:"));
}

#[test]
fn test_path_validation() {
    let config = SandboxConfig {
        enabled: true,
        allowed_paths: vec![PathBuf::from("/workspace")],
        ..SandboxConfig::default()
    };
    let executor = SandboxExecutor::new(config);

    assert!(executor.is_path_allowed(Path::new("/workspace/file.txt")));
    assert!(executor.is_path_allowed(Path::new("/workspace/subdir/file.txt")));
    assert!(!executor.is_path_allowed(Path::new("/etc/passwd")));
    assert!(!executor.is_path_allowed(Path::new("/root/.ssh/id_rsa")));
}

#[test]
fn test_relative_path_denied() {
    let config = SandboxConfig {
        enabled: true,
        allowed_paths: vec![PathBuf::from("/workspace")],
        ..SandboxConfig::default()
    };
    let executor = SandboxExecutor::new(config);

    let result = executor.check_path(Path::new("file.txt"));
    assert!(!result.allowed);
    assert!(result.reason.is_some());
}

#[test]
fn test_output_size_limit() {
    let config = SandboxConfig {
        enabled: true,
        max_output_bytes: 1024,
        ..SandboxConfig::default()
    };
    let executor = SandboxExecutor::new(config);

    assert!(executor.is_output_size_allowed(512));
    assert!(!executor.is_output_size_allowed(2048));
}

#[test]
fn test_execution_time_limit() {
    let config = SandboxConfig {
        enabled: true,
        max_execution_time_ms: 5000,
        ..SandboxConfig::default()
    };
    let executor = SandboxExecutor::new(config);

    assert!(executor.is_execution_time_allowed(3000));
    assert!(!executor.is_execution_time_allowed(10000));
}

#[test]
fn test_check_command_detailed_result() {
    let config = SandboxConfig {
        enabled: true,
        ..SandboxConfig::default()
    };
    let executor = SandboxExecutor::new(config);

    let allowed_result = executor.check_command("ls -la");
    assert!(allowed_result.allowed);

    let denied_result = executor.check_command("rm -rf /");
    assert!(!denied_result.allowed);
    assert!(denied_result.reason.is_some());
    assert_eq!(denied_result.severity, Some(SecuritySeverity::Critical));
}

#[test]
fn test_config_with_allowed_paths() {
    let config = SandboxConfig::with_allowed_paths(vec![
        PathBuf::from("/project"),
        PathBuf::from("/tmp"),
    ]);
    assert!(config.enabled);
    assert_eq!(config.allowed_paths.len(), 2);
}

#[test]
fn test_default_blocked_commands() {
    let config = SandboxConfig::default();
    // Verify dangerous commands are blocked by default
    assert!(
        config
            .blocked_commands
            .iter()
            .any(|c| c.contains("rm -rf /"))
    );
    assert!(config.blocked_commands.iter().any(|c| c.contains("mkfs")));
}
