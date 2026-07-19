# OpenCode 控制面架构模式 → synthia 借鉴清单

> 本节从 opencode `packages/core/src/` 提炼 8 条**用户可见控制面**架构模式（Tool / Event pubsub / Plugin / 并行调度），每条含：opencode 实现摘要（file:line）+ synthia 现状差距 + Rust trait 草案 + 落地代价。

---

## 模式 1 — Tool 注册表：按名字的 LIFO 栈（scope finalizer）

**opencode**：`registry.ts:47` 用 `Map<String, Array<{token, registration}>>` 存储每个工具名的活跃注册数组；`materialize` / `settleWith` 用 `.at(-1)` 选最新项（`registry.ts:51, 108`）。每次 `register` 调用创建私有 `token = {}` 并装入 `Effect.uninterruptible` 内的 `addFinalizer`（`registry.ts:88-103`），闭包到 scope 时**按 token 选择性移除**——这意味着嵌套 scope 闭包不会误删更新的 override（`registry.ts:96-98`）。**没有**显式 `RegistrationToken`；公开 API 只暴露 `register(): Effect<void, _, Scope.Scope>`（`registry.ts:25`），scope 本身即 lifetime handle。

**synthia 现状**：`synthia-tool/src/registry/registration/registry.rs:63-78` 是 `RwLock<HashMap<String, ToolEntry>>` 的 flat map；`unregister` 是显式调用（`registry_trait.rs:38-83`）。已存在 `ScopedToolRegistry` + `LayeredToolRegistry`（`synthia-tool/src/scoped_registry.rs:29, 208`）用 `DashMap<Vec<ScopedRegistration>>` + RAII `ScopeGuard`，但与主 `ToolRegistry` 并存两套抽象，没人用。

**Rust 草案**（替换 flat map，**向后兼容**—— `HashMap` API 保留为 deprecated）：
```rust
pub struct StackedToolRegistry {
    // name -> stack of registrations; newest wins
    inner: RwLock<HashMap<String, Vec<Registration>>>,
}

pub struct Registration {
    token: Arc<RegistrationToken>,  // drop-unregister
    tool: Arc<dyn Tool>,
    scope: ScopeGuard,
}
impl Drop for Registration {
    fn drop(&mut self) { /* filter-by-token from name stack */ }
}

impl StackedToolRegistry {
    pub fn push(&self, name: String, tool: Arc<dyn Tool>) -> RegistrationToken;
    pub fn materialize(&self) -> Vec<ToolDefinition>;  // top-of-stack per name
    pub fn resolve(&self, name: &str) -> Option<Arc<dyn Tool>>;  // .last()
}
```

**代价**：**中（一个 crate + 一个 PR）**。纯增量，不破坏 `Tool` trait。需新建 `stacked-registry.rs` 子模块 + 让 `DynamicResolver` 复用。**向后兼容**——旧 `ToolRegistry` 可保留 `#[deprecated]`。

---

## 模式 2 — Tool 双输出：`structured`（DB/replay）+ `content`（LLM/UI）

**opencode**：`tool.ts:44-107` 的 `Tool.Config` 暴露 `execute → Effect<Output, Failure>` 返回 typed 值 + 可选 `toModelOutput({input, output}) → ReadonlyArray<{text|file}>`。`settle` 在 `tool.ts:84-111` 内部把 output **重新 encode** 通过 output schema（捕获作者 bug），调用 `toModelOutput` 得到 model 视图。最终 `ToolOutput = { structured, content }`（`packages/llm/src/schema/messages.ts:95-98`）。Provider 仅收到 `inputSchema`（`tool.ts:74` + `messages.ts:119-124` 折叠规则）。

**synthia 现状**：`synthia-tool/src/types.rs:50-55` 的 `ToolInput.input` 是 `serde_json::Value`；`ToolOutput.content` 是 `Vec<ContentPart>`（`types.rs:67-81`）+ `metadata: serde_json::Map` 包，没有 formal output schema。`AgentEvent::ToolCallCompleted.output` 进一步塌缩成 `String`（`events/event_enum.rs:44-48`）。`synthemars = "0.8"` 已在 `synthia-tool/Cargo.toml:29` 但**无人使用**。

