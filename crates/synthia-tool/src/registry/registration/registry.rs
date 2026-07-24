//! [`ToolRegistry`] — the in-process tool catalog
//! plus the dispatch path that turns a vector of
//! `ToolUse` into a vector of `ToolOutput`.
//!
//! Responsibilities are split across three submodules:
//!
//! - [`super::entry`]: the [`ToolEntry`] value type
//!   (one entry per registered tool).
//! - `registry` (this file): the [`ToolRegistry`]
//!   struct itself — the in-memory catalog, the
//!   permission-aware dispatch pipeline
//!   ([`ToolRegistry::run_with_context`]), and the
//!   per-call semaphore-bounded executor
//!   ([`ToolRegistry::execute_tools`]).
//! - [`super::registry_trait`]: the `impl
//!   Registry<ToolEntry> for ToolRegistry` block
//!   (CRUD over the catalog — register, unregister,
//!   get, list).
//!
//! The trait impl lives in its own file because it
//! has a long, mostly-mechanical surface (4 methods)
//! that would otherwise dominate this file. Keeping
//! it separate also makes it easy to verify the
//! inherent API (used by the agent runtime) is
//! unchanged when the trait surface evolves.

use std::{collections::HashMap, sync::Arc};

use parking_lot::RwLock;
use synthia_core::{Error, registry::RegistryItem};
use synthia_permission::{Permission, PermissionChecker, PermissionRequest};
#[cfg(feature = "otel")]
use tracing::Instrument;

use super::entry::ToolEntry;
use crate::{
    builtin::{
        ApplyPatchTool,
        GlobTool,
        GrepTool,
        MultiEditTool,
        ReadTool,
        WebFetchTool,
        WriteTool,
    },
    traits::Tool,
    types::*,
};

