use std::path::Path;

use super::{config::SandboxConfig, result::SandboxCheckResult};
use crate::types::SecuritySeverity;

/// Sandbox executor: enforces sandbox constraints on command execution
pub struct SandboxExecutor {
    config: SandboxConfig,
}

impl SandboxExecutor {
    /// Creates a new sandbox executor with the given configuration
    #[must_use]
    pub fn new(config: SandboxConfig) -> Self {
        Self { config }
    }

    /// Checks if a command is allowed in the sandbox
    #[must_use]
    pub fn is_command_allowed(&self, command: &str) -> bool {
        self.check_command(command).allowed
    }

    /// Checks a command and returns detailed result
    #[must_use]
    pub fn check_command(&self, command: &str) -> SandboxCheckResult {
        if !self.config.enabled {
            return SandboxCheckResult::allowed();
        }

        // Check blocked commands
        for blocked in &self.config.blocked_commands {
            if command.contains(blocked) {
                return SandboxCheckResult::denied(
                    format!("Command contains blocked pattern: '{blocked}'"),
                    SecuritySeverity::Critical,
                );
            }
        }

        SandboxCheckResult::allowed()
    }

    /// Checks if a path is within allowed paths
    #[must_use]
    pub fn is_path_allowed(&self, path: &Path) -> bool {
        self.check_path(path).allowed
    }

    /// Checks a path and returns detailed result
    #[must_use]
    pub fn check_path(&self, path: &Path) -> SandboxCheckResult {
        if !self.config.enabled || self.config.allowed_paths.is_empty() {
            return SandboxCheckResult::allowed();
        }

        // Resolve the path to absolute form for comparison
        let path_to_check = if path.is_absolute() {
            path.to_path_buf()
        } else {
            // If relative, we can't validate - deny for safety
            return SandboxCheckResult::denied(
                format!("Relative paths not allowed in sandbox: {path:?}"),
                SecuritySeverity::High,
            );
        };

        // Check if path starts with any allowed path
        let is_allowed = self
            .config
            .allowed_paths
            .iter()
            .any(|allowed| path_to_check.starts_with(allowed));

        if !is_allowed {
            return SandboxCheckResult::denied(
                format!("Path {path_to_check:?} is not within allowed paths"),
                SecuritySeverity::High,
            );
        }

        SandboxCheckResult::allowed()
    }

    /// Checks if the output size is within limits
    #[must_use]
    pub fn is_output_size_allowed(&self, size: usize) -> bool {
        if !self.config.enabled {
            return true;
        }
        size <= self.config.max_output_bytes
    }

    /// Checks if the execution time is within limits
    #[must_use]
    pub fn is_execution_time_allowed(&self, time_ms: u64) -> bool {
        if !self.config.enabled {
            return true;
        }
        time_ms <= self.config.max_execution_time_ms
    }

    /// Returns the maximum allowed output size in bytes
    #[must_use]
    pub fn max_output_bytes(&self) -> usize {
        self.config.max_output_bytes
    }

    /// Returns the maximum execution time in milliseconds
    #[must_use]
    pub fn max_execution_time_ms(&self) -> u64 {
        self.config.max_execution_time_ms
    }

    /// Returns a reference to the sandbox configuration
    #[must_use]
    pub fn config(&self) -> &SandboxConfig {
        &self.config
    }
}