**Rust 草案**（**破坏性**——需扩展 `Tool` trait）：
```rust
pub trait Tool: Send + Sync {
    type Input: DeserializeOwned + JsonSchema;
    type Output: Serialize + DeserializeOwned + JsonSchema;
    
    async fn execute(&self, input: Self::Input, ctx: &ToolContext) 
        -> Result<Self::Output, ToolFailure>;
    
    /// Optional: override default text-from-output model view.
    fn to_model_output(&self, input: &Self::Input, output: &Self::Output) 
        -> Vec<ContentPart> { /* default: if output serializes to string, wrap as Text */ }
}

pub struct ToolOutput {
    pub structured: serde_json::Value,  // Output re-encoded
    pub content: Vec<ContentPart>,
    pub metadata: ToolMetadata,         // duration, tokens, output_paths
}
```

**代价**：**高（跨 crate 接口变更）**。需要改 5 个内置工具（`read/write/edit/bash/grep/glob/multi_edit`）+ MCP adapter。**破坏性**——`Tool::call(&self, ToolInput) -> ToolOutput` 需替换为带关联类型的版本。可分两阶段：阶段 1 增加 `ToolV2: Tool<I,O>` 默认实现 + `#[deprecated] Tool::call`；阶段 2 迁移完后删除旧版。

---

## 模式 3 — Event Bus：durable / ephemeral 双通道 + versioned type

**opencode**：`event.ts:418-429` 一个 `notify()` 函数派发到 3 个 sink（listener 数组、per-type PubSub、global PubSub）。**关键差异**在 definition metadata：`Definition.sync` 字段决定是否持久化（`event.ts:31-34, 385-407`）。`durable` 事件走 `commitSyncEvent`（写 `EventTable` + `EventSequenceTable`，`event.ts:255-369`，**`uninterruptible`**）+ 触发 `sync` handlers；`ephemeral` 仅走内存 PubSub。`session/event.ts:471-500` 显式分割 `DurableDefinitions` vs `EphemeralDefinitions`——`Text.Delta/Reasoning.Delta/Tool.Input.Delta/Compaction.Delta` 是 ephemeral（`session/event.ts:235, 273, 317, 436`），`*.Ended` 是 durable。`versionedType(type, version)` 用 `${type}.${version}` 作 `syncRegistry` key（`event.ts:81-83`）；**没有 upcasting shim**——只支持 strict version match + 旧版本定义保留解码已存行（`session/event.ts:446-468` 的 `Compaction.EndedV1`）。

**synthia 现状**：3 个独立通道（`agent/src/events/emitter.rs:19` `mpsc::UnboundedSender` + `server/event_stream.rs:18` `broadcast::Sender(128)` + `orchestrator/lib.rs:443` `broadcast::Sender(256)`），加独立 `synthia-session/src/store/events.rs:69-202` JSONL `EventStore`。`AgentEvent::is_durable()`（`events/event_enum.rs:228-252`）按 variant 硬编码分类。`EventBusExtensionRegistry`（`agent/tools/dynamic_provider/extension_points/event_bus.rs:231-438`）已实现但**未接入 agent loop**。

**Rust 草案**（统一在 `synthia-session`，**向后兼容**）：
```rust
pub trait Event: Serialize + DeserializeOwned + Send + Sync + 'static {
    const TYPE: &'static str;
    /// None = ephemeral (in-memory only); Some(v) = durable w/ version
    const SYNC: Option<SyncSpec> = None;
}

pub struct SyncSpec {
    pub aggregate: &'static str,   // e.g. "session_id"
    pub version: u32,
}

pub struct EventBus {
    typed_pubsub: DashMap<TypeId, broadcast::Sender<Arc<dyn AnyEvent>>>,
    all: broadcast::Sender<Arc<dyn AnyEvent>>,
    durable_log: Arc<EventStore>,
}

impl EventBus {
    pub async fn publish<E: Event>(&self, e: E);  
    // - if E::SYNC.is_some() → append to EventStore + broadcast seq
    // - else → broadcast only
    pub fn subscribe<E: Event>(&self) -> impl Stream<Item = E>;
}
```

**代价**：**中（拆 crate + 一个 PR）**。新建 `synthia-eventbus` crate 合并 `AgentEventEmitter` + `EventBroadcaster` + JSONL `EventStore`。`sync` 元数据按需添加（默认 `None`），**向后兼容**——旧 `mpsc` 调用点用 `compat` 子模块。

---

