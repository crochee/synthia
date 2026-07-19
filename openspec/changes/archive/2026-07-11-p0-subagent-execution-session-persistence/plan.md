# P0: Sub-Agent Execution + Session State Persistence Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement sub-agent ReAct execution loop (foreground/background modes) and durable session state for crash recovery.

**Architecture:** Extend Synthia's existing `AgentTool` and `StreamBuilder` with a sub-agent execution bridge using OpenCode's foreground/background pattern and Codex's config inheritance. Add `SessionMetadata` fields and a `SessionInputQueue` for persistent session state backed by JSONL files.

**Tech Stack:** Rust, tokio (oneshot channels, async tasks), serde (JSON/JSONL), existing synthia crates

---

## Task 1: Extend SessionMetadata with Loop Recovery Fields

**Files:**
- Modify: `crates/synthia-session/src/store/types.rs`
- Modify: `crates/synthia-session/src/manager/persistence.rs`
- Modify: `crates/synthia-agent/src/loop_context.rs`

- [ ] **Step 1: Add new fields to SessionMetadata**

In `crates/synthia-session/src/store/types.rs`, add new fields to `SessionMetadata`:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMetadata {
    pub version: u32,
    pub id: String,
    pub owner_user_id: String,
    pub state: SessionState,
    pub token_usage: TokenUsage,
    pub created_at: String,
    pub updated_at: String,
    pub config: SessionConfig,
    pub message_count: usize,
    // NEW: loop recovery fields
    #[serde(default)]
    pub end_reason: Option<SessionEndReason>,
    #[serde(default)]
    pub iteration: usize,
    #[serde(default)]
    pub cumulative_tokens: usize,
    #[serde(default)]
    pub context_token_limit: Option<usize>,
}
```

- [ ] **Step 2: Verify SessionEndReason derives Serialize/Deserialize**

Check `SessionEndReason` (likely in `synthia-core` or `synthia-agent`) has `#[derive(Serialize, Deserialize)]`. If not, add the derives.

Run: `cargo check -p synthia-session 2>&1 | head -20`

- [ ] **Step 3: Update persistence to write new fields**

In `crates/synthia-session/src/manager/persistence.rs`, find `save_metadata` or equivalent function. Update it to accept and write the new fields. The minimal change is to add parameters or read from a context struct:

```rust
// In the save_metadata function, update the SessionMetadata construction:
pub(crate) async fn save_metadata(
    store: &SessionStore,
    metadata: &SessionMetadata,
) -> Result<(), SessionError> {
    store.save_metadata(metadata).await
}
```

The caller in the agent loop must now pass `iteration`, `end_reason`, `cumulative_tokens`, and `context_token_limit` when constructing the metadata.

- [ ] **Step 4: Update LoopContext construction**

In `crates/synthia-agent/src/loop_context.rs`, add a `from_metadata` constructor or update `new`:

```rust
impl LoopContext {
    pub fn from_metadata(
        session_id: String,
        metadata: &SessionMetadata,
        messages: Vec<Message>,
    ) -> Self {
        Self {
            session_id,
            iteration: metadata.iteration,
            messages,
            end_reason: metadata.end_reason,
            cumulative_tokens: metadata.cumulative_tokens,
            context_token_limit: metadata.context_token_limit,
            // transient fields — not persisted, use defaults
            recent_tool_results: Vec::new(),
            needs_compact: false,
            span_ctx: SpanContext::default(),
            current_turn_id: None,
        }
    }
}
```

- [ ] **Step 5: Verify compilation**

Run: `cargo check -p synthia-session -p synthia-agent`
Expected: Compilation succeeds, no errors.

- [ ] **Step 6: Commit**

```bash
git add crates/synthia-session/src/store/types.rs crates/synthia-session/src/manager/persistence.rs crates/synthia-agent/src/loop_context.rs
git commit -m "feat: extend SessionMetadata with loop recovery fields (iteration, end_reason, cumulative_tokens, context_token_limit)"
```

---

## Task 2: Add SessionInputQueue for Persistent Steering Messages

**Files:**
- Create: `crates/synthia-session/src/store/session_input.rs`
- Modify: `crates/synthia-session/src/store/mod.rs`
- Modify: `crates/synthia-agent/src/stream_builder/builder/iteration/init.rs`

- [ ] **Step 1: Create SessionInputQueue module**

