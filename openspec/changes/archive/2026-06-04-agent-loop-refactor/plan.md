# Agent Loop Refactor Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Refactor synthia-agent main loop to StreamBuilder + LoopContext architecture with post-loop Self-reflection and AgentBus multi-agent communication abstraction.

**Architecture:** Replace ~1100 line `build_stream()` single function with modular steps (sample/tool_execute/compact/reflect), add AgentBus trait for inter-agent communication, move Self-reflection to post-loop execution with HotMemory storage.

**Tech Stack:** Rust, async/await, tokio, Arc<RwLock<>> for shared state

---

## Task 1: Create steps/ directory structure

**Files:**
- Create: `crates/synthia-agent/src/stream_builder/steps/mod.rs`
- Create: `crates/synthia-agent/src/stream_builder/steps/sample.rs`
- Create: `crates/synthia-agent/src/stream_builder/steps/tool_execute.rs`
- Create: `crates/synthia-agent/src/stream_builder/steps/compact.rs`
- Create: `crates/synthia-agent/src/stream_builder/steps/reflect.rs`

- [ ] **Step 1: Create steps/mod.rs with module declarations**

```rust
pub mod sample;
pub mod tool_execute;
pub mod compact;
pub mod reflect;

pub use sample::StepSample;
pub use tool_execute::StepToolExecute;
pub use compact::StepCompact;
pub use reflect::StepReflect;
```

- [ ] **Step 2: Create steps/sample.rs with StepSample struct**

```rust
use std::sync::Arc;
use synthia_provider::traits::ModelProvider;
use synthia_provider::types::{CompletionRequest, Message, ToolChoice};
use crate::loop_context::LoopContext;
use crate::events::{AgentEvent, TokenUsage};
use crate::config::AgentConfig;
use crate::types::SamplingResult;

pub struct StepSample {
    provider: Arc<dyn ModelProvider>,
    config: AgentConfig,
}

impl StepSample {
    pub fn new(provider: Arc<dyn ModelProvider>, config: AgentConfig) -> Self {
        Self { provider, config }
    }

    pub async fn execute(
        &self,
        ctx: &mut LoopContext,
        tools: Vec<synthia_provider::ToolDefinition>,
    ) -> Result<SamplingResult, synthia_core::Error> {
        let request = CompletionRequest {
            model: self.config.model.clone(),
            messages: ctx.messages.clone(),
            tools,
            tool_choice: ToolChoice::Auto,
            temperature: self.config.temperature,
            max_tokens: Some(self.config.max_tokens),
            ..Default::default()
        };

        let mut text = String::new();
        let mut tool_calls = Vec::new();
        let mut reasoning = String::new();

        // Stream and collect response
        let response = self.provider.stream(request).await?;
        // ... parsing logic ...

        Ok(SamplingResult { text, tool_calls, reasoning })
    }
}
```

- [ ] **Step 3: Create steps/tool_execute.rs with StepToolExecute**

```rust
use std::sync::Arc;
use synthia_tool::registry::ToolRegistry;
use synthia_tool::types::ToolExecutionContext;
use crate::loop_context::LoopContext;
use crate::types::ToolResult;

pub struct StepToolExecute {
    tool_registry: Arc<ToolRegistry>,
}

impl StepToolExecute {
    pub fn new(tool_registry: Arc<ToolRegistry>) -> Self {
        Self { tool_registry }
    }

    pub async fn execute(
        &self,
        ctx: &mut LoopContext,
        tool_calls: Vec<synthia_provider::ToolUse>,
    ) -> Result<Vec<ToolResult>, synthia_core::Error> {
        let context = ToolExecutionContext::new(
            ctx.session_id.clone(),
            std::path::PathBuf::from("."),
        );

        let outputs = self.tool_registry.run_with_context(tool_calls, context).await?;
        Ok(outputs.into_iter().map(ToolResult::from).collect())
    }
}
```

- [ ] **Step 4: Create steps/compact.rs with StepCompact**

