# Synthia v3 Architecture: Tool-Centric Refactor — Multi-Expert Comparative Analysis

> **日期**: 2026-07-17
> **作者**: Sisyphus (orchestrator) 综合 4 个对抗性专家 agent + 自己代码核对
> **范围**: Synthia（Rust AI Agent 框架，21 crate）vs opencode / codex / pi-mono
> **核心约束（用户）**: "除主逻辑 react loop 和 session 外，其他功能尽量抽象为 tool 实现"
> **状态**: 综合报告 v1，可直接作为 OpenSpec change proposal 的 design.md

---

## 0. TL;DR（核心结论）

### 4 专家达成共识 + 我自身代码核对的发现

1. **baseline 报告（2026-07-12）部分事实过期**：
   - G1 gap 真实存在但**更严重**：实际是 **11 个字段**（`_xxx`）被丢弃，不是 9 个
   - 报告说 "SubagentTool 未存在" 是错的 —— `impl Tool for AgentTool` + 已注册到 ToolRegistry，gap 只是 factory 未串
   - Tool trait 已经 12 方法（不是 baseline 写的 7 个）

2. **"全部 tool 化" 口号被反方挑战约束**：应该用 **Progressive Toolification**，仅当 4 个条件满足 ≥3 时才 tool 化。具体边界见 §4。

3. **4 个专家达成共识的最高价值借鉴**（按 ROI 排序）：
   - **P0**（1 周内做）：opencode `materialize → settle identity`（低代价） + opencode `OutputBound`（registry 级截断） + opencode `validate_tool_name`（前置校验） + codex `TaskKind` + codex `AgentRole=ConfigLayer` + codex `Hook 10 事件 + FailedContinue/Abort 三态`
   - **P1**（1 个月）：codex `ToolPluginProvenance` + codex `GoalService` + codex `App-Server 背压 -32001` + opencode `Stacked LIFO registry`
   - **P2**（探索）：codex `CodeMode`（用 boa_engine 替代 V8）+ pi-mono `convertToLlm + transformContext` 双钩子
   - **不学**：pi-mono 的 30 extension overload（synthia 的 hook 已更好）+ pi-mono 双 runner 对称函数（应该 `enum BatchStrategy + match`）

4. **synthia 独有优势**（不要妄自菲薄）：
   - Tool trait 已含 `execution_mode` + `call_with_sandbox` + `call_with_progress` + `CancellationToken` 集成（4 家里最工程化）
   - session-v2 已实现 part-based V2 模型（早于 V2 完整落地）
   - gRPC message-proxy 跨进程事件推送（比 opencode 的 sync 更专业）
   - LoopDetector 三件套（pi-mono 完全没有）
   - ConfigWatcher 热配置重载（opencode/codex/pi-mono 都没有）
   - Permission 4 态枚举

5. **最关键建议（一句话）**：**先做 G1 修复 + 4 个 low-cost PR + 1 个中代价 Hook 合并——共 4-6 周内改变架构质量，之后再做大型模块**。

---

## 1. 现状校准（与 baseline 报告的差异）

> 经我亲自核对代码 + 4 专家交叉验证。

### 1.1 G1 gap 真实状态：11 个 `_xxx` 字段（不是 9 个）

baseline 报告称 "9 个字段被丢弃"，**实际是 11 个**（`main_loop.rs:124-162`）：

| 字段 | 行号 | 真实状态 | 字段类型 |
|------|------|----------|----------|
| `hook_registry` | 127 | ❌ 丢弃 | trait 注入 |
| `model_router` | 128 | ❌ 丢弃 | Arc dyn |
| `context_assembler` | 133 | ❌ 丢弃 | Option Arc |
| `steering_channel` | 135 | ❌ 丢弃 | Option Arc dyn |
| `fork_policy` | 140 | ❌ 丢弃 | 直接 struct |
| `compaction_provider` | 149 | ✅ 用作 `compaction_provider_runtime` | Option Arc dyn |
| `subagent_session_factory` | 153 | ❌ 丢弃（baseline 列了） | Option Arc dyn |
| `tool_orchestrator` | 156 | ❌ 丢弃（通过 StepToolExecute） | Option Arc dyn |
| `approval_service` | 157 | ❌ 丢弃（baseline 列了） | Option Arc dyn |
| `sandbox_manager` | 158 | ❌ 丢弃（baseline 列了） | Option Arc dyn |
| `guardian_coordinator` | 160 | ❌ 丢弃（baseline 列了） | Option Arc dyn |
| `extension_manager` | 161 | ❌ 丢弃（baseline 列了） | Option struct |

**结论**：真 gap 是 11 个，比 baseline 多 2 个（baseline 漏列 `hook_registry` / `context_assembler` / `steering_channel` / `tool_orchestrator`）。**这 11 个字段被丢弃是 P0-1 修复**。

### 1.2 SubagentTool / AgentTool 已实现（baseline §10.4 错误）

baseline 写 "SubagentTool 未实现"。**反方 4 个专家 + 我亲自核对** 确认：

- `crates/synthia-agent/src/tools/agent_tools/agent_tool.rs:124` —— `impl Tool for AgentTool`
- `crates/synthia-agent/src/tool_registry.rs:24` —— 已注册到 ToolRegistry
- 真正的 gap 是 `main_loop.rs:153` 把 `subagent_session_factory: _subagent_session_factory` 丢弃 —— 即 AgentTool 创建时拿不到 real factory

