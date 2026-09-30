//! Tool registry storage, registration, scoped cleanup, materialization,
//! dispatch, snapshots, and `Registry` integration.
//!
//! # Module layout
//!
//! - `catalog`: the descriptor / provenance / exposure types.
//! - `entry`: the registration value type + the internal storage row.
//! - `ToolRegistry` (this file): storage, registration, mutation.
//! - `scope` / `snapshot` / `dispatch`: method continuations.
//! - `registry_trait`: the `synthia_core::Registry` impl, kept private
//!   to the crate.

use std::{collections::HashMap, sync::Arc};

use parking_lot::RwLock;

mod catalog;
mod dispatch;
mod entry;
mod registry_trait;
mod scope;
mod snapshot;

pub use catalog::{
    ToolAnnotations,
    ToolCategory,
    ToolDescriptor,
    ToolExposure,
    ToolMetadataSnapshot,
    ToolProvenance,
    ToolProvenanceRecord,
};
pub use entry::ToolEntry;
use entry::{ProviderEntry, entry_from_provider};
pub use scope::{RegistrationScope, RegistrationToken};

/// Unified tool registry.
pub struct ToolRegistry {
    pub(crate) inner: RwLock<ToolRegistryInner>,
    /// Max parallel tool invocations per dispatch call.
    ///
    /// The dispatch path lives in section 6; this field is the
    /// per-call semaphore-bounded executor's knob.
    pub(crate) max_concurrent: usize,
    /// Monotonically increasing version counter, bumped on
    /// every register/unregister. Lets callers cheaply key a
    /// snapshot cache (e.g. `collect_tool_defs` in
    /// `crates/synthia-server/src/routes/tool.rs`) by the
    /// current version without holding the registry lock or
    /// diffing the full tool list.
    pub(crate) version: std::sync::atomic::AtomicU64,
    /// Where `run_stream`'s per-tool tasks go. Default =
    /// `crate::spawn::TokioSpawner`; replaceable via
    /// [`ToolRegistry::with_spawner`] so dispatch does not decide the
    /// consumer's runtime.
    pub(crate) spawner: Arc<dyn synthia_core::spawn::Spawner>,
    /// Version-keyed memo of [`ToolRegistry::descriptors`]: the
    /// projection inputs the agent loop rebuilds every iteration. Holds
    /// `(registry version, descriptors)`; a version mismatch recomputes.
    pub(crate) descriptor_cache:
        parking_lot::Mutex<Option<(u64, Arc<Vec<ToolDescriptor>>)>>,
    /// Version-keyed memo of [`ToolRegistry::snapshot`]: the catalog
    /// every "what should the model see?" consumer starts from
    /// (`AdaptiveRegistry`'s tier cap, `GroupedRegistry`'s groups, the
    /// server's tool listing). Same contract as `descriptor_cache` —
    /// `(registry version, snapshots)`, recomputed on a version
    /// mismatch.
    pub(crate) snapshot_cache:
        parking_lot::Mutex<Option<(u64, Arc<Vec<ToolMetadataSnapshot>>)>>,
    /// When `true`, `run_stream` validates every tool call's `input`
    /// against the resolved tool's `parameters()` (a JSON Schema)
    /// before spawning a task. Mismatches synthesize a
    /// `ToolOutput::error` listing the dotted-path violations, so the
    /// model can self-correct in the same turn. Default `false` —
    /// validation is opt-in so the hot dispatch path is unchanged
    /// for consumers that already validate inside `Tool::call` or
    /// trust the provider's tool-call arguments.
    pub(crate) validate_arguments: bool,
}

#[derive(Clone)]
pub(crate) struct ToolRegistryInner {
    /// Tool name → entries (LIFO for non-core tools).
    pub(crate) tools: HashMap<String, Vec<ProviderEntry>>,
    /// Next registration token.
    pub(crate) next_registration: u64,
}

impl ToolRegistry {
    /// Create an empty registry.
    pub fn new() -> Self {
        Self {
            inner: RwLock::new(ToolRegistryInner {
                tools: HashMap::new(),
                next_registration: 1,
            }),
            max_concurrent: 5,
            version: std::sync::atomic::AtomicU64::new(0),
            spawner: Arc::new(crate::spawn::TokioSpawner),
            descriptor_cache: parking_lot::Mutex::new(None),
            snapshot_cache: parking_lot::Mutex::new(None),
            validate_arguments: false,
        }
    }

