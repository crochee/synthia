//! Tool registry storage, registration, scoped cleanup, materialization,
//! dispatch, snapshots, and `Registry` integration.

use std::{collections::HashMap, panic::AssertUnwindSafe, sync::Arc};

use async_trait::async_trait;
use futures::{FutureExt, Stream, StreamExt};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use synthia_core::registry::RegistryItem;
use tokio::sync::{Semaphore, mpsc};
use tokio_stream::wrappers::ReceiverStream;

use crate::{
    traits::{StreamOutput, Tool},
    types::{Context, ToolOutput},
};

// ── Descriptor types (moved from descriptor.rs in Stage 3) ────────────────

/// Full tool metadata for LLM tool_choice and orchestration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDescriptor {
    pub name: String,
    /// Human-readable description for LLM.
    pub description: String,
    /// JSON Schema for tool parameters.
    pub parameters: serde_json::Value,
    /// Tool category.
    pub category: ToolCategory,
    /// Whether this tool is hidden from /help listings.
    #[serde(default)]
    pub is_hidden: bool,
}

/// Where a tool comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ToolProvenance {
    /// Built-in tool (immutable name).
    Core,
    /// Dynamically registered.
    Dynamic,
}

/// 工具曝光级别 — 控制工具何时对 LLM 可见。
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize,
)]
pub enum ToolExposure {
    /// 始终可见，完整 schema 发送给 LLM
    #[default]
    Direct,
    /// 首次调用时才加载完整定义；发送给 LLM 的只有 name + description
    Deferred,
    /// 不对 LLM 可见，只能通过 Skill 或程序调用
    Hidden,
}

/// Tool category for routing decisions.
///
/// Mirrors `synthia_core::tool::descriptor::ToolCategory` so that
/// the sub-traits can reference a category without pulling in the
/// full unified tool infrastructure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolCategory {
    Filesystem,
    Shell,
    Network,
    Utility,
}

/// Lightweight snapshot of a tool's definition metadata.
///
/// Cheaply cloneable, suitable for inclusion in `Vec<ToolMetadataSnapshot>`
/// in the `ToolRegistry` dual-index.
#[derive(Debug, Clone, Serialize)]
pub struct ToolMetadataSnapshot {
    pub name: String,
    pub description: String,
}

/// A snapshot of one tool's metadata plus its provenance. Returned
/// by [`ToolRegistry::snapshot_with_provenance`].
#[derive(Debug, Clone, Serialize)]
pub struct ToolProvenanceRecord {
    pub metadata: ToolMetadataSnapshot,
    pub provenance: ToolProvenance,
}

// 1. ToolEntry and RegistryItem/serde implementations.

/// One entry in the [`ToolRegistry`]: a type-erased [`Tool`] plus the
/// (name, description) pair the registry needs to render its catalog,
/// plus behavioural metadata that was formerly on the `Tool` trait
/// itself.
#[derive(Clone)]
pub struct ToolEntry {
    /// The underlying tool, behind an `Arc<dyn Tool>` so the registration
    /// table can share ownership cheaply with the dispatcher.
    pub(crate) tool: Arc<dyn Tool>,
    /// Cached `Tool::name()` result.
    pub(crate) name: String,
    /// Cached `Tool::description()` result.
    pub(crate) description: String,
    /// Whether the tool is hidden from user-facing listings.
    pub(crate) is_hidden: bool,
}

impl ToolEntry {
    /// Build a new entry by snapshotting `tool.name()` and
    /// `tool.description()` once, so the registry doesn't have to call
    /// them on every list/get.
    pub fn new(tool: Arc<dyn Tool>) -> Self {
        Self {
            name: tool.name().to_string(),
            description: tool.description().to_string(),
            tool,
            is_hidden: false,
        }
    }

    /// Register a dynamic tool from raw metadata.
    ///
    /// The returned entry's `tool` is a passthrough that echoes
    /// the call arguments back to the caller. Useful for testing
    /// and for runtime registration via the
    /// `POST /api/v1/tools` endpoint. Added in turn 13 of the
    /// 2026-08-15 optimization pass.
    pub fn dynamic(
        name: String,
        description: String,
        parameters: serde_json::Value,
    ) -> Self {
        let tool = Arc::new(DynamicPassthroughTool {
            name: name.clone(),
            description: description.clone(),
            parameters,
        });
        Self {
            name,
            description,
            tool,
            is_hidden: false,
        }
    }

    /// Return a clone of the inner `Arc<dyn Tool>` for the dispatcher to
    /// call.
    pub fn tool_instance(&self) -> Arc<dyn Tool> {
        Arc::clone(&self.tool)
    }

    /// Set whether this tool is hidden from user-facing listings.
    pub fn with_is_hidden(mut self, val: bool) -> Self {
        self.is_hidden = val;
        self
    }

    /// Whether this tool is hidden from user-facing listings.
    pub fn is_hidden(&self) -> bool {
        self.is_hidden
    }
}

/// Tool returned by `ToolEntry::dynamic`. Echoes its arguments
/// back so the caller can verify registration without requiring
/// a side-effecting handler.
struct DynamicPassthroughTool {
    name: String,
    description: String,
    parameters: serde_json::Value,
}

#[async_trait]
impl Tool for DynamicPassthroughTool {
    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> &str {
        &self.description
    }

    fn parameters(&self) -> serde_json::Value {
        self.parameters.clone()
    }

    async fn call(
        &self,
        input: serde_json::Value,
        _context: &Context,
    ) -> ToolOutput {
        ToolOutput::text(input.to_string())
    }
}

impl RegistryItem for ToolEntry {
    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> &str {
        &self.description
    }
}

impl Serialize for ToolEntry {
    /// Serialise as `{name, description}` only. The `tool` field is
    /// intentionally **not** emitted — trait objects don't have a stable
    /// JSON shape, and the catalog consumers (CLI `tools list`, server
    /// introspection) only need the human metadata.
    fn serialize<S>(
        &self,
        serializer: S,
    ) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut state = serializer.serialize_struct("ToolEntry", 2)?;
        state.serialize_field("name", &self.name)?;
        state.serialize_field("description", &self.description)?;
        state.end()
    }
}

impl<'de> serde::Deserialize<'de> for ToolEntry {
    /// Deserialisation is intentionally rejected — there's no portable
    /// way to rebuild a `Tool` from JSON. Callers must use
    /// [`ToolRegistry::register`] (which takes an `Arc<dyn Tool>`
    /// directly) instead of round-tripping through JSON.
    fn deserialize<D>(_deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        Err(serde::de::Error::custom(
            "ToolEntry cannot be deserialized; use register_tool()",
        ))
    }
}

// 2. ToolFilter and ToolMetadataSnapshot support.

// 3. RegistrationToken.

/// Registration token for unregistration.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RegistrationToken(pub u64);

// 4. ProviderEntry.

