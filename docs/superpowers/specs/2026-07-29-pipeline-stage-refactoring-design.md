# Pipeline-Stage AI Agent Refactoring Design

**Date**: 2026-07-29
**Status**: ⚠️ **DEPRECATED** — see [ADR 001: Pipeline-Stage Architecture — Deprecated](../../adrs/2026-07-31-pipeline-stage-architecture-deprecated.md)
**Scope**: Internal refactoring — no public API / A2A protocol breakage

> **DEPRECATED 2026-07-31**: The 9-commit Pipeline-Stage refactor was rolled back in commit `68c05d33` because it violated the "one-shot, no shims" principle. The current `crates/synthia-agent/src/stream_builder/builder/run/main_loop.rs` is the production implementation. Do not re-attempt this refactor without first satisfying all six conditions in ADR 001.

***

## 1. Problem Statement

### 1.1 God Function: main\_loop.rs (1748 lines)

The `StreamBuilder::run_with_steps()` method in `crates/synthia-agent/src/stream_builder/builder/run/main_loop.rs` is a 1748-line `stream!` macro block that mixes 8+ concerns:

1. Session initialization / restoration
2. Iteration control (should\_stop / iteration / turn\_id)
3. Compaction check and execution
4. LLM sampling and cascade
5. Tool execution and interception
6. Loop detection / guardian
7. Hook dispatch (7 HookEvent types)
8. Session termination / reflection

This violates Single Responsibility Principle at the function level. The cyclomatic complexity makes it nearly impossible to reason about individual behaviors, and any change risks unintended side effects across the monolith.

### 1.2 God Object: ToolOrchestrator (1764 lines)

`DefaultToolOrchestrator` in `crates/synthia-tool-orchestrator/src/lib.rs` has:

* 12 `pub(crate)` fields mixing security, execution, and lifecycle concerns

* `execute()` method combining: permission check + sandbox + conflict detection + concurrency lock + retry + execution

* `execute_batch()` duplicating the same mixed logic × N

Security concerns (Capability/Provenance/Approval) are interleaved with execution concerns (Resolve/Sandbox/Concurrency/Retry), making both harder to test and reason about independently.

### 1.3 Anemic Model: LoopContext

`LoopContext` has 14 `pub` fields with no encapsulation. External code directly mutates state (e.g., `ctx.iteration`, `ctx.forwarded_this_turn`), leading to scattered mutation points and no state transition validation.

### 1.4 Dual-Path Coexistence

The Registry-First migration left `InterceptorChain` (new path) and direct `AgentRunConfig` fields (legacy path) coexisting. The main loop contains scattered `interceptor_chain.as_ref().and_then(...).or(_direct_field)` fallback patterns.

### 1.5 Fragmented Event System

5 parallel event systems:

* `AgentEvent` (Model / ModelDone / Hook / System)

* `SystemEvent` (SessionStarted / SessionEnded / Warning / Iteration\*)

* `HookEvent` (7 payload types)

* `ToolOrchestratorEvent` (Started / Completed / Failed)

* `InterceptorEvent` (BeforeTool / AfterTool / SessionEnd)

Stage inter-communication uses ad-hoc `yield` statements without a unified model.

***

## 2. Design Overview

### 2.1 Architecture: Pipeline-Stage

```
SessionInput ──▶ ┌──────────┐   ┌──────────┐   ┌──────────┐
                  │ Stage 1  │──▶│ Stage 2  │──▶│ Stage 3  │──▶ ...
                  │ InitStg  │   │CompactChk│   │SampleStg │
                  └──────────┘   └──────────┘   └──────────┘
                       │              │              │
                       ▼              ▼              ▼
                  ┌──────────────────────────────────────────┐
                  │         LoopContext (shared bus)          │
                  │  + SessionPhase state machine             │
                  └──────────────────────────────────────────┘
```

Each Stage is an independent struct implementing the `Stage` trait. The Pipeline driver iterates Stages sequentially. Stage output (`StageOutput`) determines control flow: `Continue`, `Skip` (advance to next iteration), or `Terminate` (end session).

