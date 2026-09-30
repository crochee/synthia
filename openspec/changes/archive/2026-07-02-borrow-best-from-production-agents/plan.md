# Borrow Best from Production Agents Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 修复 synthia 与生产级 AI agent（opencode/codex/pi-mono）的差距，先修 H1/H4 静默风险，再分 4 阶段引入 13 项借鉴特性，遵循 agent_rule.md P1-P10 原则。

**Architecture:** 5 阶段渐进交付（修复 → 小改动 → 中期补强 → 长期架构 → Tool 化）。每阶段独立可验证 + 独立 commit + 独立 rollback。所有改动遵循 P1 前缀一致性（Arc::ptr_eq 短路、SystemContext typed source）、P6 不信任 LLM（tool 兜底机制）、P10 文件即记忆（无 SQLite）。

**Tech Stack:** Rust + tokio::sync::Mutex + Arc::ptr_eq + std::ops::ControlFlow + tracing + opentelemetry（可选 feature）

---

## File Structure

**新增文件：**
- `crates/synthia-tool-exec-base/src/file_mutation_queue.rs` — per-filepath 串行化队列
- `crates/synthia-context/src/anchored_summary.rs` — 8 段式 summary 模板与 prompt
- `crates/synthia-context/src/system_context/mod.rs` — SystemContext 注册器 + Snapshot
- `crates/synthia-context/src/system_context/source.rs` — Source trait
- `crates/synthia-context/src/system_context/environment_source.rs` — 首个示例 source
- `crates/synthia-context/src/system_context/reconcile.rs` — reconcile 状态机
- `crates/synthia-provider/src/context_overflow.rs` — ContextOverflowDetector
- `crates/synthia-telemetry/src/compaction_analytics.rs` — CompactionAnalyticsAttempt
- `crates/synthia-telemetry/src/span_attributes_processor.rs` — SpanAttributesProcessor
- `crates/synthia-agent/src/turn_transition.rs` — TurnTransition defect 通道
- `crates/synthia-guardian/src/tool.rs` — self_reflect tool 定义
- `crates/synthia-context/src/compaction_tool.rs` — compact_context tool 定义

**修改文件：**
- `crates/synthia-agent/src/agent.rs` — D1 run_stream 自动装配
- `crates/synthia-agent/src/stream_builder/builder/run/main_loop.rs:191-194` — D2 LoopContext 完整恢复
- `crates/synthia-provider/src/cache_policy.rs` — D3 引用相等短路
- `crates/synthia-tool-exec-base/src/lib.rs` — D4 FileMutationQueue 导出 + ToolAdapter 集成
- `crates/synthia-permission/src/manager.rs` — D5 "always" 传播 + "reject" 级联
- `crates/synthia-context/src/compaction.rs` — D6 Anchored Summary 集成
- `crates/synthia-telemetry/src/lib.rs` — D9/D10 新模块导出
- `crates/synthia-agent/src/stream_builder/builder/run/main_loop.rs` — D12/D13 tool 注册 + 兜底
- `crates/synthia-guardian/src/lib.rs` — D12 self_reflect tool 导出

---

## Task 1: H1 — Agent::run_stream auto-assemble tool orchestrator

**Files:**
- Modify: `crates/synthia-agent/src/agent.rs` (run_stream 方法)
- Test: `crates/synthia-agent/tests/run_stream_auto_assemble.rs`

- [ ] **Step 1: Write failing test for auto-assembly**

```rust
// crates/synthia-agent/tests/run_stream_auto_assemble.rs
use synthia_agent::Agent;
use synthia_core::config::AgentConfig;

#[tokio::test]
async fn run_stream_auto_assembles_orchestrator_when_none() {
    let agent = Agent::new(AgentConfig::default_for_test());
    // Call run_stream without orchestrator
    let result = agent.run_stream("hello", None).await;
    assert!(result.is_ok(), "run_stream should succeed with auto-assembly");
}

#[tokio::test]
async fn run_stream_warns_when_auto_assembling() {
    let agent = Agent::new(AgentConfig::default_for_test());
    let _ = agent.run_stream("hello", None).await;
    // Verify warning log via tracing capture (use tracing-test or similar)
    // The warning must contain "auto-assembled tool orchestrator"
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p synthia-agent --test run_stream_auto_assemble`
Expected: FAIL — `run_stream` does not accept `None` orchestrator or panics

- [ ] **Step 3: Implement auto-assembly branch in run_stream**

```rust
// crates/synthia-agent/src/agent.rs — in run_stream method
pub async fn run_stream(
    &mut self,
    input: &str,
    tool_orchestrator: Option<ToolOrchestrator>,
) -> Result<RunResult, AgentError> {
    let orchestrator = match tool_orchestrator {
        Some(orch) => orch,
        None => {
            tracing::warn!("auto-assembled tool orchestrator (caller did not inject one)");
            assemble_default_tool_orchestrator(self.config.clone())
                .map_err(|e| AgentError::OrchestratorAssembly(format!(
                    "failed to assemble default tool orchestrator: {e}"
                )))?
        }
    };
    // ... existing run_stream logic using `orchestrator`
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p synthia-agent --test run_stream_auto_assemble`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/synthia-agent/src/agent.rs crates/synthia-agent/tests/run_stream_auto_assemble.rs
git commit -m "fix(agent): auto-assemble tool orchestrator in run_stream when not injected (H1)"
```

---

## Task 2: H4 — LoopContext full restoration via from_metadata

**Files:**
- Modify: `crates/synthia-agent/src/stream_builder/builder/run/main_loop.rs:191-194`
- Test: `crates/synthia-agent/tests/loop_context_restoration.rs`

- [ ] **Step 1: Write failing test for full restoration**

```rust
// crates/synthia-agent/tests/loop_context_restoration.rs
use synthia_agent::loop_context::{LoopContext, SessionMetadata};
use synthia_agent::stream_builder::builder::run::main_loop;

#[test]
fn loop_context_restores_all_4_fields_from_metadata() {
    let metadata = SessionMetadata {
        iteration: 50,
        end_reason: Some("DoomLoopDetected".into()),
        cumulative_tokens: 50000,
        context_token_limit: 100000,
    };
    let ctx = LoopContext::from_metadata(&metadata);
    assert_eq!(ctx.iteration, 50);
    assert_eq!(ctx.end_reason.as_deref(), Some("DoomLoopDetected"));
    assert_eq!(ctx.cumulative_tokens, 50000);
    assert_eq!(ctx.context_token_limit, 100000);
}
```

- [ ] **Step 2: Run test to verify current state**

Run: `cargo test -p synthia-agent --test loop_context_restoration`
Expected: PASS if `from_metadata` already exists (per design.md API is complete); the bug is in main_loop.rs not calling it.

- [ ] **Step 3: Fix main_loop.rs to use from_metadata**

```rust
// crates/synthia-agent/src/stream_builder/builder/run/main_loop.rs:191-194
// BEFORE (manual 2-field restoration):
// let mut loop_ctx = LoopContext::new();
// loop_ctx.iteration = metadata.iteration;
// loop_ctx.end_reason = metadata.end_reason.clone();