Create `crates/synthia-session/src/store/session_input.rs`:

```rust
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionInput {
    pub id: String,
    pub session_id: String,
    pub content: String,
    pub delivery: InputDelivery,
    pub admitted_at: String,
    pub promoted: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum InputDelivery {
    Steer,
    Queue,
}

#[derive(Debug)]
pub struct SessionInputQueue {
    base_path: PathBuf,
}

impl SessionInputQueue {
    pub fn new(base_path: PathBuf) -> Self {
        Self { base_path }
    }

    fn file_path(&self, session_id: &str) -> PathBuf {
        self.base_path
            .join(session_id)
            .join("session_input.jsonl")
    }

    pub async fn push(
        &self,
        session_id: &str,
        content: String,
        delivery: InputDelivery,
    ) -> Result<(), std::io::Error> {
        let input = SessionInput {
            id: uuid::Uuid::new_v4().to_string(),
            session_id: session_id.to_string(),
            content,
            delivery,
            admitted_at: chrono::Utc::now().to_rfc3339(),
            promoted: false,
        };
        let path = self.file_path(session_id);
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        let line = serde_json::to_string(&input)? + "\n";
        tokio::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .await?
            .write_all(line.as_bytes())
            .await?;
        Ok(())
    }

    pub async fn drain_pending(
        &self,
        session_id: &str,
    ) -> Result<Vec<SessionInput>, std::io::Error> {
        let path = self.file_path(session_id);
        if !path.exists() {
            return Ok(Vec::new());
        }
        let content = tokio::fs::read_to_string(&path).await?;
        let mut pending: Vec<SessionInput> = Vec::new();
        let mut all: Vec<SessionInput> = Vec::new();
        for line in content.lines() {
            if let Ok(input) = serde_json::from_str::<SessionInput>(line) {
                if !input.promoted {
                    pending.push(input.clone());
                }
                all.push(input);
            }
        }
        // Mark all as promoted and rewrite
        for input in &mut all {
            input.promoted = true;
        }
        let mut new_content = String::new();
        for input in &all {
            new_content.push_str(&serde_json::to_string(&input)?);
            new_content.push('\n');
        }
        tokio::fs::write(&path, new_content).await?;
        Ok(pending)
    }

    pub async fn has_pending(&self, session_id: &str) -> Result<bool, std::io::Error> {
        let path = self.file_path(session_id);
        if !path.exists() {
            return Ok(false);
        }
        let content = tokio::fs::read_to_string(&path).await?;
        for line in content.lines() {
            if let Ok(input) = serde_json::from_str::<SessionInput>(line) {
                if !input.promoted {
                    return Ok(true);
                }
            }
        }
        Ok(false)
    }
}
```

- [ ] **Step 2: Register module and add dependencies**

In `crates/synthia-session/src/store/mod.rs`, add:
```rust
pub mod session_input;
```

Ensure `synthia-session/Cargo.toml` has dependencies for `uuid`, `chrono`, `tokio` (with `fs` feature). Check existing deps and add only what's missing:
```bash
grep -E "uuid|chrono" crates/synthia-session/Cargo.toml
```

- [ ] **Step 3: Wire into SessionManager**

Add `SessionInputQueue` to `SessionManager` in `crates/synthia-session/src/manager/mod.rs`:

```rust
pub struct SessionManager {
    store: SessionStore,
    pub input_queue: SessionInputQueue,
    // ... existing fields
}

impl SessionManager {
    pub fn new(store: SessionStore, base_path: PathBuf) -> Self {
        Self {
            store,
            input_queue: SessionInputQueue::new(base_path),
            // ...
        }
    }
}
```

- [ ] **Step 4: Replace mpsc steering with SessionInputQueue**

In `crates/synthia-agent/src/stream_builder/builder/iteration/init.rs`, update `drain_steering`:

```rust
pub(crate) async fn drain_steering(
    ctx: &mut LoopContext,
    session_manager: &SessionManager,
) -> Vec<SteeringMessage> {
    match session_manager.input_queue.drain_pending(&ctx.session_id).await {
        Ok(inputs) => inputs
            .into_iter()
            .map(|i| SteeringMessage {
                content: i.content,
                delivery: match i.delivery {
                    InputDelivery::Steer => DeliveryType::Steer,
                    InputDelivery::Queue => DeliveryType::Queue,
                },
            })
            .collect(),
        Err(e) => {
            tracing::warn!("Failed to drain pending inputs: {}", e);
            Vec::new()
        }
    }
}
```

