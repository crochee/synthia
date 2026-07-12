# Synthia Agent 架构升级：完整实施计划

**基于**: 6 个深度研究方向，27 个源码研究，4 个生产系统对比
**日期**: 2026-07-12
**状态**: 实施计划

---

## 执行摘要

深度研究揭示了关键发现：

1. **好消息**: Synthia 已有 Event Sourcing 和部分并行基础设施，但未启用
2. **核心差距**: 工具系统静态注册、无 Session 树、无 steer/followUp 机制
3. **最小改动最大收益**: 启用现有的 `execute_batch()` 并行执行

---

## 阶段 1: 启用并行工具执行 (1-2 天)

### 为什么优先

这是**零架构改动**的最大性能提升。现有基础设施已完整，只是未启用。

### 现有代码位置

```
crates/synthia-tool-orchestrator/src/
├── orchestrator.rs          # execute_batch() 存在但未调用
├── concurrency.rs          # ConcurrencyPolicy { max_concurrent: 5 } 已配置
├── per_tool_lock.rs         # per_tool_locks 准备好了
└── file_mutation.rs         # FileMutationQueue 准备好了
```

### 改动

在 `crates/synthia-agent/src/stream_builder/steps/tool_execute.rs` 中：

```rust
// 当前 (sequential):
for request in requests {
    let result = orchestrator.execute_one(request, &ctx).await?;
    results.push(result);
}

// 改为 (parallel):
let results = orchestrator.execute_batch(requests, &ctx).await?;
```

### 验证

```bash
cargo test -p synthia-tool-orchestrator --test parallel_execution
cargo bench --bench tool_execution
```

---

## 阶段 2: 增强 Steer/followUp 机制 (2-3 天)

### 当前状态

Synthia 有 `SteeringChannel` 但缺乏：
- Drain modes (all / one-at-a-time)
- 阻塞式 followUp
- 注入点控制

### 新增文件

```
crates/synthia-agent/src/steering/
├── mod.rs
├── message_queue.rs      # 扩展 SteeringChannel
├── drain_mode.rs         # DrainMode enum
└── follow_up.rs           # FollowUpOutcome future
```

### 核心改动

```rust
// crates/synthia-agent/src/steering/message_queue.rs

#[derive(Clone)]
pub struct MpscMessageQueue {
    inner: Arc<Mutex<Vec<SteeringMessage>>>,
    watch_tx: watch::Sender<WatchSeq>,
    capacity: usize,
}

impl MessageQueue for MpscMessageQueue {
    fn send(&self, msg: SteeringMessage) -> Result<(), SteeringError> {
        let mut guard = self.inner.lock();
        if guard.len() >= self.capacity {
            // 溢出时删除最低优先级
            if let Some(pos) = guard.iter()
                .enumerate()
                .min_by_key(|(_, m)| m.priority)
                .map(|(i, _)| i)
            {
                guard.remove(pos);
            }
        }
        guard.push(msg);
        let _ = self.watch_tx.send(WatchSeq::default());
        Ok(())
    }

    fn drain_with_mode(&self, mode: DrainMode) -> Vec<SteeringMessage> {
        let mut guard = self.inner.lock();
        match mode {
            DrainMode::All => guard.drain(..).collect(),
            DrainMode::OneAtATime => {
                // 按优先级排序，只取一个
                guard.sort_by(|a, b| b.priority.cmp(&a.priority));
                guard.drain(0..1).collect()
            }
        }
    }

    fn follow_up(
        &self,
        content: String,
        priority: i32,
    ) -> FollowUpFuture {
        let (tx, rx) = oneshot::channel();
        let msg = SteeringMessage {
            content,
            priority,
            response_tx: Some(tx),
            injection_point: InjectionPoint::BeforeLlmCall,
        };
        self.send(msg.into());
        FollowUpFuture(rx)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DrainMode {
    All,        // 默认，清空队列
    OneAtATime, // 一次只处理一条
}

pub enum InjectionPoint {
    BeforeLlmCall,     // 默认
    AfterToolExecution, // 工具执行后
    AtIterationBoundary,// 迭代边界
}
```

### 修改 main_loop.rs