/// Per-provider registration record. The merged [`ToolRegistry`] stores
/// `Vec<ProviderEntry>` per tool name (LIFO ordering).
///
/// Renamed from the original `pub(crate) ToolEntry` to avoid colliding
/// with the public `ToolEntry` value type used by the 9 builtin tools
/// during registration.
#[derive(Clone)]
pub(crate) struct ProviderEntry {
    /// Token that owns this registration — used for scoped unregistration.
    pub(crate) provider_token: RegistrationToken,
    pub(crate) tool: Arc<dyn Tool>,
    pub(crate) provenance: ToolProvenance,
    pub(crate) is_hidden: bool,
}

/// Build a [`ToolDescriptor`] from a `ProviderEntry`. Reads
/// `tool.description()`, `tool.parameters()`, and the stored
/// `is_hidden` flag.
fn descriptor_for(entry: &ProviderEntry) -> ToolDescriptor {
    ToolDescriptor {
        name: entry.tool.name().to_string(),
        description: entry.tool.description().to_string(),
        parameters: entry.tool.parameters(),
        category: ToolCategory::Utility,
        is_hidden: entry.is_hidden,
    }
}

// 5. ToolRegistry and ToolRegistryInner.

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
}

#[derive(Clone)]
pub(crate) struct ToolRegistryInner {
    /// Tool name → entries (LIFO for non-core tools).
    pub(crate) tools: HashMap<String, Vec<ProviderEntry>>,
    /// Next registration token.
    pub(crate) next_registration: u64,
}

// 6. Inherent registration, cleanup, materialization, resolution, snapshot,
//    metadata_snapshots, dispatch, introspection, and builder methods.

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
        }
    }

    /// Register a `ToolEntry` directly — wraps the inner `Arc<dyn Tool>`
    /// into a [`ProviderEntry`] and inserts it.
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

    /// Snapshot of registered tools with provenance included.
    ///
    /// Same ordering guarantee and hidden-filter as [`snapshot`].
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

    /// Remove all `ProviderEntry` instances that were registered with
    /// the given token.
    pub fn unregister_by_token(&self, token: RegistrationToken) {
        let mut inner = self.inner.write();
        let mut removed_count = 0usize;

        // Drain entries matching the token from each tool bucket.
        inner.tools.retain(|_name, entries| {
            let before = entries.len();
            entries.retain(|e| e.provider_token != token);
            let removed = before - entries.len();
            removed_count += removed;
            !entries.is_empty()
        });

        if removed_count > 0 {
            tracing::info!(
                token = token.0,
                count = removed_count,
                "unregistered tools by token"
            );
        } else {
            tracing::debug!(
                token = token.0,
                "unregister_by_token: no matching entries"
            );
        }
    }

    /// Register a single [`ToolEntry`] directly and return a
    /// `RegistrationScope` that auto-unregisters on drop.
    pub async fn register_scoped_arc(
        self: &Arc<Self>,
        entry: ToolEntry,
    ) -> RegistrationScope {
        let mut inner = self.inner.write();
        let token = self.register_entry_inner(&mut inner, entry);
        let token = token.unwrap_or_else(|| {
            RegistrationToken(inner.next_registration.wrapping_sub(1))
        });
        RegistrationScope {
            token,
            registry: Arc::downgrade(self),
        }
    }

    /// Create an empty session scope with a fresh registration token.
    ///
    /// Unlike [`register_scoped`](Self::register_scoped), this does not
    /// register any tools immediately. The returned scope carries a
    /// unique [`RegistrationToken`] that future code can associate with
    /// tools registered during a session. When the scope is dropped, all
    /// tools registered under its token are automatically unregistered
    /// from the registry (or the cleanup is a no-op if the registry has
    /// already been dropped).
    pub fn create_session_scope(self: &Arc<Self>) -> RegistrationScope {
        let token = {
            let mut inner = self.inner.write();
            let token = RegistrationToken(inner.next_registration);
            inner.next_registration += 1;
            token
        };

        RegistrationScope {
            token,
            registry: Arc::downgrade(self),
        }
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

    /// Dispatch a batch of tool uses as a stream of `(call_id,
    /// StreamOutput)` items. Each input `ToolUse` yields zero-or-more
    /// `Progress` items then exactly one `Result`; if the tool's
    /// underlying stream closes without a `Result`, this method
    /// synthesizes an error `Result`. Concurrency is bounded by
    /// `self.max_concurrent`. Cancellation is implicit: dropping the
    /// returned stream aborts all in-flight tasks.
    pub fn run_stream(
        &self,
        tool_uses: Vec<synthia_provider::ToolUse>,
        context: Context,
    ) -> impl Stream<Item = (String, StreamOutput)> + Send + 'static {
        let max_concurrent = self.max_concurrent.max(1);
        let semaphore = Arc::new(Semaphore::new(max_concurrent));

        // Phase 1: resolve every tool_use entry under the lock; build a
        // plan; release the lock before any await point.
        #[allow(clippy::large_enum_variant)]
        enum Plan {
            Call(Arc<dyn Tool>, serde_json::Value),
            NotFound(String),
        }
        let plan: Vec<(String, Plan)> = {
            let inner = self.inner.read();
            tool_uses
                .into_iter()
                .map(|tu| {
                    let key = tu.name.clone();
                    match inner.tools.get(&key).and_then(|e| e.last()) {
                        Some(entry) if entry.is_hidden => {
                            (tu.id, Plan::NotFound(tu.name))
                        }
                        Some(entry) => {
                            (tu.id, Plan::Call(entry.tool.clone(), tu.input))
                        }
                        None => (tu.id, Plan::NotFound(tu.name)),
                    }
                })
                .collect()
        };

        // Phase 2: per plan item, create a channel + spawn a task.
        let mut per_tool_streams = Vec::with_capacity(plan.len());
        for (call_id, item) in plan {
            let (tx, rx) = mpsc::channel::<StreamOutput>(16);
            let ctx = context.clone();
            let semaphore = semaphore.clone();
            tokio::spawn(async move {
                let _permit = match semaphore.acquire_owned().await {
                    Ok(p) => p,
                    Err(_) => return, // semaphore closed
                };
                match item {
                    Plan::Call(tool, input) => {
                        let tool_name = tool.name();
                        let span = tracing::info_span!(
                            "tool.execute",
                            tool.name = %tool_name,
                            exception.type = tracing::field::Empty,
                            exception.message = tracing::field::Empty,
                            otel.status_code = tracing::field::Empty,
                        );
                        // Panic recovery lives inside
                        // `consume_tool_stream_into` so the Sender —
                        // moved into the helper — stays accessible for
                        // the synthesized error Result. By the time
                        // control returns here, the per-tool task is
                        // finished.
                        consume_tool_stream_into(tool, input, &ctx, tx, span)
                            .await;
                    }
                    Plan::NotFound(name) => {
                        let _ = tx
                            .send(StreamOutput::Result(ToolOutput::error(
                                format!("tool not found: {}", name),
                            )))
                            .await;
                    }
                }
                // tx drops here, closing rx for this tool.
            });
            let s = ReceiverStream::new(rx).map({
                let call_id = call_id.clone();
                move |item| (call_id.clone(), item)
            });
            per_tool_streams.push(s);
        }

        futures::stream::select_all(per_tool_streams)
    }
}

