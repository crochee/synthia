# Production-Grade Agent Architecture Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use omo-subagent-driven-development (recommended) or omo-dispatching-parallel-agents to implement this plan task-by-task. Each task specifies a `category` (quick/deep/ultrabrain/visual-engineering) and `load_skills` for oh-my-opencode's task() tool. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix critical architectural gaps in Synthia agent vs production-grade agents (OpenCode, Codex, pi-mono) across 5 capabilities: tool cancellation propagation, async permission deferred, scoped tool registry, proactive doom-loop detection, smart compaction agent.

**Architecture:** Five independent subsystems added to existing crates. Tool cancellation extends the Tool trait with CancellationToken propagation. Async permission adds PermissionFuture wrapping oneshot. Scoped registry adds token-based registration with RAII cleanup. DoomLoopDetector uses sliding window of hash signatures. Smart compaction extends ContextAssembler with LLM summarization.

**Tech Stack:** Rust (tokio async, CancellationToken), existing synthia crates (synthia-tool, synthia-permission, synthia-guardian, synthia-context), xxhash for doom-loop signatures.

---

## Task 1: Tool Cancellation Propagation (P0)

**Files:**
- Modify: `crates/synthia-tool/src/traits.rs`
- Modify: `crates/synthia-tool-orchestrator/src/lib.rs`
- Modify: `crates/synthia-agent/src/tools/**/*.rs` (built-in tools)

- [ ] **Step 1: Add `ToolError::Cancelled` variant to `ToolError` enum**

Locate `crates/synthia-tool/src/error.rs` and add:
```rust
#[derive(Debug, Clone)]
pub enum ToolError {
    // ... existing variants
    Cancelled,
}
```

- [ ] **Step 2: Add `CancellationToken` parameter to `call_with_sandbox` trait signature**

Modify `crates/synthia-tool/src/traits.rs`:
```rust
// Change from:
async fn call_with_sandbox(&self, input: Value, sandbox: SandboxAttempt) -> Result<Value, ToolError>
// To:
async fn call_with_sandbox(&self, input: Value, sandbox: SandboxAttempt, token: &CancellationToken) -> Result<Value, ToolError>
```

- [ ] **Step 3: Add `CancellationToken` parameter to `call_with_progress` trait signature**

In `crates/synthia-tool/src/traits.rs`, update `call_with_progress` to accept and pass through token.

- [ ] **Step 4: Fix `ToolAdapter::execute()` to propagate token**

In `crates/synthia-tool-orchestrator/src/lib.rs` line ~935, change `_cancellation_token` to `cancellation_token` (remove underscore) and pass it to `self.tool.call_with_sandbox()`.

- [ ] **Step 5: Fix `ToolAdapter::execute_with_events()` to propagate token**

Pass token through the `call_with_progress` path.

- [ ] **Step 6: Update `ReadTool::call_with_sandbox()` with yield points**

Add chunked reading (64KB chunks) with `tokio::task::yield_now().await` and cancellation checks.

- [ ] **Step 7: Update `WriteTool::call_with_sandbox()` with yield points**

Add chunked writing with yield points.

- [ ] **Step 8: Update `GlobTool` and `GrepTool` with yield points**

Add yield between directory levels / files.

- [ ] **Step 9: Run tests**

```bash
cd /home/crochee/workspace/synthia
cargo test -p synthia-tool --lib 2>&1 | tail -30
cargo test -p synthia-tool-orchestrator --lib 2>&1 | tail -30
```

- [ ] **Step 10: Commit**

```bash
git add -A
git commit -m "feat(tool): add CancellationToken propagation to tool trait"
```

---

## Task 2: Async Permission Deferred

**Files:**
- Create: `crates/synthia-permission/src/permission_future.rs`
- Modify: `crates/synthia-permission/src/traits.rs`
- Modify: `crates/synthia-permission/src/headless.rs`
- Modify: `crates/synthia-tool-orchestrator/src/lib.rs`

- [ ] **Step 1: Create `PermissionFuture` struct with `Future` impl**

Create `crates/synthia-permission/src/permission_future.rs`:
```rust
use tokio::sync::oneshot;
use crate::PermissionResult;

pub struct PermissionFuture {
    rx: oneshot::Receiver<Result<PermissionResult, PermissionFutureError>>,
}

pub enum PermissionFutureError {
    Cancelled,
    Denied,
    Dropped,
}
```

- [ ] **Step 2: Implement `Future` trait for `PermissionFuture`**

```rust
impl Future for PermissionFuture {
    type Output = Result<PermissionResult, PermissionFutureError>;
    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        Pin::new(&mut self.rx).poll(cx)
    }
}
```

- [ ] **Step 3: Add `await_with_cancellation()` method**

