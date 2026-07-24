# Package A — Architecture Migration (Dual-Rail) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Legacy `Agent::run()` internally delegates to `StreamBuilder`, enabling CLI/Server to switch between legacy and stream_builder implementations via config flag with zero caller changes.

**Architecture (Path A — Legacy Agent delegates to StreamBuilder):**

```
CLI/Server
    │
    ▼
legacy Agent::run(session_id, input, cancel_token)
    │
    ├─ flag = "legacy" → existing implementation (ReAct loop)
    │
    └─ flag = "stream_builder" → StreamBuilder::run_stream(run_config)
```

CLI/Server always construct legacy `Agent` via `Agent::new(config, AgentDeps)`. The `run()` method checks the flag and delegates to either the existing ReAct loop or `StreamBuilder`.

**Tech Stack:** Rust (cargo build / cargo test)

---

## Current State

### Two Agents, Two Files

```
synthia-agent/src/agent.rs         ← NEW Agent (stream_builder-powered)
synthia-agent/src/agent/core.rs   ← LEGACY Agent (what CLI/Server use today)
```

**Legacy Agent** (`agent/core.rs`):
```rust
pub struct AgentDeps {
    pub tools: Arc<ToolRegistry>,
    pub context: Arc<ContextAssembler>,
    pub session: Arc<dyn SessionManagerTrait>,
    pub router: Arc<dyn ModelRouter>,
    pub hooks: Arc<HookRegistry>,
    pub skills: Arc<dyn SkillGenerator>,
    pub control: Arc<super::control::AgentControl>,
}

impl Agent {
    pub fn new(init: super::AgentInitConfig) -> Self
    pub fn run(&self, session_id, input, cancel_token) -> AgentOutput
}
```

**New Agent** (`agent.rs`):
```rust
impl Agent {
    pub fn builder(config: AgentConfig) -> AgentBuilder
    pub fn run_stream(run_config: AgentRunConfig) -> AgentOutput
}
```

### CLI/Server Construction

Both CLI and Server construct legacy `Agent`:
```rust
let agent = Agent::new(
    Arc::new(agent_config.clone()),
    synthia_agent::agent::AgentDeps { /* deps */ },
);
```

The `agent.run(session_id, input, cancel_token)` call is made later.

---

## File Map

### Key Files (Read Before Starting)

| File | Purpose |
|------|---------|
| `crates/synthia-agent/src/agent/core.rs` | Legacy Agent `run()` method — will be modified |
| `crates/synthia-agent/src/agent.rs` | New `Agent::run_stream()` — static entry |
| `crates/synthia-agent/src/stream_builder/builder.rs` | `StreamBuilder::run()` and `AgentRunConfig` |
| `crates/synthia-agent/src/config/agent_config.rs` | `AgentConfig` definition (add flag here) |
| `crates/synthia-cli/src/config.rs` | CLI config (add flag) |
| `crates/synthia-server/src/config/mod.rs` | Server config (add flag) |

---

## Task 1: Add agent_implementation flag to AgentConfig

**Files:**
- Modify: `crates/synthia-agent/src/config/agent_config.rs`

- [ ] **Step 1: Read agent_config.rs to find AgentConfig struct**

Run: `grep -n "struct AgentConfig\|impl AgentConfig" crates/synthia-agent/src/config/agent_config.rs`
Expected: find the struct definition

- [ ] **Step 2: Add AgentImplementation enum and field**

Add to `AgentConfig`:
```rust
pub agent_implementation: AgentImplementation,
```

Add enum:
```rust
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub enum AgentImplementation {
    #[default]
    Legacy,
    StreamBuilder,
}
```

Add the field to the struct definition. Add `#[serde(default)]` if the struct is deserialized.

- [ ] **Step 3: Build verification**

Run: `cargo build -p synthia-agent 2>&1 | grep -E "^error" | head -10`
Expected: clean build

---

## Task 2: Modify legacy Agent::run() to support delegation

**Files:**
- Modify: `crates/synthia-agent/src/agent/core.rs`

- [ ] **Step 1: Read the current Agent::run() implementation**

Run: `grep -n "pub fn run" crates/synthia-agent/src/agent/core.rs`
Read the full `run()` method (likely around line 150+ based on earlier analysis where `impl Agent` blocks were found).