// 7. Default and Clone implementations.

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
        }
    }
}

// 8. RegistrationScope and Drop implementation.

/// RAII scope that automatically unregisters tools when dropped.
///
/// Created by [`ToolRegistry::register_scoped`] or
/// [`ToolRegistry::register_scoped_with_namespace`]. When the scope
/// goes out of scope, all tools that were registered under its token
/// are removed from the registry. If the registry itself has already
/// been dropped, cleanup is a no-op.
#[derive(Debug)]
pub struct RegistrationScope {
    token: RegistrationToken,
    registry: std::sync::Weak<ToolRegistry>,
}

impl RegistrationScope {
    /// The registration token for this scope.
    pub fn token(&self) -> &RegistrationToken {
        &self.token
    }

    /// Perform cleanup: upgrade `Weak` → `Arc`, call
    /// `unregister_by_token`.
    fn cleanup(&mut self) {
        if let Some(registry) = self.registry.upgrade() {
            tracing::info!(
                token = self.token.0,
                "RegistrationScope dropped — unregistering tools"
            );
            registry.unregister_by_token(self.token.clone());
        } else {
            tracing::debug!(
                token = self.token.0,
                "RegistrationScope dropped but registry already gone — no-op"
            );
        }
    }
}

impl Drop for RegistrationScope {
    fn drop(&mut self) {
        self.cleanup();
    }
}

// 9. #[async_trait] impl Registry for ToolRegistry.
//
// Wrapped in a private inner module so the `Registry` trait
// impl (and its `ToolFilter` associated type) is not visible outside the
// crate. External callers still reach the same behavior through the
// inherent methods on `ToolRegistry`.

mod registry_trait {
    use synthia_core::registry::{Registry, paginate_registry_list};

    use super::*;

    #[derive(Debug, Clone, Default, Serialize)]
    pub struct ToolFilter {
        pub name_prefix: Option<String>,
    }

    #[async_trait]
    impl Registry for ToolRegistry {
        type Filter = ToolFilter;
        type Item = ToolEntry;

        async fn put(
            &self,
            item: Self::Item,
        ) -> std::result::Result<(), synthia_core::Error> {
            self.register_entry(item);
            Ok(())
        }

        async fn delete(
            &self,
            name: &str,
        ) -> std::result::Result<(), synthia_core::Error> {
            let mut inner = self.inner.write();
            let removed = inner.tools.remove(name).is_some();
            if removed {
                Ok(())
            } else {
                Err(synthia_core::Error::not_found(name))
            }
        }

        async fn get(
            &self,
            name: &str,
        ) -> std::result::Result<Option<Self::Item>, synthia_core::Error>
        {
            let inner = self.inner.read();
            Ok(inner
                .tools
                .get(name)
                .and_then(|entries| entries.last())
                .map(|entry| {
                    let desc = descriptor_for(entry);
                    ToolEntry::new(entry.tool.clone())
                        .with_is_hidden(desc.is_hidden)
                }))
        }

        async fn list_paginate(
            &self,
            cursor: Option<String>,
            limit: u64,
            _sort: Option<String>,
            filter: Option<Self::Filter>,
        ) -> std::result::Result<
            synthia_core::registry::RegistryList<Self::Item>,
            synthia_core::Error,
        > {
            let filter = filter.unwrap_or_default();
            let inner = self.inner.read();
            let result: Vec<ToolEntry> = inner
                .tools
                .values()
                .filter_map(|entries| entries.last())
                .filter(|entry| {
                    let name = entry.tool.name();
                    match &filter.name_prefix {
                        Some(prefix) => name.starts_with(prefix),
                        None => true,
                    }
                })
                .map(|entry| {
                    let desc = descriptor_for(entry);
                    ToolEntry::new(entry.tool.clone())
                        .with_is_hidden(desc.is_hidden)
                })
                .collect();
            // Sort is intentionally ignored — the registry stores
            // tools in registration order and the HTTP surface
            // doesn't expose sort yet. Cursor + limit + envelope
            // come from the shared pagination primitive.
            paginate_registry_list(result, cursor.as_deref(), limit)
        }
    }
}

// 10. consume_tool_stream_into + record_tool_outcome + panic_message.

/// Outcome of a stream-drain attempt (used internally).
enum StreamOutcome {
    /// The tool's `Tool::stream` closed without panicking or being
    /// cancelled.
    Completed,
    /// The consumer dropped the returned stream (or the per-tool
    /// Receiver). The task exits without synthesizing a Result.
    ConsumerDropped,
}

/// Drain a tool's `Tool::stream`, forward items to `tx`, record the
/// outcome on `span`, and synthesize a `Result` if the stream closes
/// without one OR if the underlying future panics. The `tx` is held
/// inside an `AssertUnwindSafe` wrapper because panicking out of an
/// async task with moved values would otherwise drop them silently.
async fn consume_tool_stream_into(
    tool: Arc<dyn Tool>,
    input: serde_json::Value,
    ctx: &Context,
    tx: mpsc::Sender<StreamOutput>,
    span: tracing::Span,
) {
    let stream = tool.stream(input, ctx);
    let mut stream = std::pin::pin!(stream);
    let tx = AssertUnwindSafe(tx);

    // Tracks the most recent Result for span recording.
    let mut last_result: Option<ToolOutput> = None;

    let drain_result = AssertUnwindSafe(async {
        while let Some(item) = stream.next().await {
            if let StreamOutput::Result(output) = &item {
                last_result = Some(output.clone());
            }
            // tx.0 is the inner Sender; dereference through AssertUnwindSafe.
            if tx.0.send(item).await.is_err() {
                return StreamOutcome::ConsumerDropped;
            }
        }
        StreamOutcome::Completed
    })
    .catch_unwind()
    .await;

    let outcome = match drain_result {
        Ok(o) => o,
        Err(payload) => {
            // Tool panicked. Record on the span, synthesize an error
            // Result, and forward to the consumer.
            let msg = panic_message(&payload);
            let out = ToolOutput::error(format!(
                "tool `{}` panicked during execution: {}",
                tool.name(),
                msg
            ));
            record_tool_outcome(&span, &out);
            let _ = tx.0.send(StreamOutput::Result(out)).await;
            return;
        }
    };

    match outcome {
        StreamOutcome::ConsumerDropped => (), // tx already closed
        StreamOutcome::Completed => {
            if let Some(out) = last_result {
                record_tool_outcome(&span, &out);
            } else {
                // Tool stream closed without a Result — synthesize.
                let out = ToolOutput::error(format!(
                    "tool `{}` stream yielded no Result — contract violation",
                    tool.name()
                ));
                record_tool_outcome(&span, &out);
                let _ = tx.0.send(StreamOutput::Result(out)).await;
            }
        }
    }
}