```rust
// 迭代开始时 drain，使用配置的模式
let steering_config = config.steering_config.clone();
for msg in session_input_queue.drain_with_mode(steering_config.drain_mode).await {
    ctx.messages.insert(0, Message::user(msg.content));
    // ...
}
```

---

## 阶段 3: Session 树支持 (5-7 天)

### 当前状态

Session 是扁平的 JSONL，无法 fork/branch。

### 新增文件

```
crates/synthia-session/src/
├── tree/
│   ├── mod.rs
│   ├── entry.rs          # SessionEntry enum
│   ├── navigation.rs      # fork, branch, navigate
│   └── context.rs         # build_context 遍历
└── branch_summary.rs      # 分支摘要生成
```

### 核心类型

```rust
// crates/synthia-session/src/tree/entry.rs

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum SessionEntry {
    Message(MessageEntry),
    ThinkingLevelChange { level: ThinkingLevel, timestamp: i64 },
    ModelChange { model: String, timestamp: i64 },
    Compaction { tokens_before: usize, tokens_after: usize, timestamp: i64 },
    BranchSummary { summary: String, branch_id: String, timestamp: i64 },
    Custom(CustomEntry),
    CustomMessage { role: String, content: String, metadata: Value },
    Label { label: String, timestamp: i64 },
    SessionInfo { name: Option<String>, created_at: i64 },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageEntry {
    pub id: String,
    pub parent_id: Option<String>,  // 树的边
    pub role: String,
    pub content: String,
    pub timestamp: i64,
}

#[derive(Debug, Clone)]
pub struct SessionTree {
    pub root_id: String,
    pub leaf_id: String,  // 当前分支尖端
    pub entries: HashMap<String, SessionEntry>,
}

impl SessionTree {
    /// Fork: 创建新分支，保留现有 entries
    pub fn fork(&mut self, name: Option<String>) -> String {
        let fork_id = uuid::Uuid::new_v4().to_string();
        let fork_entry = SessionEntry::SessionInfo {
            name,
            created_at: Utc::now().timestamp(),
        };
        self.entries.insert(fork_id.clone(), fork_entry);
        self.leaf_id = fork_id;
        fork_id
    }

    /// Navigate: 移动 leaf_id 到指定 entry
    pub fn navigate(&mut self, entry_id: &str) -> Result<(), TreeError> {
        if !self.entries.contains_key(entry_id) {
            return Err(TreeError::EntryNotFound(entry_id.to_string()));
        }
        self.leaf_id = entry_id.to_string();
        Ok(())
    }

    /// Build context: 从 leaf 向上遍历到 root
    pub fn build_context(&self) -> Vec<&SessionEntry> {
        let mut path = Vec::new();
        let mut current = Some(self.leaf_id.clone());

        while let Some(id) = current {
            if let Some(entry) = self.entries.get(&id) {
                path.push(entry);
                current = entry.parent_id().map(|p| p.to_string());
            } else {
                break;
            }
        }

        path.reverse();
        path
    }
}
```

### JSONL 格式扩展

```json
{"id": "msg-1", "parentId": null, "type": "Message", "role": "user", "content": "hello"}
{"id": "msg-2", "parentId": "msg-1", "type": "Message", "role": "assistant", "content": "hi"}
{"id": "msg-3", "parentId": "msg-2", "type": "BranchSummary", "summary": "discussed X and Y"}
{"id": "fork-1", "parentId": "msg-2", "type": "SessionInfo", "name": "alternative-path"}
```

### 迁移策略

1. 添加 `id` 和 `parent_id` 字段到现有 JSONL（向后兼容）
2. 现有 session 的 parent_id 默认为 null（根）
3. fork 时生成新 entry，leaf_id 指向它

---

## 阶段 4: 工具系统动态化 (7-10 天)

### 当前状态

`ToolRegistry` 静态注册，无法运行时添加工具。

### 研究发现

- **opencode**: Effect Schema 驱动，工具定义用 Schema 验证
- **codex**: 两层 `ToolExecutor<Invocation>` + `CoreToolRuntime`
- **synthia**: 已有 `DynamicResolver` 在 orchestrator 层

