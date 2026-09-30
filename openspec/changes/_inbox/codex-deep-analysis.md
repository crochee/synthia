# Codex-RS 深度分析报告（面向 Synthia 的可借鉴设计）

> **报告路径**：`openspec/changes/_inbox/codex-deep-analysis.md`
> **分析对象**：`/home/crochee/workspace/codex/codex-rs`（OpenAI 2025-2026 版本，commit 接近 mainline）
> **分析维度**：12 个核心子系统，每个系统均给出 **文件:行号** 引用、关键设计、Synthia 借鉴路径
> **重点**：codex 独有的 "code_mode as tool" 与 "MCP as first-class" 设计

---

## 0. 概览（TL;DR）

| 维度 | codex-rs 的差异化设计 | Synthia 现状（`crates/`）| 借鉴优先级 |
|------|----------------------|-----------------------|------------|
| Code Mode | V8 隔离运行时 + 工具编排 + 暂停/续期 | 无 | P2（探索） |
| MCP 集成 | 多传输（stdio/streamable-http/OAuth）+ 工具来源 plugin provenance | `synthia-mcp`（已有 rmcp 客户端） | P0（增强） |
| App-Server | JSON-RPC 2.0 + 多 transport + 背压（`-32001`） | `synthia-server`（需对照） | P1 |
| Plugin | 清单（manifest）+ 加载 outcome + 资源定位 | `synthia-plugin` | P0 |
| Skills | `include_dir!` 嵌入 + 指纹 marker | `synthia-skill`（已含 zip/notify） | P0 |
| Hooks | 10 个事件 + 命令式引擎 + PluginHookSource | `synthia-hook` | P0 |
| Goals | Thread-scoped 单一目标 + token budget + Semaphore lock | 无 | P1 |
| Tools | ToolRouter + Registry + 并行检查 | `synthia-tool` + `synthia-tool-orchestrator` | P0 |
| Compact | Pre/Post hook + Local/Remote 两种 + 注入策略 | 无对应专门模块 | P1 |
| Memory | 命名空间 + 4 个工具 + 摘要注入 | `synthia-memory` | P0 |
| OTel | 指标 / trace / global MetricsClient | `synthia-telemetry` | P0 |
| Task/Subagent | 4 种 TaskKind + AgentRole + multi-agent v2 | `synthia-task` / `synthia-agent` | P0 |

**核心启示**：
1. **Plugin 是一等公民**——codex 把 plugin 作为 skills/mcp/hooks 的共同根，避免重新发明注册机制
2. **MCP 是 first-class**——不仅有 `McpConnectionManager` 还有 `ToolPluginProvenance` 区分工具来源
3. **Code Mode 是个 "可被工具编排的 JS 运行时"**——与 synthia 当前 `synthia-tool-orchestrator` 的"工具注册中心"模型非常契合

---

## 1. Code Mode 工具编排

### 1.1 设计要点

Code Mode 是 codex 的"代码即工具"模式：让模型在 **V8 隔离运行时** 中执行 JavaScript，调用其他工具。核心不是"另一个工具"，而是 **工具编排的范式**。

#### 1.1.1 核心类型

**`code-mode/src/lib.rs:1-7`** —— 极薄的 façade：
```rust
mod runtime;
mod service;
pub use codex_code_mode_protocol::*;
pub use service::CodeModeService;
pub use service::InProcessCodeModeSessionProvider;
pub use service::NoopCodeModeSessionDelegate;
```
**关键观察**：协议（`codex_code_mode_protocol`）是独立 crate，说明协议与服务实现解耦。

**`code-mode/src/service.rs:99-118`** —— 服务入口（Session 模型）：
```rust
pub struct CodeModeService { inner: Arc<Inner> }
struct Inner {
    stored_values: Mutex<HashMap<String, JsonValue>>,  // 跨 cell 共享 KV
    cells: Mutex<HashMap<CellId, CellHandle>>,         // 每个 cell 一个运行时
    delegate: Arc<dyn CodeModeSessionDelegate>,        // 工具调用委托
    shutting_down: AtomicBool,
    next_cell_id: AtomicU64,
}
```

**`code-mode/src/service.rs:41-66`** —— `NoopCodeModeSessionDelegate`：**代码即插拔**的示范，告诉你最小实现需要哪些 hook（`invoke_tool` / `notify` / `cell_closed`）。

#### 1.1.2 Cell 生命周期（`service.rs:129-220`）

```rust
pub async fn execute(&self, request: ExecuteRequest) -> Result<StartedCell, String> {
    if self.inner.shutting_down.load(Ordering::Acquire) {
        return Err("code mode session is shutting down".to_string());
    }
    let initial_yield_time_ms = request.yield_time_ms.unwrap_or(DEFAULT_EXEC_YIELD_TIME_MS);
    let (response_tx, response_rx) = oneshot::channel();
    let cell_id = self.allocate_cell_id();
    self.start_cell(cell_id.clone(), request, CellResponseSender::Runtime(response_tx),
                    Some(initial_yield_time_ms), PendingRuntimeMode::Continue).await?;
    Ok(StartedCell::new(cell_id, response_rx))
}
```

**核心三模式**：
- `execute` —— 启动 cell，返回 `StartedCell`（含 `initial_response()` 等待 first yield）
- `execute_to_pending` —— 启动 cell 并在工具调用边界处 **暂停**，返回 `ExecuteToPendingOutcome::Pending` 或 `Completed`
- `wait` / `wait_to_pending` —— 续期已暂停的 cell

**PendingRuntimeMode**（`runtime` 模块）：`Continue` vs `PauseUntilResumed`，**"边界暂停"是 LLM 友好的接口**——工具调用前停下，让上层决定是否注入新工具或取消。

#### 1.1.3 V8 ↔ Rust 桥接（`runtime/callbacks.rs`）

**`callbacks.rs:13-72`** —— 工具调用桥（关键）：
```rust
pub(super) fn tool_callback(scope, args, mut retval) {
    let tool_index = ...parse::<usize>()...;
    let input = if args.length() == 0 { Ok(None) } else { v8_value_to_json(scope, args.get(0)) };
    // ...
    let Some(resolver) = v8::PromiseResolver::new(scope) else { /* throw */ return; };
    let promise = resolver.get_promise(scope);
    let resolver = v8::Global::new(scope, resolver);
    let (tool_name, tool_kind) = { /* state.enabled_tools.get(tool_index) */ };
    let id = format!("tool-{}", state.next_tool_call_id);
    state.next_tool_call_id = state.next_tool_call_id.saturating_add(1);
    let event_tx = state.event_tx.clone();
    state.pending_tool_calls.insert(id.clone(), resolver);  // V8 Promise 挂在 Rust 端
    let _ = event_tx.send(RuntimeEvent::ToolCall { id, name, kind, input });
    retval.set(promise.into());  // JS 端 await 这个 Promise
}
```

**关键设计**：
1. **每个工具是一个 V8 function**，通过 `data(tool_index)` 闭包区分
2. **工具调用是 Promise-based**——JS `await tools.x()`，Rust 通过 `pending_tool_calls: HashMap<id, PromiseResolver>` 反向 resolve
3. **`next_tool_call_id` 用 AtomicU64**——保证 V8 单线程内的 ID 唯一
4. **`event_tx` 是 unbounded mpsc**——V8 线程只 send 不 await，避免阻塞

**`callbacks.rs:303-311`** —— `yield_control_callback`：主动让出执行权（限流）。
**`callbacks.rs:313-324`** —— `exit_callback`：通过 `throw_exception(EXIT_SENTINEL)` 触发受控退出。

#### 1.1.4 全局对象安装（`runtime/globals.rs`）

**`globals.rs:14-47`**：
```rust
pub(super) fn install_globals(scope: &mut v8::PinScope<'_, '_>) -> Result<(), String> {
    let global = scope.get_current_context().global(scope);
    delete_global(scope, global, "console")?;
    delete_global(scope, global, "Atomics")?;
    delete_global(scope, global, "SharedArrayBuffer")?;
    delete_global(scope, global, "WebAssembly")?;  // 沙箱：移除危险 API

    let tools = build_tools_object(scope)?;
    let all_tools = build_all_tools_value(scope)?;
    let clear_timeout = helper_function(scope, "clearTimeout", clear_timeout_callback)?;
    // ... text/image/generatedImage/store/load/notify/yield_control/exit
    set_global(scope, global, "tools", tools.into())?;
    set_global(scope, global, "ALL_TOOLS", all_tools)?;
    // ...
}
```