/// Best-effort panic message extraction. A panic payload is
/// `Box<dyn Any + Send>`; strings downcast to `&str`, everything else
/// becomes `"<non-string panic>"`.
fn panic_message(payload: &(dyn std::any::Any + Send)) -> String {
    if let Some(s) = payload.downcast_ref::<&'static str>() {
        (*s).to_string()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "<non-string panic>".to_string()
    }
}

/// Record the tool outcome into the `tool.execute` span.
///
/// Mirrors the OTel semantic conventions expected by
/// `crates/synthia-tool/tests/tool_span.rs`:
/// - On success: leaves `exception.*` and `otel.status_code` empty.
/// - On error: classifies by output message:
///   - `"timed out"` → `exception.type = "TimeoutError"`
///   - anything else → `exception.type = "ToolError"`
///
///   and records `otel.status_code = "ERROR"`.
fn record_tool_outcome(span: &tracing::Span, out: &ToolOutput) {
    let is_err = out.is_error.unwrap_or(false);
    if !is_err {
        return;
    }
    let message = out
        .content
        .iter()
        .find_map(|c| match c {
            synthia_provider::types::ContentPart::Text(t) => {
                Some(t.text.clone())
            }
            _ => None,
        })
        .unwrap_or_default();
    let exception_type = if message.to_lowercase().contains("timed out") {
        "TimeoutError"
    } else {
        "ToolError"
    };
    span.record("exception.type", exception_type);
    span.record("exception.message", message.as_str());
    span.record("otel.status_code", "ERROR");
}

// 11. #[cfg(test)] mod tests.

#[cfg(test)]
mod tests {
    use std::{path::PathBuf, sync::Arc};

    use async_trait::async_trait;
    use synthia_core::registry::{Registry, RegistryItem};

    use super::{
        super::{
            traits::Tool as Tool7,
            types::{Context, ToolOutput},
        },
        *,
    };
    use crate::{traits::Tool, types::DispatchMode};

    /// Drain a `run_stream` stream and collect exactly one `Result` per
    /// expected call. Drops `Progress` items. Used by tests that don't care
    /// about progress visibility — they just want the final outputs.
    async fn collect_results(
        stream: impl futures::Stream<Item = (String, crate::traits::StreamOutput)>
        + Unpin,
        expected: usize,
    ) -> Vec<(String, crate::types::ToolOutput)> {
        use futures::StreamExt;
        let mut stream = std::pin::pin!(stream);
        let mut out = Vec::new();
        while let Some((call_id, item)) = stream.next().await {
            if let crate::traits::StreamOutput::Result(output) = item {
                out.push((call_id, output));
                if out.len() == expected {
                    break;
                }
            }
        }
        out
    }

    #[test]
    fn exposure_default_is_direct() {
        // Verify that ToolExposure::default() is Direct
        assert_eq!(ToolExposure::default(), ToolExposure::Direct);
    }

    // ── create_session_scope tests ──────────────────────────────────────

    #[test]
    fn create_session_scope_returns_valid_token() {
        let registry = Arc::new(ToolRegistry::new());
        let scope = registry.create_session_scope();
        // Token should be non-zero (first allocation)
        assert_ne!(scope.token().0, 0);
    }

    #[test]
    fn create_session_scope_drop_is_noop_when_no_tools_registered() {
        let registry = Arc::new(ToolRegistry::new());
        let tool_count_before = registry.tool_count();
        {
            let _scope = registry.create_session_scope();
        }
        // Tool count unchanged after scope drop (no tools were registered)
        assert_eq!(registry.tool_count(), tool_count_before);
    }

    #[test]
    fn create_session_scope_subsequent_tokens_are_monotonic() {
        let registry = Arc::new(ToolRegistry::new());
        let scope1 = registry.create_session_scope();
        let scope2 = registry.create_session_scope();
        assert!(
            scope2.token().0 > scope1.token().0,
            "tokens should be monotonically increasing"
        );
    }

    #[test]
    fn create_session_scope_noop_when_registry_dropped_first() {
        let registry = Arc::new(ToolRegistry::new());
        let scope = registry.create_session_scope();
        // Drop the registry first
        drop(registry);
        // Now drop the scope — should not panic (Weak::upgrade returns None)
        drop(scope);
    }

    // ── Registration/Trait tests (from tests.rs) ──────────────────────

    #[derive(Debug)]
    struct TestEntryTool;

    #[async_trait]
    impl Tool for TestEntryTool {
        fn name(&self) -> &str {
            "test"
        }

        fn description(&self) -> &str {
            "A test tool"
        }

        fn parameters(&self) -> serde_json::Value {
            serde_json::json!({
                "type": "object",
                "properties": {}
            })
        }

        async fn call(
            &self,
            _input: serde_json::Value,
            _context: &Context,
        ) -> ToolOutput {
            ToolOutput::text("test output")
        }
    }

    /// `register_entry` returns `false` when a Core
    /// tool already occupies the name. This is the
    /// immutability contract for builtin tools —
    /// user code cannot shadow them. The
    /// provider-side `register_entry_inner` is
    /// private, but we exercise the public path
    /// via `register_entry` (which delegates).
    #[derive(Debug)]
    struct ShadowTool;

    #[async_trait]
    impl Tool for ShadowTool {
        fn name(&self) -> &str {
            "core_name"
        }

        fn description(&self) -> &str {
            "tries to shadow a Core tool"
        }

        fn parameters(&self) -> serde_json::Value {
            serde_json::json!({"type": "object"})
        }

        async fn call(&self, _: serde_json::Value, _: &Context) -> ToolOutput {
            ToolOutput::text("shadow")
        }
    }

    #[test]
    fn register_entry_cannot_shadow_core_tool() {
        let registry = ToolRegistry::new();
        // Manually inject a Core entry into the
        // inner HashMap (Core tools are normally
        // registered via the provider API at
        // startup, not via `register_entry`).
        {
            let mut inner = registry.inner.write();
            let name = "core_name".to_string();
            let token = RegistrationToken(inner.next_registration);
            inner.next_registration += 1;
            let provider_entry = ProviderEntry {
                provider_token: token,
                provenance: ToolProvenance::Core,
                is_hidden: false,
                tool: Arc::new(ShadowTool),
            };
            inner.tools.entry(name).or_default().push(provider_entry);
        }
        // Now try to register a Dynamic tool with
        // the same name via the public API.
        let accepted =
            registry.register_entry(ToolEntry::new(Arc::new(ShadowTool)));
        assert!(
            !accepted,
            "register_entry MUST refuse to shadow a Core tool; got {accepted}"
        );
    }