## 模式 4 — Plugin：每个插件 fork child scope + 热卸载

**opencode**：`plugin.ts:110` 每个 `add` 调用 `Scope.fork(parentScope)` 创 child scope，把插件 effect 在 child scope 内执行（`Scope.provide`，`plugin.ts:112`）；任何 `Effect.forkScoped` / `Effect.addFinalizer` 由插件 effect 启动的都归属 child scope——`models-dev.ts:144-147` 的事件订阅就是典型。重 `add` 同 id 会**先 close 旧 child scope 再 fork 新的**（`plugin.ts:108-109`），实现 hot-reload。`KeyedMutex`（`plugin.ts:102` + `effect/keyed-mutex.ts:20-42`）保证 per-id 串行。无 token；scope 即 lifetime。

**synthia 现状**：`synthia-plugin/src/registry/store.rs:22-25` `PluginRegistry { plugins: HashMap<...> }` 是**进程级 flat map**，无 RwLock（`store.rs:50-67` `load_plugin` 需要外部 Mutex），**无 per-plugin scope / lifetime**。`unload_plugin`（`store.rs:70-75`）只移除 map 项，不取消订阅。`HookHandler::Prompt` 是 stub（`hook_runner/execute.rs:32-41`）。

**Rust 草案**（**破坏性**——改 `PluginRegistry`）：
```rust
pub struct ScopedPluginRegistry {
    inner: Arc<DashMap<PluginId, PluginHandle>>,
    scopes: DashMap<PluginId, Arc<tokio_util::sync::CancellationToken>>,
}

pub struct PluginHandle {
    pub id: PluginId,
    pub manifest: PluginManifest,
    pub hooks: Vec<HookConfig>,
    pub cancel_token: Arc<CancellationToken>,  // signal hot-unload
}

impl ScopedPluginRegistry {
    pub async fn load(&self, path: PluginPath) -> Result<PluginHandle, PluginError>;
    pub async fn unload(&self, id: &PluginId) -> Result<()> {
        // 1. cancel_token.cancel() → all fork-scoped tasks receive signal
        // 2. drop from inner map
    }
    pub fn reload(&self, path: PluginPath) -> impl Future<Output = Result<PluginHandle>> {
        // unload(old) then load(new) — atomic w.r.t. per-id mutex
    }
}
```

**代价**：**中（一个 PR 跨 `synthia-plugin` + `synthia-agent`）**。每个 hook 启动时 `let child = token.child_token(); ...; select! { _ = child.cancelled() => break, ... }`。**向后兼容**——`HookConfig` 不变，只加 lifetime token。

---

## 模式 5 — 工具并行：`FiberSet` + `uninterruptibleMask` 保护 commit

**opencode**：`session/runner/llm.ts:191` 为每 turn 创建 `FiberSet<void, ToolOutputStore.Error>`。每个 local tool call `FiberSet.run(toolFibers, settle_effect)` 立刻 fire（`llm.ts:259-280`），**不等 provider stream 结束**——并行 fan-out。Join 用 `raceFirst(FiberSet.join, FiberSet.awaitEmpty)`（`llm.ts:137-138`），避免 join 在 child 已完成消失时死等。每个 tool settlement 包在 `Effect.uninterruptibleMask` 里（`llm.ts:259-280`）：**只有 `toolMaterialization.settle(...)` 在 `restore(...)` 内可中断**，durable event 发布在 masked 区——确保 "tool 完成 → 发布结果" 原子不被中断。Publication 串行化（`llm.ts:239-241` 一 permit semaphore）。**并发上限无界**——limit 在上层 provider/UI 而非 runner。

**synthia 现状**：`synthia-tool-orchestrator/src/lib.rs:807-849` 的 `execute_batch` 用 `futures::stream::iter(...).buffer_unordered(self.concurrency_policy.max_concurrent)`——**正确的并行**，但**无 commit 保护**：`tool_output` 直接返回，没有 `event.publish` 这步（publish 由 agent 的 stream builder 后续做，期间若 cancelled 结果丢失）。`CancellationToken` 通过 `child_token()` 传入（`orchestrator/lib.rs:561`）正确；`is_concurrency_safe` 工具用 per-tool mutex（`orchestrator/lib.rs:516-526`）。`MAX_STEPS` 类似保护在 `agent/main_loop` 但具体常量分散。

