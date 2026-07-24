//! Plugin manifest parsing and validation

use std::{path::Path, sync::OnceLock};

use regex::Regex;
use semver::Version;
use serde::Deserialize;

/// Validates that a name is in kebab-case (lowercase letters, numbers, and hyphens)
#[allow(clippy::expect_used)]
fn is_valid_kebab_case(name: &str) -> bool {
    static KEBAB_REGEX: OnceLock<Regex> = OnceLock::new();
    let regex = KEBAB_REGEX.get_or_init(|| {
        Regex::new(r"^[a-z][a-z0-9]*(?:-[a-z0-9]+)*$")
            .expect("regex pattern is valid")
    });
    regex.is_match(name)
}

/// Validates that a version string is valid semver
fn is_valid_semver(version: &str) -> bool {
    Version::parse(version).is_ok()
}

/// Plugin manifest structure
///
/// Defines the metadata for a Synthia plugin. Each plugin must have a
/// `plugin.json` manifest file in its `.synthia-plugin/` directory.
///
/// # Example manifest
/// ```json
/// {
///     "name": "example-plugin",
///     "version": "1.0.0",
///     "description": "An example plugin",
///     "author": "Plugin Author",
///     "hooks": {
///         "pre-task": "./hooks/pre-task.js"
///     },
///     "mcpServers": {
///         "example": {
///             "command": "npx",
///             "args": ["-y", "@example/mcp-server"]
///         }
///     }
/// }
/// ```
#[derive(Debug, Clone, Deserialize)]
pub struct PluginManifest {
    /// Plugin name in kebab-case
    pub name: String,

    /// Semantic version string
    pub version: String,

    /// Human-readable description
    pub description: String,

    /// Plugin author
    pub author: String,

    /// Optional hook definitions
    #[serde(default)]
    pub hooks: Option<serde_json::Value>,

    /// Optional MCP server configurations
    #[serde(rename = "mcpServers", default)]
    pub mcp_servers: Option<serde_json::Value>,
}

/// Errors that can occur during plugin operations
#[derive(Debug, thiserror::Error)]
pub enum PluginError {
    #[error("Failed to read plugin manifest: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Failed to parse plugin manifest: {0}")]
    ParseError(#[from] serde_json::Error),

    #[error(
        "Invalid plugin name '{0}': must be kebab-case (lowercase, numbers, hyphens)"
    )]
    InvalidName(String),

    #[error("Invalid plugin version '{0}': must be valid semver (e.g., 1.0.0)")]
    InvalidVersion(String),

    #[error("Plugin manifest not found at path")]
    ManifestNotFound,

    #[error("Cannot find HOME environment variable")]
    HomeDirectoryNotFound,

    #[error("Duplicate plugin name: '{0}'")]
    DuplicatePlugin(String),

    #[error("Plugin not loaded: {0}")]
    PluginNotLoaded(uuid::Uuid),

    #[error("Invalid hooks.json config for hook '{0}'")]
    InvalidHooksConfig(String),

    #[error("Invalid mcp.json config for server '{0}'")]
    InvalidMcpConfig(String),
}

impl PluginManifest {
    /// Parse a plugin manifest from a JSON file
    pub fn from_path(path: &Path) -> Result<Self, PluginError> {
        let content = std::fs::read_to_string(path)?;
        let manifest: PluginManifest = serde_json::from_str(&content)?;
        manifest.validate()?;
        Ok(manifest)
    }

    /// Validate the plugin manifest fields
    pub fn validate(&self) -> Result<(), PluginError> {
        // Validate name is kebab-case
        if !is_valid_kebab_case(&self.name) {
            return Err(PluginError::InvalidName(self.name.clone()));
        }

        // Validate version is semver
        if !is_valid_semver(&self.version) {
            return Err(PluginError::InvalidVersion(self.version.clone()));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_valid_manifest() {
        let json = r#"{
            "name": "example-plugin",
            "version": "1.0.0",
            "description": "An example plugin",
            "author": "Test Author"
        }"#;

        let manifest: PluginManifest = serde_json::from_str(json).unwrap();
        assert_eq!(manifest.name, "example-plugin");
        assert_eq!(manifest.version, "1.0.0");
        assert_eq!(manifest.description, "An example plugin");
        assert_eq!(manifest.author, "Test Author");
        assert!(manifest.hooks.is_none());
        assert!(manifest.mcp_servers.is_none());
        manifest.validate().unwrap();
    }

    #[test]
    fn test_parse_manifest_with_optional_fields() {
        let json = r#"{
            "name": "my-plugin-v2",
            "version": "2.1.0-beta.1",
            "description": "A plugin with hooks",
            "author": "Author Name",
            "hooks": {"pre-task": "./hooks/pre.js"},
            "mcpServers": {"server": {"command": "node", "args": ["server.js"]}}
        }"#;

        let manifest: PluginManifest = serde_json::from_str(json).unwrap();
        assert!(manifest.hooks.is_some());
        assert!(manifest.mcp_servers.is_some());
        manifest.validate().unwrap();
    }