```rust
use crate::loop_context::LoopContext;
use crate::config::AgentConfig;
use crate::types::{TokenBudget, TokenBudgetStatus, CONTEXT_HARD_MIN, CONTEXT_WARN_BELOW};
use synthia_provider::estimate_messages_token_count;

pub struct StepCompact;

impl StepCompact {
    pub fn check(&self, ctx: &LoopContext, config: &AgentConfig) -> CompactAction {
        let Some(budget) = &config.context_token_budget else {
            return CompactAction::None;
        };

        let token_count = estimate_messages_token_count(&ctx.messages);
        let status = budget.check(token_count);

        match status {
            TokenBudgetStatus::MustCompact => CompactAction::MustCompact,
            TokenBudgetStatus::Warning => CompactAction::Warning,
            _ => CompactAction::None,
        }
    }
}

pub enum CompactAction {
    None,
    Warning,
    MustCompact,
}
```

- [ ] **Step 5: Create steps/reflect.rs with StepReflect**

```rust
use std::sync::Arc;
use synthia_provider::traits::ModelProvider;
use crate::loop_context::LoopContext;
use crate::types::Reflection;

pub struct StepReflect {
    provider: Arc<dyn ModelProvider>,
}

impl StepReflect {
    pub fn new(provider: Arc<dyn ModelProvider>) -> Self {
        Self { provider }
    }

    pub async fn execute(&self, ctx: &LoopContext) -> Result<Reflection, synthia_core::Error> {
        let system_prompt = r#"
你是一个专门进行执行反思的助手。请分析最近的执行过程，提供结构化的反思。
严格以 JSON 格式输出：
{
    "summary": "执行过程的简要总结",
    "issues": ["问题1", "问题2", ...],
    "suggestions": ["建议1", "建议2", ...]
}
"#;

        let user_message = Message::user(format!(
            "请分析以下对话历史，提供反思：\n\n{:?}",
            ctx.messages
        ));

        // Call LLM and parse response...
        // Return Reflection struct
    }
}
```

- [ ] **Step 6: Commit**

```bash
git add crates/synthia-agent/src/stream_builder/steps/
git commit -m "feat(agent): add steps/ directory structure for loop modularization"
```

---

## Task 2: Extend LoopContext

**Files:**
- Modify: `crates/synthia-agent/src/loop_context.rs:1-123`

- [ ] **Step 1: Add new fields to LoopContext struct**

```rust
// Add to existing LoopContext struct:
pub struct LoopContext {
    pub session_id: String,
    pub iteration: usize,
    pub messages: Vec<Message>,
    pub end_reason: Option<SessionEndReason>,
    pub cumulative_tokens: usize,
    pub recent_tool_results: Vec<(String, String, bool)>,  // (name, output, is_error)
    pub needs_compact: bool,
    pub span_ctx: SpanContext,
    // New fields:
    pub steering_messages: Vec<SteeringMessage>,  // Pending steering messages
    pub agent_bus: Option<Arc<dyn AgentBus>>,      // Optional agent bus
}
```

- [ ] **Step 2: Add should_stop method to LoopContext**

```rust
impl LoopContext {
    pub fn should_stop(&self, config: &AgentConfig) -> bool {
        if self.end_reason.is_some() {
            return true;
        }
        if self.iteration >= config.max_iterations {
            return true;
        }
        false
    }

    pub fn should_reflect(&self) -> bool {
        self.end_reason == Some(SessionEndReason::Completed) && self.iteration > 0
    }
}
```

- [ ] **Step 3: Add merge_tool_results method**

```rust
pub fn merge_tool_results(&mut self, results: Vec<ToolResult>) {
    for result in results {
        let is_error = result.is_error.unwrap_or(false);
        self.add_tool_result(result.name.clone(), result.summary.clone(), !is_error);
    }
}
```

- [ ] **Step 4: Run tests to verify LoopContext changes**

Run: `cargo test -p synthia-agent loop_context -- --test-threads=1`

- [ ] **Step 5: Commit**

```bash
git add crates/synthia-agent/src/loop_context.rs
git commit -m "feat(agent): extend LoopContext with should_stop/should_reflect methods"
```

---

## Task 3: Refactor stream_builder/mod.rs main loop

**Files:**
- Modify: `crates/synthia-agent/src/stream_builder/mod.rs`

- [ ] **Step 1: Create new run() method using steps**