- [ ] **Step 5: Verify compilation**

Run: `cargo check -p synthia-session -p synthia-agent`
Expected: Compilation succeeds.

- [ ] **Step 6: Commit**

```bash
git add crates/synthia-session/src/store/session_input.rs crates/synthia-session/src/store/mod.rs crates/synthia-session/src/manager/mod.rs crates/synthia-agent/src/stream_builder/builder/iteration/init.rs
git commit -m "feat: add SessionInputQueue for persistent steering message delivery"
```

---

## Task 3: Unify AgentInstance Types

**Files:**
- Create: `crates/synthia-agent/src/agent_instance.rs`
- Modify: `crates/synthia-agent/src/registry/instance.rs`
- Modify: `crates/synthia-agent/src/tools/agent_tools/coordinator.rs`
- Modify: `crates/synthia-agent/src/lib.rs`

- [ ] **Step 1: Create unified AgentInstance**

Create `crates/synthia-agent/src/agent_instance.rs`:

```rust
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tokio::sync::oneshot;

use crate::control::fork_policy::ForkPolicy;
use crate::registry::types::AgentDefinition;
use synthia_session::store::types::TokenBudget;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AgentStatus {
    Idle,
    Running,
    Completed,
    Errored,
    Cancelled,
}

#[derive(Debug)]
pub struct AgentResult {
    pub output: String,
    pub status: AgentStatus,
    pub token_usage: TokenUsage,
}

#[derive(Debug)]
pub struct TokenUsage {
    pub input_tokens: usize,
    pub output_tokens: usize,
}

#[derive(Debug)]
pub struct AgentInstance {
    // From registry::instance::AgentInstance
    pub id: String,
    pub definition: Option<AgentDefinition>,
    pub session: Option<Session>,
    pub token_budget: Option<TokenBudget>,
    pub state: AgentStatus,
    pub parent_id: Option<String>,
    pub created_at: DateTime<Utc>,
    // From tools::agent_tools::coordinator::AgentInstance
    pub role: String,
    pub capabilities: Vec<String>,
    pub system_prompt: String,
    pub tools: Vec<String>,
    pub metadata: HashMap<String, serde_json::Value>,
    // New: execution bridge
    pub fork_policy: ForkPolicy,
    pub depth: usize,
    pub result_tx: Option<oneshot::Sender<AgentResult>>,
}

impl AgentInstance {
    pub fn new(
        id: String,
        role: String,
        capabilities: Vec<String>,
        system_prompt: String,
        tools: Vec<String>,
        metadata: HashMap<String, serde_json::Value>,
    ) -> Self {
        Self {
            id,
            definition: None,
            session: None,
            token_budget: None,
            state: AgentStatus::Idle,
            parent_id: None,
            created_at: Utc::now(),
            role,
            capabilities,
            system_prompt,
            tools,
            metadata,
            fork_policy: ForkPolicy::SystemOnly,
            depth: 0,
            result_tx: None,
        }
    }
}
```

- [ ] **Step 2: Convert registry::instance to re-export shim**

In `crates/synthia-agent/src/registry/instance.rs`, replace the existing `AgentInstance` struct with:

```rust
pub use crate::agent_instance::{AgentInstance, AgentResult, AgentStatus, TokenUsage};
```

Remove the old `AgentInstance` struct definition and `AgentInstanceState` enum.

- [ ] **Step 3: Convert coordinator to re-export shim**

In `crates/synthia-agent/src/tools/agent_tools/coordinator.rs`, replace the existing `AgentInstance` struct with:

```rust
pub use crate::agent_instance::AgentInstance;
```

Update `AgentInstance::new()` calls in coordinator.rs to include the new fields (definition, session, token_budget, state, parent_id, created_at, fork_policy, depth, result_tx all default).

- [ ] **Step 4: Add module to lib.rs**

In `crates/synthia-agent/src/lib.rs`, add:
```rust
pub mod agent_instance;
```

- [ ] **Step 5: Verify compilation**

Run: `cargo check -p synthia-agent`
Expected: Compilation succeeds. Fix any field access mismatches.

- [ ] **Step 6: Commit**

