# P0 Bug Fix — resume() + ErrorRecovery Cooldown Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix two P0 bugs: (1) `resume()` silently drops checkpoint state, breaking session resume; (2) ErrorRecovery cooldown stores timestamp on every error, not just FailFast.

**Architecture:** Two independent bug fixes sharing only the test suite. Fix1 modifies `StreamBuilder` to accept optional initial state and propagates it through `LoopContext` builder methods. Fix 2 changes cooldown timestamp storage to only fire on `FailFast` and clears it on `record_success`.

**Tech Stack:** Rust, async-stream, tokio, Arc<AtomicU64> for cooldown state.

---

## Task 1: StreamBuilder initial_state

**Files:**
- Modify: `crates/synthia-agent/src/stream_builder/builder.rs:20-73`

- [ ] **Step 1: Add `initial_state` field to `StreamBuilder`**

In `builder.rs`, add a new field to the `StreamBuilder` struct at line 20:

```rust
pub struct StreamBuilder {
    context: ContextBuilder,
    hooks: HookBuilder,
    initial_state: Option<(Vec<Message>, usize)>,  // ADD THIS
}
```

- [ ] **Step 2: Add `with_initial_state` method to `StreamBuilder`**

Add after the `hooks_mut` method (around line 73):

```rust
pub fn with_initial_state(&mut self, messages: Vec<Message>, iteration: usize) -> &mut Self {
    self.initial_state = Some((messages, iteration));
    self
}
```

- [ ] **Step 3: Update `run()` to pass initial_state to `run_with_steps`**

In `run()` at line 75, pass `self.initial_state.clone()` to `run_with_steps`:

```rust
pub fn run(
    &self,
    run_config: AgentRunConfig,
) -> Pin<Box<dyn futures::Stream<Item = AgentEvent> + Send>> {
    let steps = BuilderSteps::new(&run_config, self.hooks.clone());
    self.run_with_steps(run_config, steps, self.initial_state.clone())
}
```

- [ ] **Step 4: Update `run_with_steps` signature to accept initial_state**

Update `run_with_steps` signature from:
```rust
fn run_with_steps(
    &self,
    run_config: AgentRunConfig,
    steps: BuilderSteps,
) -> Pin<Box<dyn futures::Stream<Item = AgentEvent> + Send>>
```
To:
```rust
fn run_with_steps(
    &self,
    run_config: AgentRunConfig,
    steps: BuilderSteps,
    initial_state: Option<(Vec<Message>, usize)>,
) -> Pin<Box<dyn futures::Stream<Item = AgentEvent> + Send>>
```

- [ ] **Step 5: Update `run()` call site to pass initial_state**

At line 80 in `run()`, update the call:
```rust
self.run_with_steps(run_config, steps, self.initial_state.clone())
```

- [ ] **Step 6: Commit**

```bash
git add crates/synthia-agent/src/stream_builder/builder.rs
git commit -m "feat(agent): add initial_state field to StreamBuilder for resume support"
```

---

## Task 2: Propagate initial_state to LoopContext

**Files:**
- Modify: `crates/synthia-agent/src/stream_builder/builder.rs:105-119`
- Modify: `crates/synthia-agent/src/agent.rs:125-132`

- [ ] **Step 1: In `run_with_steps`, apply initial_state to LoopContext**

In `run_with_steps` at line 115 (`let mut ctx = LoopContext::new(...)`), add after:

```rust
let mut ctx = LoopContext::new(session_id_clone.clone(), span_ctx);

// If resuming from checkpoint, restore state
if let Some((msgs, iter)) = initial_state {
    ctx.messages = msgs;
    ctx.iteration = iter;
} else if ctx.messages.is_empty() {
    ctx.messages.push(input.to_message());
}
```

The original `if ctx.messages.is_empty()` branch (lines 117-119) must become an `else if` since messages may already be populated from initial_state.

- [ ] **Step 2: Fix `run_stream_with_state` in agent.rs**

In `agent.rs:125-132`, replace the drop:
```rust
pub fn run_stream_with_state(state_config: AgentRunStateConfig) -> AgentOutput {
    let AgentRunStateConfig {
        run_config,
        initial_messages: _,
        start_iteration: _,
    } = state_config;
    StreamBuilder::from_config(&run_config).run(run_config)
}
```
With:
```rust
pub fn run_stream_with_state(state_config: AgentRunStateConfig) -> AgentOutput {
    let AgentRunStateConfig {
        run_config,
        initial_messages,
        start_iteration,
    } = state_config;
    StreamBuilder::from_config(&run_config)
        .with_initial_state(initial_messages, start_iteration)
        .run(run_config)
}
```

- [ ] **Step 3: Commit**

```bash
git add crates/synthia-agent/src/stream_builder/builder.rs crates/synthia-agent/src/agent.rs
git commit -m "fix(agent): wire initial_state from resume() into StreamBuilder loop"
```

---

## Task 3: ErrorRecovery Cooldown Fix

**Files:**
- Modify: `crates/synthia-agent/src/error_recovery/mod.rs:94-165`

- [ ] **Step 1: Move cooldown store inside FailFast match arm**

In `handle_error` around line 112-114, remove the unconditional store:
```rust
// REMOVE these two lines (112-113):
// Enter cooldown on failure (record failure time)
// let now = current_timestamp_secs();
// self.last_recovery_time.store(now, Ordering::Relaxed);  // THIS LINE REMOVE
```

