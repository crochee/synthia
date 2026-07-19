# Architecture Cleanup — react.rs + AgentConfig + Steering Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** (1) Add `#[deprecated]` to `ReActLoop`, (2) Delete dead `config/agent.rs::AgentConfig`, (3) Wire steering channel into `run_with_steps`.

**Architecture:** Three independent cleanup tasks. Deprecation is a one-line change. Steering wiring is the most involved — `BuilderSteps` needs to store and forward the steering channel.

**Tech Stack:** Rust, async-stream, tokio.

---

## Task 1: ReActLoop Deprecation

**Files:**
- Modify: `crates/synthia-agent/src/react.rs:31`

- [ ] **Step 1: Add #[deprecated] attribute to ReActLoop**

In `react.rs` at line 31, add before the struct:

```rust
#[deprecated(note = "Use StreamBuilder in stream_builder/builder.rs; ReActLoop will be removed once external consumers migrate. See openspec/changes/agent-architecture-optimization/")]
pub struct ReActLoop {
```

- [ ] **Step 2: Verify deprecation warning**

Run: `cargo build -p synthia-agent 2>&1 | grep -i deprecat`
Expected: warning about ReActLoop usage

- [ ] **Step 3: Commit**

```bash
git add crates/synthia-agent/src/react.rs
git commit -m "deprecate(agent): mark ReActLoop as deprecated"
```

---

## Task 2: Delete config/agent.rs

**Files:**
- Delete: `crates/synthia-agent/src/config/agent.rs`

- [ ] **Step 1: Delete the file**

```bash
rm crates/synthia-agent/src/config/agent.rs
```

- [ ] **Step 2: Run tests to confirm no breakage**

Run: `cargo test -p synthia-agent 2>&1 | tail -30`
Expected: all tests pass

- [ ] **Step 3: Commit**

```bash
git rm crates/synthia-agent/src/config/agent.rs
git commit -m "cleanup(agent): delete dead config/agent.rs (AgentConfig persona struct)"
```

---

## Task 3: Steering Channel Wiring

**Files:**
- Modify: `crates/synthia-agent/src/stream_builder/builder.rs:25-44, 83-101`
- Modify: `crates/synthia-agent/src/stream_builder/builder.rs:run_with_steps` loop body (around line 123)
- Modify: `crates/synthia-agent/src/config/agent_config.rs` (add `steering_channel` to `BuilderSteps` if needed)

- [ ] **Step 1: Add steering_channel to BuilderSteps**

In `builder.rs`, `BuilderSteps` struct (lines 25-32) — add field:

```rust
pub struct BuilderSteps {
    pub sample: StepSample,
    pub tool_execute: StepToolExecute,
    pub compact: StepCompact,
    pub reflect: StepReflect,
    pub hooks: HookBuilder,
    pub recovery: crate::error_recovery::ErrorRecoveryCoordinator,
    pub steering_channel: Option<Arc<dyn SteeringChannel>>,  // ADD THIS
}
```

- [ ] **Step 2: Initialize steering_channel in BuilderSteps::new**

In `BuilderSteps::new` (lines 35-44), add field:

```rust
pub fn new(config: &AgentRunConfig, hooks: HookBuilder) -> Self {
    Self {
        sample: StepSample::new(config.config.clone()),
        tool_execute: StepToolExecute::new(Arc::new(config.tool_registry.clone())),
        compact: StepCompact,
        reflect: StepReflect::new(config.config.model.clone()),
        hooks,
        recovery: crate::error_recovery::ErrorRecoveryCoordinator::new(5),
        steering_channel: config.steering_channel.clone(),  // ADD THIS
    }
}
```

- [ ] **Step 3: Wire steering_channel into run_with_steps**

In `run_with_steps` at line 83, add `steering_channel` to destructure:
```rust
fn run_with_steps(
    &self,
    run_config: AgentRunConfig,
    steps: BuilderSteps,
    initial_state: Option<(Vec<Message>, usize)>,  // also add this for completeness
) -> Pin<Box<dyn futures::Stream<Item = AgentEvent> + Send>>
```

At the top of the `while` loop (line 123 `while !ctx.should_stop(...)`), before `ctx.increment_iteration()`, add steering drain:

```rust
while !ctx.should_stop(config.max_iterations) {
    // Drain steering channel at start of iteration
    if let Some(ref steering_channel) = steps.steering_channel {
        if let Some(msg) = steering_channel.try_recv() {
            yield AgentEvent::SteeringReceived {
                session_id: session_id_clone.clone(),
                message: msg.clone(),
            };
            ctx.messages.insert(0, Message::user(msg.content.clone()));
        }
    }

    ctx.increment_iteration();
```

Note: `SteeringChannel` trait and `try_recv` method need to be imported. Verify the trait has a `try_recv` method — if it uses `recv` (blocking), use `try_recv` variant or `poll`.

- [ ] **Step 4: Add import for SteeringChannel**

In `builder.rs` imports (top of file), add:
```rust
use crate::steering::{SteeringChannel, SteeringMessage};
```

- [ ] **Step 5: Run steering tests**

Run: `cargo test -p synthia-agent steering2>&1 | tail -20`
Expected: all steering tests pass (e2e_steering_injection_test.rs, span_hierarchy_test.rs, etc.)

- [ ] **Step 6: Commit**

```bash
git add crates/synthia-agent/src/stream_builder/builder.rs
git commit -m "feat(agent): wire steering channel into run_with_steps loop"
```

---

## Task 4: Cleanup Verification

- [ ] **Step 1: Run full test suite**

Run: `cargo test -p synthia-agent 2>&1 | tail -20`
Expected: all tests pass

- [ ] **Step 2: Run clippy**

Run: `cargo clippy -p synthia-agent 2>&1 | grep -E "error|warning" | head -20`
Expected: clean

- [ ] **Step 3: Commit**

```bash
git add -A && git commit -m "test(agent): verify architecture cleanup passes all tests"
```

---

## Self-Review Checklist

- [ ] Spec coverage: react.rs deprecation → Task 1 ✅, delete AgentConfig → Task 2 ✅, steering wire-up → Task 3 ✅
- [ ] No placeholders: all file:line references exact ✅
- [ ] Type consistency: `steering_channel: Option<Arc<dyn SteeringChannel>>` matches `AgentRunConfig` field type ✅
- [ ] Test isolation: steering tests run independently ✅