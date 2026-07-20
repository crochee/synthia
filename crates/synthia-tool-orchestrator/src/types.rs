use std::{
    collections::HashMap,
    sync::{Arc, RwLock as StdRwLock},
};

use async_trait::async_trait;
use dashmap::DashMap;
use serde::{Deserialize, Serialize};

/// A request to invoke a single tool call within an orchestrated execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallRequest {
    pub call_id: String,
    pub tool_name: String,
    pub arguments: serde_json::Value,
    /// Effective permission level that governs whether approval is required.
    pub permission: synthia_permission::Permission,
    /// Materialized tool identity for audit traceability.
    /// Populated by the orchestrator from the registry's materialization data.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_id: Option<synthia_tool_materialization::ToolId>,
}

/// The result of a completed tool call.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallResult {
    pub call_id: String,
    pub tool_name: String,
    pub outcome: serde_json::Value,
    pub is_error: bool,
    /// Echo of the request's tool_id for audit correlation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_id: Option<synthia_tool_materialization::ToolId>,
}

/// Runtime context attached to a tool call.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionContext {
    pub session_id: String,
    pub workspace_root: std::path::PathBuf,
    pub caller_agent: String,
}

/// Lifecycle events emitted by a [`ToolOrchestrator`](crate::ToolOrchestrator).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ToolOrchestratorEvent {
    Started {
        call_id: String,
        tool_name: String,
    },
    Completed {
        call_id: String,
        tool_name: String,
        result: ToolCallResult,
        tool_id: Option<synthia_tool_materialization::ToolId>,
    },
    Failed {
        call_id: String,
        tool_name: String,
        error: String,
    },
    Cancelled {
        call_id: String,
        tool_name: String,
    },
    /// A file-mutating tool emitted a progress event (e.g. a patch hunk was
    /// applied).
    FileChange {
        call_id: String,
        tool_name: String,
        event: synthia_tool::FileChangeEvent,
    },
    /// Edit conflict detected: file was modified since agent read it.
    EditConflict {
        call_id: String,
        tool_name: String,
        path: std::path::PathBuf,
        conflict: crate::ConflictInfo,
    },
}

/// Errors that can be returned by a [`ToolOrchestrator`](crate::ToolOrchestrator).
#[derive(Debug, thiserror::Error, Clone, Serialize, Deserialize)]
pub enum ToolOrchestratorError {
    #[error("tool call {call_id} error: {message}")]
    Generic { call_id: String, message: String },
    #[error("tool call {call_id} was cancelled")]
    Cancelled { call_id: String },
    #[error("tool call {call_id} was denied")]
    Denied { call_id: String },
    #[error("tool call {call_id} sandbox error: {message}")]
    Sandbox { call_id: String, message: String },
    #[error("tool call {call_id} tool not found: {tool_name}")]
    NotFound { call_id: String, tool_name: String },
    #[error("tool call {call_id} edit conflict on {path}")]
    EditConflict {
        call_id: String,
        path: std::path::PathBuf,
        original_content_hash: u64,
        current_content_hash: u64,
    },
}

impl ToolOrchestratorError {
    pub(crate) fn cancelled(call_id: impl Into<String>) -> Self {
        Self::Cancelled {
            call_id: call_id.into(),
        }
    }

    pub(crate) fn denied(call_id: impl Into<String>) -> Self {
        Self::Denied {
            call_id: call_id.into(),
        }
    }

    pub(crate) fn generic(
        call_id: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self::Generic {
            call_id: call_id.into(),
            message: message.into(),
        }
    }
}

/// Errors that can be returned by an [`ExecutableTool`](crate::ExecutableTool).
#[derive(Debug, thiserror::Error, Clone)]
pub enum ToolExecutionError {
    #[error("transient error: {0}")]
    Transient(String),
    #[error("permanent error: {0}")]
    Permanent(String),
    #[error("cancelled")]
    Cancelled,
}

impl ToolExecutionError {
    /// Return `true` if the error is transient and the call should be retried.
    pub fn is_transient(&self) -> bool {
        matches!(self, Self::Transient(_))
    }
}

/// Resolves a tool name to its materialized [`ToolId`], if available.
///
/// Implemented by registries that track tool materialization (e.g.
/// `ScopedToolRegistry`). The orchestrator consults this after
/// `ToolResolver::resolve()` to populate `request.tool_id` for
/// audit traceability.
pub trait ToolIdResolver: Send + Sync {
    fn resolve_id(
        &self,
        name: &str,
    ) -> Option<synthia_tool_materialization::ToolId>;
}

