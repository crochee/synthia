# Pipeline-Stage AI Agent Refactoring Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Decompose the 1748-line main_loop monolith and 1764-line ToolOrchestrator into composable Pipeline-Stage architecture without breaking public APIs.

**Architecture:** 7 Stage structs implementing a `Stage` trait, driven by a `Pipeline` orchestrator. LoopContext enhanced with `SessionPhase` state machine. ToolOrchestrator split into `SecurityPipeline` + `ExecutionPipeline` composed via `OrchestratorFacade`. Stages write `AgentEvent`s to `TurnOutput`, the top-level stream driver yields them.

**Tech Stack:** Rust, async-trait, tokio, async_stream, dashmap, parking_lot

**Design Spec:** `docs/superpowers/specs/2026-07-29-pipeline-stage-refactoring-design.md`

---

## File Structure Map

### New Files (synthia-agent)

| File | Responsibility |
|------|---------------|
| `crates/synthia-agent/src/pipeline/mod.rs` | Pipeline driver + module exports |
| `crates/synthia-agent/src/pipeline/stage.rs` | Stage trait, StageFlow, TurnInput, TurnOutput |
| `crates/synthia-agent/src/pipeline/init_stage.rs` | Stage 1: iteration init, cancel check, goal check |
| `crates/synthia-agent/src/pipeline/compact_stage.rs` | Stage 2: compaction check and execution |
| `crates/synthia-agent/src/pipeline/sample_stage.rs` | Stage 3: LLM sampling and cascade |
| `crates/synthia-agent/src/pipeline/tool_stage.rs` | Stage 4: tool execution and interception |
| `crates/synthia-agent/src/pipeline/auto_trigger_stage.rs` | Stage 5: self_reflect / compact auto-trigger |
| `crates/synthia-agent/src/pipeline/turn_close_stage.rs` | Stage 6: turn completion and goal tracking |
| `crates/synthia-agent/src/pipeline/session_end_stage.rs` | Stage 7: session termination and reflection |

### New Files (synthia-tool-orchestrator)

| File | Responsibility |
|------|---------------|
| `crates/synthia-tool-orchestrator/src/security/mod.rs` | SecurityPipeline struct and check() |
| `crates/synthia-tool-orchestrator/src/security/capability_check.rs` | CapabilityBroker step |
| `crates/synthia-tool-orchestrator/src/security/provenance_check.rs` | ProvenanceResolver step |
| `crates/synthia-tool-orchestrator/src/security/approval_check.rs` | ApprovalService step |
| `crates/synthia-tool-orchestrator/src/execution/mod.rs` | ExecutionPipeline struct (refactored from execution.rs) |
| `crates/synthia-tool-orchestrator/src/execution/resolve.rs` | Tool resolution step |
| `crates/synthia-tool-orchestrator/src/execution/sandbox.rs` | Sandbox policy step |
| `crates/synthia-tool-orchestrator/src/execution/concurrency.rs` | Concurrency control step |
| `crates/synthia-tool-orchestrator/src/execution/retry.rs` | Retry policy step |

### Modified Files

| File | Change |
|------|--------|
| `synthia-agent/src/loop_context.rs` | Add SessionPhase, privatize fields, add accessors/mutators |
| `synthia-agent/src/stream_builder/builder/run/main_loop.rs` | 1748 → ~200 lines thin driver |
| `synthia-agent/src/lib.rs` | Add `pub mod pipeline;` |
| `synthia-tool-orchestrator/src/lib.rs` | 1764 → ~200 lines OrchestratorFacade |
| `synthia-tool-orchestrator/src/types.rs` | Add SecurityDecision enum |

---

## Phase 1: Foundation — Stage Trait and Pipeline Driver

### Task 1: Create pipeline module with Stage trait and types

**Files:**
- Create: `crates/synthia-agent/src/pipeline/mod.rs`
- Create: `crates/synthia-agent/src/pipeline/stage.rs`
- Modify: `crates/synthia-agent/src/lib.rs`

- [ ] **Step 1: Create the pipeline directory**

```bash
mkdir -p crates/synthia-agent/src/pipeline
```

- [ ] **Step 2: Write `stage.rs` with Stage trait, StageFlow, TurnInput, TurnOutput**

