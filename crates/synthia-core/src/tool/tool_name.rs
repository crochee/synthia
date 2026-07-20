//! ToolName — namespaced tool identifier.
//!
//! Supports `namespace::tool_name` format to prevent name collisions
//! across tool sources (MCP servers, plugins, dynamic registration).
//!
//! # Examples
//!
//! ```
//! use synthia_core::tool::tool_name::ToolName;
//!
//! let plain = ToolName::plain("bash");
//! assert_eq!(plain.full_name(), "bash");
//! assert_eq!(plain.namespace(), None);
//!
//! let namespaced = ToolName::namespaced("mcp__github", "create_issue");
//! assert_eq!(namespaced.full_name(), "mcp__github::create_issue");
//! assert_eq!(namespaced.namespace(), Some("mcp__github"));
//!
//! // From string
//! let from_str: ToolName = "bash".into();
//! assert_eq!(from_str, plain);
//! ```

use std::{
    fmt,
    hash::{Hash, Hasher},
};

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Namespaced tool identifier.
///
/// Format: `namespace::name` (if namespaced) or `name` (if plain).
/// Used as the key in `ToolRegistry` and as the LLM function name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolName {
    /// Namespace prefix (e.g., `"mcp__github"`, `"plugin__myext"`).
    /// `None` for built-in / plain tools.
    namespace: Option<String>,
    /// Local tool name within the namespace (e.g., `"create_issue"`).
    name: String,
}

impl ToolName {
    /// Create a plain tool name (no namespace).
    pub fn plain(name: impl Into<String>) -> Self {
        Self {
            namespace: None,
            name: name.into(),
        }
    }

    /// Create a namespaced tool name.
    pub fn namespaced(
        namespace: impl Into<String>,
        name: impl Into<String>,
    ) -> Self {
        Self {
            namespace: Some(namespace.into()),
            name: name.into(),
        }
    }

    /// Create an MCP-namespaced tool name (`mcp__{server}::{tool}`).
    pub fn mcp(server: impl AsRef<str>, tool: impl Into<String>) -> Self {
        Self::namespaced(format!("mcp__{}", server.as_ref()), tool)
    }

    /// Create a plugin-namespaced tool name (`plugin__{plugin}::{tool}`).
    pub fn plugin(plugin_id: impl AsRef<str>, tool: impl Into<String>) -> Self {
        Self::namespaced(format!("plugin__{}", plugin_id.as_ref()), tool)
    }

    /// The namespace prefix, if any.
    pub fn namespace(&self) -> Option<&str> {
        self.namespace.as_deref()
    }

    /// The local tool name (without namespace).
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Fully qualified name: `namespace::name` or `name`.
    pub fn full_name(&self) -> String {
        match &self.namespace {
            Some(ns) => format!("{}::{}", ns, self.name),
            None => self.name.clone(),
        }
    }

    /// Whether this name has a namespace.
    pub fn is_namespaced(&self) -> bool {
        self.namespace.is_some()
    }

    /// Parse a `namespace::name` string into a ToolName.
    /// Returns `None` if the string is empty.
    ///
    /// ```
    /// use synthia_core::tool::tool_name::ToolName;
    /// let parsed = ToolName::parse("mcp__github::create_issue").unwrap();
    /// assert_eq!(parsed.namespace(), Some("mcp__github"));
    /// assert_eq!(parsed.name(), "create_issue");
    ///
    /// let plain = ToolName::parse("bash").unwrap();
    /// assert_eq!(plain.namespace(), None);
    /// assert_eq!(plain.name(), "bash");
    /// ```
    pub fn parse(full_name: &str) -> Option<Self> {
        if full_name.is_empty() {
            return None;
        }
        match full_name.split_once("::") {
            Some((ns, name)) if !ns.is_empty() && !name.is_empty() => {
                Some(Self::namespaced(ns, name))
            }
            _ => Some(Self::plain(full_name)),
        }
    }
}

// ── Display ──────────────────────────────────────────────────────────────

impl fmt::Display for ToolName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.namespace {
            Some(ns) => write!(f, "{}::{}", ns, self.name),
            None => write!(f, "{}", self.name),
        }
    }
}

// ── Hash ─────────────────────────────────────────────────────────────────