```rust
impl StreamBuilder {
    pub async fn run(&self, session_id: String, input: AgentInput) -> AgentOutput {
        let mut ctx = LoopContext::new(session_id, SpanContext::new(&session_id));
        ctx.messages.push(input.to_message());

        while !ctx.should_stop(&self.config) {
            ctx.iteration += 1;
            yield AgentEvent::IterationStarted { iteration: ctx.iteration };

            // 1. Process steering messages
            self.process_steering(&mut ctx);

            // 2. Sample - LLM call
            let sampling_result = self.step_sample.execute(&mut ctx, self.get_tool_definitions()).await?;
            
            // 3. Tool execution if needed
            if !sampling_result.tool_calls.is_empty() {
                let results = self.step_tool_execute.execute(&mut ctx, sampling_result.tool_calls).await?;
                ctx.merge_tool_results(results);
            }

            // 4. Compact check
            match self.step_compact.check(&ctx, &self.config) {
                CompactAction::MustCompact => { /* trigger compaction */ }
                CompactAction::Warning => { yield AgentEvent::TokenBudgetWarning { ... } }
                CompactAction::None => {}
            }

            // 5. Check for completion
            if sampling_result.tool_calls.is_empty() {
                ctx.set_end_reason(SessionEndReason::Completed);
                break;
            }
        }

        // === Main loop ended === 
        
        // Self-reflection (post-loop)
        if ctx.should_reflect() {
            let reflection = self.step_reflect.execute(&ctx).await?;
            self.store_reflection_to_hot_memory(&ctx, &reflection).await;
            yield AgentEvent::SelfReflection { ... };
        }

        yield AgentEvent::SessionEnded { reason: ctx.end_reason.clone().unwrap() };
    }
}
```

- [ ] **Step 2: Add helper methods**

```rust
impl StreamBuilder {
    fn process_steering(&self, ctx: &mut LoopContext) {
        while let Some(msg) = self.steering_channel.try_recv() {
            ctx.messages.push(Message::user(format!("Steering: {}", msg.content)));
        }
    }

    fn get_tool_definitions(&self) -> Vec<synthia_provider::ToolDefinition> {
        self.tool_registry.list(None).await.unwrap_or_default()
            .iter()
            .map(|e| synthia_provider::ToolDefinition {
                name: e.name().to_string(),
                description: e.description().to_string(),
                input_schema: e.tool_instance().parameters(),
            })
            .collect()
    }

    async fn store_reflection_to_hot_memory(&self, ctx: &LoopContext, reflection: &Reflection) {
        if let Some(sender) = &self.memory_event_sender {
            let key = format!("reflection/{}/{}", ctx.session_id, ctx.iteration);
            let value = serde_json::to_vec(reflection).unwrap();
            let _ = sender.send(MemoryEvent::reflection_stored(ctx.session_id.clone(), key, value)).await;
        }
    }
}
```

- [ ] **Step 3: Verify legacy.rs still works as backup**

Run: `cargo build -p synthia-agent 2>&1 | tail -20`

- [ ] **Step 4: Run existing tests**

Run: `cargo test -p synthia-agent -- --test-threads=4 2>&1 | tail -30`

- [ ] **Step 5: Commit**

```bash
git add crates/synthia-agent/src/stream_builder/mod.rs
git commit -m "refactor(agent): refactor main loop using steps pattern"
```

---

## Task 4: Create AgentBus trait and types

**Files:**
- Create: `crates/synthia-agent/src/agent_bus/mod.rs`
- Create: `crates/synthia-agent/src/agent_bus/types.rs`

- [ ] **Step 1: Create agent_bus/types.rs with BusMessage and BusError**

```rust
use std::time::SystemTime;

#[derive(Clone, Debug)]
pub struct BusMessage {
    pub id: String,
    pub from: String,
    pub to: String,
    pub payload: Vec<u8>,
    pub timestamp: i64,
}

impl BusMessage {
    pub fn new(from: &str, to: &str, payload: Vec<u8>) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            from: from.to_string(),
            to: to.to_string(),
            payload,
            timestamp: SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .map(|d| d.as_millis() as i64)
                .unwrap_or(0),
        }
    }
}

#[derive(Debug, Clone)]
pub enum BusError {
    NotRegistered(String),
    NotConnected(String),
    SendFailed(String),
    SubscribeFailed(String),
}

impl std::fmt::Display for BusError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BusError::NotRegistered(id) => write!(f, "Agent {} is not registered", id),
            BusError::NotConnected(addr) => write!(f, "Not connected to {}", addr),
            BusError::SendFailed(msg) => write!(f, "Send failed: {}", msg),
            BusError::SubscribeFailed(msg) => write!(f, "Subscribe failed: {}", msg),
        }
    }
}

impl std::error::Error for BusError {}
```