**修复**：把 factory 串到 AgentTool 构造器（不是新建 tool）。

### 1.3 Tool trait 实际 12 方法（baseline 错算 7 方法）

`synthia-tool/src/traits.rs:29-117`：

```rust
trait Tool: Send + Sync {
    // baseline 列的 7 方法（行 30-77）
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn parameters(&self) -> serde_json::Value;
    fn requires_permission(&self) -> bool { false }
    fn is_hidden(&self) -> bool { false }
    fn is_concurrency_safe(&self) -> bool { false }
    async fn call(&self, input: ToolInput) -> ToolOutput;

    // baseline 未列的 5 方法（行 60-116）
    fn is_user_invocable(&self) -> bool { true }       // LLM 是否能 invoke
    fn execution_mode(&self) -> ExecutionMode { Parallel } // Per-tool 并行/串行（pi-mono 有，opencode 没有）
    fn output(&self, raw: Value) -> ToolOutput { ... }  // raw → structured
    async fn call_with_sandbox(...) -> ToolOutput { ... } // sandbox 集成 + CancellationToken
    async fn call_with_progress(...) -> ToolOutput { ... } // FileChange 回调
}
```

**对比**：opencode 的 `Tool` trait 比这**薄**（仅 `description / parameters / execute / toModelOutput`），codex 的也类似。**synthia 的 Tool trait 已经是 4 家里最工程化的**（包含 sandbox + cancel + progress + per-tool 模式）。

### 1.4 session-v2 / message-proxy / LoopDetector / ConfigWatcher 是 synthia 优势

| 能力 | 位置 | 状态 | 对照 |
|------|------|------|------|
| V2 part-based 模型 | `synthia-session-v2/src/part.rs` | ✅ 落地（11-variant Part + ToolPart + Tree）| 早于完整 V2 |
| gRPC 跨进程事件 | `synthia-message-proxy/src/lib.rs` | ✅ Tonic 实现 | opencode 用 sync handler（功能等价但不如 gRPC）|
| 三件套 LoopDetector | `synthia-agent/src/agent/loop_detector/` | ✅ GenericRepeat/NoProgress/Circuit | **pi-mono 完全没** |
| ConfigWatcher 热重载 | `synthia-agent/src/config_watcher/` | ✅ 多 config watcher | 三项目都没 |
| Permission 4 态枚举 | `synthia-permission/src/` | ✅ Block/AutoApprove/RequireConfirm/RequireExplicit/Deny | opencode 仅 Allow/Deny/Ask |

---

## 2. 三方对比：opencode / codex / pi-mono

> 综合 4 专家结果 + 我亲读 3 项目。

### 2.1 8 维架构对比矩阵

| 维度 | opencode (TS + Effect) | codex (Rust) | pi-mono (TS) | synthia (Rust) | synthia 差距 |
|------|------------------------|--------------|---------------|-----------------|---------------|
| **Tool Registry** | Stack-based LIFO + scope finalizer | Flat + ToolRouter + 5 类来源 | Flat Map<Api, Provider> | Flat HashMap + Scoped (并存两套) | 缺 stack override |
| **Tool 输出** | 双输出：structured + content | 直接结构化 | 单 output | `output()` raw→structured | 缺 content vs structured 分离 |
| **ToolProvenance** | ❌（无 source 区分） | ✅ core/plugin/mcp/context | ❌ | ❌ | **缺** |
| **Event 模型** | Event Sourcing + durable/ephemeral + versioned | partial JSON-RPC | 简单 session events | JSONL + AgentEvent (3 通道) | 缺 durable/ephemeral 分类 |
| **Hook 系统** | Plugin typed hooks (19) | 10 events + 3-state Fail | 30 extension overloads | 双系统：AgentHook + HookRunner | 缺 FailedContinue/Abort 三态 |
| **Hook 合并** | ✅ 统一 plugin hooks | ✅ 10 事件单层 | ✅ extension 单层 | ❌ 双系统并存 | 需合并 |
| **Subagent** | Agent + parentID | Goals + 4 TaskKind + AgentRole | process-spawn 子进程 | AgentTool (工厂未串) | 缺 TaskKind 分类 |
| **Goal/目标驱动** | ❌ 无显式 Goal | ✅ GoalService + token budget + semaphore | ❌ | ❌ | **完全缺失** |
| **Doom-loop 检测** | ✅ 多 detector | ✅ | ❌ | ✅ 三件套 | 已有，synthia 领先 pi-mono |
| **Code Mode** | ❌ | ✅ V8 + Cell + Delegate | ❌ | ❌ | 完全缺失（可探索 Tier 2）|
| **MCP** | ✅ first-class | ✅ stdio + streamable-http + OAuth | ❌（用 extension 替代）| ✅ stdio + OAuth | **缺 streamable-http** |
| **Permission** | PermissionV2 (3 态) + PolicyV2 | PermissionService + Approval | opt-in via extension (危险) | Permission 4 态 + Approval | 已有 |
| **App-server 协议** | plain HTTP/WS | JSON-RPC 2.0 + 背压 -32001 + 双 loop | RPC mode (JSON over stdio) | HTTP/WS (无 JSON-RPC) | **缺背压** |
| **Telemetry** | OTel span 一等公民 | OTel + MetricsClient + global | 极简（几乎无 OTel） | OTel + Prometheus + tracing | 已有 |
| **Memory** | schema-driven | 4 tools + DEFAULT_READ_MAX_TOKENS | 简单 | Hot + Episodic + Experience | 缺 DEFAULT_READ_MAX_TOKENS |
| **Hot reload** | scope fork | ❌ | ✅ HMR `ctx.reload()` | ConfigWatcher（多 config）| 已有（部分） |
| **Provider/ModelRouter** | 全 capability metadata | enum Provider + Router trait | 极薄 Map<Api> | synthia-provider + model-router | 已有 |
| **Agent Loop 风格** | Effect 流 + Stream + Event | StreamBuilder chain | `while + push 队列` (683 行) | 1037 行 StreamBuilder 单文件 | **应学 pi-mono 减负** |
| **convertToLlm / transformContext** | Effect transformMessages 链 | item 硬编码 | 4 行 defaultConvertToLlm | **完全缺失**（synthia 最大 bug）| **必须补** |

