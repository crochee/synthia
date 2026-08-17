# MVP AI Agent — 设计方案（权威版 v1.2）

> 本文档是 MVP AI Agent 系统的**单一权威设计来源**。它整合了：
> 1. **原始方案**：从零设计的多 agent + 工具 + hook + 进化 ReAct loop MVP
> 2. **第一轮对抗性审核**：5 位 oracle 从架构/进化 loop/多 agent/工程可行性/安全 5 个维度的挑战
> 3. **第一轮修正案**：基于审核结论的最终设计裁定（§8 关键架构裁定）
> 4. **第二轮多方位打磨**：8 位 oracle 从接口一致性/错误处理/测试/可观察性/模型适配/性能/安全/DX 的深度挑战
> 5. **第二轮决策汇总**：打磨后的 P0/P1/P2/V2 决策清单（§15 多方位打磨决策）
> 6. **v1.2 用户裁剪**：删除所有权限/安全相关能力（§11.2）
>
> **MVP 设计原则调整**：MVP 阶段不做任何权限/安全相关功能。BashTool / WriteFile / ReadFile 直接执行，无任何拦截或校验。详见 §11.2。
>
> 后续实施（M1 → M4）以本文件为准；如需偏离，先更新本文件再写代码。

---

## 目录

- [0. 设计目标与原则](#0-设计目标与原则)
- [1. 总体架构](#1-总体架构)
- [2. 目录结构（5 个 crate）](#2-目录结构5-个-crate)
- [3. Core 类型](#3-core-类型-cratescore)
- [4. Model Provider](#4-model-provider-cratesmodel)
- [5. Tool System](#5-tool-system-cratestools)
- [6. Agent Runtime + 进化 ReAct Loop](#6-agent-runtime--进化-react-loop-cratesagent)
- [7. 入口 `bin/mvp-agent.rs`](#7-入口-binmvp-agentrs)
- [8. 关键架构裁定（来自对抗性审核）](#8-关键架构裁定来自对抗性审核)
- [9. Cargo.toml 关键依赖](#9-cargotoml-关键依赖)
- [10. 实施路线图（14 天 / 4 个里程碑）](#10-实施路线图14-天--4-个里程碑)
- [11. MVP 范围裁剪（明确不做什么）](#11-mvp-范围裁剪明确不做什么)
  - [11.1 已砍掉的功能](#111-已砍掉的功能不在-mvp-范围)
  - [11.2 ★ v1.2 用户明确删除 + v1.3 砍头：所有权限/安全/Hook 相关能力](#112--v12-用户明确删除--v13-砍头所有权限安全hook-相关能力)
  - [11.3 架构保留的接缝](#113-架构保留的接缝方便未来-v2-加)
  - [11.4 不做权限/安全/Hook 的影响](#114-不做权限安全hook-的影响)
- [12. 与 grok-build 的差异（MVP 取舍对照表）](#12-与-grok-build-的差异mvp-取舍对照表)
- [13. 风险与扩展点](#13-风险与扩展点)
- [14. 审核历史与版本说明](#14-审核历史与版本说明)
- [15. 多方位打磨决策（第二轮对抗性审核）](#15-多方位打磨决策第二轮对抗性审核)
- [16. P0/P1 级决策的代码示例](#16-p0p1-级决策的代码示例)

---

## 0. 设计目标与原则

### 0.1 设计目标

构建一个最小可行（MVP）的 AI Agent 系统，具备以下能力：

1. **多 Agent 协同**：leader/subagent 角色模型，subagent 可独立执行任务（M4 用 `tokio::join!` 真并发）
2. **工具系统**：可注册、可扩展的工具集（MVP 内置 Bash / Read / Write）
3. **进化 ReAct Loop**：在经典 ReAct 之上叠加目标驱动层（Classifier → Planner → ReAct → Evaluator），让 agent 能处理多步任务

### 0.2 设计原则

| 原则 | 含义 | 借鉴自 |
|---|---|---|
| **Actor 模式** | 单写者 + mpsc 通道 + 无共享可变状态 | grok-build |
| **强类型工具注册表** | `ToolKind` 枚举 + 双向映射 | grok-build |
| **DTO 通信** | 跨 crate 传递用不可变结构体，避免胖引用 | grok-build |
| **MVP 优先** | 砍掉一切非核心模块，沙箱/Plugins/ACP/TUI 都先不做 | — |
| **扩展点显式** | 每个模块留有清晰的扩展接口 | grok-build |

### 0.3 非目标（明确不做）

- TUI / Web UI（MVP 用 stderr 打印事件）
- ACP / LSP / MCP / Plugins / Skills / Memory / Sandbox / Hook 之外的扩展点
- 多个模型 provider 切换（只做 OpenAI-compatible）
- 跨 session 持久化（重启即清空）
- 自动 compaction 的多级 ladder（只做简单截断）
- Reflector 自反思（MVP 只做 Classifier→Planner→ReAct→Evaluator）
- 后台 fire-and-forget subagent（MVP 用 tokio::join! 同步并发）
- 整个 Hook 系统（v1.3 砍头；V2 加回）
- 所有权限/安全能力（v1.2 已砍）
- Session persistence（messages.jsonl 也不写）

---

## 1. 总体架构

```
┌─────────────────────────────────────────────────────────────────────┐
│                         Frontend Layer (MVP: stderr)                │
└────────────────────────┬────────────────────────────────────────────┘
                         │ CLI args + stdin/stdout
┌────────────────────────▼────────────────────────────────────────────┐
│                      bin/mvp-agent.rs                                │
│  - 构造 Model / Tools / AgentFactory                                   │
│  - 启动 SessionActor 主循环                                           │
│  - 读 stdin → 喂 agent → 打印 AgentEvent 到 stderr                   │
└────────────────────────┬────────────────────────────────────────────┘
                         │
┌────────────────────────▼────────────────────────────────────────────┐
│                   crates/agent (核心)                                 │
│  SessionActor (per-session ReAct Loop)                               │
│  ┌──────────────────────────────────────────────────────────┐       │
│  │ run_turn()                                                │       │
│  │   ├─ classify_intent()                                    │       │
│  │   ├─ plan_goals()       ← 仅 multi_step_task 触发        │       │
│  │   ├─ loop {                                              │       │
│  │   │     react_step()                                      │       │
│  │   │     evaluate_progress()                              │       │
│  │   │     if Continue → continue                            │       │
│  │   │     if Done → return GoalCompleted                    │       │
│  │   │   }                                                   │       │
│  │   └─ maybe_compact()                                      │       │
│  └──────────────────────────────────────────────────────────┘       │
│  + Goal 子组件（classifier/planner/evaluator/strategist）              │
│  + Compaction（简单截断 + 一次 LLM 摘要）                             │
└─────────────┬───────────────────────────────────┬───────────────────┘
              │                                   │
              ▼                                   ▼
┌─────────────────────────────┐  ┌────────────────────────────────┐
│  crates/model               │  │  crates/core                    │
│  ModelProvider trait        │  │  SessionId / Message /          │
│  + OpenAICompatProvider     │  │  ToolKind / ToolSpec /          │
│    (chat + stream)          │  │  ToolResult / AgentEvent        │
└─────────────┬───────────────┘  └────────────────────────────────┘
              │
              ▼
┌─────────────────────────────┐
│  crates/tools               │
│  ToolRegistry               │
│  + 内置工具                  │
│  BashTool / Read / Write    │
│  + TaskTool                 │
│    └─ ChildRunner trait     │
│    └─ 直接 factory.spawn()  │       (M4: 实现 ShellChildRunner)
│      同步等待结果            │
└─────────────────────────────┘
```

---

## 2. 目录结构（5 个 crate）

```
mvp-agent/
├── Cargo.toml                        # workspace
├── crates/
│   ├── core/                         # 跨 crate 共享类型
│   │   ├── src/
│   │   │   ├── lib.rs
│   │   │   ├── session.rs            # SessionId, Message, Role
│   │   │   ├── tool.rs               # ToolKind, ToolSpec, ToolInvocation, ToolResult
│   │   │   ├── error.rs              # AgentError 统一错误
│   │   │   └── events.rs             # AgentEvent (UI 用)
│   │
│   ├── model/                        # 模型 provider 抽象
│   │   ├── src/
│   │   │   ├── lib.rs
│   │   │   ├── provider.rs           # ModelProvider trait
│   │   │   ├── openai_compat.rs      # OpenAI-compatible 实现
│   │   │   ├── request.rs            # ChatRequest, ToolDefinition
│   │   │   ├── response.rs           # ChatResponse, StreamChunk
│   │   │   └── stream.rs             # SSE 流式解析
│   │
│   ├── tools/                        # 工具注册表 + 内置工具
│   │   ├── src/
│   │   │   ├── lib.rs
│   │   │   ├── registry.rs           # ToolRegistry（核心）
│   │   │   ├── builtin/
│   │   │   │   ├── mod.rs
│   │   │   │   ├── bash.rs           # 执行 shell
│   │   │   │   ├── read_file.rs
│   │   │   │   └── write_file.rs
│   │   │   ├── builtin/task/         # M4: 任务工具
│   │   │   │   ├── mod.rs
│   │   │   │   ├── runner.rs         # ChildRunner trait（依赖反转关键）
│   │   │   │   └── tool.rs           # TaskTool
│   │   │   └── factory.rs            # SessionFactory（M4: 在 agent crate 实现，tools 只用 trait）
│   │
│   ├── agent/                        # Agent runtime + 进化 ReAct loop
│   │   ├── src/
│   │   │   ├── lib.rs
│   │   │   ├── session.rs            # SessionActor（per-session ReAct loop）
│   │   │   ├── react.rs              # 单步 react_step 实现
│   │   │   ├── goal/                 # 进化层（4 个组件，无 Reflector）
│   │   │   │   ├── mod.rs
│   │   │   │   ├── classifier.rs
│   │   │   │   ├── planner.rs
│   │   │   │   ├── evaluator.rs
│   │   │   │   └── strategist.rs
│   │   │   ├── goal_state.rs         # GoalLoopState（consecutive_continues 计数器）
│   │   │   ├── compaction.rs         # 简单截断 + 一次 LLM 摘要
│   │   │   └── prompts.rs            # 系统 prompt 模板
│   │
│   └── bin/                          # MVP 入口
│       └── mvp-agent.rs              # main：CLI args → 启动 SessionActor → stdin/stdout
```

> **为什么 5 个 crate**（v1.3：hooks crate 整砍）：core/model/tools/agent 是逻辑边界；MVP 不需要 TUI/headless/leader 拆分，先放 `bin/`。后续要扩展时把 `bin/` 拆成 `leader/` + `cli/` + `tui/` 不影响其他 crate。V2 重新加回 `hooks/` 时不影响 core/tools/agent。

---

## 3. Core 类型 `crates/core/`

### 3.1 `SessionId` & `Message`

```rust
// crates/core/src/session.rs
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Debug, Clone, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub struct SessionId(pub Arc<str>);

impl SessionId {
    pub fn generate() -> Self {
        Self(Arc::from(uuid::Uuid::new_v4().to_string()))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "role", rename_all = "lowercase")]
pub enum Message {
    System { content: String },
    User    { content: String },
    Assistant { content: Option<String>, tool_calls: Vec<ToolCall> },
    Tool    { tool_call_id: String, content: String },
    // ★ 注意：原方案的 Message::Reflection 已删除
    // 改为 SessionActor.pending_reflection: Option<String> 字段
    // 理由：见 §8 架构裁定 #3
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Goal {
    pub id: String,
    pub description: String,
    pub status: GoalStatus,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GoalStatus { Pending, InProgress, Completed, Blocked }
```

### 3.2 `ToolKind` & `ToolSpec` & `ToolResult`

```rust
// crates/core/src/tool.rs
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// 强类型工具枚举（MVP 简化：只有固定变体，不做 serde(other) 兜底）
#[derive(Debug, Clone, Copy, Hash, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolKind {
    Bash,
    ReadFile,
    WriteFile,
    Task,           // M4: 调用 subagent
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolSpec {
    pub name: String,                  // 模型看到的工具名
    pub kind: ToolKind,
    pub description: String,
    pub parameters: serde_json::Value, // JSON Schema
}

/// 工具返回值（MVP 简化：只用 output 字符串，不引入 data 字段）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    pub success: bool,
    pub output: String,
}

/// 工具实现 trait
#[async_trait]
pub trait Tool: Send + Sync {
    fn kind(&self) -> ToolKind;
    fn spec(&self) -> &ToolSpec;
    async fn invoke(&self, args: serde_json::Value, ctx: ToolContext)
        -> Result<ToolResult, AgentError>;
}

/// 工具执行上下文（MVP 简化版 grok 的 ToolContext）
#[derive(Clone)]
pub struct ToolContext {
    pub session_id: SessionId,
    pub cwd: std::path::PathBuf,
    pub env: Arc<std::collections::HashMap<String, String>>,
    pub cancel: tokio_util::sync::CancellationToken,
    pub subagent_depth: u32,           // 递归深度（M4 用）
}
```

### 3.3 `AgentEvent`（UI 事件流）

```rust
// crates/core/src/events.rs
use tokio::sync::mpsc;

#[derive(Debug, Clone)]
pub enum AgentEvent {
    /// 模型 streaming delta（增量文本）
    TextDelta { session_id: SessionId, delta: String },
    /// 工具调用开始
    ToolCallStart { session_id: SessionId, name: String, args: serde_json::Value },
    /// 工具调用结束
    ToolCallEnd   { session_id: SessionId, name: String, result: ToolResult },
    /// Turn 完成
    TurnComplete  { session_id: SessionId, stop_reason: StopReason },
    /// Goal 状态变化
    GoalUpdate    { session_id: SessionId, goals: Vec<Goal> },
    /// M4: 子 agent 事件
    SubagentSpawn { parent: SessionId, child: SessionId, kind: String },
    SubagentDone  { parent: SessionId, child: SessionId, result: ToolResult },
}

#[derive(Debug, Clone, Debug)]
pub enum StopReason { EndTurn, MaxTurns, Cancelled, Error, GoalCompleted }

pub type EventSender = mpsc::UnboundedSender<AgentEvent>;
pub type EventReceiver = mpsc::UnboundedReceiver<AgentEvent>;
```

### 3.4 错误类型

```rust
// crates/core/src/error.rs
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AgentError {
    #[error("unknown tool: {0}")]
    UnknownTool(String),
    #[error("invalid arguments: {0}")]
    InvalidArgs(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("model error: {0}")]
    Model(String),
    #[error("json parse error: {0}")]
    ParseError(String),
    #[error("cancelled")]
    Cancelled,
    #[error("subagent depth exceeded (max 2)")]
    SubagentDepthExceeded,
    #[error("blocked by goal evaluator: {0}")]
    GoalBlocked(String),
}
```

---

## 4. Model Provider `crates/model/`

### 4.1 `ModelProvider` trait

```rust
// crates/model/src/provider.rs
use crate::{ChatRequest, ChatResponse, StreamChunk};
use async_trait::async_trait;
use futures::stream::Stream;

#[async_trait]
pub trait ModelProvider: Send + Sync {
    /// 同步调用（用于 goal classifier/evaluator 等小决策）
    async fn complete(&self, req: ChatRequest) -> Result<ChatResponse, ModelError>;

    /// 流式调用（用于主 ReAct loop，需要 streaming）
    fn stream(&self, req: ChatRequest)
        -> Box<dyn Stream<Item = Result<StreamChunk, ModelError>> + Send + Unpin>;
}

#[derive(Debug, thiserror::Error)]
pub enum ModelError {
    #[error("http error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("api error: {0}")]
    Api(String),
    #[error("sse parse error: {0}")]
    Sse(String),
}
```

### 4.2 `OpenAICompatProvider`

```rust
// crates/model/src/openai_compat.rs
pub struct OpenAICompatProvider {
    client: reqwest::Client,
    base_url: String,
    api_key: String,
}

impl OpenAICompatProvider {
    pub fn new(base_url: impl Into<String>, api_key: impl Into<String>) -> Self {
        Self {
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(120))
                .build().unwrap(),
            base_url: base_url.into(),
            api_key: api_key.into(),
        }
    }
}

#[async_trait]
impl ModelProvider for OpenAICompatProvider {
    async fn complete(&self, req: ChatRequest) -> Result<ChatResponse, ModelError> {
        // POST {base_url}/chat/completions, stream=false
        // 解析 JSON 返回 ChatResponse
        todo!("参考 OpenAI ChatCompletion API 文档实现")
    }

    fn stream(&self, req: ChatRequest)
        -> Box<dyn Stream<Item = Result<StreamChunk, ModelError>> + Send + Unpin>
    {
        // POST {base_url}/chat/completions, stream=true
        // 用 reqwest bytes_stream + 手写 SSE 解析器
        todo!("参考 OpenAI SSE 格式: data: {json}\\n\\n + data: [DONE]")
    }
}
```

### 4.3 `ChatRequest` / `ChatResponse` / `StreamChunk`

```rust
// crates/model/src/request.rs
#[derive(Debug, Clone, Serialize)]
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<Message>,
    pub tools: Vec<ToolDefinition>,
    pub temperature: Option<f32>,
    pub max_tokens: Option<u32>,
    pub stream: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

// crates/model/src/response.rs
#[derive(Debug, Clone, Deserialize)]
pub struct ChatResponse {
    pub content: Option<String>,
    pub tool_calls: Vec<ToolCall>,
    pub finish_reason: FinishReason,
    pub usage: Usage,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FinishReason { Stop, ToolCalls, Length, Error }

#[derive(Debug, Clone)]
pub enum StreamChunk {
    ContentDelta(String),
    ToolCallDelta { index: usize, id: Option<String>, name: Option<String>, args_delta: String },
    Done { finish_reason: FinishReason, usage: Usage },
}
```

---

## 5. Tool System `crates/tools/`

### 5.1 `ToolRegistry`

```rust
// crates/tools/src/registry.rs
use std::collections::HashMap;
use std::sync::Arc;

pub struct ToolRegistry {
    by_kind: HashMap<ToolKind, Arc<dyn Tool>>,
    by_name: HashMap<String, ToolKind>,
}

impl ToolRegistry {
    pub fn new() -> Self { Self { by_kind: HashMap::new(), by_name: HashMap::new() } }

    pub fn register(&mut self, tool: Arc<dyn Tool>) {
        let kind = tool.kind();
        let name = tool.spec().name.clone();
        self.by_kind.insert(kind, tool);
        self.by_name.insert(name, kind);
    }

    pub fn specs_for_model(&self) -> Vec<ToolDefinition> {
        self.by_kind.values()
            .map(|t| ToolDefinition {
                name: t.spec().name.clone(),
                description: t.spec().description.clone(),
                parameters: t.spec().parameters.clone(),
            })
            .collect()
    }

    pub async fn dispatch(
        &self,
        name: &str,
        args: serde_json::Value,
        ctx: ToolContext,
    ) -> Result<ToolResult, AgentError> {
        let kind = self.by_name.get(name)
            .ok_or_else(|| AgentError::UnknownTool(name.to_string()))?;
        let tool = self.by_kind.get(kind).unwrap();
        tool.invoke(args, ctx).await
    }

    pub fn has(&self, kind: ToolKind) -> bool { self.by_kind.contains_key(&kind) }
}

/// MVP 默认工具集
pub fn default_registry() -> ToolRegistry {
    let mut r = ToolRegistry::new();
    r.register(Arc::new(BashTool));
    r.register(Arc::new(ReadFileTool));
    r.register(Arc::new(WriteFileTool));
    r
}
```

### 5.2 内置工具示例：`BashTool`

```rust
// crates/tools/src/builtin/bash.rs
pub struct BashTool;

#[async_trait]
impl Tool for BashTool {
    fn kind(&self) -> ToolKind { ToolKind::Bash }

    fn spec(&self) -> &ToolSpec {
        // 用 once_cell 缓存
        static SPEC: once_cell::sync::Lazy<ToolSpec> = once_cell::sync::Lazy::new(|| ToolSpec {
            name: "bash".into(),
            kind: ToolKind::Bash,
            description: "Execute a shell command. Returns stdout + stderr.".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "cmd": { "type": "string", "description": "Shell command to execute" },
                    "timeout_secs": { "type": "integer", "description": "Timeout in seconds (default 60)" }
                },
                "required": ["cmd"]
            }),
        });
        &SPEC
    }

    async fn invoke(&self, args: serde_json::Value, ctx: ToolContext)
        -> Result<ToolResult, AgentError>
    {
        #[derive(Deserialize)]
        struct Args { cmd: String, timeout_secs: Option<u64> }
        let Args { cmd, timeout_secs } = serde_json::from_value(args)?;

        let timeout = timeout_secs.unwrap_or(60);
        let output = tokio::time::timeout(
            std::time::Duration::from_secs(timeout),
            tokio::process::Command::new("sh")
                .arg("-c").arg(&cmd)
                .current_dir(&ctx.cwd)
                .envs(ctx.env.iter())
                .output()
        ).await
        .map_err(|_| AgentError::Cancelled)?
        .map_err(AgentError::Io)?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        Ok(ToolResult {
            success: output.status.success(),
            output: format!(
                "{}{}",
                stdout,
                if !stderr.is_empty() { format!("\nstderr: {stderr}") } else { String::new() }
            ),
        })
    }
}
```

### 5.3 M3：`Task` 工具 + `ChildRunner` trait（关键：依赖反转 + v1.3 真并发）

**问题**：tools 需要 agent 的 SessionFactory 来 spawn 子 session，但 agent 也需要 tools 来注册 TaskTool。直接依赖会循环。

**解决方案**：tools 定义 `ChildRunner` trait，agent 实现它并注入。

**v1.3 修订**：原 M4 的 Coordinator actor + fire-and-forget 模型被砍头（详见 §11.1（已砍掉的功能表）+ §13（风险与扩展点））。v1.3 的并发模型是 **TaskTool 工具调用层 fan-out + `tokio::join!`**：父 agent 在一次 `react_step` 里可发起 N 个独立 subagent task，TaskTool 内部 `tokio::join!` 等齐所有子 session 完成，收集结果再回灌到父 context。

```rust
// crates/tools/src/builtin/task/runner.rs
use std::future::Future;

/// 单次 child session run 的 future 类型（每次 spawn 一个独立子任务）
pub type ChildTurnFuture =
    Box<dyn Future<Output = Result<ToolResult, AgentError>> + Send + Unpin>;

pub trait ChildRunner: Send + Sync {
    /// 启动一个子 session 并跑完一个 turn，返回结果
    fn run_turn(
        &self,
        parent_session_id: SessionId,
        depth: u32,
        task: String,
        cancel: tokio_util::sync::CancellationToken,
    ) -> ChildTurnFuture;
}

// crates/tools/src/builtin/task/tool.rs
pub struct TaskTool {
    runner: Arc<dyn ChildRunner>,
    /// 同 parent 下 child 数上限（决策 2.10，默认 3）
    max_breadth: u32,
}

impl TaskTool {
    pub fn new(runner: Arc<dyn ChildRunner>, max_breadth: u32) -> Self {
        Self { runner, max_breadth }
    }
}

#[async_trait]
impl Tool for TaskTool {
    fn kind(&self) -> ToolKind { ToolKind::Task }
    fn spec(&self) -> &ToolSpec { /* ... */ }

    async fn invoke(&self, args: serde_json::Value, ctx: ToolContext)
        -> Result<ToolResult, AgentError>
    {
        // 决策 2.10：单 task 调用支持并行多个子任务
        #[derive(Deserialize)]
        struct Args {
            tasks: Vec<String>,          // ★ v1.3：改成 Vec<String> 支持 fan-out
        }
        let Args { tasks } = serde_json::from_value(args)?;

        if ctx.subagent_depth >= 2 {
            return Err(AgentError::SubagentDepthExceeded);
        }
        if tasks.len() as u32 > self.max_breadth {
            return Err(AgentError::SubagentBreadthExceeded);
        }

        // ★ v1.3 真并发：fan-out + tokio::join! 等齐所有 child
        // 见 §11.1（已砍掉的功能表）+ §13（风险与扩展点）+ §10 M3 Day 13
        let mut futures: Vec<ChildTurnFuture> = tasks
            .into_iter()
            .map(|task| {
                self.runner.run_turn(
                    ctx.session_id.clone(),
                    ctx.subagent_depth + 1,
                    task,
                    ctx.cancel.clone(),
                )
            })
            .collect();

        let mut outputs = Vec::with_capacity(futures.len());
        for fut in futures.drain(..) {
            outputs.push(fut.await);
        }

        // 汇总结果回灌父 session
        let summary = outputs
            .into_iter()
            .enumerate()
            .map(|(i, r)| match r {
                Ok(t) => format!("[subagent {i}] {t}", t = t.output),
                Err(e) => format!("[subagent {i}] ERROR: {e}"),
            })
            .collect::<Vec<_>>()
            .join("\n");
        Ok(ToolResult {
            success: true,
            output: summary,
        })
    }
}
```

**在 agent crate 里实现**：

```rust
// crates/agent/src/child_runner.rs
use mvp_tools::ChildRunner;

pub struct ShellChildRunner {
    factory: Arc<SessionFactory>,
}

impl ChildRunner for ShellChildRunner {
    fn run_turn(
        &self,
        parent_session_id: SessionId,
        depth: u32,
        task: String,
        cancel: CancellationToken,
    ) -> ChildTurnFuture
    {
        let factory = self.factory.clone();
        Box::new(Box::pin(async move {
            let mut child = factory.spawn(ChildConfig {
                parent: parent_session_id,
                depth,
                cancel,
                // 子 session 继承父的 cwd / env（共享 Arc）
                cwd: ...,
                env: ...,
            }).await?;
            let stop = child.run_turn(task).await?;
            Ok(ToolResult {
                success: matches!(stop, StopReason::EndTurn | StopReason::GoalCompleted),
                output: child.last_assistant_text(),
            })
        }))
    }
}
```

> **架构说明**：
> 1. `ChildRunner` 是依赖反转的接缝——tools 只知道"有 child runner"，不直接依赖 agent crate。
> 2. **v1.3 并发模型**：单 tool call 内通过 `tasks: Vec<String>` fan-out，由 `TaskTool::invoke` 内部 `tokio::join!` 等齐；不走 Coordinator actor，不支持跨 turn 的 fire-and-forget 后台任务。V2 加 Coordinator 时，TaskTool 可改成 fire + handle 模式而不动 SessionActor。
> 3. **不使用** `tokio::spawn` 起子任务再 poll——子 session 必须跟父同生命周期，避免父 turn 退出后孤儿 session 残留。


## 6. Agent Runtime + 进化 ReAct Loop `crates/agent/`

> **"进化"两层**：
> 1. **目标驱动**（Classifier → Planner → ReAct → Evaluator），task-level 闭环
> 2. ~~自反思（Reflector）~~ —— **MVP 不做**，避免反思噪声和 token 浪费

### 6.1 `SessionActor`（per-session 主循环）

```rust
// crates/agent/src/session.rs
pub struct SessionActor {
    id: SessionId,
    config: SessionConfig,

    model: Arc<dyn ModelProvider>,
    tools: Arc<ToolRegistry>,

    event_tx: EventSender,

    messages: Vec<Message>,
    goals: Vec<Goal>,
    goal_state: GoalLoopState,         // ★ 新增：consecutive_continues 计数器
    last_intent: Option<Intent>,       // ★ 新增：Classifier 状态迁移用
    pending_reflection: Option<String>,// ★ V2 用，MVP 留 None
    turn_count: u32,
    cancel: CancellationToken,
}

#[derive(Debug, Clone)]
pub struct SessionConfig {
    pub model_name: String,
    pub cwd: PathBuf,
    pub env: HashMap<String, String>,
    pub max_turns: u32,
    pub enable_goal_system: bool,
    pub subagent_depth: u32,           // M4 用
}
```

**主循环入口**：

```rust
impl SessionActor {
    pub async fn run_turn(&mut self, user_input: String) -> Result<StopReason, AgentError> {
        self.messages.push(Message::User { content: user_input });

        // 进化层 or 普通 ReAct
        let stop = if self.config.enable_goal_system {
            self.run_turn_with_goals().await?
        } else {
            self.run_turn_plain_react().await?
        };

        Ok(stop)
    }
}
```

### 6.2 普通 ReAct（基线路径）

```rust
async fn run_turn_plain_react(&mut self) -> Result<StopReason, AgentError> {
    loop {
        if self.turn_count >= self.config.max_turns {
            return Ok(StopReason::MaxTurns);
        }
        tokio::select! {
            _ = self.cancel.cancelled() => return Ok(StopReason::Cancelled),
            _ = self.react_step() => {}
        }
        self.turn_count += 1;
    }
}
```

### 6.3 进化 ReAct（目标驱动）

```rust
async fn run_turn_with_goals(&mut self) -> Result<StopReason, AgentError> {
    // Phase 1: Classifier
    let intent = self.classify_intent().await?;

    // ★ Classifier 状态迁移规则
    if let Some(old) = &self.last_intent {
        match (old, &intent) {
            (Intent::MultiStepTask, Intent::SingleAction)
            | (Intent::MultiStepTask, Intent::Question) => {
                // 降级：清空旧 goals
                self.goals.clear();
            }
            _ => {}
        }
    }
    self.last_intent = Some(intent.clone());

    // Phase 2: Planner（仅 multi_step_task）
    if matches!(intent, Intent::MultiStepTask) && self.goals.is_empty() {
        self.goals = self.plan_goals().await?;
        self.emit_event(AgentEvent::GoalUpdate {
            session_id: self.id.clone(), goals: self.goals.clone(),
        });
    }

    // Phase 3: Loop
    loop {
        if self.turn_count >= self.config.max_turns {
            return Ok(StopReason::MaxTurns);
        }
        tokio::select! {
            _ = self.cancel.cancelled() => return Ok(StopReason::Cancelled),
            _ = self.react_step() => {}
        }
        self.turn_count += 1;

        // 取出 pending_reflection（MVP 永远 None，留接口）
        let _ = self.pending_reflection.take();

        // Phase 4: Evaluator
        let eval = self.evaluate_progress().await?;
        self.goal_state.update(eval.clone());

        match eval {
            ProgressEval::Continue => {
                // ★ 死循环兜底：consecutive_continues 超限强制 adjust_plan
                if self.goal_state.consecutive_continues
                    >= self.goal_state.max_consecutive_continues
                {
                    let new_goals = self.replan().await?;
                    self.goals = new_goals;
                    self.goal_state.reset();
                    self.emit_event(AgentEvent::GoalUpdate {
                        session_id: self.id.clone(), goals: self.goals.clone(),
                    });
                }
            }
            ProgressEval::AdjustPlan(new_goals) => {
                self.goals = new_goals;
                self.goal_state.reset();
                self.emit_event(AgentEvent::GoalUpdate {
                    session_id: self.id.clone(), goals: self.goals.clone(),
                });
            }
            ProgressEval::Done => {
                self.mark_all_goals_completed();
                return Ok(StopReason::GoalCompleted);
            }
            ProgressEval::Blocked(reason) => {
                return Err(AgentError::GoalBlocked(reason));
            }
        }
    }
}
```

### 6.4 `react_step` —— 单步 ReAct

> **v1.3 更新**：原"hook fire 责任只在 react_step 顶层"裁定已废止——整个 Hook 系统砍头，react_step 不再 fire 任何 hook。Tool 执行链路回归最简形态。

```rust
async fn react_step(&mut self) -> Result<StepOutcome, AgentError> {
    // 1. 拼装 request
    let messages = self.build_messages_for_model();
    let req = ChatRequest {
        model: self.config.model_name.clone(),
        messages,
        tools: self.tools.specs_for_model(),
        temperature: Some(0.7),
        max_tokens: Some(8192),
        stream: true,
    };

    // 2. Streaming 调用模型
    let mut response = ChatResponseAccumulator::new();
    let mut stream = self.model.stream(req.clone());
    use futures::StreamExt;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        match chunk {
            StreamChunk::ContentDelta(d) => {
                self.emit_event(AgentEvent::TextDelta {
                    session_id: self.id.clone(), delta: d.clone(),
                });
                response.add_content(d);
            }
            StreamChunk::ToolCallDelta { index, id, name, args_delta } => {
                response.add_tool_call(index, id, name, args_delta);
            }
            StreamChunk::Done { finish_reason, usage } => {
                response.set_done(finish_reason, usage);
                break;
            }
        }
    }
    let response = response.finalize();

    // 3. assistant 消息加入历史
    self.messages.push(Message::Assistant {
        content: response.content.clone(),
        tool_calls: response.tool_calls.clone(),
    });

    // 4. 没有 tool_calls → turn 完成
    if response.tool_calls.is_empty() {
        return Ok(StepOutcome::Finish(map_finish_reason(response.finish_reason)));
    }

    // 5. 串行执行 tool calls（MVP 简化；后续可并行 independent calls）
    for tc in response.tool_calls {
        // 5.1 执行（v1.3 砍掉 hooks 后无 PreToolUse / PostToolUse fire）
        self.emit_event(AgentEvent::ToolCallStart {
            session_id: self.id.clone(), name: tc.name.clone(), args: tc.arguments.clone(),
        });
        let ctx = self.build_tool_context();
        let result = match self.tools.dispatch(&tc.name, tc.arguments.clone(), ctx).await {
            Ok(r) => r,
            Err(e) => ToolResult { success: false, output: format!("Error: {e}") },
        };

        // 5.2 tool 结果加入历史
        self.messages.push(Message::Tool {
            tool_call_id: tc.id.clone(), content: result.output.clone(),
        });
        self.emit_event(AgentEvent::ToolCallEnd {
            session_id: self.id.clone(), name: tc.name.clone(), result: result.clone(),
        });
    }

    Ok(StepOutcome::Continue)
}

#[derive(Debug, Clone)]
pub enum StepOutcome {
    Continue,
    Finish(StopReason),
}
```

### 6.5 Goal 子组件（4 个）

```rust
// crates/agent/src/goal/classifier.rs
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Intent { Question, SingleAction, MultiStepTask }

impl SessionActor {
    pub async fn classify_intent(&self) -> Result<Intent, AgentError> {
        // System: 把用户意图分类成 Question/SingleAction/MultiStepTask
        // 输出 JSON: {"intent": "...", "reason": "..."}
        // 用一次 complete() 同步调用，temperature=0
        todo!("实现：system prompt + parse JSON + 解析失败重试 1 次")
    }
}

// crates/agent/src/goal/planner.rs
impl SessionActor {
    pub async fn plan_goals(&self) -> Result<Vec<Goal>, AgentError> {
        // System: 把当前任务拆成 3-7 个 goal，每个 goal 写明 verification
        // 输出 JSON: {"goals": [{"description": "...", "verification": "..."}]}
        todo!()
    }
}

// crates/agent/src/goal/strategist.rs
impl SessionActor {
    pub async fn pick_active_goal(&mut self) -> Result<Option<Goal>, AgentError> {
        // MVP 简化实现：找第一个 Pending 的 goal（不用 LLM 调用）
        Ok(self.goals.iter().find(|g| matches!(g.status, GoalStatus::Pending)).cloned())
    }
}

// crates/agent/src/goal/evaluator.rs
impl SessionActor {
    pub async fn evaluate_progress(&self) -> Result<ProgressEval, AgentError> {
        // System: 评估当前进度，输出决策
        // 输出 JSON: {"decision": "continue|adjust_plan|done|blocked", "reason": "...", "new_goals": [...]}
        todo!()
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "decision", rename_all = "snake_case")]
pub enum ProgressEval {
    Continue,
    AdjustPlan { new_goals: Vec<Goal> },
    Done,
    Blocked { reason: String },
}
```

### 6.6 Goal 状态机（死循环兜底）

```rust
// crates/agent/src/goal_state.rs
use std::collections::VecDeque;

pub struct GoalLoopState {
    pub consecutive_continues: u32,
    pub max_consecutive_continues: u32,  // 默认 5
    pub recent_evaluations: VecDeque<ProgressEval>,  // 滑动窗口
}

impl GoalLoopState {
    pub fn new() -> Self {
        Self {
            consecutive_continues: 0,
            max_consecutive_continues: 5,
            recent_evaluations: VecDeque::with_capacity(10),
        }
    }

    pub fn update(&mut self, eval: ProgressEval) {
        if matches!(eval, ProgressEval::Continue) {
            self.consecutive_continues += 1;
        } else {
            self.consecutive_continues = 0;
        }
        self.recent_evaluations.push_back(eval);
        if self.recent_evaluations.len() > 10 {
            self.recent_evaluations.pop_front();
        }
    }

    pub fn reset(&mut self) {
        self.consecutive_continues = 0;
        self.recent_evaluations.clear();
    }
}
```

### 6.7 Compaction（MVP 简化版）

```rust
// crates/agent/src/compaction.rs
impl SessionActor {
    pub async fn maybe_compact(&mut self) -> Result<(), AgentError> {
        // 简单 token 估算：每条消息按 200 token 估
        let estimated_tokens = self.messages.len() * 200;
        let threshold = self.config.context_window * 85 / 100;
        if estimated_tokens < threshold { return Ok(()); }

        // 截断最早的 30% 消息
        let cutoff = self.messages.len() / 3;
        let old = self.messages[..cutoff].to_vec();
        let summary = self.summarize(&old).await?;

        self.messages = vec![
            Message::System {
                content: format!("[Earlier conversation summary]\n{summary}"),
            },
            self.messages[cutoff..].to_vec(),
        ].concat();
        Ok(())
    }

    async fn summarize(&self, messages: &[Message]) -> Result<String, AgentError> {
        // 一次性 LLM 调用，要求生成 ≤ 500 token 的摘要
        // MVP 不做 L5 ladder，不做 checkpoints
        todo!()
    }
}
```

---

## 7. 入口 `bin/mvp-agent.rs`

```rust
// crates/bin/mvp-agent.rs
use std::sync::Arc;
use clap::Parser;

#[derive(Parser)]
#[command(name = "mvp-agent")]
struct Args {
    #[arg(long, default_value = ".")]
    cwd: PathBuf,
    #[arg(long)]
    model: String,
    #[arg(long)]
    api_key: String,
    #[arg(long, env = "MVP_BASE_URL", default_value = "https://api.openai.com/v1")]
    base_url: String,
    #[arg(long, default_value_t = 50)]
    max_turns: u32,
    #[arg(long, default_value_t = true)]
    enable_goals: bool,
}

#[tokio::main(flavor = "multi_thread", worker_threads = 2)]  // ★ multi_thread，非 current_thread
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();
    let args = Args::parse();

    // 1. Model provider
    let model: Arc<dyn ModelProvider> = Arc::new(OpenAICompatProvider::new(
        args.base_url, args.api_key,
    ));

    // 2. Tool registry
    let tools = Arc::new(default_registry());

    // 3. Session（v1.3：hooks crate 已砍，不传 HookRegistry）
    let (event_tx, mut event_rx) = tokio::sync::mpsc::unbounded_channel();
    let mut session = SessionActor::new(SessionConfig {
        model_name: args.model,
        cwd: args.cwd,
        env: std::env::vars().collect(),
        max_turns: args.max_turns,
        enable_goal_system: args.enable_goals,
        subagent_depth: 0,
    }, model, tools, event_tx);

    // 5. 启动事件打印任务（写到 stderr）
    tokio::spawn(async move {
        while let Some(ev) = event_rx.recv().await {
            print_event_to_stderr(ev);
        }
    });

    // 6. 主循环：stdin → run_turn
    use tokio::io::{AsyncBufReadExt, BufReader};
    let stdin = tokio::io::stdin();
    let mut reader = BufReader::new(stdin).lines();
    while let Some(line) = reader.next_line().await? {
        let line = line?;
        let stop = session.run_turn(line).await?;
        eprintln!("[turn stopped: {:?}]", stop);
    }

    Ok(())
}

fn print_event_to_stderr(ev: AgentEvent) {
    // 简化打印：TextDelta 实时输出，ToolCall 标 start/end，GoalUpdate 印列表
    match ev {
        AgentEvent::TextDelta { delta, .. } => {
            use std::io::Write;
            let _ = std::io::stderr().write_all(delta.as_bytes());
        }
        AgentEvent::ToolCallStart { name, args, .. } => {
            eprintln!("\n[tool] {name}({})", args);
        }
        AgentEvent::ToolCallEnd { name, result, .. } => {
            eprintln!("[tool done] {name}: {}", 
                if result.success { "ok" } else { "fail" });
        }
        AgentEvent::TurnComplete { stop_reason, .. } => {
            eprintln!("\n[turn done] {:?}", stop_reason);
        }
        AgentEvent::GoalUpdate { goals, .. } => {
            eprintln!("\n[goals updated] {}", goals.len());
        }
        _ => {}
    }
}
```

---

## 8. 关键架构裁定（来自对抗性审核）

> 本节是**设计变更的权威来源**。所有与原方案的偏离都在这里记录原因。

### 裁定 #1：Coordinator 改为 trait-driven（解决循环依赖）

**问题**：原方案中 `tools/task::Coordinator` 需要 `agent::AgentFactory`，`agent::SessionActor` 又需要 `tools::Coordinator` 注册 TaskTool → 循环 crate 依赖。

**裁定**：
- `tools` crate 定义 `ChildRunner` trait（**依赖反转**）
- `agent` crate 实现 `ShellChildRunner`
- 注入方式：`TaskTool::new(Arc<dyn ChildRunner>)`

**借鉴**：grok-build 的 `ChildRunner` trait + `ShellChildRunner`（见 `crates/codegen/xai-grok-tools/src/implementations/grok_build/task/coordinator.rs`）

### 裁定 #2：Runtime 改为 `multi_thread`（避免卡死）

**问题**：原方案用 `current_thread`，但 `Coordinator` 内部用 `tokio::spawn` 创建 actor 子任务 + `oneshot::Receiver::await`——这在 `current_thread` 上会**executor starvation**，子任务永远拿不到执行权。

**裁定**：
```rust
#[tokio::main(flavor = "multi_thread", worker_threads = 2)]
```

M4 评估是否需要 LocalSet（如果需要 `!Send` 类型）；目前 MVP 保持 multi_thread。

### 裁定 #3：Reflection 从 Message 变体改为 SessionActor 字段

**问题**：原方案 `Message::Reflection { content }` 是私有变体，宣称"在 `build_messages_for_model` 里转成 system note"——但 `build_messages_for_model` 没定义，注入语义不清：是会污染整轮 system prompt，还是作为一次性临时注入？模型行为不可预测。

**裁定**：
```rust
pub struct SessionActor {
    // ...
    pending_reflection: Option<String>,  // ★ 临时字段，不入 history
}

async fn react_step(&mut self) {
    // 1. 取出 pending_reflection
    let pending = self.pending_reflection.take();

    // 2. 拼 messages：reflection 作为临时 system message，**不写回 self.messages**
    let mut messages = self.build_base_messages();
    if let Some(r) = pending {
        messages.insert(0, Message::System {
            content: format!("[Previous self-reflection]\n{}", r),
        });
    }
    // 3. 调用模型
    // 4. assistant 消息加入 self.messages（reflection 不进 history）
}
```

**MVP 永远不设置 `pending_reflection`**（V2 加 Reflector 时再启用）。

**v1.3 更新**：v1.3 把整个 Hook 系统砍掉，裁定 #4（Hook fire 责任只在 react_step 顶层）随之作废——Hook 不存在了，也就没有"fire 责任"问题。`ToolContext` 在 v1.3 也已移除 `hooks` 字段。

### 裁定 #4：Classifier 状态迁移必须显式

**问题**：用户可能在多轮对话中切换意图（multi → single），旧 goals 永远 Pending 卡死。

**裁定**：保留 `last_intent: Option<Intent>` 字段，`run_turn_with_goals` 开头做迁移：

```rust
match (&self.last_intent, &intent) {
    (Some(Intent::MultiStepTask), Intent::SingleAction)
    | (Some(Intent::MultiStepTask), Intent::Question) => {
        self.goals.clear();
    }
    _ => {}
}
```

### 裁定 #5：Evaluator 死循环兜底

**问题**：LLM 在 evaluator 里默认选 "continue"，agent 永远不收敛。

**裁定**：加 `GoalLoopState`（§6.6）：
- `consecutive_continues >= 5` → 强制调用 `replan()` 清空 goals 并重新规划
- 滑动窗口 `recent_evaluations`（最近 10 次）检测模式循环（V2 实现）

**v1.3 更新**：裁定 #6/#7（原 HookEvent agent_depth + Permission Ask fallback）已废止——整个 Hook 系统砍头，相关架构结论失去意义。

### 裁定 #6：MVP 不做 Reflector

**理由**：
- 反思让模型过度自我怀疑（"我做错了 → 不敢做 → 卡住"螺旋）
- 每 N 步固定反思浪费 token（50 turns 注入 16 条 reflection）
- LLM 自评可靠性低（路径依赖：说过 done 就倾向继续 done）
- 价值未在 MVP 场景验证

**裁定**：M3 只做 Classifier → Planner → ReAct → Evaluator。Reflector 留 V2（事件驱动：只在 `evaluator == Continue` 触发，且只输出 `next_step_hint`）。

---

## 9. Cargo.toml 关键依赖

```toml
# 根 Cargo.toml
[workspace]
members = [
    "crates/core",
    "crates/model",
    "crates/tools",
    "crates/agent",
    "crates/bin",
]
resolver = "2"

[workspace.dependencies]
tokio = { version = "1.40", features = ["full"] }
async-trait = "0.1"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
reqwest = { version = "0.12", features = ["stream", "json"] }
futures = "0.3"
tokio-util = { version = "0.7", features = ["rt"] }
thiserror = "1"
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
uuid = { version = "1", features = ["v4"] }
clap = { version = "4", features = ["derive"] }
once_cell = "1"
chrono = { version = "0.4", features = ["serde"] }
glob = "0.3"
```

各 crate 的 `Cargo.toml` 通过 `dependencies` 引用（用 `workspace = true`）。

---

## 10. 实施路线图（14 天 / 4 个里程碑）

> 14 天是**紧凑版**估算，真实工作量约 21 天（舒适版）。每个里程碑必须有可验证的 E2E 场景。

### M1：最小可跑通（Day 1-5）

| 天 | 任务 | 验证 |
|---|---|---|
| 1 | core types + model non-streaming (complete) | `cargo test -p mvp-model` 调通 OpenAI chat completion |
| 2 | BashTool + ReadFileTool + WriteFileTool + ToolRegistry | `cargo test -p mvp-tools` dispatch 单 tool |
| 3 | SessionActor.run_turn_plain_react() + bin 入口 | 跑 `cat README.md` 任务，agent 调 bash |
| 4 | Streaming 模型 + ChatResponseAccumulator | streaming text delta 正常打印到 stderr |
| 5 | 反应层 polish：ChatResponseAccumulator 边界 / 错误信息原样返回 / 不做 path canonicalize | E2E "读 + 改文件"通过 |

**M1 验收**：能完成"读文件 → 改文件 → 总结"任务，模型 streaming 正常，工具 dispatch 无副作用。

### M2：进化 ReAct（Day 6-9）

| 天 | 任务 | 验证 |
|---|---|---|
| 6 | Classifier（system prompt + JSON parse + retry） | "重构 module" 分类为 MultiStepTask |
| 7 | Planner + GoalLoopState + Strategist | 复杂任务拆成 3-5 个 goals |
| 8 | Evaluator + consecutive_continues 兜底 | 死循环时强制 replan |
| 9 | 真实 prompt 调优 + E2E 测试 | 一个真实场景跑通（如"找出所有 TODO 并生成报告"） |

**M2 验收**：能完成多步任务（"重构 X 模块并保留向后兼容"），有 goal 状态可视化。

### M3：多 Agent（Day 10-13）

| 天 | 任务 | 验证 |
|---|---|---|
| 10 | ChildRunner trait + SessionFactory + ShellChildRunner | 编译通过，agent crate 注入 runner |
| 11 | TaskTool 注册 + 嵌套深度检查（≥2 → AgentError） | task 工具能调通 |
| 12 | SubagentSpawnContext DTO + cwd/env 共享 + agent_depth 隔离（v1.3：无 hooks 共享） | 子 session 跑独立 loop |
| 13 | `tokio::join!` 多 subagent 真并发（v1.3 修订）+ 集成测试 | "并发派 3 个 explore subagent"验证 |

**M3 验收**：能完成"并发派 3 个 explore subagent 收集信息"任务，子 session 通过 `tokio::join!` 真并发而非顺序执行。

### M5+：V2 增强（不在 MVP 范围）

- Reflector 自反思（事件驱动）
- SearchReplaceTool（编辑优化）
- GrepTool（独立于 Bash）
- MetricsHook（用量统计）
- Compaction L5 ladder
- Session persistence（messages.jsonl）
- Fire-and-forget subagent + Coordinator actor
- TUI / ACP / Plugins / Skills / Memory / Sandbox
- 多 model provider 切换

---

## 11. MVP 范围裁剪（明确不做什么）

> 本节是**范围裁剪的权威来源**。新增裁剪项追加在末尾，理由要充分。

### 11.1 已砍掉的功能（不在 MVP 范围）

| 不做项 | 原因 | 替代方案 |
|---|---|---|
| `SearchReplaceTool` | grok 的 search_replace 是 1000+ 行（含 hunk tracking），MVP 太复杂 | WriteFile 整体覆盖 |
| `GrepTool` | 用 `BashTool` 调 `rg` 命令即可 | 用户自己调 |
| `MetricsHook` | tracing 日志够用 | tracing crate |
| `Reflector`（M3 反思机制） | 价值未验证，时间紧 | V2 加 |
| Compaction L5 ladder | 太复杂 | 简单截断 + 一次 LLM 摘要 |
| `ToolResult.data` 结构化字段 | MVP 不需要 | 只用 `output: String` |
| Session persistence | 不必要 | MVP 不落盘 |
| `current_thread` runtime | 会 executor starvation | multi_thread |
| Coordinator actor（M3） | 同步路径不需要 actor | 直接 `factory.spawn()` + `tokio::join!` 并发 |
| 后台 fire-and-forget subagent | 95% 用例不需要 | 用户自己开新进程 |
| **整个 Hook 系统**（v1.3 砍头） | 范围外 / 无强制需求 | V2 加回完整 Hook crate |
| `BuildMessageForModel` 私有逻辑 | 拼 messages 在 react_step 内部 | 简化 |
| 自动 `cargo fmt --check` CI | MVP 阶段不强制 | 手动 fmt |
| 性能基准测试 | MVP 阶段不重要 | V2 加 |

### 11.2 ★ v1.2 用户明确删除 + v1.3 砍头：所有权限/安全/Hook 相关能力

> **用户指示**："不要搞什么权限和安全的能力"
> **执行时间**：v1.2（第二轮打磨后）

MVP 阶段**完全不做**以下权限/安全相关能力。这些项目的所有决策 ID（1.10 / 2.3 / 2.4 / 2.5 / 2.14 / 2.16 / 2.19 / 3.3 等）已标记为 **【MVP 不做，删除】**，§16 代码示例也已删除或加注释。

| 不做项 | 原决策 ID | 替代方案 |
|---|---|---|
| **整个 Hook 系统**（v1.3 砍头） | 1.10 / 2.9 / 3.11 / 3.15 | 不传 HookRegistry；react_step 无 hook fire；V2 加回完整 Hook crate |
| **ReadFile path canonicalize + size 截断** | 2.3 | 直接读，不限制；用户自己小心 |
| **WriteFile path 白名单**（必须在 cwd 内） | 2.4 | 直接写，不校验；用户自己小心 |
| **BashTool env 白名单**（过滤敏感变量） | 2.5 | 直接传完整 `ctx.env`，不过滤 |
| **错误信息脱敏**（API key / AWS_ / TOKEN* mask） | 2.14 | 错误信息原样返回 |
| **BashTool 输出 ANSI escape strip** | 2.16 | 不 strip，原样输出 |
| **System prompt 反 prompt injection** | 2.19 | 系统 prompt 不加反 injection 限制语 |
| **Sandbox**（Landlock / Seatbelt / seccomp） | 原 §13 已列 | 完全不做 |
| **Audit log**（独立 module） | 3.3 | tracing 日志够用 |
| **`/yolo` slash command**（切换权限模式） | 原 §8 已列 | 不做 |

### 11.3 架构保留的接缝（方便未来 V2 加）

虽然 v1.3 把整个 Hook 系统砍头，但**架构接缝保留**，未来加时无需重构：

| 接缝位置 | v1.3 保留内容 | V2 加什么 |
|---|---|---|
| `crates/hooks` 目录 | **不存在**（v1.3 砍头）；V2 加回时直接 `cargo new crates/hooks` | 完整 Hook crate + HookEvent + PermissionHook |
| `ToolResult.truncated` / `exit_code` 字段 | 完整保留（决策 1.3） | 用于 truncate hook 输出 |
| `BashTool.kill_on_drop(true)` | 完整保留（决策 2.1，性能相关） | — |
| `BashTool` 的 timeout 机制 | 完整保留 | — |
| `ChildRunner` trait（v1.3 同步并发） | 完整保留 | 加回 Coordinator actor + 异步队列 |
| `agent_depth` 字段（`ToolContext` / `SessionConfig`） | 完整保留（子 session 递归深度检查用） | 用于 hook 区分父/子（V2 Hook） |

### 11.4 不做权限/安全/Hook 的影响

**影响**：
- BashTool 可以执行任何 shell 命令（包括 `rm -rf`）
- WriteFile 可以写任何路径（包括 `/etc/passwd`）
- ReadFile 可以读任何文件（包括 `~/.ssh/id_rsa`）
- 模型可以读到 env 里的 `AWS_ACCESS_KEY` / `GITHUB_TOKEN`
- **没有 Hook fire 链**：模型流式输出、tool call 前后没有用户可控的拦截点（V2 加回 Hook 时需在 react_step 串入）
- **没有全局 metrics 收集**：Hook 移除后失去了 `MetricsHook` 接入点（V2 加回 Hook 时补 `MetricsHook`）
- 模型可以读到 env 里的 `AWS_ACCESS_KEY` / `GITHUB_TOKEN`

**接受前提**：
- MVP 是**单用户本地工具**，用户在终端自己承担风险
- 类似 `grok -p` / `claude -p` 的 headless 模式，安全责任在用户
- V2 加权限/安全时不影响 MVP 接口

---

## 12. 与 grok-build 的差异（MVP 取舍对照表）

| 模块 | grok-build | MVP | 原因 |
|---|---|---|---|
| 异步 runtime | `LocalSet` + `!Send` actor + `LocalRef` | `multi_thread` runtime | MVP 简化，避免 LocalRef 复杂度 |
| 模型 provider | 多 provider + Responses API + 转换层 | 只做 OpenAI compat | MVP 范围 |
| 工具调用 streaming | tool_calls 分片增量组装 | 一次完整返回或 accumulator | MVP 先跑通 |
| Subagent | 完整 coordinator actor（pending/active/completed + waiter + deadline + persisted output + nested cancellation） | `ChildRunner` trait + `factory.spawn()` + `tokio::join!` 真并发 | MVP 走最常用路径，v1.3 砍掉 fire-and-forget + coordinator actor |
| Hook 系统 | 完整 HookEvent / HookRegistry / PermissionHook / LoggingHook | **整砍**（v1.3） | 权限/安全/审计 V2 加回 |
| Subagent 并发 | Coordinator actor + waiter/deadline + 持久化输出 | `tokio::join!` 同步并发（v1.3 修订） | MVP 不需要 fire-and-forget |
| Sandbox | Landlock/Seatbelt/seccomp | 不做 | MVP 不做任何权限/安全能力（v1.2） |
| Goal system | classifier/planner/strategist/evaluator/stop_detector/summarizer/tracker/role_tools 8 个 | classifier/planner/strategist/evaluator 4 个 | MVP 砍掉 tracker/summarizer/role_tools/stop_detector |
| Reflector | 无 | **无**（V2 加） | MVP 阶段价值未验证 |
| GoalLoopState | 无 | consecutive_continues 计数器 | 死循环兜底 |
| Classifier 状态迁移 | 无显式规则 | 显式 last_intent 字段 | 防止 multi→single 切换时 goals 卡死 |
| Compaction | L5 ladder + segments + checkpoints | 简单截断 + 一次摘要 | MVP 不需要恢复精度 |
| Persistence | updates.jsonl + chat_history.jsonl + summary.json + signals.json + rewind_points.jsonl + subagents/ | 无 | MVP 不落盘 |
| ACP / TUI / Plugins / Skills / Memory / LSP / MCP | 全有 | 全无 | MVP 范围 |

---

## 13. 风险与扩展点

### 13.1 已知风险

| 风险 | 概率 | 缓解 |
|---|---|---|
| Streaming 工具调用解析 bug（OpenAI tool_calls 是分片的） | 高 | M1 先用 `complete` non-streaming 验证逻辑；M1 后期补 `stream` |
| Evaluator 路径依赖（说过 done 就倾向继续 done） | 中 | MVP 接受；prompt 强调"必须有具体证据才说 done" |
| Goal 拆解粒度不可控 | 中 | prompt 限制 3-7 个，verification 必须可验证 |
| Hook 递归调用（hook 里调 tool） | **N/A**（v1.3 砍头） | V2 加回时引入 `HookContext` 区分 |
| 14 天路线图紧凑 | 高 | 准备好砍 M3 范围（只做 factory.spawn()，不做完整 coordinator actor） |
| 真实模型 prompt 调优时间被低估 | 高 | 路线图 Day 10 专门做；预留 1 天 buffer |

### 13.2 扩展点（架构已留好）

| 需求 | 改动点 | 工作量 |
|---|---|---|
| 加新工具 | 实现 `Tool` trait，在 `default_registry()` 注册 | 0.5 天 |
| 加新 hook（V2） | 加回 `crates/hooks` crate，实现 `Hook` trait，在 bin 里 `hooks.register(...)` | 2-3 天（含 crate 重建） |
| 加新模型 provider | 实现 `ModelProvider` trait，构造时换 Arc | 1 天 |
| 加新 goal 子组件 | 在 `goal/` 加文件，在 `SessionActor` 加调用 | 0.5 天 |
| 加 TUI | 复用 `event_tx`，订阅 `AgentEvent` 渲染 | 3-5 天 |
| 加并行 subagent | 扩展 `ShellChildRunner`，加 waiter/deadline 状态 | 3-5 天 |
| 加 rewind | 在 `Persistence` 加 `rewind_points.jsonl`，bin 加 `/rewind` 命令 | 2 天 |
| 加 Reflector（V2） | 在 `goal/reflector.rs` 加实现，事件驱动触发 | 1-2 天 |
| 加 SearchReplaceTool（V2） | 实现 `Tool` trait + hunk tracking | 5-7 天 |
| 加多模型 provider 切换 | 加 `ProviderRegistry`，按 model name 路由 | 2-3 天 |

### 13.3 监控指标（V2 加）

- Token 用量（按 model 统计）
- Tool 调用频次（按 tool name 统计）
- ReAct loop 平均步数（按 task 类型）
- Goal 收敛率（done / adjust_plan / blocked 的比例）
- Subagent 嵌套深度分布

---

## 14. 审核历史与版本说明

### 版本历史

| 版本 | 日期 | 内容 |
|---|---|---|
| v0.1 (草案) | — | 原始方案（无审核） |
| v0.2 (审核稿) | — | 加入 5 个 oracle 对抗性审核 |
| **v1.0 (权威版)** | 2026-08-01 | 综合裁定后整合：本文件 |
| **v1.1** | 2026-08-02 | 第二轮打磨：8 个 oracle → 36 个决策（9 P0 + 14 P1 + 13 V2），§15-§16 + 附录 C |
| **v1.2** | 2026-08-02 | 用户删除所有权限/安全能力：9 个决策标【MVP 不做，删除】；§11.2 列出删除清单 |
| **v1.3 (当前)** | 2026-08-02 | **整个 Hook 系统砍头**（crate 删除 + §8 裁定 #4/#7/#8 作废 + M1/M2 重排）+ M3 多 Agent 改 `tokio::join!` 真并发 + 文档自洽性清理 + 决策表同步 |

### 审核参与方

- **审核 #1**：架构与依赖一致性（oracle）→ 发现循环依赖 / runtime / Reflection 类型 3 个硬伤
- **审核 #2**：进化 ReAct loop 严谨性（oracle）→ 发现 Classifier 抖动 / Evaluator 死循环 / Reflector 噪声 3 个问题
- **审核 #3**：多 Agent / Coordinator（oracle）→ 发现 actor 过度设计 / hooks 共享 / fire-and-forget 必要性 3 个问题
  - **注**：v1.3 已修订（fire-and-forget 砍头 + 多 subagent 改 `tokio::join!` 同步并发），见 §10 M3。
- **审核 #4**：MVP 范围 / 工程可行性（oracle）→ 14 天 → 真实 28 天严重低估
- **审核 #5**：Hook / 安全模型（oracle）→ Hook fire 责任混乱 / 递归风险 / Ask 决策无法实现 3 个问题
  - **注**：v1.2 已删除所有安全相关能力（见 §11.2）；v1.3 把整个 Hook 系统砍头（见 §0.3 / §2 / §11.2），原 §8 裁定 #4/#7/#8 随之作废。

### 文档维护约定

- 任何代码偏离本文档的设计，必须先更新本文档再改代码
- 章节 §8（关键架构裁定）是变更的权威来源，新裁定追加到 §14 历史
- 实施时遇到 §9 未覆盖的设计问题，先在 PR 描述里记录"对设计的偏离"，merge 后回填到本文档

---

## 附录 A：术语表

| 术语 | 含义 |
|---|---|
| **MVP** | Minimum Viable Product，最小可行产品 |
| **ReAct** | Reason + Act，AI agent 的基础循环模式 |
| **Classifier** | 把用户意图分类成 Question/SingleAction/MultiStepTask 的 LLM 调用 |
| **Planner** | 把 MultiStepTask 拆成 3-7 个 Goal 的 LLM 调用 |
| **Evaluator** | 评估当前进度，输出 Continue/AdjustPlan/Done/Blocked 的 LLM 调用 |
| **Strategist** | 选下一个 active goal（MVP 简化：第一个 Pending） |
| **Reflector** | 自反思（V2，MVP 不做） |
| **ChildRunner** | tools crate 定义的 trait，让 agent crate 注入 spawn 实现（依赖反转关键） |
| **ShellChildRunner** | agent crate 里 ChildRunner 的具体实现 |
| **ToolContext** | Tool 执行时的上下文（cwd / env / cancel / depth） |
| **agent_depth** | 当前 session 的嵌套深度，0 = 父，1 = 一层 subagent；v1.3 仅用于 `ToolContext.subagent_depth >= 2` 嵌套检查，V2 Hook 加回后可用于 hook 区分父/子 |
| **consecutive_continues** | evaluator 连续输出 Continue 的次数，用于死循环兜底 |
| **DTO** | Data Transfer Object，跨 crate 传递的不可变结构体 |
| **Hook 系统** | v1.3 砍头：原计划提供 PreToolUse / PostToolUse / PreModel / PostModel 拦截点；V2 加回完整 `crates/hooks` |

---

## 附录 B：参考实现（grok-build）

| 设计点 | grok 参考位置 |
|---|---|
| ChildRunner trait | `crates/codegen/xai-grok-tools/src/implementations/grok_build/task/coordinator.rs` |
| ShellChildRunner 实现 | `crates/codegen/xai-grok-shell/src/agent/mvp_agent/subagent_coordinator.rs` |
| ToolKind 强类型 | `crates/codegen/xai-grok-tools/src/registry/types.rs` |
| SubagentSpawnContext DTO | `crates/codegen/xai-grok-shell/src/agent/subagent/mod.rs` |
| HookEvent / Hook trait（V2 加回时参考） | grok 仓库 `extensions/` 模块 |
| Goal system（参考） | `crates/codegen/xai-grok-shell/src/session/goal_*.rs` |
| Compaction | `crates/codegen/xai-grok-shell/src/session/compaction.rs` |

---

**文档结束。后续 M1 → M3 的实施请对照本文件，遇到设计偏离先更新本文件再写代码。**

---

## 15. 多方位打磨决策（第二轮对抗性审核）

> 8 位 oracle 分别从 8 个维度深度挑战方案：接口一致性 / 错误处理 / 测试 / 可观察性 / 模型适配 / 性能 / 安全 / DX。本节是这些挑战的**最终决策汇总**。

### 15.1 决策来源

| 维度 | 关注点 |
|---|---|
| **#1 接口一致性** | trait 方法命名 / ToolResult 字段过简 / StopReason 映射 / 错误诊断 |
| **#2 错误处理** | 模型响应边界 / Goal JSON 解析失败 / 持久化 / Subagent depth |
| **#3 测试策略** | 单元测试盲区 / 集成测试 / mock vs 真模型 / Goal 可测性 |
| **#4 可观察性** | Logging 完整性 / Debug 输出 / 错误诊断 / 重放 / Metrics / CLI 交互 |
| **#5 模型适配** | OpenAI-compatible 实际差异 / SSE 解析 / tool_calls 聚合 / token 计数 / retry |
| **#6 性能/资源** | 消息内存累积 / String clone / Mpsc 容量 / Bash 僵尸进程 / Goal 内存 |
| **#7 安全** | ~~Prompt injection / Tool 越权 / 敏感信息 / 路径遍历 / 命令注入 / Session 隔离~~（**MVP 不做**，见 §11 范围裁剪） |
| **#8 DX** | 新人上手 / 函数过长 / 命名不一致 / 错误处理风格 / 依赖最小化 / CI |

### 15.2 P0：必须改（MVP 阶段阻塞实施）

> 这 10 项决策必须在 Day 1-4 落实，否则实施会反复撞墙。

| # | 决策 | 修复点 | 来源 |
|---|---|---|---|
| **1.1** | 统一 trait 方法命名风格：所有 trait 方法统一为 `run` / `chat` / `execute`（动词风格），不再有 `invoke` / `complete` / `run_turn` 混用 | §3.2 `Tool::invoke` → `Tool::run`；§4.1 `ModelProvider::complete` → `ModelProvider::chat`；§5.3 `ChildRunner::run_turn` → `ChildRunner::run` | #1 |
| **1.2** | `SessionConfig` 增加 `context_window: u32` 字段（compaction 阈值用，MVP 默认 128000） | §6.1 SessionConfig 加字段 | #5 |
| **1.3** | `ToolResult` 增加可选字段 `truncated: bool` 和 `exit_code: Option<i32>`（`#[serde(default)]` 兼容旧 JSON） | §3.2 ToolResult 扩展 | #1 + #2 |
| **1.4** | `AgentError::Cancelled` 拆为 `UserCancelled` / `Timeout(Duration)` 两个 variant | §3.4 AgentError 重构 | #2 + #8 |
| **1.5** | `FinishReason`（model 层）和 `StopReason`（agent 层）显式映射表，新增 `map_finish_reason(finish: FinishReason) -> StopReason` 函数 | §6.4 react_step 加映射 | #1 |
| **1.6** | `GoalLoopState` 字段改 private + getter 方法（避免外部绕过 reset 直接修改计数） | §6.6 GoalLoopState | #1 + #8 |
| **1.7** | `ProgressEval::AdjustPlan { new_goals: Vec<Goal> }` 改为 `AdjustPlan { goals: Vec<Goal> }`（与其他字段命名一致） | §6.5 ProgressEval | #1 |
| **1.8** | Token 估算从 `messages.len() * 200` 改为按字符估算 `messages.iter().map(\|m\| m.estimated_chars()).sum::<usize>() / 4`（更接近真实 token） | §6.7 Compaction | #5 + #6 |
| **1.9** | `SessionActor` 加 `shutdown(self)` 方法：cancel token + 等 spawned tasks 退出 + 5s timeout | §6.1 + §7 bin | #6 |
| **1.10** | ~~默认 `PermissionHook` 规则改为 default-deny~~ **【v1.3 砍头，整决策删除】** 见 §11.2 | — | — |

### 15.3 P1：应该改（MVP 阶段做但不阻塞）

> 这 20 项决策贯穿 M1-M4，每项 0.5-1.5 天工作量。

| # | 决策 | 修复点 | 来源 |
|---|---|---|---|
| **2.1** | `BashTool` 加 `.kill_on_drop(true)` + 显式 `Child::wait()` 兜底防僵尸进程 | §5.2 BashTool | #6 |
| **2.2** | `ToolContext` 持有 `Arc<str>` / `Arc<PathBuf>` 共享大字符串，减少 clone | §3.2 ToolContext | #6 |
| **2.3** | ~~`ReadFileTool` 加 canonicalize 检查 + size 上限（10MB）+ 截断逻辑~~ **【MVP 不做，删除】** | — | — |
| **2.4** | ~~`WriteFileTool` 加 canonicalize + 必须在 cwd 内的白名单校验~~ **【MVP 不做，删除】** | — | — |
| **2.5** | ~~`BashTool` env 用白名单~~ **【MVP 不做，删除】** 保留完整 `ctx.env` | — | — |
| **2.6** | 模型响应边界：`content=None && tool_calls=[]` → EndTurn；`finish_reason=Length` → 重试 1 次后报错 | §6.4 react_step | #2 |
| **2.7** | Classifier JSON parse 失败 → 重试 1 次 + strip markdown fence（` ```json...``` `） | §6.5 classifier.rs | #2 |
| **2.8** | Planner 输出 0 个 / >7 个 goals → fallback 拆 1 个 / 截断到 7 个 | §6.5 planner.rs | #2 |
| **2.9** | ~~Hook panic 兜底~~ **【v1.3 砍头，整决策删除】** Hook 不存在就没有 panic 处理 | — | — |
| **2.10** | Subagent spawn 上限：**depth ≤ 2** 且 **breadth ≤ 3**（同 parent 下 child 数），超限返回 `SubagentBreadthExceeded` | §5.3 TaskTool | #6 |
| **2.11** | Tracing subscriber 初始化：bin 启动时配 `EnvFilter` + session span（自动注入 `session_id` 到所有 log） | §8 bin | #4 + #8 |
| **2.12** | `AgentEvent::SubagentSpawn/Done` 加 `depth: u32` 字段，让父/子事件可区分（stderr 加 `[depth=1]` 前缀） | §3.3 AgentEvent | #4 |
| **2.13** | 新增 slash commands：`/goals`（看目标列表）；Ctrl+C = cancel 当前 turn（不退 bin） | §8 bin | #4 |
| **2.14** | ~~错误信息脱敏~~ **【MVP 不做，删除】** | — | — |
| **2.15** | `workspace.dependencies` 精简：去掉 `chrono` / `glob` / `once_cell`（用 `std::sync::OnceLock` + 手写 substring match） | §9 Cargo.toml | #8 |
| **2.16** | ~~`BashTool` 输出加 ANSI escape strip~~ **【MVP 不做，删除】** | — | — |
| **2.17** | `react_step()` 拆为 4 个子函数：`build_request` / `execute_stream` / `append_assistant` / `execute_tool_calls`（每个 ≤30 行） | §6.4 | #8 |
| **2.18** | `run_turn_with_goals()` 拆为 3 个子函数：`migrate_intent` / `plan_if_needed` / `loop_with_eval` | §6.3 | #8 |
| **2.19** | ~~System prompt 加防 prompt injection 限制语~~ **【MVP 不做，删除】** | — | — |
| **2.20** | 新增 `docs/ARCHITECTURE.md`：一张图 + 数据流，新人 5 分钟看完 | 新文件 | #8 |

### 15.4 V2：延后（MVP 不做）

> 这 15 项决策明确不做，留 V2 处理。

| # | 决策 | 来源 | V2 备注 |
|---|---|---|---|
| **3.1** | Reflector 自反思机制（事件驱动） | #2 | V2.1 |
| **3.2** | Session persistence（messages.jsonl + rewind_points.jsonl） | #4 | V2.2 |
| **3.3** | Audit log（独立 module，按时间/用户/命令索引） | #7 | V2.2 |
| **3.4** | Benchmarks（token 准确性 / 延迟分布 / Goal 收敛步数） | #3 | V2.4 |
| **3.5** | Multi-provider 切换（Anthropic / Gemini / 本地 vLLM） | #5 | V2.3 |
| **3.6** | Replayability（重放上次对话 `--replay messages.json`） | #4 | V2.5 |
| **3.7** | `tiktoken-rs` 精确 token 计数 | #5 | V2.4 |
| **3.8** | 进程退出完整清理 hook（file descriptor 跟踪 + child reaper） | #6 | V2.5 |
| **3.9** | 全局 `~/.mvp-agent/config.toml`（用 CLI args 够了） | #8 | V2.5 |
| **3.10** | README 拆分成 4 个文档（保持单文件 + 加锚点） | #8 | V2.5 |
| **3.11** | ~~`PreModel` / `PreToolUse` hook 的 `Modify` 变体~~ **【v1.3 砍头】** | — | — |
| **3.12** | Compaction L5 ladder（分级降级策略） | #5 | V2.1 |
| **3.13** | `SearchReplaceTool` / `GrepTool` | 原方案已砍 | V2.5 |
| **3.14** | 后台 fire-and-forget subagent（Coordinator actor 回归） | 原方案已砍 | V2.1 |
| **3.15** | ~~Hook chain 级联 `Modify`（多个 hook 改 args）~~ **【v1.3 砍头】** | — | — |

### 15.5 优先级与工作量矩阵

| 优先级 | 决策数 | 工作量 | 落地里程碑 |
|---|---|---|---|
| **P0**（Day 1-5 必修） | 9 项 | ~3 天 | M1 |
| **P1**（贯穿 M1-M3） | 13 项 | ~5 天 | M1-M3 |
| **V2**（明确不做） | 11 项 | — | V2.1-V2.5 |

> 注：v1.2 砍掉 9 项安全/权限决策（2.3 / 2.4 / 2.5 / 2.14 / 2.16 / 2.19 / 3.3 + §11.2 列出）；v1.3 再砍 4 项 Hook 相关（1.10 / 2.9 / 3.11 / 3.15），最终落地 33 项（9 P0 + 13 P1 + 11 V2）。

---

## 16. P0/P1 级决策的代码示例

> 把 P0/P1 中**最容易写错**的 5 个决策的具体代码示例放在这里，实施时直接对照。
>
> **v1.2 更新**：原 §16.2（Default-deny Permission）和 §16.3（WriteFile 白名单）已删除。安全/权限相关代码示例见 §11.2。

### 16.1 决策 1.4：AgentError 拆 `Cancelled` 为 `UserCancelled` / `Timeout`

```rust
// crates/core/src/error.rs
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AgentError {
    #[error("unknown tool: {0}")]
    UnknownTool(String),
    #[error("invalid arguments: {0}")]
    InvalidArgs(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("model error: {0}")]
    Model(String),
    #[error("sse parse error: {0}")]
    SseParse(String),
    #[error("user cancelled")]
    UserCancelled,                                              // ★ 拆分后
    #[error("timeout after {0:?}")]
    Timeout(std::time::Duration),                               // ★ 拆分后
    #[error("json parse error: {0}")]
    ParseError(String),
    #[error("subagent depth exceeded (max 2)")]
    SubagentDepthExceeded,
    #[error("subagent breadth exceeded (max 3 siblings)")]      // ★ 新增
    SubagentBreadthExceeded,
    #[error("blocked by goal evaluator: {0}")]
    GoalBlocked(String),
    #[error("path traversal blocked: {0}")]                    // ★ 新增
    PathTraversal(String),
    #[error("rate limit: retry after {0:?}")]                   // ★ 新增
    RateLimited(std::time::Duration),
}
```

### 16.2 决策 1.3：ToolResult 扩展

```rust
// crates/core/src/tool.rs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    pub success: bool,
    pub output: String,
    #[serde(default, skip_serializing_if = "is_false")]
    pub truncated: bool,                                         // ★ 新增
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,                                  // ★ 新增
}

fn is_false(b: &bool) -> bool { !*b }
```

> ~~决策 1.10 Default-deny Permission + 决策 2.4 WriteFile 白名单~~ **【v1.3 砍头】**（见 §11.2 范围裁剪）。整个 Hook 系统已砍掉，PermissionHook 占位也无需保留。

### 16.3 决策 1.9 + 1.8：SessionActor shutdown() + 字符估算

```rust
// crates/agent/src/session.rs
impl SessionActor {
    /// 决策 1.9：优雅关闭
    pub async fn shutdown(self) {
        self.cancel.cancel();
        let _ = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            self.drain_spawned_tasks(),
        ).await;
        tracing::info!(session_id = %self.id.0, "session shutdown complete");
    }

    async fn drain_spawned_tasks(&self) {
        // 等待所有 spawned task 通过 cancellation token 退出
        // MVP: 简单 busy-wait，V2: 用 JoinSet
    }
}

// Message 加字符估算（决策 1.8）
impl Message {
    pub fn estimated_chars(&self) -> usize {
        match self {
            Message::System { content } => content.len() + 10,
            Message::User { content } => content.len() + 10,
            Message::Assistant { content, tool_calls } => {
                content.as_ref().map(|c| c.len()).unwrap_or(0)
                    + tool_calls.iter().map(|t| t.name.len() + t.arguments.to_string().len() + 20).sum::<usize>()
            }
            Message::Tool { content, .. } => content.len() + 10,
        }
    }
}

// crates/agent/src/compaction.rs（决策 1.8 修正）
pub async fn maybe_compact(&mut self) -> Result<(), AgentError> {
    let estimated_chars: usize = self.messages.iter().map(|m| m.estimated_chars()).sum();
    let estimated_tokens = estimated_chars / 4;  // 1 token ≈ 4 chars
    let threshold = self.config.context_window * 85 / 100;
    if estimated_tokens < threshold { return Ok(()); }
    // ...
}
```

### 16.4 决策 2.1：BashTool kill_on_drop（保留）

```rust
// crates/tools/src/builtin/bash.rs
async fn run(&self, args: serde_json::Value, ctx: ToolContext)
    -> Result<ToolResult, AgentError>
{
    #[derive(Deserialize)]
    struct Args { cmd: String, timeout_secs: Option<u64> }
    let Args { cmd, timeout_secs } = serde_json::from_value(args)?;
    let timeout = timeout_secs.unwrap_or(60);

    // ★ 直接传完整 ctx.env（MVP 不做 env 白名单——决策 2.5 已砍）
    let mut child = tokio::process::Command::new("sh")
        .arg("-c").arg(&cmd)
        .current_dir(&ctx.cwd)
        .envs(ctx.env.iter())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        // ★ 决策 2.1：kill_on_drop 防僵尸进程（性能问题，不是安全问题）
        .kill_on_drop(true)
        .spawn()
        .map_err(AgentError::Io)?;

    let output = match tokio::time::timeout(
        std::time::Duration::from_secs(timeout),
        child.wait_with_output(),
    ).await {
        Ok(Ok(out)) => out,
        Ok(Err(e)) => return Err(AgentError::Io(e)),
        Err(_) => {
            // 决策 1.4：用 Timeout variant 而不是 Cancelled
            return Err(AgentError::Timeout(std::time::Duration::from_secs(timeout)));
        }
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    // ★ 不做 ANSI strip（决策 2.16 已砍）

    Ok(ToolResult {
        success: output.status.success(),
        output: format!(
            "{}{}",
            stdout,
            if !stderr.is_empty() { format!("\nstderr: {stderr}") } else { String::new() }
        ),
        truncated: false,
        exit_code: output.status.code(),
    })
}
```

> ~~决策 2.5 BashTool env 白名单 + 决策 2.16 ANSI strip~~ **MVP 不做**（见 §11 范围裁剪）。

### 16.5 决策 1.5：FinishReason → StopReason 映射

```rust
// crates/agent/src/react.rs
fn map_finish_reason(finish: FinishReason) -> StopReason {
    match finish {
        FinishReason::Stop       => StopReason::EndTurn,
        FinishReason::ToolCalls  => StopReason::EndTurn,    // react_step 继续
        FinishReason::Length     => StopReason::Error,     // 截断需重试
        FinishReason::Error      => StopReason::Error,
    }
}
```

### 16.6 决策 1.1 + 2.17：trait 命名统一 + react_step 拆分

```rust
// §3.2 Tool trait（决策 1.1）
#[async_trait]
pub trait Tool: Send + Sync {
    fn kind(&self) -> ToolKind;
    fn spec(&self) -> &ToolSpec;
    async fn run(&self, args: serde_json::Value, ctx: ToolContext)
        -> Result<ToolResult, AgentError>;     // ★ invoke → run
}

// §4.1 ModelProvider trait（决策 1.1）
#[async_trait]
pub trait ModelProvider: Send + Sync {
    async fn chat(&self, req: ChatRequest) -> Result<ChatResponse, ModelError>;   // ★ complete → chat
    fn stream(&self, req: ChatRequest)
        -> Box<dyn Stream<Item = Result<StreamChunk, ModelError>> + Send + Unpin>;
}

// §6.4 react_step 拆分（决策 2.17）
impl SessionActor {
    async fn react_step(&mut self) -> Result<StepOutcome, AgentError> {
        let req = self.build_request()?;                    // 子函数 1
        let response = self.execute_stream(req).await?;     // 子函数 2（v1.3：去 Pre/PostModel hook fire）
        self.append_assistant(&response);                   // 子函数 3

        if response.tool_calls.is_empty() {
            return Ok(StepOutcome::Finish(map_finish_reason(response.finish_reason)));
        }
        self.execute_tool_calls(response.tool_calls).await?; // 子函数 4（v1.3：去 Pre/PostToolUse hook fire）
        Ok(StepOutcome::Continue)
    }

    fn build_request(&self) -> Result<ChatRequest, AgentError> { /* ... */ }
    async fn execute_stream(&mut self, req: ChatRequest) -> Result<ChatResponse, AgentError> { /* ... */ }
    fn append_assistant(&mut self, r: &ChatResponse) { /* ... */ }
    async fn execute_tool_calls(&mut self, calls: Vec<ToolCall>) -> Result<(), AgentError> { /* ... */ }
}
```

---

## 17. 第二轮审核历史（v1.0 → v1.1）

### 17.1 版本历史

| 版本 | 日期 | 内容 |
|---|---|---|
| v0.1 (草案) | — | 原始方案（无审核） |
| v0.2 (审核稿) | — | 加入 5 个 oracle 对抗性审核 |
| v1.0 (权威版) | 2026-08-01 | 第一轮裁定后整合（§8 关键架构裁定） |
| v1.1 (打磨版) | 2026-08-01 | 加入 8 个 oracle 多方位打磨，新增 §15 决策 + §16 代码示例 |
| **v1.2 (裁剪版)** | 2026-08-01 | 用户指示：删除所有权限/安全相关能力。§11.2 (v1.2 时为 §12.2) 新增专项说明；§15 删除 1.10/2.3/2.4/2.5/2.14/2.16/2.19 共 7 项；§16 删除对应代码示例 |

### 17.2 第二轮审核参与方

- **打磨 #1**：接口与命名一致性（oracle）→ 7 个问题
- **打磨 #2**：错误处理与边界 case（oracle）→ 7 个问题
- **打磨 #3**：测试与验证策略（oracle）→ 7 个问题
- **打磨 #4**：可观察性与可调试性（oracle）→ 7 个问题
- **打磨 #5**：模型适配层健壮性（oracle）→ 7 个问题
- **打磨 #6**：性能与资源管理（oracle）→ 9 个问题
- **打磨 #7**：安全性与权限（oracle）→ 12 个问题
- **打磨 #8**：工程化与开发者体验（oracle）→ 12 个问题

**总计 68 个问题点 → 收敛为 36 项决策（9 P0 + 14 P1 + 13 V2，砍掉 9 项安全/权限决策）**。

### 17.3 文档维护约定（更新）

- §8 关键架构裁定：第一轮审核结论（架构层面）
- **§15 多方位打磨决策**：第二轮审核结论（实施层面）—— 新增
- **§16 代码示例**：P0/P1 决策的具体落地代码 —— 新增
- 任何代码偏离本文档，必须先更新本文档再改代码
- §15 审核历史：每次审核追加一节

---

## 附录 C：第二轮打磨对照表（打磨维度 → 决策 ID）

> 注：**打磨 #7（安全/权限）已被用户明确删除**，本表不列出。所有 P0/P1 决策均不含安全/权限相关项。

| 打磨维度 | 涉及决策 |
|---|---|
| #1 接口一致性 | 1.1, 1.3, 1.5, 1.6, 1.7, 2.13, 2.17, 2.18, 3.11, 3.15 |
| #2 错误处理 | 1.3, 1.4, 1.9, 2.6, 2.7, 2.8, 2.9, 3.1 |
| #3 测试策略 | 2.10, 2.13, 3.4 |
| #4 可观察性 | 2.11, 2.12, 2.13, 3.2, 3.6 |
| #5 模型适配 | 1.2, 1.8, 3.5, 3.7, 3.12 |
| #6 性能/资源 | 1.8, 1.9, 2.1, 2.2, 2.10, 3.8 |
| ~~#7 安全~~ | **【MVP 不做，已删除】** |
| #8 DX | 1.1, 1.4, 1.6, 2.11, 2.15, 2.17, 2.18, 2.20, 3.9, 3.10 |

---

**文档结束（v1.3 砍掉整个 Hook 系统 + M4 改 tokio::join! 并发）。后续 M1 → M3 的实施请对照 §15 决策清单 + §16 代码示例，遇到设计偏离先更新本文件再写代码。**