- [ ] **Step 2: Create agent_bus/mod.rs with AgentBus trait**

```rust
use async_trait::async_trait;
use futures::Stream;
use std::pin::Pin;

use crate::agent_bus::types::{BusMessage, BusError};

#[async_trait]
pub trait AgentBus: Send + Sync {
    async fn register(&self, agent_id: &str) -> Result<(), BusError>;
    async fn send(&self, to: &str, payload: Vec<u8>) -> Result<(), BusError>;
    async fn broadcast(&self, recipients: &[&str], payload: Vec<u8>) -> Result<usize, BusError>;
    fn subscribe(&self) -> Pin<Box<dyn Stream<Item = Result<BusMessage, BusError>> + Send + '_>>;
}
```

- [ ] **Step 3: Commit**

```bash
git add crates/synthia-agent/src/agent_bus/
git commit -m "feat(agent): add AgentBus trait and BusMessage/BusError types"
```

---

## Task 5: Implement MemoryAgentBus

**Files:**
- Create: `crates/synthia-agent/src/agent_bus/memory.rs`

- [ ] **Step 1: Create MemoryAgentBus struct and implementation**

```rust
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};
use dashmap::DashMap;
use futures::Stream;
use async_trait::async_trait;

use super::{AgentBus, BusMessage, BusError};

pub struct MemoryAgentBus {
    agents: Arc<DashMap<String, broadcast::Sender<BusMessage>>>,
    agent_id: RwLock<Option<String>>,
}

impl MemoryAgentBus {
    pub fn new() -> Self {
        Self {
            agents: Arc::new(DashMap::new()),
            agent_id: RwLock::new(None),
        }
    }
}

#[async_trait]
impl AgentBus for MemoryAgentBus {
    async fn register(&self, agent_id: &str) -> Result<(), BusError> {
        let (tx, _) = broadcast::channel(256);
        self.agents.insert(agent_id.to_string(), tx);
        *self.agent_id.write().await = Some(agent_id.to_string());
        Ok(())
    }

    async fn send(&self, to: &str, payload: Vec<u8>) -> Result<(), BusError> {
        let agent_id = self.agent_id.read().await.clone()
            .ok_or_else(|| BusError::NotConnected("not registered".to_string()))?;

        let msg = BusMessage::new(&agent_id, to, payload);

        match self.agents.get(to) {
            Some(tx) => tx.send(msg).map_err(|_| BusError::SendFailed("no receiver".to_string())),
            None => Err(BusError::NotRegistered(to.to_string())),
        }
    }

    async fn broadcast(&self, recipients: &[&str], payload: Vec<u8>) -> Result<usize, BusError> {
        let agent_id = self.agent_id.read().await.clone()
            .ok_or_else(|| BusError::NotConnected("not registered".to_string()))?;

        let mut delivered = 0;
        for recipient in recipients {
            let msg = BusMessage::new(&agent_id, recipient, payload.clone());
            if let Some(tx) = self.agents.get(recipient) {
                let _ = tx.send(msg);
                delivered += 1;
            }
        }
        Ok(delivered)
    }

    fn subscribe(&self) -> Pin<Box<dyn Stream<Item = Result<BusMessage, BusError>> + Send + '_>> {
        // Implementation for subscribing to messages
        Box::pin(futures::stream::pending())
    }
}
```