/// A simple in-memory `ToolIdResolver` backed by a `HashMap`.
#[derive(Clone, Default)]
pub struct HashMapToolIdResolver {
    ids: Arc<HashMap<String, synthia_tool_materialization::ToolId>>,
}

impl HashMapToolIdResolver {
    /// Create a new resolver from a map of tool names to ToolIds.
    pub fn new(
        ids: HashMap<String, synthia_tool_materialization::ToolId>,
    ) -> Self {
        Self { ids: Arc::new(ids) }
    }
}

impl ToolIdResolver for HashMapToolIdResolver {
    fn resolve_id(
        &self,
        name: &str,
    ) -> Option<synthia_tool_materialization::ToolId> {
        self.ids.get(name).copied()
    }
}

/// Retry policy for individual tool calls.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RetryPolicy {
    pub max_attempts: u32,
    pub base_delay_ms: u64,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 1,
            base_delay_ms: 0,
        }
    }
}

/// Concurrency policy for batch execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConcurrencyPolicy {
    pub max_concurrent: usize,
}

impl Default for ConcurrencyPolicy {
    fn default() -> Self {
        Self { max_concurrent: 5 }
    }
}

/// A simple in-memory resolver backed by a `HashMap`.
#[derive(Clone, Default)]
pub struct HashMapResolver {
    tools: Arc<HashMap<String, Arc<dyn crate::ExecutableTool>>>,
}

impl HashMapResolver {
    /// Create a new resolver from a map of tool names to tools.
    pub fn new(tools: HashMap<String, Arc<dyn crate::ExecutableTool>>) -> Self {
        Self {
            tools: Arc::new(tools),
        }
    }

    /// Consume the resolver and return the underlying tool map.
    pub fn into_tools(self) -> HashMap<String, Arc<dyn crate::ExecutableTool>> {
        Arc::try_unwrap(self.tools).unwrap_or_else(|arc| (*arc).clone())
    }
}

#[async_trait]
impl crate::ToolResolver for HashMapResolver {
    fn resolve(&self, name: &str) -> Option<Arc<dyn crate::ExecutableTool>> {
        self.tools.get(name).cloned()
    }
}

/// A resolver that supports runtime registration of tools.
///
/// Useful for dynamically discovered tools (e.g. from MCP servers) that must
/// be added to the orchestrator after construction.
#[derive(Clone, Default)]
pub struct DynamicResolver {
    tools: Arc<StdRwLock<HashMap<String, Arc<dyn crate::ExecutableTool>>>>,
}

impl DynamicResolver {
    /// Create an empty resolver.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a resolver pre-populated with the given tools.
    pub fn with_tools(
        tools: HashMap<String, Arc<dyn crate::ExecutableTool>>,
    ) -> Self {
        Self {
            tools: Arc::new(StdRwLock::new(tools)),
        }
    }

    /// Register a tool at runtime.
    pub fn register(
        &self,
        name: impl Into<String>,
        tool: Arc<dyn crate::ExecutableTool>,
    ) {
        self.tools
            .write()
            .expect("DynamicResolver RwLock poisoned")
            .insert(name.into(), tool);
    }

    /// Remove a previously registered tool.
    pub fn unregister(&self, name: &str) -> bool {
        self.tools
            .write()
            .expect("DynamicResolver RwLock poisoned")
            .remove(name)
            .is_some()
    }

    /// Check whether a tool is currently registered.
    pub fn contains(&self, name: &str) -> bool {
        self.tools
            .read()
            .expect("DynamicResolver RwLock poisoned")
            .contains_key(name)
    }
}

#[async_trait]
impl crate::ToolResolver for DynamicResolver {
    fn resolve(&self, name: &str) -> Option<Arc<dyn crate::ExecutableTool>> {
        self.tools
            .read()
            .expect("DynamicResolver RwLock poisoned")
            .get(name)
            .cloned()
    }
}

/// Bookkeeping for a single in-flight tool call.
///
/// Stored as the value of `active_calls` so that
/// [`DefaultToolOrchestrator::fail_interrupted_tools`](crate::DefaultToolOrchestrator::fail_interrupted_tools) can recover the
/// `tool_name` without an auxiliary map.
#[derive(Clone)]
pub(crate) struct ActiveCall {
    pub tool_name: String,
    pub token: tokio_util::sync::CancellationToken,
}

/// Removes a call ID from the active-calls map when dropped.
pub(crate) struct ActiveCallGuard {
    pub map: Arc<DashMap<String, ActiveCall>>,
    pub call_id: String,
}

impl Drop for ActiveCallGuard {
    fn drop(&mut self) {
        self.map.remove(&self.call_id);
    }
}