```rust
impl PermissionFuture {
    pub async fn await_with_cancellation(
        self,
        token: &CancellationToken,
    ) -> Result<PermissionResult, PermissionFutureError> {
        tokio::select! {
            result = self => result?,
            _ = token.cancelled() => Err(PermissionFutureError::Cancelled),
        }
    }
}
```

- [ ] **Step 4: Add `ask()` to `PermissionService` trait**

In `crates/synthia-permission/src/traits.rs`, add:
```rust
fn ask(&self, request: PermissionRequest) -> PermissionFuture;
```

- [ ] **Step 5: Implement `ask()` for `HeadlessApprovalService`**

Return immediately-denied future:
```rust
impl PermissionService for HeadlessApprovalService {
    fn ask(&self, _request: PermissionRequest) -> PermissionFuture {
        PermissionFuture::immediate_denied()
    }
}
```

- [ ] **Step 6: Update `DefaultToolOrchestrator` to use async `ask()`**

In `crates/synthia-tool-orchestrator/src/lib.rs`, replace `check()` calls with `ask().await_with_cancellation()`.

- [ ] **Step 7: Run tests**

```bash
cargo test -p synthia-permission --lib 2>&1 | tail -20
cargo test -p synthia-tool-orchestrator --lib 2>&1 | tail -20
```

- [ ] **Step 8: Commit**

```bash
git add -A
git commit -m "feat(permission): add async PermissionFuture with Deferred pattern"
```

---

## Task 3: Scoped Tool Registry

**Files:**
- Create: `crates/synthia-tool/src/scoped_registry.rs`
- Modify: `crates/synthia-tool/src/lib.rs`

- [ ] **Step 1: Create `ScopedToolRegistry` struct**

Create `crates/synthia-tool/src/scoped_registry.rs`:
```rust
use std::sync::{Arc, RwLock as StdRwLock};
use dashmap::DashMap;

pub type Token = Arc<()>;

pub struct ScopedRegistration {
    pub token: Token,
    pub tool: Arc<dyn Tool>,
}

pub struct ScopedToolRegistry {
    local: DashMap<String, Vec<ScopedRegistration>>,
    global: Arc<dyn ToolRegistry>,
}

pub struct ScopeGuard {
    token: Token,
    registry: Arc<StdRwLock<ScopedToolRegistry>>,
}
```

- [ ] **Step 2: Implement `register_scoped()` method**

```rust
impl ScopedToolRegistry {
    pub fn register_scoped(&self, tools: Vec<(String, Arc<dyn Tool>)>, token: Token) {
        for (name, tool) in tools {
            self.local.entry(name).or_default().push(ScopedRegistration {
                token: token.clone(),
                tool,
            });
        }
    }
}
```

- [ ] **Step 3: Implement `ScopeGuard` with RAII `Drop`**

```rust
impl Drop for ScopeGuard {
    fn drop(&mut self) {
        let registry = self.registry.write().unwrap();
        for (name, registrations) in registry.local.iter() {
            registrations.retain(|r| !Arc::ptr_eq(&r.token, &self.token));
        }
    }
}
```

- [ ] **Step 4: Implement `materialize()` with last-wins semantics**

Return most recent scoped registration for each tool name.

- [ ] **Step 5: Add `create_scope()` factory**

```rust
impl ScopedToolRegistry {
    pub fn create_scope(global: Arc<dyn ToolRegistry>) -> (Arc<ScopedToolRegistry>, ScopeGuard) {
        let registry = Arc::new(StdRwLock::new(ScopedToolRegistry {
            local: DashMap::new(),
            global,
        }));
        let token = Arc::new(());
        let guard = ScopeGuard { token: token.clone(), registry: registry.clone() };
        (registry, guard)
    }
}
```

- [ ] **Step 6: Run tests**

```bash
cargo test -p synthia-tool --lib scoped 2>&1 | tail -20
```

- [ ] **Step 7: Commit**

```bash
git add -A
git commit -m "feat(tool): add ScopedToolRegistry with RAII cleanup"
```

---

## Task 4: Proactive Doom-Loop Detection

**Files:**
- Create: `crates/synthia-guardian/src/doom_loop_detector.rs`
- Modify: `crates/synthia-guardian/src/lib.rs`

- [ ] **Step 1: Create `DoomLoopDetector` struct with sliding window**

Create `crates/synthia-guardian/src/doom_loop_detector.rs`:
```rust
use std::collections::VecDeque;
use serde::{Deserialize, Serialize};

#[derive(PartialEq, Eq, Hash, Clone)]
struct ToolCallSignature {
    tool_name: String,
    input_hash: u64,
}

pub struct DoomLoopDetector {
    recent: VecDeque<ToolCallSignature>,
    threshold: usize,
}

#[derive(Debug, Clone)]
pub enum LoopStatus {
    Ok,
    Detected { severity: Severity },
}

#[derive(Debug, Clone)]
pub enum Severity { Critical }

#[derive(Debug, Clone)]
pub enum LoopAction {
    RequirePermission,
}
```