### 新增文件

```
crates/synthia-agent/src/tools/
├── dynamic_provider.rs    # ToolProvider trait
├── registry_builder.rs     # PlannedTools 风格构建器
└── schema_ref.rs          # SchemaRef enum
```

### 核心 Trait

```rust
// crates/synthia-agent/src/tools/dynamic_provider.rs

/// 扩展点：实现此 trait 即可扩展工具系统
pub trait ToolProvider: Send + Sync {
    /// 返回此扩展提供的工具列表
    fn list_tools(&self) -> Vec<Arc<dyn Tool>>;

    /// 可选：处理生命周期事件
    fn on_event(&self, _event: &AgentEvent) -> Option<Vec<AgentEvent>> { None }

    /// 可选：工具执行前检查
    fn before_tool_execute(
        &self,
        _tool: &str,
        _input: &Value,
    ) -> Option<ToolPreCheck> { None }

    /// 可选：工具执行后处理
    fn after_tool_execute(
        &self,
        _tool: &str,
        _output: &Value,
    ) -> Option<Value> { None }
}

/// 内置工具特征
pub trait Tool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn parameters(&self) -> &Schema;
    fn execute(&self, input: Value, ctx: ToolContext) -> impl Future<Output = Result<ToolResult>> + Send;

    /// 可选：是否支持并行执行
    fn supports_parallel(&self) -> bool { true }
}

/// 工具预检查结果
pub enum ToolPreCheck {
    /// 允许执行
    Allow,
    /// 需要确认
    RequiresApproval(String),
    /// 拒绝执行
    Deny(String),
}

/// Schema 引用
pub enum SchemaRef {
    Inline(Schema),
    Registry(String),      // 引用注册表中的 schema
    Effect(Ast),           // Effect Schema AST
}
```

### RegistryBuilder

```rust
// crates/synthia-agent/src/tools/registry_builder.rs

/// codex PlannedTools 风格的构建器
pub struct ToolRegistryBuilder {
    tools: HashMap<String, Arc<dyn Tool>>,
    deferred: HashMap<String, Schema>,
    dispatch_only: HashSet<String>,
}

impl ToolRegistryBuilder {
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
            deferred: HashMap::new(),
            dispatch_only: HashSet::new(),
        }
    }

    pub fn with(mut self, tool: Arc<dyn Tool>) -> Self {
        self.tools.insert(tool.name().to_string(), tool);
        self
    }

    pub fn with_deferred(mut self, name: &str, schema: Schema) -> Self {
        self.deferred.insert(name.to_string(), schema);
        self
    }

    pub fn with_dispatch_only(mut self, name: &str) -> Self {
        self.dispatch_only.insert(name.to_string());
        self
    }

    pub fn build(self) -> DynamicToolRegistry {
        DynamicToolRegistry {
            tools: self.tools,
            deferred: self.deferred,
            dispatch_only: self.dispatch_only,
        }
    }
}
```

### ExtensionManager

```rust
// crates/synthia-agent/src/tools/extension_manager.rs

pub struct ExtensionManager {
    providers: RwLock<Vec<Arc<dyn ToolProvider>>>,
    tool_cache: RwLock<HashMap<String, Arc<dyn Tool>>>,
    invalidation_token: AtomicU64,
}

impl ExtensionManager {
    pub fn register(&self, provider: Arc<dyn ToolProvider>) {
        let tools = provider.list_tools();
        let mut cache = self.tool_cache.write();
        let mut providers = self.providers.write();

        // Invalidate cache
        self.invalidation_token.fetch_add(1, Ordering::SeqCst);
        cache.clear();

        for tool in tools {
            cache.insert(tool.name().to_string(), tool);
        }
        providers.push(provider);
    }

    pub async fn reload(&self, extension_path: &Path) -> Result<()> {
        // 1. 卸载旧扩展 (通过 provider_id)
        // 2. libloading 加载新的 .so
        // 3. 注册新工具
        todo!()
    }
}
```

---

## 阶段 5: 动态插件加载 (10-14 天)

### 研究发现