### 2.2 ToolOrchestrator: Dual-Pipeline

```
┌─────────────────────────────────────────────┐
│  OrchestratorFacade (implements trait)       │
│  ┌──────────────┐    ┌──────────────┐       │
│  │ Security     │───▶│ Execution    │       │
│  │ Pipeline     │    │ Pipeline     │       │
│  │ (Cap→Prov→   │    │ (Resolve→    │       │
│  │  Approve)    │    │  Sandbox→    │       │
│  │              │    │  Concur→     │       │
│  │              │    │  Exec→Retry) │       │
│  └──────────────┘    └──────────────┘       │
└─────────────────────────────────────────────┘
```

The existing `ToolOrchestrator` trait (public API) is preserved. `OrchestratorFacade` implements it by composing `SecurityPipeline` + `ExecutionPipeline`.

***

## 3. Detailed Design

### 3.1 Stage Trait and Pipeline Driver

```rust
/// Input context for a single turn iteration.
pub struct TurnInput {
    pub cancel_token: CancellationToken,
    pub session_input_queue: Option<Arc<dyn SessionInputQueue>>,
}

/// Output collected from a single turn iteration.
pub struct TurnOutput {
    pub events: Vec<AgentEvent>,
}

/// Stage output determines pipeline control flow.
pub enum StageFlow {
    /// Normal: proceed to next Stage.
    Continue,
    /// Skip remaining Stages, advance to next iteration.
    Skip,
    /// Terminate the session with the given reason.
    Terminate(SessionEndReason),
}

/// Single Stage in the agent pipeline.
#[async_trait]
pub trait Stage: Send + Sync {
    /// Process this stage.
    ///
    /// - `ctx` is the shared LoopContext (pipeline bus).
    /// - `input` is the turn-level input.
    /// - `output` collects events to be yielded by the stream.
    ///
    /// Returns `StageFlow` to control pipeline progression.
    async fn process(
        &self,
        ctx: &mut LoopContext,
        input: &TurnInput,
        output: &mut TurnOutput,
    ) -> StageFlow;
}

/// Ordered sequence of Stages.
pub struct Pipeline {
    stages: Vec<Box<dyn Stage>>,
}

impl Pipeline {
    /// Run all stages for one turn iteration.
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

**Key decision**: Stages write events to `TurnOutput.events`, not directly `yield`. The top-level stream driver collects and yields events. This makes Stages pure logic — testable without async stream machinery.

### 3.2 Seven Stages

#### Stage 1: InitStage (\~80 lines)

Responsibilities:

* Increment iteration counter

* Assign new turn\_id

* Drain steering channel

* Check goal status (achieved/blocked → Terminate)

* Check cancellation (cancelled → Terminate)

* Emit turn start events

* Check completed background sub-agents

#### Stage 2: CompactStage (\~60 lines)

Responsibilities:

* Dispatch PreCompact hook

* Run `StepCompact::check()`

* On `MustCompact`: execute compaction, dispatch PostCompact hook, emit warning → Skip

* On `Warning`: emit token budget warning → Continue

* On `None`: → Continue

#### Stage 3: SampleStage (\~120 lines)

Responsibilities:

* Build tool definitions

* Dispatch UserPromptSubmit and PreResponse hooks

* Capture prefix snapshot (pre-LLM)

* Call `sample_llm_and_cascade()`

* On `Continue` cascade: emit events → Skip

* On `Terminate` cascade: emit events → Terminate

* On `Done`: record sampling, dispatch PostResponse hook

* Accumulate token usage, record in rollout tracker

* Capture prefix snapshot (post-LLM), emit stability event

* If no tool calls: set end\_reason=Completed, add assistant message, emit ModelDone → Terminate(Completed)

* Check doom loop → Terminate(LoopDetected)

* Emit ModelDone, add assistant message → Continue

#### Stage 4: ToolStage (\~150 lines)

Responsibilities:

* Transition turn to Executing status

* Emit TOOL\_CALL\_ISSUED event

* Run InterceptorChain BeforeTool dispatch (skipped tools recorded)

* Execute tools via `execute_and_emit()`

* On Continue: record file changes in rollout tracker, run InterceptorChain AfterTool dispatch → Continue

* On Terminate: → Terminate

This stage absorbs the currently-scattered InterceptorChain logic (BeforeTool/AfterTool) that currently lives in main\_loop lines 928-1118.

#### Stage 5: AutoTriggerStage (\~80 lines)

Responsibilities:

* Run `maybe_auto_trigger_self_reflect()`

* Run `maybe_auto_trigger_compact_context()`

* Handle LLM-driven compact\_context (dedup with auto-trigger)

* Dispatch PreCompact/PostCompact for LLM-driven compaction

* Emit compaction analytics → Continue

#### Stage 6: TurnCloseStage (\~60 lines)

Responsibilities:

* Emit TOOL\_RESULT\_RECEIVED event

* Transition turn to Completed status

* Emit TURN\_COMPLETED event

* Update goal tracker budget → Continue

#### Stage 7: SessionEndStage (\~50 lines)

Responsibilities:

* Only active when LoopContext indicates session should end

* Set end\_reason fallback (MaxIterationsReached if iteration >= cap)

* Dispatch SessionEnd hook

* Dispatch InterceptorChain SessionEnd

* Emit SESSION\_ENDED event

* Run end\_of\_session\_reflect()

### 3.3 Main Loop Driver (\~200 lines)

The existing 1748-line `run_with_steps()` becomes:

```rust
impl StreamBuilder {
    pub(crate) fn run_with_steps(/* same params */) -> Pin<Box<dyn Stream<Item = AgentEvent> + Send>> {
        // ... session initialization (~100 lines, unchanged) ...
        // ... LoopContext setup, FragmentRegistry, etc. ...

        let pipeline = Pipeline::new(/* construct 7 stages */);

        Box::pin(stream! {
            yield AgentEvent::System(SystemEvent::SessionStarted { session_id });

            while !ctx.should_stop_with_timeout(config.max_iterations, config.session_wall_clock_timeout) {
                let (turn_output, flow) = pipeline.run_turn(&mut ctx, &input).await;
                for ev in turn_output.events {
                    yield ev;
                }
                if let StageFlow::Terminate(_) = flow {
                    break;
                }
            }

            // Session end logic (delegated to SessionEndStage or inline ~50 lines)
            // ...
        })
    }
}
```

### 3.4 LoopContext Enhancement

```rust
/// Session phase for state machine validation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionPhase {
    Idle,
    Running,
    Compacting,
    Reflecting,
    Ending,
}