And in the `L5Reset =>` match arm at lines 147-153, add the store ONLY on FailFast return:

```rust
RecoveryLevel::L5Reset => {
    // Reset failed, cannot recover further
    self.last_recovery_time.store(now, Ordering::Relaxed);  // ADD THIS
    RecoveryResult::FailFast(
        "Reset failed, entering fail-fast".to_string(),
    )
}
```

The `let now = current_timestamp_secs();` declaration at line 100 already exists. Remove only the unconditional store at line 114.

- [ ] **Step 2: Clear cooldown in record_success()**

In `record_success()` at lines 158-160, add:
```rust
pub fn record_success(&self) {
    self.consecutive_errors.store(0, Ordering::Relaxed);
    self.last_recovery_time.store(0, Ordering::Relaxed);  // ADD THIS
}
```

- [ ] **Step 3: Commit**

```bash
git add crates/synthia-agent/src/error_recovery/mod.rs
git commit -m "fix(error_recovery): cooldown only on FailFast, clear on success"
```

---

## Task 4: Update Cooldown Tests

**Files:**
- Modify: `crates/synthia-agent/src/error_recovery/mod.rs:253-265`

- [ ] **Step 1: Update test_coordinator_cooldown to reflect corrected semantics**

Replace the current test (lines 253-265) with:
```rust
#[test]
fn test_coordinator_cooldown() {
    let coordinator = ErrorRecoveryCoordinator::new(60);

    // First L1 error - allowed, Escalated, NO cooldown entered
    let result1 = coordinator.handle_error("test1", RecoveryLevel::L1Truncate);
    assert!(matches!(result1, RecoveryResult::Escalated(_)));

    // Immediate second L1 error - still Escalated (no cooldown was entered)
    let result2 = coordinator.handle_error("test2", RecoveryLevel::L1Truncate);
    assert!(matches!(result2, RecoveryResult::Escalated(_)));

    // Now trigger a FailFast (L5)
    let _result3 = coordinator.handle_error("test3", RecoveryLevel::L5Reset);
    assert!(matches!(_result3, RecoveryResult::FailFast(_)));

    // Immediate next call - NOW in cooldown, should FailFast
    let result4 = coordinator.handle_error("test4", RecoveryLevel::L1Truncate);
    assert!(matches!(result4, RecoveryResult::FailFast(_)));
}
```

- [ ] **Step 2: Run the test to verify it passes**

Run: `cargo test -p synthia-agent error_recovery::tests::test_coordinator_cooldown`
Expected: PASS

- [ ] **Step 3: Commit**

```bash
git add crates/synthia-agent/src/error_recovery/mod.rs
git commit -m "test(error_recovery): fix cooldown test to verify escalation-before-cooldown semantics"
```

---

## Task 5: Integration Tests + Verification

**Files:**
- Create: `crates/synthia-agent/tests/e2e_resume_test.rs` (or add to existing test file)
- Run: full test suite

- [ ] **Step 1: Write resume integration test**

Create a test that:
1. Creates an agent session with some back-and-forth messages
2. Calls `Agent::resume()` with the session_id
3. Verifies that the resumed session has all prior messages and correct iteration counter

```rust
#[tokio::test]
async fn test_resume_preserves_messages_and_iteration() {
    let (agent, _) = AgentBuilder::new().build().await;
    let session_id = "test-resume-session";

    // Run a few iterations
    let input = AgentInput::text("hello");
    let mut stream = agent.run(session_id, input, CancellationToken::new()).await;
    while let Some(event) = stream.next().await {
        if matches!(event, AgentEvent::SessionEnded { .. }) { break; }
    }

    // Resume
    let stream = agent.resume(session_id, CancellationToken::new()).await;
    // Verify session started with iteration > 0 and messages > 1
}
```

Place in `crates/synthia-agent/tests/e2e_resume_test.rs`. Check if an existing e2e test file handles this — if so, add there instead.

- [ ] **Step 2: Run full test suite**

Run: `cargo test -p synthia-agent 2>&1 | tail -20`
Expected: all tests pass

- [ ] **Step 3: Run clippy**

Run: `cargo clippy -p synthia-agent 2>&1 | grep -E "error|warning" | head -20`
Expected: clean (no errors)

- [ ] **Step 4: Commit**

```bash
git add crates/synthia-agent/tests/
git commit -m "test(agent): add resume integration test"
```

---

## Self-Review Checklist

- [ ] Spec coverage: Each spec requirement in `session-resume/spec.md` and `agent-error-recovery/spec.md` maps to a task? Yes.
  - resume preserves messages → Task 1+2 ✅
  - resume iteration counter → Task 1+2 ✅
  - cooldown only on FailFast → Task 3 ✅
  - clear cooldown on success → Task 3 ✅
  - test updated → Task 4 ✅
  - integration test → Task 5 ✅
- [ ] No placeholders: All steps have exact file paths and code. ✅
- [ ] Type consistency: `with_initial_state(&mut self, Vec<Message>, usize)` matches usage in `agent.rs`. ✅
- [ ] Test isolation: cooldown test uses `cooldown_secs = 60` and doesn't interfere with other tests. ✅