// AFTER (full restoration via from_metadata):
let mut loop_ctx = LoopContext::from_metadata(metadata);
```

- [ ] **Step 4: Add integration test for doom_loop continuity**

```rust
#[tokio::test]
async fn resumed_session_with_max_iterations_immediately_stops() {
    // Setup: metadata.iteration = 50, max_iterations = 50
    // Resume session
    // Assert: next loop check triggers MaxIterationsReached without executing full iteration
}
```

- [ ] **Step 5: Run all tests + clippy**

Run: `cargo test -p synthia-agent && cargo clippy -p synthia-agent --all-features`
Expected: PASS, zero warnings

- [ ] **Step 6: Commit**

```bash
git add crates/synthia-agent/src/stream_builder/builder/run/main_loop.rs crates/synthia-agent/tests/loop_context_restoration.rs
git commit -m "fix(agent): restore LoopContext via from_metadata with all 4 fields (H4)"
```

---

## Task 3: Cache Policy reference equality short-circuit

**Files:**
- Modify: `crates/synthia-provider/src/cache_policy.rs`
- Test: `crates/synthia-provider/tests/cache_policy_short_circuit.rs`

- [ ] **Step 1: Write failing test**

```rust
// crates/synthia-provider/tests/cache_policy_short_circuit.rs
use std::sync::Arc;
use synthia_provider::cache_policy::apply_cache_policy;

#[test]
fn returns_original_when_all_arcs_ptr_eq() {
    let tools = Arc::new(vec![]);
    let system = Arc::new("system".to_string());
    let messages = Arc::new(vec![]);
    let cached = apply_cache_policy(tools.clone(), system.clone(), messages.clone());
    let result = apply_cache_policy(tools, system, messages);
    assert!(Arc::ptr_eq(&result.tools, &cached.tools));
    assert!(Arc::ptr_eq(&result.system, &cached.system));
    assert!(Arc::ptr_eq(&result.messages, &cached.messages));
}

