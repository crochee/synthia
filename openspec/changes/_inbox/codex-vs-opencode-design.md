# §3 差异化设计借鉴：codex 独有 vs opencode 缺失

> **本节范围**：聚焦 codex-rs **独有**而 opencode 没有、或 synthia 当前实现存在显著差距的架构模式。
> 不重复 §1（共识原则）和 §2（inbox 报告 §1-§12）。
> 每条标注：**codex 实现** → **opencode 对照** → **synthia 现状差距** → **优先级 + Rust 草案**。

---

## 3.1 借鉴目录（按优先级）

| # | 模式 | codex 唯一性 | synthia 差距 | 优先级 |
|---|------|------------|------------|--------|
| A | **GoalService**（Thread-scoped 单一目标 + token budget + Semaphore 锁） | 高 | 完全缺失 | **P1** |
| B | **TaskKind 多态 + SessionTaskContext**（Regular/Compact/Review/UserShell + 100ms 宽限中断 + MultiAgentVersion） | 高 | 弱类型（synthia-task 是通用 scheduler，无 turn 类型分类） | **P0** |
| C | **AgentRole = ConfigLayer**（角色即配置覆盖层 + provider/service_tier 粘性） | 中 | synthia-agent 直接传完整 Config，缺 layer 叠加 | **P0** |
| D | **ToolPluginProvenance + 5 种 ToolSpec**（core/plugin/mcp/context 来源分类 + Function/Freeform/Namespace/WebSearch/ToolSearch） | 高 | synthia-tool-orchestrator 全部一锅烩，无来源溯源 | **P1** |
| E | **CodeModeService Cell/Session/Delegate 三层**（V8 隔离 + 暂停/续期 + Promise↔oneshot 桥） | **极高** | 完全缺失 | **P2**（Tier 2 探索） |
| F | **Hook 10 事件 + FailedContinue/Abort 三态**（含 PreCompact/PostCompact + SubagentStart/Stop） | 中 | synthia-hook 仅 6 事件，缺 Compact/Subagent 边界 | **P0** |
| G | **App-Server 背压 `-32001` + optOutNotification + 双 loop** | 中 | synthia-server 有 init 但缺背压/通知抑制 | **P1** |

> **说明**：E（Code Mode）是 codex 最独有的设计，但 Rust 复刻 V8 代价高（参见 §3.7）；其余六条均有明确的低成本实现路径。

---

## 3.2 模式 A — GoalService（Thread-scoped 单一目标 + Token Budget）

### codex 实现
- `ext/goal/src/api.rs:75-200` —— `GoalService` 持有 `Mutex<HashMap<ThreadId, Weak<GoalRuntimeHandle>>>`；`set_thread_goal()` **先获取 `goal_state_permit`（`Semaphore(1)`）再 `prepare_external_goal_mutation()`**，最后才写 DB。
- `ext/goal/src/runtime.rs:49,101` —— `goal_state_lock: Semaphore`，保证同 thread 同目标 mutation 互斥；`Weak<GoalRuntimeHandle>` 让 thread 死亡时自动清理。
- `ext/goal/src/api.rs:42-46` —— `GoalTokenBudgetUpdate::Set(Option<i64>)` —— 每个目标独立 token 上限。
- `ext/goal/src/steering.rs` —— 每轮注入 `continuation_steering_item`（"你正在追求 goal X，progress Y/Z"）—— 类似 P5 末尾复述。

### opencode 对照
opencode 的 session 无"目标"概念，仅有 system prompt + user message 历史。

### synthia 现状
- `crates/synthia-session` 无 goal 字段
- `crates/synthia-agent` 无 steering 注入目标进度
- 无 token budget per goal（仅全局 `summary_max_tokens`）

### Rust 草案（`synthia-goal` 新 crate）