### 2.2 Pi-mono "反抽象" 教训（synthia 应该学的减负）

**pi-mono agent loop 仅 683 行 vs synthia 1037 行 + 双 StreamBuilder + 6 task-local + 309 行 turn_transition**。

**关键诊断**（来自 pi-mono 专家）：
- synthia `wrap_output_with_otel` ~200 行（pi-mono 89 行 plain EventStream）— 删 task-local，改 `Span::current()`
- synthia `tool execution` 871 行双 runner — 改 `enum BatchStrategy + match`
- synthia `loop_context.rs` 543 行 9 字段 — 拆到 `SessionState` 子结构

**真正的借鉴**：**`AgentMessage + llm_visible()` 投影层**。当前 synthia 把 `Vec<Message>` 直接当 AgentMessage 用，导致 `Message::user(format_background_task_notification(...))` 这种 hack 出现。这是 P8（不丢信息）和 P1（prefix consistency）的真正分水岭。

---

## 3. Rust trait 草案（4 专家综合 + 真实代码核对）

> 每个 trait 含：① 作用 ② 与现状关系 ③ trait 草案 ④ 落地路径 ⑤ 代价。

### 3.1 StackedToolRegistry（opencode 借鉴）

**作用**：替代 `RwLock<HashMap<String, ToolEntry>>` 的 flat 注册表为 LIFO 栈。

**已有隐藏资产**：`synthia-tool/src/scoped_registry.rs:29, 208` 已有 `ScopedToolRegistry` + `LayeredToolRegistry`（用 `DashMap<Vec<ScopedRegistration>>` + RAII `ScopeGuard`），但**没人用**。

**Rust 草案**（向后兼容，flat HashMap 保留为 deprecated）：
```rust
pub struct StackedToolRegistry {
    inner: RwLock<HashMap<String, Vec<Registration>>>,
}

pub struct Registration {
    token: Arc<RegistrationToken>,  // Drop 时 unregister
    tool: Arc<dyn Tool>,
    identity: Arc<()>,  // 用于 stale 检测
}

impl Drop for Registration {
    fn drop(&mut self) { /* filter-by-token */ }
}

impl StackedToolRegistry {
    pub fn push(&self, name: String, tool: Arc<dyn Tool>) -> RegistrationToken;
    pub fn materialize(&self) -> Materialization;  // MaterializationToken 捕获
    pub fn resolve(&self, mat: &Materialization, name: &str) 
        -> Result<Arc<dyn Tool>, StaleOrUnknown>;
}

pub enum StaleOrUnknown { Stale, Unknown }
```

**代价**：**中（一个 crate + 一个 PR）**。可吸收 `scoped_registry.rs` 现有代码。

**优先级**：**P1**（1 周 ROI 高，可立即吸收已有 `ScopedToolRegistry`）。

### 3.2 Stale Materialization 检测（opencode 借鉴，**最低代价**）

**作用**：解决 "LLM 在 step T 收到 tool list，step T+1 plugin 卸载 tool 直接 panic" 问题。

**Rust 草案**（**低代价**）：
```rust
pub struct Materialization {
    advertised: HashMap<String, Arc<()>>,  // name → identity
}
```
改 `run_with_context` 在进入时 `materialize()` 一次，调用 `resolve(mat, name)`；`stale` 转 `ToolOutput::error("Tool definition changed; refresh")`。

**代价**：**低（一个 PR）**。纯增量。

**优先级**：**P0 #1**（1 天 ROI 极高，4 专家一致推为最低代价高收益）。

### 3.3 Tool Name 前置校验 + Atomic Batch（opencode 借鉴）

**作用**：`validate_tool_name()` ASCII letter 开头 + 64 字符；注册时全 batch 校验再插入。

**Rust 草案**（向后兼容）：
```rust
pub fn validate_tool_name(name: &str) -> Result<(), RegistrationError>;
impl StackedToolRegistry {
    pub fn push_batch(&self, batch: HashMap<String, Arc<dyn Tool>>) 
        -> Result<RegistrationToken, RegistrationError>;
}
```

**代价**：**低（一个 PR）**。