pub struct LoopContext {
    // ── Session identity ──
    session_id: String,              // pub → pub(crate)
    iteration: usize,                 // pub → pub(crate)
    current_turn_id: Option<TurnId>, // pub → pub(crate)

    // ── Phase state machine (NEW) ──
    phase: SessionPhase,
    phase_since: Instant,

    // ── Messages ──
    messages: Vec<Message>,           // pub → pub(crate)
    end_reason: Option<SessionEndReason>,

    // ── Tokens ──
    cumulative_tokens: usize,
    context_token_limit: Option<usize>,

    // ── Tool results ──
    recent_tool_results: Vec<(String, String, bool)>,

    // ── Auto-trigger ──
    next_self_reflect_iteration: usize,
    forwarded_this_turn: usize,

    // ── Lifecycle ──
    session_start: Option<Instant>,
    registration_scope: Option<RegistrationScope>,
    span_ctx: SpanContext,
    needs_compact: bool,
}

impl LoopContext {
    /// Transition to a new phase. Validates the transition.
    /// Invalid transitions log a warning in production, panic in debug.
    pub fn transition_to(&mut self, phase: SessionPhase) {
        debug_assert!(self.is_valid_transition(&self.phase, &phase),
            "Invalid phase transition: {:?} → {:?}", self.phase, phase);
        if !self.is_valid_transition(&self.phase, &phase) {
            tracing::warn!(from = ?self.phase, to = ?phase, "Invalid phase transition");
        }
        self.phase = phase;
        self.phase_since = Instant::now();
    }