    #[test]
    fn test_invalid_name_uppercase() {
        let json = r#"{
            "name": "ExamplePlugin",
            "version": "1.0.0",
            "description": "Test",
            "author": "Author"
        }"#;

        let manifest: PluginManifest = serde_json::from_str(json).unwrap();
        let result = manifest.validate();
        assert!(matches!(result, Err(PluginError::InvalidName(_))));
    }

    #[test]
    fn test_invalid_name_underscores() {
        let json = r#"{
            "name": "example_plugin",
            "version": "1.0.0",
            "description": "Test",
            "author": "Author"
        }"#;

        let manifest: PluginManifest = serde_json::from_str(json).unwrap();
        let result = manifest.validate();
        assert!(matches!(result, Err(PluginError::InvalidName(_))));
    }

    #[test]
    fn test_invalid_name_starts_with_number() {
        let json = r#"{
            "name": "123plugin",
            "version": "1.0.0",
            "description": "Test",
            "author": "Author"
        }"#;

        let manifest: PluginManifest = serde_json::from_str(json).unwrap();
        let result = manifest.validate();
        assert!(matches!(result, Err(PluginError::InvalidName(_))));
    }

    #[test]
    fn test_invalid_name_empty() {
        let json = r#"{
            "name": "",
            "version": "1.0.0",
            "description": "Test",
            "author": "Author"
        }"#;

        let manifest: PluginManifest = serde_json::from_str(json).unwrap();
        let result = manifest.validate();
        assert!(matches!(result, Err(PluginError::InvalidName(_))));
    }

    #[test]
    fn test_invalid_version_format() {
        let json = r#"{
            "name": "valid-name",
            "version": "1.0",
            "description": "Test",
            "author": "Author"
        }"#;

        let manifest: PluginManifest = serde_json::from_str(json).unwrap();
        let result = manifest.validate();
        assert!(matches!(result, Err(PluginError::InvalidVersion(_))));
    }

    #[test]
    fn test_invalid_version_letters() {
        let json = r#"{
            "name": "valid-name",
            "version": "1.0.0abc",
            "description": "Test",
            "author": "Author"
        }"#;

        let manifest: PluginManifest = serde_json::from_str(json).unwrap();
        let result = manifest.validate();
        assert!(matches!(result, Err(PluginError::InvalidVersion(_))));
    }

    #[test]
    fn test_invalid_version_empty() {
        let json = r#"{
            "name": "valid-name",
            "version": "",
            "description": "Test",
            "author": "Author"
        }"#;

        let manifest: PluginManifest = serde_json::from_str(json).unwrap();
        let result = manifest.validate();
        assert!(matches!(result, Err(PluginError::InvalidVersion(_))));
    }

    #[test]
    fn test_valid_complex_names() {
        let valid_names =
            vec!["a", "ab", "a-b", "a1-b2-c3", "my-awesome-plugin"];

        for name in valid_names {
            let json = format!(
                r#"{{
                "name": "{}",
                "version": "1.0.0",
                "description": "Test",
                "author": "Author"
            }}"#,
                name
            );

            let manifest: PluginManifest = serde_json::from_str(&json).unwrap();
            manifest.validate().unwrap();
        }
    }

    #[test]
    fn test_valid_version_formats() {
        let valid_versions =
            vec!["0.0.0", "1.0.0", "1.2.3", "0.1.0", "10.20.30"];

        for version in valid_versions {
            let json = format!(
                r#"{{
                "name": "valid-name",
                "version": "{}",
                "description": "Test",
                "author": "Author"
            }}"#,
                version
            );

            let manifest: PluginManifest = serde_json::from_str(&json).unwrap();
            manifest.validate().unwrap();
        }
    }

    #[test]
    fn test_semver_prerelease_versions() {
        let prerelease_versions = vec![
            "1.0.0-alpha",
            "1.0.0-alpha.1",
            "1.0.0-beta.2",
            "1.0.0-rc.1",
            "2.1.0-beta.1",
        ];

        for version in prerelease_versions {
            let json = format!(
                r#"{{
                "name": "valid-name",
                "version": "{}",
                "description": "Test",
                "author": "Author"
            }}"#,
                version
            );

            let manifest: PluginManifest = serde_json::from_str(&json).unwrap();
            manifest.validate().unwrap();
        }
    }
}