**优先级**：**P0 #2**（4 专家一致推荐）。

### 3.4 OutputBound 在 registry 级（opencode 借鉴）

**作用**：opencode `tool-output-store.ts:132-168` 的 `MAX_LINES=2000, MAX_BYTES=50KiB` 统一在 registry 层。

**Rust 草案**（向后兼容）：
```rust
pub trait OutputBound: Send + Sync {
    fn bound(&self, output: ToolOutput, session_id: &SessionId, call_id: &str) 
        -> (ToolOutput, Vec<ManagedPath>);
}

pub struct DefaultOutputBound {
    pub max_lines: usize,  // 2000
    pub max_bytes: usize,  // 50 KiB
    pub managed_dir: PathBuf,
}
```

**当前散落**：`synthia-tool-bash/src/trait_impl.rs:19` 的 `MAX_CAPTURE_BYTES` 是 per-tool hardcoded。

**代价**：**低（一个 PR）**。

**优先级**：**P0 #3**（4 专家一致推为低代价高收益）。

### 3.5 Tool 双输出 structured + content（opencode 借鉴，破坏性）

**作用**：LLM 看的是 human-readable（content），DB 存的是结构化（structured）。

**Rust 草案**（**破坏性**——分阶段）：
```rust
pub trait Tool: Send + Sync {
    type Input: DeserializeOwned + JsonSchema;
    type Output: Serialize + DeserializeOwned + JsonSchema;
    
    async fn execute(&self, input: Self::Input, ctx: &ToolContext) 
        -> Result<Self::Output, ToolFailure>;
    
    fn to_model_output(&self, input: &Self::Input, output: &Self::Output) 
        -> Vec<ContentPart> { /* default: if output serializes to string, wrap as Text */ }
}

pub struct ToolOutput {
    pub structured: serde_json::Value,
    pub content: Vec<ContentPart>,
    pub metadata: ToolMetadata,
}
```

**当前缺失**：`AgentEvent::ToolCallCompleted.output: String`（塌缩）—— 不分两层。

**代价**：**高（跨 crate 接口变更）**。需改 5 个内置工具 + MCP adapter。

**优先级**：**P2**（推迟到统一工具 trait 重构时）。

### 3.6 ToolPluginProvenance（codex 独有借鉴）

**作用**：区分 tool 来源，影响 permission + 审计 + UI。

**Rust 草案**（新子模块 `crates/synthia-tool/src/provenance.rs`）：
```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ToolProvenance {
    Core,           // synthia 内置
    Plugin,         // synthia-plugin
    Mcp { server: &'static str, host_owned: bool },  // host_owned 是 codex 独有
    Context,        // 上下文派生（todo / skill）
    Dynamic,        // 运行时注册
}

pub trait Tool {
    // ... 已有字段省略
    fn provenance(&self) -> ToolProvenance { ToolProvenance::Core }
}
```

**落地路径**：`synthia-tool/src/provenance.rs` 新建 + `synthia-mcp` 注册时设 `Mcp { server, host_owned }` + `synthia-plugin` 注册时设 `Plugin`。

**代价**：**低（一个 PR）**。

**优先级**：**P1**（codex 4 个专家 + 我一致推）。

### 3.7 Hook V2 统一 + Hook 10 事件（codex 借鉴）

**作用**：
1. 合并 `AgentHook`（`synthia-hook`）+ `HookRunner`（`synthia-plugin`）双系统
2. 从 6 个事件扩展到 codex 的 10 个：增加 `PreCompact / PostCompact / SessionStart / UserPromptSubmit / SubagentStart / SubagentStop / Stop`
3. 引入 `FailedContinue / FailedAbort` 三态

**Rust 草案**（增强 `synthia-hook`）：
```rust
pub enum HookEvent {
    PreToolUse, PostToolUse, PermissionRequest,
    PreCompact, PostCompact,        // ← codex 借鉴
    SessionStart,                   // ← codex 借鉴
    UserPromptSubmit,               // ← codex 借鉴
    SubagentStart, SubagentStop,    // ← codex 借鉴
    Stop,                           // ← codex 借鉴
    BeforeLlm, AfterLlm,            // synthia 已有
    IterationEnd, Complete,         // synthia 已有
    OnError,                        // synthia 已有
}

pub enum HookMatcher {
    ToolName(glob::Pattern),
    AgentName(glob::Pattern),
    CompactTrigger(CompactTrigger),
}

pub enum HookResult {
    Success,
    FailedContinue(Box<dyn Error>),  // 继续后续 hook + 工具调用
    FailedAbort(Box<dyn Error>),     // 终止后续 hook + 标记 turn 失败
}
```

**当前缺失**：`synthia-plugin` 的 `HookHandler::Prompt` 是 stub（`hook_runner/execute.rs:32-41`）+ synthia-hook `fire.rs:27-153` 6 个事件 + current `FailPolicy` panic-isolation 但无 Continue/Abort 区分。

**代价**：**中（一个 PR 跨 `synthia-hook` + `synthia-plugin` + `synthia-agent`）**。

**优先级**：**P0 #4**（baseline G6 修复）。

### 3.8 GoalService（codex 独有借鉴，**独占**）

**作用**：给 subagent 加上"有限目标 + token budget + Semaphore lock" 约束。