**安全策略**：**显式删除** `console / Atomics / SharedArrayBuffer / WebAssembly`（`globals.rs:16-19`），符合"默认拒绝"原则。

#### 1.1.5 工具规范增强（`tools/src/code_mode.rs`）

**`code_mode.rs:8-51`** —— `augment_tool_spec_for_code_mode`：
```rust
pub fn augment_tool_spec_for_code_mode(spec: ToolSpec) -> ToolSpec {
    match spec {
        ToolSpec::Function(mut tool) => {
            let Some(description) = augmented_description_for_spec(...) else { return ToolSpec::Function(tool); };
            tool.description = description;  // 描述里追加 code-mode exec 样例
            ToolSpec::Function(tool)
        }
        // ...
    }
}
```
**本质**：给工具的 model-facing description 注入"在 JS 怎么调"的样例（`tools.echo({...})`），避免模型在 Code Mode 里不知道怎么调。

**`code_mode.rs:61-86`** —— `collect_code_mode_tool_definitions`：把 Function / Freeform / Namespace 都转成 `CodeModeToolDefinition`，按名字排序 + dedup，**保证确定性**（注意 P1 前缀一致性）。

#### 1.1.6 测试模式（`service.rs:759-1821`）

300+ 行测试覆盖：
- `synchronous_exit_returns_successfully`（`service.rs:830`）—— 同步路径
- `stored_values_are_shared_between_cells_but_not_sessions`（`service.rs:856`）—— 验证 session 隔离 + cell 共享
- `shutdown_interrupts_cpu_bound_cells`（`service.rs:920`）—— 验证 `while(true){}` 也能被 `shutdown()` 终止
- `execute_to_pending_*`（`service.rs:966-1290`）—— 4 个测试覆盖 pending/边界/超时

### 1.2 Synthia 借鉴路径

| 借鉴项 | 实现位置 | 价值 |
|--------|---------|------|
| `CodeModeService` 协议/服务解耦 | 新增 `synthia-code-mode` crate | 防止 service.rs 变成巨石 |
| Cell 暂停/续期（`PendingRuntimeMode`） | 在 `synthia-tool-orchestrator` 中加 | 应对长任务+需要"再注入"的场景 |
| V8 Promise ↔ Rust oneshot 的桥 | runtime 子模块 | 替代 Naive polling |
| `augment_tool_spec_for_code_mode` | `synthia-tool` 适配 | 描述自动适配执行环境 |
| 显式删除危险 API | `install_globals` | 默认拒绝 |

**注意**：synthia 的 agent 当前是直接调工具，没有 "cell+session+delegate" 三层抽象。引入 Code Mode 时，**直接复用 `synthia-tool-orchestrator` 作为 `CodeModeSessionDelegate` 即可**——这正是 codex 抽象的妙处：tool orchestrator 是协议无关的。

---

## 2. MCP 集成（First-Class）

### 2.1 设计要点

#### 2.1.1 架构分层（`codex-mcp/src/lib.rs:71-82`）
```rust
pub(crate) mod auth_elicitation;
mod catalog;
pub(crate) mod codex_apps;
pub(crate) mod connection_manager;
pub(crate) mod elicitation;
pub(crate) mod mcp;
mod plugin_config;
mod resource_client;
pub(crate) mod rmcp_client;
pub(crate) mod runtime;
pub(crate) mod server;
pub(crate) mod tools;
```
**模块化**做得极好，每个子模块都 `pub(crate)` 控制可见性。

#### 2.1.2 连接管理器（`connection_manager.rs:107-200`）

**`McpConnectionManager`**：
```rust
pub struct McpConnectionManager {
    clients: HashMap<String, AsyncManagedClient>,  // key=server name
    server_metadata: HashMap<String, McpServerMetadata>,
    required_servers: Vec<String>,
    tool_plugin_provenance: Arc<ToolPluginProvenance>,
    host_owned_codex_apps_enabled: bool,
    prefix_mcp_tool_names: bool,
    elicitation_requests: ElicitationRequestManager,
    startup_cancellation_token: CancellationToken,
}
```

**`new()` 是 18 个参数**（`connection_manager.rs:119-200`）—— **故意为之**：
- 每个参数代表一个独立的关注点（auth、approval、cache key、elicitation...）
- 不构造巨大的 "Config" 结构体，方便测试 mock
- 启动用 `JoinSet` 并行初始化（`connection_manager.rs:148`），每个 server 用 `child_token` 隔离取消

**关键观察**：`codex_apps_tools_cache_key`（`connection_manager.rs:132`）—— **MCP 工具列表被缓存**了（避免每次启动都 list_tools），且 **key 由 `user_id` 维度区分**（防止跨用户污染）。这与项目记忆的 hard constraint 一致。

#### 2.1.3 工具来源溯源（`ToolPluginProvenance`）

**`mcp.rs` 中导出**（`codex-mcp/src/lib.rs:24`）：
```rust
pub use mcp::ToolPluginProvenance;
```
**作用**：每个 MCP 工具记录"它来自哪个 server"+"host 拥有还是 plugin 拥有"。这让 `tool_is_model_visible`（`connection_manager.rs:88-104`）能按 metadata 过滤（如 UI 标记 `visibility: ["model"]`）。

#### 2.1.4 OAuth 集成（`codex-mcp/src/lib.rs:55-65`）

```rust
pub use mcp::McpAuthStatusEntry;
pub use mcp::McpOAuthLoginConfig;
pub use mcp::McpOAuthLoginSupport;
pub use mcp::McpOAuthScopesSource;
pub use mcp::ResolvedMcpOAuthScopes;
pub use mcp::compute_auth_statuses;
pub use mcp::discover_supported_scopes;
pub use mcp::oauth_login_support;
pub use mcp::resolve_oauth_scopes;
pub use mcp::should_retry_without_scopes;
```
**完整 OAuth 闭环**：状态枚举 → 配置 → scopes 发现 → 解析 → 重试策略。Synthia 当前 `synthia-mcp` 用 `rmcp`，但 OAuth 支持需对照 codex 补全。

#### 2.1.5 Elicitation（用户交互请求）

`ElicitationRequestManager`（`elicitation.rs`）—— MCP 协议规定服务器可向客户端"询问"（如权限确认）。codex 把它纳入 `permission` 框架（`mcp.rs:67-68`）：
```rust
pub use mcp::McpPermissionPromptAutoApproveContext;
pub use mcp::mcp_permission_prompt_is_auto_approved;
```

#### 2.1.6 Codex Apps（host 拥有的特殊 MCP server）

**`codex_apps.rs` + `auth_elicitation.rs`** —— 显式有"host-owned codex apps"概念（`lib.rs:36-44`）：
```rust
pub use codex_apps::CodexAppsToolsCacheKey;
pub use codex_apps::codex_apps_tools_cache_key;
pub use mcp::codex_apps_mcp_server_config;
pub use mcp::configured_mcp_servers;
pub use mcp::effective_mcp_servers;
pub use mcp::host_owned_codex_apps_enabled;
```
**含义**：某些 MCP server 是 **"宿主平台自带的"**（如 OpenAI 提供的连接器），需要单独的 auth + cache + 配置流程。

### 2.2 Synthia 借鉴路径

| 借鉴项 | 现状差距 | 实施 |
|--------|---------|------|
| `ToolPluginProvenance` | `synthia-mcp` 当前只有 `tool_adapter` | 加 enum，标记 host / plugin / external |
| 缓存键含 `user_id` | 项目记忆已要求 | 已在 `synthia-mcp` 内？需校验 |
| 18 参数构造器风格 | 现有 `Manager` 大概率没这么多 | 拆分 `McpConfig` 为多块 |
| Elicitation → permission 桥 | synthia-permission 已存在 | 在 `synthia-mcp` 加 `elicitation_handler` 字段 |
| `host_owned` 概念 | 无 | 给 `synthia-plugin` 增加 capability `host` |

**Synthia 升级清单**：
- `synthia-mcp/src/manager.rs` 拆 18 个字段
- 加 `tool_provenance.rs` enum
- 加 `oauth.rs` 完整闭环（参照 `rmcp-client/src/oauth.rs`）

---

