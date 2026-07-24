use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use synthia_permission::{Permission, rule::PermissionRule};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AgentDefinition {
    pub id: String,
    pub name: String,
    pub description: String,
    pub capabilities: Vec<String>,
    pub when_to_use: Vec<String>,
    pub constraints: Vec<String>,
    pub system_prompt: String,
    pub source_path: PathBuf,
    pub file_hash: String,
    pub loaded_at: DateTime<Utc>,
    pub enabled: bool,
    /// Permission rules loaded from the agent file frontmatter.
    #[serde(default)]
    pub permission_rules: Vec<PermissionRule>,
    /// Default permission action when no rule matches.
    #[serde(default)]
    pub permission_default: Option<Permission>,
    /// Explicitly allowed tool names (allowlist).
    #[serde(default)]
    pub tools: Option<Vec<String>>,
    /// Explicitly denied tool names (deny list).
    #[serde(default)]
    pub denied_tools: Option<Vec<String>>,
    /// ID of another agent this one extends.
    #[serde(default)]
    pub extends: Option<String>,
    /// Agent mode (e.g. "architect", "executor").
    #[serde(default)]
    pub mode: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct AgentFilter {
    pub name: Option<String>,
    pub capability: Option<String>,
    pub enabled_only: bool,
}
