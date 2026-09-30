//! [`ToolRestriction`] — per-scope allow / deny filter for tool
//! visibility.
//!
//! R29 (dsh `ToolRestriction` parity, see
//! `docs/subsystems/tools.md:240-265`).
//!
//! ## Why a separate value type
//!
//! `ToolRegistry::register_entry` is the only knob for "what
//! is visible to the LLM". Per-scope delegation needs a
//! different shape: the parent decides which tools a child
//! may answer through, and the child may not see tools the
//! parent already decided to hide. The dsh shape — a single
//! `ToolRestriction { allow?, deny? }` value that gets
//! intersected across ancestor scopes — is the right
//! primitive:
//!
//! - `allow = None`, `deny = []` → no restriction (default).
//! - `allow = Some(["read", "write"])` → only the listed
//!   inherited tools are visible; unlisted inherited tools
//!   are excluded.
//! - `deny = ["shell"]` → the listed inherited tools are
//!   hidden; unlisted inherited tools are still visible.
//! - `allow = Some([…])` and `deny = [..]` → both filters
//!   apply (allow gates, deny trims).
//!
//! The child's own registrations stay visible regardless of
//! the restriction (the child answers through its own tool
//! set, not the parent's).

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::registry::ToolRegistry;

/// Per-scope tool-visibility filter.
///
/// Wire format: a JSON object with optional `allow` (array of
/// tool names) and `deny` (array of tool names). The default
/// `{}` means "no restriction".
///
/// ## Composition
///
/// Two restrictions compose by **intersection**: the visible set
/// is the intersection of both. This is the dsh
/// "intersect-and-shadow" rule — every ancestor scope
/// contributes a filter, and a tool is visible only if every
/// ancestor permits it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolRestriction {
    /// Allow-list. When `Some`, only the listed inherited tools
    /// are visible. When `None`, the parent's allow-list
    /// (or the absence of one) passes through.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allow: Option<Vec<String>>,
    /// Deny-list. The listed inherited tools are always
    /// hidden, regardless of the allow-list.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub deny: Vec<String>,
}

impl ToolRestriction {
    /// No restriction — every tool passes through.
    pub fn none() -> Self {
        Self::default()
    }

    /// Build a deny-list-only restriction.
    pub fn deny(tools: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self {
            allow: None,
            deny: tools.into_iter().map(Into::into).collect(),
        }
    }

    /// Build an allow-list-only restriction.
    pub fn allow(tools: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self {
            allow: Some(tools.into_iter().map(Into::into).collect()),
            deny: Vec::new(),
        }
    }

    /// Returns `true` iff `name` survives this restriction
    /// when considered as an *inherited* tool. The child's own
    /// registrations are exempt — call [`Self::is_relevant`]
    /// with `is_own = true` to skip the filter.
    pub fn is_relevant(&self, name: &str, is_own: bool) -> bool {
        if is_own {
            return true;
        }
        if self.deny.iter().any(|d| d == name) {
            return false;
        }
        if let Some(allow) = &self.allow
            && !allow.iter().any(|a| a == name)
        {
            return false;
        }
        true
    }

    /// Compose two restrictions. The result permits exactly the
    /// tools both sides permit. This is dsh's
    /// "intersect-and-shadow" rule: every ancestor's restriction
    /// shrinks the visible set.
    pub fn intersect(&self, other: &ToolRestriction) -> ToolRestriction {
        let allow = match (&self.allow, &other.allow) {
            (None, None) => None,
            (Some(a), None) | (None, Some(a)) => Some(a.clone()),
            (Some(a), Some(b)) => {
                let mut out: Vec<String> = a
                    .iter()
                    .filter(|n| b.iter().any(|m| m == *n))
                    .cloned()
                    .collect();
                out.sort();
                out.dedup();
                Some(out)
            }
        };
        let mut deny = self.deny.clone();
        for d in &other.deny {
            if !deny.iter().any(|n| n == d) {
                deny.push(d.clone());
            }
        }
        deny.sort();
        deny.dedup();
        ToolRestriction { allow, deny }
    }

    /// Returns `true` iff the restriction is empty (the default
    /// pass-through).
    pub fn is_empty(&self) -> bool {
        self.allow.is_none() && self.deny.is_empty()
    }
}

/// Restricted view of a parent [`ToolRegistry`].
///
/// `RestrictedRegistry` wraps any `Arc<ToolRegistry>` and a
/// [`ToolRestriction`]; every visibility query
/// (`snapshot`, `snapshot_with_provenance`, `get`) passes
/// through the restriction. The wrapper is cheap to clone (an
/// `Arc` + the filter value).
///
/// ## Why not a wrapper trait
///
/// `ToolRegistry`'s API surface is the one already used
/// everywhere in the workspace (and by the
/// `synthia_harness::ReActAgent::with_tool_restriction`); a sealed wrapper that
/// re-exports the visibility-query methods is the simplest
/// way to keep the call sites unchanged.
#[derive(Clone)]
pub struct RestrictedRegistry {
    inner: Arc<ToolRegistry>,
    restriction: Arc<ToolRestriction>,
}

impl RestrictedRegistry {
    /// Wrap a registry with a restriction. The wrapper is
    /// `Clone` and may be cheaply passed through the
    ///   `ReActAgent::with_tool_registry(Arc)` slot.
    pub fn new(inner: Arc<ToolRegistry>, restriction: ToolRestriction) -> Self {
        Self {
            inner,
            restriction: Arc::new(restriction),
        }
    }

    /// Current restriction.
    pub fn restriction(&self) -> &ToolRestriction {
        &self.restriction
    }

