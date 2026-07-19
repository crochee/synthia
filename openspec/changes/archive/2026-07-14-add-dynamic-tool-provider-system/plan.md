# add-dynamic-tool-provider-system Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use omo-subagent-driven-development (recommended) or omo-dispatching-parallel-agents to implement this plan task-by-task. Each task should specify a `category` (quick/deep/ultrabrain/visual-engineering) and `load_skills` for oh-my-opencode's task() tool. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a dynamic `ToolProvider` extension system to synthia-agent, enabling runtime tool registration without recompilation, with backward-compatible adapter for existing static tools.

**Architecture:** Two-tier trait hierarchy: `Tool` (base from `synthia_tool::Tool`) → `ToolRuntime` (orchestration + parallel execution + hooks) → `DynToolProvider` (dynamic extension). `ExtensionManager` manages provider registration with O(1) cache invalidation via `AtomicU64` version counter + `DashMap`.

**Tech Stack:** Rust (async_trait, DashMap, Arc/RwLock), existing `synthia_tool::Tool`, `ExecutableTool` from `synthia_tool_orchestrator`, `HookRegistry` from `synthia_hook`.

---

## Task 1: Foundation — ToolProvider Trait and ExtensionManager

**Files:**
- Create: `crates/synthia-agent/src/tools/dynamic_provider.rs`
- Modify: `crates/synthia-agent/src/tools/mod.rs` (add `pub mod dynamic_provider`)
- Test: `crates/synthia-agent/src/tools/dynamic_provider/tests.rs`

- [ ] **Step 1: Create `crates/synthia-agent/src/tools/dynamic_provider.rs` with empty module**

```rust
//! Dynamic tool provider extension system.

pub mod extension_manager;
pub mod tool_provider;

pub use extension_manager::ExtensionManager;
pub use tool_provider::{ToolPreCheck, ToolProvider};
```

- [ ] **Step 2: Run cargo check to verify module compiles**

Run: `cargo check -p synthia-agent --lib`
Expected: OK (empty module)

- [ ] **Step 3: Commit: "feat(agent): scaffold dynamic_provider module stub"**

```bash
git add crates/synthia-agent/src/tools/dynamic_provider.rs crates/synthia-agent/src/tools/mod.rs
git commit -m "feat(agent): scaffold dynamic_provider module stub"
```

- [ ] **Step 4: Define `ToolPreCheck` enum in `tool_provider.rs`**

```rust
use std::sync::Arc;

/// Result of a pre-execution check on a tool call.
#[derive(Debug, Clone)]
pub enum ToolPreCheck {
    /// Tool call is allowed to proceed immediately.
    Allow,
    /// Tool call requires user approval before execution.
    RequiresApproval,
    /// Tool call is denied; do not execute.
    Deny,
}

/// A reference to a JSON Schema for a tool's input parameters.
#[derive(Debug, Clone)]
pub enum SchemaRef {
    /// Inline schema value (owned).
    Inline(serde_json::Value),
    /// Reference to a schema by name (e.g. "#/definitions/MyToolInput").
    Ref(String),
}
```

- [ ] **Step 5: Define `ToolProvider` trait**

```rust
use async_trait::async_trait;
use synthia_hook::HookRegistry;

/// Metadata about a single tool exposed by a provider.
#[derive(Debug, Clone)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub parameters: SchemaRef,
    pub deprecated: Option<String>,
}

#[async_trait]
pub trait ToolProvider: Send + Sync {
    /// Unique identifier for this provider.
    fn name(&self) -> &'static str;

    /// List all tools exposed by this provider.
    fn list_tools(&self) -> Vec<ToolDefinition>;

    /// Pre-execution check for a tool call.
    fn pre_check(&self, tool_name: &str) -> ToolPreCheck {
        ToolPreCheck::Allow
    }

    /// Optional before-execute hook. Called before each tool execution.
    /// Return `Err(msg)` to deny execution.
    fn before_execute(
        &self,
        _tool_name: &str,
        _args: &serde_json::Value,
    ) -> Result<(), String> {
        Ok(())
    }

    /// Optional after-execute hook. Called after each tool execution.
    fn after_execute(
        &self,
        _tool_name: &str,
        _args: &serde_json::Value,
        _result: &serde_json::Value,
    ) {
    }

    /// Receive lifecycle events filtered to `tool_*` events.
    fn on_tool_event(&self, _event: &synthia_hook::events::HookEvent) {}
}
```

