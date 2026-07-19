# Fix Agent Critical Bugs and Production Gaps Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix critical bugs (Hook Modify, tool name loss), implement token tracking, add structured error logging, and remove dead code in synthia-agent.

**Architecture:** Changes are focused on 4 files in `stream_builder/` plus removal of dead code. The hook modify fix requires collecting modified calls in a separate vector. Tool name fix requires zipping original calls with outputs. Token tracking requires accumulating usage after sample step.

**Tech Stack:** Rust, async-streams, tokio, tracing

---

## Task 1: Fix Hook Modify Bug

**Files:**
- Modify: `crates/synthia-agent/src/stream_builder/builder.rs:306-338`

- [ ] **Step 1: Read current Hook Modify code**

File: `crates/synthia-agent/src/stream_builder/builder.rs:306-338`

Current code loops over `sampling_result.tool_calls` and fires hooks. The `Modify` branch creates `_modified_call` but never uses it. We need to collect modified calls and pass them to `tool_execute.execute()`.

- [ ] **Step 2: Add modified_calls vector before the loop**

After line 305 (after loop over tool_calls), add:
```rust
let mut modified_tool_calls: Vec<synthia_provider::types::ToolUse> = Vec::new();
```

- [ ] **Step 3: Modify the Modify branch to populate modified_tool_calls**

Replace lines 319-323:
```rust
Ok(synthia_hook::ToolAction::Modify(new_input)) => {
    let modified_name = new_input.get("name")
        .and_then(|v| v.as_str())
        .map(String::from)
        .unwrap_or_else(|| tool_call.name.clone());
    let modified_input = new_input.get("input")
        .cloned()
        .unwrap_or_else(|| tool_call.input.clone());
    modified_tool_calls.push(synthia_provider::types::ToolUse {
        id: tool_call.id.clone(),
        name: modified_name,
        input: modified_input,
    });
    tracing::debug!(tool=%tool_call.name, "Hook modified tool input");
}
```

- [ ] **Step 4: Add Proceed branch to collect unmodified calls**

Add after the Modify branch (still inside the match):
```rust
Ok(synthia_hook::ToolAction::Proceed) => {
    modified_tool_calls.push(tool_call.clone());
}
```

- [ ] **Step 5: Change Err branch to also collect unmodified calls**

Replace `Err(_) => {}` with:
```rust
Err(_) => {
    modified_tool_calls.push(tool_call.clone());
}
```

- [ ] **Step 6: Use modified_tool_calls instead of sampling_result.tool_calls**

Replace line 328:
```rust
let tool_results = match steps.tool_execute.execute(&ctx, modified_tool_calls.clone()).await {
```

- [ ] **Step 7: Run tests to verify**

Run: `cargo test -p synthia-agent --lib -- hook 2>&1`
Expected: Existing hook tests pass

- [ ] **Step 8: Commit**

```bash
git add crates/synthia-agent/src/stream_builder/builder.rs
git commit -m "fix(agent): apply hook Modify input to actual tool execution"
```

---

## Task 2: Fix Tool Name Preservation

**Files:**
- Modify: `crates/synthia-agent/src/stream_builder/steps/tool_execute.rs:27-32`

- [ ] **Step 1: Read current tool_execute.rs code**

File: `crates/synthia-agent/src/stream_builder/steps/tool_execute.rs:27-32`

Current code uses `enumerate()` and formats `tool_{i}`. Need to use `zip()` to pair original calls with outputs.

- [ ] **Step 2: Change enumerate to zip**

Replace line 28:
```rust
Ok(tool_calls.into_iter().zip(outputs).map(|(call, o)| ToolResult {
    tool_name: call.name,
```

- [ ] **Step 3: Run tests to verify**

Run: `cargo test -p synthia-agent --lib -- tool_execute 2>&1`
Expected: Tests pass

- [ ] **Step 4: Commit**

```bash
git add crates/synthia-agent/src/stream_builder/steps/tool_execute.rs
git commit -m "fix(agent): preserve actual tool name in ToolResult"
```

---

## Task 3: Fix Error Path Tool Name

**Files:**
- Modify: `crates/synthia-agent/src/stream_builder/builder.rs:330-337`

- [ ] **Step 1: Read error handling in builder.rs**

File: `crates/synthia-agent/src/stream_builder/builder.rs:330-337`

