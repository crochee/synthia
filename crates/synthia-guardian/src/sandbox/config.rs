use std::path::PathBuf;

/// Sandbox configuration
#[derive(Clone, Debug)]
pub struct SandboxConfig {
    /// Whether sandbox enforcement is enabled
    pub enabled: bool,
    /// Allowed base paths (operations must be within these paths)
    pub allowed_paths: Vec<PathBuf>,
    /// Blocked command patterns
    pub blocked_commands: Vec<String>,
    /// Maximum execution time in milliseconds
    pub max_execution_time_ms: u64,
    /// Maximum output size in bytes
    pub max_output_bytes: usize,
    /// Maximum memory usage in megabytes
    pub max_memory_mb: usize,
}

impl Default for SandboxConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            allowed_paths: vec![],
            blocked_commands: vec![
                "rm -rf /".to_string(),
                "rm -rf /*".to_string(),
                "mkfs".to_string(),
                "dd".to_string(),
                ":(){:|:&};:".to_string(), // Fork bomb
                "chmod 777 /".to_string(),
                "chmod -R 777 /".to_string(),
                "sudo rm -rf".to_string(),
                "mv / /dev/null".to_string(),
                "> /dev/sda".to_string(),
            ],
            max_execution_time_ms: 300_000, // 5 minutes
            max_output_bytes: 10 * 1024 * 1024, // 10MB
            max_memory_mb: 512,
        }
    }
}

impl SandboxConfig {
    /// Creates a new sandbox configuration with the given allowed paths
    #[must_use]
    pub fn with_allowed_paths(paths: Vec<PathBuf>) -> Self {
        Self {
            enabled: true,
            allowed_paths: paths,
            ..Self::default()
        }
    }

    /// Creates a new sandbox configuration with custom blocked commands
    #[must_use]
    pub fn with_blocked_commands(commands: Vec<String>) -> Self {
        Self {
            blocked_commands: commands,
            ..Self::default()
        }
    }
}