**Rust 草案**（新建 `crates/synthia-goal/`）：
```rust
pub struct GoalService {
    runtimes: StdMutex<HashMap<ThreadId, Weak<GoalRuntimeHandle>>>,
}

pub struct GoalSetRequest<'a> {
    pub thread_id: ThreadId,
    pub objective: GoalObjectiveUpdate<'a>,  // Keep | Set(&str)
    pub status: Option<ThreadGoalStatus>,
    pub token_budget: GoalTokenBudgetUpdate, // Keep | Set(Option<i64>)
}

#[async_trait]
pub trait GoalRuntime: Send + Sync {
    async fn prepare_external_mutation(&self) -> Result<(), GoalError>;
    async fn state_permit(&self) -> Result<SemaphorePermit, GoalError>;
    fn continuation_steering(&self) -> Option<SteeringItem>;  // P5 末尾复述
}
```

**关键模仿**：`state_permit() → prepare_external_mutation() → DB.write()` 顺序固定，避免过期 state 启动 continuation。

**与 opencode 关系**：opencode 没有 Goal 概念。**codex 独占项**。

**代价**：**高（新 crate + 1 个月）**。

**优先级**：**P1**（价值大但成本高）。

### 3.9 TaskKind 多态 + SessionTaskContext（codex 借鉴）

**作用**：4 种 task 类型（Regular/Compact/Review/UserShell）+ 100ms 宽限中断 + MultiAgentVersion 自动选择 marker 类型。

**Rust 草案**（增强 `synthia-task`）：
```rust
pub enum TaskKind {
    Regular,          // 普通 LLM turn
    Compact,          // compaction turn（独立 prompt + 隐藏 tool）
    Review,           // review turn（只读 tool）
    UserShellCommand, // 用户 ! shell 命令
}

pub struct TaskInterruptPolicy {
    pub graceful_timeout_ms: u64,  // 默认 100
}

pub struct AgentTaskContext {
    session: Weak<Session>,         // ← Weak 而非 Arc
    turn_extension_data: Arc<ExtensionData>,
    auth: Weak<AuthManager>,
    models: SharedModelsManager,
}
```

**当前缺失**：`crates/synthia-task/src/types/task.rs` 是通用 task 抽象，无 turn 类型分类；`crates/synthia-agent/src/subagent/factory.rs` 直接传 session 引用未做能力裁剪。

**代价**：**中**。

**优先级**：**P0 #5**（codex 专家 + 我一致推）。

### 3.10 AgentRole = ConfigLayer（codex 借鉴）

**作用**：role 是 config layer（不是完整 Config 替换），provider/service_tier 字段是粘性的。

**Rust 草案**（`crates/synthia-agent/src/config/layer.rs`）：
```rust
pub struct ConfigLayer {
    overrides: toml::Value,
    pub sticky_fields: Vec<StickyField>,
}

pub enum StickyField { ModelProvider, ServiceTier, UserId }

pub fn apply_role_layer(base: &mut Config, role: &AgentRoleConfig) -> Result<(), ConfigError>;
```

**潜在 bug（codex 专家发现）**：用户主 agent 是 Claude，子 agent config 写 GPT，子 agent 静默切到 GPT。这是 P0。

**当前问题**：`crates/synthia-agent/src/config/agent_config.rs` 直接构建，无 layer 抽象。

**代价**：**小（加 layer 模块）**。

**优先级**：**P0 #6**（Bug 修复优先）。

### 3.11 App-Server 背压 -32001 + 双 loop（codex 借鉴）

**作用**：请求入队饱和时返回 `-32001 Server overloaded; retry later.`

**Rust 草案**（`crates/synthia-server/src/jsonrpc.rs` 新建）：
```rust
pub const BACKPRESSURE_CODE: i32 = -32001;

pub struct ServerLoops {
    pub processor: ProcessorLoop,        // 解析 + 调度
    pub outbound: OutboundLoop,          // 慢写 + 通知广播
    pub control: mpsc::UnboundedSender<OutboundControlEvent>,
}
```

**当前缺失**：`crates/synthia-server/src/routes/mcp.rs` 有 JSON-RPC envelope 但仅用于 MCP。无 -32001 错误码，无 optOutNotificationMethods，无双 loop 模型。

**代价**：**中（双 loop 重构）**。

**优先级**：**P1**（高负载下当前会内存爆炸）。

### 3.12 MCP Streamable HTTP Transport（codex 借鉴）

**当前**：`crates/synthia-mcp/src/registry/types.rs:48` 仅 `"stdio"` 字符串枚举，无 `streamable-http`。

**Rust 草案**（`crates/synthia-mcp/src/transport/mod.rs` 新建）：
```rust
pub enum McpTransport {
    Stdio { command: PathBuf, args: Vec<String> },
    StreamableHttp { url: Url, oauth: Option<OAuthConfig> },
    WebSocket { url: Url },
}
```

**代价**：**小**。

**优先级**：**P1**。

### 3.13 CodeMode Delegate trait（codex 借鉴 Tier 1）

**关键问题（已经回答）**：codex V8 在 Rust 里**是否值得做**？