```rust
// crates/synthia-goal/src/lib.rs
pub struct GoalService {
    runtimes: StdMutex<HashMap<ThreadId, Weak<GoalRuntimeHandle>>>,
}

#[derive(Debug, Clone)]
pub struct GoalSetRequest<'a> {
    pub thread_id: ThreadId,
    pub objective: GoalObjectiveUpdate<'a>,  // Keep | Set(&str)
    pub status: Option<ThreadGoalStatus>,
    pub token_budget: GoalTokenBudgetUpdate, // Keep | Set(Option<i64>)
}

#[async_trait]
pub trait GoalRuntime: Send + Sync {
    async fn prepare_external_mutation(&self) -> Result<(), GoalError>;
    async fn apply_external_set(&self, prev: Option<PreviousGoal>) -> Result<(), GoalError>;
    /// Semaphore(1) 互斥 mutation
    async fn state_permit(&self) -> Result<SemaphorePermit, GoalError>;
    /// Steering item for prompt injection (P5 末尾复述变体)
    fn continuation_steering(&self) -> Option<SteeringItem>;
}

pub enum ThreadGoalStatus { Active, Completed, Abandoned }
```

**关键模仿**：mutation 顺序固定 `state_permit() → prepare_external_mutation() → DB.write()`，避免基于过期 state 启动 continuation。

---

## 3.3 模式 B — TaskKind 多态 + SessionTaskContext + 100ms 宽限中断

### codex 实现
- `core/src/tasks/mod.rs:1-65` —— 4 种 task：`CompactTask / RegularTask / ReviewTask / UserShellCommandTask`；`GRACEFULL_INTERRUPTION_TIMEOUT_MS: u64 = 100`。
- `core/src/tasks/mod.rs:68-89` —— `InterruptedTurnHistoryMarker { Disabled, ContextualUser, Developer }` + `MultiAgentVersion` 自动选择 marker 类型。
- `core/src/tasks/mod.rs:170-200` —— `SessionTaskContext` 细粒度能力暴露：`clone_session() / turn_extension_data() / auth_manager() / models_manager()` —— task 上下文**不是整个 Session**，避免子 agent 拿到不该有的能力。
- `core/src/tasks/mod.rs:815-816` —— 100ms 宽限后再强行终止。

### opencode 对照
opencode 仅 `session/lifecycle.ts` 单层；无 task kind 分类；中断直接 kill。

### synthia 现状
- `crates/synthia-task/src/types/task.rs` —— 通用 task 抽象（structured_output / progress），**无 turn 类型分类**
- `crates/synthia-agent/src/subagent/factory.rs` —— 子 agent 创建时**直接传完整 session 引用**，未做能力裁剪

### Rust 草案（增强 `synthia-task` + `synthia-agent`）

```rust
// crates/synthia-task/src/types/task.rs
pub enum TaskKind {
    Regular,         // 普通 LLM turn
    Compact,         // compaction turn（独立 prompt + 隐藏 tool）
    Review,          // review turn（只读 tool）
    UserShellCommand,// 用户 ! shell 命令
}

pub struct TaskInterruptPolicy {
    pub graceful_timeout_ms: u64,  // 默认 100
    pub history_marker: InterruptedTurnHistoryMarker,
}

// crates/synthia-agent/src/task/context.rs
pub struct AgentTaskContext {
    session: Weak<Session>,
    turn_extension_data: Arc<ExtensionData>,
    auth: Weak<AuthManager>,
    models: SharedModelsManager,
    // 故意不暴露：permission policy 写入能力、session lifecycle 控制器
}
impl AgentTaskContext {
    pub fn clone_session(&self) -> Option<Arc<Session>> { self.session.upgrade() }
}
```

**关键模仿**：`AgentTaskContext` 用 `Weak<Session>` 而非 `Arc<Session>`，子 agent 死亡不阻止 parent session 释放。

---

## 3.4 模式 C — AgentRole = ConfigLayer（角色即配置覆盖层 + 粘性 provider）