- pi-mono: 扩展在 agent core 初始化前加载，注册入队，`on_ready()` 后刷新
- codex: Plugin Manager 生命周期完整
- Security: Extism WASM 沙箱用于不可信插件

### 新增文件

```
crates/synthia-extension/
├── src/
│   ├── trait.rs           # Extension trait
│   ├── loader.rs          # PluginLoader with libloading
│   ├── vtable.rs         # C-compatible vtable
│   ├── manifest.rs        # plugin.json 解析
│   └── wasm_loader.rs      # Extism WASM 支持
├── sys/
│   └── src/lib.rs         # 插件 API types (shared)
└── tests/
    └── mock_extension/     # 测试用 mock 插件
```

### Extension Trait

```rust
// crates/synthia-extension/src/trait.rs

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtensionMetadata {
    pub name: String,
    pub version: String,
    pub description: Option<String>,
}

pub trait Extension: Send + Sync {
    fn metadata(&self) -> ExtensionMetadata;
    fn list_tools(&self) -> Vec<ToolDefinition>;
    fn execute_tool(
        &self,
        tool_name: &str,
        params: Value,
        context: ToolCallContext,
    ) -> Result<Value, String>;
    fn on_ready(&self) {}
    fn on_shutdown(&self) {}
}
```

### PluginLoader

```rust
// crates/synthia-extension/src/loader.rs

pub struct PluginLoader {
    loaded: HashMap<String, PluginBundle<'static>>,
    search_paths: Vec<PathBuf>,
}

impl PluginLoader {
    pub fn load_plugin(&mut self, path: &Path) -> Result<String, ExtensionError> {
        // RTLD_NOW | RTLD_LOCAL for safety
        let lib = unsafe { Library::new(path)? };

        // API version 检查
        let info_fn: Symbol<PluginInfoFn> = unsafe { lib.get(b"plugin_api_version")? };
        let info = unsafe { info_fn() };
        if info.api_version != PLUGIN_API_VERSION {
            return Err(ExtensionError::ApiVersionMismatch {
                expected: PLUGIN_API_VERSION,
                got: info.api_version,
            });
        }

        // 解析入口点
        let entry_sym: Symbol<ExtensionEntryFn> = unsafe {
            lib.get(b"synthia_extension_entry")?
        };
        // ...
    }
}
```

---

## 实施优先级矩阵

| 阶段 | 改动规模 | 收益 | 风险 | 优先级 |
|------|---------|------|------|--------|
| 1. 并行执行 | 极小 | 高性能 | 低 | **P0** |
| 2. Steer/followUp | 小 | 用户体验 | 低 | **P1** |
| 3. Session 树 | 中 | 功能完整 | 中 | **P2** |
| 4. 动态工具 | 大 | 架构升级 | 中 | **P3** |
| 5. 插件系统 | 大 | 可扩展性 | 高 | **P4** |

---

## 关键文件参考

| 研究 | 文件 |
|------|------|
| 并行执行 | `crates/synthia-tool-orchestrator/src/orchestrator.rs` |
| Steer/followUp | `crates/synthia-agent/src/steering.rs` |
| Session 树 | `packages/coding-agent/src/core/session-manager.ts` (pi-mono) |
| 工具系统 | `codex-rs/core/src/tools/registry.rs` |
| 动态插件 | `packages/coding-agent/src/core/extensions/runner.ts` (pi-mono) |

---

## 验证清单

- [ ] 并行执行基准测试通过
- [ ] Steer/followUp 集成测试
- [ ] Session fork/branch/navigate 测试
- [ ] 动态工具注册测试
- [ ] 插件加载/卸载测试
- [ ] WASM 沙箱安全测试

---

## 风险缓解

1. **并行执行**: 现有 `ConcurrencyPolicy` 限制 max_concurrent=5，防止资源耗尽
2. **Session 树**: 向后兼容，现有 JSONL 自动成为单一根分支
3. **插件安全**: 不可信插件强制使用 Extism WASM 沙箱
4. **API 版本**: `plugin_api_version()` 检查在符号解析前

---

*计划基于 6 个深度研究任务、27 个源码研究、4 个生产系统(opencode, codex, pi-mono, synthia) 对比*