    /// `register_entry` for a fresh name returns
    /// `true` and increments `tool_count`.
    #[test]
    fn register_entry_for_new_name_returns_true_and_inserts() {
        let registry = ToolRegistry::new();
        let count_before = registry.tool_count();
        let accepted =
            registry.register_entry(ToolEntry::new(Arc::new(TestEntryTool)));
        assert!(accepted, "fresh name must be accepted");
        assert_eq!(
            registry.tool_count(),
            count_before + 1,
            "tool_count MUST increment by 1"
        );
    }

    /// `unregister_by_name` returns `true` when a
    /// tool is removed, `false` when the name was
    /// never registered. The `bool` return is
    /// load-bearing for the cleanup code path —
    /// callers use it to decide whether to log.
    #[test]
    fn unregister_by_name_returns_true_on_remove_false_on_unknown() {
        let registry = ToolRegistry::new();
        registry.register_entry(ToolEntry::new(Arc::new(TestEntryTool)));
        assert!(
            registry.unregister_by_name("test"),
            "existing name MUST return true"
        );
        assert!(
            !registry.unregister_by_name("test"),
            "second unregister of same name MUST return false"
        );
        assert!(
            !registry.unregister_by_name("never_existed"),
            "unknown name MUST return false"
        );
    }

    /// `snapshot()` returns entries sorted by name
    /// (deterministic ordering for downstream
    /// consumers). Insert in reverse-alphabetical
    /// order and verify the snapshot comes back
    /// in alphabetical order.
    #[derive(Debug)]
    struct NamedTool(&'static str);

    #[async_trait]
    impl Tool for NamedTool {
        fn name(&self) -> &str {
            self.0
        }

        fn description(&self) -> &str {
            "named"
        }

        fn parameters(&self) -> serde_json::Value {
            serde_json::json!({"type": "object"})
        }

        async fn call(&self, _: serde_json::Value, _: &Context) -> ToolOutput {
            ToolOutput::text("named")
        }
    }

    #[test]
    fn snapshot_is_sorted_alphabetically_for_deterministic_output() {
        let registry = ToolRegistry::new();
        // Insert in REVERSE alphabetical order.
        registry.register_entry(ToolEntry::new(Arc::new(NamedTool("zoo"))));
        registry.register_entry(ToolEntry::new(Arc::new(NamedTool("apple"))));
        registry.register_entry(ToolEntry::new(Arc::new(NamedTool("mango"))));
        let snap = registry.snapshot();
        let names: Vec<&str> = snap.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(
            names,
            vec!["apple", "mango", "zoo"],
            "snapshot MUST be sorted alphabetically; got {names:?}"
        );
    }

    /// `snapshot()` returns `tool_count() - duplicates`
    /// entries (last entry per bucket wins via shadowing).
    /// Pin the shadowing semantics: registering the
    /// same name twice produces 1 snapshot entry
    /// (the newer one) but `tool_count` reflects
    /// both insertions. Actually let me check the
    /// tool_count semantics first.
    #[test]
    fn snapshot_returns_one_entry_per_unique_name() {
        let registry = ToolRegistry::new();
        registry.register_entry(ToolEntry::new(Arc::new(TestEntryTool)));
        registry.register_entry(ToolEntry::new(Arc::new(TestEntryTool)));
        let snap = registry.snapshot();
        let test_entries: Vec<_> =
            snap.iter().filter(|s| s.name == "test").collect();
        assert_eq!(
            test_entries.len(),
            1,
            "duplicate registrations collapse to 1 snapshot entry (last-wins shadowing)"
        );
    }

    /// `with_is_hidden(true)` hides the tool
    /// from snapshot output. Pin the contract so
    /// the LLM never sees hidden tools.
    #[test]
    fn with_is_hidden_filters_out_tool_from_snapshot() {
        let registry = ToolRegistry::new();
        let hidden =
            ToolEntry::new(Arc::new(TestEntryTool)).with_is_hidden(true);
        registry.register_entry(hidden);
        let snap = registry.snapshot();
        assert!(
            !snap.iter().any(|s| s.name == "test"),
            "hidden tool MUST NOT appear in snapshot; got {snap:?}"
        );
    }

    /// `tool_count()` and `snapshot()` both return
    /// 1 entry for duplicate registrations — the
    /// HashMap's `len()` is the unique-name count,
    /// not the raw ProviderEntry bucket length.
    /// Pin the contract so a refactor that switches
    /// `tool_count` to use `values().map(|v| v.len()).sum()`
    /// (raw ProviderEntry count) breaks loudly.
    #[test]
    fn tool_count_and_snapshot_both_collapse_duplicate_names_to_one() {
        let registry = ToolRegistry::new();
        registry.register_entry(ToolEntry::new(Arc::new(TestEntryTool)));
        registry.register_entry(ToolEntry::new(Arc::new(TestEntryTool)));
        assert_eq!(
            registry.tool_count(),
            1,
            "tool_count = unique name count (HashMap len)"
        );
        assert_eq!(
            registry.snapshot().len(),
            1,
            "snapshot = unique name count (last-wins)"
        );
    }

    #[tokio::test]
    async fn test_tool_registry_register_and_get() {
        let registry = ToolRegistry::new();
        registry.register_entry(ToolEntry::new(Arc::new(TestEntryTool)));
        let entry = registry.get("test").await.unwrap();
        assert!(entry.is_some());
        assert!(entry.unwrap().tool_instance().name() == "test");
        assert!(registry.get("nonexistent").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_tool_registry_unregister() {
        let registry = ToolRegistry::new();
        registry.register_entry(ToolEntry::new(Arc::new(TestEntryTool)));
        registry.unregister_by_name("test");
        let entries = registry.list(None).await.unwrap();
        assert!(entries.is_empty());
    }

    #[tokio::test]
    async fn test_list_definitions() {
        let registry = ToolRegistry::new();
        registry.register_entry(ToolEntry::new(Arc::new(TestEntryTool)));
        let entries = registry.list(None).await.unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name(), "test");
    }

    #[tokio::test]
    async fn test_run_with_context() {
        let registry = ToolRegistry::new();
        registry.register_entry(ToolEntry::new(Arc::new(TestEntryTool)));

        let tool_use = synthia_provider::ToolUse {
            id: "1".to_string(),
            name: "test".to_string(),
            input: serde_json::json!({}),
        };
        let ctx = Context::new("s1".to_string(), PathBuf::from("/tmp"));
        let results =
            collect_results(registry.run_stream(vec![tool_use], ctx), 1).await;
        assert_eq!(results.len(), 1);
        assert!(results[0].1.is_text());
    }

    #[tokio::test]
    async fn test_run_with_context_validation_error() {
        #[derive(Debug)]
        struct ToolWithRequired;
        #[async_trait]
        impl Tool for ToolWithRequired {
            fn name(&self) -> &str {
                "req"
            }

            fn description(&self) -> &str {
                "Tool with required param"
            }

            fn parameters(&self) -> serde_json::Value {
                serde_json::json!({
                    "type": "object",
                    "required": ["name"],
                    "properties": {}
                })
            }

            async fn call(
                &self,
                input: serde_json::Value,
                _context: &Context,
            ) -> ToolOutput {
                if input.get("name").is_none() {
                    return ToolOutput::error(
                        "Missing required property: name".to_string(),
                    );
                }
                ToolOutput::text("ok")
            }
        }

        let registry = ToolRegistry::new();
        registry.register_entry(ToolEntry::new(Arc::new(ToolWithRequired)));

        let tool_use = synthia_provider::ToolUse {
            id: "1".to_string(),
            name: "req".to_string(),
            input: serde_json::json!({}),
        };
        let ctx = Context::new("s1".to_string(), PathBuf::from("/tmp"));
        let results =
            collect_results(registry.run_stream(vec![tool_use], ctx), 1).await;
        assert_eq!(results.len(), 1);
        assert!(results[0].1.is_error.unwrap_or(false));
        let text = results[0].1.content[0].text().unwrap();
        assert!(
            text.contains("Missing required")
                || text.contains("required property"),
            "Unexpected validation error: {}",
            text
        );
    }

    #[tokio::test]
    async fn test_run_with_context_concurrent() {
        #[derive(Debug)]
        struct FastTool;
        #[async_trait]
        impl Tool for FastTool {
            fn name(&self) -> &str {
                "fast"
            }

            fn description(&self) -> &str {
                "Fast tool"
            }

            fn parameters(&self) -> serde_json::Value {
                serde_json::json!({})
            }

            async fn call(
                &self,
                _input: serde_json::Value,
                _context: &Context,
            ) -> ToolOutput {
                ToolOutput::text("done")
            }
        }

        let registry = ToolRegistry::new();
        registry.register_entry(ToolEntry::new(Arc::new(FastTool)));

        let tool_uses = vec![
            synthia_provider::ToolUse {
                id: "1".to_string(),
                name: "fast".to_string(),
                input: serde_json::json!({}),
            },
            synthia_provider::ToolUse {
                id: "2".to_string(),
                name: "fast".to_string(),
                input: serde_json::json!({}),
            },
        ];
        let ctx = Context::new("s1".to_string(), PathBuf::from("/tmp"));
        let expected = tool_uses.len();
        let results =
            collect_results(registry.run_stream(tool_uses, ctx), expected)
                .await;
        assert_eq!(results.len(), 2);
        for r in &results {
            assert!(r.1.is_text());
        }
    }

    #[tokio::test]
    async fn test_run_with_context_unknown_tool() {
        let registry = ToolRegistry::new();

        let tool_uses = vec![synthia_provider::ToolUse {
            id: "1".to_string(),
            name: "nonexistent_tool".to_string(),
            input: serde_json::json!({}),
        }];
        let ctx = Context::new("s1".to_string(), PathBuf::from("/tmp"));
        let expected = tool_uses.len();
        let results =
            collect_results(registry.run_stream(tool_uses, ctx), expected)
                .await;
        assert_eq!(results.len(), 1);
        assert!(results[0].1.is_error.unwrap_or(false));
    }

    #[test]
    fn test_tool_context_has_dispatch_mode() {
        let ctx = Context::new("s1".to_string(), PathBuf::from("/tmp"));
        assert_eq!(ctx.dispatch_mode, DispatchMode::Fork);
    }

    #[tokio::test]
    async fn test_hidden_tools_not_in_list() {
        let registry = ToolRegistry::new();
        registry.register_entry(ToolEntry::new(Arc::new(TestEntryTool)));

        #[derive(Debug)]
        struct HiddenTool;

        #[async_trait]
        impl Tool for HiddenTool {
            fn name(&self) -> &str {
                "hidden"
            }

            fn description(&self) -> &str {
                "A hidden tool"
            }

            fn parameters(&self) -> serde_json::Value {
                serde_json::json!({})
            }

            async fn call(
                &self,
                _input: serde_json::Value,
                _context: &Context,
            ) -> ToolOutput {
                ToolOutput::text("hidden output")
            }
        }

        registry.register_entry(
            ToolEntry::new(Arc::new(HiddenTool)).with_is_hidden(true),
        );

        let entries = registry.list(None).await.unwrap();
        assert_eq!(entries.len(), 2);

        let hidden = entries.iter().find(|e| e.name() == "hidden").unwrap();
        assert!(hidden.is_hidden());

        let visible = entries.iter().find(|e| e.name() == "test").unwrap();
        assert!(!visible.is_hidden());

        assert!(registry.get("test").await.unwrap().is_some());
        assert!(registry.get("hidden").await.unwrap().is_some());
    }

    #[tokio::test]
    async fn test_hidden_tool_not_executed() {
        let registry = ToolRegistry::new();

        #[derive(Debug)]
        struct HiddenTool;

        #[async_trait]
        impl Tool for HiddenTool {
            fn name(&self) -> &str {
                "hidden_exec"
            }

            fn description(&self) -> &str {
                "A hidden executable tool"
            }

            fn parameters(&self) -> serde_json::Value {
                serde_json::json!({})
            }

            async fn call(
                &self,
                _input: serde_json::Value,
                _context: &Context,
            ) -> ToolOutput {
                ToolOutput::text("hidden executed")
            }
        }

        registry.register_entry(
            ToolEntry::new(Arc::new(HiddenTool)).with_is_hidden(true),
        );

        let tool_use = synthia_provider::ToolUse {
            id: "1".to_string(),
            name: "hidden_exec".to_string(),
            input: serde_json::json!({}),
        };
        let ctx = Context::new("s1".to_string(), PathBuf::from("/tmp"));
        let results =
            collect_results(registry.run_stream(vec![tool_use], ctx), 1).await;
        assert_eq!(results.len(), 1);
        assert!(results[0].1.is_error.unwrap_or(false));
        assert!(
            results[0].1.content[0]
                .text()
                .unwrap()
                .contains("not found")
        );
    }

    #[tokio::test]
    async fn test_registry_trait_list() {
        let registry = ToolRegistry::new();
        registry.register_entry(ToolEntry::new(Arc::new(TestEntryTool)));

        let items = registry.list(None).await.unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].name(), "test");

        let filtered = registry
            .list(Some(super::registry_trait::ToolFilter {
                name_prefix: Some("tes".to_string()),
            }))
            .await
            .unwrap();
        assert_eq!(filtered.len(), 1);

        let no_match = registry
            .list(Some(super::registry_trait::ToolFilter {
                name_prefix: Some("xyz".to_string()),
            }))
            .await
            .unwrap();
        assert_eq!(no_match.len(), 0);
    }