### codex 实现
- `core/src/agent/role.rs:130-200` —— `apply_role_to_config_inner()`：role 是 config layer（`role_layer(role_layer_toml.clone())`），**不是完整 Config 替换**。
- 关键代码（`role.rs:204-208`）：
  ```rust
  let preserve_current_provider = role_layer_toml.get("model_provider").is_none();
  let preserve_current_service_tier = role_layer_toml.get("service_tier").is_none();
  *config = reload::build_next_config(config, role_layer_toml, preserve_current_provider, preserve_current_service_tier).await?;
  ```
- `role.rs:91-110` —— built-in role 用 `include_str!` 嵌入；user-defined role 从 config.toml 加载。

### opencode 对照
opencode 的 agent 配置是直接替换（`packages/opencode/src/agent/agent.ts` 全量覆盖），无 layer 叠加。

### synthia 现状
- `crates/synthia-agent/src/config/agent_config.rs` —— agent 配置直接构建，无 layer 抽象
- 子 agent 创建时 provider/service_tier **会被 role config 覆盖**（潜在 bug：用户主 agent 是 Claude，子 agent config 写了 GPT，子 agent 静默切到 GPT）

### Rust 草案（`synthia-agent/src/config/layer.rs`）

```rust
// crates/synthia-agent/src/config/layer.rs
pub struct ConfigLayer {
    /// TOML 片段（覆盖 / 新增键）
    overrides: toml::Value,
    /// 强制粘性的字段（即使 layer 没设也不能改）
    pub sticky_fields: Vec<StickyField>,
}
pub enum StickyField { ModelProvider, ServiceTier, UserId }

pub fn apply_role_layer(
    base: &mut Config,
    role: &AgentRoleConfig,
) -> Result<(), ConfigError> {
    let layer_toml = role.config_file.as_ref()
        .map(|p| std::fs::read_to_string(p))
        .transpose()?
        .and_then(|s| s.parse::<toml::Value>().ok())
        .unwrap_or_default();

    // 关键：粘性字段不覆盖
    let preserve_provider = !layer_toml.get("model_provider").is_some();
    let preserve_tier    = !layer_toml.get("service_tier").is_some();

    base.merge_layer(&layer_toml, &[StickyField::ModelProvider, StickyField::ServiceTier][..preserve_provider as usize..])?;
    base.merge_layer(&layer_toml, &[StickyField::ServiceTier][..preserve_tier as usize..])?;
    Ok(())
}
```

**关键模仿**：`preserve_*` 布尔位（layer 缺失时才粘性，layer 显式设了就尊重）—— 这避免"role config 漏写 provider 就静默回退"。

---

## 3.5 模式 D — ToolPluginProvenance + 5 种 ToolSpec + deferred/discoverable 工具

### codex 实现
- `codex-mcp/src/lib.rs:24` —— `pub use mcp::ToolPluginProvenance` —— 每个工具标记 `core | plugin | mcp | context`。
- `core/src/tools/router.rs:39-45` —— `ToolRouterParams { mcp_tools, deferred_mcp_tools, discoverable_tools, extension_tool_executors, dynamic_tools }` —— 5 类来源、3 类时机（immediate / deferred / discoverable-on-search）。
- `tools/src/lib.rs:1-25` —— `ToolSpec { Function | Freeform | Namespace | WebSearch | ImageGeneration | ToolSearch }` —— 6 种 spec 形态（注意 `ToolSearch` 元工具）。
- `core/src/tools/router.rs:83-93` —— `tool_supports_parallel() / tool_waits_for_runtime_cancellation()` —— 每个工具有并行/取消元数据。
- `tools/src/code_mode.rs:8-51` —— `augment_tool_spec_for_code_mode()` —— 给工具 description 注入 code-mode 调用样例。

### opencode 对照
`packages/opencode/src/tool/registry.ts:217-239` —— 扁平 `[...builtin, ...custom]` 列表，**无 provenance / 无 deferred / 无 discoverable / 无 tool_search**。