    /// Public read accessors (replace direct field access).
    pub fn session_id(&self) -> &str { &self.session_id }
    pub fn iteration(&self) -> usize { self.iteration }
    pub fn current_turn_id(&self) -> Option<TurnId> { self.current_turn_id }
    pub fn messages(&self) -> &[Message] { &self.messages }
    pub fn phase(&self) -> &SessionPhase { &self.phase }

    /// Mutation methods (replace direct field mutation).
    pub fn increment_iteration(&mut self) { self.iteration += 1; }
    pub fn push_message(&mut self, msg: Message) { self.messages.push(msg); }
    pub fn set_forwarded_this_turn(&mut self, count: usize) { self.forwarded_this_turn = count; }

    // ... existing methods preserved: add_tool_result, add_assistant_message_from_sampling, etc.

    fn is_valid_transition(&self, from: &SessionPhase, to: &SessionPhase) -> bool {
        matches!((from, to),
            (Idle, Running) |
            (Running, Compacting) |
            (Running, Reflecting) |
            (Running, Ending) |
            (Compacting, Running) |
            (Reflecting, Running) |
            (_, Ending)
        )
    }
}
```

### 3.5 Event Model

Stages write `AgentEvent`s directly to `TurnOutput.events`. The top-level stream driver yields them in order. This keeps the external SSE event stream unchanged.

For Stage-to-Stage communication that does not map to `AgentEvent` (e.g., "this iteration used LLM-driven compact\_context" → auto-trigger should skip), Stages read/write typed flags on `LoopContext`:

```rust
// Example: flag set by SampleStage, read by AutoTriggerStage
impl LoopContext {
    /// Mark that LLM-driven compact_context was called this iteration.
    /// AutoTriggerStage reads this to skip auto-compact dedup.
    pub fn set_llm_compact_called(&mut self) {
        self.llm_compact_called_this_iter = true;
    }

    pub fn take_llm_compact_called(&mut self) -> bool {
        std::mem::take(&mut self.llm_compact_called_this_iter)
    }
}
```

This avoids a parallel `StageEvent` enum that would need mapping logic. The principle: **events for the outside →** **`AgentEvent`; signals between stages → LoopContext flags**.

### 3.6 Dual-Path Elimination

Remove the following direct fields from `AgentRunConfig`:

* `_memory_event_sender_direct` → always via `InterceptorChain::memory_event_sender()`

* `_steering_channel_direct` → always via `InterceptorChain::steering_channel()`

The `InterceptorChain` becomes the sole access path. The fallback `or()` patterns in main\_loop are removed. If `InterceptorChain` is `None`, the service is simply unavailable (no silent fallback to stale direct bindings).

### 3.7 ToolOrchestrator Dual-Pipeline

#### SecurityPipeline (\~300 lines)

```rust
pub struct SecurityPipeline {
    capability_broker: Option<Arc<CapabilityBroker>>,
    provenance_resolver: Option<Arc<dyn ToolProvenanceResolver>>,
    approval_service: Arc<dyn ApprovalService>,
}

pub enum SecurityDecision {
    Allow,
    Deny { reason: String },
    NeedConfirm { prompt: String },
}

impl SecurityPipeline {
    /// Run all security checks in sequence: Capability → Provenance → Approval.
    /// Short-circuits on first Deny.
    pub async fn check(&self, request: &ToolCallRequest) -> SecurityDecision {
        // Step 1: Capability check
        if let Some(ref broker) = self.capability_broker {
            // ... check tool capability
        }
        // Step 2: Provenance check
        if let Some(ref resolver) = self.provenance_resolver {
            // ... check provenance floor
        }
        // Step 3: Approval check
        // ... consult approval_service
    }
}
```

#### ExecutionPipeline (\~400 lines)

```rust
pub struct ExecutionPipeline {
    tool_resolver: Arc<dyn ToolResolver>,
    sandbox_manager: Arc<dyn SandboxManager>,
    sandbox_policy: SandboxPolicy,
    retry_policy: RetryPolicy,
    concurrency_policy: ConcurrencyPolicy,
    active_calls: Arc<DashMap<String, ActiveCall>>,
    per_tool_locks: Arc<DashMap<String, Arc<Mutex<()>>>>,
    snapshot_store: Arc<RwLock<HashMap<PathBuf, FileSnapshot>>>,
    tool_id_resolver: Option<Arc<dyn ToolIdResolver>>,
    event_sender: broadcast::Sender<ToolOrchestratorEvent>,
}