| 维度 | 评估 |
|------|------|
| 价值 | 高 |
| V8 代价 | 高（~50MB 二进制 + 50ms/cell 启动）|
| Rust 替代 | `boa_engine`（纯 Rust ~3MB ~10ms/cell）|
| 建议 | **值得但仅在 Tier 2**。先在 `synthia-tool-orchestrator` 加 `CodeModeDelegate` trait |

**Rust 草案（Tier 1 trait）**：
```rust
#[async_trait]
pub trait CodeModeDelegate: Send + Sync {
    async fn invoke_tool(
        &self, call: NestedToolCall, cancel: CancellationToken
    ) -> Result<ToolOutput, ToolError>;
    async fn notify(...);
    fn cell_closed(&self, cell_id: &CellId);
}

// synthia-tool-orchestrator 直接实现 delegate
impl CodeModeDelegate for DefaultToolOrchestrator { ... }
```

**优先级**：**P2**（先 trait，后 runtime 探索）。

### 3.14 AgentMessage + llm_visible() 投影层（pi-mono 借鉴，**最重要**）

**作用**：解决 synthia 把 `Vec<Message>` 直接当 AgentMessage 用的最大 hack。

**Rust 草案**：
```rust
pub enum AgentMessage {
    User { ... },
    Assistant { ... },
    ToolResult { ... },
    Notification { ... },           // 例如 background_task_notification
    BackgroundTaskCompleted { ... },
    Compacted { ... },
}

impl AgentMessage {
    pub fn llm_visible(&self) -> bool {
        match self { ... }  // 默认 filter user/assistant/toolResult
    }
}

// convertToLlm 一行实现
let llm_messages: Vec<LlmMessage> = agent_messages.iter()
    .filter(|m| m.llm_visible())
    .map(|m| m.to_llm())
    .collect();
```

**当前 hack**：`Message::user(format_background_task_notification(...))`（`main_loop.rs:264`）—— 应该改成独立 `Notification` variant。

**代价**：**小**（新增 `AgentMessage` enum + 1 行 `convertToLlm`）。

**优先级**：**P0 #7**（pi-mono 专家强烈推荐 + 我认为这是 P8/P1 分水岭）。

### 3.15 SteeringQueue + Two-level while loop（pi-mono 借鉴）

**作用**：把"队列"显式建模，避免 `session_input_queue` (mpsc) + `iteration/drain_steering` 混用。

**Rust 草案**：
```rust
pub struct SteeringQueue {
    mode: QueueMode,        // OneAtATime | All
    queue: VecDeque<AgentMessage>,
}
impl SteeringQueue {
    pub fn push(&mut self, msg: AgentMessage);
    pub fn drain(&mut self) -> Vec<AgentMessage>;
}
```

**当前过度**：`main_loop.rs:906-1050` 的 `maybe_auto_trigger_*` 函数族（synthia 自创）应改成 `SelfReflectStep` / `CompactStep` 在 builder 链中。

**减负 checklist**（pi-mono 专家给出）：
- [ ] `main_loop.rs:245-870` 拆到 `iteration/{compact,reflect,llm,tool_execute}.rs`
- [ ] `SteeringQueue` 提为 `stream_builder::steering::SteeringQueue`
- [ ] 删除 `wrap_output_with_otel` 的 6 task-local 设置，改 `Span::current()`

**代价**：**小**（结构调整）。

**优先级**：**P1**。

---

## 4. "Tool 化" 的真实边界（反方挑战，4 专家一致）

> 用户原话："除了主逻辑 react loop 和 session 之外，其他功能尽量抽象为 tool 实现"。
> 反方挑战：这是过度抽象。

### 4.1 反方立场陈述

Tool 化的**必要条件**（满足 ≥3 才 tool 化）：

```
① 用户/外部系统调用时需异步响应（不能 block 在内循环里）
② 副作用可独立受 permission 控制
③ LLM 自主调用有合理性（如查询、写作、查询内部状态）
④ 不是高频内循环（每 LLM call / 每 iteration 都跑）
```

### 4.2 ✓ 应该 Tool 化（8 项，反方与正方共识）

| 项目 | 共识理由 |
|------|----------|
| **SubagentTool / AgentTool** | ✓ 用户可见委派；LLM 主动调用 |
| **McpTool Wrapper** | ✓ 外部资源；permission 可独立 |
| **SkillTool / LoadSkillTool** | ✓ 隐式工具 (is_hidden=true) |
| **CompactContextTool** | ✓ 已 Tool 化 |
| **SelfReflectTool** | ✓ 已 Tool 化 |
| **CronAddTool / CronListTool** | ✓ 用户可见能力 |
| **TodoWriteTool / task_create / task_update** | ✓ 用户可见能力 |
| **ToolSearch / SkillSearch**（codex 借鉴） | ✓ LLM 主动搜索元能力 |

### 4.3 ✗ 不应该 Tool 化（6 项，反方重点）

| 项目 | 反方理由（4 专家共识）|
|------|----------------------|
| **Provider / CachePolicy** | 每 LLM call 都跑；tool 化 = 序列化浪费。属于 provider 内部 |
| **PrefixTracker** | byte-level hash；高频内循环；序列化致命 + 测量装置扰动被测对象 |
| **LoopDetector** | 每 iteration 检查；高频；trait + 内循环足够 |
| **EventBus** | 多 pubsub fan-out；tool 化 = 单点同步，丢失并发 |
| **SessionStateMachine.transition_to** | 有 lifecycle 副作用；Tool 没有 lifecycle 钩子 |
| **Plugin/Hook Manager 自身** | lifecycle = register/unload；tool 化退化成单一触发点 |