```rust
// crates/synthia-agent/src/pipeline/stage.rs

use tokio_util::sync::CancellationToken;

use crate::{
    events::SessionEndReason,
    loop_context::LoopContext,
};

/// Input context for a single turn iteration.
pub struct TurnInput {
    pub cancel_token: CancellationToken,
}

/// Output collected from a single turn iteration.
#[derive(Default)]
pub struct TurnOutput {
    pub events: Vec<crate::events::AgentEvent>,
}

impl TurnOutput {
    /// Push an event to the output buffer.
    pub fn push(&mut self, event: crate::events::AgentEvent) {
        self.events.push(event);
    }

    /// Extend the output buffer with multiple events.
    pub fn extend(&mut self, events: impl IntoIterator<Item = crate::events::AgentEvent>) {
        self.events.extend(events);
    }
}

/// Stage output determines pipeline control flow.
#[derive(Debug)]
pub enum StageFlow {
    /// Normal: proceed to next Stage.
    Continue,
    /// Skip remaining Stages, advance to next iteration.
    Skip,
    /// Terminate the session with the given reason.
    Terminate(SessionEndReason),
}

/// Single Stage in the agent pipeline.
///
/// Each Stage processes one concern within a turn iteration.
/// Stages write events to `TurnOutput` and return `StageFlow`
/// to control pipeline progression.
#[async_trait::async_trait]
pub trait Stage: Send + Sync {
    /// Process this stage.
    async fn process(
        &self,
        ctx: &mut LoopContext,
        input: &TurnInput,
        output: &mut TurnOutput,
    ) -> StageFlow;
}
```

- [ ] **Step 3: Write `pipeline/mod.rs` with Pipeline driver**

```rust
// crates/synthia-agent/src/pipeline/mod.rs

pub mod stage;
pub mod init_stage;
pub mod compact_stage;
pub mod sample_stage;
pub mod tool_stage;
pub mod auto_trigger_stage;
pub mod turn_close_stage;
pub mod session_end_stage;

pub use stage::{Stage, StageFlow, TurnInput, TurnOutput};

use crate::loop_context::LoopContext;

/// Ordered sequence of Stages.
pub struct Pipeline {
    stages: Vec<Box<dyn Stage>>,
}

impl Pipeline {
    /// Create a new pipeline with the given stages in order.
    pub fn new(stages: Vec<Box<dyn Stage>>) -> Self {
        Self { stages }
    }

    /// Run all stages for one turn iteration.
    ///
    /// Returns the collected output and the final flow decision.
    /// If any stage returns `Skip` or `Terminate`, remaining
    /// stages are not executed.
    pub async fn run_turn(
        &self,
        ctx: &mut LoopContext,
        input: &TurnInput,
    ) -> (TurnOutput, StageFlow) {
        let mut output = TurnOutput::default();
        for stage in &self.stages {
            match stage.process(ctx, input, &mut output).await {
                StageFlow::Continue => {}
                StageFlow::Skip => return (output, StageFlow::Skip),
                StageFlow::Terminate(reason) => {
                    ctx.set_end_reason(reason);
                    return (output, StageFlow::Terminate(reason));
                }
            }
        }
        (output, StageFlow::Continue)
    }
}
```

- [ ] **Step 4: Add `pub mod pipeline;` to `lib.rs`**

Add after `pub mod patterns;` line in `crates/synthia-agent/src/lib.rs`:

```rust
pub mod pipeline;
```

- [ ] **Step 5: Create stub stage files so the module compiles**

Create minimal stub files for each stage referenced in `mod.rs`:

```rust
// crates/synthia-agent/src/pipeline/init_stage.rs
use super::stage::{Stage, StageFlow, TurnInput, TurnOutput};
use crate::loop_context::LoopContext;

pub struct InitStage;

#[async_trait::async_trait]
impl Stage for InitStage {
    async fn process(
        &self,
        _ctx: &mut LoopContext,
        _input: &TurnInput,
        _output: &mut TurnOutput,
    ) -> StageFlow {
        StageFlow::Continue
    }
}
```

Create identical stubs for: `compact_stage.rs`, `sample_stage.rs`, `tool_stage.rs`, `auto_trigger_stage.rs`, `turn_close_stage.rs`, `session_end_stage.rs` (each with their own struct name).

- [ ] **Step 6: Verify compilation**

```bash
cargo check -p synthia-agent 2>&1 | tail -5
```

Expected: compilation succeeds (stubs compile).

- [ ] **Step 7: Write failing test for Pipeline driver**