**Rust 草案**（**向后兼容**——仅 orchestrator 内部改造）：
```rust
pub struct ProtectedExecutor {
    fibers: Arc<tokio::task::JoinSet<ToolCallResult>>,
    publisher: Arc<EventBus>,
}

impl ProtectedExecutor {
    pub async fn run_batch(
        &self,
        requests: Vec<ToolCallRequest>,
        cancellation: CancellationToken,
    ) -> Result<Vec<ToolCallResult>> {
        for req in requests {
            let token = cancellation.child_token();
            let publisher = self.publisher.clone();
            // Outer uninterruptible mask: protect commit
            self.fibers.spawn(tokio::spawn(async move {
                let result = token.run_cancellable(req.execute()).await;  // interruptible
                // Inner uninterruptible: commit MUST complete or roll back
                tokio::time::timeout(Duration::from_secs(30), 
                    commit_result(publisher, &result)
                ).await
            }));
        }
        // raceFirst-equivalent: collect all or fail on first defect
        join_all_or_first_error(&self.fibers).await
    }
}
```

**代价**：**中（orchestrator 内部改造 + PR）**。需把 `event.publish` 移入 orchestrator 路径（现在在 agent stream builder 里），加 commit timeout。**向后兼容**——外部 API 不变。

---

## 模式 6 — Event Bus 上的 materialize → settle 分离 + stale identity 检测

**opencode**：`ToolRegistry.materialize`（`registry.ts:105-119`）返回 `Settlement` closure，**闭包捕获**了 materialize 时的 advertised name → identity map（`registry.ts:115-118`）。Settlement 时 `settleWith` 比对当前 effective registration 与 advertised identity，**不一致返回 stale**（`registry.ts:59-60`）——避免"广告时刻之后被 override，调用落到错的实现"。同时 `materialize` 也做权限 whole-tool 过滤（`registry.ts:111-114`）——`permission.findLast(rule => resource=="*" && effect=="deny")` 即完全禁用。

**synthia 现状**：`ToolRegistry::run_with_context`（`synthia-tool/src/registry/registration/registry.rs:153-254`）**每次调用同步拉取**——无 snapshot，无 stale 检测。如果 LLM 在 step T 收到 tool list，step T+1 plugin 卸载了该 tool，会**直接 panic / fall through**。`LayeredToolRegistry::materialize(session_id)`（`synthia-tool/src/scoped_registry.rs:208-298`）有这个概念但只在 session 维度。

**Rust 草案**（**向后兼容**）：
```rust
pub struct Materialization {
    advertised: HashMap<String, Arc<dyn Tool>>,  // name -> tool ref
    identity_token: Arc<MaterializationToken>,
}

pub struct ToolResolution {
    pub tool: Arc<dyn Tool>,
    pub identity: Arc<MaterializationToken>,  // mismatch → stale
}

impl StackedToolRegistry {
    pub fn materialize(&self) -> Materialization;  // snapshot w/ identity
    pub fn resolve(&self, mat: &Materialization, name: &str) 
        -> Result<ToolResolution, StaleOrUnknown>;
}
```

**代价**：**低（一个 PR）**。改 `run_with_context` 在进入时 `materialize()` 一次，调用 `resolve(mat, name)`；`stale` 转 `ToolOutput::error("Tool definition changed; refresh")`。

---

## 模式 7 — Tool 名字校验 + 注册时全部名前置校验

**opencode**：`tool.ts:116-119` 名字必须 ASCII letter 开头、剩余字符 `[a-zA-Z0-9_-]`、最长 64。`Registry.register`（`registry.ts:85-87`）**先全名校验再插入**——任一名字非法则整个 batch 拒绝（`registry.ts:88-103` 的 `Effect.uninterruptible` 保证 atomicity）。`Effect.uninterruptible` 同时保护 mutation 和 finalizer 安装，防止中断半途留下无 finalizer 的注册。

**synthia 现状**：`PluginManifest::validate`（`synthia-plugin/src/manifest.rs:109-129`）只校验 plugin name（kebab-case + semver）；**Tool name 无校验**——任何 string 都可 `register`。注册时无 atomic batch 概念（`registry.rs:140-144` 单个插入）。