```bash
git add crates/synthia-agent/src/agent_instance.rs crates/synthia-agent/src/registry/instance.rs crates/synthia-agent/src/tools/agent_tools/coordinator.rs crates/synthia-agent/src/lib.rs
git commit -m "feat: unify AgentInstance types into single canonical type"
```

---

## Task 4: Implement Sub-Agent Execution Bridge

**Files:**
- Create: `crates/synthia-agent/src/subagent/runner.rs`
- Create: `crates/synthia-agent/src/subagent/config.rs`
- Create: `crates/synthia-agent/src/subagent/mod.rs`
- Modify: `crates/synthia-agent/src/lib.rs`

- [ ] **Step 1: Create subagent module**

Create `crates/synthia-agent/src/subagent/mod.rs`:

```rust
pub mod config;
pub mod runner;
```

- [ ] **Step 2: Implement sub-agent config builder**

Create `crates/synthia-agent/src/subagent/config.rs`:

```rust
use crate::agent_instance::AgentInstance;
use crate::control::fork_policy::ForkPolicy;
use crate::AgentRunConfig;
use synthia_permission::checker::PermissionChecker;
use synthia_permission::merged_policy::PermissionAction;

pub fn build_subagent_config(
    instance: &AgentInstance,
    parent_config: &AgentRunConfig,
    parent_messages: &[Message],
) -> AgentRunConfig {
    let mut config = parent_config.clone();

    // Apply ForkPolicy to filter messages
    let filtered_messages = apply_fork_policy(&instance.fork_policy, parent_messages);
    config.initial_messages = Some(filtered_messages);

    // Downgrade permission to User layer
    if let Some(checker) = &parent_config.permission_checker {
        config.permission_checker = Some(downgrade_permission_to_user(checker));
    }

    config.max_iterations = parent_config.max_iterations;
    config.context_token_budget = parent_config.context_token_budget.clone();

    config
}

fn apply_fork_policy(policy: &ForkPolicy, messages: &[Message]) -> Vec<Message> {
    match policy {
        ForkPolicy::InheritAll => messages.to_vec(),
        ForkPolicy::LastNTurns(n) => {
            // Keep last N user-assistant turn pairs
            let mut turns = Vec::new();
            let mut current_turn = Vec::new();
            for msg in messages.iter().rev() {
                current_turn.push(msg.clone());
                if msg.role == "user" {
                    turns.push(current_turn);
                    current_turn = Vec::new();
                    if turns.len() >= *n {
                        break;
                    }
                }
            }
            turns.into_iter().rev().flatten().collect()
        }
        ForkPolicy::Empty => Vec::new(),
        ForkPolicy::SystemOnly | _ => {
            messages
                .iter()
                .filter(|m| m.role == "system")
                .cloned()
                .collect()
        }
    }
}

fn downgrade_permission_to_user(
    checker: &PermissionChecker,
) -> PermissionChecker {
    let mut checker = checker.clone();
    checker.set_default_action(PermissionAction::Ask);
    checker
}
```

- [ ] **Step 3: Implement sub-agent runner**

Create `crates/synthia-agent/src/subagent/runner.rs`:

```rust
use std::sync::Arc;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;

use crate::agent_instance::{AgentInstance, AgentResult, AgentStatus, TokenUsage};
use crate::subagent::config::build_subagent_config;
use crate::AgentRunConfig;

pub fn run_subagent(
    mut instance: AgentInstance,
    parent_config: AgentRunConfig,
    parent_messages: Vec<Message>,
) -> JoinHandle<()> {
    let (tx, rx) = oneshot::channel();
    instance.result_tx = Some(tx);
    instance.state = AgentStatus::Running;

    let subagent_config = build_subagent_config(&instance, &parent_config, &parent_messages);

    tokio::spawn(async move {
        let result = execute_subagent(instance.id.clone(), subagent_config).await;

        let agent_result = match result {
            Ok(output) => AgentResult {
                output: output.last_message.unwrap_or_default(),
                status: AgentStatus::Completed,
                token_usage: TokenUsage {
                    input_tokens: output.total_input_tokens,
                    output_tokens: output.total_output_tokens,
                },
            },
            Err(e) => AgentResult {
                output: format!("Sub-agent error: {}", e),
                status: AgentStatus::Errored,
                token_usage: TokenUsage {
                    input_tokens: 0,
                    output_tokens: 0,
                },
            },
        };

        if let Some(tx) = instance.result_tx.take() {
            let _ = tx.send(agent_result);
        }
    })
}

struct SubagentOutput {
    last_message: Option<String>,
    total_input_tokens: usize,
    total_output_tokens: usize,
}

async fn execute_subagent(
    agent_id: String,
    config: AgentRunConfig,
) -> Result<SubagentOutput, Box<dyn std::error::Error + Send + Sync>> {
    // Use the existing StreamBuilder to run the agent
    let mut stream = crate::Agent::run_stream(config).await?;

    let mut last_message = None;
    let mut total_input = 0;
    let mut total_output = 0;

    use futures::StreamExt;
    while let Some(event) = stream.next().await {
        match event {
            crate::AgentEvent::AssistantMessage { content, .. } => {
                last_message = Some(content);
            }
            crate::AgentEvent::TokenUsage { input, output } => {
                total_input += input;
                total_output += output;
            }
            crate::AgentEvent::SessionEnded { .. } => break,
            _ => {}
        }
    }

    Ok(SubagentOutput {
        last_message,
        total_input_tokens: total_input,
        total_output_tokens: total_output,
    })
}
```