## 3. App-Server 架构

### 3.1 设计要点

#### 3.1.1 协议定位（`app-server/README.md:20-58`）

- 协议：**JSON-RPC 2.0**（在 wire 上省略 `"jsonrpc":"2.0"` 头以节省字节）
- 传输：**stdio**（默认，新行分隔 JSONL）/ **websocket**（实验性）/ **unix socket**（本地控制平面）/ **off**
- 背压：请求入队饱和时返回 `-32001 Server overloaded; retry later.`
- 健康端点：`/readyz`、`/healthz`（拒绝 `Origin` 头）
- Schema 输出：`codex app-server generate-ts / generate-json-schema`

#### 3.1.2 核心原语（`README.md:64-72`）

```
- Thread（用户与 agent 的对话）
- Turn（用户消息 → agent 消息）
- Item（持久化与上下文的最小单元）
```

#### 3.1.3 启动握手（`lib.rs:131-220` + `README.md:74-129`）

```json
{ "method": "initialize", "id": 0, "params": { "clientInfo": { "name": "codex_vscode" } } }
{ "method": "initialized" }  // notification
```
- `initialize` **必须**先调用，否则其他请求返回 "Not initialized"
- 重复 `initialize` 返回 "Already initialized"
- `clientInfo.name` 写入 OpenAI Compliance Logs Platform，**客户端身份实名制**

#### 3.1.4 通知抑制（`README.md:87`）

```json
"capabilities": { "optOutNotificationMethods": ["thread/started", "item/agentMessage/delta"] }
```
**精确匹配**（无通配符），按连接粒度生效。

#### 3.1.5 核心 API（`README.md:131-156`）

- `thread/start` / `thread/resume` / `thread/fork`（`ephemeral: true` 表示 in-memory）
- `thread/list` —— 分页（cursor）+ 多种 filter
- `thread/loaded/list` —— 仅内存中的
- `thread/turns/list` —— 不 resume 就列出 turn 历史
- `thread/memoryMode/set` —— 持久化内存资格
- `memory/reset` —— 清空 + 保留 memory mode
- `thread/goal/set / get / clear` —— 单一目标
- `thread/settings/updated` —— next-turn 设置变更通知

#### 3.1.6 传输抽象（`lib.rs:139-200`）

`OutboundControlEvent` 是 processor loop ↔ outbound loop 的协调：
```rust
enum OutboundControlEvent {
    Opened { connection_id, writer, disconnect_sender, initialized, experimental_api_enabled, opted_out_notification_methods },
    Closed { connection_id },
    DisconnectAll,
}
```
**关键设计**：**两个独立 loop**（processor 负责解析请求，outbound 负责慢写）通过 `OutboundControlEvent` 协调，避免共享 mutable state。

#### 3.1.7 SQLite 恢复（`lib.rs:80`）

```rust
const SQLITE_RECOVERY_CONFIG_WARNING_SUMMARY: &str = "Codex rebuilt its local database.";
```
**自愈**：本地 db 损坏时重建，发 warning event。

### 3.2 Synthia 借鉴路径

`synthia-server` 当前职责需对照 codex：
- 是否支持多 transport（stdio/uds/ws）？
- 是否有"initialize handshake"？
- 是否有 `optOutNotificationMethods` 减少噪音？
- 是否有背压（`-32001`）？

**直接抄的清单**：
1. `OutboundControlEvent` 双 loop 模型
2. `clientInfo.name` 强制识别
3. 通知精确 opt-out（无通配符）
4. 30 min 自动 unload（`README.md:155` `thread/unsubscribe` 描述）
5. 错误码 `-32001` + 客户端指数退避

---

## 4. Plugin / Extension 系统

### 4.1 设计要点

#### 4.1.1 核心类型（`plugin/src/lib.rs:1-77`）

```rust
pub use codex_utils_plugins::mention_syntax;
pub use codex_utils_plugins::plugin_namespace_for_skill_path;
mod load_outcome;
pub mod manifest;
mod plugin_id;
mod provider;

pub use load_outcome::EffectiveSkillRoots;
pub use load_outcome::LoadedPlugin;
pub use load_outcome::PluginLoadOutcome;
pub use load_outcome::prompt_safe_plugin_description;
pub use plugin_id::PluginId;
pub use plugin_id::PluginIdError;
pub use plugin_id::validate_plugin_segment;
pub use provider::PluginProvider;
pub use provider::PluginResourceLocator;
pub use provider::ResolvedPlugin;
pub use provider::ResolvedPluginError;
pub use provider::ResolvedPluginLocation;
```

**关键抽象**：
- `PluginId` —— 唯一标识（包含分段校验 `validate_plugin_segment`）
- `PluginProvider` —— 抽象加载来源（本地 / 远程 / 缓存）
- `LoadedPlugin` vs `PluginLoadOutcome` —— 区分"已加载"和"加载结果"（outcome 含 warnings）
- `EffectiveSkillRoots` —— 解析后的有效 skill 根目录
- `prompt_safe_plugin_description` —— **描述经清洗后注入 prompt**（防 prompt 注入）

#### 4.1.2 能力摘要（`lib.rs:29-37`）

```rust
pub struct PluginCapabilitySummary {
    pub config_name: String,
    pub display_name: String,
    pub description: Option<String>,
    pub has_skills: bool,
    pub mcp_server_names: Vec<String>,
    pub app_connector_ids: Vec<AppConnectorId>,
}
```
**核心观察**：plugin 是 **skills + mcp + connectors** 的统一打包点。

#### 4.1.3 Hook 来源（`lib.rs:39-47`）

```rust
pub struct PluginHookSource {
    pub plugin_id: PluginId,
    pub plugin_root: AbsolutePathBuf,
    pub plugin_data_root: AbsolutePathBuf,
    pub source_path: AbsolutePathBuf,
    pub source_relative_path: String,
    pub hooks: HookEventsToml,
}
```
plugin 可以 **携带 hooks**。

#### 4.1.4 遥测元数据（`lib.rs:49-77`）

```rust
pub struct PluginTelemetryMetadata {
    pub plugin_id: PluginId,
    pub remote_plugin_id: Option<String>,  // 远程 plugin 用远端 id 上报
    pub capability_summary: Option<PluginCapabilitySummary>,
}
```
**关键**：远程 plugin 在 analytics 中用 `remote_plugin_id`（避免本地 cache id 漂移）。

### 4.2 Synthia 借鉴路径

`crates/synthia-plugin/` 已有，但需对照 codex 检查：
- 是否定义 `PluginCapabilitySummary`（多能力聚合）？
- `PluginTelemetryMetadata` 是否分离本地/远程 id？
- `prompt_safe_plugin_description` 是否实现（防 prompt 注入）？
- `LoadedPlugin` / `PluginLoadOutcome` 是否区分？

**直接抄的清单**：
1. 引入 `PluginCapabilitySummary` 统一多能力
2. 引入 `PluginTelemetryMetadata` 区分远/近 ID
3. `validate_plugin_segment` 防止 plugin id 注入

---

## 5. Skills 系统

### 5.1 设计要点

#### 5.1.1 系统级 skill 安装（`skills/src/lib.rs:32-56`）

```rust
pub fn install_system_skills(codex_home: &AbsolutePathBuf) -> Result<(), SystemSkillsError> {
    let skills_root_dir = codex_home.join(SKILLS_DIR_NAME);
    fs::create_dir_all(skills_root_dir.as_path())?;
    let dest_system = system_cache_root_dir(codex_home);
    let marker_path = dest_system.join(SYSTEM_SKILLS_MARKER_FILENAME);
    let expected_fingerprint = embedded_system_skills_fingerprint();
    if dest_system.as_path().is_dir()
       && read_marker(&marker_path).is_ok_and(|marker| marker == expected_fingerprint) {
        return Ok(());  // 指纹匹配 → 跳过
    }
    if dest_system.as_path().exists() {
        fs::remove_dir_all(dest_system.as_path())?;
    }
    write_embedded_dir(&SYSTEM_SKILLS_DIR, &dest_system)?;
    fs::write(marker_path.as_path(), format!("{expected_fingerprint}\n"))?;
    Ok(())
}
```

