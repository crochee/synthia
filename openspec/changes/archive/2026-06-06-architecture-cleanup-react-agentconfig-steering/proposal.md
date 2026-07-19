## Why

The agent codebase has three architectural cleanliness issues causing developer confusion and blocking shipped features: (1) the old `ReActLoop` has no deprecation marker despite being replaced; (2) two different structs named `AgentConfig` create import ambiguity; (3) the steering channel feature was fully implemented and tested but never wired into the production loop.

## What Changes

**ReActLoop Deprecation**
- From: `ReActLoop` struct in `react.rs` has no deprecation marker; developers cannot tell which loop to use
- To: `#[deprecated(note = "Use StreamBuilder...")]` attribute added; external consumer migrated
- Reason: D1 decision (2026-06-03) from `agent-architecture-optimization` change was never implemented
- Impact: Non-breaking; deprecation warning guides developers

**AgentConfig Naming Cleanup**
- From: Two structs named `AgentConfig` in `config/agent.rs` and `config/agent_config.rs`
- To: `config/agent.rs::AgentConfig` deleted; only runtime config remains
- Reason: The persona config struct has no production consumers and creates naming confusion
- Impact: Non-breaking; only affects dead code

**Steering Channel Wiring**
- From: `steering_channel` passed through `AgentRunConfig` but destructured and discarded in `run_with_steps`
- To: `try_recv()` drained at iteration start; `SteeringReceived` events yielded; steering content injected as user message
- Reason: 5 test files assert this behavior; feature was clearly intended to ship
- Impact: Non-breaking; enables a shipped feature

## Capabilities

### New Capabilities
- `steering-wire-up`: Out-of-band steering messages are received and injected into the agent context during loop execution

### Modified Capabilities
- `agent-react-loop`: `ReActLoop` marked deprecated; `StreamBuilder` is the production implementation

## Impact

- `crates/synthia-agent/src/react.rs` — deprecation attribute
- `crates/synthia-agent/src/config/agent.rs` — file deleted
- `crates/synthia-agent/src/stream_builder/builder.rs` — steering channel wired
- `synthia-e2e/reasoning_tracking.rs` — consumer migrated
- Tests: steering e2e tests currently fail → should pass after fix