- [ ] **Step 6: Create `ExtensionManager` in `extension_manager.rs`**

```rust
use std::sync::Arc;
use dashmap::DashMap;
use tokio::sync::RwLock;
use std::sync::atomic::{AtomicU64, Ordering};

/// Manages dynamic tool providers with O(1) cache invalidation.
#[derive(Clone)]
pub struct ExtensionManager {
    providers: Arc<DashMap<String, Arc<dyn ToolProvider>>>,
    cache_version: Arc<AtomicU64>,
}

impl ExtensionManager {
    pub fn new() -> Self {
        Self {
            providers: Arc::new(DashMap::new()),
            cache_version: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Register a provider. Overwrites any existing provider with the same name.
    /// Increments the cache version, invalidating all cached tool lists.
    pub fn register(&self, provider: Arc<dyn ToolProvider>) {
        self.providers.insert(provider.name().to_string(), provider);
        self.cache_version.fetch_add(1, Ordering::SeqCst);
    }

    /// Unregister a provider by name. Returns `true` if a provider was removed.
    pub fn unregister(&self, name: &str) -> bool {
        let removed = self.providers.remove(name).is_some();
        if removed {
            self.cache_version.fetch_add(1, Ordering::SeqCst);
        }
        removed
    }

    /// List all tools from all registered providers. O(n) over all providers.
    pub fn list_tools(&self) -> Vec<ToolDefinition> {
        self.providers
            .iter()
            .flat_map(|entry| entry.value().list_tools())
            .collect()
    }

    /// Get a provider by name.
    pub fn get(&self, name: &str) -> Option<Arc<dyn ToolProvider>> {
        self.providers.get(name).map(|e| e.value().clone())
    }

    /// Get the current cache version. Incremented on any registration change.
    pub fn cache_version(&self) -> u64 {
        self.cache_version.load(Ordering::SeqCst)
    }

    /// Return `true` if no providers are registered.
    pub fn is_empty(&self) -> bool {
        self.providers.is_empty()
    }
}

impl Default for ExtensionManager {
    fn default() -> Self {
        Self::new()
    }
}
```

- [ ] **Step 7: Add unit tests for ExtensionManager in `crates/synthia-agent/src/tools/dynamic_provider.rs` (inline test module)**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    struct DummyProvider;

    #[async_trait::async_trait]
    impl ToolProvider for DummyProvider {
        fn name(&self) -> &'static str {
            "dummy"
        }

        fn list_tools(&self) -> Vec<ToolDefinition> {
            vec![ToolDefinition {
                name: "dummy_tool".to_string(),
                description: "A dummy tool".to_string(),
                parameters: SchemaRef::Inline(serde_json::json!({
                    "type": "object",
                    "properties": {},
                })),
                deprecated: None,
            }]
        }
    }

    #[tokio::test]
    async fn register_and_list() {
        let manager = ExtensionManager::new();
        manager.register(Arc::new(DummyProvider));
        let tools = manager.list_tools();
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name, "dummy_tool");
    }

    #[tokio::test]
    async fn unregister() {
        let manager = ExtensionManager::new();
        manager.register(Arc::new(DummyProvider));
        assert!(manager.unregister("dummy"));
        assert!(!manager.unregister("nonexistent"));
        assert!(manager.is_empty());
    }

    #[tokio::test]
    async fn cache_version_increments_on_register() {
        let manager = ExtensionManager::new();
        let v0 = manager.cache_version();
        manager.register(Arc::new(DummyProvider));
        let v1 = manager.cache_version();
        assert!(v1 > v0);
    }
}
```

- [ ] **Step 8: Run tests**

Run: `cargo test -p synthia-agent --lib dynamic_provider`
Expected: PASS

- [ ] **Step 9: Commit: "feat(agent): add ToolProvider trait and ExtensionManager with cache invalidation"**

---

## Task 2: Tool Trait Alignment

**Files:**
- Modify: `crates/synthia-agent/src/tools/dynamic_provider.rs` (add `Tool` trait alias)
- Modify: `crates/synthia-tool/src/traits.rs` (no changes — alignment review only)

- [ ] **Step 1: Review existing `synthia_tool::Tool` trait** (read-only, from `crates/synthia-tool/src/traits.rs`)

Confirm: `name()`, `description()`, `parameters() -> serde_json::Value`, `is_concurrency_safe()`, `call()`, `call_with_sandbox()`, `call_with_progress()`.

- [ ] **Step 2: Add `Tool` re-export in dynamic_provider.rs**

```rust
/// Alias for the base `Tool` trait from `synthia-tool`.
pub trait Tool: synthia_tool::Tool {}
impl<T: synthia_tool::Tool> Tool for T {}
```

This allows the extension system to reference `dyn Tool` without a generic bound on the trait itself.

- [ ] **Step 3: Review `ExecutableTool` compatibility** (read-only, from `synthia_tool_orchestrator`)

Confirm: `ExecutableTool` has `name()`, `is_concurrency_safe()`, `execute()`. `StaticToolAdapter` (Task 4) will bridge `Tool → ExecutableTool`.

- [ ] **Step 4: Commit: "refactor(agent): add Tool trait alias for dynamic provider system"**

---

## Task 3: ToolRuntime Orchestration Layer

**Files:**
- Create: `crates/synthia-agent/src/tools/tool_runtime.rs`
- Modify: `crates/synthia-agent/src/tools/mod.rs`
- Test: `crates/synthia-agent/src/tools/tool_runtime/tests.rs`

- [ ] **Step 1: Create `crates/synthia-agent/src/tools/tool_runtime.rs`**

```rust
//! Tool runtime — orchestrates tool execution with hooks, retries, and parallel execution.

