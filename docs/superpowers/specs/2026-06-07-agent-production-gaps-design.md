# Agent Production Gaps — Design

## Status

Draft | Pending user review

## Context

Analysis of synthia-agent against opencode (reference) and production-grade AI agent standards. Three work packages identified: dead code cleanup (C), architecture migration (A), compaction upgrade (D).

---

## Package C — Dead Code Cleanup

### Scope

Remove code confirmed to have zero production callers within the workspace.

### Items

**C1: `llm.rs::call_llm_streaming` (93 lines)**

- `pub async fn call_llm_streaming` is exported from `lib.rs:23:pub mod llm`
- Workspace search confirms zero callers
- Action: Delete the function. Keep `pub mod llm;` in lib.rs or remove it if the module becomes empty.

**C2: `retry.rs::RetryPolicy` (root level, ~80 lines)**

- `synthia-agent/src/retry.rs` exports `RetryPolicy`, `RetryManager`, `RetryPolicyProvider`
- Different from `synthia-provider/src/retry.rs` (separate crate, no relation)
- `ErrorRecoveryCoordinator` uses `error_recovery/retry.rs::RetryStrategy`, NOT this `RetryPolicy`
- Workspace search: no external callers to root-level `retry.rs` types
- Action: Delete `synthia-agent/src/retry.rs` if confirmed no callers outside that file

**C3: `error_recovery/compact.rs::estimate_tokens`**

- `CompactCoordinator::estimate_tokens(text: &str) -> usize` uses `char / 4` heuristic
- Tests call it; production never calls it (main loop uses `synthia_context` compaction)
- Action: Delete `estimate_tokens` function only. Keep `CompactCoordinator` struct.

**C4: `context_builder.rs` (entire file)**

- `with_system_prompt()` is defined but never called
- `ContextBuilder::build_messages` is never invoked from the main loop
- File is ~50 lines, module is re-exported from `stream_builder/mod.rs`
- Action: Delete file. Remove `pub use context_builder::ContextBuilder` from `stream_builder/mod.rs`. Remove `context: ContextBuilder` field from `StreamBuilder` struct.

**C5: `hook_builder.rs` dead wrapper methods**

- `HookBuilder::fire_after_tool` and `HookBuilder::fire_iteration_end` are wrapper methods
- `fire_after_tool` is called only in `on_tool_error` path (`hooks.rs:105`) and in tests
- `fire_iteration_end` is called only in tests (`hooks.rs:500`)
- Production main path: builder.rs calls `fire_before_llm`, `fire_after_llm`, `fire_before_tool` only
- Action: Delete `fire_after_tool` and `fire_iteration_end` from `HookBuilder`. Keep `on_tool_error` path unchanged (it calls `executor.fire_after_tool` directly, not via `HookBuilder`).

### Risk

Very low. All items confirmed to have zero production callers. Build and test after each item.

### Verification

After each deletion: `cargo build -p synthia-agent && cargo test -p synthia-agent`

---

## Package A — Architecture Migration (Dual-Rail)

### Goal

CLI and Server run on new stream_builder architecture alongside legacy architecture, enabling gradual migration without breaking production.

### Current State

| Consumer | Uses |
|----------|------|
| synthia-cli | legacy `Agent` via `AgentDeps` |
| synthia-server | legacy `Agent` via `AgentDeps` |
| synthia-agent tests | Both `agent::Agent` (legacy) and `stream_builder::Agent` (new) |

Legacy `Agent` is defined in `agent/core.rs`. New `Agent` is defined in `agent.rs` (root level). Both are `pub struct Agent`.

### Migration Path

**Step A-a: Dual-rail agent construction**

Add a feature flag or config field `agent_implementation: "legacy" | "stream_builder"` to `AppConfig` and `ServerConfig`.

- CLI's `build_agent()` and Server's agent construction check this field
- When `"stream_builder"`: construct new `Agent` (stream_builder) with equivalent dependencies
- When `"legacy"`: keep current behavior (default for now)
- Both paths must be functional and tested

**Step A-b: Migrate executors**

`orchestrator.rs` has `BuildExecutor`, `PlanExecutor`, `GeneralExecutor` as empty placeholders.

- Implement the three executors using `stream_builder` steps
- Add `@build` / `@plan` / `@general` routing to main loop

**Step A-c: Switch default**

Flip the default from `"legacy"` to `"stream_builder"` once A-b is verified.

**Step A-d: Remove legacy**

Delete `src/agent/` directory and `src/react.rs` once all consumers are on new architecture.

### Risk

Medium. Two full implementations must coexist without breaking either. Incremental A-a first reduces risk.

### Dependency

C (dead code cleanup) should complete before A begins, to reduce code confusion during migration.

---

## Package D — Compaction Upgrade

### Current State

Compaction exists in three locations:

```
synthia-context/src/manager.rs  ← actual compaction logic
stream_builder/steps/compact.rs ← trigger layer for new architecture
agent/compact.rs               ← trigger layer for old architecture (unused)
```

`StepCompact::check` uses `synthia_context::traits::estimate_message_tokens` for threshold, then calls compaction on `synthia-context`.

Current compaction is simple message truncation. No structural summary.

### Opencode Reference

Opencode compaction produces a structured `CompactionPart` with markdown summary:

```
## Goal
## Constraints
## Progress
## Decisions
## Next Steps
## Critical Context
## Relevant Files
```

Summaries are anchored to previous summary to maintain context across compactions.

### Target Design

**D1: Structured summary compaction**

Extend `synthia-context/manager.rs` compaction to produce structured output:

- Add a `CompactionSummary` struct with fields: goal, constraints, progress, decisions, next_steps, critical_context, relevant_files
- Call LLM with a prompt that extracts these fields from conversation history
- Insert summary as a special `Message` variant (not just truncation)
- Next render treats summary as a context boundary

**D2: Tail preservation**

Implement in `synthia-context/manager.rs`:

- Keep most recent N user turns verbatim (`config.tail_turns`)
- Keep last N tokens of recent assistant messages (`config.compaction.preserve_recent_tokens`)
- Apply before summary generation

**D3: Anchor to previous summary**

- Each compaction summary includes a reference to the previous summary's ID
- LLM prompt for new summary includes: "Previous summary:<anchor>", maintaining continuity

**D4: Integration with new architecture**

- `StepCompact` in `stream_builder` triggers the upgraded compaction
- Old architecture (`agent/compact.rs`) remains unchanged during migration (then deleted with A-d)

### Risk

Medium. Changing compaction affects context assembly, which is a core behavior. Incremental approach: D1 first, verify with tests.

### Dependency

A-a (dual-rail) should be complete so D work is on the new architecture path.

---

## Work Order

```
C →  A-a  →  A-b  →  D →  A-c  →  A-d
      ↑_________↑ ↑
      dual-rail          compaction
      (parallel OK)      (depends on A-a)
```

C is independent and starts immediately.
A-a and D can proceed in parallel after C.
A-b waits for A-a.
D waits for A-a completion.

---

## Out of Scope

- Provider abstraction improvements (separate track)
- Tool system redesign (separate track)
- Session persistence (JSONL → better backend, separate track)
- Permission system overhaul (separate track)