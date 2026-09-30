# Explicit Recovery Paths Implementation Plan

> **For agentic workers:** Use superpowers:subagent-driven-development
> to implement this plan task-by-task.

**Goal:** Wire up `run_recovery_cascade` (L3-L5) and L1 tool-result truncation into `stream_builder/builder.rs` so the 5-layer recovery system is actually invoked when errors occur, with `AgentEvent::RecoveryApplied` emitted for observability.

**Architecture:** Add 2 state fields to `BuilderSteps` (`reset`, `failure_tracker`); add 1 new `AgentEvent` variant; replace 2 inline error-handling blocks in the agent loop with calls to `run_recovery_cascade`; insert 1 L1 truncate hook at the tool-result injection boundary. No new traits, no API changes to `error_recovery/*`.

**Tech Stack:** Rust, async_stream, synthia_context::truncate, existing error_recovery/* modules.

**Reference:**
- `design.md` — D1-D8 decisions + risks
- `specs/recovery-cascade-wiring/spec.md` — 6 ADDED Requirements
- `tasks.md` — 8 task groups, 22 micro-tasks
- 上轮 archive: `archive/2026-06-12-error-recovery-cascade/` (公共 API 不可修改)

---

## Task 1: AgentEvent Schema Extension

- [ ] **Step 1:** Read `crates/synthia-agent/src/events.rs` to find the `AgentEvent` enum
- [ ] **Step 2:** Add new variant after `LoopWarning`:
  ```rust
  /// Recovery action applied (L1 truncate, L3 fallback, L4 compact, L5 reset).
  /// `level_number`: 1=Truncate, 2=Retry, 3=Fallback, 4=Compact, 5=Reset.
  /// `tool_name`: Some for tool-specific recovery; Some("llm_sample") for LLM errors.
  RecoveryApplied {
      level_number: u32,
      tool_name: Option<String>,
      message: String,
      iteration: usize,
  },
  ```
- [ ] **Step 3:** Run `cargo check -p synthia-agent` — must compile
- [ ] **Step 4:** Commit: `feat(agent): add AgentEvent::RecoveryApplied variant`

## Task 2: BuilderSteps Cascade State

- [ ] **Step 1:** In `stream_builder/builder.rs`, add imports for `ResetCoordinator` and `ConsecutiveFailureTracker`:
  ```rust
  use crate::error_recovery::{
      reset::ResetCoordinator,
      recovery_cascade::ConsecutiveFailureTracker,
  };
  ```
- [ ] **Step 2:** Add 2 fields to `BuilderSteps`:
  ```rust
  pub struct BuilderSteps {
      // ... existing ...
      pub reset: ResetCoordinator,
      pub failure_tracker: ConsecutiveFailureTracker,
  }
  ```
- [ ] **Step 3:** Initialize in `BuilderSteps::new`:
  ```rust
  reset: ResetCoordinator::new(),
  failure_tracker: ConsecutiveFailureTracker::new(),
  ```
- [ ] **Step 4:** Run `cargo check -p synthia-agent` — must compile
- [ ] **Step 5:** Commit: `feat(agent): BuilderSteps carries reset + failure_tracker state`

## Task 3: L1 Truncation at Tool Result Boundary

- [ ] **Step 1:** Write failing integration test in `stream_builder/builder.rs` test module:
  - Setup: configure a tool that returns 50KB string
  - Assert: stream emits `AgentEvent::RecoveryApplied { level_number: 1, ... }`
- [ ] **Step 2:** Run `cargo test -p synthia-agent` to confirm test fails
- [ ] **Step 3:** In the `for result in &tool_results` loop, add truncation:
  ```rust
  use synthia_context::truncate::{truncate_output, TruncateConfig};
  let truncate_cfg = TruncateConfig::default();
  let mut output = result.output.clone();
  let truncated = truncate_output(&output, &truncate_cfg);
  if truncated.was_truncated {
      output = truncated.content;
      yield AgentEvent::RecoveryApplied {
          level_number: 1,
          tool_name: Some(result.tool_name.clone()),
          message: format!(
              "Truncated tool output ({} -> {} bytes)",
              result.output.len(),
              output.len()
          ),
          iteration: ctx.iteration,
      };
  }
  ```
- [ ] **Step 4:** Run the integration test — must pass
- [ ] **Step 5:** Run `cargo test -p synthia-agent` — all tests pass
- [ ] **Step 6:** Commit: `feat(agent): L1 truncate tool results before context injection`

## Task 4: LLM Sampling Error Path → Cascade

- [ ] **Step 1:** Write failing integration test: a scripted provider that returns `Err` from `complete()` triggers cascade → `Recovered` continues OR `FailFast` yields `SessionEnded`
- [ ] **Step 2:** Run test — confirm failure
- [ ] **Step 3:** Replace inline `handle_error(L2Retry)` block in `builder.rs:355-383` with cascade:
  ```rust
  Err(e) => {
      tracing::error!(error = %e, "Sampling failed");
      let action = run_recovery_cascade(
          &e.to_string(),
          "llm_sample",
          &mut ctx,
          &mut steps.failure_tracker,
          &steps.recovery,
          config.context_token_budget.as_ref(),
          config.compaction_provider.as_deref(),
          &mut loop_detectors,
          steps.steering_channel.as_deref().map(|s| s.as_ref()),
          &steps.reset,
      ).await;
      match action {
          RecoveryAction::Recovered(msg) => {
              yield AgentEvent::RecoveryApplied {
                  level_number: msg_level_number(&msg),  // parse from cascade message
                  tool_name: Some("llm_sample".to_string()),
                  message: msg,
                  iteration: ctx.iteration,
              };
              continue;
          }
          RecoveryAction::FailFast(reason) => {
              ctx.set_end_reason(SessionEndReason::Error(reason));
              yield AgentEvent::LlmError { error: e.to_string() };
              yield AgentEvent::SessionEnded { reason: ctx.end_reason.clone().unwrap() };
              return;
          }
          _ => unreachable!("run_recovery_cascade no longer produces Escalate"),
      }
  }
  ```
- [ ] **Step 4:** Helper function to extract level number from cascade message (or change cascade to return `(RecoveryAction, u32)` tuple — see design §Open Q1)
- [ ] **Step 5:** Run integration test — must pass
- [ ] **Step 6:** Commit: `feat(agent): LLM sampling errors trigger L3-L5 recovery cascade`

## Task 5: Tool Execution Error Path → Cascade

- [ ] **Step 1:** Write failing integration test: tool returns `Err` → cascade L3 fallback injects message as `ToolResult { is_error: true, output: fallback_message }`
- [ ] **Step 2:** Run test — confirm failure
- [ ] **Step 3:** Replace inline `Err(e)` arm in `builder.rs:531-541` with cascade:
  ```rust
  Err(e) => {
      tracing::error!(error = %e, "Tool execution failed");
      let action = run_recovery_cascade(
          &e.to_string(),
          &tool_name_on_error,
          &mut ctx,
          &mut steps.failure_tracker,
          &steps.recovery,
          config.context_token_budget.as_ref(),
          config.compaction_provider.as_deref(),
          &mut loop_detectors,
          steps.steering_channel.as_deref().map(|s| s.as_ref()),
          &steps.reset,
      ).await;
      match action {
          RecoveryAction::Recovered(msg) => {
              yield AgentEvent::RecoveryApplied {
                  level_number: parse_cascade_level(&msg),
                  tool_name: Some(tool_name_on_error.clone()),
                  message: msg.clone(),
                  iteration: ctx.iteration,
              };
              tool_results = vec![ToolResult {
                  tool_name: tool_name_on_error.clone(),
                  output: msg,
                  is_error: true,
              }];
          }
          RecoveryAction::FailFast(reason) => {
              ctx.set_end_reason(SessionEndReason::Error(reason));
              yield AgentEvent::SessionEnded { reason: ctx.end_reason.clone().unwrap() };
              return;
          }
          _ => unreachable!(),
      }
  }
  ```
- [ ] **Step 4:** Run integration test — must pass
- [ ] **Step 5:** Commit: `feat(agent): tool execution errors trigger L3-L5 recovery cascade`

## Task 6: AgentRunConfig Compaction Provider

- [ ] **Step 1:** Read `crates/synthia-agent/src/config.rs::AgentRunConfig`
- [ ] **Step 2:** If `compaction_provider` field is absent, add:
  ```rust
  pub compaction_provider: Option<Arc<dyn synthia_context::compaction::compactor::CompactionProvider>>,
  ```
- [ ] **Step 3:** Update `AgentRunConfig::default()` to set the field to `None`
- [ ] **Step 4:** Pass the new field into `run_recovery_cascade` calls in builder.rs
- [ ] **Step 5:** Run `cargo check -p synthia-agent` — must compile
- [ ] **Step 6:** Commit: `feat(agent): AgentRunConfig carries compaction_provider for L4 cascade`

## Task 7: End-to-End Integration Tests

- [ ] **Step 1:** Test: 3 consecutive LLM sampling errors → L5 reset → `ctx.messages` cleared, 4th call succeeds
- [ ] **Step 2:** Test: tool execution error with `bash` fallback → L3 fallback message appears in next LLM call's context
- [ ] **Step 3:** Test: oversized tool result (50KB) → L1 truncate → `RecoveryApplied { level_number: 1, tool_name: Some("..."), message: "Truncated tool output (50000 -> 30000 bytes)", iteration }`
- [ ] **Step 4:** Run all 3 — must pass
- [ ] **Step 5:** Commit: `test(agent): E2E tests for explicit recovery paths`

## Task 8: Verification

- [ ] **Step 1:** `cargo +nightly fmt --all` — must be clean
- [ ] **Step 2:** `cargo clippy --all-targets --all-features --tests --all` — 0 new warnings
- [ ] **Step 3:** `cargo test -p synthia-agent` — all tests pass
- [ ] **Step 4:** `openspec validate explicit-recovery-paths` — pass
- [ ] **Step 5:** `openspec validate auto-compact-on-error session-reset tool-fallback tool-output-truncate tool-retry` — all 5 archive specs continue to pass
- [ ] **Step 6:** `cargo test --workspace` — verify no regression in other crates (allow pre-existing `synthia-session` compile errors)
- [ ] **Step 7:** Commit any formatting/lint fixes

---

## Notes for Implementer

- **Helper for level number**: Task 4/5 need to know which level (3/4/5) the cascade applied. Two options:
  - (A) Change `run_recovery_cascade` to return `(RecoveryAction, u32)` — clean but modifies cascade API
  - (B) Parse the message string (e.g. "Context auto-compacted..." → 4) — fragile
  - **Recommended**: (A) — but careful, may need to update 13 unit tests in `recovery_cascade.rs`. Alternative: add a `level: RecoveryLevel` field to `RecoveryAction::Recovered(String)`. Decide before Task 4.
- **TruncateConfig location**: `StepSample` has its own `truncate_cfg`. Reuse `TruncateConfig::default()` for tool results (Task 3) to avoid cross-step coupling.
- **Async context**: `run_recovery_cascade` is async. The `stream! { ... }` block in `builder.rs` is already async (`async_stream`), so `.await` works directly.
- **CompactionProvider trait**: Located at `synthia_context::compaction::compactor::CompactionProvider`. Confirm the import path in Task 6.

---

## Estimated Effort

- 22 micro-tasks across 8 groups
- Each micro-task: 2-5 minutes (TDD-style)
- Total: ~2-3 hours of focused implementation
- Phases: 1 (event) → 2 (state) → 3 (L1) → 4 (LLM cascade) → 5 (tool cascade) → 6 (config) → 7 (E2E) → 8 (verify)