- [ ] **Step 2: Add unit tests for MemoryAgentBus**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_register_and_send() {
        let bus = MemoryAgentBus::new();
        bus.register("agent1").await.unwrap();
        bus.register("agent2").await.unwrap();

        bus.send("agent2", b"hello".to_vec()).await.unwrap();
    }

    #[tokio::test]
    async fn test_send_to_unregistered() {
        let bus = MemoryAgentBus::new();
        bus.register("agent1").await.unwrap();

        let result = bus.send("ghost", b"hello".to_vec()).await;
        assert!(result.is_err());
    }
}
```

- [ ] **Step 3: Run tests**

Run: `cargo test -p synthia-agent agent_bus -- --test-threads=1`

- [ ] **Step 4: Commit**

```bash
git add crates/synthia-agent/src/agent_bus/memory.rs
git commit -m "feat(agent): implement MemoryAgentBus for in-process communication"
```

---

## Task 6: Implement FileAgentBus

**Files:**
- Create: `crates/synthia-agent/src/agent_bus/file.rs`

- [ ] **Step 1: Create FileAgentBus struct**

```rust
use std::path::PathBuf;
use async_trait::async_trait;
use tokio::fs;
use tokio::sync::mpsc;
use futures::Stream;
use std::pin::Pin;

use super::{AgentBus, BusMessage, BusError};

pub struct FileAgentBus {
    base_path: PathBuf,
    agent_id: RwLock<Option<String>>,
    rx: RwLock<Option<mpsc::Receiver<BusMessage>>>,
}

impl FileAgentBus {
    pub fn new(base_path: PathBuf) -> Self {
        Self {
            base_path,
            agent_id: RwLock::new(None),
            rx: RwLock::new(None),
        }
    }

    fn agent_dir(&self, agent_id: &str) -> PathBuf {
        self.base_path.join(agent_id).join("inbound")
    }
}
```

- [ ] **Step 2: Implement AgentBus for FileAgentBus**

```rust
#[async_trait]
impl AgentBus for FileAgentBus {
    async fn register(&self, agent_id: &str) -> Result<(), BusError> {
        let dir = self.agent_dir(agent_id);
        fs::create_dir_all(&dir).await
            .map_err(|e| BusError::NotConnected(e.to_string()))?;

        *self.agent_id.write().await = Some(agent_id.to_string());
        Ok(())
    }

    async fn send(&self, to: &str, payload: Vec<u8>) -> Result<(), BusError> {
        let agent_id = self.agent_id.read().await.clone()
            .ok_or_else(|| BusError::NotConnected("not registered".to_string()))?;

        let msg = BusMessage::new(&agent_id, to, payload);
        let filename = format!("{}.msg", msg.id);
        let path = self.agent_dir(to).join(filename);

        let data = serde_json::to_vec(&msg)
            .map_err(|e| BusError::SendFailed(e.to_string()))?;

        fs::write(&path, data).await
            .map_err(|e| BusError::SendFailed(e.to_string()))?;

        Ok(())
    }

    async fn broadcast(&self, recipients: &[&str], payload: Vec<u8>) -> Result<usize, BusError> {
        let mut delivered = 0;
        for recipient in recipients {
            if self.send(recipient, payload.clone()).await.is_ok() {
                delivered += 1;
            }
        }
        Ok(delivered)
    }

    fn subscribe(&self) -> Pin<Box<dyn Stream<Item = Result<BusMessage, BusError>> + Send + '_>> {
        Box::pin(futures::stream::pending())
    }
}
```

- [ ] **Step 3: Commit**

```bash
git add crates/synthia-agent/src/agent_bus/file.rs
git commit -m "feat(agent): implement FileAgentBus for filesystem-based IPC"
```

---

## Task 7: Implement MessageProxyAgentBus adapter

**Files:**
- Create: `crates/synthia-agent/src/agent_bus/proxy.rs`

- [ ] **Step 1: Create MessageProxyAgentBus struct**

```rust
use std::sync::Arc;
use async_trait::async_trait;
use tokio::sync::Mutex;
use futures::Stream;
use std::pin::Pin;

use synthia_message_proxy::{MessageBusProxy, MessageBus};
use super::{AgentBus, BusMessage, BusError};

pub struct MessageProxyAgentBus {
    inner: Arc<Mutex<MessageBusProxy>>,
    agent_id: String,
}

impl MessageProxyAgentBus {
    pub async fn connect(agent_id: String, addr: &str) -> Result<Self, BusError> {
        let proxy = MessageBusProxy::connect_to(agent_id.clone(), addr.to_string())
            .await
            .map_err(|e| BusError::NotConnected(e.to_string()))?;

        Ok(Self {
            inner: Arc::new(Mutex::new(proxy)),
            agent_id,
        })
    }
}

