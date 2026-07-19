# Brainstorming: Track B — Architecture Cleanup (react.rs + AgentConfig + Steering)

## Background

Three architecture issues identified:
1. `ReActLoop` in `react.rs` has no `#[deprecated]` mark despite being the old implementation
2. Two `AgentConfig` structs with same name in `config/agent.rs` and `config/agent_config.rs`
3. Steering channel silently discarded in `stream_builder/builder.rs`

## Decision Chain

### Issue 3: Steering Channel — Wire Up or Remove?

**Evidence from codebase:**
- 5 test files assert `AgentEvent::SteeringReceived` behavior (none `#[ignore]`'d)
- Trait `SteeringChannel` + impl `MpscSteeringChannel` fully implemented and tested
- `SteeringMessage.priority` already honored by internal ordering
- `AgentRunConfig.steering_channel` exists and passed through — just dropped at the last step

**Options:**
- (a) Wire it up — drain `try_recv()` per iteration, yield `SteeringReceived`, inject as user message
- (b) Remove entirely — delete files, tests, `SteeringReceived` variant
- (c) Document and leave — `// TODO`, broken tests

**Decision: (a) Wire it up.** Tests already encode expected behavior. Feature was clearly intended to ship.

### Issue 1: ReActLoop Deprecation Path

**Options:**
- Deprecate-then-delete in follow-up PR (safe, incremental)
- Delete immediately (risky — breaks external consumers)

**Decision: Deprecate-then-delete.** Add `#[deprecated]`, fix 1 external consumer (`synthia-e2e/reasoning_tracking.rs`), delete in follow-up.

### Issue 2: AgentConfig Naming Collision

**Analysis:** `config/agent.rs::AgentConfig` (persona definition) is not re-exported from `config/mod.rs`. Only `AgentName` is used from that file. Blast radius contained.

**Decision: Delete `config/agent.rs` entirely.** Keep `AgentName`. Run tests after deletion to confirm.

## Design Trade-offs

### Steering Wire-up Implementation

| Approach | Pros | Cons |
|----------|------|------|
| Drain at iteration start | Simple, predictable | Misses mid-iteration steering |
| Drain in `try_recv()` loop | More responsive | More complex loop logic |

**Chosen: Drain at iteration start (top of `run_with_steps` loop).** Mid-iteration steering is not a stated requirement. Simplicity wins.

### ReActLoop Deprecation Message

Need to clearly indicate:
- What to use instead (`StreamBuilder`)
- When it will be removed ("once external consumers migrate")
- Where to learn more (openspec change)

## Output

Design doc committed to `docs/superpowers/specs/2026-06-06-track-b-architecture-cleanup-design.md`.

## Verification

- `cargo test -p synthia-agent` passes
- Steering e2e tests pass
- `cargo clippy` clean