When tool execution fails, tool_name is hardcoded as "error". We need to track which tool was being executed.

- [ ] **Step 2: Add tool_name tracking before tool execution**

Add before line 328:
```rust
let tool_calls_for_exec = if modified_tool_calls.is_empty() {
    sampling_result.tool_calls.clone()
} else {
    modified_tool_calls.clone()
};
let first_tool_name = tool_calls_for_exec.first()
    .map(|c| c.name.clone())
    .unwrap_or_else(|| "unknown".to_string());
```

- [ ] **Step 3: Use first_tool_name in error result**

Replace line 333:
```rust
tool_name: first_tool_name,
```

- [ ] **Step 4: Run tests to verify**

Run: `cargo test -p synthia-agent --lib 2>&1 | head -50`
Expected: Tests pass

- [ ] **Step 5: Commit**

```bash
git add crates/synthia-agent/src/stream_builder/builder.rs
git commit -m "fix(agent): preserve tool name in error path"
```

---

## Task 4: Fix Unsafe unwrap

**Files:**
- Modify: `crates/synthia-agent/src/stream_builder/builder.rs:360`

- [ ] **Step 1: Find and fix the unwrap**

Line 360 already has `unwrap_or(SessionEndReason::Completed)` - this is already safe! Check if there are other unwrap calls.

Run: `grep -n "\.unwrap()" crates/synthia-agent/src/stream_builder/builder.rs`

If line 249 has `ctx.end_reason.clone().unwrap()`, fix it:
```rust
ctx.end_reason.clone().unwrap_or(SessionEndReason::Error("Unknown".to_string()))
```

- [ ] **Step 2: Commit**

```bash
git add crates/synthia-agent/src/stream_builder/builder.rs
git commit -m "fix(agent): replace unsafe unwrap with unwrap_or"
```

---

## Task 5: Implement Token Tracking

**Files:**
- Modify: `crates/synthia-agent/src/stream_builder/builder.rs` (after sample step)
- Modify: `crates/synthia-agent/src/stream_builder/builder.rs` (TokenBudgetWarning events)

- [ ] **Step 1: Add token accumulation after sample step**

After line 272 (where `SamplingResult` is used), add:
```rust
ctx.cumulative_tokens += sampling_result.usage.total_tokens;
```

- [ ] **Step 2: Update TokenBudgetWarning at line ~173**

Replace hardcoded zeros:
```rust
yield AgentEvent::TokenBudgetWarning {
    status: "must_compact".to_string(),
    current_tokens: ctx.cumulative_tokens,
    threshold_tokens: self.config.context_token_budget.as_ref()
        .map(|b| b.hard_limit)
        .unwrap_or(0),
};
```

- [ ] **Step 3: Update TokenBudgetWarning at line ~181**

Replace hardcoded zeros for warning status.

- [ ] **Step 4: Run tests to verify**

Run: `cargo test -p synthia-agent --lib -- token 2>&1`
Expected: Tests pass

- [ ] **Step 5: Commit**

```bash
git add crates/synthia-agent/src/stream_builder/builder.rs
git commit -m "feat(agent): wire up token tracking for budget warnings"
```

---

## Task 6: Structured Error Logging

**Files:**
- Modify: `crates/synthia-agent/src/stream_builder/builder.rs:110,214,261,349,385`

- [ ] **Step 1: Replace silent error at line 110**

Replace:
```rust
let _ = session_store.ensure_session_dir(&session_id_clone);
```
With:
```rust
if let Err(e) = session_store.ensure_session_dir(&session_id_clone).await {
    tracing::warn!(session_id = %session_id_clone, error = %e, "Failed to ensure session directory");
}
```

- [ ] **Step 2: Replace silent error at line 214**

Replace:
```rust
let _ = steps.hooks.fire_before_llm(&mut agent_ctx).await;
```
With:
```rust
if let Err(e) = steps.hooks.fire_before_llm(&mut agent_ctx).await {
    tracing::warn!(error = %e, "before_llm hook failed");
}
```

- [ ] **Step 3: Replace silent error at line 261**

Replace:
```rust
let _ = steps.hooks.fire_after_llm(&agent_ctx, &response_json).await;
```
With:
```rust
if let Err(e) = steps.hooks.fire_after_llm(&agent_ctx, &response_json).await {
    tracing::warn!(error = %e, "after_llm hook failed");
}
```