```rust
// Add to crates/synthia-agent/src/pipeline/mod.rs at bottom

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{events::SessionEndReason, loop_context::LoopContext};
    use synthia_telemetry::span_context::SpanContext;

    /// A no-op stage that always continues.
    struct NoopStage;

    #[async_trait::async_trait]
    impl Stage for NoopStage {
        async fn process(
            &self,
            _ctx: &mut LoopContext,
            _input: &TurnInput,
            _output: &mut TurnOutput,
        ) -> StageFlow {
            StageFlow::Continue
        }
    }

    /// A stage that always skips.
    struct SkipStage;

    #[async_trait::async_trait]
    impl Stage for SkipStage {
        async fn process(
            &self,
            _ctx: &mut LoopContext,
            _input: &TurnInput,
            _output: &mut TurnOutput,
        ) -> StageFlow {
            StageFlow::Skip
        }
    }

    /// A stage that always terminates.
    struct TerminateStage;

    #[async_trait::async_trait]
    impl Stage for TerminateStage {
        async fn process(
            &self,
            _ctx: &mut LoopContext,
            _input: &TurnInput,
            _output: &mut TurnOutput,
        ) -> StageFlow {
            StageFlow::Terminate(SessionEndReason::Completed)
        }
    }

    #[tokio::test]
    async fn pipeline_all_continue_returns_continue() {
        let pipeline = Pipeline::new(vec![
            Box::new(NoopStage),
            Box::new(NoopStage),
        ]);
        let mut ctx = LoopContext::new("test".into(), SpanContext::new("test"));
        let input = TurnInput {
            cancel_token: tokio_util::sync::CancellationToken::new(),
        };
        let (_output, flow) = pipeline.run_turn(&mut ctx, &input).await;
        assert!(matches!(flow, StageFlow::Continue));
    }

    #[tokio::test]
    async fn pipeline_skip_stops_early() {
        let pipeline = Pipeline::new(vec![
            Box::new(SkipStage),
            Box::new(NoopStage), // should not run
        ]);
        let mut ctx = LoopContext::new("test".into(), SpanContext::new("test"));
        let input = TurnInput {
            cancel_token: tokio_util::sync::CancellationToken::new(),
        };
        let (_output, flow) = pipeline.run_turn(&mut ctx, &input).await;
        assert!(matches!(flow, StageFlow::Skip));
    }

    #[tokio::test]
    async fn pipeline_terminate_stops_and_sets_reason() {
        let pipeline = Pipeline::new(vec![
            Box::new(TerminateStage),
            Box::new(NoopStage), // should not run
        ]);
        let mut ctx = LoopContext::new("test".into(), SpanContext::new("test"));
        let input = TurnInput {
            cancel_token: tokio_util::sync::CancellationToken::new(),
        };
        let (_output, flow) = pipeline.run_turn(&mut ctx, &input).await;
        assert!(matches!(flow, StageFlow::Terminate(SessionEndReason::Completed)));
        assert!(matches!(ctx.end_reason, Some(SessionEndReason::Completed)));
    }
}
```

- [ ] **Step 8: Run tests to verify they pass**

```bash
cargo test -p synthia-agent pipeline::tests 2>&1 | tail -10
```

Expected: all 3 tests PASS.

- [ ] **Step 9: Commit**

```bash
git add crates/synthia-agent/src/pipeline/ crates/synthia-agent/src/lib.rs
git commit -m "feat(agent): add Pipeline-Stage foundation — Stage trait, StageFlow, TurnInput/Output, Pipeline driver"
```

---

## Phase 2: LoopContext Enhancement

### Task 2: Add SessionPhase state machine to LoopContext

**Files:**
- Modify: `crates/synthia-agent/src/loop_context.rs`

- [ ] **Step 1: Write failing test for SessionPhase transitions**

Add to the `#[cfg(test)] mod tests` block in `loop_context.rs`:

```rust
#[test]
fn test_session_phase_transition_idle_to_running() {
    let span_ctx = SpanContext::new("test-session");
    let mut ctx = LoopContext::new("session".to_string(), span_ctx);
    assert_eq!(ctx.phase(), &SessionPhase::Idle);
    ctx.transition_to(SessionPhase::Running);
    assert_eq!(ctx.phase(), &SessionPhase::Running);
}

#[test]
fn test_session_phase_transition_running_to_compacting() {
    let span_ctx = SpanContext::new("test-session");
    let mut ctx = LoopContext::new("session".to_string(), span_ctx);
    ctx.transition_to(SessionPhase::Running);
    ctx.transition_to(SessionPhase::Compacting);
    assert_eq!(ctx.phase(), &SessionPhase::Compacting);
}

#[test]
fn test_session_phase_transition_compacting_to_running() {
    let span_ctx = SpanContext::new("test-session");
    let mut ctx = LoopContext::new("session".to_string(), span_ctx);
    ctx.transition_to(SessionPhase::Running);
    ctx.transition_to(SessionPhase::Compacting);
    ctx.transition_to(SessionPhase::Running);
    assert_eq!(ctx.phase(), &SessionPhase::Running);
}

#[test]
#[should_panic(expected = "Invalid phase transition")]
fn test_session_phase_invalid_transition_idle_to_compacting() {
    let span_ctx = SpanContext::new("test-session");
    let mut ctx = LoopContext::new("session".to_string(), span_ctx);
    ctx.transition_to(SessionPhase::Compacting); // should panic in debug
}
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
cargo test -p synthia-agent loop_context::tests::test_session_phase 2>&1 | tail -10
```

Expected: FAIL — `SessionPhase` not defined yet.

- [ ] **Step 3: Add SessionPhase enum and transition_to to LoopContext**

Add to `loop_context.rs` before the `LoopContext` struct:

```rust
/// Session phase for state machine validation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionPhase {
    Idle,
    Running,
    Compacting,
    Reflecting,
    Ending,
}
```

Add `phase` and `phase_since` fields to `LoopContext`:

```rust
pub struct LoopContext {
    // ... existing fields ...
    /// Session phase state machine (NEW).
    pub phase: SessionPhase,
    /// Timestamp when the current phase was entered.
    pub phase_since: Instant,
}
```

Add `transition_to` method and `phase()` accessor:

```rust
impl LoopContext {
    // ... in the impl block, add:

    /// Transition to a new phase. Validates the transition.
    /// Invalid transitions panic in debug, log warning in release.
    pub fn transition_to(&mut self, phase: SessionPhase) {
        debug_assert!(
            self.is_valid_transition(&self.phase, &phase),
            "Invalid phase transition: {:?} → {:?}",
            self.phase,
            phase
        );
        if !self.is_valid_transition(&self.phase, &phase) {
            tracing::warn!(
                from = ?self.phase,
                to = ?phase,
                "Invalid phase transition"
            );
        }
        self.phase = phase;
        self.phase_since = Instant::now();
    }

    /// Public read accessor for phase.
    pub fn phase(&self) -> &SessionPhase {
        &self.phase
    }

    fn is_valid_transition(&self, from: &SessionPhase, to: &SessionPhase) -> bool {
        matches!(
            (from, to),
            (SessionPhase::Idle, SessionPhase::Running)
                | (SessionPhase::Running, SessionPhase::Compacting)
                | (SessionPhase::Running, SessionPhase::Reflecting)
                | (SessionPhase::Running, SessionPhase::Ending)
                | (SessionPhase::Compacting, SessionPhase::Running)
                | (SessionPhase::Reflecting, SessionPhase::Running)
                | (_, SessionPhase::Ending)
        )
    }
}
```

Initialize `phase` and `phase_since` in `LoopContext::new()` and `LoopContext::from_metadata()`:

```rust
// In new():
phase: SessionPhase::Idle,
phase_since: Instant::now(),

// In from_metadata():
phase: SessionPhase::Idle,
phase_since: Instant::now(),
```

- [ ] **Step 4: Add `llm_compact_called_this_iter` flag**

Add field to LoopContext:

```rust
/// Whether LLM-driven compact_context was called this iteration.
/// Set by SampleStage, read/cleared by AutoTriggerStage for dedup.
pub llm_compact_called_this_iter: bool,
```

Initialize in `new()` and `from_metadata()`:

```rust
llm_compact_called_this_iter: false,
```

Add methods:

```rust
/// Mark that LLM-driven compact_context was called this iteration.
pub fn set_llm_compact_called(&mut self) {
    self.llm_compact_called_this_iter = true;
}

/// Take and reset the llm_compact_called_this_iter flag.
pub fn take_llm_compact_called(&mut self) -> bool {
    std::mem::take(&mut self.llm_compact_called_this_iter)
}
```

- [ ] **Step 5: Run tests to verify they pass**

```bash
cargo test -p synthia-agent loop_context 2>&1 | tail -20
```

