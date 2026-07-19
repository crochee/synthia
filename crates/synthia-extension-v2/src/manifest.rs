//! `ExtensionManifest` — declarative registration for extensions.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

/// Capabilities an extension may declare.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Capability {
    /// Read files.
    FileRead,
    /// Write files.
    FileWrite,
    /// Execute shell commands.
    Shell,
    /// Make network requests.
    Network,
    /// Access the event bus.
    EventBus,
    /// Register hooks.
    HookRegistration,
    /// Spawn subagents.
    SubagentSpawn,
    /// Access MCP servers.
    McpAccess,
    /// Access OAuth flows.
    OAuthFlow,
    /// Read session state.
    SessionRead,
    /// Modify session state.
    SessionWrite,
    /// Access tool registry.
    ToolRegistry,
    /// Access service registry.
    ServiceRegistry,
    /// Perform definition drift checks.
    DefinitionDrift,
    /// Access steering input.
    Steering,
    /// Access context compaction.
    Compaction,
    /// Access agent memory.
    Memory,
    /// Access telemetry / metrics.
    Telemetry,
    /// Custom capability (stringly-typed escape hatch).
    Custom,
}

impl Capability {
    /// All known capability names (for manifest validation).
    const KNOWN_NAMES: &'static [&'static str] = &[
        "file_read",
        "file_write",
        "shell",
        "network",
        "event_bus",
        "hook_registration",
        "subagent_spawn",
        "mcp_access",
        "oauth_flow",
        "session_read",
        "session_write",
        "tool_registry",
        "service_registry",
        "definition_drift",
        "steering",
        "compaction",
        "memory",
        "telemetry",
        "custom",
    ];

    /// Parse a capability from its `snake_case` name.
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "file_read" => Some(Self::FileRead),
            "file_write" => Some(Self::FileWrite),
            "shell" => Some(Self::Shell),
            "network" => Some(Self::Network),
            "event_bus" => Some(Self::EventBus),
            "hook_registration" => Some(Self::HookRegistration),
            "subagent_spawn" => Some(Self::SubagentSpawn),
            "mcp_access" => Some(Self::McpAccess),
            "oauth_flow" => Some(Self::OAuthFlow),
            "session_read" => Some(Self::SessionRead),
            "session_write" => Some(Self::SessionWrite),
            "tool_registry" => Some(Self::ToolRegistry),
            "service_registry" => Some(Self::ServiceRegistry),
            "definition_drift" => Some(Self::DefinitionDrift),
            "steering" => Some(Self::Steering),
            "compaction" => Some(Self::Compaction),
            "memory" => Some(Self::Memory),
            "telemetry" => Some(Self::Telemetry),
            "custom" => Some(Self::Custom),
            _ => None,
        }
    }

    /// Returns the `snake_case` name of this capability.
    #[must_use]
    pub fn name(self) -> &'static str {
        Self::KNOWN_NAMES[self as usize]
    }
}

/// Errors during manifest creation or validation.
#[derive(Debug, thiserror::Error)]
pub enum ExtensionManifestError {
    /// The extension name must not be empty.
    #[error("extension name must not be empty")]
    EmptyName,
    /// At least one capability is required.
    #[error("extension manifest must declare at least one capability")]
    NoCapabilities,
    /// Unknown capability name in manifest.
    #[error("unknown capability: {0}")]
    UnknownCapability(String),
}

/// Declarative manifest for an extension.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtensionManifest {
    /// Extension name (non-empty).
    pub name: String,
    /// Extension version.
    pub version: String,
    /// Description.
    pub description: String,
    /// Declared capabilities.
    pub capabilities: HashSet<Capability>,
}

impl ExtensionManifest {
    /// Create a new manifest builder.
    pub fn builder() -> ExtensionManifestBuilder {
        ExtensionManifestBuilder::default()
    }

    /// Validate that all capabilities are known.
    pub fn validate_capabilities(
        names: &[String],
    ) -> Result<HashSet<Capability>, ExtensionManifestError> {
        let mut caps = HashSet::new();
        for name in names {
            let cap = Capability::from_name(name).ok_or_else(|| {
                ExtensionManifestError::UnknownCapability(name.clone())
            })?;
            caps.insert(cap);
        }
        Ok(caps)
    }
}

/// Builder for `ExtensionManifest`.
#[derive(Debug, Default)]
pub struct ExtensionManifestBuilder {
    name: Option<String>,
    version: Option<String>,
    description: Option<String>,
    capabilities: HashSet<Capability>,
}

impl ExtensionManifestBuilder {
    #[must_use]
    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    #[must_use]
    pub fn version(mut self, version: impl Into<String>) -> Self {
        self.version = Some(version.into());
        self
    }

    #[must_use]
    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    #[must_use]
    pub fn capability(mut self, cap: Capability) -> Self {
        self.capabilities.insert(cap);
        self
    }

    /// Build the manifest, validating required fields.
    pub fn build(self) -> Result<ExtensionManifest, ExtensionManifestError> {
        let name = self.name.unwrap_or_default();
        if name.is_empty() {
            return Err(ExtensionManifestError::EmptyName);
        }
        if self.capabilities.is_empty() {
            return Err(ExtensionManifestError::NoCapabilities);
        }
        Ok(ExtensionManifest {
            name,
            version: self.version.unwrap_or_else(|| "0.1.0".into()),
            description: self.description.unwrap_or_default(),
            capabilities: self.capabilities,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builder_rejects_empty_name() {
        let err = ExtensionManifest::builder()
            .capability(Capability::FileRead)
            .build()
            .unwrap_err();
        assert!(matches!(err, ExtensionManifestError::EmptyName));
    }

    #[test]
    fn builder_rejects_no_capabilities() {
        let err = ExtensionManifest::builder()
            .name("test")
            .build()
            .unwrap_err();
        assert!(matches!(err, ExtensionManifestError::NoCapabilities));
    }

    #[test]
    fn builder_succeeds_with_name_and_capability() {
        let m = ExtensionManifest::builder()
            .name("my-ext")
            .capability(Capability::Network)
            .build()
            .unwrap();
        assert_eq!(m.name, "my-ext");
        assert!(m.capabilities.contains(&Capability::Network));
    }

    #[test]
    fn validate_capabilities_rejects_unknown() {
        let names = vec!["file_read".into(), "unknown_cap".into()];
        let err = ExtensionManifest::validate_capabilities(&names).unwrap_err();
        assert!(matches!(err, ExtensionManifestError::UnknownCapability(_)));
    }

    #[test]
    fn validate_capabilities_accepts_known() {
        let names = vec!["file_read".into(), "network".into()];
        let caps = ExtensionManifest::validate_capabilities(&names).unwrap();
        assert_eq!(caps.len(), 2);
    }
}
