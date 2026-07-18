//! Plugin manifest — single source of truth for plugin identity.
//!
//! This is a skeleton for Phase 4 (Plugin unification).
//! Full implementation deferred to a follow-up change.

use serde::{Deserialize, Serialize};

/// Kebab-case plugin identifier.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PluginId(pub String);

impl std::fmt::Display for PluginId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Plugin manifest. Single source of truth for plugin identity.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginManifest {
    pub id: PluginId,
    pub version: String,
    pub description: String,
    pub author: String,
}

/// Capabilities the plugin declares.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PluginCapabilities {
    pub tools: Vec<String>,
    pub services: Vec<String>,
    pub hooks: Vec<String>,
}