#[async_trait]
impl AgentBus for MessageProxyAgentBus {
    async fn register(&self, agent_id: &str) -> Result<(), BusError> {
        let client = self.inner.lock().await;
        client.register(agent_id).await
            .map_err(|e| BusError::NotConnected(e.to_string()))
    }

    async fn send(&self, to: &str, payload: Vec<u8>) -> Result<(), BusError> {
        let client = self.inner.lock().await;
        client.send(&self.agent_id, to, payload).await
            .map_err(|e| BusError::SendFailed(e.to_string()))
    }

    async fn broadcast(&self, recipients: &[&str], payload: Vec<u8>) -> Result<usize, BusError> {
        let client = self.inner.lock().await;
        client.broadcast(&self.agent_id, recipients.to_vec(), payload).await
            .map_err(|e| BusError::SendFailed(e.to_string()))
    }

    fn subscribe(&self) -> Pin<Box<dyn Stream<Item = Result<BusMessage, BusError>> + Send + '_>> {
        Box::pin(futures::stream::pending())
    }
}
```

- [ ] **Step 2: Commit**

```bash
git add crates/synthia-agent/src/agent_bus/proxy.rs
git commit -m "feat(agent): implement MessageProxyAgentBus adapter"
```

---

## Task 8: Integrate AgentBus into StreamBuilder

**Files:**
- Modify: `crates/synthia-agent/src/stream_builder/mod.rs`
- Modify: `crates/synthia-agent/src/agent_bus/mod.rs` (export new implementations)

- [ ] **Step 1: Add agent_bus field to StreamBuilder**

```rust
pub struct StreamBuilder {
    config: AgentConfig,
    provider: Arc<dyn ModelProvider>,
    tool_registry: Arc<ToolRegistry>,
    hook_registry: Arc<HookRegistry>,
    context_assembler: Arc<ContextAssembler>,
    model_router: Arc<ModelRouter>,
    agent_bus: Option<Arc<dyn AgentBus>>,  // NEW
    // ... existing fields ...
}
```

- [ ] **Step 2: Add with_agent_bus() builder method**

```rust
impl StreamBuilder {
    pub fn with_agent_bus(mut self, bus: Arc<dyn AgentBus>) -> Self {
        self.agent_bus = Some(bus);
        self
    }
}
```

- [ ] **Step 3: Add agent communication after main loop**

```rust
// In run() method, after self-reflection:
if let Some(bus) = &self.agent_bus {
    let summary = ctx.generate_summary();
    let _ = bus.broadcast(&["orchestrator"], summary.into_bytes()).await;
}
```

- [ ] **Step 4: Commit**

```bash
git add crates/synthia-agent/src/stream_builder/mod.rs
git commit -m "feat(agent): integrate AgentBus into StreamBuilder"
```

---

## Task 9: Verify and clean up

- [ ] **Step 1: Run full test suite**

Run: `cargo test --all 2>&1 | tail -50`

- [ ] **Step 2: Verify legacy.rs backup works**

Run: `cargo test -p synthia-agent -- legacy --test-threads=1 2>&1 | tail -20`

- [ ] **Step 3: Compare outputs between legacy and new implementation**

```bash
# Run same input through both implementations and compare outputs
```

- [ ] **Step 4: Delete legacy.rs (after verification)**

```bash
rm crates/synthia-agent/src/stream_builder/legacy.rs
git add -A
git commit -m "refactor(agent): remove legacy.rs after verifying new implementation"
```

- [ ] **Step 5: Final clippy and fmt check**

Run: `cargo clippy --all-targets && cargo fmt --check`

- [ ] **Step 6: Update documentation**

---

## Verification Checklist

- [ ] All tests pass: `cargo test --all`
- [ ] No clippy warnings: `cargo clippy --all-targets`
- [ ] Code formatted: `cargo fmt`
- [ ] Legacy.rs removed after verification
- [ ] Documentation updated

---

**Plan complete and saved to `openspec/changes/agent-loop-refactor/plan.md`.**

**Two execution options:**

**1. Subagent-Driven (recommended)** - I dispatch a fresh subagent per task, review between tasks, fast iteration

**2. Inline Execution** - Execute tasks in this session using executing-plans, batch execution with checkpoints

**Which approach?**