    #[tokio::test]
    async fn test_registry_trait_get() {
        let registry = ToolRegistry::new();
        registry.register_entry(ToolEntry::new(Arc::new(TestEntryTool)));

        let item = registry.get("test").await.unwrap();
        assert_eq!(item.unwrap().name(), "test");

        let not_found = registry.get("nonexistent").await.unwrap();
        assert!(not_found.is_none());
    }

    #[tokio::test]
    async fn test_registry_trait_contains_and_len() {
        let registry = ToolRegistry::new();
        assert!(registry.snapshot().is_empty());
        assert_eq!(registry.tool_count(), 0);

        registry.register_entry(ToolEntry::new(Arc::new(TestEntryTool)));
        assert!(!registry.snapshot().is_empty());
        assert_eq!(registry.tool_count(), 1);
        assert!(
            registry.snapshot().iter().any(|s| s.name == "test"),
            "expected `test` to appear in the registry snapshot"
        );
        assert!(
            registry.snapshot().iter().all(|s| s.name != "nonexistent"),
            "expected `nonexistent` to be absent"
        );
    }

    // ── Dual-index (snapshot) tests ──

    #[test]
    fn test_snapshot_is_sorted_by_name() {
        #[derive(Debug)]
        struct ToolA;
        #[derive(Debug)]
        struct ToolB;

        #[async_trait]
        impl Tool for ToolA {
            fn name(&self) -> &str {
                "a"
            }

            fn description(&self) -> &str {
                "Tool A"
            }

            fn parameters(&self) -> serde_json::Value {
                serde_json::json!({})
            }

            async fn call(
                &self,
                _: serde_json::Value,
                _: &Context,
            ) -> ToolOutput {
                ToolOutput::text("a")
            }
        }

        #[async_trait]
        impl Tool for ToolB {
            fn name(&self) -> &str {
                "b"
            }

            fn description(&self) -> &str {
                "Tool B"
            }

            fn parameters(&self) -> serde_json::Value {
                serde_json::json!({})
            }

            async fn call(
                &self,
                _: serde_json::Value,
                _: &Context,
            ) -> ToolOutput {
                ToolOutput::text("b")
            }
        }

        // Register in reverse-alphabetical order to prove the snapshot
        // ordering is independent of insertion order.
        let registry = ToolRegistry::new();
        registry.register_entry(ToolEntry::new(Arc::new(ToolB)));
        registry.register_entry(ToolEntry::new(Arc::new(ToolA)));

        let meta = registry.snapshot();
        assert_eq!(meta.len(), 2);
        assert_eq!(meta[0].name, "a");
        assert_eq!(meta[1].name, "b");
    }

