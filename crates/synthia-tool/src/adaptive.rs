//! [`AdaptiveRegistry`] — tier-aware tool-set wrapper.
//!
//! Adopted from traitclaw
//! `crates/traitclaw-core/src/registries.rs:373-457`
//! (`AdaptiveRegistry::with_tier_limits`). The wrapper sits on top
//! of any [`ToolRegistry`] and exposes the same dispatch path
//! unchanged; the *read side* — the LLM-facing tool-definition list
//! — is filtered to the active tier so small models see the most
//! useful ≤N tools instead of the full catalog.
//!
//! ## Why a wrapper, not a flag on `ToolRegistry`?
//!
//! `ToolRegistry` is the writer-of-truth (registration, dispatch,
//! concurrency invariants). Adding tier to it would couple every
//! read site to the active tier; instead, [`AdaptiveRegistry`] takes
//! `Arc<ToolRegistry>` and only re-shapes what the LLM sees. The
//! registry's own dispatcher continues to enforce name uniqueness,
//! provenance, and the panic-isolation contract.
//!
//! ## Consuming the cap
//!
//! [`AdaptiveRegistry::visible_tool_names`] is the allow-list half of
//! the projection in [`crate::surface`]:
//!
//! ```ignore
//! let visible: HashSet<String> =
//!     adaptive.visible_tool_names().into_iter().collect();
//! let defs = synthia_tool::project_tool_definitions(
//!     &registry.descriptors(),
//!     &synthia_tool::called_tool_names(&messages),
//!     Some(&visible),
//! );
//! ```
//!
//! ## Visibility precedence
//!
//! The wrapper caps the LLM-visible list at
//! [`TierLimits::max_visible_tools`]. The cap is applied **after**
//! [`ToolRegistry::snapshot`] has already filtered out hidden
//! tools, so the wrapper only ever sees the catalog the registry
//! would normally expose. Tools are kept in registration order
//! (FIFO).

use std::sync::Arc;

use synthia_provider::{ModelTier, TierLimits};

use crate::{ToolMetadataSnapshot, ToolRegistry};

/// Tier-aware tool-set wrapper. Cheap to clone; shares the inner
/// registry by `Arc`.
#[derive(Clone)]
pub struct AdaptiveRegistry {
    inner: Arc<ToolRegistry>,
    tier: ModelTier,
}

impl AdaptiveRegistry {
    /// Wrap `inner` with a tier-aware read-side filter. The inner
    /// registry's dispatcher is unchanged; only the LLM-facing tool
    /// list is filtered to the tier's `max_visible_tools` cap.
    pub fn new(inner: Arc<ToolRegistry>, tier: ModelTier) -> Self {
        Self { inner, tier }
    }

    /// Change the active tier. Returns `Self` for builder-style
    /// chaining. Cheap — no allocation.
    pub fn with_tier(mut self, tier: ModelTier) -> Self {
        self.tier = tier;
        self
    }

    /// Current tier.
    pub fn tier(&self) -> ModelTier {
        self.tier.clone()
    }

    /// Tier-driven cap.
    pub fn limits(&self) -> TierLimits {
        TierLimits::for_tier(self.tier.clone())
    }

    /// Inner registry (tier-agnostic). Use it for dispatch;
    /// use the wrapper for the LLM-facing tool list.
    pub fn inner(&self) -> &Arc<ToolRegistry> {
        &self.inner
    }

    /// Visible tool names — the set the LLM should be told about.
    /// Reads the inner registry's cached snapshot (which already
    /// filters out hidden tools) and caps at the tier's
    /// `max_visible_tools`.
    ///
    /// Only the names the cap keeps are cloned: the tier's whole point
    /// is to show fewer tools, so the work should shrink with the cap,
    /// not with the registry.
    pub fn visible_tool_names(&self) -> Vec<String> {
        let caps = self.limits().max_visible_tools;
        self.inner
            .snapshot_cached()
            .iter()
            .take(caps)
            .map(|snap| snap.name.clone())
            .collect()
    }