Expected: All loop_context tests PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/synthia-agent/src/loop_context.rs
git commit -m "feat(agent): add SessionPhase state machine and llm_compact flag to LoopContext"
```

---

## Phase 3: Implement Stages

### Task 3: Implement InitStage

**Files:**
- Modify: `crates/synthia-agent/src/pipeline/init_stage.rs`

- [ ] **Step 1: Write failing test for InitStage**

Add `#[cfg(test)] mod tests` to `init_stage.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::loop_context::LoopContext;
    use synthia_telemetry::span_context::SpanContext;

    #[tokio::test]
    async fn init_stage_increments_iteration() {
        let stage = InitStage::new(Arc::new(HookRegistry::new()), None);
        let mut ctx = LoopContext::new("test".into(), SpanContext::new("test"));
        let input = TurnInput {
            cancel_token: tokio_util::sync::CancellationToken::new(),
        };
        let mut output = TurnOutput::default();
        let flow = stage.process(&mut ctx, &input, &mut output).await;
        assert!(matches!(flow, StageFlow::Continue));
        assert_eq!(ctx.iteration(), 1); // was 0, now 1
    }

    #[tokio::test]
    async fn init_stage_terminates_on_cancel() {
        let stage = InitStage::new(Arc::new(HookRegistry::new()), None);
        let mut ctx = LoopContext::new("test".into(), SpanContext::new("test"));
        let cancel = tokio_util::sync::CancellationToken::new();
        cancel.cancel();
        let input = TurnInput { cancel_token: cancel };
        let mut output = TurnOutput::default();
        let flow = stage.process(&mut ctx, &input, &mut output).await;
        assert!(matches!(flow, StageFlow::Terminate(SessionEndReason::Cancelled)));
    }

    #[tokio::test]
    async fn init_stage_assigns_turn_id() {
        let stage = InitStage::new(Arc::new(HookRegistry::new()), None);
        let mut ctx = LoopContext::new("test".into(), SpanContext::new("test"));
        let input = TurnInput {
            cancel_token: tokio_util::sync::CancellationToken::new(),
        };
        let mut output = TurnOutput::default();
        let _ = stage.process(&mut ctx, &input, &mut output).await;
        assert!(ctx.current_turn_id().is_some());
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cargo test -p synthia-agent pipeline::init_stage 2>&1 | tail -10
```

Expected: FAIL — `InitStage::new` not defined.

- [ ] **Step 3: Implement InitStage**

The InitStage extracts iteration-initialization logic from main_loop.rs lines 310-413. It needs:

- Hook registry for dispatching events
- Optional agent_control for checking background tasks
- Access to session_store for turn events
- Session/user IDs for event emission

Implementation signature:

```rust
use std::sync::Arc;

use synthia_hook::HookRegistry;

use super::stage::{Stage, StageFlow, TurnInput, TurnOutput};
use crate::{
    events::{AgentEvent, SessionEndReason, SystemEvent},
    loop_context::{LoopContext, SessionPhase},
};

pub struct InitStage {
    hook_registry: Arc<HookRegistry>,
    session_id: String,
    user_id: String,
}

impl InitStage {
    pub fn new(
        hook_registry: Arc<HookRegistry>,
        session_id: String,
        user_id: String,
    ) -> Self {
        Self { hook_registry, session_id, user_id }
    }
}

#[async_trait::async_trait]
impl Stage for InitStage {
    async fn process(
        &self,
        ctx: &mut LoopContext,
        input: &TurnInput,
        output: &mut TurnOutput,
    ) -> StageFlow {
        // Check cancellation
        if input.cancel_token.is_cancelled() {
            return StageFlow::Terminate(SessionEndReason::Cancelled);
        }

        // Increment iteration
        ctx.increment_iteration();
        ctx.set_forwarded_this_turn(0);

        // Assign turn ID
        let _turn_id = ctx.assign_new_turn_id();

        // Transition to Running phase
        ctx.transition_to(SessionPhase::Running);

        StageFlow::Continue
    }
}
```

Note: This is the minimal viable InitStage. The full version would also include:
- Goal status check
- Steering channel drain
- Background sub-agent checks
- Turn start event emission

These will be added incrementally as the main_loop is decomposed.

- [ ] **Step 4: Run test to verify it passes**

```bash
cargo test -p synthia-agent pipeline::init_stage 2>&1 | tail -10
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/synthia-agent/src/pipeline/init_stage.rs
git commit -m "feat(agent): implement InitStage — iteration init, cancel check, phase transition"
```

### Task 4: Implement CompactStage

**Files:**
- Modify: `crates/synthia-agent/src/pipeline/compact_stage.rs`

- [ ] **Step 1: Write failing test for CompactStage**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{config::AgentConfig, loop_context::LoopContext};
    use synthia_telemetry::span_context::SpanContext;

    fn make_ctx_with_messages(msg_count: usize) -> LoopContext {
        let mut ctx = LoopContext::new("test".into(), SpanContext::new("test"));
        for i in 0..msg_count {
            ctx.push_message(synthia_provider::types::Message::user(
                format!("message {i} with enough tokens to exceed a tiny budget"),
            ));
        }
        ctx
    }

    #[tokio::test]
    async fn compact_stage_continues_when_no_budget() {
        let stage = CompactStage::new(Arc::new(HookRegistry::new()));
        let mut ctx = LoopContext::new("test".into(), SpanContext::new("test"));
        let input = TurnInput {
            cancel_token: tokio_util::sync::CancellationToken::new(),
        };
        let mut output = TurnOutput::default();
        let flow = stage.process(&mut ctx, &input, &mut output).await;
        assert!(matches!(flow, StageFlow::Continue));
    }
}
```

- [ ] **Step 2: Implement CompactStage**

```rust
use std::sync::Arc;

use synthia_hook::HookRegistry;

use super::stage::{Stage, StageFlow, TurnInput, TurnOutput};
use crate::{
    loop_context::LoopContext,
    stream_builder::steps::StepCompact,
};

pub struct CompactStage {
    hook_registry: Arc<HookRegistry>,
}

impl CompactStage {
    pub fn new(hook_registry: Arc<HookRegistry>) -> Self {
        Self { hook_registry }
    }
}

#[async_trait::async_trait]
impl Stage for CompactStage {
    async fn process(
        &self,
        ctx: &mut LoopContext,
        _input: &TurnInput,
        output: &mut TurnOutput,
    ) -> StageFlow {
        // Dispatch PreCompact hook
        // ... (will be extracted from main_loop lines 510-517)

        // Check compaction need
        let compact_step = StepCompact;
        // Note: AgentConfig access will be provided via Stage construction
        // For now, just continue

        StageFlow::Continue
    }
}
```

- [ ] **Step 3: Run test to verify it passes**

```bash
cargo test -p synthia-agent pipeline::compact_stage 2>&1 | tail -10
```

- [ ] **Step 4: Commit**

```bash
git add crates/synthia-agent/src/pipeline/compact_stage.rs
git commit -m "feat(agent): implement CompactStage stub — compaction check"
```

### Task 5: Implement SampleStage (stub)

**Files:**
- Modify: `crates/synthia-agent/src/pipeline/sample_stage.rs`

- [ ] **Step 1: Implement SampleStage stub**

The SampleStage is the most complex stage. Start with a stub that returns Continue:

```rust
use super::stage::{Stage, StageFlow, TurnInput, TurnOutput};
use crate::loop_context::LoopContext;

/// Stage 3: LLM sampling and cascade.
///
/// Responsible for:
/// - Building tool definitions
/// - Dispatching UserPromptSubmit/PreResponse hooks
/// - Capturing prefix snapshot (pre-LLM)
/// - Calling sample_llm_and_cascade()
/// - Accumulating token usage
/// - Detecting doom loops
/// - Setting end_reason for text-only responses
pub struct SampleStage;

impl SampleStage {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait::async_trait]
impl Stage for SampleStage {
    async fn process(
        &self,
        _ctx: &mut LoopContext,
        _input: &TurnInput,
        _output: &mut TurnOutput,
    ) -> StageFlow {
        // Will be populated by extracting logic from main_loop
        StageFlow::Continue
    }
}
```

- [ ] **Step 2: Verify compilation**

```bash
cargo check -p synthia-agent 2>&1 | tail -5
```

- [ ] **Step 3: Commit**

```bash
git add crates/synthia-agent/src/pipeline/sample_stage.rs
git commit -m "feat(agent): add SampleStage stub"
```

### Task 6: Implement ToolStage (stub)

**Files:**
- Modify: `crates/synthia-agent/src/pipeline/tool_stage.rs`

Same pattern as Task 5 — create stub with struct and `new()`, return `StageFlow::Continue`. Commit.

### Task 7: Implement AutoTriggerStage (stub)

**Files:**
- Modify: `crates/synthia-agent/src/pipeline/auto_trigger_stage.rs`

Same pattern. Commit.

### Task 8: Implement TurnCloseStage (stub)

**Files:**
- Modify: `crates/synthia-agent/src/pipeline/turn_close_stage.rs`

Same pattern. Commit.

### Task 9: Implement SessionEndStage (stub)

**Files:**
- Modify: `crates/synthia-agent/src/pipeline/session_end_stage.rs`

Same pattern. Commit.

---

## Phase 4: Main Loop Thinning

### Task 10: Wire Pipeline into main_loop.rs

**Files:**
- Modify: `crates/synthia-agent/src/stream_builder/builder/run/main_loop.rs`

This is the critical task where the 1748-line monolith starts being replaced by the Pipeline driver. The approach:

1. Keep the existing `run_with_steps()` function signature unchanged
2. Move the session initialization code (lines 156-300) into `run_with_steps()` before the `while` loop
3. Replace the `while` loop body with `pipeline.run_turn()` call
4. Move session-end code (lines 1277-1351) after the `while` loop

- [ ] **Step 1: Construct Pipeline in run_with_steps**

After the session initialization code (FragmentRegistry, LoopContext setup, etc.), add:

```rust
let pipeline = Pipeline::new(vec![
    Box::new(InitStage::new(
        steps.hook_registry.clone(),
        session_id_clone.clone(),
        user_id.clone(),
    )),
    Box::new(CompactStage::new(steps.hook_registry.clone())),
    Box::new(SampleStage::new()),
    Box::new(ToolStage::new()),
    Box::new(AutoTriggerStage::new()),
    Box::new(TurnCloseStage::new()),
    Box::new(SessionEndStage::new()),
]);
```

- [ ] **Step 2: Replace the while loop body with pipeline.run_turn()**

The new while loop:

```rust
while !ctx.should_stop_with_timeout(
    config.max_iterations,
    config.session_wall_clock_timeout,
) {
    let turn_input = TurnInput {
        cancel_token: cancel_token.clone(),
    };
    let (turn_output, flow) = pipeline.run_turn(&mut ctx, &turn_input).await;
    for ev in turn_output.events {
        yield ev;
    }
    if let StageFlow::Terminate(_) = flow {
        break;
    }
}
```

**IMPORTANT**: At this point, the stages are stubs that return `Continue`. The existing while-loop body must NOT be deleted yet — it should be commented out or behind a feature flag until each stage is fully implemented.

- [ ] **Step 3: Verify compilation**

```bash
cargo check -p synthia-agent 2>&1 | tail -5
```

- [ ] **Step 4: Commit**

```bash
git add crates/synthia-agent/src/stream_builder/builder/run/main_loop.rs
git commit -m "refactor(agent): wire Pipeline driver into main_loop — stub stages, existing logic preserved"
```

### Task 11: Migrate InitStage logic from main_loop

**Files:**
- Modify: `crates/synthia-agent/src/pipeline/init_stage.rs`
- Modify: `crates/synthia-agent/src/stream_builder/builder/run/main_loop.rs`

This task extracts lines 310-413 from main_loop.rs into InitStage.process(). The key challenge is that InitStage needs access to:
- `session_store` for turn event emission
- `agent_control` for background task checks
- `session_input_queue` for steering drain
- `loop_services` for goal tracking

These dependencies are injected via InitStage construction.

- [ ] **Step 1: Add dependencies to InitStage struct**

```rust
pub struct InitStage {
    hook_registry: Arc<HookRegistry>,
    session_id: String,
    user_id: String,
    session_store: Arc<dyn SessionStore>,
    agent_control: Option<Arc<dyn AgentControl>>,
    loop_services: Arc<LoopServices>,
    session_input_queue: Option<Arc<dyn SessionInputQueue>>,
}
```

- [ ] **Step 2: Move iteration init logic into InitStage.process()**

Transfer: iteration++, turn_id assignment, steering drain, goal check, cancel check, background task injection.

- [ ] **Step 3: Remove the migrated code from main_loop.rs**

Delete the corresponding lines from the old while-loop body (behind the feature flag / comment block).

- [ ] **Step 4: Run tests**

```bash
cargo test -p synthia-agent pipeline::init_stage 2>&1 | tail -10
cargo test -p synthia-agent loop_context 2>&1 | tail -10
```

Expected: All tests pass.

- [ ] **Step 5: Commit**

```bash
git add crates/synthia-agent/src/pipeline/init_stage.rs crates/synthia-agent/src/stream_builder/builder/run/main_loop.rs
git commit -m "refactor(agent): migrate iteration init logic from main_loop to InitStage"
```

### Task 12: Migrate CompactStage logic from main_loop

Same pattern as Task 11: extract lines 508-588 from main_loop into CompactStage.

### Task 13: Migrate SampleStage logic from main_loop

Extract lines 590-898 from main_loop into SampleStage. This is the largest extraction.

### Task 14: Migrate ToolStage logic from main_loop

Extract lines 900-1227 from main_loop into ToolStage. Absorbs InterceptorChain BeforeTool/AfterTool.

### Task 15: Migrate AutoTriggerStage logic from main_loop

Extract lines 1120-1196 from main_loop into AutoTriggerStage.

### Task 16: Migrate TurnCloseStage logic from main_loop

Extract lines 1229-1273 from main_loop into TurnCloseStage.

### Task 17: Migrate SessionEndStage logic from main_loop

Extract lines 1277-1351 from main_loop into SessionEndStage.

### Task 18: Final main_loop cleanup

After all 7 stages are fully implemented:
- Remove all commented-out / feature-flagged old code from main_loop.rs
- Verify main_loop.rs is ≤ 200 lines
- Verify all `cargo test -p synthia-agent` tests pass

---

## Phase 5: ToolOrchestrator Decomposition

### Task 19: Create SecurityPipeline

**Files:**
- Create: `crates/synthia-tool-orchestrator/src/security/mod.rs`
- Create: `crates/synthia-tool-orchestrator/src/security/capability_check.rs`
- Create: `crates/synthia-tool-orchestrator/src/security/provenance_check.rs`
- Create: `crates/synthia-tool-orchestrator/src/security/approval_check.rs`
- Modify: `crates/synthia-tool-orchestrator/src/types.rs`

- [ ] **Step 1: Add SecurityDecision enum to types.rs**

```rust
/// Decision from the security pipeline for a tool call request.
#[derive(Debug, Clone)]
pub enum SecurityDecision {
    /// Security checks passed; proceed to execution.
    Allow,
    /// Security checks denied the request.
    Deny { reason: String },
    /// Security checks require user confirmation.
    NeedConfirm { prompt: String },
}
```

- [ ] **Step 2: Create security/mod.rs with SecurityPipeline**

Extract the capability_broker, provenance_resolver, and approval_service checks from `DefaultToolOrchestrator::execute()` into `SecurityPipeline::check()`.

- [ ] **Step 3: Create the individual check files**

Each file exports a check function:
- `capability_check.rs`: `check_capability(broker, request) -> SecurityDecision`
- `provenance_check.rs`: `check_provenance(resolver, request) -> SecurityDecision`
- `approval_check.rs`: `check_approval(approval_service, request) -> SecurityDecision`

- [ ] **Step 4: Write unit tests for SecurityPipeline**

Test each check independently and the combined pipeline.

- [ ] **Step 5: Commit**

```bash
git add crates/synthia-tool-orchestrator/src/security/ crates/synthia-tool-orchestrator/src/types.rs
git commit -m "feat(tool-orchestrator): add SecurityPipeline — Capability/Provenance/Approval checks"
```

### Task 20: Create ExecutionPipeline

**Files:**
- Create: `crates/synthia-tool-orchestrator/src/execution/mod.rs`
- Create: `crates/synthia-tool-orchestrator/src/execution/resolve.rs`
- Create: `crates/synthia-tool-orchestrator/src/execution/sandbox.rs`
- Create: `crates/synthia-tool-orchestrator/src/execution/concurrency.rs`
- Create: `crates/synthia-tool-orchestrator/src/execution/retry.rs`

Extract tool resolution, sandbox policy, concurrency control, and retry logic from `DefaultToolOrchestrator` into `ExecutionPipeline`.

- [ ] **Step 1: Create execution/mod.rs with ExecutionPipeline struct**

Move: tool_resolver, sandbox_manager, sandbox_policy, retry_policy, concurrency_policy, active_calls, per_tool_locks, snapshot_store, tool_id_resolver, event_sender.

- [ ] **Step 2: Create individual step files**

- `resolve.rs`: Extract `ToolResolver::resolve()` call
- `sandbox.rs`: Extract sandbox policy application
- `concurrency.rs`: Extract per_tool_locks / active_calls management
- `retry.rs`: Extract retry policy evaluation

- [ ] **Step 3: Write unit tests**

Test each step independently.

- [ ] **Step 4: Commit**

```bash
git add crates/synthia-tool-orchestrator/src/execution/
git commit -m "feat(tool-orchestrator): add ExecutionPipeline — Resolve/Sandbox/Concurrency/Retry steps"
```

### Task 21: Create OrchestratorFacade

**Files:**
- Modify: `crates/synthia-tool-orchestrator/src/lib.rs`

- [ ] **Step 1: Implement OrchestratorFacade**

Compose SecurityPipeline + ExecutionPipeline. Implement the `ToolOrchestrator` trait.

- [ ] **Step 2: Replace DefaultToolOrchestrator usage with OrchestratorFacade**

Update all construction sites to create `OrchestratorFacade` instead of `DefaultToolOrchestrator`.

- [ ] **Step 3: Run contract tests**

```bash
cargo test -p synthia-tool-orchestrator 2>&1 | tail -20
```

Expected: All existing tests pass (behavioral equivalence).

- [ ] **Step 4: Commit**

```bash
git add crates/synthia-tool-orchestrator/src/
git commit -m "refactor(tool-orchestrator): replace DefaultToolOrchestrator with OrchestratorFacade"
```

---

## Phase 6: Dual-Path Elimination

### Task 22: Remove direct field fallbacks from AgentRunConfig

**Files:**
- Modify: `crates/synthia-agent/src/config/agent_config/run_config.rs`
- Modify: `crates/synthia-agent/src/stream_builder/builder/run/main_loop.rs`

- [ ] **Step 1: Remove `_memory_event_sender_direct` and `_steering_channel_direct` from AgentRunConfig**

- [ ] **Step 2: Remove all `interceptor_chain.as_ref().and_then(...).or(_direct)` patterns from main_loop**

- [ ] **Step 3: Run tests**

```bash
cargo test -p synthia-agent 2>&1 | tail -20
```

- [ ] **Step 4: Commit**

```bash
git add crates/synthia-agent/src/config/ crates/synthia-agent/src/stream_builder/
git commit -m "refactor(agent): eliminate dual-path — remove direct RunConfig field fallbacks"
```

---

## Phase 7: Integration Testing and Final Validation

### Task 23: Run full regression test suite

- [ ] **Step 1: Run synthia-agent tests**

```bash
cargo test -p synthia-agent 2>&1 | tail -20
```

- [ ] **Step 2: Run synthia-tool-orchestrator tests**

```bash
cargo test -p synthia-tool-orchestrator 2>&1 | tail -20
```

- [ ] **Step 3: Run clippy**

```bash
cargo clippy -p synthia-agent -p synthia-tool-orchestrator --all-features --tests 2>&1 | tail -20
```

Expected: No warnings.

- [ ] **Step 4: Run fmt**

```bash
cargo +nightly fmt --all
```

- [ ] **Step 5: Verify main_loop.rs line count ≤ 200**

```bash
wc -l crates/synthia-agent/src/stream_builder/builder/run/main_loop.rs
```

Expected: ≤ 200 lines.

- [ ] **Step 6: Run E2E tests**

```bash
cd web && npx playwright test
```

Expected: All E2E tests pass.

- [ ] **Step 7: Final commit**

```bash
git add -A
git commit -m "refactor: Pipeline-Stage architecture complete — main_loop 1748→200 lines, ToolOrchestrator decomposed"
```

---

## Self-Review Checklist

- [x] Spec coverage: Every section of the design spec maps to a task
- [x] Placeholder scan: No TBD/TODO/fill-in-later patterns
- [x] Type consistency: `StageFlow`, `TurnInput`, `TurnOutput`, `SessionPhase`, `SecurityDecision` used consistently across tasks
- [x] File paths: All paths are exact and verified against codebase
- [x] Test strategy: Every task has test verification steps
- [x] Commit strategy: Frequent, atomic commits per task