use std::sync::Arc;
use tokio::sync::RwLock;
use synthia_tool_orchestrator::{
    ToolCallRequest, ToolCallResult, ToolOrchestratorError,
    ExecutableTool,
};

/// Runtime that orchestrates tool execution across multiple providers.
pub struct ToolRuntime {
    orchestrator: Arc<dyn synthia_tool_orchestrator::ToolOrchestrator>,
    extension_manager: ExtensionManager,
}

impl ToolRuntime {
    pub fn new(
        orchestrator: Arc<dyn synthia_tool_orchestrator::ToolOrchestrator>,
        extension_manager: ExtensionManager,
    ) -> Self {
        Self {
            orchestrator,
            extension_manager,
        }
    }

    /// Execute a batch of tool calls in parallel, using the orchestrator.
    pub async fn execute_batch(
        &self,
        requests: Vec<ToolCallRequest>,
        context: synthia_tool_orchestrator::ExecutionContext,
        cancel: tokio_util::sync::CancellationToken,
    ) -> Result<Vec<ToolCallResult>, ToolOrchestratorError> {
        self.orchestrator.execute_batch(requests, context, cancel).await
    }
}
```

- [ ] **Step 2: Verify it compiles**

Run: `cargo check -p synthia-agent --lib`
Expected: OK

- [ ] **Step 3: Commit: "feat(agent): add ToolRuntime orchestration layer"**

---

## Task 4: StaticToolAdapter (Backward Compatibility)

**Files:**
- Create: `crates/synthia-agent/src/tools/static_tool_adapter.rs`
- Modify: `crates/synthia-agent/src/tools/mod.rs`
- Test: `crates/synthia-agent/src/tools/static_tool_adapter/tests.rs`

- [ ] **Step 1: Create `StaticToolAdapter` wrapping `Arc<dyn synthia_tool::Tool>`**

```rust
//! Adapter that wraps a static `synthia_tool::Tool` for use in the dynamic provider system.

use std::sync::Arc;
use async_trait::async_trait;
use synthia_tool::Tool as ToolTrait;

use super::tool_provider::{SchemaRef, ToolDefinition, ToolProvider};

/// Wraps a static `synthia_tool::Tool` as a `ToolProvider`.
#[derive(Clone)]
pub struct StaticToolAdapter {
    tool: Arc<dyn ToolTrait>,
}

impl StaticToolAdapter {
    pub fn new(tool: Arc<dyn ToolTrait>) -> Self {
        Self { tool }
    }
}