- [ ] **Step 4: Add module to lib.rs**

In `crates/synthia-agent/src/lib.rs`, add:
```rust
pub mod subagent;
```

- [ ] **Step 5: Verify compilation**

Run: `cargo check -p synthia-agent`
Expected: Compilation succeeds. Fix any missing imports or type mismatches.

- [ ] **Step 6: Commit**

```bash
git add crates/synthia-agent/src/subagent/
git commit -m "feat: implement sub-agent execution bridge (config inheritance + runner)"
```

---

## Task 5: Rewrite AgentTool for Foreground/Background Execution

**Files:**
- Modify: `crates/synthia-agent/src/tools/agent_tools/agent_tool.rs`

- [ ] **Step 1: Rewrite AgentTool::call()**

In `crates/synthia-agent/src/tools/agent_tools/agent_tool.rs`, replace the existing `call` method:

```rust
use crate::agent_instance::{AgentInstance, AgentStatus};
use crate::subagent::runner::run_subagent;
use crate::control::fork_policy::ForkPolicy;

#[async_trait]
impl Tool for AgentTool {
    fn name(&self) -> &str { "agent" }

    fn description(&self) -> &str {
        "Delegate a task to a sub-agent. Use for complex, multi-step subtasks."
    }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "description": {
                    "type": "string",
                    "description": "Short description of the task (3-5 words)"
                },
                "prompt": {
                    "type": "string",
                    "description": "Detailed instructions for the sub-agent"
                },
                "subagent_type": {
                    "type": "string",
                    "description": "Type of agent to spawn (default: worker)"
                },
                "background": {
                    "type": "boolean",
                    "description": "Run in background (default: false, wait for result)",
                    "default": false
                },
                "fork_policy": {
                    "type": "string",
                    "enum": ["inherit_all", "last_n_turns", "empty", "system_only"],
                    "description": "How much conversation history to share",
                    "default": "system_only"
                }
            },
            "required": ["description", "prompt"]
        })
    }

    async fn call(&self, input: ToolInput) -> ToolOutput {
        let description = input.input.get("description")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let prompt = input.input.get("prompt")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let background = input.input.get("background")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let fork_policy_str = input.input.get("fork_policy")
            .and_then(|v| v.as_str())
            .unwrap_or("system_only");

        if prompt.is_empty() {
            return ToolOutput::error("prompt parameter is required");
        }

        let fork_policy = match fork_policy_str {
            "inherit_all" => ForkPolicy::InheritAll,
            "empty" => ForkPolicy::Empty,
            "system_only" | _ => ForkPolicy::SystemOnly,
        };

        // Check depth limit
        let parent_depth = self.manager.current_depth();
        if parent_depth >= self.manager.max_depth() {
            return ToolOutput::error(format!(
                "Agent depth limit reached (max: {}). Solve the task yourself.",
                self.manager.max_depth()
            ));
        }

        // Check concurrency limit
        if !self.manager.try_acquire_slot() {
            return ToolOutput::error(format!(
                "Maximum concurrent sub-agents reached (max: {}). Wait for existing sub-agents to complete.",
                self.manager.max_concurrent()
            ));
        }

        // Create agent instance
        let mut instance = AgentInstance::new(
            uuid::Uuid::new_v4().to_string(),
            "worker".to_string(),
            vec![],
            format!("Task: {}\n\n{}", description, prompt),
            vec![],
            std::collections::HashMap::new(),
        );
        instance.parent_id = Some(self.manager.current_session_id());
        instance.depth = parent_depth + 1;
        instance.fork_policy = fork_policy;

        let instance_id = instance.id.clone();

        if background {
            // Background mode: spawn and return immediately
            let handle = run_subagent(
                instance,
                self.manager.parent_config(),
                self.manager.current_messages(),
            );

            self.manager.register_background_task(instance_id, handle);

            ToolOutput::text(format!(
                "Sub-agent spawned in background (id: {}). Task: {}",
                instance_id, description
            ))
        } else {
            // Foreground mode: spawn and await result
            let result_rx = {
                let (tx, rx) = tokio::sync::oneshot::channel();
                instance.result_tx = Some(tx);
                rx
            };

            run_subagent(
                instance,
                self.manager.parent_config(),
                self.manager.current_messages(),
            );

            match result_rx.await {
                Ok(result) => {
                    self.manager.release_slot();
                    match result.status {
                        AgentStatus::Completed => {
                            ToolOutput::text(format!(
                                "Sub-agent completed.\n\n{}",
                                result.output
                            ))
                        }
                        AgentStatus::Errored => {
                            ToolOutput::error(format!(
                                "Sub-agent error: {}",
                                result.output
                            ))
                        }
                        _ => ToolOutput::error("Sub-agent ended unexpectedly"),
                    }
                }
                Err(_) => {
                    self.manager.release_slot();
                    ToolOutput::error("Sub-agent task was cancelled")
                }
            }
        }
    }
}
```