impl ExecutionPipeline {
    /// Execute a single tool call through: Resolve → Sandbox → Concurrency → Execute → Retry.
    pub async fn execute(
        &self,
        request: ToolCallRequest,
        context: ExecutionContext,
        cancel_token: CancellationToken,
    ) -> Result<ToolCallResult, ToolOrchestratorError> {
        // Step 1: Resolve tool
        // Step 2: Apply sandbox policy
        // Step 3: Acquire concurrency lock
        // Step 4: Execute tool call
        // Step 5: Retry if needed
    }
}
```

#### OrchestratorFacade (\~200 lines)

```rust
pub struct OrchestratorFacade {
    security: SecurityPipeline,
    execution: ExecutionPipeline,
}

#[async_trait]
impl ToolOrchestrator for OrchestratorFacade {
    async fn execute(
        &self,
        request: ToolCallRequest,
        context: ExecutionContext,
        cancel_token: CancellationToken,
    ) -> Result<ToolCallResult, ToolOrchestratorError> {
        // Security check first
        match self.security.check(&request).await {
            SecurityDecision::Deny { reason } => {
                return Err(ToolOrchestratorError::Denied {
                    call_id: request.call_id.clone(),
                    reason,
                });
            }
            SecurityDecision::NeedConfirm { prompt } => {
                // ... delegate to approval flow
            }
            SecurityDecision::Allow => {}
        }
        // Then execute
        self.execution.execute(request, context, cancel_token).await
    }

    // ... other trait methods delegate to execution pipeline
}
```

***

## 4. File Structure

### 4.1 New Files

```
crates/synthia-agent/src/
├── pipeline/                        # NEW
│   ├── mod.rs                       # Pipeline driver (~150 lines)
│   ├── stage.rs                     # Stage trait + StageFlow + TurnInput/Output
│   ├── init_stage.rs                # Stage 1 (~80 lines)
│   ├── compact_stage.rs             # Stage 2 (~60 lines)
│   ├── sample_stage.rs              # Stage 3 (~120 lines)
│   ├── tool_stage.rs                # Stage 4 (~150 lines)
│   ├── auto_trigger_stage.rs        # Stage 5 (~80 lines)
│   ├── turn_close_stage.rs          # Stage 6 (~60 lines)
│   └── session_end_stage.rs         # Stage 7 (~50 lines)

crates/synthia-tool-orchestrator/src/
├── security/                        # NEW
│   ├── mod.rs                       # SecurityPipeline (~300 lines)
│   ├── capability_check.rs          # Capability broker step
│   ├── provenance_check.rs          # Provenance resolver step
│   └── approval_check.rs            # Approval service step
├── execution/                       # REFACTORED
│   ├── mod.rs                       # ExecutionPipeline (~400 lines)
│   ├── resolve.rs                   # Tool resolution
│   ├── sandbox.rs                   # Sandbox policy application
│   ├── concurrency.rs               # Concurrency control (per_tool_locks)
│   └── retry.rs                     # Retry policy evaluation
```

### 4.2 Modified Files

| File                                                        | Change                                            |
| ----------------------------------------------------------- | ------------------------------------------------- |
| `synthia-agent/src/stream_builder/builder/run/main_loop.rs` | 1748 → \~200 lines (thin driver)                  |
| `synthia-agent/src/loop_context.rs`                         | Add SessionPhase, privatize fields, add accessors |
| `synthia-agent/src/events.rs`                               | Add StageFlow enum, remove unused variants        |
| `synthia-tool-orchestrator/src/lib.rs`                      | 1764 → \~200 lines (OrchestratorFacade)           |
| `synthia-tool-orchestrator/src/types.rs`                    | Add SecurityDecision enum                         |

### 4.3 Unchanged Files

All files outside `synthia-agent` and `synthia-tool-orchestrator` crates. The public API surface (`ToolOrchestrator` trait, `AgentEvent`, SSE protocol, A2A protocol) remains unchanged.

***

## 5. Testing Strategy

### 5.1 Per-Stage Unit Tests

Each Stage has its own `#[cfg(test)] mod tests` block. Stages are pure logic — testable with mock `LoopContext`:

```rust
#[tokio::test]
async fn compact_stage_must_compact_skips_remaining() {
    let mut ctx = LoopContext::new("test".into(), SpanContext::new("test"));
    ctx.needs_compact = true;
    ctx.context_token_limit = Some(100);
    // ... set up messages that exceed budget

    let stage = CompactStage::new(/* ... */);
    let input = TurnInput::default();
    let mut output = TurnOutput::default();
    let flow = stage.process(&mut ctx, &input, &mut output).await;

    assert!(matches!(flow, StageFlow::Skip));
    assert!(output.events.iter().any(|e| /* compaction warning */));
}
```

### 5.2 Pipeline Integration Tests

Test that Stage sequencing produces the same observable behavior as the original monolith:

```rust
#[tokio::test]
async fn pipeline_text_only_response_terminates_with_completed() {
    let pipeline = Pipeline::new(/* all 7 stages with mocks */);
    let mut ctx = LoopContext::new("test".into(), SpanContext::new("test"));
    ctx.messages.push(Message::user("hello"));

    let (output, flow) = pipeline.run_turn(&mut ctx, &TurnInput::default()).await;

    assert!(matches!(flow, StageFlow::Terminate(SessionEndReason::Completed)));
}
```

### 5.3 OrchestratorFacade Contract Tests

Verify that the refactored OrchestratorFacade produces identical results to the original DefaultToolOrchestrator for the same inputs:

```rust
#[tokio::test]
async fn facade_denies_when_security_pipeline_deny() {
    let facade = OrchestratorFacade::new(/* security that always denies */, execution);
    let result = facade.execute(request, context, cancel).await;
    assert!(matches!(result, Err(ToolOrchestratorError::Denied { .. })));
}
```

### 5.4 Regression: Existing Tests

All existing test suites are preserved. After refactoring:

* Run `cargo test -p synthia-agent` — must pass

* Run `cargo test -p synthia-tool-orchestrator` — must pass

* Run E2E Playwright tests — must pass

* Run `cargo clippy --all-targets --all-features --tests --all` — no warnings

***

## 6. Success Criteria

1. **Complexity reduction**: No single function exceeds 300 lines. main\_loop.rs ≤ 200 lines.
2. **Cyclomatic complexity**: Per-function CC ≤ 15 (measured by `cargo clippy`).
3. **Test coverage**: Every Stage has unit tests covering all StageFlow branches.
4. **Behavioral equivalence**: All existing integration/E2E tests pass without modification.
5. **No API breakage**: `ToolOrchestrator` trait, `AgentEvent`, SSE stream unchanged.
6. **Dead code eliminated**: No `#[allow(dead_code)]` or `#[allow(unused)]` in new code.

***

## 7. Risks and Mitigations

| Risk                               | Mitigation                                                                                  |
| ---------------------------------- | ------------------------------------------------------------------------------------------- |
| Stage ordering sensitivity         | Pipeline::run\_turn() processes Stages in fixed order; order is part of the type contract   |
| LoopContext mutation across Stages | Each Stage documents which fields it reads/writes; transition\_to() validates phase changes |
| Async Stream + Stage interaction   | Stages don't yield; they write to TurnOutput. Only the top-level stream! macro yields       |
| Regression in tool execution       | OrchestratorFacade contract tests validate identical behavior                               |
| `stream!` macro limitations        | The top-level driver remains a `stream!` block but is now \~200 lines                       |

***

## 8. Out of Scope

* Event-driven architecture (Scheme B) — rejected in favor of Pipeline

* Public API changes to ToolOrchestrator trait

* A2A protocol changes

* Frontend changes

* New features — this is purely structural refactoring