### synthia 现状
- `crates/synthia-tool/src/traits.rs:19-67` —— `ExecutionMode { Parallel, Sequential }` 已实现（与 codex 的 `tool_supports_parallel` 对齐），但缺**取消语义**和**provenance**
- `crates/synthia-tool-orchestrator/src/lib.rs` —— 5 个字段：`active_calls / edit_conflict / permission / sandbox`，**无 mcp/extension/dynamic 分桶**，无 deferred 工具
- `crates/synthia-mcp/src/manager/` —— 已有 OAuth 和 hybrid manager，但**工具注册无 provenance 字段**
- `crates/synthia-mcp/src/registry/types.rs:48` —— 仅 `"stdio"` 字符串枚举，**无 streamable-http transport**

### Rust 草案（增强 `synthia-tool` + `synthia-mcp`）

```rust
// crates/synthia-tool/src/provenance.rs  (新)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ToolProvenance {
    Core,           // synthia 内置
    Plugin,         // synthia-plugin 加载
    Mcp { server: &'static str, host_owned: bool },  // ← host_owned 字段是 codex 独有
    Context,        // 上下文派生（如 todo / skill）
    Dynamic,        // 运行时注册
}

// crates/synthia-tool/src/spec.rs  (增强)
pub enum ToolSpec {
    Function(FunctionToolSpec),
    Freeform(FreeformToolSpec),
    Namespace { name: String, tools: Vec<FunctionToolSpec> },
    WebSearch { provider: SearchProvider },
    ToolSearch,    // 元工具：model 主动按关键字搜索工具
}

pub trait Tool: Send + Sync {
    // 已有字段省略
    fn provenance(&self) -> ToolProvenance { ToolProvenance::Core }
    fn is_deferred(&self) -> bool { false }        // 工具存在但需 tool_search 激活
    fn tool_spec(&self) -> ToolSpec { ToolSpec::Function(...) }
    fn cancel_behavior(&self) -> CancelBehavior { CancelBehavior::Interrupt }  // vs AwaitCompletion
}

// crates/synthia-tool-orchestrator/src/lib.rs  (重写 dispatch)
pub struct OrchestratorParams<'a> {
    pub mcp_tools: Option<Vec<McpToolInfo>>,
    pub deferred_mcp_tools: Option<Vec<McpToolInfo>>,
    pub discoverable_tools: Option<Vec<DiscoverableTool>>,
    pub extension_tools: Vec<Arc<dyn ExtensionToolExecutor>>,
    pub dynamic_tools: &'a [DynamicToolSpec],
}
```

### MCP 多传输草案（`synthia-mcp/src/transport/` 新子模块）

```rust
// crates/synthia-mcp/src/transport/mod.rs  (新)
pub enum McpTransport {
    Stdio { command: PathBuf, args: Vec<String> },
    StreamableHttp { url: Url, oauth: Option<OAuthConfig> },  // ← codex 独有
    WebSocket { url: Url },                                     // ← codex 独有（实验性）
}

pub struct McpServerConfig {
    pub name: String,
    pub transport: McpTransport,
    pub host_owned: bool,           // ← codex 独有（"host 平台自带"）
    pub required: bool,
    pub startup_timeout: Duration,
    pub cache_key_suffix: String,   // ← 含 user_id 维度（项目记忆硬约束）
}
```

---

## 3.6 模式 E — CodeModeService（V8 隔离 + Cell/Session/Delegate + 暂停/续期）

> **本节直接回答关键问题**：codex 的 Code Mode 在 Rust 里**是否值得做**？

### codex 实现（codex 独有性：**极高**，opencode 无此设计）
- `code-mode/src/service.rs:99-220` —— `CodeModeService` 三层抽象：
  - **Session**：跨 cell 共享 `stored_values: Mutex<HashMap<String, JsonValue>>`
  - **Cell**：每次 `execute()` 一个 V8 isolate + 一个控制 task
  - **Delegate**：由调用方实现 `invoke_tool / notify / cell_closed`