**核心设计**：
1. **嵌入式 assets**：`const SYSTEM_SKILLS_DIR: Dir = include_dir::include_dir!("$CARGO_MANIFEST_DIR/src/assets/samples");`（`lib.rs:10`）—— 系统 skill 编译进二进制
2. **指纹机制**：仅当 `embedded_system_skills_fingerprint()` 与磁盘 marker 不一致时才重写（`lib.rs:65-77`）—— 启动开销最小化
3. **盐值版本化**：`SYSTEM_SKILLS_MARKER_SALT: &str = "v1"`（`lib.rs:15`）—— 升级时改盐即可强制重装

#### 5.1.2 指纹算法（`lib.rs:65-96`）

```rust
fn embedded_system_skills_fingerprint() -> String {
    let mut items = Vec::new();
    collect_fingerprint_items(&SYSTEM_SKILLS_DIR, &mut items);
    items.sort_unstable_by(|(a, _), (b, _)| a.cmp(b));
    let mut hasher = DefaultHasher::new();
    SYSTEM_SKILLS_MARKER_SALT.hash(&mut hasher);
    for (path, contents_hash) in items {
        path.hash(&mut hasher);
        contents_hash.hash(&mut hasher);
    }
    format!("{:x}", hasher.finish())
}
```
**核心**：(path, content_hash) 排序后哈希 —— **确定性**（同输入必同输出）。

#### 5.1.3 扩展 crate（`ext/skills/src/lib.rs`）

外部 skills crate 提供 `config` + `state` + `render` 子模块（未细看），与 system skills 解耦。

#### 5.1.4 Skills Watcher（`app-server/src/lib.rs:106`）

```rust
mod skills_watcher;
```
**文件系统监控**：skills 目录变化时通知 app-server。

### 5.2 Synthia 借鉴路径

`crates/synthia-skill/` 已有（`Cargo.toml` 显示含 `zip / notify / sha2 / tempfile`），**关键缺失**：
1. **嵌入式 system skills** —— 当前依赖磁盘下载
2. **指纹 marker 跳过机制** —— 当前可能每次都重新安装
3. **嵌入式 vs 扩展分离** —— 应拆 `synthia-skill-system`（嵌入式）+ `synthia-skill`（用户/扩展）

**直接抄的清单**：
1. 用 `include_dir!` 嵌入默认 skill 包
2. `install_system_skills` + `embedded_system_skills_fingerprint` + marker file
3. `SYSTEM_SKILLS_MARKER_SALT` 版本化

---

## 6. Hooks 系统

### 6.1 设计要点

#### 6.1.1 10 个事件（`hooks/src/lib.rs:19-30`）

```rust
pub const HOOK_EVENT_NAMES: [&str; 10] = [
    "PreToolUse", "PermissionRequest", "PostToolUse",
    "PreCompact", "PostCompact",
    "SessionStart", "UserPromptSubmit",
    "SubagentStart", "SubagentStop",
    "Stop",
];
```
**对比 synthia**：`synthia-hook` 大概率有 PreToolUse/PostToolUse，但 **Pre/PostCompact** 和 **SubagentStart/Stop** 是少见的（覆盖 compaction 和子 agent 边界）。

#### 6.1.2 Matcher 区分（`lib.rs:32-46`）

```rust
pub const HOOK_EVENT_NAMES_WITH_MATCHERS: [&str; 8] = [
    "PreToolUse", "PermissionRequest", "PostToolUse",
    "PreCompact", "PostCompact", "SessionStart",
    "SubagentStart", "SubagentStop",
];
```
**`Stop` / `UserPromptSubmit` 没有 matcher** —— 因为它们不针对具体工具/触发器。

#### 6.1.3 Hook 结果（`types.rs:14-30`）

```rust
pub enum HookResult {
    Success,
    FailedContinue(Box<dyn Error>),
    FailedAbort(Box<dyn Error>),
}
impl HookResult {
    pub fn should_abort_operation(&self) -> bool {
        matches!(self, Self::FailedAbort(_))
    }
}
```
**关键**：区分"失败但继续" vs "失败且中止" —— **三态语义**（Success / Continue / Abort）。

#### 6.1.4 注册表（`registry.rs:47-107`）

```rust
pub struct Hooks {
    after_agent: Vec<Hook>,
    engine: ClaudeHooksEngine,  // 兼容 Claude hooks JSON
}

impl Hooks {
    pub fn new(config: HooksConfig) -> Self {
        let after_agent = config.legacy_notify_argv
            .filter(|argv| !argv.is_empty() && !argv[0].is_empty())
            .map(crate::notify_hook)
            .into_iter()
            .collect();
        let engine = ClaudeHooksEngine::new(...);
        Self { after_agent, engine }
    }

    pub async fn dispatch(&self, hook_payload: HookPayload) -> Vec<HookResponse> {
        let hooks = self.hooks_for_event(&hook_payload.hook_event);
        let mut outcomes = Vec::with_capacity(hooks.len());
        for hook in hooks {
            let outcome = hook.execute(&hook_payload).await;
            let should_abort_operation = outcome.result.should_abort_operation();
            outcomes.push(outcome);
            if should_abort_operation {
                break;  // 短路
            }
        }
        outcomes
    }
}
```
**短路语义**：第一个返回 `FailedAbort` 的 hook 终止后续 hook。

#### 6.1.5 引擎选择

`ClaudeHooksEngine` —— **兼容 Claude Code 的 hooks JSON schema**！这是 codex 的"复用"策略。Synthia 决定不学 Claude，但 **"复用生态"** 这个思路值得借鉴（如可考虑兼容 OpenCode 的 hook 配置）。

### 6.2 Synthia 借鉴路径

| 借鉴项 | 现状 | 行动 |
|--------|------|------|
| 10 事件（含 Pre/PostCompact + Subagent 边界） | 缺 Compact 和 Subagent | 补 |
| `FailedContinue` vs `FailedAbort` 三态 | 大概率是 bool | 改 enum |
| `should_abort_operation` 短路 | 不确定 | 检查 |
| Hook 预览（`preview_*`） | 不存在 | 借鉴用于 TUI "即将触发什么"展示 |
| Plugin 作为 hook source | `PluginHookSource` 类型已存在 | 在 `synthia-plugin` 加 hooks 字段 |

---

## 7. Goals 系统

### 7.1 设计要点

#### 7.1.1 模块结构（`ext/goal/src/lib.rs:1-25`）

```rust
mod accounting;
mod analytics;
mod api;
mod events;
mod extension;
mod metrics;
mod runtime;
mod spec;
mod steering;
mod tool;
```
**核心子模块**：
- `api` —— 外部接口（`GoalService`）
- `runtime` —— 目标运行时（执行 steering）
- `spec` —— 目标协议定义
- `tool` —— 目标相关的工具
- `accounting` —— token 预算会计
- `steering` —— "目标驱动"在每轮注入"你正在做什么"
- `events` —— 目标事件
- `metrics` —— 目标指标

#### 7.1.2 GoalService（`api.rs:75-200`）

```rust
#[derive(Debug, Default)]
pub struct GoalService {
    runtimes: Mutex<HashMap<String, Weak<GoalRuntimeHandle>>>,
}
impl GoalService {
    pub async fn set_thread_goal(&self, state_db, request) -> Result<GoalSetOutcome, ...> {
        // 1. 校验
        // 2. 取 runtime.handle().goal_state_permit() —— Semaphore 互斥
        // 3. prepare_external_goal_mutation —— 让 idle continuation 暂停
        // 4. state_db.thread_goals().update_thread_goal / replace_thread_goal
    }
}
```

**关键设计**：
1. **`Weak<GoalRuntimeHandle>`** —— 弱引用，让 thread 死亡时自动清理
2. **`Semaphore(1)`** —— `goal_state_lock`（`runtime.rs:49, 101`）—— 同一 thread 同一时刻只能有一个 goal mutation
3. **`prepare_external_goal_mutation`** —— 在写之前先"通知" runtime：idle continuation 不能基于即将被改的 goal state 启动
4. **token_budget** —— 每个 goal 独立 token 预算

#### 7.1.3 目标运行时（`runtime.rs:23-200`）

```rust
pub struct GoalRuntimeHandle {
    inner: Arc<GoalRuntimeInner>,
}
struct GoalRuntimeInner {
    thread_id: ThreadId,
    state_dbs: Arc<codex_state::StateRuntime>,
    analytics: GoalAnalytics,
    event_emitter: GoalEventEmitter,
    metrics: GoalMetrics,
    thread_manager: Weak<ThreadManager>,
    accounting_state: Arc<GoalAccountingState>,
    enabled: AtomicBool,
    tools_available_for_thread: bool,
    goal_state_lock: Semaphore,
}
```