    /// Run dispatch tasks on `spawner` instead of tokio.
    ///
    /// [`ToolRegistry::run_stream`] hands one detached task per tool
    /// call to this spawner. The default is tokio (the crate's own
    /// runtime); a consumer building the registry for another executor
    /// installs their implementation here. Builder-style, because
    /// `new()` is the only constructor.
    #[must_use]
    pub fn with_spawner(
        mut self,
        spawner: Arc<dyn synthia_core::spawn::Spawner>,
    ) -> Self {
        self.spawner = spawner;
        self
    }

    /// Enable JSON-Schema validation of every tool call's arguments
    /// against the tool's `parameters()` before dispatch.
    ///
    /// Off by default — the common case is "the provider's tool-call
    /// arguments are already well-formed, the tool validates inside
    /// `call`". A deployment that wants the registry to surface a
    /// schema mismatch as an `is_error` tool result (with the
    /// dotted-path violations in the body) flips this on once at
    /// boot:
    ///
    /// ```ignore
    /// let registry = ToolRegistry::new()
    ///     .with_argument_validation(true);
    /// ```
    ///
    /// Validation is paid once per `run_stream` call, in the same
    /// read-lock span as the tool lookup, so a misbehaving call
    /// costs a synthesized error Result instead of a spawned task
    /// that runs a tool it shouldn't. Empty / non-object schemas
    /// pass through (matches `synthia_core::validate_against_schema`
    /// — unknown keywords are annotations, not assertions).
    #[must_use]
    pub fn with_argument_validation(mut self, on: bool) -> Self {
        self.validate_arguments = on;
        self
    }

    /// Whether the registry validates tool-call arguments against
    /// the tool's `parameters()` before dispatch.
    #[must_use]
    pub fn argument_validation_enabled(&self) -> bool {
        self.validate_arguments
    }