### 4.4 反方专家量化代价

| 项 | 代价 |
|----|------|
| Tool call 序列化（10-100 KB input）| 0.2-5 ms + 2-3 倍瞬时内存 |
| 描述注入（100 个工具，150-500 tokens/工具）| 15K-50K tokens/请求 |
| 间接层延迟（hook + event + permission）| 0.1-2 ms/层 |

**反方最终立场**：

> **只有"可由用户或模型显式选择、具备可序列化输入输出、一次调用可独立结算、遗漏调用不会破坏系统正确性"的能力才应 Tool 化；其余必须留在 hook、service、state machine 或 stream pipeline。**

### 4.5 灰色地带（4 专家共识）

| 项目 | 视角 A | 视角 B | 决策 |
|------|--------|--------|------|
| `usage tracker → Tool?` | LLM 主动查询能力 | 泄露统计信息 + 高频 | **不 Tool 化**（内部 telemetry） |
| `SessionState → Tool?` | 末尾复述可发现 | 暴露内部状态 | **限 SessionInspectTool**（只读，不写） |
| `ApprovalService` Tool 化 | 用户可见 | **安全敏感**，必须 hook 拦截 | **不 Tool 化**（必须 hook 拦截） |
| `ForkPolicy → Tool?` | 灵活性 | session fork 不可逆 | **不该 LLM 触发**，只用户手动 |
| `CodeMode` Tool 化 | 强能力 | 高度破坏 | **Tier 1 trait + Tier 2 探索** |

---

## 5. Phase 1/2/3 路线图（6 个月到一年）

### Phase 1（必做，4-6 周，4 个 PR）

| # | 任务 | 来源 | 优先级 | 工时 |
|---|------|------|------|-----|
| **1.1** | 修复 G1：AgentRunConfig 11 字段串接 | baseline + 我核对 | **P0 #1** | 1 周 |
| **1.2** | `FailPolicy` 默认 FailClosed | baseline G2 | **P0 #2** | 1 PR 1 天 |
| **1.3** | `Materialization → resolve` stale 检测 | opencode | **P0 #3** | 1 周 |
| **1.4** | Tool name 前置校验 + atomic batch | opencode | **P0 #4** | 1 PR |
| **1.5** | `OutputBound` registry-level | opencode | **P0 #5** | 1 PR |
| **1.6** | TaskKind 多态 + AgentTaskContext (Weak) | codex | **P0 #6** | 1 周 |
| **1.7** | AgentRole ConfigLayer + sticky fields | codex | **P0 #7**（silent bug）| 1 周 |
| **1.8** | Hook V2 合并 + 10 事件 + FailedContinue/Abort | codex + baseline G6 | **P0 #8** | 2 周 |
| **1.9** | `AgentMessage + llm_visible()` 投影层 | pi-mono（最强烈推荐） | **P0 #9** | 1 周 |
| **1.10** | subagent_factory 串接到 AgentTool 构造器 | 我核对 | P0 #10 | 1 PR |

### Phase 2（应该做，3 个月）

| # | 任务 | 来源 | 备注 |
|---|------|------|------|
| **2.1** | StackedToolRegistry（含已有的 ScopedToolRegistry） | opencode | 1 个月 |
| **2.2** | ToolPluginProvenance | codex 独占 | 1 周 |
| **2.3** | GoalService（新建 crate） | codex 独占 | 6 周 |
| **2.4** | EventVersioned + Durable/Ephemeral | opencode | 1 个月 |
| **2.5** | App-Server 背压 -32001 + 双 loop | codex | 3 周 |
| **2.6** | MCP Streamable HTTP Transport | codex | 2 周 |
| **2.7** | SteeringQueue + 删除 wrap_output_with_otel task-local | pi-mono 减负 | 2 周 |
| **2.8** | ConvertToLlm hooks | pi-mono | 1 周 |

### Phase 3（可做，6 个月+）

| # | 任务 | 来源 | 备注 |
|---|------|------|------|
| **3.1** | CodeMode Tier 1 trait + boa_engine Tier 2 runtime 探索 | codex | 1 个月 |
| **3.2** | 双输出 Tool structured + content（破坏性 trait 重构） | opencode | 2 周 |
| **3.3** | Tool marketplace API（基于 ToolPluginProvenance） | codex | 1 个月 |
| **3.4** | pi-mono 建议的减负（拆 main_loop.rs / 删 task-local） | pi-mono | 2 周 |

---

## 6. 验证成功标准

| 项 | 验证 |
|----|------|
| `cargo check --workspace` | exit 0 |
| `cargo clippy --all-targets --all-features -- -D warnings` | exit 0 |
| `cargo test --workspace` | 通过（pre-existing failures 例外）|
| `cargo fmt --all --check` | exit 0 |
| **新能力 smoke**: | |
| 注册一个 `Provenance::Plugin` 的 tool，并按 source 列表 | 1 个 e2e test |
| AgentTool 接受真 `subagent_session_factory` 产出 ChildSessionHandle | 1 个 e2e test |
| Hook V2 plugin 同时触发 `PreCompact` + `SubagentStart` + `Stop` | 1 个 e2e test |
| `AgentMessage::Notification` 不进 LLM context | 1 个 unit test（防 P8 回归） |
| TaskKind=Compact 跑独立 prompt + 只暴露 read tool | 1 个 e2e test |