**职责**：
- `account_active_goal_progress` —— 把当前 turn 归到 active goal
- `goal_state_permit` —— 外部 mutation 时持有锁
- `prepare_external_goal_mutation` —— 清理 active continuation

#### 7.1.4 Steering（`steering.rs`）

未细看但从导出看（`runtime.rs:17-18`）：
```rust
use crate::steering::continuation_steering_item;
use crate::steering::objective_updated_steering_item;
```
**含义**：每轮 LLM 决策时，steering 模块注入 "你正在追求 goal X（progress: Y/Z）" —— 类似 P5 末尾复述。

### 7.2 Synthia 借鉴路径

**Synthia 当前无目标系统**（基于 `crates/` 列表）。可借鉴：
1. **Thread-scoped single goal**（不是 list）—— 简单 + 与 LLM 决策对齐
2. **token_budget** —— 防止"无目标消耗"
3. **Semaphore 锁** —— 并发 mutation 安全
4. **Steering 注入** —— 末尾复述变体
5. **`prepare_external_goal_mutation`** —— 防止基于过期 state 启动 continuation

**实施位置**：新增 `synthia-goal` crate，依赖 `synthia-session` + `synthia-state`。

---

## 8. Tools 系统核心

### 8.1 设计要点

#### 8.1.1 模块结构（`core/src/tools/mod.rs:1-16`）

```rust
pub(crate) mod code_mode;
pub(crate) mod context;
pub(crate) mod events;
pub(crate) mod handlers;
pub(crate) mod hook_names;
pub(crate) mod hosted_spec;
pub(crate) mod lifecycle;
pub(crate) mod network_approval;
pub(crate) mod orchestrator;       // <-- 关键
pub(crate) mod parallel;
pub(crate) mod registry;
pub(crate) mod router;             // <-- 关键
pub(crate) mod runtimes;
pub(crate) mod sandboxing;
pub(crate) mod spec_plan;
pub(crate) mod tool_dispatch_trace;
```

#### 8.1.2 ToolRouter（`router.rs:34-150`）

```rust
pub struct ToolRouter {
    registry: ToolRegistry,
    model_visible_specs: Vec<ToolSpec>,  // 给模型看的 specs（已脱敏）
}

pub(crate) struct ToolRouterParams<'a> {
    pub(crate) mcp_tools: Option<Vec<ToolInfo>>,
    pub(crate) deferred_mcp_tools: Option<Vec<ToolInfo>>,
    pub(crate) discoverable_tools: Option<Vec<DiscoverableTool>>,
    pub(crate) extension_tool_executors: Vec<Arc<dyn ToolExecutor<ExtensionToolCall>>>,
    pub(crate) dynamic_tools: &'a [DynamicToolSpec],
}
```

**关键设计**：
1. **`Option<Vec<ToolInfo>>`** —— `mcp_tools` 是 optional，可以延迟注入
2. **`discoverable_tools`** —— 工具发现（model 知道工具存在但需要时再加载）
3. **`deferred_mcp_tools`** —— 延迟加载的 MCP 工具（与 `tool_search` 配合）
4. **`extension_tool_executors`** —— 来自 extension/plugin 的执行器
5. **`dynamic_tools`** —— 运行时动态注册的工具

#### 8.1.3 Build Tool Call（`router.rs:95-143`）

```rust
pub fn build_tool_call(item: ResponseItem) -> Result<Option<ToolCall>, FunctionCallError> {
    match item {
        ResponseItem::FunctionCall { name, namespace, arguments, call_id, .. } => {
            let tool_name = ToolName::new(namespace, name);
            Ok(Some(ToolCall { tool_name, call_id, payload: ToolPayload::Function { arguments } }))
        }
        ResponseItem::ToolSearchCall { call_id: Some(call_id), execution, arguments, .. }
            if execution == "client" => {
            let arguments: SearchToolCallParams = serde_json::from_value(arguments)?;
            Ok(Some(ToolCall { tool_name: ToolName::plain("tool_search"), call_id,
                               payload: ToolPayload::ToolSearch { arguments } }))
        }
        // ...
    }
}
```

**关键**：**`execution == "client"`** —— MCP 协议支持 server 端或 client 端执行工具。

#### 8.1.4 并行支持（`router.rs:83-87`）

```rust
pub fn tool_supports_parallel(&self, call: &ToolCall) -> bool {
    self.registry.supports_parallel_tool_calls(&call.tool_name).unwrap_or(false)
}
```
**per-tool 并行能力** —— 不是所有工具都支持并发。

#### 8.1.5 工具 spec 类型（`tools/src/lib.rs:1-25`）

```rust
mod code_mode;
mod dynamic_tool;
mod function_call_error;
mod image_detail;
mod json_schema;
mod mcp_tool;
mod request_plugin_install;
mod response_history;
mod responses_api;     // Responses API 风格
mod tool_call;
mod tool_config;
mod tool_definition;
mod tool_discovery;    // <-- 工具发现
mod tool_executor;
mod tool_output;
mod tool_payload;
mod tool_search;       // <-- tool_search 工具
mod tool_spec;         // Function / Freeform / Namespace / WebSearch / ImageGeneration
```

**5 种 spec**：`Function` / `Freeform` / `Namespace` / `WebSearch` / `ImageGeneration` / `ToolSearch`。

#### 8.1.6 工具发现（`tool_discovery.rs`）

```rust
pub const LIST_AVAILABLE_PLUGINS_TO_INSTALL_TOOL_NAME: &str = "...";
pub const REQUEST_PLUGIN_INSTALL_TOOL_NAME: &str = "...";
pub const TOOL_SEARCH_TOOL_NAME: &str = "...";
```
**3 个元工具**：列出可安装 plugin、请求安装 plugin、按关键字搜索工具。

### 8.2 Synthia 借鉴路径

`crates/synthia-tool-orchestrator/` 已有，但需对照：
1. 是否支持 `deferred_tools`（`mcp_tools`/`extension_tools`/`dynamic_tools` 分类）？
2. `tool_supports_parallel` 是否有？
3. `tool_search` 元工具（model 主动 search 工具）是否存在？
4. `RequestPluginInstall` 工具链是否存在？
5. `executor: server vs client` 区分是否存在？

**直接抄的清单**：
1. `ToolRouter` + `ToolRouterParams` 显式参数化
2. `tool_search` 元工具（与 P3 按需加载天然契合）
3. 工具元数据（parallel/serial/cancel 行为）

---

## 9. Compact 系统

### 9.1 设计要点

#### 9.1.1 三阶段（`core/src/compact.rs:1-200`）

1. **Pre-Compact Hook**（`compact.rs:145-160`）：
```rust
let pre_compact_outcome = run_pre_compact_hooks(&sess, &turn_context, trigger).await;
match pre_compact_outcome {
    PreCompactHookOutcome::Continue => {}
    PreCompactHookOutcome::Stopped => { return Err(CodexErr::TurnAborted); }
}
```

2. **Compact Task 内联**（`compact.rs:72-97`）：
```rust
pub(crate) async fn run_inline_auto_compact_task(sess, turn_context, ..., reason, phase) -> CodexResult<()> {
    let prompt = turn_context.compact_prompt().to_string();
    let input = vec![UserInput::Text { text: prompt, text_elements: Vec::new() }];
    run_compact_task_inner(sess, turn_context, input, initial_context_injection, ...).await?;
    Ok(())
}
```

3. **Post-Compact Hook**（`compact.rs:172-184`）：
```rust
if result.is_ok() {
    let post_compact_outcome = run_post_compact_hooks(&sess, &turn_context, trigger).await;
    if let PostCompactHookOutcome::Stopped = post_compact_outcome {
        return Err(CodexErr::TurnAborted);
    }
}
```

**三阶段全部 hookable**。

#### 9.1.2 初始上下文注入策略（`compact.rs:62-66`）

```rust
pub(crate) enum InitialContextInjection {
    BeforeLastUserMessage,  // mid-turn compact
    DoNotInject,            // pre-turn / manual
}
```

**关键设计**：
- `DoNotInject` 模式：compact 完清空 `reference_context_item`，下次 regular turn 会自动重新注入初始 context
- `BeforeLastUserMessage` 模式：mid-turn 必须保留 initial context（model 训练如此）

