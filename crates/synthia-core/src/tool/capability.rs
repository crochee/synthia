//! ToolCapabilities + CapabilityBroker (security B5).

/// Per-tool capability allowlist. Default is all-false (pure function tools).
#[derive(Debug, Clone, Default)]
pub struct ToolCapabilities {
    pub memory_read: bool,
    pub memory_write: bool,
    pub session_fork: bool,
    pub permission_record: bool,
    pub hook_emit: bool,
    pub telemetry_record: bool,
    pub skill_invoke: bool,
    pub command_invoke: bool,
}

/// Thin wrapper that checks capabilities before granting service access.
#[derive(Debug, Clone)]
pub struct CapabilityBroker {
    capabilities: ToolCapabilities,
}

impl CapabilityBroker {
    pub fn new(capabilities: ToolCapabilities) -> Self {
        Self { capabilities }
    }

    /// Check if a specific capability is allowed.
    pub fn allowed(&self, capability: &str) -> bool {
        match capability {
            "memory_read" => self.capabilities.memory_read,
            "memory_write" => self.capabilities.memory_write,
            "session_fork" => self.capabilities.session_fork,
            "permission_record" => self.capabilities.permission_record,
            "hook_emit" => self.capabilities.hook_emit,
            "telemetry_record" => self.capabilities.telemetry_record,
            "skill_invoke" => self.capabilities.skill_invoke,
            "command_invoke" => self.capabilities.command_invoke,
            _ => false,
        }
    }

    pub fn capabilities(&self) -> &ToolCapabilities {
        &self.capabilities
    }
}

impl From<ToolCapabilities> for CapabilityBroker {
    fn from(capabilities: ToolCapabilities) -> Self {
        Self::new(capabilities)
    }
}