    /// Visible tool metadata snapshots (name + description only).
    /// Cheap, allocation-light; use it when the caller only needs the
    /// catalog text. The model-facing JSON-Schema list comes from
    /// [`crate::surface::project_tool_definitions`] fed with
    /// [`ToolRegistry::descriptors`] and these names as the
    /// visible-name filter.
    pub fn visible_metadata_snapshots(&self) -> Vec<ToolMetadataSnapshot> {
        self.inner
            .snapshot_cached()
            .iter()
            .take(self.limits().max_visible_tools)
            .cloned()
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use async_trait::async_trait;
    use synthia_provider::{ModelTier, TierLimits};

    use super::*;
    use crate::{
        Tool,
        ToolEntry,
        ToolOutput,
        traits::ExecutionMode,
        types::Context,
    };

    struct EchoTool {
        name: String,
        desc: String,
    }

    #[async_trait]
    impl Tool for EchoTool {
        fn name(&self) -> &str {
            &self.name
        }

        fn description(&self) -> &str {
            &self.desc
        }

        fn parameters(&self) -> serde_json::Value {
            serde_json::json!({})
        }

        fn mode(&self) -> ExecutionMode {
            ExecutionMode::Parallel
        }

        async fn call(
            &self,
            _input: serde_json::Value,
            _context: &Context,
        ) -> ToolOutput {
            ToolOutput::text(format!("echo:{}", self.name))
        }
    }

    fn make_registry(names: &[&str]) -> Arc<ToolRegistry> {
        let reg = ToolRegistry::new();
        for n in names {
            reg.register_entry(ToolEntry::new(Arc::new(EchoTool {
                name: (*n).to_string(),
                desc: format!("desc for {n}"),
            })));
        }
        Arc::new(reg)
    }

    #[test]
    fn small_tier_caps_visible_tools() {
        let reg = make_registry(&["a", "b", "c", "d", "e", "f", "g"]);
        let adaptive = AdaptiveRegistry::new(reg, ModelTier::Small);
        let visible = adaptive.visible_tool_names();
        assert_eq!(visible.len(), TierLimits::SMALL.max_visible_tools);
        assert_eq!(visible, vec!["a", "b", "c", "d", "e"]);
    }

    #[test]
    fn large_tier_keeps_all_tools() {
        let reg = make_registry(&["a", "b", "c", "d", "e"]);
        let adaptive = AdaptiveRegistry::new(reg, ModelTier::Large);
        let visible = adaptive.visible_tool_names();
        // All 5 should fit under the Large cap (32).
        assert_eq!(visible.len(), 5);
    }

    #[test]
    fn hidden_tools_are_excluded() {
        let reg = ToolRegistry::new();
        reg.register_entry(ToolEntry::new(Arc::new(EchoTool {
            name: "visible".into(),
            desc: "shown".into(),
        })));
        reg.register_entry(
            ToolEntry::new(Arc::new(EchoTool {
                name: "secret".into(),
                desc: "hidden".into(),
            }))
            .with_is_hidden(true),
        );
        let adaptive = AdaptiveRegistry::new(Arc::new(reg), ModelTier::Large);
        let visible = adaptive.visible_tool_names();
        assert!(!visible.contains(&"secret".to_string()));
        assert!(visible.contains(&"visible".to_string()));
    }

    #[test]
    fn with_tier_swaps_tier() {
        let reg = make_registry(&["a", "b", "c"]);
        let adaptive = AdaptiveRegistry::new(reg, ModelTier::Large)
            .with_tier(ModelTier::Small);
        assert_eq!(adaptive.tier(), ModelTier::Small);
    }

    #[test]
    fn limits_reflect_active_tier() {
        let reg = make_registry(&["a"]);
        let adaptive = AdaptiveRegistry::new(reg, ModelTier::Medium);
        assert_eq!(
            adaptive.limits().tool_budget,
            TierLimits::MEDIUM.tool_budget
        );
    }
}