- `code-mode/src/service.rs:129-220` —— `execute() / execute_to_pending() / wait() / wait_to_pending()` 四种 cell 生命周期。
- `runtime/callbacks.rs:13-72` —— **V8 Promise ↔ Rust oneshot 桥**：工具调用返回 V8 Promise，Rust 端通过 `pending_tool_calls: HashMap<id, PromiseResolver>` 反向 resolve。
- `runtime/callbacks.rs:303-324` —— `yield_control_callback`（主动让出）+ `exit_callback`（受控退出）。
- `runtime/globals.rs:14-47` —— **默认拒绝**：显式删除 `console / Atomics / SharedArrayBuffer / WebAssembly`。

### 关键问题回答

| 维度 | 评估 |
|------|------|
| **价值** | 高。LLM 用代码（而非 JSON）编排工具能减少 round-trip + 允许闭包/局部变量 + 批量并行 |
| **V8 代价** | 高。`v8` crate 是 `deno_core`/`rquickjs`/`boa` 中最重的（~10MB 二进制，启动 ~50ms/cell） |
| **Rust 低成本替代** | 用 **`boa_engine`**（纯 Rust，~3MB，~10ms/cell）—— 代码量 80% 一致 |
| **是否值得做** | **值得但仅在 Tier 2**。先在 `synthia-tool-orchestrator` 加 **ToolPlan** trait（让 LLM 输出 DAG 计划），观察是否够用 |

### 借鉴策略：分层降级
- **Tier 1（必须）**：抽 `CodeModeSessionDelegate` trait（即使不实现 V8，让 `synthia-tool-orchestrator` 实现 delegate），**未来可平替运行时**。
- **Tier 2（探索）**：`synthia-code-mode` crate，runtime 用 `boa_engine`，protocol 镜像 `codex_code_mode_protocol`。
- **Tier 3（不推荐）**：V8 / `deno_core` —— 二进制 +50MB，与 synthia 轻量目标冲突。

### Rust 草案（Tier 1 trait）

```rust
// crates/synthia-tool/src/code_mode.rs  (新)
#[async_trait]
pub trait CodeModeDelegate: Send + Sync {
    async fn invoke_tool(
        &self,
        call: NestedToolCall,
        cancel: CancellationToken,
    ) -> Result<ToolOutput, ToolError>;

    async fn notify(
        &self,
        call_id: String,
        cell_id: CellId,
        text: String,
        cancel: CancellationToken,
    ) -> Result<(), ToolError>;

    fn cell_closed(&self, cell_id: &CellId);
}

// synthia-tool-orchestrator 直接实现 delegate
impl CodeModeDelegate for DefaultToolOrchestrator {
    async fn invoke_tool(&self, call: NestedToolCall, cancel: CancellationToken) -> Result<ToolOutput, ToolError> {
        // 直接调 self.dispatch(call) —— protocol 无关
        ...
    }
}
```

**关键模仿**：`Delegate` trait 让 `synthia-tool-orchestrator` **未来成为 CodeMode 的天然 backend**——orchestrator 是 protocol 无关的，正好是 delegate 应有的形态。

---

## 3.7 模式 F — Hook 10 事件 + FailedContinue/Abort 三态

### codex 实现
- `hooks/src/lib.rs:19-30` —— `HOOK_EVENT_NAMES = ["PreToolUse", "PermissionRequest", "PostToolUse", "PreCompact", "PostCompact", "SessionStart", "UserPromptSubmit", "SubagentStart", "SubagentStop", "Stop"]`。
- `hooks/src/lib.rs:32-46` —— 8 个事件有 matcher（`Stop` / `UserPromptSubmit` 无 matcher，因为它们不针对具体触发器）。
- `hooks/src/types.rs:14-30` —— `HookResult { Success, FailedContinue(Box<dyn Error>), FailedAbort(Box<dyn Error>) }` + `should_abort_operation()` —— **三态语义**。
- `hooks/src/registry.rs:600-612` —— 短路：第一个 `FailedAbort` 终止后续 hook。