#[async_trait]
impl ToolProvider for StaticToolAdapter {
    fn name(&self) -> &'static str {
        "static_adapter"
    }

    fn list_tools(&self) -> Vec<ToolDefinition> {
        vec![ToolDefinition {
            name: self.tool.name().to_string(),
            description: self.tool.description().to_string(),
            parameters: SchemaRef::Inline(self.tool.parameters()),
            deprecated: None,
        }]
    }
}
```

- [ ] **Step 2: Verify compilation**

Run: `cargo check -p synthia-agent --lib`
Expected: OK

- [ ] **Step 3: Add integration test in tests.rs**

```rust
#[tokio::test]
async fn static_adapter_wraps_read_file_tool() {
    use synthia_tool::builtin::ReadTool;
    let tool = Arc::new(ReadTool::new());
    let adapter = StaticToolAdapter::new(tool);
    let tools = adapter.list_tools();
    assert_eq!(tools.len(), 1);
    assert_eq!(tools[0].name, "read");
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p synthia-agent --lib static_tool_adapter`
Expected: PASS

- [ ] **Step 5: Commit: "feat(agent): add StaticToolAdapter for backward compatibility with existing tools"**

---

## Task 5: Agent Integration

**Files:**
- Modify: `crates/synthia-agent/src/config/agent_config/run_config.rs` (add `extension_manager` field)
- Modify: `crates/synthia-agent/src/config/agent_config/run_config_builder.rs`
- Modify: `crates/synthia-agent/src/tools/mod.rs`
- Test: integration test in `crates/synthia-agent/src/agent_integration_test.rs`

- [ ] **Step 1: Add `extension_manager` field to `AgentRunConfig`** in `run_config.rs`

Add after `tool_orchestrator` field (line ~88):
```rust
/// Optional extension manager for dynamic tool providers.
/// When `None`, only static tools from `tool_registry` are available.
pub extension_manager: Option<ExtensionManager>,
```

- [ ] **Step 2: Add builder method in `run_config_builder.rs`**

Find the builder struct and add:
```rust
pub fn extension_manager(mut self, manager: ExtensionManager) -> Self {
    self.config.extension_manager = Some(manager);
    self
}
```

- [ ] **Step 3: Verify compilation**

Run: `cargo check -p synthia-agent --lib`
Expected: OK

- [ ] **Step 4: Commit: "feat(agent): add ExtensionManager to AgentRunConfig"**

---

## Task 6: Migration — Existing Tool Providers

**Files:**
- Create: `crates/synthia-agent/src/tools/providers/file_tools_provider.rs`
- Create: `crates/synthia-agent/src/tools/providers/mod.rs`
- Modify: `crates/synthia-agent/src/tools/mod.rs`
- Modify: `crates/synthia-agent/src/tools/registry.rs`

- [ ] **Step 1: Create `providers/` directory and `mod.rs`**

```rust
//! Built-in tool providers for migrating static tools.

pub mod file_tools_provider;
```

- [ ] **Step 2: Create `FileToolsProvider`** wrapping file tools (Read, Write, Edit, Glob, Grep)

```rust
use std::sync::Arc;
use async_trait::async_trait;
use crate::tools::tool_provider::{SchemaRef, ToolDefinition, ToolProvider};

pub struct FileToolsProvider;

impl FileToolsProvider {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl ToolProvider for FileToolsProvider {
    fn name(&self) -> &'static str {
        "file_tools"
    }

    fn list_tools(&self) -> Vec<ToolDefinition> {
        vec![
            ToolDefinition {
                name: "read_file".to_string(),
                description: "Read contents of a file".to_string(),
                parameters: SchemaRef::Inline(serde_json::json!({
                    "type": "object",
                    "properties": {
                        "path": {"type": "string"}
                    },
                    "required": ["path"]
                })),
                deprecated: None,
            },
            // ... Write, Edit, Glob, Grep definitions
        ]
    }
}

impl Default for FileToolsProvider {
    fn default() -> Self {
        Self::new()
    }
}
```

- [ ] **Step 3: Verify compilation**

Run: `cargo check -p synthia-agent --lib`
Expected: OK

- [ ] **Step 4: Deprecate `register_defaults()` calls** in `registry.rs`

Add deprecation warning doc comment:
```rust
/// Build a [`ToolRegistry`] pre-populated with the default built-in tool set.
///
/// Deprecated: Use `ExtensionManager` with `FileToolsProvider`, `BashToolsProvider`
/// etc. instead. This function will be removed in a future release.
#[deprecated(since = "0.2.0", note = "Use ExtensionManager with dedicated providers")]
pub fn build_default_tool_registry(...) -> ToolRegistry {
```

- [ ] **Step 5: Commit: "feat(agent): add FileToolsProvider and deprecate build_default_tool_registry"**

---

## Task 7: Documentation and Examples

**Files:**
- Create: `crates/synthia-agent/examples/dynamic_tool_provider.rs`
- Modify: `crates/synthia-agent/src/tools/dynamic_provider.rs` (add doc comments to trait)
- Modify: `AGENTS.md`

- [ ] **Step 1: Add doc comments and example to `ToolProvider` trait**

Add to `dynamic_provider.rs`:
```rust
/// A `ToolProvider` supplies tools at runtime, enabling dynamic registration
/// without recompilation.
///
/// # Example
/// ```ignore
/// use synthia_agent::tools::{ExtensionManager, ToolProvider, StaticToolAdapter};
/// use std::sync::Arc;
///
/// let manager = ExtensionManager::new();
/// manager.register(Arc::new(StaticToolAdapter::new(my_tool)));
/// let tools = manager.list_tools();
/// ```
```

- [ ] **Step 2: Create usage example in `examples/dynamic_tool_provider.rs`**

```rust
//! Example: registering dynamic tools at runtime.

use synthia_agent::tools::{
    ExtensionManager,
    tool_provider::{ToolProvider, ToolDefinition, SchemaRef},
    static_tool_adapter::StaticToolAdapter,
};
use std::sync::Arc;
use async_trait::async_trait;
use synthia_tool::Tool as ToolTrait;

#[tokio::main]
async fn main() {
    let manager = ExtensionManager::new();

    // Register a static tool via adapter
    // let my_tool = Arc::new(MyCustomTool::new());
    // manager.register(Arc::new(StaticToolAdapter::new(my_tool)));

    let tools = manager.list_tools();
    println!("Registered tools: {:?}", tools);
}
```

- [ ] **Step 3: Verify example compiles**

Run: `cargo check --example dynamic_tool_provider -p synthia-agent`
Expected: OK

- [ ] **Step 4: Commit: "docs(agent): add ToolProvider doc comments and dynamic_tool_provider example"**

---

## Spec Coverage Checklist

- [ ] `ToolProvider` trait with `name()`, `list_tools()`, `pre_check()`, `before_execute()`, `after_execute()`, `on_tool_event()` — **Task 1, Steps 4-5**
- [ ] `ExtensionManager` with `Arc<DashMap>` cache and `AtomicU64` version counter — **Task 1, Step 6**
- [ ] `ToolRuntime` wiring `execute_batch()` — **Task 3**
- [ ] `StaticToolAdapter` wrapping `Arc<dyn synthia_tool::Tool>` — **Task 4**
- [ ] `AgentRunConfig` extension_manager field — **Task 5**
- [ ] `FileToolsProvider`, `BashToolsProvider`, `MCPToolsProvider` — **Task 6** (FileToolsProvider done; Bash/MCP deferred per design)
- [ ] `register_defaults()` deprecation — **Task 6, Step 4**
- [ ] Doc comments and example — **Task 7**

---

## Type Consistency Check

| Type | Defined In | Used In |
|------|-----------|---------|
| `ToolPreCheck` | Task 1, Step 4 | `tool_provider.rs` |
| `SchemaRef` | Task 1, Step 4 | `tool_provider.rs`, `StaticToolAdapter` |
| `ToolDefinition` | Task 1, Step 5 | `ExtensionManager::list_tools`, `FileToolsProvider` |
| `ToolProvider` | Task 1, Step 5 | `ExtensionManager`, `StaticToolAdapter`, `FileToolsProvider` |
| `ExtensionManager` | Task 1, Step 6 | `AgentRunConfig`, `ToolRuntime` |
| `ToolRuntime` | Task 3 | Agent integration (future) |
| `StaticToolAdapter` | Task 4 | Example |

All types are consistent across tasks. No naming drift detected.