    /// Returns `true` if `name` is visible through this
    /// wrapper. Own registrations are exempt; for inherited
    /// tools, `is_relevant(name, false)` is consulted.
    pub fn is_visible(&self, name: &str) -> bool {
        // R29 keeps the cheap path for the empty case: no
        // lock, no filter.
        if self.restriction.is_empty() {
            return true;
        }
        self.restriction.is_relevant(name, false)
    }

    /// The underlying (unrestricted) registry. For routing
    /// decisions that need to ignore the filter (e.g. a
    /// tool that needs to look up its own dependencies).
    pub fn inner(&self) -> &Arc<ToolRegistry> {
        &self.inner
    }

    /// Filter the parent's snapshot through the restriction.
    /// Returns the names the child can see.
    ///
    /// Reads the parent's cached catalog and clones only the names that
    /// survive the filter, so a narrowed child is cheap on a large
    /// registry.
    pub fn visible_names(&self) -> Vec<String> {
        let visible = |snap: &crate::ToolMetadataSnapshot| {
            self.restriction.is_empty()
                || self.restriction.is_relevant(&snap.name, false)
        };
        self.inner
            .snapshot_cached()
            .iter()
            .filter(|snap| visible(snap))
            .map(|snap| snap.name.clone())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::registry::ToolEntry;

    /// Local stand-in for a registered tool — the builtin tool
    /// implementations live in their own plugin crates now.
    struct RestrictionProbeTool;

    #[async_trait::async_trait]
    impl crate::traits::Tool for RestrictionProbeTool {
        fn name(&self) -> &str {
            "read"
        }

        fn description(&self) -> &str {
            "probe"
        }

        fn parameters(&self) -> serde_json::Value {
            serde_json::json!({"type": "object"})
        }

        async fn call(
            &self,
            _: serde_json::Value,
            _: &crate::types::Context,
        ) -> crate::types::ToolOutput {
            crate::types::ToolOutput::text("ok")
        }
    }

    fn build_registry() -> Arc<ToolRegistry> {
        let r = Arc::new(ToolRegistry::new());
        r.register_entry(ToolEntry::new(Arc::new(RestrictionProbeTool)));
        r
    }

    #[test]
    fn empty_restriction_passes_through() {
        let r = ToolRestriction::none();
        assert!(r.is_relevant("read", false));
        assert!(r.is_relevant("anything", true));
        assert!(r.is_empty());
    }

    #[test]
    fn deny_hides_listed_tools() {
        let r = ToolRestriction::deny(["read"]);
        assert!(!r.is_relevant("read", false));
        assert!(r.is_relevant("write", false));
    }

    #[test]
    fn allow_hides_unlisted_inherited_tools() {
        let r = ToolRestriction::allow(["read"]);
        assert!(r.is_relevant("read", false));
        assert!(!r.is_relevant("write", false));
    }

    #[test]
    fn own_tools_are_always_visible() {
        let r = ToolRestriction::deny(["read"]);
        // Even a denied tool is visible when the child
        // registered it itself.
        assert!(r.is_relevant("read", true));
    }

    #[test]
    fn intersect_combines_deny_lists() {
        let a = ToolRestriction::deny(["shell"]);
        let b = ToolRestriction::deny(["write"]);
        let c = a.intersect(&b);
        assert!(!c.is_relevant("shell", false));
        assert!(!c.is_relevant("write", false));
        assert!(c.is_relevant("read", false));
    }

    #[test]
    fn intersect_combines_allow_lists() {
        let a = ToolRestriction::allow(["read", "write"]);
        let b = ToolRestriction::allow(["read", "shell"]);
        let c = a.intersect(&b);
        assert!(c.is_relevant("read", false));
        assert!(!c.is_relevant("write", false));
        assert!(!c.is_relevant("shell", false));
    }

    #[test]
    fn intersect_none_allow_passes_through() {
        let a = ToolRestriction::allow(["read"]);
        let b = ToolRestriction::none();
        let c = a.intersect(&b);
        assert!(c.is_relevant("read", false));
        assert!(!c.is_relevant("write", false));
    }

    #[test]
    fn restricted_registry_filters_visible_names() {
        let inner = build_registry();
        let wrapper =
            RestrictedRegistry::new(inner, ToolRestriction::deny(["read"]));
        let visible = wrapper.visible_names();
        assert!(visible.is_empty(), "deny hides the only tool");
    }

    #[test]
    fn restricted_registry_passthrough_when_empty() {
        let inner = build_registry();
        let wrapper = RestrictedRegistry::new(inner, ToolRestriction::none());
        let visible = wrapper.visible_names();
        assert_eq!(visible, vec!["read".to_string()]);
    }

    #[test]
    fn restricted_registry_is_visible_helper() {
        let inner = build_registry();
        let wrapper =
            RestrictedRegistry::new(inner, ToolRestriction::deny(["read"]));
        assert!(!wrapper.is_visible("read"));
        assert!(wrapper.is_visible("write"));
    }

    #[test]
    fn wire_roundtrip_default_restriction() {
        let r = ToolRestriction::none();
        let v = serde_json::to_value(&r).expect("serialize");
        assert!(v.as_object().unwrap().is_empty());
        let back: ToolRestriction =
            serde_json::from_value(v).expect("deserialize");
        assert_eq!(back, ToolRestriction::none());
    }

    #[test]
    fn wire_roundtrip_allow_deny() {
        let r = ToolRestriction {
            allow: Some(vec!["read".to_string()]),
            deny: vec!["shell".to_string()],
        };
        let v = serde_json::to_value(&r).expect("serialize");
        assert_eq!(v["allow"][0], "read");
        assert_eq!(v["deny"][0], "shell");
        let back: ToolRestriction =
            serde_json::from_value(v).expect("deserialize");
        assert_eq!(back, r);
    }
}