### opencode 对照
opencode 的 plugin hook 是 dynamic function（`packages/opencode/src/plugin/index.ts`），**仅 `tool.definition` 等少数事件**，无 SessionStart/Compact/Subagent 边界事件。

### synthia 现状
- `crates/synthia-hook/src/registry/fire.rs:27-153` —— 6 个事件：`before_llm / after_llm / before_tool / after_tool / iteration_end / complete`，**无 Compact / Subagent / SessionStart**。
- `traits.rs:79` —— `FailPolicy` enum（已三态雏形），但 `fire.rs` 实际是 panic-isolation `catch_unwind`，**没有 Continue/Abort 区分**。

### Rust 草案（增强 `synthia-hook`）

```rust
// crates/synthia-hook/src/events.rs  (新)
pub enum HookEvent {
    PreToolUse,
    PermissionRequest,
    PostToolUse,
    PreCompact,           // ← 新
    PostCompact,          // ← 新
    SessionStart,         // ← 新
    UserPromptSubmit,     // ← 新
    SubagentStart,        // ← 新
    SubagentStop,         // ← 新
    Stop,                 // ← 新
}

pub enum HookMatcher {
    ToolName(glob::Pattern),
    AgentName(glob::Pattern),
    CompactTrigger(CompactTrigger),
}

// crates/synthia-hook/src/types.rs  (改)
pub enum HookResult {
    Success,
    FailedContinue(Box<dyn Error>),  // 继续后续 hook + 工具调用
    FailedAbort(Box<dyn Error>),     // 终止后续 hook + 标记 turn 失败
}
impl HookResult {
    pub fn should_abort(&self) -> bool { matches!(self, Self::FailedAbort(_)) }
}

// crates/synthia-hook/src/registry/fire.rs  (改)
for hook in hooks {
    let outcome = hook.execute(&payload).await;
    if outcome.result.should_abort() { break; }  // 短路
    outcomes.push(outcome);
}
```

---

## 3.8 模式 G — App-Server 背压 + optOutNotification + 双 loop

### codex 实现
- `app-server/README.md:49-53` —— 背压：请求入队饱和时返回 `-32001 Server overloaded; retry later.`
- `app-server/README.md:87` —— `capabilities.optOutNotificationMethods` 按连接粒度精确匹配（无通配符）抑制通知。
- `app-server/README.md:74-129` —— `initialize` handshake：必须先调，重复返回 `Already initialized`；`clientInfo.name` 实名制。
- `app-server/src/lib.rs:139-200` —— **双 loop 模型**：`processor`（解析请求） + `outbound`（慢写），通过 `OutboundControlEvent` 协调，避免共享 mutable state。

### opencode 对照
opencode 是 plain HTTP/WS（`packages/opencode/src/server/`），**无 JSON-RPC 2.0 协议层**，无背压语义。

### synthia 现状
- `crates/synthia-server/src/routes/mcp.rs:12-97` —— 有 JSON-RPC 2.0 envelope（`"jsonrpc": "2.0"`），有 `initialize` 方法，**但仅用于 MCP，不用于 server 自身**
- 无 `-32001` 错误码
- 无 `optOutNotificationMethods`
- 单 loop 模型（axum default），无 processor/outbound 分离

### Rust 草案（增强 `synthia-server`）

```rust
// crates/synthia-server/src/jsonrpc.rs  (新)
pub const BACKPRESSURE_CODE: i32 = -32001;  // "Server overloaded; retry later"
pub const BACKPRESSURE_MSG: &str = "request queue saturated; retry with exponential backoff";

pub struct JsonRpcServerConfig {
    pub max_inflight: usize,           // 默认 64，超过返回 -32001
    pub max_queue_depth: usize,        // 默认 256
    pub notification_suppression: HashSet<String>,  // exact match
}

// crates/synthia-server/src/loop_pair.rs  (新)
pub struct ServerLoops {
    pub processor: ProcessorLoop,        // 解析 + 调度
    pub outbound: OutboundLoop,          // 慢写 + 通知广播
    pub control: mpsc::UnboundedSender<OutboundControlEvent>,
}

pub enum OutboundControlEvent {
    Opened { conn_id: u64, writer: Box<dyn AsyncWrite>, disconnect: oneshot::Sender<()>, initialized: bool, suppressed_notifications: HashSet<String> },
    Closed { conn_id: u64 },
    DisconnectAll,
}
```