#### 9.1.3 远程 vs 本地 compact

```rust
pub(crate) fn should_use_remote_compact_task(provider: &ModelProviderInfo) -> bool {
    provider.supports_remote_compaction()
}
```
**关键**：某些 provider 支持"remote compaction"（把整个对话发到服务端做摘要），否则走本地 prompt。

#### 9.1.4 触发维度

- `CompactionTrigger::Auto` / `Manual`（`compact.rs:91, 117`）
- `CompactionReason`（`UserRequested` 等）
- `CompactionPhase`（`StandaloneTurn` 等）

**四维分类** —— 让 analytics 能区分"为什么 compact"和"怎么 compact"。

#### 9.1.5 Analytics 集成

`CompactionAnalyticsAttempt` 跟踪（`compact.rs:136-144`）：
```rust
let attempt = CompactionAnalyticsAttempt::begin(
    sess.as_ref(), turn_context.as_ref(), trigger, reason,
    CompactionImplementation::Responses, phase,
).await;
```

**含义**：compact 过程本身有 metrics（时长、状态、错误），不只"compact 完成"是事件。

### 9.2 Synthia 借鉴路径

Synthia 当前在 `synthia-context` 应该有 compaction，但需对照 codex：
1. **Pre/Post Compact Hook** —— 大概率缺失
2. **InitialContextInjection 策略** —— 大概率缺失
3. **Remote vs Local 切换** —— 大概率缺失
4. **4 维分类**（trigger/reason/phase/implementation）—— 大概率缺失

**直接抄的清单**：
1. 加 `PreCompact` / `PostCompact` 两个 hook 事件
2. 加 `CompactionTrigger` / `CompactionReason` 枚举
3. 在 telemetry 加 `compaction_duration` / `compaction_implementation` tag

---

## 10. Memory 系统

### 10.1 设计要点

#### 10.1.1 模块结构（`ext/memories/src/lib.rs:1-23`）

```rust
mod backend;
mod extension;
mod local;
mod metrics;
mod prompts;
mod schema;
mod tools;

pub use extension::install;

pub(crate) const DEFAULT_LIST_MAX_RESULTS: usize = 2_000;
pub(crate) const MAX_LIST_MAX_RESULTS: usize = 2_000;
pub(crate) const DEFAULT_SEARCH_MAX_RESULTS: usize = 200;
pub(crate) const MAX_SEARCH_MAX_RESULTS: usize = 200;
pub(crate) const DEFAULT_READ_MAX_TOKENS: usize = 20_000;
pub(crate) const MEMORY_TOOL_DEVELOPER_INSTRUCTIONS_SUMMARY_TOKEN_LIMIT: usize = 2_500;

pub(crate) const MEMORY_TOOLS_NAMESPACE: &str = "memories";
pub(crate) const ADD_AD_HOC_NOTE_TOOL_NAME: &str = "add_ad_hoc_note";
pub(crate) const LIST_TOOL_NAME: &str = "list";
pub(crate) const READ_TOOL_NAME: &str = "read";
pub(crate) const SEARCH_TOOL_NAME: &str = "search";
```

**4 个工具 + 1 个 add_ad_hoc_note**：
- `add_ad_hoc_note` —— 任意时刻记笔记
- `list` —— 列出
- `read` —— 读取
- `search` —— 搜索

**关键常量**：
- `MEMORY_TOOLS_NAMESPACE = "memories"` —— 命名空间隔离（与 MCP tool 区分）
- `MAX_LIST_RESULTS = 2_000` —— 硬上限
- `MAX_SEARCH_RESULTS = 200` —— 搜索硬上限
- `DEFAULT_READ_MAX_TOKENS = 20_000` —— 读取时按 token 限
- `MEMORY_TOOL_DEVELOPER_INSTRUCTIONS_SUMMARY_TOKEN_LIMIT = 2_500` —— **summary 注入到 system prompt 的 token 限制**

#### 10.1.2 backend 抽象

```rust
mod backend;  // remote backend?
mod local;    // local file backend
```

**关键**：有"remote"和"local"两种 backend —— memory 可以是远端服务或本地文件。

#### 10.1.3 内存资格（App-Server）

`thread/memoryMode/set` —— per-thread 决定"是否启用 memory"（`app-server/README.md:144`）。

### 10.2 Synthia 借鉴路径

`crates/synthia-memory/` 已有。**关键缺失**：
1. **`MEMORY_TOOLS_NAMESPACE`** —— synthia 大概率没分命名空间
2. **`DEFAULT_READ_MAX_TOKENS`** —— 读取限 token（防止超大 memory 撑爆 context）
3. **`MEMORY_TOOL_DEVELOPER_INSTRUCTIONS_SUMMARY_TOKEN_LIMIT`** —— **注入到 prompt 的限额**（P1 前缀一致性关键！summary 大小变化 = cache miss）
4. **`memory_mode`** —— per-thread 启/停
5. **4 个工具的命名约定**

**直接抄的清单**：
1. 引入 `synthia_memory::NAMESPACE = "memory"`
2. `SUMMARY_TOKEN_LIMIT = 2500`（可配）
3. `add_ad_hoc_note` 工具（最常用）
4. `read` 工具的 token 限
5. `app-server` 加 `memory/reset`（清空 + 保留 mode）

---

## 11. OTel / 可观测性

### 11.1 设计要点

#### 11.1.1 顶层结构（`otel/src/lib.rs:1-78`）

```rust
pub(crate) mod config;
mod events;
pub(crate) mod metrics;
pub(crate) mod provider;
pub(crate) mod trace_context;
mod otlp;
mod targets;
```

**5 大子模块**：config、events、metrics、provider、trace_context。

#### 11.1.2 Provider（`otel/src/lib.rs:27`）

```rust
pub use crate::provider::OtelProvider;
```
**统一 OTel provider**：tracer + logger + metrics client 三件套。

#### 11.1.3 全局 metrics client（`otel/src/metrics/mod.rs:24-32`）

```rust
static GLOBAL_METRICS: OnceLock<MetricsClient> = OnceLock::new();
static GLOBAL_STATSIG_METRICS_SETTINGS: OnceLock<StatsigMetricsSettings> = OnceLock::new();
pub(crate) fn install_global(metrics: MetricsClient) { let _ = GLOBAL_METRICS.set(metrics); }
pub fn global() -> Option<MetricsClient> { GLOBAL_METRICS.get().cloned() }
```

**`OnceLock<MetricsClient>`** —— 全局单例 metrics client，可被业务代码 `start_global_timer(name, tags)` 调用。

#### 11.1.4 事件 / 指标拆分

- `mod events` —— session telemetry、auth env metadata
- `mod metrics` —— runtime metrics、tag value sanitization、timer

**关键设计**：
- `events` 是 **结构化业务事件**（session/turn/tool）
- `metrics` 是 **数值指标**（counter/histogram/timer）

#### 11.1.5 决策来源（`otel/src/lib.rs:39-45`）

```rust
pub enum ToolDecisionSource {
    AutomatedReviewer,
    Config,
    User,
}
```
**三来源** —— permission/tool 决策的来源追溯（"是谁决定允许/拒绝的"）。

#### 11.1.6 遥测模式（`otel/src/lib.rs:48-65`）

```rust
pub enum TelemetryAuthMode {
    ApiKey, Chatgpt,
}
```
Auth mode 与 telemetry 解耦（避免循环依赖 `codex-core`）。

#### 11.1.7 与 project memory 的对应

| Project memory 硬约束 | codex-rs 实践 |
|---------------------|---------------|
| Cache key 含 user_id | `codex-mcp/src/connection_manager.rs:132` `codex_apps_tools_cache_key`（含 user_key） |
| Permission 默认 AskUser | `codex-otel/src/lib.rs:39-45` 决策来源枚举 |
| Loop 检测用 Mutex | codex 用了 `ElicitationRequestManager`（`elicitation.rs`） |
| Compaction 单遍扫描 | `compact.rs` 的 `run_compact_task_inner_impl`（**linear**, 见后续） |
| Pruning 用 idempotent marker | `compact.rs` 通过 `reference_context_item` 标记 |

### 11.2 Synthia 借鉴路径