- [ ] **Step 4: Replace silent error at line 349**

Replace:
```rust
let _ = sender.send(synthia_memory::types::MemoryEvent::tool_executed(...)).await;
```
With:
```rust
if let Err(e) = sender.send(synthia_memory::types::MemoryEvent::tool_executed(...)).await {
    tracing::warn!(error = %e, "Failed to send tool_executed memory event");
}
```

- [ ] **Step 5: Replace silent error at line 385**

Replace:
```rust
let _ = sender.send(synthia_memory::types::MemoryEvent::session_end(...)).await;
```
With:
```rust
if let Err(e) = sender.send(synthia_memory::types::MemoryEvent::session_end(...)).await {
    tracing::warn!(error = %e, "Failed to send session_end memory event");
}
```

- [ ] **Step 6: Run tests to verify**

Run: `cargo test -p synthia-agent --lib 2>&1 | head -50`
Expected: Tests pass

- [ ] **Step 7: Commit**

```bash
git add crates/synthia-agent/src/stream_builder/builder.rs
git commit -m "feat(agent): replace silent error swallowing with structured logging"
```

---

## Task 7: Dead Code Cleanup

**Files:**
- Delete: `crates/synthia-agent/src/agent_runtime.rs`
- Delete: `crates/synthia-agent/src/agent.rs`
- Modify: `crates/synthia-agent/src/react.rs`
- Modify: `crates/synthia-agent/src/lib.rs`

- [ ] **Step 1: Verify agent_runtime.rs is unused**

Run: `grep -r "agent_runtime" crates/synthia-agent/src/ --include="*.rs" | grep -v "agent_runtime.rs:"`
Expected: No results

- [ ] **Step 2: Verify agent.rs is unused**

Run: `grep -r "mod agent\|use agent" crates/synthia-agent/src/ --include="*.rs" | grep -v "agent/core.rs\|agent/react.rs\|agent/compact.rs\|agent/mod.rs"`
Expected: Only lib.rs exports remain

- [ ] **Step 3: Delete agent_runtime.rs**

```bash
rm crates/synthia-agent/src/agent_runtime.rs
git add -A
git commit -m "chore(agent): remove unused agent_runtime.rs"
```

- [ ] **Step 4: Delete agent.rs**

```bash
rm crates/synthia-agent/src/agent.rs
git add -A
git commit -m "chore(agent): remove unused agent.rs"
```

- [ ] **Step 5: Remove step_self_reflection from react.rs**

Read `crates/synthia-agent/src/react.rs` and remove the `step_self_reflection` function (lines ~138-195).

- [ ] **Step 6: Verify build**

Run: `cargo build -p synthia-agent 2>&1`
Expected: Build succeeds

- [ ] **Step 7: Commit**

```bash
git add crates/synthia-agent/src/react.rs
git commit -m "chore(agent): remove duplicate step_self_reflection (replaced by steps/reflect.rs)"
```

---

## Task 8: Add Hook Modify Integration Test

**Files:**
- Create: `crates/synthia-agent/src/hooks/tests/hook_modify_test.rs`

- [ ] **Step 1: Create integration test for Hook Modify**

```rust
#[tokio::test]
async fn test_hook_modify_actually_modifies_tool_input() {
    // Create a mock hook that returns Modify with changed input
    // Execute a tool call
    // Verify the tool received the modified input, not original
}
```

- [ ] **Step 2: Run test**

Run: `cargo test -p synthia-agent --lib -- test_hook_modify 2>&1`
Expected: Test passes

- [ ] **Step 3: Commit**

```bash
git add crates/synthia-agent/src/hooks/tests/
git commit -m "test(agent): add integration test for Hook Modify functionality"
```

---

## Task 9: Final Verification

- [ ] **Step 1: Run all tests**

Run: `cargo test -p synthia-agent 2>&1`
Expected: All tests pass

- [ ] **Step 2: Run clippy**

Run: `cargo clippy -p synthia-agent -- -D warnings 2>&1`
Expected: No warnings

- [ ] **Step 3: Final build**

Run: `cargo build -p synthia-agent 2>&1`
Expected: Build succeeds

- [ ] **Step 4: Commit all remaining changes**

```bash
git add -A
git commit -m "fix(agent): complete Phase 1 fixes - critical bugs, token tracking, structured logging, dead code cleanup"
```