**关键模仿**：`processor` 处理快路径（解析+调度），`outbound` 处理慢路径（写 socket + 通知广播），通过 `OutboundControlEvent` 协调 —— 避免在请求处理中 await I/O 阻塞后续请求。

---

## 3.9 借鉴优先级总表

| 模式 | codex 唯一性 | synthia 差距 | 工作量 | 优先级 | 理由 |
|------|------------|------------|------|------|------|
| B TaskKind | 高 | 高（无 turn 分类） | 中（增强 synthia-task） | **P0** | 子 agent 当前无 kind 区分，compaction/review 混入 regular turn |
| C AgentRole=ConfigLayer | 中 | 高（直接覆盖） | 小（加 layer 模块） | **P0** | 静默回退是真实 bug（user main=Claude, sub=GPT） |
| F Hook 10 事件 | 中 | 中（6→10） | 小（加 4 个 fire 方法） | **P0** | Compact/Subagent 边界当前无 hook 点，调试困难 |
| D ToolProvenance | 高 | 中（无分类） | 小（加 enum + ToolRouterParams） | **P1** | 关键为 mcp/host 区分，未来 plugin 安全审计需要 |
| G App-Server 背压/通知 | 中 | 中（无背压） | 中（双 loop 重构） | **P1** | 高负载下当前会内存爆炸 |
| A GoalService | 高 | 高（完全缺失） | 大（新 crate） | **P1** | "目标驱动"是 LLM 决策框架升级，价值高但成本大 |
| E CodeMode | **极高** | 高（完全缺失） | 极大（V8/boa + protocol） | **P2** | Tier 1 trait 可先落（成本小），Tier 2 runtime 探索性 |

---

## 3.10 落地建议（5 步法）

1. **第 1 步（P0，1 周）**：B + C + F —— 三个都是改 `synthia-agent` / `synthia-task` / `synthia-hook`，**不需要新 crate**，一次 PR。
2. **第 2 步（P1，1 周）**：D —— `synthia-mcp/src/provenance.rs` + `synthia-tool-orchestrator` 重构 `dispatch()` 参数化。
3. **第 3 步（P1，1 周）**：G —— `synthia-server/src/jsonrpc.rs` + 双 loop 模型。
4. **第 4 步（P1，2 周）**：A —— 新建 `synthia-goal` crate，集成进 `synthia-agent` steering。
5. **第 5 步（P2 探索，1 月）**：E —— 先做 Tier 1 trait，验证 `synthia-tool-orchestrator` 作为 delegate 可行，再决定是否做 Tier 2 runtime。

---

## 3.11 反例（不要学的 codex 设计）

> 出于诚实，也列出 codex **过度工程化**、synthia **不应模仿**的部分：

1. **V8 isolates per cell** —— `deno_core` 二进制 +50MB，启动 ~50ms/cell，synthia 走 boa 或不上。
2. **`InterruptedTurnHistoryMarker` 枚举 + `MultiAgentVersion`** —— codex v1/v2 双实现是为了迁移历史，synthia 还没这个负担，**直接定 v2 形态即可**。
3. **`OptOutNotificationMethods` 精确匹配无通配符** —— codex 的合规约束决定，synthia 可用 glob 但需文档明示。
4. **`CLAUDE.md` hooks JSON schema 兼容** —— codex 为 Claude Code 用户迁移成本低，synthia 应**定义自己的 hooks 协议**（P10 自治）。