/// In-process tool catalog + dispatch pipeline.
///
/// The catalog is the `RwLock<HashMap<String,
/// ToolEntry>>` at the heart of the struct. The
/// `max_concurrent` knob bounds how many tool
/// invocations can run in parallel from a single
/// `run_with_context` call (default 5). The optional
/// `checker` is consulted before any tool that
/// reports `requires_permission()` — if it returns a
/// `Deny` / `RequireConfirm` / `RequireExplicit` /
/// `Block` verdict, the corresponding output slot
/// gets an error string instead of the tool's actual
/// result.
pub struct ToolRegistry {
    /// The catalog. Read-locked for every `get` /
    /// `list` / `run_with_context`, write-locked only
    /// at `register` / `unregister` time.
    ///
    /// `pub(super)` so [`super::registry_trait`] can
    /// share the lock for the `Registry<ToolEntry>`
    /// CRUD surface.
    pub(super) tools: RwLock<HashMap<String, ToolEntry>>,
    /// Max parallel tool invocations per dispatch
    /// call. See [`ToolRegistry::with_max_concurrent`].
    pub(super) max_concurrent: usize,
    /// Optional permission policy checker. See
    /// [`ToolRegistry::with_checker`].
    pub(super) checker: Option<PermissionChecker>,
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ToolRegistry {
    /// Build an empty registry with the default
    /// `max_concurrent = 5` and no permission checker.
    pub fn new() -> Self {
        Self {
            tools: RwLock::new(HashMap::new()),
            max_concurrent: 5,
            checker: None,
        }
    }

    /// Attach a [`PermissionChecker`] that will be
    /// consulted before every tool that reports
    /// `requires_permission() == true`.
    pub fn with_checker(mut self, checker: PermissionChecker) -> Self {
        self.checker = Some(checker);
        self
    }

    /// Build a registry pre-populated with the
    /// built-in tool set: `ReadTool`, `WriteTool`,
    /// `GlobTool`, `GrepTool`, `MultiEditTool`,
    /// `ApplyPatchTool`, `WebFetchTool`.
    pub fn register_defaults() -> Self {
        let registry = Self::new();

        registry.register(ToolEntry::new(Arc::new(ReadTool::new())));
        registry.register(ToolEntry::new(Arc::new(WriteTool)));
        registry.register(ToolEntry::new(Arc::new(GlobTool)));
        registry.register(ToolEntry::new(Arc::new(GrepTool)));
        registry.register(ToolEntry::new(Arc::new(MultiEditTool)));
        registry.register(ToolEntry::new(Arc::new(ApplyPatchTool::default())));
        registry.register(ToolEntry::new(Arc::new(WebFetchTool::new())));

        registry
    }

    /// Override the per-dispatch concurrency cap.
    /// Builders chain this off [`Self::new`] or
    /// [`Self::register_defaults`].
    pub fn with_max_concurrent(mut self, max: usize) -> Self {
        self.max_concurrent = max;
        self
    }

    /// Inherent (non-trait) registration entry point.
    /// Used by [`Self::register_defaults`] and other
    /// pre-population paths that don't need the
    /// `Registry<ToolEntry>` trait's "already-exists"
    /// error semantics.
    ///
    /// For external callers, prefer the
    /// `Registry::register` method in
    /// [`super::registry_trait`].
    pub fn register(&self, item: ToolEntry) {
        let name = item.name().to_string();
        let mut tools = self.tools.write();
        tools.insert(name, item);
    }

    /// The dispatch pipeline. Walks `tool_uses`, runs
    /// the permission check (if any), drops denied
    /// invocations into error outputs, and runs the
    /// remaining ones through the semaphore-bounded
    /// [`Self::execute_tools`]. Every input position
    /// gets exactly one output (errors are still
    /// outputs).
    pub async fn run_with_context(
        &self,
        tool_uses: Vec<synthia_provider::ToolUse>,
        context: ToolExecutionContext,
    ) -> Result<Vec<ToolOutput>> {
        if tool_uses.is_empty() {
            return Ok(Vec::new());
        }

        let executable = {
            let tools = self.tools.read();
            tools
                .iter()
                .filter(|(_, v)| !v.tool.is_hidden())
                .map(|(k, v)| (k.clone(), Arc::clone(&v.tool)))
                .collect::<HashMap<_, _>>()
        };

        let mut outputs: Vec<Option<ToolOutput>> = vec![None; tool_uses.len()];
        let mut allowed = Vec::new();

        let permission_requests: Vec<(usize, PermissionRequest)> = tool_uses
            .iter()
            .enumerate()
            .filter(|&(_, tu)| executable.contains_key(&tu.name))
            .map(|(i, tu)| {
                let requires_perm = executable[&tu.name].requires_permission();
                (
                    i,
                    PermissionRequest::new(
                        tu.name.clone(),
                        tu.input.clone(),
                        requires_perm,
                    ),
                )
            })
            .collect();

        if !permission_requests.is_empty() {
            if let Some(ref checker) = self.checker {
                let requests: Vec<_> = permission_requests
                    .iter()
                    .map(|(_, r)| r.clone())
                    .collect();
                let decisions = checker
                    .check(&requests)
                    .await
                    .map_err(|e| Error::Internal(e.to_string()))?;
                for (idx, req) in &permission_requests {
                    match decisions.get(&req.tool_name) {
                        Some(Permission::Deny { reason }) => {
                            outputs[*idx] = Some(ToolOutput::error(format!(
                                "Permission denied for '{}': {}",
                                req.tool_name, reason
                            )));
                        }
                        Some(Permission::RequireConfirm)
                        | Some(Permission::RequireExplicit)
                        | Some(Permission::Block) => {
                            outputs[*idx] = Some(ToolOutput::error(format!(
                                "Tool '{}' denied by user",
                                req.tool_name
                            )));
                        }
                        _ => allowed.push((*idx, tool_uses[*idx].clone())),
                    }
                }
            } else {
                for (idx, _) in &permission_requests {
                    allowed.push((*idx, tool_uses[*idx].clone()));
                }
            }
        }

        for (i, tu) in tool_uses.iter().enumerate() {
            if outputs[i].is_none() && !executable.contains_key(&tu.name) {
                outputs[i] = Some(ToolOutput::error(format!(
                    "Tool '{}' not found",
                    tu.name
                )));
            }
        }

        if !allowed.is_empty() {
            let exec_outputs = self
                .execute_tools(
                    allowed.iter().map(|(_, tu)| tu.clone()).collect(),
                    executable,
                    context,
                )
                .await;
            for (idx, output) in exec_outputs.into_iter().enumerate() {
                let (i, _) = &allowed[idx];
                outputs[*i] = Some(output);
            }
        }

        Ok(outputs
            .into_iter()
            .map(|o| o.unwrap_or_else(|| ToolOutput::error("No output")))
            .collect())
    }

    /// Run a batch of `tool_uses` under a semaphore
    /// that limits concurrency to `self.max_concurrent`.
    /// Hidden tools (which were filtered out of the
    /// `executable` map by `run_with_context`) and
    /// missing tools both produce error outputs;
    /// panics in the spawned task are caught and
    /// surfaced as `Task panicked: ...` errors.
    async fn execute_tools(
        &self,
        tool_uses: Vec<synthia_provider::ToolUse>,
        tools: HashMap<String, Arc<dyn Tool>>,
        context: ToolExecutionContext,
    ) -> Vec<ToolOutput> {
        let semaphore =
            Arc::new(tokio::sync::Semaphore::new(self.max_concurrent));
        let mut futures = Vec::with_capacity(tool_uses.len());

        for tool_use in tool_uses {
            let permit = semaphore
                .clone()
                .acquire_owned()
                .await
                .map_err(|e| {
                    Error::Internal(format!("Semaphore closed: {}", e))
                })
                .unwrap();
            let tools = tools.clone();
            let context = context.clone();

            futures.push(tokio::spawn(async move {
                let _permit = permit;
                let name = &tool_use.name;
                if let Some(tool) = tools.get(name.as_str()) {
                    let tool_input = ToolInput {
                        name: name.clone(),
                        input: tool_use.input.clone(),
                        context,
                    };

                    // `tool.execute` span (OTel). Created inside the
                    // spawned task so it wraps `tool.call()`.
                    // `.instrument(span.clone())` is used (rather
                    // than `Span::enter`) because `tracing::span::
                    // Entered` is `!Send` and cannot be held across
                    // `.await` in a spawned (Send) future. The
                    // original span handle is retained to record
                    // exception attributes after the call returns.
                    //
                    // All fields populated after creation MUST be
                    // declared as `Empty` at the callsite —
                    // `Span::record(field, value)` is a silent
                    // no-op if the field was not declared in the
                    // `span!` macro (lesson from Task 7).
                    //
                    // Without the `otel` feature the `span!` macro
                    // and all recording logic are compile-time
                    // eliminated — `tool.call()` runs directly with
                    // zero span overhead.
                    #[cfg(feature = "otel")]
                    let tool_span = tracing::span!(
                        target: "synthia.tool",
                        tracing::Level::INFO,
                        "tool.execute",
                        tool.name = %name,
                        exception.type = tracing::field::Empty,
                        exception.message = tracing::field::Empty,
                        otel.status_code = tracing::field::Empty,
                    );

                    #[cfg(feature = "otel")]
                    let output = tool
                        .call(tool_input)
                        .instrument(tool_span.clone())
                        .await;
                    #[cfg(not(feature = "otel"))]
                    let output = tool.call(tool_input).await;

                    #[cfg(feature = "otel")]
                    if output.is_error.unwrap_or(false) {
                        let msg: String = output
                            .content
                            .iter()
                            .filter_map(|p| p.text())
                            .collect::<Vec<_>>()
                            .join("\n");
                        let lowered = msg.to_lowercase();
                        // Tools that enforce their own timeout (e.g.
                        // the bash tool wraps `tokio::time::timeout`
                        // and surfaces "Command timed out ...") are
                        // mapped to the OTel `TimeoutError`
                        // exception type per the spec scenario.
                        if lowered.contains("timed out")
                            || lowered.contains("timeout")
                        {
                            tool_span.record("exception.type", "TimeoutError");
                        } else {
                            tool_span.record("exception.type", "ToolError");
                        }
                        tool_span.record("exception.message", &msg);
                        tool_span.record("otel.status_code", "ERROR");
                    }

                    output
                } else {
                    ToolOutput::error(format!("Tool '{}' not found", name))
                }
            }));
        }

        let mut results = Vec::with_capacity(futures.len());
        for f in futures {
            match f.await {
                Ok(result) => results.push(result),
                Err(e) => results
                    .push(ToolOutput::error(format!("Task panicked: {}", e))),
            }
        }
        results
    }

    /// Cheap presence check; no allocation.
    pub fn contains(&self, name: &str) -> bool {
        self.tools.read().contains_key(name)
    }

    /// Current catalog size.
    pub fn len(&self) -> usize {
        self.tools.read().len()
    }

    /// Convenience for "no tools registered" checks.
    pub fn is_empty(&self) -> bool {
        self.tools.read().is_empty()
    }
}

impl Clone for ToolRegistry {
    /// Clone the registry by snapshotting the
    /// catalog (every entry is itself cheaply cloneable
    /// — `Arc<dyn Tool>` + two `String`s) and copying
    /// the concurrency + checker settings. The clone
    /// shares the underlying trait objects with the
    /// original via `Arc`, so dispatch from either
    /// side ends up calling the same tool instance.
    fn clone(&self) -> Self {
        Self {
            tools: RwLock::new(self.tools.read().clone()),
            max_concurrent: self.max_concurrent,
            checker: self.checker.clone(),
        }
    }
}