impl Hash for ToolName {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.namespace.hash(state);
        self.name.hash(state);
    }
}

// ── From<String> / From<&str> ───────────────────────────────────────────

impl From<String> for ToolName {
    fn from(name: String) -> Self {
        Self::parse(&name).unwrap_or_else(|| Self::plain(name))
    }
}

impl From<&str> for ToolName {
    fn from(name: &str) -> Self {
        Self::parse(name).unwrap_or_else(|| Self::plain(name))
    }
}

// ── Serialize / Deserialize ──────────────────────────────────────────────

impl Serialize for ToolName {
    fn serialize<S: Serializer>(
        &self,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        self.full_name().serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for ToolName {
    fn deserialize<D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        Ok(Self::parse(&s).unwrap_or_else(|| Self::plain(&s)))
    }
}

// ── Ord (for consistent ordering in maps / BTreeMap) ─────────────────────

impl Ord for ToolName {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.full_name().cmp(&other.full_name())
    }
}

impl PartialOrd for ToolName {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

// ── Tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_name() {
        let name = ToolName::plain("bash");
        assert_eq!(name.name(), "bash");
        assert_eq!(name.namespace(), None);
        assert_eq!(name.full_name(), "bash");
        assert!(!name.is_namespaced());
    }

    #[test]
    fn namespaced_name() {
        let name = ToolName::namespaced("mcp__github", "create_issue");
        assert_eq!(name.name(), "create_issue");
        assert_eq!(name.namespace(), Some("mcp__github"));
        assert_eq!(name.full_name(), "mcp__github::create_issue");
        assert!(name.is_namespaced());
    }

    #[test]
    fn mcp_factory() {
        let name = ToolName::mcp("github", "create_issue");
        assert_eq!(name.full_name(), "mcp__github::create_issue");
    }

    #[test]
    fn plugin_factory() {
        let name = ToolName::plugin("myext", "custom_tool");
        assert_eq!(name.full_name(), "plugin__myext::custom_tool");
    }

    #[test]
    fn from_string() {
        let name: ToolName = "bash".into();
        assert_eq!(name, ToolName::plain("bash"));
    }

    #[test]
    fn from_namespaced_string() {
        let name: ToolName = "mcp__github::create_issue".into();
        assert_eq!(name, ToolName::namespaced("mcp__github", "create_issue"));
    }

    #[test]
    fn parse_empty() {
        assert!(ToolName::parse("").is_none());
    }

    #[test]
    fn parse_plain() {
        let name = ToolName::parse("bash").unwrap();
        assert_eq!(name, ToolName::plain("bash"));
    }

    #[test]
    fn parse_namespaced() {
        let name = ToolName::parse("mcp__github::create_issue").unwrap();
        assert_eq!(name.namespace(), Some("mcp__github"));
        assert_eq!(name.name(), "create_issue");
    }

    #[test]
    fn display_trait() {
        let plain = ToolName::plain("bash");
        assert_eq!(format!("{}", plain), "bash");

        let namespaced = ToolName::namespaced("mcp__github", "create_issue");
        assert_eq!(format!("{}", namespaced), "mcp__github::create_issue");
    }

    #[test]
    fn hash_equality() {
        use std::collections::HashSet;
        let mut set = HashSet::new();
        set.insert(ToolName::plain("bash"));
        set.insert(ToolName::namespaced("mcp__github", "bash"));
        assert_eq!(set.len(), 2); // different namespaces → different entries
    }

    #[test]
    fn serde_roundtrip_plain() {
        let name = ToolName::plain("bash");
        let json = serde_json::to_string(&name).unwrap();
        assert_eq!(json, "\"bash\"");
        let deserialized: ToolName = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, name);
    }

    #[test]
    fn serde_roundtrip_namespaced() {
        let name = ToolName::namespaced("mcp__github", "create_issue");
        let json = serde_json::to_string(&name).unwrap();
        assert_eq!(json, "\"mcp__github::create_issue\"");
        let deserialized: ToolName = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, name);
    }

    #[test]
    fn ordering() {
        let a = ToolName::plain("alpha");
        let b = ToolName::plain("beta");
        let c = ToolName::namespaced("mcp", "alpha");
        assert!(a < b);
        assert!(a < c); // "alpha" < "mcp::alpha"
    }
}