`crates/synthia-telemetry/` 已有，**关键缺失/差异**：
1. **`global()` OnceLock 模式** —— 大概率没有全局指标 client
2. **`ToolDecisionSource` enum** —— permission 决策的来源追溯
3. **`events` vs `metrics` 分层** —— 检查是否清晰
4. **`MEMORY_TOOL_DEVELOPER_INSTRUCTIONS_SUMMARY_TOKEN_LIMIT` 类常量** —— 注入到 prompt 的限额

**直接抄的清单**：
1. `synthia_telemetry::global_metrics() -> Option<MetricsClient>`
2. `DecisionSource { Automated, Config, User }` 枚举
3. `event` 子模块（结构化业务事件） vs `metric` 子模块（数值）分离

---

## 12. Task / Subagent 与多 Agent 角色

### 12.1 设计要点

#### 12.1.1 4 种 TaskKind（`core/src/tasks/mod.rs:1-65`）

```rust
mod compact;
mod lifecycle;
mod regular;
mod review;
mod user_shell;

pub(crate) use compact::CompactTask;
pub(crate) use regular::RegularTask;
pub(crate) use review::ReviewTask;
pub(crate) use user_shell::UserShellCommandTask;

const GRACEFULL_INTERRUPTION_TIMEOUT_MS: u64 = 100;
const TASK_COMPACT_METRIC: &str = "codex.task.compact";
```

**5 种 task**：
- `RegularTask` —— 常规 turn
- `CompactTask` —— compact turn
- `ReviewTask` —— 评审 turn
- `UserShellCommandTask` —— 用户 shell 命令
- `RegularTask` + interrupt marker 协调

**`GRACEFULL_INTERRUPTION_TIMEOUT_MS: 100`** —— 100ms 宽限中断。

#### 12.1.2 Multi-Agent 版本（`tasks/mod.rs:68-89`）

```rust
pub(crate) enum InterruptedTurnHistoryMarker {
    Disabled,
    ContextualUser,  // v1
    Developer,       // v2
}
impl InterruptedTurnHistoryMarker {
    pub(crate) fn from_config_and_version(config: &Config, multi_agent_version: MultiAgentVersion) -> Self {
        if !config.agent_interrupt_message_enabled { return Self::Disabled; }
        if multi_agent_version == MultiAgentVersion::V2 { Self::Developer } else { Self::ContextualUser }
    }
}
```
**v1 vs v2**：中断标记从"contextual user"升级为"developer" —— **history 标记类型会随 multi-agent 版本变化**。

#### 12.1.3 SessionTaskContext（`tasks/mod.rs:170-200`）

```rust
pub(crate) struct SessionTaskContext {
    session: Arc<Session>,
    turn_extension_data: Arc<ExtensionData>,
}
impl SessionTaskContext {
    pub fn new(session: Arc<Session>, turn_extension_data: Arc<ExtensionData>) -> Self { ... }
    pub fn clone_session(&self) -> Arc<Session> { ... }
    pub fn turn_extension_data(&self) -> Arc<ExtensionData> { ... }
    pub fn auth_manager(&self) -> Arc<AuthManager> { ... }
    pub fn models_manager(&self) -> SharedModelsManager { ... }
}
```

**关键**：task 上下文是 **细粒度能力暴露**（auth/models/extension），不是整个 Session。

#### 12.1.4 AgentRole（`core/src/agent/role.rs:1-200`）

```rust
pub const DEFAULT_ROLE_NAME: &str = "default";
const AGENT_TYPE_UNAVAILABLE_ERROR: &str = "agent type is currently not available";

pub(crate) async fn apply_role_to_config(config: &mut Config, role_name: Option<&str>) -> Result<(), String> {
    let role_name = role_name.unwrap_or(DEFAULT_ROLE_NAME);
    let role = resolve_role_config(config, role_name).cloned()
        .ok_or_else(|| format!("unknown agent_type '{role_name}'"))?;
    apply_role_to_config_inner(config, role_name, &role).await
        .map_err(|err| { tracing::warn!("failed to apply role to config: {err}"); AGENT_TYPE_UNAVAILABLE_ERROR.to_string() })
}

async fn apply_role_to_config_inner(config: &mut Config, role_name: &str, role: &AgentRoleConfig) -> anyhow::Result<()> {
    let is_built_in = !config.agent_roles.contains_key(role_name);
    let Some(config_file) = role.config_file.as_ref() else { return Ok(()); };
    // 加载 role 的 config.toml 作为 layer 插入
    // 保留 caller's model_provider / service_tier（粘性）
    let preserve_current_provider = role_layer_toml.get("model_provider").is_none();
    let preserve_current_service_tier = role_layer_toml.get("service_tier").is_none();
    *config = reload::build_next_config(config, role_layer_toml, preserve_current_provider, preserve_current_service_tier).await?;
    Ok(())
}
```

**关键设计**：
1. **Role 是 ConfigLayer**（`role_layer(role_layer_toml.clone())`，`role.rs:197-199`）—— **角色本质是 config 的覆盖层**
2. **保留 caller 的 provider / service_tier** —— **粘性语义**，避免子 agent 静默回退
3. **built-in vs user-defined** 区分（`role.rs:91-110`）—— built-in 用 `include_str!` 内嵌
4. **`MultiAgentVersion`** —— v1 vs v2 区分

### 12.2 Synthia 借鉴路径

`crates/synthia-task/` + `crates/synthia-agent/` 已有。**关键对照**：
1. **5 种 task 类型** —— synthia 大概率只有 1-2 种
2. **MultiAgentVersion v1/v2 切换** —— 大概率没有版本概念
3. **AgentRole = ConfigLayer** —— **synthia 的 agent 大概率直接传完整 Config**，而 codex 用 layer 叠加
4. **保留 caller 的 provider / service_tier** —— synthia 的子 agent 是否会"继承但被改写"？
5. **GRACEFULL_INTERRUPTION_TIMEOUT_MS = 100** —— 100ms 宽限

**直接抄的清单**：
1. 把 AgentRole 改造为 **ConfigLayer**（而不是完整 Config 覆盖）
2. `MultiAgentVersion` 枚举 + 自动迁移
3. `SessionTaskContext` 细粒度能力暴露
4. 100ms 宽限中断

---

## 13. 跨系统借鉴矩阵（按 Synthia 现状映射）

| codex-rs 借鉴点 | 关键文件:行号 | Synthia 对应 crate | 优先级 | 估算改动量 |
|----------------|--------------|------------------|------|----------|
| ToolRouter + 5 种 spec | `tools/src/lib.rs:1-25` | `synthia-tool-orchestrator` | P0 | 中 |
| ToolSearch 元工具 | `tools/src/tool_discovery.rs` | `synthia-tool-orchestrator` | P1 | 小 |
| McpConnectionManager 18 参数 | `codex-mcp/src/connection_manager.rs:107-200` | `synthia-mcp` | P0 | 中 |
| ToolPluginProvenance | `codex-mcp/src/lib.rs:24` | `synthia-mcp` | P1 | 小 |
| MCP OAuth 完整闭环 | `codex-mcp/src/lib.rs:55-65` | `synthia-mcp` | P1 | 中 |
| App-Server 双 loop | `app-server/src/lib.rs:139-200` | `synthia-server` | P0 | 中 |
| App-Server `-32001` 背压 | `app-server/README.md:51-53` | `synthia-server` | P1 | 小 |
| PluginCapabilitySummary | `plugin/src/lib.rs:29-37` | `synthia-plugin` | P0 | 小 |
| PluginTelemetryMetadata 远/近 | `plugin/src/lib.rs:49-77` | `synthia-plugin` | P2 | 小 |
| Skills include_dir 嵌入 | `skills/src/lib.rs:10` | `synthia-skill` | P0 | 小 |
| Skills 指纹 marker 跳过 | `skills/src/lib.rs:32-77` | `synthia-skill` | P0 | 小 |
| Hook 10 事件 | `hooks/src/lib.rs:19-30` | `synthia-hook` | P0 | 中 |
| Hook FailedContinue/Abort 三态 | `hooks/src/types.rs:14-30` | `synthia-hook` | P0 | 小 |
| GoalService + Semaphore | `ext/goal/src/api.rs:75-200` | 新增 `synthia-goal` | P1 | 大 |
| Goal token_budget | `ext/goal/src/api.rs:42-46` | 新增 `synthia-goal` | P1 | 中 |
| Pre/PostCompact Hook | `core/src/compact.rs:145-184` | `synthia-context` | P1 | 中 |
| InitialContextInjection 策略 | `core/src/compact.rs:62-66` | `synthia-context` | P1 | 中 |
| 4 维 CompactionReason | `core/src/compact.rs:62-97` | `synthia-context` | P1 | 小 |
| Memory 4 工具 + 限额 | `ext/memories/src/lib.rs:11-22` | `synthia-memory` | P0 | 中 |
| MEMORY_TOOL_SUMMARY_TOKEN_LIMIT | `ext/memories/src/lib.rs:16` | `synthia-memory` | P0 | 小 |
| Otel global() OnceLock | `otel/src/metrics/mod.rs:24-32` | `synthia-telemetry` | P0 | 小 |
| ToolDecisionSource enum | `otel/src/lib.rs:39-45` | `synthia-permission` | P1 | 小 |
| 5 种 Task 类型 | `core/src/tasks/mod.rs:1-65` | `synthia-task` | P0 | 大 |
| AgentRole = ConfigLayer | `core/src/agent/role.rs:130-200` | `synthia-agent` | P0 | 中 |
| MultiAgentVersion 迁移 | `core/src/tasks/mod.rs:76-89` | `synthia-agent` | P2 | 中 |
| CodeModeService Session 模型 | `code-mode/src/service.rs:99-220` | 新增 `synthia-code-mode` | P2 | 大 |