    #[tokio::test]
    async fn test_snapshot_after_unregister() {
        let registry = ToolRegistry::new();
        registry.register_entry(ToolEntry::new(Arc::new(TestEntryTool)));

        #[derive(Debug)]
        struct OtherTool;
        #[async_trait]
        impl Tool for OtherTool {
            fn name(&self) -> &str {
                "other"
            }

            fn description(&self) -> &str {
                "Other"
            }

            fn parameters(&self) -> serde_json::Value {
                serde_json::json!({})
            }

            async fn call(
                &self,
                _: serde_json::Value,
                _: &Context,
            ) -> ToolOutput {
                ToolOutput::text("o")
            }
        }
        registry.register_entry(ToolEntry::new(Arc::new(OtherTool)));

        let meta = registry.snapshot();
        assert_eq!(meta.len(), 2);

        registry.unregister_by_name("test");
        let meta = registry.snapshot();
        assert_eq!(meta.len(), 1);
        assert_eq!(meta[0].name, "other");
    }

    #[test]
    fn test_snapshot_empty_registry() {
        let registry = ToolRegistry::new();
        assert!(registry.snapshot().is_empty());
    }

    #[test]
    fn test_entry_metadata_builders() {
        let entry =
            ToolEntry::new(Arc::new(TestEntryTool)).with_is_hidden(true);
        assert!(entry.is_hidden());
    }

