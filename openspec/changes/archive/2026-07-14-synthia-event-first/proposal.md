# Proposal: synthia-event-first (Change 2 of v3 architecture)

**Date**: 2026-07-12
**Status**: Skeleton, awaiting user approval (no auto-commit, no writer launched)
**Parent design**: [`docs/superpowers/specs/2026-07-12-synthia-v3-tool-first-architecture-design.md`](../../../docs/superpowers/specs/2026-07-12-synthia-v3-tool-first-architecture-design.md) §3.2
**Depends on**: `synthia-tool-refactor` (Change 1) — needs `AgentTool` + `ExtensionTool` + `AnyExtensionContext` types
**Absorbs**:
- `extension-points-phase-2` R2-R4 (35 of 43 remaining points: Permission × 5, Provider × 4, Event Bus × 4, Plugin Lifecycle × 6, Session Tree × 5, Output/UI × 4 + 64-point integration test)
- Existing `extension-points/{agent_loop,llm,context}.rs` (Phase 3 + Round 1) — keep as-is; only wire new points

## Why

Synthia's main loop (`crates/synthia-agent/src/stream_builder/builder/run/main_loop.rs` 1037 LOC) is **not** thin. It contains:
- 11 separate `emit_turn_event(...)` call sites for JSONL event emission
- 4+ hardcoded tool-name string comparisons (`SELF_REFLECT_TOOL_NAME`, `compact_context_tool`, `doom_loop_detected`, `sample_cascade_*`)
- Doom-loop handling with hardcoded `DefaultDoomLoopHandler::Cancel` (86 LOC in `doom_loop_handler.rs`)
- LLM-driven-vs-auto compact-context dispatch (lines 752-795)
- `format_background_task_notification` XML inline (lines 82-99)
- 5 separate session-end-reason mutations
- OTel context wrapping (parenthetical in `agent.rs:48-790`)