- [ ] **Step 2: Read StreamBuilder and AgentRunConfig**

Run: `grep -n "pub struct AgentRunConfig" crates/synthia-agent/src/config/agent_config.rs`
Read the `AgentRunConfig` struct fields to understand what needs to be constructed.

Also read `StreamBuilder::from_config()` and `StreamBuilder::run()` in `builder.rs`.

**Key insight:** `AgentRunConfig` has these fields:
- `provider: Arc<dyn ModelProvider>` — NOT in `AgentDeps`
- `tool_registry: ToolRegistry` — from `deps.tools`
- `hook_registry: Arc<HookRegistry>` — from `deps.hooks`
- `model_router: Arc<dyn ModelRouter>` — from `deps.router`
- `session_id: String` — parameter
- `input: AgentInput` — parameter
- `config: AgentConfig` — from `self.config`
- `context_assembler: Option<Arc<ContextAssembler>>` — from `deps.context`
- `session_store: SessionStore` — NOT in `AgentDeps`
- `steering_channel: Option<Arc<dyn SteeringChannel>>` — not in `AgentDeps`
- `cancel_token: CancellationToken` — parameter
- `memory_event_sender: Option<mpsc::Sender<MemoryEvent>>` — not in `AgentDeps`

**Missing fields** that `AgentRunConfig` requires but `AgentDeps` doesn't have:
- `provider: Arc<dyn ModelProvider>` — required
- `session_store: SessionStore` — required
- `cancel_token: CancellationToken` — available as parameter

**Solution:** For `StreamBuilder` path, these missing fields must come from `AgentConfig` or be created/fetched. The `AgentConfig` has `provider_registry` — we need to understand how to get a `provider` from it.

- [ ] **Step 3: Read AgentFullConfig to understand provider access**

Run: `grep -n "AgentFullConfig\|provider" crates/synthia-agent/src/config/agent_config.rs | head -20`
Run: `grep -n "pub struct AgentFullConfig" crates/synthia-agent/src/config/agent_config.rs`

The `AgentFullConfig` is the legacy config type used by `Agent`. It should contain provider info.

- [ ] **Step 4: Implement delegation in Agent::run()**

```rust
pub fn run(
    &self,
    session_id: String,
    input: AgentInput,
    cancel_token: CancellationToken,
) -> AgentOutput {
    match self.config.agent_implementation {
        AgentImplementation::Legacy => {
            // existing ReAct loop implementation
            self.run_legacy(session_id, input, cancel_token)
        }
        AgentImplementation::StreamBuilder => {
            // Build AgentRunConfig from AgentDeps and config
            let run_config = self.build_run_config(session_id, input, cancel_token);
            Agent::run_stream(run_config)
        }
    }
}

fn build_run_config(
    &self,
    session_id: String,
    input: AgentInput,
    cancel_token: CancellationToken,
) -> AgentRunConfig {
    // Extract provider from config.provider_registry
    // Create session_store from self.deps.session
    // Build rest from deps fields
}
```

**NOTE:** This is a simplified sketch. The actual implementation needs careful type handling:
- `provider` must be obtained from `self.config.provider_registry` — read `AgentFullConfig` to understand how
- `session_store` must be created from `self.deps.session` — read `SessionStore` type
- Some fields may be `None` or empty for the initial implementation

**Ask for clarification if types don't align cleanly before implementing.**

- [ ] **Step 5: Build verification**

Run: `cargo build -p synthia-agent 2>&1 | grep -E "^error" | head -10`
Expected: clean build

- [ ] **Step 6: Test**

Run: `cargo test -p synthia-agent --lib 2>&1 | tail -10`
Expected: all lib tests pass

---

## Task 3: Add feature flag to CLI config

**Files:**
- Modify: `crates/synthia-cli/src/config.rs`

- [ ] **Step 1: Read config.rs**

Run: `grep -n "struct AppConfig\|impl AppConfig\|fn from_file" crates/synthia-cli/src/config.rs | head -10`

- [ ] **Step 2: Add AgentImplementation field**

Add to `AppConfig`:
```rust
pub agent_implementation: synthia_agent::config::AgentImplementation,
```