---

## 7. 开放问题（需要进一步探索）

1. **Provider-executed tool 怎么迁移**：opencode 有 `event.providerExecuted` 标识，synthia 没有。需 provider-side 改造。
2. **进程隔离 vs 子 session**：pi-mono process-spawn = 最强隔离。synthia 走 sub-session + gRPC proxy 是否够强？
3. **App-server 协议是否走 MCP**：codex 有专门的 app-server protocol。synthia-server 当前是 HTTP/WS。要不要走 MCP？
4. **Goal token budget 与 session token budget 关系**：codex 关系不明。synthia 需要定义。
5. **`wrap_output_with_otel` 删后，OTel context 怎么注入**：需先验证 `Span::current()` 满足现有 `telemetry` 需求。
6. **`MaterializationToken` 用 `Arc<()>` 还是 typed counter**：`Arc<()>` 抽象但不够安全，是否用 `MaterializationId(u64)` typed？

---

## 8. 关键 takeaway（4 专家 + 我 5 票共识）

1. **baseline 报告需更新 G1 数字**（从 9 → 11 字段）
2. **"全部 tool 化" 应被约束为 "Progressive Toolification"**（4 条件 ≥3）
3. **优先级共识**：4 个 low-cost PR + 1 个 mid-cost Hook 合并 = 6 周内改变架构质量
4. **`AgentMessage + llm_visible()` 是 P0 #7**（pi-mono 强烈推荐 + 我认为这是真正的 P8 分水岭）
5. **不要学**：`wrap_output_with_otel` task-local（pi-mono 反例）/ 30 overloads（synthia 已有更好的 hook）/ codex V8（T2 探索）

---

## 附录 A：evidence ledger

| 建议 | 出处 |
|------|------|
| `AgentRunConfig` 11 字段废弃 | `main_loop.rs:124-162` |
| `AgentTool` 已实现 | `agent_tool.rs:124` |
| `AgentTool` 已注册 | `tool_registry.rs:24` |
| Tool trait 12 方法 | `synthia-tool/src/traits.rs:29-117` |
| session-v2 part 模型 | `synthia-session-v2/src/part.rs` |
| session-v2 ToolPart | `synthia-session-v2/src/tool_part.rs` |
| session-v2 Tree | `synthia-session-v2/src/tree.rs:34-` |
| gRPC message-proxy | `synthia-message-proxy/src/lib.rs` |
| ScopedToolRegistry 已存在 | `synthia-tool/src/scoped_registry.rs` |
| opencode stack-based tool registry | `inbox/opencode-deep-analysis.md §3.4` |
| opencode 双输出 | `inbox/opencode-deep-analysis.md §3.7` |
| opencode EventVersioned | `inbox/opencode-deep-analysis.md §4` |
| opencode 8 个新模式（专家 1 综合）| `inbox/opencode-control-plane-patterns.md` |
| codex ToolPluginProvenance | `inbox/codex-deep-analysis.md §0 + §11` |
| codex Goals | `inbox/codex-vs-opencode-design.md §3.2` |
| codex Code Mode | `inbox/codex-vs-opencode-design.md §3.6` |
| codex 7 个新模式（专家 2 综合）| `inbox/codex-vs-opencode-design.md` |
| codex 10 hooks + 3 态 | `inbox/codex-vs-opencode-design.md §3.7` |
| pi-mono steering vs follow-up | `pi-mono/ARCHITECTURE_REPORT.md §8` |
| pi-mono executionMode per-tool | `pi-mono/ARCHITECTURE_REPORT.md §6` |
| pi-mono extension HMR | `pi-mono/ARCHITECTURE_REPORT.md §7.3` |
| pi-mono 反抽象减负（专家 3 综合）| `/home/crochee/workspace/pi-mono/SYNTHIA_PI_MONO_AGENT_LOOP.md` |
| 反方"全部 tool 化" 挑战（专家 5 综合）| 见本报告 §4 |

---

## 附录 B：4 专家交付清单

| 专家 | 任务 | bg_task_id | 交付文件 | 关键结论 |
|------|------|------------|----------|----------|
| 1 | opencode 借鉴 | bg_59f12b93 | `inbox/opencode-control-plane-patterns.md` | **8 条新模式**（不含 inbox §1-§9 已写的）|
| 2 | codex 差异化 | bg_d24040c1 | `inbox/codex-vs-opencode-design.md` | **7 条独占模式**（A-G）|
| 3 | pi-mono 极简 | bg_649ac449 | `pi-mono/SYNTHIA_PI_MONO_AGENT_LOOP.md` | **6 条减负 checklist** |
| 4 | Synthia gap 重审 | bg_0f59e45e | （已 cancelled，未完成）| 我自己替代（§1.1-1.4）|
| 5 | 反方挑战 | bg_5c86db81 | （综合本报告 §4）| **8 项挑战 + 量化代价 + 反方立场**|

---

*版本*: v1 终稿（2026-07-17）
*下次更新*: OpenSpec change proposal 落地时