#[test]
fn reallocates_when_tools_arc_changed() {
    let tools1 = Arc::new(vec![]);
    let tools2 = Arc::new(vec!["new_tool".to_string()]);
    let system = Arc::new("system".to_string());
    let messages = Arc::new(vec![]);
    let _cached = apply_cache_policy(tools1, system.clone(), messages.clone());
    let result = apply_cache_policy(tools2, system, messages);
    // Should NOT be ptr_eq since tools changed
    // (verification depends on internal caching state)
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p synthia-provider --test cache_policy_short_circuit`
Expected: FAIL — current implementation always rebuilds

- [ ] **Step 3: Implement short-circuit in apply_cache_policy**

```rust
// crates/synthia-provider/src/cache_policy.rs
pub fn apply_cache_policy(
    tools: Arc<Vec<ToolSchema>>,
    system: Arc<String>,
    messages: Arc<Vec<Message>>,
) -> CachedRequest {
    // Short-circuit: if all three Arcs are ptr_eq to previous, return cached
    if let Some(prev) = &CACHED_PREVIOUS.with(|c| c.borrow().clone()) {
        if Arc::ptr_eq(&tools, &prev.tools)
            && Arc::ptr_eq(&system, &prev.system)
            && Arc::ptr_eq(&messages, &prev.messages)
        {
            return prev.clone();
        }
    }
    // Full evaluation path
    let new_request = /* ... existing logic ... */;
    CACHED_PREVIOUS.with(|c| *c.borrow_mut() = Some(new_request.clone()));
    new_request
}
```

- [ ] **Step 4: Run test + clippy**

Run: `cargo test -p synthia-provider --test cache_policy_short_circuit && cargo clippy -p synthia-provider`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/synthia-provider/src/cache_policy.rs crates/synthia-provider/tests/cache_policy_short_circuit.rs
git commit -m "perf(provider): short-circuit cache policy on Arc::ptr_eq (D3)"
```

---

## Task 4: FileMutationQueue type + per-filepath mutex

**Files:**
- Create: `crates/synthia-tool-exec-base/src/file_mutation_queue.rs`
- Modify: `crates/synthia-tool-exec-base/src/lib.rs`
- Test: `crates/synthia-tool-exec-base/tests/file_mutation_queue.rs`

- [ ] **Step 1: Write failing test for serialization**

```rust
// crates/synthia-tool-exec-base/tests/file_mutation_queue.rs
use synthia_tool_exec_base::file_mutation_queue::FileMutationQueue;
use std::path::PathBuf;

#[tokio::test]
async fn same_filepath_serializes() {
    let queue = FileMutationQueue::new();
    let path = PathBuf::from("/tmp/test_same.txt");
    let _guard1 = queue.acquire(path.clone()).await;
    // Second acquire should block
    let handle = tokio::spawn({
        let queue = queue.clone();
        let path = path.clone();
        async move { queue.acquire(path).await }
    });
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    assert!(!handle.is_finished(), "second acquire should block");
    drop(_guard1);
    handle.await.unwrap(); // Now should complete
}

#[tokio::test]
async fn different_filepaths_parallel() {
    let queue = FileMutationQueue::new();
    let g1 = queue.acquire(PathBuf::from("/tmp/a.txt")).await;
    let g2 = queue.acquire(PathBuf::from("/tmp/b.txt")).await;
    // Both acquired immediately, no blocking
    drop(g1);
    drop(g2);
}

#[tokio::test]
async fn symlink_shares_realpath_key() {
    // Create symlink /tmp/link -> /tmp/real.txt
    // Acquire on /tmp/link, then /tmp/real.txt should block
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p synthia-tool-exec-base --test file_mutation_queue`
Expected: FAIL — module doesn't exist

- [ ] **Step 3: Implement FileMutationQueue**

```rust
// crates/synthia-tool-exec-base/src/file_mutation_queue.rs
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Clone, Default)]
pub struct FileMutationQueue {
    inner: Arc<Mutex<HashMap<PathBuf, Arc<Mutex<()>>>>>,
}

impl FileMutationQueue {
    pub fn new() -> Self { Self::default() }

    pub async fn acquire(&self, path: PathBuf) -> FileGuard {
        let canonical = std::fs::canonicalize(&path)
            .unwrap_or(path);
        let mutex = {
            let mut map = self.inner.lock().await;
            map.entry(canonical.clone())
                .or_insert_with(|| Arc::new(Mutex::new(())))
                .clone()
        };
        let _guard = mutex.lock().await;
        FileGuard {
            queue: self.inner.clone(),
            key: canonical,
            _guard,
        }
    }
}

pub struct FileGuard {
    queue: Arc<Mutex<HashMap<PathBuf, Arc<Mutex<()>>>>>,
    key: PathBuf,
    _guard: tokio::sync::MutexGuard<'static, ()>,
}

impl Drop for FileGuard {
    fn drop(&mut self) {
        // Schedule cleanup via try_lock to avoid blocking
        let queue = self.queue.clone();
        let key = self.key.clone();
        tokio::spawn(async move {
            let mut map = queue.lock().await;
            if let Some(mutex) = map.get(&key) {
                if Arc::strong_count(mutex) == 1 {
                    map.remove(&key);
                }
            }
        });
    }
}
```

- [ ] **Step 4: Export in lib.rs**

```rust
// crates/synthia-tool-exec-base/src/lib.rs
pub mod file_mutation_queue;
pub use file_mutation_queue::{FileMutationQueue, FileGuard};
```

- [ ] **Step 5: Run tests + clippy**

Run: `cargo test -p synthia-tool-exec-base && cargo clippy -p synthia-tool-exec-base`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add crates/synthia-tool-exec-base/src/file_mutation_queue.rs crates/synthia-tool-exec-base/src/lib.rs crates/synthia-tool-exec-base/tests/file_mutation_queue.rs
git commit -m "feat(tool-exec): FileMutationQueue per-filepath serialization (D4)"
```

---

## Task 5: Integrate FileMutationQueue into ToolAdapter

**Files:**
- Modify: `crates/synthia-tool-exec-base/src/adapter.rs` (ToolAdapter::execute)
- Test: `crates/synthia-tool-exec-base/tests/adapter_integration.rs`

- [ ] **Step 1: Write integration test**

```rust
#[tokio::test]
async fn write_file_acquires_mutation_queue() {
    // Setup ToolAdapter with FileMutationQueue
    // Call write_file tool
    // Verify queue was acquired (via mock or spy)
}
```

- [ ] **Step 2: Modify ToolAdapter::execute**

```rust
// crates/synthia-tool-exec-base/src/adapter.rs
pub struct ToolAdapter {
    // ... existing fields
    file_mutation_queue: FileMutationQueue,
}

impl ToolAdapter {
    pub async fn execute(&self, tool: &dyn Tool, input: ToolInput) -> ToolResult {
        // For file-mutating tools, acquire queue
        let needs_queue = matches!(tool.name().as_str(),
            "write_file" | "apply_patch" | "edit_file");
        let _guard = if needs_queue {
            let path = input.get_path()?;
            Some(self.file_mutation_queue.acquire(path).await)
        } else {
            None
        };
        // ... existing execute logic
    }
}
```

- [ ] **Step 3: Run tests + commit**

```bash
cargo test -p synthia-tool-exec-base
git add -A && git commit -m "feat(tool-exec): integrate FileMutationQueue into ToolAdapter (D4)"
```

---

## Task 6: Permission "always" propagation + "reject" cascade

**Files:**
- Modify: `crates/synthia-permission/src/manager.rs`
- Test: `crates/synthia-permission/tests/propagation.rs`

- [ ] **Step 1: Write failing tests**

```rust
// crates/synthia-permission/tests/propagation.rs
#[tokio::test]
async fn always_allow_auto_resolves_identical_pending() {
    // Setup: two pending requests with identical resources
    // User selects "always allow" on first
    // Assert: second is auto-resolved without prompt
}

#[tokio::test]
async fn always_allow_does_not_resolve_overlapping() {
    // Two pending: resources ["ls"] and ["ls", "pwd"]
    // User selects "always allow" for ["ls"]
    // Assert: second still prompts
}

#[tokio::test]
async fn reject_cascades_to_same_session_pending() {
    // Three pending in session A
    // User rejects one
    // Assert: all three terminated with "cascade-from-session-reject"
}

#[tokio::test]
async fn reject_does_not_cross_session() {
    // Pending in session A and B
    // Reject in A
    // Assert: B's pending unaffected
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p synthia-permission --test propagation`
Expected: FAIL

- [ ] **Step 3: Implement propagation logic**

```rust
// crates/synthia-permission/src/manager.rs
impl PermissionManager {
    pub async fn handle_user_decision(&mut self, decision: UserDecision) {
        match decision.choice {
            Choice::AlwaysAllow => {
                self.save_rule(decision.tool, decision.resources.clone());
                self.propagate_allow_to_pending(&decision.resources).await;
            }
            Choice::Reject | Choice::AlwaysReject => {
                self.cascade_reject_in_session(decision.session_id).await;
            }
            _ => {}
        }
    }

    async fn propagate_allow_to_pending(&mut self, resources: &[Resource]) {
        let mut to_resolve = Vec::new();
        for pending in self.pending.iter() {
            if pending.resources.iter().all(|r| resources.contains(r)) {
                to_resolve.push(pending.id);
            }
        }
        for id in to_resolve {
            self.resolve(id, Decision::Allowed);
        }
    }

    async fn cascade_reject_in_session(&mut self, session_id: SessionId) {
        let to_reject: Vec<_> = self.pending.iter()
            .filter(|p| p.session_id == session_id)
            .map(|p| p.id)
            .collect();
        for id in to_reject {
            self.resolve_with_reason(id, Decision::Rejected, "cascade-from-session-reject");
        }
    }
}
```

- [ ] **Step 4: Run tests + clippy + commit**

```bash
cargo test -p synthia-permission && cargo clippy -p synthia-permission
git add -A && git commit -m "feat(permission): always-allow propagation + reject cascade (D5)"
```

---

## Task 7: Anchored Summary 8-section template

**Files:**
- Create: `crates/synthia-context/src/anchored_summary.rs`
- Modify: `crates/synthia-context/src/compaction.rs`
- Test: `crates/synthia-context/tests/anchored_summary.rs`

- [ ] **Step 1: Write failing test for 8-section structure**

```rust
// crates/synthia-context/tests/anchored_summary.rs
use synthia_context::anchored_summary::{AnchoredSummary, SUMMARY_TEMPLATE};

#[test]
fn template_has_8_sections() {
    let sections = SUMMARY_TEMPLATE.sections;
    assert_eq!(sections.len(), 8);
    assert_eq!(sections[0], "Goal");
    assert_eq!(sections[1], "Constraints");
    assert_eq!(sections[2], "Progress");
    assert_eq!(sections[3], "Key Decisions");
    assert_eq!(sections[4], "Next Steps");
    assert_eq!(sections[5], "Critical Context");
    assert_eq!(sections[6], "Relevant Files");
    // 8th is implicit (Relevant Files + closing)
}

#[test]
fn empty_section_uses_placeholder() {
    let summary = AnchoredSummary::parse("Goal: build auth\nConstraints: _(none)_");
    assert_eq!(summary.get("Constraints"), Some("_(none)_"));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p synthia-context --test anchored_summary`
Expected: FAIL — module doesn't exist

- [ ] **Step 3: Implement AnchoredSummary**

```rust
// crates/synthia-context/src/anchored_summary.rs
use serde::{Serialize, Deserialize};

pub const SECTIONS: [&str; 8] = [
    "Goal",
    "Constraints",
    "Progress",  // with Done/InProgress/Blocked subsections
    "Key Decisions",
    "Next Steps",
    "Critical Context",
    "Relevant Files",
    "Closing",  // 8th section
];

pub const PLACEHOLDER: &str = "_(none)_";

pub fn generate_prompt(previous_summary: Option<&str>) -> String {
    match previous_summary {
        Some(prev) => format!(
            "Update the anchored summary with the following 8 sections.\n\
             Previous summary:\n{prev}\n\n\
             Sections: Goal, Constraints, Progress (Done/InProgress/Blocked), \
             Key Decisions, Next Steps, Critical Context, Relevant Files, Closing.\n\
             Preserve unchanged sections. Use '{PLACEHOLDER}' for empty sections."
        ),
        None => format!(
            "Generate the anchored summary with the following 8 sections.\n\
             Sections: Goal, Constraints, Progress (Done/InProgress/Blocked), \
             Key Decisions, Next Steps, Critical Context, Relevant Files, Closing.\n\
             Use '{PLACEHOLDER}' for empty sections."
        ),
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AnchoredSummary {
    sections: std::collections::BTreeMap<String, String>,
}

impl AnchoredSummary {
    pub fn parse(text: &str) -> Self { /* parse 8 sections */ }
    pub fn get(&self, section: &str) -> Option<&str> { /* ... */ }
}
```

- [ ] **Step 4: Implement token-budget aware split**

```rust
pub fn split_at_token_budget(
    previous_summary: &str,
    new_messages: &[Message],
    budget: usize,
) -> Vec<Message> {
    let mut result = Vec::new();
    let mut used = previous_summary.len(); // simplified; use tokenizer in production
    for msg in new_messages {
        let msg_tokens = msg.estimated_tokens();
        if used + msg_tokens > budget {
            if result.is_empty() {
                // Single message exceeds budget — mid-message slice
                let sliced = msg.slice_at(budget - used);
                result.push(sliced.with_marker("[truncated-mid-message]"));
            }
            break;
        }
        result.push(msg.clone());
        used += msg_tokens;
    }
    result
}
```

- [ ] **Step 5: Run tests + clippy + commit**

```bash
cargo test -p synthia-context && cargo clippy -p synthia-context
git add -A && git commit -m "feat(context): Anchored Summary 8-section template + token-aware split (D6)"
```

---

## Task 8: ContextOverflowDetector

**Files:**
- Create: `crates/synthia-provider/src/context_overflow.rs`
- Test: `crates/synthia-provider/tests/context_overflow.rs`

- [ ] **Step 1: Write failing tests for 21 regex patterns**

```rust
// crates/synthia-provider/tests/context_overflow.rs
use synthia_provider::context_overflow::ContextOverflowDetector;

#[test]
fn detects_anthropic_overflow() {
    let det = ContextOverflowDetector::default();
    assert!(det.is_overflow("context length exceeded"));
    assert!(det.is_overflow("prompt is too long"));
}

#[test]
fn detects_openai_overflow() {
    let det = ContextOverflowDetector::default();
    assert!(det.is_overflow("maximum context length exceeded"));
}

#[test]
fn excludes_rate_limit() {
    let det = ContextOverflowDetector::default();
    assert!(!det.is_overflow("Rate limit exceeded, retry after 30s"));
    assert!(!det.is_overflow("Too many requests"));
    assert!(!det.is_overflow("Request throttled"));
}

#[test]
fn detects_silent_overflow() {
    let det = ContextOverflowDetector::default();
    let usage = Usage { input_tokens: 50000, cache_read_tokens: 80000, ..Default::default() };
    assert!(det.is_silent_overflow(&usage, 100000));
    assert!(!det.is_silent_overflow(&usage, 200000));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p synthia-provider --test context_overflow`
Expected: FAIL

- [ ] **Step 3: Implement detector**

```rust
// crates/synthia-provider/src/context_overflow.rs
use regex::Regex;
use once_cell::sync::Lazy;

static OVERFLOW_PATTERNS: Lazy<Vec<Regex>> = Lazy::new(|| {
    [
        // Anthropic (5)
        r"(?i)context length exceeded",
        r"(?i)prompt is too long",
        // OpenAI (5)
        r"(?i)maximum context length",
        r"(?i)this model.*maximum context",
        // Google (4)
        r"(?i)exceeds.*context window",
        // Other providers (7)
        r"(?i)input too long",
        r"(?i)token limit exceeded",
        // ... 21 total patterns (full list in source)
    ].iter().map(|p| Regex::new(p).unwrap()).collect()
});

static EXCLUSION_PATTERNS: Lazy<Vec<Regex>> = Lazy::new(|| {
    [
        r"(?i)rate limit",
        r"(?i)too many requests",
        r"(?i)throttl",
    ].iter().map(|p| Regex::new(p).unwrap()).collect()
});

pub struct ContextOverflowDetector;

impl ContextOverflowDetector {
    pub fn is_overflow(&self, error_message: &str) -> bool {
        // Check exclusions first
        if EXCLUSION_PATTERNS.iter().any(|r| r.is_match(error_message)) {
            return false;
        }
        OVERFLOW_PATTERNS.iter().any(|r| r.is_match(error_message))
    }

    pub fn is_silent_overflow(&self, usage: &Usage, context_window: usize) -> bool {
        usage.input_tokens + usage.cache_read_tokens > context_window
    }

    pub fn synthesize_orphan_result(tool_use_id: &str) -> ToolResult {
        ToolResult {
            tool_use_id: tool_use_id.to_string(),
            content: "[orphan tool call - result truncated]".to_string(),
        }
    }
}
```

- [ ] **Step 4: Run tests + clippy + commit**

```bash
cargo test -p synthia-provider && cargo clippy -p synthia-provider
git add -A && git commit -m "feat(provider): ContextOverflowDetector with 21 patterns + silent overflow (D7)"
```

---

## Task 9: TurnTransition defect channel

**Files:**
- Create: `crates/synthia-agent/src/turn_transition.rs`
- Modify: `crates/synthia-agent/src/stream_builder/builder/run/main_loop.rs`
- Test: `crates/synthia-agent/tests/turn_transition.rs`

- [ ] **Step 1: Write failing tests**

```rust
// crates/synthia-agent/tests/turn_transition.rs
use synthia_agent::turn_transition::{TurnTransition, handle_defect};
use std::ops::ControlFlow;

#[tokio::test]
async fn continue_defect_triggers_retry() {
    let result = handle_defect(ControlFlow::Continue(TurnTransition::ContextOverflow)).await;
    assert!(matches!(result.action, DefectAction::Retry));
}

#[tokio::test]
async fn break_defect_terminates() {
    let result = handle_defect(ControlFlow::Break(TurnTransition::FatalError("test".into()))).await;
    assert!(matches!(result.action, DefectAction::Terminate(_)));
}

#[tokio::test]
async fn fourth_retry_rejected() {
    // After 3 retries, 4th attempt is rejected
    let mut counter = 3;
    let result = handle_defect_with_count(ControlFlow::Continue(TurnTransition::ContextOverflow), &mut counter).await;
    assert!(matches!(result.action, DefectAction::Terminate(_)));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p synthia-agent --test turn_transition`
Expected: FAIL

- [ ] **Step 3: Implement TurnTransition**

```rust
// crates/synthia-agent/src/turn_transition.rs
use std::ops::ControlFlow;

#[derive(Debug)]
pub enum TurnTransition {
    ContextOverflow,
    ToolExecutionFailure(String),
    FatalError(String),
}

pub type TurnResult<T> = Result<T, ControlFlow<TurnTransition>>;

pub const MAX_DEFECT_RETRIES: u32 = 3;

#[derive(Debug)]
pub enum DefectAction {
    Retry,
    Terminate(String),
}

pub async fn handle_defect(defect: ControlFlow<TurnTransition>) -> DefectAction {
    handle_defect_with_count(defect, &mut 0)
}

pub async fn handle_defect_with_count(
    defect: ControlFlow<TurnTransition>,
    retry_count: &mut u32,
) -> DefectAction {
    match defect {
        ControlFlow::Continue(_) => {
            if *retry_count >= MAX_DEFECT_RETRIES {
                DefectAction::Terminate("max defect retries (3) exceeded".into())
            } else {
                *retry_count += 1;
                DefectAction::Retry
            }
        }
        ControlFlow::Break(t) => DefectAction::Terminate(format!("{t:?}")),
    }
}
```

- [ ] **Step 4: Integrate into main_loop.rs turn execution**

```rust
// In main_loop.rs turn execution
let mut retry_count = 0u32;
loop {
    match execute_turn(/* ... */).await {
        Ok(output) => break Ok(output),
        Err(ControlFlow::Continue(defect)) => {
            match handle_defect_with_count(
                ControlFlow::Continue(defect), &mut retry_count
            ).await {
                DefectAction::Retry => {
                    // Run compaction if ContextOverflow
                    continue;
                }
                DefectAction::Terminate(msg) => break Err(msg),
            }
        }
        Err(ControlFlow::Break(defect)) => {
            break Err(format!("{defect:?}"));
        }
    }
}
```

- [ ] **Step 5: Run tests + clippy + commit**

```bash
cargo test -p synthia-agent && cargo clippy -p synthia-agent --all-features
git add -A && git commit -m "feat(agent): TurnTransition defect channel with 3-retry cap (D8)"
```

---

## Task 10: CompactionAnalyticsAttempt

**Files:**
- Create: `crates/synthia-telemetry/src/compaction_analytics.rs`
- Modify: `crates/synthia-telemetry/src/lib.rs` + compaction stages in `synthia-context`
- Test: `crates/synthia-telemetry/tests/compaction_analytics.rs`

- [ ] **Step 1: Write failing tests**

```rust
// crates/synthia-telemetry/tests/compaction_analytics.rs
use synthia_telemetry::compaction_analytics::{CompactionAnalyticsAttempt, CompactionTrigger};

#[test]
fn record_has_5_fields() {
    let attempt = CompactionAnalyticsAttempt {
        active_context_tokens_before: 102400,
        trigger: CompactionTrigger::AutoThreshold,
        reason: "context-usage-80-percent".to_string(),
        implementation: "stage1-soft-trim".to_string(),
        phase: "head-tail".to_string(),
    };
    assert_eq!(attempt.active_context_tokens_before, 102400);
    assert_eq!(attempt.trigger, CompactionTrigger::AutoThreshold);
}

#[test]
fn otel_emission_sets_5_attributes() {
    // With otel feature enabled
    // Verify span has 5 attributes prefixed with "compaction."
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p synthia-telemetry --features otel --test compaction_analytics`
Expected: FAIL

- [ ] **Step 3: Implement struct**

```rust
// crates/synthia-telemetry/src/compaction_analytics.rs
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub enum CompactionTrigger {
    AutoThreshold,
    ToolCall,
    Manual,
}

#[derive(Debug, Clone, Serialize)]
pub struct CompactionAnalyticsAttempt {
    pub active_context_tokens_before: usize,
    pub trigger: CompactionTrigger,
    pub reason: String,
    pub implementation: String,  // "stage1-soft-trim" | "stage2-hard-clear" | ...
    pub phase: String,           // "head-tail" | "replace" | "compress"
}

impl CompactionAnalyticsAttempt {
    #[cfg(feature = "otel")]
    pub fn emit_to_span(&self, span: &opentelemetry::trace::Span) {
        span.set_attribute(KeyValue::new("compaction.active_context_tokens_before", self.active_context_tokens_before as i64));
        span.set_attribute(KeyValue::new("compaction.trigger", format!("{:?}", self.trigger)));
        span.set_attribute(KeyValue::new("compaction.reason", self.reason.clone()));
        span.set_attribute(KeyValue::new("compaction.implementation", self.implementation.clone()));
        span.set_attribute(KeyValue::new("compaction.phase", self.phase.clone()));
    }

    pub fn emit(&self) {
        #[cfg(feature = "otel")]
        {
            if let Some(span) = opentelemetry::trace::Span::current() {
                self.emit_to_span(&span);
            }
        }
        tracing::info!(
            active_context_tokens_before = self.active_context_tokens_before,
            trigger = ?self.trigger,
            reason = %self.reason,
            implementation = %self.implementation,
            phase = %self.phase,
            "compaction_attempt"
        );
    }
}
```

- [ ] **Step 4: Add emit calls in compaction stages**

```rust
// In synthia-context compaction.rs Stage 1
let attempt = CompactionAnalyticsAttempt {
    active_context_tokens_before,
    trigger: CompactionTrigger::AutoThreshold,
    reason: "context-usage-80-percent".to_string(),
    implementation: "stage1-soft-trim".to_string(),
    phase: "head-tail".to_string(),
};
attempt.emit();
```

- [ ] **Step 5: Run tests + clippy + commit**

```bash
cargo test -p synthia-telemetry --features otel && cargo clippy -p synthia-telemetry --all-features
git add -A && git commit -m "feat(telemetry): CompactionAnalyticsAttempt with 5 fields + OTel emission (D9)"
```

---

## Task 11: SpanAttributesProcessor

**Files:**
- Create: `crates/synthia-telemetry/src/span_attributes_processor.rs`
- Test: `crates/synthia-telemetry/tests/span_attributes_processor.rs`

- [ ] **Step 1: Write failing tests**

```rust
#[test]
#[cfg(feature = "otel")]
fn on_start_injects_6_attributes() {
    // Create span with processor
    // Verify 6 attributes set: session.id, user.id, agent.id, turn.id, gen_ai.system, gen_ai.request.model
}

#[test]
#[cfg(feature = "otel")]
fn missing_context_uses_empty_string() {
    // user.id not available
    // Assert: user.id attribute is ""
}

#[test]
fn no_statsig_dependency() {
    // grep Cargo.toml + binary for "statsig"
    // Assert: no statsig symbol
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p synthia-telemetry --features otel --test span_attributes_processor`
Expected: FAIL

- [ ] **Step 3: Implement processor**

```rust
// crates/synthia-telemetry/src/span_attributes_processor.rs
use opentelemetry::KeyValue;
use tracing_subscriber::Layer;

pub struct SpanAttributesProcessor {
    session_id: Option<String>,
    user_id: Option<String>,
    agent_id: Option<String>,
    turn_id: Option<String>,
    gen_ai_system: String,
    gen_ai_request_model: Option<String>,
}

impl SpanAttributesProcessor {
    pub fn new(
        session_id: Option<String>,
        user_id: Option<String>,
        agent_id: Option<String>,
        turn_id: Option<String>,
        gen_ai_system: String,
        gen_ai_request_model: Option<String>,
    ) -> Self {
        Self {
            session_id, user_id, agent_id, turn_id,
            gen_ai_system, gen_ai_request_model,
        }
    }

    fn attributes(&self) -> Vec<KeyValue> {
        vec![
            KeyValue::new("session.id", self.session_id.clone().unwrap_or_default()),
            KeyValue::new("user.id", self.user_id.clone().unwrap_or_default()),
            KeyValue::new("agent.id", self.agent_id.clone().unwrap_or_default()),
            KeyValue::new("turn.id", self.turn_id.clone().unwrap_or_default()),
            KeyValue::new("gen_ai.system", self.gen_ai_system.clone()),
            KeyValue::new("gen_ai.request.model", self.gen_ai_request_model.clone().unwrap_or_default()),
        ]
    }
}

// Note: implement tracing_subscriber::Layer or opentelemetry trace processor
// depending on synthia-telemetry's existing OTel integration approach
```

- [ ] **Step 4: Verify no Statsig code**

```bash
grep -r "statsig" crates/synthia-telemetry/
# Expected: no matches
```

- [ ] **Step 5: Verify OTLP exporter compatibility**

```bash
# Test with grpc:// scheme
SYNTHIA_OTLP_ENDPOINT="grpc://collector:4317" cargo test -p synthia-telemetry --features otel
# Test with http:// scheme
SYNTHIA_OTLP_ENDPOINT="http://collector:4318" cargo test -p synthia-telemetry --features otel
```

- [ ] **Step 6: Run tests + clippy + commit**

```bash
cargo test -p synthia-telemetry --features otel && cargo clippy -p synthia-telemetry --all-features
git add -A && git commit -m "feat(telemetry): SpanAttributesProcessor on_start + Statsig strip (D10)"
```

---

## Task 12: SystemContext Source trait + Snapshot

**Files:**
- Create: `crates/synthia-context/src/system_context/mod.rs`
- Create: `crates/synthia-context/src/system_context/source.rs`
- Test: `crates/synthia-context/tests/system_context.rs`

- [ ] **Step 1: Write failing tests for Source trait**

```rust
use synthia_context::system_context::{Source, Snapshot};

#[derive(Debug, PartialEq, serde::Serialize, serde::Deserialize)]
struct TestValue { count: usize }

struct TestSource { value: TestValue }
impl Source for TestSource {
    type Value = TestValue;
    fn key(&self) -> &str { "test" }
    fn load(&self) -> anyhow::Result<Self::Value> { Ok(self.value.clone()) }
    fn baseline(&self) -> Self::Value { TestValue { count: 0 } }
    fn update(&self, prev: &Self::Value) -> anyhow::Result<Option<Self::Value>> {
        let current = self.load()?;
        if current == *prev { Ok(None) } else { Ok(Some(current)) }
    }
    fn removed(&self) -> bool { false }
}

#[test]
fn snapshot_serializes() {
    let snap = Snapshot::new(TestValue { count: 5 }, 1);
    let json = serde_json::to_string(&snap).unwrap();
    let restored: Snapshot<TestValue> = serde_json::from_str(&json).unwrap();
    assert_eq!(restored.value.count, 5);
    assert_eq!(restored.revision, 1);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p synthia-context --test system_context`
Expected: FAIL

- [ ] **Step 3: Implement Source trait + Snapshot**

```rust
// crates/synthia-context/src/system_context/source.rs
use serde::{Serialize, Deserialize};

pub trait Source {
    type Value: PartialEq + Serialize + serde::de::DeserializeOwned;
    fn key(&self) -> &str;
    fn load(&self) -> anyhow::Result<Self::Value>;
    fn baseline(&self) -> Self::Value;
    fn update(&self, prev: &Self::Value) -> anyhow::Result<Option<Self::Value>>;
    fn removed(&self) -> bool;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snapshot<V> {
    pub value: V,
    pub revision: u64,
}

impl<V> Snapshot<V> {
    pub fn new(value: V, revision: u64) -> Self {
        Self { value, revision }
    }
    pub fn bump(&mut self, new_value: V) {
        self.value = new_value;
        self.revision += 1;
    }
}
```

```rust
// crates/synthia-context/src/system_context/mod.rs
pub mod source;
pub mod environment_source;
pub mod reconcile;

pub use source::{Source, Snapshot};
pub use reconcile::ReconcileResult;
```

- [ ] **Step 4: Run tests + clippy + commit**

```bash
cargo test -p synthia-context && cargo clippy -p synthia-context
git add -A && git commit -m "feat(context): SystemContext Source trait + Snapshot (D11)"
```

---

## Task 13: SystemContext reconcile + EnvironmentSource

**Files:**
- Create: `crates/synthia-context/src/system_context/reconcile.rs`
- Create: `crates/synthia-context/src/system_context/environment_source.rs`
- Test: extend `crates/synthia-context/tests/system_context.rs`

- [ ] **Step 1: Write failing tests for reconcile**

```rust
#[test]
fn reconcile_unchanged_when_value_identical() {
    let source = TestSource { value: TestValue { count: 5 } };
    let prev = Snapshot::new(TestValue { count: 5 }, 1);
    let result = reconcile(&source, &prev).unwrap();
    assert!(matches!(result, ReconcileResult::Unchanged));
}

#[test]
fn reconcile_updated_when_value_changed() {
    let source = TestSource { value: TestValue { count: 10 } };
    let prev = Snapshot::new(TestValue { count: 5 }, 1);
    let result = reconcile(&source, &prev).unwrap();
    assert!(matches!(result, ReconcileResult::Updated));
}

#[test]
fn reconcile_replacement_blocked_during_in_flight() {
    // Setup in-flight tool call
    // Assert: ReplacementBlocked returned + warning logged
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p synthia-context --test system_context`
Expected: FAIL for reconcile tests

- [ ] **Step 3: Implement reconcile**

```rust
// crates/synthia-context/src/system_context/reconcile.rs
use super::{Source, Snapshot};

pub enum ReconcileResult<V> {
    Unchanged,
    Updated,
    ReplacementReady(Snapshot<V>),
    ReplacementBlocked,
}

pub fn reconcile<S: Source>(source: &S, prev: &Snapshot<S::Value>) -> anyhow::Result<ReconcileResult<S::Value>> {
    if source.removed() {
        // Removed sources handled by caller
    }
    let new_value = match source.update(&prev.value)? {
        None => return Ok(ReconcileResult::Unchanged),
        Some(v) => v,
    };
    // Check if in-flight tool call blocks replacement
    if has_in_flight_tool_call() {
        tracing::warn!("SystemContext replacement blocked due to in-flight tool call");
        return Ok(ReconcileResult::ReplacementBlocked);
    }
    let new_snapshot = Snapshot::new(new_value, prev.revision + 1);
    // Caller decides Updated vs ReplacementReady based on context
    Ok(ReconcileResult::ReplacementReady(new_snapshot))
}

fn has_in_flight_tool_call() -> bool {
    // Implementation depends on runtime state
    false  // placeholder; real impl checks runtime
}
```

- [ ] **Step 4: Implement EnvironmentSource**

```rust
// crates/synthia-context/src/system_context/environment_source.rs
use super::source::Source;
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct EnvironmentValue {
    pub vars: HashMap<String, String>,
}

pub struct EnvironmentSource {
    baseline: EnvironmentValue,
}

impl EnvironmentSource {
    pub fn new() -> Self {
        let vars: HashMap<String, String> = std::env::vars().collect();
        Self { baseline: EnvironmentValue { vars } }
    }
}

impl Source for EnvironmentSource {
    type Value = EnvironmentValue;
    fn key(&self) -> &str { "environment" }
    fn load(&self) -> anyhow::Result<Self::Value> {
        Ok(EnvironmentValue { vars: std::env::vars().collect() })
    }
    fn baseline(&self) -> Self::Value { self.baseline.clone() }
    fn update(&self, prev: &Self::Value) -> anyhow::Result<Option<Self::Value>> {
        let current = self.load()?;
        if current == *prev { Ok(None) } else { Ok(Some(current)) }
    }
    fn removed(&self) -> bool { false }
}
```

- [ ] **Step 5: Verify SystemContext NOT registered as tool**

```rust
#[test]
fn system_context_not_in_tool_registry() {
    let registry = build_default_tool_registry();
    assert!(!registry.has_tool("update_system_context"));
    assert!(!registry.has_tool("set_system_context"));
}
```

- [ ] **Step 6: Run tests + clippy + commit**

```bash
cargo test -p synthia-context && cargo clippy -p synthia-context
git add -A && git commit -m "feat(context): SystemContext reconcile + EnvironmentSource + no-tool guard (D11)"
```

---

## Task 14: Guardian as Tool

**Files:**
- Create: `crates/synthia-guardian/src/tool.rs`
- Modify: `crates/synthia-guardian/src/lib.rs`
- Modify: `crates/synthia-agent/src/stream_builder/builder/run/main_loop.rs`
- Test: `crates/synthia-guardian/tests/tool.rs`

- [ ] **Step 1: Write failing tests**

```rust
// crates/synthia-guardian/tests/tool.rs
use synthia_guardian::tool::SelfReflectTool;

#[tokio::test]
async fn tool_has_correct_name() {
    let tool = SelfReflectTool::new(/* ... */);
    assert_eq!(tool.name(), "self_reflect");
}

#[tokio::test]
async fn tool_description_mentions_independent_review() {
    let tool = SelfReflectTool::new(/* ... */);
    assert!(tool.description().contains("independent context review"));
}

#[tokio::test]
async fn tool_call_dispatches_to_guardian() {
    let tool = SelfReflectTool::new(/* guardian mock */);
    let result = tool.call(json!({})).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn auto_trigger_at_iteration_5() {
    // Setup main loop with iter = 5
    // Verify synthetic self_reflect tool_use injected
}

#[tokio::test]
async fn llm_call_resets_counter() {
    // LLM calls self_reflect at iter 3
    // Verify next auto-trigger scheduled for iter 8
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p synthia-guardian --test tool`
Expected: FAIL

- [ ] **Step 3: Implement SelfReflectTool**

```rust
// crates/synthia-guardian/src/tool.rs
use async_trait::async_trait;
use serde_json::{json, Value};

pub struct SelfReflectTool {
    guardian: Arc<Guardian>,
}

impl SelfReflectTool {
    pub fn new(guardian: Arc<Guardian>) -> Self { Self { guardian } }
}

#[async_trait]
impl Tool for SelfReflectTool {
    fn name(&self) -> &str { "self_reflect" }

    fn description(&self) -> &str {
        "Trigger an independent context review. Returns structured feedback on the current session state. No parameters required."
    }

    fn schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {},
            "required": []
        })
    }

    async fn call(&self, _input: Value) -> Result<Value, ToolError> {
        let review = self.guardian.review().await?;
        Ok(json!({ "feedback": review }))
    }
}
```

- [ ] **Step 4: Integrate into main_loop.rs with auto-trigger fallback**

```rust
// In main_loop.rs
const AUTO_TRIGGER_INTERVAL: u32 = 5;

// In iteration loop
let llm_called_self_reflect = turn.tool_calls.iter()
    .any(|c| c.name == "self_reflect");

if !llm_called_self_reflect && iter % AUTO_TRIGGER_INTERVAL == 0 {
    // Inject synthetic self_reflect call
    let synthetic = ToolCall::new("self_reflect", json!({}));
    let result = orchestrator.execute(&synthetic).await?;
    context.add_tool_result(result);
}

// Reset counter on LLM call
if llm_called_self_reflect {
    self_reflect_counter = iter;  // Next auto-trigger at iter + 5
}
```

- [ ] **Step 5: Run tests + clippy + commit**

```bash
cargo test -p synthia-guardian -p synthia-agent && cargo clippy -p synthia-guardian -p synthia-agent
git add -A && git commit -m "feat(guardian): self_reflect as tool + every-5-rounds fallback (D12)"
```

---

## Task 15: Compaction as Tool

**Files:**
- Create: `crates/synthia-context/src/compaction_tool.rs`
- Modify: `crates/synthia-agent/src/stream_builder/builder/run/main_loop.rs`
- Test: `crates/synthia-context/tests/compaction_tool.rs`

- [ ] **Step 1: Write failing tests**

```rust
#[tokio::test]
async fn compact_context_tool_registered() {
    let registry = build_registry_with_compaction_tool();
    assert!(registry.has_tool("compact_context"));
}

#[tokio::test]
async fn tool_description_has_token_hint() {
    let tool = CompactContextTool::new(/* queue */, 75000);
    let desc = tool.description();
    assert!(desc.contains("<context_tokens>75000</context_tokens>"));
}

#[tokio::test]
async fn llm_call_records_tool_call_trigger() {
    // LLM calls compact_context
    // Assert: CompactionAnalyticsAttempt.trigger == ToolCall
}

#[tokio::test]
async fn auto_trigger_at_80_percent_still_fires() {
    // LLM called compact_context at 70%
    // Context grows to 80%
    // Assert: auto-trigger fires
}

#[tokio::test]
async fn same_iter_dedup() {
    // LLM calls compact_context at iter where auto-trigger also scheduled
    // Assert: only one compaction runs, trigger = ToolCall
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p synthia-context --test compaction_tool`
Expected: FAIL

- [ ] **Step 3: Implement CompactContextTool**

```rust
// crates/synthia-context/src/compaction_tool.rs
use async_trait::async_trait;
use serde_json::{json, Value};

pub struct CompactContextTool {
    queue: Arc<CompactionQueue>,
    current_tokens: Arc<AtomicUsize>,
}

impl CompactContextTool {
    pub fn new(queue: Arc<CompactionQueue>, current_tokens: Arc<AtomicUsize>) -> Self {
        Self { queue, current_tokens }
    }
}

#[async_trait]
impl Tool for CompactContextTool {
    fn name(&self) -> &str { "compact_context" }

    fn description(&self) -> &str {
        let tokens = self.current_tokens.load(Ordering::Relaxed);
        format!(
            "Compact the context to free up tokens. Current context size: <context_tokens>{tokens}</context_tokens>. \
             Optional 'reason' parameter explains why compaction is requested."
        )
    }

    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "reason": { "type": "string", "description": "Why compaction is requested" }
            },
            "required": []
        })
    }

    async fn call(&self, input: Value) -> Result<Value, ToolError> {
        let reason = input.get("reason")
            .and_then(|v| v.as_str())
            .unwrap_or("llm-requested");
        let freed = self.queue.compact(/* ... */).await?;
        Ok(json!({
            "status": "compacted",
            "freed_tokens": freed,
            "reason": reason
        }))
    }
}
```

- [ ] **Step 4: Integrate auto-trigger fallback in main_loop.rs**

```rust
const AUTO_TRIGGER_THRESHOLD: f64 = 0.8;

// In iteration loop
let context_usage = current_tokens as f64 / context_window as f64;
let llm_called_compact = turn.tool_calls.iter()
    .any(|c| c.name == "compact_context");

if llm_called_compact {
    // Skip auto-trigger this iter to avoid duplicate
} else if context_usage >= AUTO_TRIGGER_THRESHOLD {
    // Auto-trigger
    let _ = orchestrator.execute(&ToolCall::new("compact_context", json!({}))).await?;
}
```

- [ ] **Step 5: Run tests + clippy + commit**

```bash
cargo test -p synthia-context -p synthia-agent && cargo clippy -p synthia-context -p synthia-agent
git add -A && git commit -m "feat(context): compact_context as tool + token hints + auto-trigger fallback (D13)"
```

---

## Task 16: Final Verification

- [ ] **Step 1: Run cargo +nightly fmt --all**

Run: `cargo +nightly fmt --all`
Expected: no changes

- [ ] **Step 2: Run cargo clippy with all features**

Run: `cargo clippy --all-targets --all-features --tests --all`
Expected: zero warnings, zero errors

- [ ] **Step 3: Run full test suite**

Run: `cargo test --workspace --all-features`
Expected: all tests pass

- [ ] **Step 4: Run openspec validate --strict**

Run: `openspec validate --strict`
Expected: validation passes

- [ ] **Step 5: Verify no new third-party dependencies**

Run: `cargo tree -d`
Expected: no new duplicate dependencies; confirm tokio::sync::Mutex, Arc::ptr_eq, std::ops::ControlFlow are stdlib/tokio

- [ ] **Step 6: Verify otel feature is optional and default-disabled**

Run: `cargo build` (without otel) && `cargo build --features otel`
Expected: both succeed

- [ ] **Step 7: Verify SYNTHIA_OTLP_ENDPOINT scheme switching**

Run: `SYNTHIA_OTLP_ENDPOINT="grpc://x:4317" cargo test -p synthia-telemetry --features otel` && `SYNTHIA_OTLP_ENDPOINT="http://x:4318" cargo test -p synthia-telemetry --features otel`
Expected: both pass

- [ ] **Step 8: Verify SystemContext NOT registered as tool**

Run: `grep -r "update_system_context\|set_system_context" crates/`
Expected: no matches in tool registration code

- [ ] **Step 9: Verify Statsig fully stripped**

Run: `grep -ri "statsig" crates/`
Expected: no matches

- [ ] **Step 10: Final commit (if any cleanup)**

```bash
git add -A
git commit -m "chore: final verification for borrow-best-from-production-agents"
```

---

## Self-Review Notes

**Spec coverage:** All 12 capabilities from proposal.md have at least one Task implementing them:
- `agent-resume-correctness` → Task 1 (D1) + Task 2 (D2)
- `cache-policy-short-circuit` → Task 3 (D3)
- `file-mutation-queue` → Task 4 (D4 type) + Task 5 (D4 integration)
- `permission-always-propagation` → Task 6 (D5)
- `anchored-summary` → Task 7 (D6)
- `context-overflow-detection` → Task 8 (D7)
- `turn-transition-control` → Task 9 (D8)
- `compaction-telemetry` → Task 10 (D9)
- `otel-span-processor` → Task 11 (D10)
- `system-context-source` → Task 12 (D11 trait) + Task 13 (D11 reconcile + EnvironmentSource)
- `guardian-tool` → Task 14 (D12)
- `compaction-tool` → Task 15 (D13)

**Type consistency:**
- `FileMutationQueue` consistent across Task 4/5
- `CompactionAnalyticsAttempt` consistent across Task 10/15
- `TurnTransition` / `ControlFlow` consistent across Task 9
- `Source` / `Snapshot` / `ReconcileResult` consistent across Task 12/13

**Open questions from design.md handled:**
- Q1 (Source trait Eq granularity): per-source PartialEq (Task 12 Step 3)
- Q2 (TurnTransition retry cap): 3 attempts (Task 9 Step 3)
- Q3 (Anchored Summary provider compat): tests in Task 7 cover structure, provider compat deferred to implementation
- Q4 (file mutation queue integration point): ToolAdapter layer (Task 5)
- Q5 (Guardian auto-trigger interval): every 5 rounds (Task 14 Step 4)
- Q6 (Compaction token hints format): `<context_tokens>X</context_tokens>` XML (Task 15 Step 3)
