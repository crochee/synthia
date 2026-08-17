# synthia-agent

Agent runtime + multi-agent registry for the Synthia framework.

## Features

- **Agent trait**: an async [`Agent`] contract every agent paradigm
  (ReAct, pipeline, planner, router, …) implements. Streams
  [`AgentEvent`] in real time.
- **`AgentRegistry`**: multi-agent catalog implementing
  [`synthia_core::registry::Registry`] with descriptor metadata +
  capability filtering.
- **`ReActAgent`**: the canonical ReAct-loop implementation,
  reusing the existing ReAct cycle through a `mpsc`-bridged
  [`AgentEvent`] stream.
- **Cancellation**: `tokio_util::sync::CancellationToken` is
  observed between iterations and between stream chunks.

## `AgentDescriptor` (industry-aligned)

`AgentDescriptor` mirrors the de-facto shape used by the Anthropic
Agents SDK, the OpenAI Swarm/Agents SDK, and the A2A / MCP-aligned
reference designs. Fields are additive — older payloads that
populate only `name`, `description`, `kind`, `version`, and
`capabilities` keep deserializing.

| Field | Industry source | Purpose |
|---|---|---|
| `name` / `description` | Anthropic `name`, OpenAI `name` + `handoffDescription` | Identity in traces, handoff surfaces |
| `kind` / `version` | — | Paradigm + schema revision |
| `instructions` | Anthropic + OpenAI `instructions` | System prompt; externally inspectable |
| `capabilities` | — | Coarse capability tags (`streaming`, `cancellation`) |
| `tools` | OpenAI `tools` | Concrete tool names the agent calls |
| `model_hint` | Anthropic + OpenAI `model` | Preferred model identifier |
| `handoffs` | OpenAI `handoffs` | Names of agents this specialist can route to |
| `handoff_hint` | OpenAI `handoffDescription` | Short label for orchestrators |
| `output_schema` | OpenAI `outputType` | Optional JSON-schema for structured outputs |
| `owner` / `domain` | — | Multi-tenant routing + observability |

## Multi-expert adversarial design

`AgentDescriptor` is pure identity + capability metadata.
Multi-expert adversarial patterns (proposer / critic / judge
fan-outs, voting, debate protocols) are **not** baked into the
descriptor — they are runtime policy composed by the caller
(server / orchestrator) using the registry plus per-agent
prompt context.

A caller wanting a multi-expert ensemble:

1. Registers each specialist agent in `AgentRegistry`.
2. Resolves the configured default (or explicit name) via
   `resolve_sync`.
3. Wraps the run in its own orchestration logic that fans
   out / aggregates per the chosen strategy. The agent
   loop itself is unaware of the panel.

`ReActAgent` carries a `persona` (single short role-framing
sentence) so each specialist can present a coherent voice to
the LLM without needing panel-specific metadata.

## Public API

```rust,ignore
use std::sync::Arc;
use futures::StreamExt;
use synthia_agent::{
    Agent, AgentInput, AgentRunConfig, ReActAgent,
};
use tokio_util::sync::CancellationToken;

let agent = Arc::new(ReActAgent::from_config(AgentRunConfig {
    provider,
    tool_registry,
    user_id,
    session_id,
    input: AgentInput::text("hello"),
    cancel_token: CancellationToken::new(),
    workspace_root: PathBuf::from("/abs/path/to/project"),
}));

let mut stream = agent
    .run(AgentInput::text("hello"), Arc::new(CancellationToken::new()))
    .await;
while let Some(event) = stream.next().await {
    // forward / render / persist
}
```

`AgentRunConfig` carries the live `ModelProvider`, `ToolRegistry`,
and cancellation primitive. There is **no** builder — construct the
struct directly.

## Architecture

The crate is organised as small modules:

- `agent/mod.rs` — the [`Agent`] async trait + module root.
- `agent/descriptor.rs` — [`AgentDescriptor`] /
  [`AgentFilter`] / [`AgentEntry`] (metadata + entry wrappers).
- `agent/registry.rs` — [`AgentRegistry`] implementing
  [`synthia_core::registry::Registry`].
- `agent/re_act.rs` — [`ReActAgent`] (canonical impl). The
  full ReAct loop is self-contained inside this module so an
  `impl Agent` is fully self-contained.
- `events/`, `input.rs`, `config.rs` — event taxonomy, input
  types, per-session config.
- `prompt/` — XML-delimited system-prompt assembly.
- `lib.rs` — module surface and top-level re-exports.

## Multi-agent registry

```rust,ignore
use std::sync::Arc;
use synthia_agent::{Agent, AgentDescriptor, AgentEntry, AgentRegistry};
use synthia_core::registry::Registry;

let registry = AgentRegistry::new();
registry
    .put(AgentEntry::new(Arc::new(my_react_agent)))
    .await?;

// Resolve by name and run (sync — backed by parking_lot::RwLock).
if let Some(agent) = registry.resolve_sync("agent") {
    let mut stream = agent.run(input, cancel).await;
    // ...
}

// Filter by capability.
let streaming = registry.list(Some(AgentFilter {
        capability: Some("streaming".into()),
        ..Default::default()
    }))
    .await?;
```

## What's deliberately *not* here

These concerns belong in the wider Synthia framework
(`synthia-server`, `synthia-context`, `synthia-hook`, etc.),
not in the agent loop:

- OpenTelemetry context injection.
- Interceptor / permission / approval / trace cross-cuts.
- Steering channels, hook bus, extension registry, custom-hook events.
- Compaction, summarization, token-budget enforcement, self-reflection.