- [ ] **Step 2: Implement `check()` method with sliding window detection**

```rust
impl DoomLoopDetector {
    pub fn check(&mut self, tool_name: &str, args: &serde_json::Value) -> (LoopStatus, Option<LoopAction>) {
        let sig = ToolCallSignature {
            tool_name: tool_name.to_string(),
            input_hash: xxhash64(args.to_string().as_bytes()),
        };

        // Check if threshold identical signatures exist
        if self.recent.len() >= self.threshold {
            let all_match = self.recent.iter().take(self.threshold).all(|s| s == &sig);
            if all_match {
                return (LoopStatus::Detected { severity: Severity::Critical }, Some(LoopAction::RequirePermission));
            }
        }

        self.recent.push_back(sig);
        if self.recent.len() > self.threshold {
            self.recent.pop_front();
        }
        (LoopStatus::Ok, None)
    }
}
```

- [ ] **Step 3: Add configurable threshold via `AgentConfig`**

Add `doom_loop_threshold: usize` field (default 3) to `AgentConfig`.

- [ ] **Step 4: Wire `RequirePermission` to permission system**

In agent loop, when `(LoopStatus::Detected, Some(RequirePermission))` is returned, call `permission.ask(doom_loop, ...)`.

- [ ] **Step 5: Run tests**

```bash
cargo test -p synthia-guardian --lib doom_loop 2>&1 | tail -20
```

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "feat(guardian): add DoomLoopDetector with proactive sliding window"
```

---

## Task 5: Smart Compaction Agent

**Files:**
- Modify: `crates/synthia-context/src/assembler.rs`
- Create: `crates/synthia-context/src/smart_compaction.rs`

- [ ] **Step 1: Create `SmartCompactionAgent` with token selection**

Create `crates/synthia-context/src/smart_compaction.rs`:
```rust
pub struct SmartCompactionAgent {
    model: Arc<dyn ModelClient>,
    keep_tokens: usize,
    buffer_tokens: usize,
}

impl SmartCompactionAgent {
    pub fn select_tokens(&self, entries: &[MessageEntry], keep_tokens: usize) -> (Vec<String>, String) {
        // Backward walk, keep newest up to keep_tokens
        // Returns (head_for_summarization, recent_to_preserve)
    }
}
```

- [ ] **Step 2: Implement `summarize()` with LLM call**

```rust
impl SmartCompactionAgent {
    pub async fn summarize(&self, model: &Model, previous: Option<&str>, head: &str) -> Result<String> {
        let prompt = build_summary_prompt(previous, head);
        let response = model.generate(&prompt, GenerationConfig {
            max_tokens: 4096,
            tools: None,
            ..Default::default()
        }).await?;
        Ok(response.text)
    }
}
```

- [ ] **Step 3: Build summary prompt template**

Use OpenCode template: Goal/Progress/Decisions/Next Steps/Critical Context/Relevant Files.

- [ ] **Step 4: Implement incremental chaining**

Include previous summary in prompt for subsequent compactions.

- [ ] **Step 5: Create `compaction` message type**

Add `{ type: "compaction", text: summary, recent: preserved_tail }` to message schema.

- [ ] **Step 6: Add one-shot recovery**

If overflow after compaction, return error instead of compacting again.

- [ ] **Step 7: Run tests**

```bash
cargo test -p synthia-context --lib compaction 2>&1 | tail -20
```

- [ ] **Step 8: Commit**

```bash
git add -A
git commit -m "feat(context): add SmartCompactionAgent with LLM summarization"
```

---

## Task 6: Integration & Verification

- [ ] **Step 1: Full workspace build**

```bash
cargo build --workspace 2>&1 | tail -30
```

- [ ] **Step 2: Clippy checks**

```bash
cargo clippy --all-targets --all-features --tests 2>&1 | grep -E "error|warning" | head -30
```

- [ ] **Step 3: Run workspace tests**

```bash
cargo test --workspace 2>&1 | tail -50
```

- [ ] **Step 4: E2E test for cancellation**

Create `crates/synthia-e2e/tests/cancellation.rs`:
```rust
#[tokio::test]
async fn test_tool_cancellation_mid_execution() {
    // Spawn agent, call long-running tool, cancel mid-execution
    // Verify tool was interrupted
}
```

- [ ] **Step 5: E2E test for doom-loop**

Create `crates/synthia-e2e/tests/doom_loop.rs`:
```rust
#[tokio::test]
async fn test_doom_loop_triggers_permission() {
    // Call same tool 3 times with identical args
    // Verify permission prompt triggered
}
```

- [ ] **Step 6: Final commit**

```bash
git add -A
git commit -m "test(e2e): add cancellation and doom-loop integration tests"
```