Meanwhile, **Permission is a 2098-LOC hardcoded layer** in `synthia-permission/src/approval.rs:1355` with `MergedPolicy`/`ApprovalService`/`AskNotifier` machinery that the orchestrator partially bypasses (`synthia-tool-orchestrator/src/lib.rs:595-618` reimplements a 12-line match because the dedicated layer's `pattern.rs` was never invoked).

Three production agents — **opencode** (event-driven `permission.asked`/`replied` bus), **codex** (`AskForApproval` enum + `Granular` + sandbox-denial escalation in `tools/orchestrator.rs:280-468`), **pi-mono** (27 `ExtensionEvent` variants + `extension-first` design) — have independently converged on event-driven everything.

The dependency order is: **events as the source of truth** → permission/doom-loop/compaction/handoff stop being hardcoded branches → main_loop becomes event-driven.

## What Changes

**C2.1** New crate `synthia-event` with:
- `ExtensionEvent` enum (27 typed variants — listed in design.md)
- `ExtensionRegistry` with `emit(event) -> Result<Option<Action<T>>>`
- `ExtensionCtx` three-state lifecycle: `Loading | Active | Stale(reason)`
- `Action<T>` mutation pattern (reuses `Action<T>` from Phase 3 `tool.rs`)
- Wildcard matching per scope
- Every `fire_*` emits `extension.hook` OTel span (P9 hard constraint)

**C2.2** `synthia-permission/approval.rs` shrunk from 1355 → ≤500 LOC; `ApprovalService` gains event-aware variant:
- New `PermissionFuture::from_event(req, reply_tx)` method
- `DefaultPermissionHandler` fires `PermissionAsk` event with oneshot reply channel; 50ms timeout → fail-closed fallback
- All existing `MergedPolicy`/`pattern.rs`/`rule.rs` preserved as observation extensions

**C2.3** DoomLoop event-driven:
- Delete hardcoded `DefaultDoomLoopHandler::Cancel`
- Replace with `DefaultDoomLoopExtension` (subscribes to `ToolCall` events, fires `DoomLoopDetected`)
- Main loop no longer has `doom_loop_detected` branch

**C2.4** Permission **fail-closed default**:
- If no listener fires within 50ms, fallback policy is `Ask` (not `Allow`) — P6
- Per extension, `Action<PermissionDecision>` constrained to "more restrictive only" via runtime guard `PermissionExtensibilityGuard`

**C2.5** StreamBuilder simplified:
- From `BuilderSteps` + 6 step types + 14 type parameters (currently) → `StreamBuilder { extensions, router }` with `Step::Hook | Step::Tool` enum (2 variants)
- All step-specific logic (compact trigger, doom loop, reflect, handoff) moves into individual extensions that subscribe to the right events

**C2.6** `main_loop.rs` shrinks 1037 → ≤400 LOC:
- All JSONL `emit_turn_event` calls removed → JSONL emitter becomes an extension that subscribes to events
- All hardcoded tool-name comparisons removed → discoveries via `ToolRegistry`
- DoomLoop handling: 0 LOC
- Permission gating: 0 LOC
- Compact context dispatch: 0 LOC (handled by event subscribers)
- `format_background_task_notification`: deleted → `OutputSink` extension

**C2.7** Wire 35 of 43 remaining extension points:
- 5 Permission (R2) — `permission.ask`, `permission.notify`, `doom_loop.detected`, `blacklist.match`, `permission.persist`
- 4 Provider (R2) — `provider.register`, `provider.unregister`, `provider.auth`, `provider.fallback`
- 4 Event Bus (R3) — `event.subscribe`, `event.publish`, `event.aggregate`, `event.replay`
- 6 Plugin Lifecycle (R3) — `extension.load`, `extension.bind`, `extension.invalidate`, `extension.unload`, `extension.hot_swap`, `extension.dual_form`
- 5 Session Tree (R4) — `session.tree.append`, `session.tree.branch`, `session.tree.walk`, `session.compaction.preserve`, `session.migrate`
- 4 Output/UI (R4) — `ui.format`, `ui.metadata.patch`, `ui.dialog.confirm`, `ui.dialog.notify`

**C2.8** PermissionFuture sync/async dual-track:
- Keep `ApprovalService::check(...) -> Result<...>` (sync) and add `PermissionFuture` (async)
- One minor cycle, then drop sync

## Capabilities

### New Capabilities

- `ExtensionEvent` 27-variant typed enum
- `ExtensionRegistry` + `ExtensionCtx` three-state
- `Action<T>` mutation pattern with wildcard matching
- `PermissionFuture::from_event` async path
- `DefaultDoomLoopExtension` (event-driven replacement)
- `OutputSink` extension (replaces `format_background_task_notification` XML)
- StreamBuilder simplified to `Step::Hook | Step::Tool` (2 variants)
- `main_loop.rs` ≤ 400 LOC
- 35 extension points wired (Round 2-4)

### Modified Capabilities

- `synthia-permission::ApprovalService` gains `PermissionFuture::from_event`
- `crates/synthia-agent::DoomLoopHandler` → `DefaultDoomLoopExtension`
- `crates/synthia-agent::StepToolExecute` becomes 1 event-emit + 1 result-collect (down from `default_permission_for_tool` switch)

## Risks

| Risk | Mitigation |
|------|-----------|
| 27 events hard to maintain | Force 1 OTel span per emit at the type level (P9) |
| PermissionFuture sync→async double-track | 1 minor deprecation then drop sync |
| DoomLoop event overhead | 50ms timeout fallback if no listener; 0-listener path is non-blocking |
| ExtensionCtx::Stale leaking permissions | `assert_active()` enforced RAII |
| StreamBuilder refactor touches every step | 6 steps become 2 enum variants; migration is additive |
| 8 ExtensionTool internal migrations affect many call sites | Per-tool thin Wrapper; no business logic moves |

## Out of Scope (Deferred)

- Submission/EventMsg wire protocol — **Change 3**
- JSONL append-only session tree — **Change 3**
- Provider hot-swap with source_id isolation — **Change 3 R7**
- 9-abstractions-toolification (external_hook_tool + plugin CLI as Tool) — **Change 3 R8**
- Compile-time extension loading (jiti-style) — explicitly rejected
- WASM tool provider — explicitly rejected

## Reference

- Parent design: [design.md](../../../../docs/superpowers/specs/2026-07-12-synthia-v3-tool-first-architecture-design.md)
- pi-mono pattern: `pi-mono/packages/coding-agent/src/core/extensions/types.ts:950-972` (27-event union), `runner.ts:680-712` (emit/bind/invalidate)
- opencode pattern: `opencode/packages/opencode/src/session/session.ts:355-375`, `permission/index.ts:23-187`
- codex pattern: `codex-rs/core/src/tools/orchestrator.rs:132-482` (approval state machine)
- Existing in-flight: [`extension-points-phase-2`](../extension-points-phase-2/) (43 points plan)