- [ ] **Step 2: Update SubagentManager to support new methods**

Add these methods to `SubagentManager` in `crates/synthia-agent/src/tools/agent_tools/team.rs`:

```rust
use std::sync::atomic::{AtomicUsize, Ordering};

// Add fields to SubagentManager:
// max_depth: AtomicUsize (default 1)
// max_concurrent: AtomicUsize (default 6)
// active_count: AtomicUsize (default 0)

pub fn current_depth(&self) -> usize {
    // Return depth from current execution context
    // For root agent, depth is 0
    0 // Placeholder — wire to actual depth tracking
}

pub fn max_depth(&self) -> usize {
    self.max_depth.load(Ordering::Relaxed)
}

pub fn max_concurrent(&self) -> usize {
    self.max_concurrent.load(Ordering::Relaxed)
}

pub fn try_acquire_slot(&self) -> bool {
    let current = self.active_count.load(Ordering::Relaxed);
    if current >= self.max_concurrent() {
        return false;
    }
    self.active_count.fetch_add(1, Ordering::Relaxed);
    true
}

pub fn release_slot(&self) {
    self.active_count.fetch_sub(1, Ordering::Relaxed);
}

pub fn parent_config(&self) -> AgentRunConfig {
    // Return the current parent agent's run config
    self.parent_config.clone()
}

pub fn current_messages(&self) -> Vec<Message> {
    // Return current conversation messages
    self.current_messages.clone()
}

pub fn current_session_id(&self) -> String {
    self.session_id.clone()
}
```

- [ ] **Step 3: Verify compilation**

Run: `cargo check -p synthia-agent`
Expected: Compilation succeeds. Fix any missing imports.

- [ ] **Step 4: Commit**

```bash
git add crates/synthia-agent/src/tools/agent_tools/
git commit -m "feat: rewrite AgentTool with foreground/background sub-agent execution"
```

---

## Task 6: Wire AgentControl and Mailbox into Main Loop

**Files:**
- Modify: `crates/synthia-agent/src/stream_builder/builder/run/main_loop.rs`
- Modify: `crates/synthia-agent/src/control/mailbox.rs`

- [ ] **Step 1: Remove agent_control ignore**

In `crates/synthia-agent/src/stream_builder/builder/run/main_loop.rs`, find the destructure that ignores `agent_control`:

```rust
// Before:
let AgentRunConfig {
    agent_control: _,  // Remove this ignore
    // ...
} = run_config;

// After:
let AgentRunConfig {
    agent_control,  // Keep it
    // ...
} = run_config;
```

- [ ] **Step 2: Add background task completion check**