Note: Use the enum from `synthia_agent::config` (defined in Task 1) rather than defining a new one.

- [ ] **Step 3: Ensure default value**

The `#[default]` derive on the enum should handle this if `AppConfig` uses `Default::default()`. If not, set the default in `AppConfig::new()` or `from_file()`.

- [ ] **Step 4: Build verification**

Run: `cargo build -p synthia-cli 2>&1 | grep -E "^error" | head -10`
Expected: clean build

---

## Task 4: Add feature flag to Server config

**Files:**
- Modify: `crates/synthia-server/src/config/mod.rs`

- [ ] **Step 1: Read server config**

Run: `grep -n "struct.*Config\|impl.*Config" crates/synthia-server/src/config/mod.rs | head -10`

- [ ] **Step 2: Add same field**

Add `agent_implementation: synthia_agent::config::AgentImplementation,` to the server's config struct.

- [ ] **Step 3: Build verification**

Run: `cargo build -p synthia-server 2>&1 | grep -E "^error" | head -10`
Expected: clean build

---

## Task 5: Ensure CLI passes config to Agent

**Files:**
- Modify: `crates/synthia-cli/src/agent.rs`

- [ ] **Step 1: Read how agent_config is built**

Run: `grep -n "agent_config\|AgentConfig" crates/synthia-cli/src/agent.rs | head -20`

The `agent_config` in `build()` is built from `config.get_all_models()`. We need to ensure `agent_implementation` is propagated from `AppConfig` to `AgentConfig`.

- [ ] **Step 2: Propagate the flag**

In `AgentSetup::build()`, when constructing `AgentConfig`:
```rust
let agent_config = AgentConfig {
    models: config.get_all_models(),
    agent_implementation: config.agent_implementation.clone(),
    ..Default::default()
};
```

Or if `AgentConfigBuilder` is used:
```rust
AgentConfigBuilder::new()
    .models(config.get_all_models())
    .agent_implementation(config.agent_implementation.clone())
    .build()?
```

- [ ] **Step 3: Build verification**

Run: `cargo build -p synthia-cli 2>&1 | grep -E "^error" | head -10`
Expected: clean build

---

## Task 6: Ensure Server passes config to Agent

**Files:**
- Modify: `crates/synthia-server/src/agent.rs`

- [ ] **Step 1: Read server agent construction**

Run: `grep -n "agent_config\|AgentConfig\|AgentFullConfig" crates/synthia-server/src/agent.rs | head -20`

- [ ] **Step 2: Propagate the flag**

Follow the same pattern as CLI Task 5.

- [ ] **Step 3: Build verification**

Run: `cargo build -p synthia-server 2>&1 | grep -E "^error" | head -10`
Expected: clean build

---

## Task 7: End-to-end verification

- [ ] **Step 1: Full build**

Run: `cargo build 2>&1 | grep -E "^error" | head -20`
Expected: zero errors across all crates

- [ ] **Step 2: Full test**

Run: `cargo test -p synthia-agent --lib 2>&1 | tail -5`
Run: `cargo test -p synthia-cli --lib 2>&1 | tail -5`
Run: `cargo test -p synthia-server --lib 2>&1 | tail -5`
Expected: all pass

- [ ] **Step 3: Commit**

```bash
git add -a
git commit -m "$(cat <<'EOF'
feat(agent): legacy Agent delegates to StreamBuilder via config flag

Agent::run() now checks agent_implementation flag:
- Legacy (default): existing ReAct loop implementation
- StreamBuilder: delegates to StreamBuilder::run_stream()

CLI and Server configs gain agent_implementation field.
Default: Legacy for backwards compatibility.

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

---

## Self-Review Checklist

- [ ] Spec coverage: all steps from spec covered
- [ ] Placeholder scan: no TBD/TODO in plan
- [ ] Type consistency: conversion handles missing fields
- [ ] Build succeeds at each task
- [ ] No breaking changes to existing legacy path
- [ ] Feature flag defaults to "legacy" (backwards compatible)

## Out of Scope (Future Tasks)

- Implementing the three orchestrator executors (A-b)
- Switching default from legacy to stream_builder (A-c)
- Deleting the legacy agent (A-d)

These are separate tasks in the design doc.