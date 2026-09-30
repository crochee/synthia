//! [`ToolSchemaBuilder`] — small builder for JSON-Schema with
//! feature-gated field removal.
//!
//! R29 (pi-subagents `invocation-config.ts:isolationParam` parity).
//! The pi-subagents pattern: when a capability is not enabled,
//! the corresponding field is **removed from the LLM-facing
//! schema** — not just hidden behind a `description`. A refused
//! parameter with a leftover description would teach the model
//! the capability exists; a removed field is invisible.
//!
//! `ToolSchemaBuilder` wraps a `serde_json::Map<String, Value>`
//! and exposes `.with`, `.without`, `.merge`, and `.build`.
//! Tools compute their schema as a normal `serde_json::Value`,
//! then surgically remove fields based on a [`ToolFeatures`]
//! value.

use std::collections::BTreeMap;

use serde_json::{Map, Value};

/// Tool capability flags. The set is open: tools may add their
/// own feature flags without modifying this struct.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ToolFeatures {
    /// Per-feature enable flags. The map key is the feature
    /// name (e.g. `"image_preview"`, `"worktree_isolation"`).
    /// Presence-with-`true` means the feature is on; absence
    /// means off.
    pub enabled: BTreeMap<String, bool>,
}

impl ToolFeatures {
    /// New feature set with nothing enabled.
    pub fn none() -> Self {
        Self::default()
    }

    /// Builder: enable a feature.
    #[must_use]
    pub fn with(mut self, feature: impl Into<String>) -> Self {
        self.enabled.insert(feature.into(), true);
        self
    }

    /// Builder: disable a feature.
    #[must_use]
    pub fn without(mut self, feature: impl Into<String>) -> Self {
        self.enabled.insert(feature.into(), false);
        self
    }

    /// Returns `true` iff the feature is enabled. A feature
    /// that has never been set is **off** (the default).
    pub fn is_enabled(&self, feature: &str) -> bool {
        self.enabled.get(feature).copied().unwrap_or(false)
    }
}

/// Builder for a tool's JSON-Schema with feature-gated removal.
#[derive(Clone, Debug, Default)]
pub struct ToolSchemaBuilder {
    inner: Map<String, Value>,
}

impl ToolSchemaBuilder {
    /// New empty builder.
    pub fn new() -> Self {
        Self::default()
    }

    /// Builder: set a top-level key. The previous value, if
    /// any, is overwritten.
    #[must_use]
    pub fn with(mut self, key: impl Into<String>, value: Value) -> Self {
        self.inner.insert(key.into(), value);
        self
    }

    /// Builder: extend with all entries from another JSON
    /// object. The other object's entries overwrite this
    /// builder's entries on key collision.
    #[must_use]
    pub fn merge(mut self, other: Map<String, Value>) -> Self {
        for (k, v) in other {
            self.inner.insert(k, v);
        }
        self
    }

    /// Builder: set a property in the `properties` sub-object.
    /// Convenience for the common case where the tool wants
    /// to add a field to its existing `properties` map.
    #[must_use]
    pub fn with_property(
        mut self,
        name: impl Into<String>,
        schema: Value,
    ) -> Self {
        let props = self
            .inner
            .entry("properties".to_string())
            .or_insert_with(|| Value::Object(Map::new()));
        if let Value::Object(map) = props {
            map.insert(name.into(), schema);
        }
        self
    }

    /// Builder: drop a top-level key. Useful for "remove this
    /// capability from the schema when the feature is off".
    #[must_use]
    pub fn without(mut self, key: &str) -> Self {
        self.inner.remove(key);
        self
    }

    /// Builder: drop a property from the `properties`
    /// sub-object.
    #[must_use]
    pub fn without_property(mut self, name: &str) -> Self {
        if let Some(Value::Object(map)) = self.inner.get_mut("properties") {
            map.remove(name);
        }
        self
    }

    /// Builder: apply a feature-gated removal. When the
    /// feature is **off**, the property is dropped (and so is
    /// its `required` entry). When the feature is **on**, the
    /// schema is unchanged.
    #[must_use]
    pub fn with_feature(
        self,
        feature: &str,
        features: &ToolFeatures,
        property: &str,
    ) -> Self {
        if !features.is_enabled(feature) {
            return self.without_property(property).without_required(property);
        }
        self
    }

    /// Builder: drop a name from the `required` array.
    #[must_use]
    pub fn without_required(mut self, name: &str) -> Self {
        if let Some(Value::Array(arr)) = self.inner.get_mut("required") {
            arr.retain(|v| v.as_str() != Some(name));
        }
        self
    }

    /// Finalise the schema.
    pub fn build(self) -> Value {
        Value::Object(self.inner)
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn empty_builder_produces_empty_object() {
        let s = ToolSchemaBuilder::new().build();
        assert_eq!(s, json!({}));
    }

    #[test]
    fn with_chain_sets_keys() {
        let s = ToolSchemaBuilder::new()
            .with("type", json!("object"))
            .with("title", json!("MyTool"))
            .build();
        assert_eq!(s["type"], "object");
        assert_eq!(s["title"], "MyTool");
    }

    #[test]
    fn merge_overwrites_collision() {
        let mut other = Map::new();
        other.insert("type".to_string(), json!("object"));
        other.insert("x".to_string(), json!(1));
        let s = ToolSchemaBuilder::new()
            .with("type", json!("string"))
            .merge(other)
            .build();
        assert_eq!(s["type"], "object"); // overwritten by merge
        assert_eq!(s["x"], 1);
    }

    #[test]
    fn with_property_adds_to_existing_properties() {
        let s = ToolSchemaBuilder::new()
            .with_property("name", json!({"type": "string"}))
            .with_property("count", json!({"type": "integer"}))
            .build();
        assert_eq!(s["properties"]["name"]["type"], "string");
        assert_eq!(s["properties"]["count"]["type"], "integer");
    }

    #[test]
    fn without_drops_key() {
        let s = ToolSchemaBuilder::new()
            .with("a", json!(1))
            .with("b", json!(2))
            .without("a")
            .build();
        assert!(s.get("a").is_none());
        assert_eq!(s["b"], 2);
    }

    #[test]
    fn feature_off_removes_property_and_required() {
        let s = ToolSchemaBuilder::new()
            .with("type", json!("object"))
            .with_property("isolation", json!({"type": "string"}))
            .with("required", json!(["isolation", "other"]))
            .with_feature(
                "worktree_isolation",
                &ToolFeatures::none(),
                "isolation",
            )
            .build();
        assert!(
            s["properties"].get("isolation").is_none(),
            "feature-off removes the property"
        );
        assert_eq!(s["required"], json!(["other"]));
    }

    #[test]
    fn feature_on_keeps_property() {
        let s = ToolSchemaBuilder::new()
            .with("type", json!("object"))
            .with_property("isolation", json!({"type": "string"}))
            .with("required", json!(["isolation"]))
            .with_feature(
                "worktree_isolation",
                &ToolFeatures::none().with("worktree_isolation"),
                "isolation",
            )
            .build();
        assert!(s["properties"].get("isolation").is_some());
        assert_eq!(s["required"], json!(["isolation"]));
    }

    #[test]
    fn features_default_is_off() {
        let f = ToolFeatures::default();
        assert!(!f.is_enabled("any"));
    }

    #[test]
    fn features_with_chain() {
        let f = ToolFeatures::none().with("a").with("b").without("a");
        assert!(!f.is_enabled("a"));
        assert!(f.is_enabled("b"));
        assert!(!f.is_enabled("c"));
    }
}