**Rust 草案**（**向后兼容**）：
```rust
pub fn validate_tool_name(name: &str) -> Result<(), RegistrationError> {
    let mut chars = name.chars();
    let first = chars.next().ok_or_else(|| empty_name())?;
    if !first.is_ascii_alphabetic() { return Err(...); }
    if !chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-') { return Err(...); }
    if name.len() > 64 { return Err(...); }
    Ok(())
}

impl StackedToolRegistry {
    pub fn push_batch(&self, batch: HashMap<String, Arc<dyn Tool>>) 
        -> Result<RegistrationToken, RegistrationError> {
        // Pre-validate ALL names BEFORE any mutation
        for name in batch.keys() { validate_tool_name(name)?; }
        // Atomic mutation under no-await span
        let token = Arc::new(RegistrationToken::new());
        let mut guard = self.inner.write();
        for (name, tool) in batch { /* push */ }
        Ok(token)
    }
}
```

**代价**：**低（一个 PR）**。纯增量，无 breaking change。

---

## 模式 8 — 列表 enumerated + Output bounding 在 registry 而非 tool

**opencode**：`tool-output-store.ts:132-168` 的 `bound()` 是 registry-sibling 服务，对**所有 tool 输出**统一应用 `MAX_LINES=2000, MAX_BYTES=50KiB`（`tool-output-store.ts:12-13`）。超额时把完整内容写入 `<data>/tool-output/tool_<id>` managed path，content 替换为预览 + 引用。`ToolOutputStore` 在 `registry.ts:74` settlement 后调用。**这意味着任何 tool 想改大小限制，只改一处**。

**synthia 现状**：truncation 在 `synthia-tool/src/types.rs:58-65` 的 `TruncatedBy` + 每个 tool 自己做（如 `synthia-tool-bash` 的 `MAX_CAPTURE_BYTES` `bash_tool/trait_impl.rs:19`）——**零散**，没有 registry 级边界。

**Rust 草案**（**向后兼容**）：
```rust
pub trait OutputBound {
    fn bound(&self, output: ToolOutput, session_id: &SessionId, call_id: &str) 
        -> (ToolOutput, Vec<ManagedPath>);
}

pub struct DefaultOutputBound {
    pub max_lines: usize,  // 2000
    pub max_bytes: usize,  // 50 * 1024
    pub managed_dir: PathBuf,
}

impl StackedToolRegistry {
    pub fn with_output_bound(self, bound: Arc<dyn OutputBound>) -> Self;
}
```

**代价**：**低（一个 PR）**。在 `run_with_context` / orchestrator settlement 路径插入 bound 调用即可。

---

## 总览矩阵

| # | 模式 | opencode 关键 cite | synthia 现状 | 代价 | 兼容性 |
|---|---|---|---|---|---|
| 1 | Stacked LIFO registry | `registry.ts:47-51, 88-103` | flat HashMap + 已存在未用 ScopedRegistry | 中 | 向后 |
| 2 | Tool 双输出 | `tool.ts:44-107`, `messages.ts:95-124` | Value 输入 + 单 content，无 output schema | 高 | 破坏 |
| 3 | Event durable/ephemeral + versioned | `event.ts:81-83, 385-407`, `session/event.ts:471-500` | 3 个独立通道，硬编码 is_durable | 中 | 向后 |
| 4 | Plugin child-scope + hot-unload | `plugin.ts:110-127` | flat HashMap 无 scope/lifetime | 中 | 向后 |
| 5 | FiberSet + uninterruptibleMask | `runner/llm.ts:137-138, 191, 259-280` | buffer_unordered + 无 commit 保护 | 中 | 向后 |
| 6 | materialize → settle identity | `registry.ts:105-119` | 每次同步拉取，无 stale 检测 | 低 | 向后 |
| 7 | 名前置校验 + atomic batch | `registry.ts:85-103`, `tool.ts:116-119` | 无 Tool name 校验 | 低 | 向后 |
| 8 | Registry 级 Output bounding | `tool-output-store.ts:132-168` | 散落在各 tool | 低 | 向后 |

**建议优先落地**（按 ROI 排序）：#6 (低 ROI 极高) → #8 (低) → #7 (低) → #1 (中) → #5 (中) → #4 (中) → #3 (中) → #2 (高，仅当重写工具层时)。

每条都可独立 PR，**先做 4 条 low 改 1 周内可合并**；中改需要拆 crate 评估，建议 2 人各领 1-2 条；#2 推迟到统一工具 trait 重构时。