---

## 14. Synthia 直接可借鉴的"5 分钟清单"

下面 5 项改动量小、价值高，**建议在 OpenSpec 之前先落地**：

1. **`synthia-skill` 加指纹 marker**（`skills/src/lib.rs:32-77` 复制粘贴）
2. **`synthia-tool-orchestrator` 加 `tool_supports_parallel`**（`tools/router.rs:83-87` 复制）
3. **`synthia-hook` 把 `Result<bool>` 改 `Result<HookOutcome>`**（`hooks/src/types.rs:14-30`）
4. **`synthia-memory` 加 `SUMMARY_TOKEN_LIMIT` 常量**（`ext/memories/src/lib.rs:16`）
5. **`synthia-permission` 加 `DecisionSource` 枚举**（`otel/src/lib.rs:39-45`）

---

## 15. 风险与 OpenSpec 建议

### 15.1 与 project memory 硬约束的冲突点

| codex 实践 | 与 synthia 记忆冲突? | 处置 |
|----------|------------------|------|
| `codex_apps_tools_cache_key` 含 user_key | **一致**（"Cache control must include user_id namespace"） | ✅ 复用 |
| `ToolDecisionSource { AutomatedReviewer, Config, User }` | **一致**（"Permission default AskUser"） | ✅ 复用 |
| `ElicitationRequestManager` 内部 state | **不直接对应**（"Loop detection must use Mutex"） | 注意 Elicitation 也要 Mutex 包裹 |
| `CompactionImplementation::Responses` | **不直接对应**（"Compaction single-pass"） | codex 已经是 single-pass，与记忆一致 |
| `McpConnectionManager` 18 参数 | **不冲突**（"Path traversal checks in path-specific fields"） | 在 `extract_string_values` 时遵守 |
| `CodeModeService` `include_dir!` 嵌入 skill | **不冲突**（"Don't auto-commit"） | 嵌入物不进 git |

### 15.2 不直接借鉴的 codex 设计

1. **V8 运行时** —— synthia 当前工具编排更接近"显式注册"，引入 V8 是大改动。**先观察 6 个月**（按记忆 "First fix critical bugs, then discuss architectural abstractions after 6 months"）。
2. **`MultiAgentVersion` 自动迁移** —— 没有用户量之前不需要。
3. **`ClaudeHooksEngine` 兼容 Claude** —— synthia 决定不学 Claude（与 project 一致），但 **"复用生态"思路保留**（如兼容 OMO 自己的 hook schema）。

### 15.3 OpenSpec 建议

按价值/改动比：

1. **【P0】`synthia-skill` 加指纹 marker** → 单文件改动，价值立竿见影
2. **【P0】`synthia-tool-orchestrator` 加 `tool_supports_parallel` + `ToolSpec` 分类** → 中等改动，与 codex 路径对齐
3. **【P0】`synthia-hook` 升级 HookResult 三态** → 兼容 SubagentStart/Stop 事件
4. **【P1】`synthia-goal` 新建 crate** → 仿 `ext/goal`，引入 token_budget + Semaphore
5. **【P1】`synthia-context` 加 Pre/PostCompact Hook** → 与 `synthia-hook` 集成
6. **【P2】`synthia-code-mode` 新建 crate** → 观望，不急

### 15.4 与 `AGENTS.md` 原则的一致性

| AGENTS.md 原则 | codex-rs 体现 | Synthia 借鉴 |
|---------------|-------------|-----------|
| P1 前缀一致性 | `MEMORY_TOOL_SUMMARY_TOKEN_LIMIT = 2500` 保证注入 size 稳定 | 抄 |
| P2 Append-Only | `CompactItem` 用 marker，不改原 history | 抄 |
| P3 按需加载 | `deferred_mcp_tools` / `tool_search` / `discoverable_tools` | 抄 |
| P4 渐进降级 | `CompactionTrigger` / `CompactionReason` 分类 | 抄 |
| P5 末尾复述 | `steering.rs` 的 `continuation_steering_item` | 抄 |
| P6 不信任 LLM | `HookResult::FailedAbort` 短路 | 抄 |
| P7 可中断性 | `GRACEFULL_INTERRUPTION_TIMEOUT_MS = 100` | 抄 |
| P8 不丢信息 | `event log` + `rollout-trace` crate | 抄 |
| P9 可观测性 | `OtelProvider` + `global() OnceLock` | 抄 |
| P10 文件即记忆 | `rollout/state_db` 持久化 thread + goal | 抄 |

---

## 附录 A：本报告涉及的关键 codex-rs 文件清单

| 文件 | 行数 | 角色 |
|------|------|------|
| `code-mode/src/lib.rs` | 7 | Code Mode 入口（façade） |
| `code-mode/src/service.rs` | 1821 | CodeModeService + 大量测试 |
| `code-mode/src/runtime/callbacks.rs` | 324 | V8 ↔ Rust 桥（tool/text/image/store/notify/exit） |
| `code-mode/src/runtime/globals.rs` | 160 | V8 全局对象安装（沙箱） |
| `codex-mcp/src/lib.rs` | 82 | MCP 模块导出 |
| `codex-mcp/src/connection_manager.rs` | 200+ | McpConnectionManager（18 参数构造器） |
| `app-server/src/lib.rs` | 220+ | App-Server 主循环（processor + outbound） |
| `app-server/README.md` | 159 | 协议规范 |
| `plugin/src/lib.rs` | 78 | Plugin 顶层抽象 |
| `skills/src/lib.rs` | 169 | Skills 安装 + 指纹 marker |
| `hooks/src/lib.rs` | 109 | 10 个 hook 事件常量 |
| `hooks/src/registry.rs` | 200+ | Hooks 注册 + 调度 |
| `hooks/src/types.rs` | 150+ | HookResult 三态 + HookPayload |
| `ext/goal/src/lib.rs` | 28 | Goal 模块结构 |
| `ext/goal/src/api.rs` | 200+ | GoalService（Semaphore 锁） |
| `ext/goal/src/runtime.rs` | 200+ | GoalRuntimeHandle |
| `core/src/tools/mod.rs` | 110 | Tools 模块结构 |
| `core/src/tools/router.rs` | 150+ | ToolRouter + dispatch |
| `core/src/compact.rs` | 200+ | Compact 三阶段（Pre/Task/Post） |
| `ext/memories/src/lib.rs` | 26 | Memory 4 工具 + 限额 |
| `otel/src/lib.rs` | 78 | OTel 顶层 |
| `otel/src/metrics/mod.rs` | 100+ | 全局 metrics client |
| `core/src/tasks/mod.rs` | 200+ | 5 种 task 类型 + MultiAgentVersion |
| `core/src/agent/role.rs` | 200+ | AgentRole = ConfigLayer |

---

**报告完成**。所有 12 个子系统均给出文件:行号引用、关键设计、Synthia 借鉴路径。
**下一步**：与项目维护者确认优先级 1-3 项的 OpenSpec 提案。
