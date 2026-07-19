## Context

Three architecture cleanliness issues in synthia-agent:

**Issue 1 — ReActLoop not deprecated.**
`react.rs` (36KB) is the old ReAct loop implementation. `stream_builder/builder.rs` is the actual production loop. D1 decision (2026-06-03) called for deprecating `ReActLoop` but the `#[deprecated]` attribute was never added. 5 references remain.

**Issue 2 — Two AgentConfig structs with same name.**
`config/agent_config.rs::AgentConfig` (runtime config) and `config/agent.rs::AgentConfig` (persona config) share a struct name. Only the former is re-exported. The latter is effectively dead code with no production consumers.

**Issue 3 — Steering channel silently discarded.**
`stream_builder/builder.rs::run_with_steps` destructures `steering_channel: _` and drops it. 5 test files assert `AgentEvent::SteeringReceived` behavior — the feature was fully implemented and tested but never wired into the production loop.

## Goals / Non-Goals

**Goals:**
- Mark `ReActLoop` deprecated to guide developers to `StreamBuilder`
- Remove dead `AgentConfig` struct to eliminate naming confusion
- Wire steering channel so out-of-band user steering messages work as designed

**Non-Goals:**
- Do not delete `react.rs` in this change (deprecation + consumer fix only; deletion in follow-up)
- Do not change steering protocol or `SteeringMessage` format
- Do not add new features — cleanup only

## Decisions

### D1: ReActLoop deprecation approach

- **選擇**: Add `#[deprecated]` attribute with message pointing to `StreamBuilder` and openspec change; fix 1 external consumer (`synthia-e2e/reasoning_tracking.rs`); delete in follow-up PR
- **理由**: Safe, incremental. External consumers get warning + migration path. Deletion is a separate concern.
- **已考慮 alternative**: Delete immediately — rejected: breaks `synthia-e2e/reasoning_tracking.rs` without warning. Status quo — rejected: D1 decision already made.

### D2: AgentConfig naming collision resolution

- **選擇**: Delete `config/agent.rs` entirely; keep `AgentName` in `config/mod.rs` re-exports
- **理由**: `agent.rs::AgentConfig` has no production consumers. Blast radius limited to file-internal tests. `AgentName` is the only public export from that file.
- **已考慮 alternative**: Rename one of them — rejected: either breaks existing references or creates ongoing confusion. Merge into one struct — rejected: different purposes (runtime vs persona config).

### D3: Steering channel wiring

- **選擇**: Wire at iteration start — drain `steering_channel.try_recv()` once per iteration, yield `SteeringReceived` event, inject steering content as `Message::User` at front of `ctx.messages`
- **理由**: Simple, predictable. Matches feature intent. `MpscSteeringChannel` already honors `priority` internally.
- **已考慮 alternative**: Mid-iteration drainage via loop — rejected: mid-iteration steering not a stated requirement; adds complexity. Remove feature entirely — rejected: tests prove feature was intended.

## Risks / Trade-offs

[Risk] Deleting `config/agent.rs` could break internal tests that reference `AgentConfig` from that file → Mitigation: Run `cargo test -p synthia-agent` after deletion to confirm; fix any broken tests.

[Trade-off] Steering wire-up at iteration start means mid-iteration steering is not supported → Accepted: mid-iteration steering is not a requirement; feature works as designed.

## Migration Plan

N/A — code cleanup, no deployment changes. Rollback via `git revert`.

## Open Questions

None.

## Files to Modify

- `crates/synthia-agent/src/react.rs`
- `crates/synthia-agent/src/lib.rs`
- `crates/synthia-agent/src/config/agent.rs` (delete)
- `crates/synthia-agent/src/config/mod.rs`
- `crates/synthia-agent/src/stream_builder/builder.rs`
- `crates/synthia-agent/src/stream_builder/builder_steps.rs`
- `synthia-e2e/reasoning_tracking.rs`