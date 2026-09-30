//! The registry's read side: model-facing descriptors and
//! user-facing snapshots, both cached by registry version.
//! Method continuation of [`ToolRegistry`](super::ToolRegistry).

use std::sync::Arc;

use super::{
    ToolRegistry,
    catalog::{ToolDescriptor, ToolMetadataSnapshot, ToolProvenanceRecord},
    entry::descriptor_for,
};

impl ToolRegistry {
    /// Model-facing metadata for every registered tool, sorted by
    /// name — the input to
    /// [`crate::surface::project_tool_definitions`].
    ///
    /// Unlike [`ToolRegistry::snapshot`] / [`ToolRegistry::output_definition`],
    /// this applies **no** visibility filtering: the returned
    /// descriptors carry the registered `is_hidden` and
    /// [`ToolExposure`](super::catalog::ToolExposure) flags so the caller can decide what the model
    /// is shown. Sorting by name makes the projected list byte-stable
    /// across runs (the underlying `HashMap` has no order), matching
    /// the ordering guarantee [`ToolRegistry::snapshot`] already gives
    /// its consumers.
    #[must_use]
    pub fn descriptors(&self) -> Vec<ToolDescriptor> {
        let mut out: Vec<ToolDescriptor> = self
            .inner
            .read()
            .tools
            .values()
            .filter_map(|entries| entries.last())
            .map(descriptor_for)
            .collect();
        out.sort_by(|a, b| a.name.cmp(&b.name));
        out
    }

    /// [`ToolRegistry::descriptors`], cached by registry version.
    ///
    /// The descriptor list is derived from the registered entries, and
    /// the registry already stamps a monotonic [`ToolRegistry::version`]
    /// on every register/unregister. Callers that rebuild the
    /// model-facing tool list on a hot path (the agent loop does, once
    /// per iteration) can therefore hold one `Arc` and pay the deep
    /// clone of every tool's JSON schema only when the catalog actually
    /// changes.
    ///
    /// Returns the same ordering and contents as
    /// [`ToolRegistry::descriptors`] — this is a cache, not a different
    /// projection.
    #[must_use]
    pub fn descriptors_cached(&self) -> Arc<Vec<ToolDescriptor>> {
        let version = self.version();
        {
            let cache = self.descriptor_cache.lock();
            if let Some((cached_version, descriptors)) = cache.as_ref()
                && *cached_version == version
            {
                return Arc::clone(descriptors);
            }
        }
        // Recompute outside the cache lock (this takes the registry
        // read lock) and tolerate a racing insert: whichever version
        // lands last wins, and a stale entry is discarded on the next
        // version mismatch.
        let computed = Arc::new(self.descriptors());
        let mut cache = self.descriptor_cache.lock();
        *cache = Some((version, Arc::clone(&computed)));
        computed
    }

    /// Snapshot of registered tools. Results are sorted by `name` so the
    /// output is stable across runs (the underlying `inner.tools` is a
    /// `HashMap`, so insertion order is not preserved). Stable ordering
    /// matters for downstream consumers like the agent card
    /// builder, where any two snapshots of the same registry must
    /// serialize identically. Hidden tools (registered via
    /// `ToolEntry::with_is_hidden(true)`) are filtered out so the LLM
    /// never sees them — this is the privacy/focus contract.
    pub fn snapshot(&self) -> Vec<ToolMetadataSnapshot> {
        let mut out: Vec<ToolMetadataSnapshot> = self
            .inner
            .read()
            .tools
            .values()
            .filter_map(|entries| entries.last())
            .filter(|entry| !entry.is_hidden)
            .map(|entry| ToolMetadataSnapshot {
                name: entry.tool.name().to_string(),
                description: entry.tool.description().to_string(),
            })
            .collect();
        out.sort_by(|a, b| a.name.cmp(&b.name));
        out
    }

    /// [`ToolRegistry::snapshot`], memoised by registry version.
    ///
    /// The catalog is the *input* to every "what should the model be
    /// told about?" question — the tier cap, the active tool groups,
    /// the operator listing — and each of those consumers wants a
    /// subset of it. Rebuilding the full list (two `String` clones per
    /// tool, then a sort) per call made a five-tool tier pay for forty
    /// descriptions; handing back the shared `Arc` lets each consumer
    /// clone only the entries it keeps.
    ///
    /// The memo shares [`ToolRegistry::version`]'s contract with
    /// `descriptors_cached`: a registration or removal bumps the
    /// version, so the next call recomputes and a stale catalog cannot
    /// reach the model.
    #[must_use]
    pub fn snapshot_cached(&self) -> Arc<Vec<ToolMetadataSnapshot>> {
        let version = self.version();
        {
            let cache = self.snapshot_cache.lock();
            if let Some((cached_version, snapshots)) = cache.as_ref()
                && *cached_version == version
            {
                return Arc::clone(snapshots);
            }
        }
        // Compute outside the lock (snapshot takes the registry read
        // lock) and tolerate a racing insert: whichever version wins is
        // a valid function of the registry, and a later mismatch
        // recomputes.
        let computed = Arc::new(self.snapshot());
        let mut cache = self.snapshot_cache.lock();
        *cache = Some((version, Arc::clone(&computed)));
        computed
    }

    /// R17: the canonical rendering contract for `name` — the
    /// consumer seam for [`crate::ToolOutputDefinition`]. The
    /// agent loop / UI reads this to pick a renderer; a tool that
    /// never overrides `Tool::output_definition` returns the
    /// passthrough default (name + `RenderKind::Text`).
    ///
    /// Returns `None` for unknown (or hidden) tool names.
    #[must_use]
    pub fn output_definition(
        &self,
        name: &str,
    ) -> Option<crate::output::ToolOutputDefinition> {
        let inner = self.inner.read();
        let entry = inner.tools.get(name)?.last()?;
        let descriptor = descriptor_for(entry);
        if descriptor.is_hidden {
            return None;
        }
        Some(entry.tool.output_definition())
    }

    /// Snapshot of registered tools with provenance included.
    ///
    /// Same ordering guarantee and hidden-filter as [`Self::snapshot`].
    /// The provenance comes from the most-recently inserted entry
    /// for each tool name (matches `snapshot`'s "last entry wins"
    /// semantics).
    pub fn snapshot_with_provenance(&self) -> Vec<ToolProvenanceRecord> {
        let mut out: Vec<ToolProvenanceRecord> = self
            .inner
            .read()
            .tools
            .values()
            .filter_map(|entries| entries.last())
            .filter(|entry| !entry.is_hidden)
            .map(|entry| ToolProvenanceRecord {
                metadata: ToolMetadataSnapshot {
                    name: entry.tool.name().to_string(),
                    description: entry.tool.description().to_string(),
                },
                provenance: entry.provenance,
            })
            .collect();
        out.sort_by(|a, b| a.metadata.name.cmp(&b.metadata.name));
        out
    }
}