    /// Register a `ToolEntry` directly — wraps the inner `Arc<dyn Tool>`
    /// into a `ProviderEntry` and inserts it.
    ///
    /// Returns `true` if the tool was inserted; `false` if a Core tool
    /// with the same name already exists and won the immutability
    /// guard.
    pub fn register_entry(&self, entry: ToolEntry) -> bool {
        let mut inner = self.inner.write();
        let inserted = self.register_entry_inner(&mut inner, entry).is_some();
        if inserted {
            self.version
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
        inserted
    }

    /// Insert a [`ToolEntry`] into `inner.tools`, allocating a fresh
    /// `RegistrationToken` for it. Caller must hold the write lock on
    /// `inner`. Returns the token used if the entry was inserted, or
    /// `None` if a core tool already occupies the name.
    fn register_entry_inner(
        &self,
        inner: &mut ToolRegistryInner,
        entry: ToolEntry,
    ) -> Option<RegistrationToken> {
        let tool = entry.tool_instance();
        let name = synthia_core::RegistryItem::name(&entry).to_string();
        if let Some(existing) = inner.tools.get(&name)
            && existing
                .iter()
                .any(|e| e.provenance == ToolProvenance::Core)
        {
            return None;
        }
        let token = RegistrationToken(inner.next_registration);
        inner.next_registration += 1;
        let provider_entry = ProviderEntry {
            provider_token: token.clone(),
            provenance: ToolProvenance::Dynamic,
            is_hidden: entry.is_hidden(),
            exposure: entry.exposure(),
            tool,
        };
        inner.tools.entry(name).or_default().push(provider_entry);
        Some(token)
    }

    /// Remove all entries whose `tool.name()` matches the given plain
    /// name. Returns `true` if anything was removed.
    pub fn unregister_by_name(&self, name: &str) -> bool {
        let mut inner = self.inner.write();
        let removed = inner.tools.remove(name).is_some();
        if removed {
            self.version
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
        removed
    }

    /// Change how much of `name` the model is shown, after
    /// registration. Returns whether the tool exists: `false` — with
    /// no mutation — when no tool by that name is registered, so a
    /// caller never mistakes a typo for a successful policy.
    ///
    /// The write lands on the entry the read paths resolve to (the
    /// last-registered provider entry for the name — the same one
    /// [`ToolRegistry::descriptors`], [`ToolRegistry::snapshot`] and
    /// `run_stream` use), so the change reaches the model-facing
    /// projection immediately. Unlike [`ToolExposure::Hidden`], an
    /// exposure change never alters dispatch. A successful *change*
    /// bumps [`ToolRegistry::version`], which is what invalidates the
    /// server's model-facing definition cache; setting a value the
    /// entry already carries is a no-op that still returns `true`
    /// and leaves the version alone.
    pub fn set_exposure(&self, name: &str, exposure: ToolExposure) -> bool {
        self.mutate_entry(name, |entry| {
            let changed = entry.exposure != exposure;
            entry.exposure = exposure;
            changed
        })
    }

    /// Set `name`'s privacy flag after registration. `hidden = true`
    /// removes the tool from every listing **and** makes
    /// [`ToolRegistry::run_stream`] refuse a model-issued call,
    /// mirroring [`ToolEntry::with_is_hidden`]; `false` restores it.
    ///
    /// Returns whether the tool exists (`false` mutates nothing), and
    /// bumps [`ToolRegistry::version`] on a successful change.
    pub fn set_hidden(&self, name: &str, hidden: bool) -> bool {
        self.mutate_entry(name, |entry| {
            let changed = entry.is_hidden != hidden;
            entry.is_hidden = hidden;
            changed
        })
    }

    /// How much of `name` the model is shown, or `None` when the tool
    /// is not registered.
    ///
    /// Reads the same entry [`ToolRegistry::descriptors`] projects.
    /// Hiding is orthogonal: a hidden tool still reports the exposure
    /// it carries.
    #[must_use]
    pub fn exposure(&self, name: &str) -> Option<ToolExposure> {
        self.inner
            .read()
            .tools
            .get(name)
            .and_then(|entries| entries.last())
            .map(|entry| entry.exposure)
    }

    /// Apply `mutate` to the entry `name` resolves to. Returns
    /// whether the tool existed; bumps the version only when `mutate`
    /// reports a real change. One write-lock acquisition, no awaits,
    /// so callers may invoke it from a sync context.
    fn mutate_entry(
        &self,
        name: &str,
        mutate: impl FnOnce(&mut ProviderEntry) -> bool,
    ) -> bool {
        let mut inner = self.inner.write();
        let Some(entry) = inner.tools.get_mut(name).and_then(|e| e.last_mut())
        else {
            return false;
        };
        let changed = mutate(entry);
        drop(inner);
        if changed {
            self.version
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
        true
    }

    /// Whether a tool named `name` is registered — under the same
    /// key `run_stream` dispatches by, and **including hidden
    /// tools**. This answers a wiring question ("is this name
    /// real?") rather than the model-facing one, so unlike
    /// [`ToolRegistry::snapshot`] it deliberately sees the whole
    /// index; callers that want the advertised set use `snapshot`.
    #[must_use]
    pub fn contains(&self, name: &str) -> bool {
        self.inner.read().tools.contains_key(name)
    }

    /// Return the number of registered tools (LIFO top-only count).
    pub fn tool_count(&self) -> usize {
        let inner = self.inner.read();
        inner.tools.len()
    }

    /// Current monotonic version of the registry's tool set.
    /// Bumped on every successful `register_entry` /
    /// `unregister_by_name`. Cheap (one relaxed atomic load) —
    /// callers can use this to key a snapshot cache without
    /// holding the registry lock.
    pub fn version(&self) -> u64 {
        self.version.load(std::sync::atomic::Ordering::Relaxed)
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for ToolRegistry {
    fn clone(&self) -> Self {
        let inner = self.inner.read().clone();
        Self {
            inner: RwLock::new(inner),
            max_concurrent: self.max_concurrent,
            version: std::sync::atomic::AtomicU64::new(
                self.version.load(std::sync::atomic::Ordering::Relaxed),
            ),
            spawner: Arc::clone(&self.spawner),
            // The memo is a pure function of (registrations, version);
            // both travel with the clone, so the cached list stays
            // valid.
            descriptor_cache: parking_lot::Mutex::new(
                self.descriptor_cache.lock().clone(),
            ),
            snapshot_cache: parking_lot::Mutex::new(
                self.snapshot_cache.lock().clone(),
            ),
            validate_arguments: self.validate_arguments,
        }
    }
}

#[cfg(test)]
mod tests;