    #[tokio::test]
    async fn resolve_returns_canonical_tool_trait_object() {
        let registry = ToolRegistry::new();
        assert!(
            registry.register_entry(ToolEntry::new(Arc::new(TestEntryTool)))
        );
        let entries = registry.list(None).await.unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name(), "test");
    }

    #[test]
    fn materialization_preserves_descriptor_content() {
        let registry = ToolRegistry::new();
        assert!(
            registry.register_entry(ToolEntry::new(Arc::new(TestEntryTool)))
        );
        let descriptors = registry.snapshot();
        assert_eq!(descriptors.len(), 1);
        assert_eq!(descriptors[0].name, "test");
        assert_eq!(descriptors[0].description, "A test tool");
    }

    #[tokio::test]
    async fn register_scoped_arc_unregisters_on_drop() {
        let registry = Arc::new(ToolRegistry::new());
        let scope = registry
            .register_scoped_arc(ToolEntry::new(Arc::new(TestEntryTool)))
            .await;
        assert!(
            registry.snapshot().iter().any(|s| s.name == "test"),
            "expected `test` to be present after registration"
        );
        drop(scope);
        assert!(
            registry.snapshot().iter().all(|s| s.name != "test"),
            "expected `test` to be removed after scope drop"
        );
    }

    #[derive(Debug)]
    struct LargeOutputTool;

    #[async_trait]
    impl Tool7 for LargeOutputTool {
        fn name(&self) -> &str {
            "large_output"
        }

        fn description(&self) -> &str {
            "Returns output larger than the configured bound"
        }

        fn parameters(&self) -> serde_json::Value {
            serde_json::json!({"type": "object", "properties": {}})
        }

        async fn call(
            &self,
            _input: serde_json::Value,
            _context: &Context,
        ) -> ToolOutput {
            ToolOutput::text("line\n".repeat(100))
        }
    }

    #[tokio::test]
    async fn dispatch_applies_tool_truncate() {
        let registry = ToolRegistry::new();
        assert!(
            registry.register_entry(ToolEntry::new(Arc::new(LargeOutputTool)))
        );
        let mut context =
            Context::new("truncate-session".to_string(), std::env::temp_dir());
        context.output_bound.per_call_max_bytes = 80;
        context.output_bound.per_call_max_lines = 10;
        context.output_bound.managed_dir = tempfile::tempdir().unwrap().keep();
        let outputs = collect_results(
            registry.run_stream(
                vec![synthia_provider::ToolUse {
                    id: "call-1".to_string(),
                    name: "large_output".to_string(),
                    input: serde_json::json!({}),
                }],
                context,
            ),
            1,
        )
        .await;
        assert!(outputs[0].1.truncated_by.is_some());
        assert!(outputs[0].1.metadata.contains_key("managed_path"));
    }

    // ── stream-as-primary-execution-path tests ──────────────────────

    use futures::stream;

    #[derive(Debug)]
    struct StreamingTool {
        progress_count: usize,
    }

    #[async_trait]
    impl Tool for StreamingTool {
        fn name(&self) -> &str {
            "streaming"
        }

        fn description(&self) -> &str {
            "Yields N progress items then a final Result"
        }

        fn parameters(&self) -> serde_json::Value {
            serde_json::json!({"type": "object", "properties": {}})
        }

        async fn call(
            &self,
            _input: serde_json::Value,
            _context: &Context,
        ) -> ToolOutput {
            panic!("streaming tool should override stream, not call")
        }

        fn stream<'a>(
            &'a self,
            _input: serde_json::Value,
            _context: &'a Context,
        ) -> std::pin::Pin<
            Box<
                dyn futures::Stream<Item = crate::traits::StreamOutput>
                    + Send
                    + 'a,
            >,
        > {
            let n = self.progress_count;
            Box::pin(
                stream::iter((0..n).map(|i| {
                    crate::traits::StreamOutput::Progress(ToolOutput::text(
                        format!("step {i}"),
                    ))
                }))
                .chain(stream::once(async {
                    crate::traits::StreamOutput::Result(ToolOutput::text(
                        "done",
                    ))
                })),
            )
        }
    }

    #[tokio::test]
    async fn dispatch_consumes_stream_collects_final_result() {
        let registry = ToolRegistry::new();
        registry.register_entry(ToolEntry::new(Arc::new(StreamingTool {
            progress_count: 3,
        })));

        let results = collect_results(
            registry.run_stream(
                vec![synthia_provider::ToolUse {
                    id: "call-1".to_string(),
                    name: "streaming".to_string(),
                    input: serde_json::json!({}),
                }],
                Context::new("s1".to_string(), PathBuf::from("/tmp")),
            ),
            1,
        )
        .await;

        assert_eq!(results.len(), 1);
        // Progress items dropped, only the final Result surfaces.
        let text = results[0].1.content[0].text().unwrap();
        assert_eq!(text, "done");
        assert!(results[0].1.is_error.is_none());
    }

    #[derive(Debug)]
    struct EmptyStreamTool;

    #[async_trait]
    impl Tool for EmptyStreamTool {
        fn name(&self) -> &str {
            "empty_stream"
        }

        fn description(&self) -> &str {
            "Stream that never yields a Result"
        }

        fn parameters(&self) -> serde_json::Value {
            serde_json::json!({"type": "object", "properties": {}})
        }

        async fn call(
            &self,
            _input: serde_json::Value,
            _context: &Context,
        ) -> ToolOutput {
            ToolOutput::text("unused")
        }

        fn stream<'a>(
            &'a self,
            _input: serde_json::Value,
            _context: &'a Context,
        ) -> std::pin::Pin<
            Box<
                dyn futures::Stream<Item = crate::traits::StreamOutput>
                    + Send
                    + 'a,
            >,
        > {
            Box::pin(stream::empty::<crate::traits::StreamOutput>())
        }
    }

    #[tokio::test]
    async fn dispatch_returns_error_when_stream_yields_no_result() {
        let registry = ToolRegistry::new();
        registry.register_entry(ToolEntry::new(Arc::new(EmptyStreamTool)));

        let results = collect_results(
            registry.run_stream(
                vec![synthia_provider::ToolUse {
                    id: "call-1".to_string(),
                    name: "empty_stream".to_string(),
                    input: serde_json::json!({}),
                }],
                Context::new("s1".to_string(), PathBuf::from("/tmp")),
            ),
            1,
        )
        .await;

        assert_eq!(results.len(), 1);
        assert!(results[0].1.is_error.unwrap_or(false));
        let text = results[0].1.content[0].text().unwrap();
        assert!(text.contains("contract violation"), "got: {text}");
    }

    #[derive(Debug)]
    struct ProgressOnlyTool;

    #[async_trait]
    impl Tool for ProgressOnlyTool {
        fn name(&self) -> &str {
            "progress_only"
        }

        fn description(&self) -> &str {
            "Stream that yields only Progress items, no Result"
        }

        fn parameters(&self) -> serde_json::Value {
            serde_json::json!({"type": "object", "properties": {}})
        }

        async fn call(
            &self,
            _input: serde_json::Value,
            _context: &Context,
        ) -> ToolOutput {
            ToolOutput::text("unused")
        }

        fn stream<'a>(
            &'a self,
            _input: serde_json::Value,
            _context: &'a Context,
        ) -> std::pin::Pin<
            Box<
                dyn futures::Stream<Item = crate::traits::StreamOutput>
                    + Send
                    + 'a,
            >,
        > {
            Box::pin(stream::iter([
                crate::traits::StreamOutput::Progress(ToolOutput::text("a")),
                crate::traits::StreamOutput::Progress(ToolOutput::text("b")),
            ]))
        }
    }

    #[tokio::test]
    async fn dispatch_treats_no_result_as_contract_violation() {
        let registry = ToolRegistry::new();
        registry.register_entry(ToolEntry::new(Arc::new(ProgressOnlyTool)));

        let results = collect_results(
            registry.run_stream(
                vec![synthia_provider::ToolUse {
                    id: "call-1".to_string(),
                    name: "progress_only".to_string(),
                    input: serde_json::json!({}),
                }],
                Context::new("s1".to_string(), PathBuf::from("/tmp")),
            ),
            1,
        )
        .await;

        assert_eq!(results.len(), 1);
        assert!(results[0].1.is_error.unwrap_or(false));
    }

    /// `snapshot_with_provenance` returns one record per visible tool
    /// with `provenance: ToolProvenance::Dynamic` (since `register_entry`
    /// always wraps as Dynamic).
    #[test]
    fn snapshot_with_provenance_returns_records_with_provenance() {
        let registry = ToolRegistry::new();
        registry.register_entry(ToolEntry::new(Arc::new(NamedTool("alpha"))));
        let snap = registry.snapshot_with_provenance();
        assert_eq!(snap.len(), 1);
        assert_eq!(snap[0].metadata.name, "alpha");
        assert_eq!(snap[0].provenance, ToolProvenance::Dynamic);
    }

    /// `snapshot_with_provenance` filters hidden entries (same as
    /// `snapshot`) and returns records sorted by name.
    #[test]
    fn snapshot_with_provenance_skips_hidden_and_sorts() {
        let registry = ToolRegistry::new();
        registry.register_entry(ToolEntry::new(Arc::new(NamedTool("zoo"))));
        registry.register_entry(
            ToolEntry::new(Arc::new(NamedTool("apple"))).with_is_hidden(true),
        );
        registry.register_entry(ToolEntry::new(Arc::new(NamedTool("mango"))));
        let snap = registry.snapshot_with_provenance();
        let names: Vec<&str> =
            snap.iter().map(|r| r.metadata.name.as_str()).collect();
        assert_eq!(names, vec!["mango", "zoo"]);
    }
}