In the main loop, after `drain_steering()` and before `sample_llm_and_cascade()`, add:

```rust
// Check for completed background sub-agents
if let Some(control) = agent_control.as_ref() {
    let completed = control.check_completed().await;
    for result in completed {
        // Inject result as synthetic user message
        let synthetic_msg = Message::user(format!(
            "<task_result id=\"{}\" state=\"completed\">\n{}\n</task_result>",
            result.agent_id, result.output
        ));
        ctx.messages.push(synthetic_msg);
    }
}
```

- [ ] **Step 3: Implement AgentControl::check_completed()**

In `crates/synthia-agent/src/control/core_ctrl.rs`, add:

```rust
impl AgentControl {
    pub async fn check_completed(&self) -> Vec<CompletedTask> {
        let mut completed = Vec::new();
        let mut registry = self.registry.lock().await;
        registry.retain(|_, meta| {
            if let Some(ref rx) = meta.result_rx {
                match rx.try_recv() {
                    Ok(result) => {
                        completed.push(CompletedTask {
                            agent_id: meta.agent_id.clone(),
                            output: result.output,
                            status: result.status,
                        });
                        false // Remove from registry
                    }
                    Err(tokio::sync::oneshot::error::TryRecvError::Closed) => {
                        false // Channel closed, remove
                    }
                    Err(tokio::sync::oneshot::error::TryRecvError::Empty) => {
                        true // Still running, keep
                    }
                }
            } else {
                true
            }
        });
        completed
    }
}
```

- [ ] **Step 4: Wire Mailbox send**

In `crates/synthia-agent/src/control/mailbox.rs`, replace the stub:

```rust
// Before (stub):
pub async fn send_message(&self, path: &AgentPath, msg: Message) -> Result<(), MailboxError> {
    // In Phase 5 this will be wired to the actual mailbox MPSC sender
    let _ = (path, msg);
    Ok(())
}

// After:
pub async fn send_message(&self, path: &AgentPath, msg: Message) -> Result<(), MailboxError> {
    let sender = self.senders.get(path)
        .ok_or(MailboxError::AgentNotFound)?;
    sender.send(msg).await
        .map_err(|_| MailboxError::ChannelClosed)?;
    Ok(())
}
```

- [ ] **Step 5: Verify compilation**

Run: `cargo check -p synthia-agent`
Expected: Compilation succeeds.

- [ ] **Step 6: Commit**

```bash
git add crates/synthia-agent/src/stream_builder/builder/run/main_loop.rs crates/synthia-agent/src/control/
git commit -m "feat: wire AgentControl and Mailbox into main agent loop"
```

---

## Task 7: Format, Lint, and Test

**Files:** All modified files

- [ ] **Step 1: Format all code**

Run: `cargo +nightly fmt --all`
Expected: No output (all files already formatted or reformatted)

- [ ] **Step 2: Run clippy**

Run: `cargo clippy --all-targets --all-features --tests --all 2>&1`
Expected: No warnings or errors

If warnings appear, fix them before proceeding.

- [ ] **Step 3: Run full test suite**

Run: `cargo test --all 2>&1 | tail -30`
Expected: All existing tests pass. Note any failures.

- [ ] **Step 4: Add backward compatibility test**

In `crates/synthia-session/tests/`, add a test for old-format metadata:

```rust
#[tokio::test]
async fn test_old_metadata_backward_compat() {
    let old_json = r#"{
        "version": 1,
        "id": "test-session",
        "owner_user_id": "user-1",
        "state": "active",
        "token_usage": {"input_tokens": 0, "output_tokens": 0},
        "created_at": "2024-01-01T00:00:00Z",
        "updated_at": "2024-01-01T00:00:00Z",
        "config": {"model": "claude-3", "max_tokens": 4096},
        "message_count": 0
    }"#;

    let metadata: SessionMetadata = serde_json::from_str(old_json).unwrap();
    assert_eq!(metadata.iteration, 0); // default
    assert_eq!(metadata.end_reason, None); // default
    assert_eq!(metadata.cumulative_tokens, 0); // default
    assert_eq!(metadata.context_token_limit, None); // default
}
```

- [ ] **Step 5: Run the new test**

Run: `cargo test -p synthia-session --test backward_compat`
Expected: PASS

- [ ] **Step 6: Final commit**

```bash
git add -A
git commit -m "chore: format, lint, and add backward compatibility tests"
```