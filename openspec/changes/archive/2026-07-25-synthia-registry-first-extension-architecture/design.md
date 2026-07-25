# Design: synthia-registry-first-extension-architecture

## 架构概览

```
Agent (精简)
├── config: AgentConfig                      // 配置
├── provider: Arc<dyn ModelProvider>         // 核心：LLM 调用
├── tool_registry: ToolRegistry              // 核心：工具注册
├── session_manager: SessionManager          // 核心：会话管理
└── extensions: ExtensionRegistry            // 统一扩展总线
    ├── tools: ToolRegistry (增强版)         // Tool 维度
    │   ├── Scope 生命周期管理
    │   ├── Namespace 隔离
    │   ├── Deferred 延迟加载
    │   └── 权限守卫硬编码
    ├── fragments: FragmentRegistry          // Fragment 维度
    │   └── ContextFragment trait
    ├── interceptors: InterceptorChain       // Interceptor 维度
    │   └── 实际实现 (非 TODO 占位)
    ├── skills: SkillRegistry               // Skill 维度
    └── plugins: PluginRegistry             // Plugin 维度
```

## 详细设计

### 1. ExtensionRegistry — 统一扩展总线

```rust
/// 统一扩展总线 — 管理五种扩展维度的生命周期
pub struct ExtensionRegistry {
    /// 工具注册表（增强版：Scope + Namespace + Deferred）
    tool_registry: ToolRegistry,
    /// 上下文片段注册表
    fragment_registry: FragmentRegistry,
    /// 拦截器链
    interceptor_chain: InterceptorChain,
    /// 技能注册表
    skill_registry: SkillRegistry,
    /// 插件注册表
    plugin_registry: PluginRegistry,
}
```

ExtensionRegistry 不是"又一个 Registry"，而是**五个正交维度的协调器**：
- 每个 Registry 独立管理自己维度的注册、发现和执行
- ExtensionRegistry 提供统一的生命周期管理（启动、关闭、健康检查）
- Plugin 加载时可能同时注册 tools + skills + fragments，ExtensionRegistry 协调这个跨维度操作

### 2. ToolName — Namespace 隔离

```rust
/// 工具名：支持 namespace::tool 格式
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ToolName {
    /// 命名空间（如 "mcp__github", "plugin__myext"）
    namespace: Option<String>,
    /// 工具名（如 "create_issue"）
    name: String,
}

impl ToolName {
    /// 创建平面工具名（无命名空间）
    pub fn plain(name: impl Into<String>) -> Self {
        Self { namespace: None, name: name.into() }
    }

    /// 创建带命名空间的工具名
    pub fn namespaced(namespace: impl Into<String>, name: impl Into<String>) -> Self {
        Self { namespace: Some(namespace.into()), name: name.into() }
    }

    /// 全名：namespace::name 或 name
    pub fn full_name(&self) -> String {
        match &self.namespace {
            Some(ns) => format!("{}::{}", ns, self.name),
            None => self.name.clone(),
        }
    }
}
```

### 3. RegistrationScope — 生命周期管理

```rust
/// 注册 Scope — Drop 时自动反注册
pub struct RegistrationScope {
    token: RegistrationToken,
    registry: Weak<ToolRegistry>,
    /// 注册的工具名列表（用于反注册）
    tool_names: Vec<ToolName>,
}

impl Drop for RegistrationScope {
    fn drop(&mut self) {
        if let Some(registry) = self.registry.upgrade() {
            registry.unregister_by_token(self.token.clone());
        }
    }
}

impl ToolRegistry {
    /// 带命名空间的 scoped 注册
    pub async fn register_scoped(
        &self,
        namespace: Option<&str>,
        provider: Arc<dyn ToolProvider>,
    ) -> Result<RegistrationScope, RegistrationError> {
        // ... 注册逻辑，返回 Scope
    }
}
```

### 4. ToolExposure — 延迟加载

```rust
/// 工具暴露级别
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolExposure {
    /// 立即可用，完整定义暴露给 LLM
    Direct,
    /// 延迟加载：仅暴露名称和简要描述
    /// 首次调用时通过 tool_search 或按需加载完整定义
    Deferred,
    /// 不暴露给 LLM，仅内部使用
    Hidden,
}
```

### 5. FragmentRegistry — 上下文模块化

```rust
/// 上下文片段 trait — 独立于 Tool 的 prompt 注入
#[async_trait]
pub trait ContextFragment: Send + Sync {
    /// 片段名称
    fn name(&self) -> &str;

    /// 优先级（数字越小越靠前）
    fn priority(&self) -> u32;

    /// 是否在当前上下文中激活
    fn is_active(&self, ctx: &FragmentContext) -> bool;

    /// 渲染片段内容
    async fn render(&self, ctx: &FragmentContext) -> Result<String, FragmentError>;
}

/// 上下文片段注册表
pub struct FragmentRegistry {
    fragments: RwLock<Vec<Arc<dyn ContextFragment>>>,
}

impl FragmentRegistry {
    /// 注册片段
    pub fn register(&self, fragment: Arc<dyn ContextFragment>);

    /// 渲染所有激活片段（按优先级排序）
    pub async fn render_active(&self, ctx: &FragmentContext) -> Result<String, FragmentError>;
}
```

内置 Fragment 列表（借鉴 Codex 的 30+ ContextFragment）：
- `SystemPromptFragment` — 系统提示
- `TokenBudgetFragment` — 令牌预算提示
- `SkillsFragment` — 技能指令
- `PermissionsFragment` — 权限说明
- `PluginsFragment` — 插件指令
- `EnvironmentFragment` — 环境信息
- `RolloutBudgetFragment` — 变更预算提示
- `CustomFragment` — 自定义片段

### 6. InterceptorChain — 实际实现

将四个 TODO 占位替换为实际实现：

```rust
/// 权限拦截器 — 硬编码为第一个拦截器，不可绕过
pub struct PermissionInterceptor {
    checker: Arc<PermissionChecker>,
}

impl Interceptor for PermissionInterceptor {
    async fn intercept(&self, ctx: &mut InterceptorContext, event: &InterceptorEvent, next: NextInterceptor<'_>) -> Result<(), InterceptorError> {
        if let InterceptorEvent::BeforeTool { tool_name } = event {
            let decision = self.checker.security_check(tool_name, &ctx.data).await;
            match decision {
                PermissionLevel::Block => return Err(InterceptorError::ShortCircuited { name: "permission".into() }),
                PermissionLevel::RequireConfirm => { /* 等待用户确认 */ }
                _ => {}
            }
        }
        next.run(ctx, event).await
    }
}

/// 循环检测拦截器 — 适配现有 LoopDetectorSet
pub struct LoopDetectInterceptor {
    detector: Arc<Mutex<LoopDetectorSet>>,
}

/// 审批拦截器 — 调用 ApprovalService
pub struct ApprovalInterceptor {
    service: Arc<dyn ApprovalService>,
}

/// 重试拦截器 — 指数退避重试
pub struct RetryInterceptor {
    max_retries: u32,
    base_delay: Duration,
}

/// 压缩拦截器 — 上下文压缩触发
pub struct CompactInterceptor {
    compactor: Arc<ContextCompactor>,
    threshold_tokens: usize,
}
```

### 7. 异步 ExtensionPoints

将 handler 签名从同步改为异步：

```rust
// 之前（同步）
pub type BeforeHandler = Arc<dyn Fn(&BeforeToolCall) -> Action<BeforeToolCall> + Send + Sync>;

// 之后（异步）
pub type BeforeHandler = Arc<
    dyn for<'a> Fn(&'a BeforeToolCall) -> Pin<Box<dyn Future<Output = Action<BeforeToolCall>> + Send + 'a>>
     + Send + Sync,
>;

// 对应 fire_before 也要改为 async
pub async fn fire_before(&self, event: BeforeToolCall) -> Action<BeforeToolCall> {
    // ...
}
```

### 8. SkillRegistry

```rust
/// 技能 trait — 提示模板 + 工具组合
#[async_trait]
pub trait Skill: Send + Sync {
    /// 技能名称
    fn name(&self) -> &str;

    /// 技能描述
    fn description(&self) -> &str;

    /// 注入到 system prompt 的指令
    fn instructions(&self) -> String;

    /// 需要的工具列表
    fn tools(&self) -> Vec<ToolName>;

    /// 隐式调用检测（从用户输入中检测是否应该激活此技能）
    fn detect_invocation(&self, input: &str) -> bool;

    /// 技能来源
    fn provenance(&self) -> SkillProvenance;
}

/// 技能注册表
pub struct SkillRegistry {
    skills: DashMap<String, Arc<dyn Skill>>,
}
```

### 9. RolloutTracker

```rust
/// 文件变更追踪
pub struct RolloutTracker {
    changes: RwLock<Vec<FileChange>>,
    token_budget: TokenBudget,
    created_at: Instant,
}

/// 文件变更记录
pub struct FileChange {
    pub path: PathBuf,
    pub change_type: ChangeType, // Created / Modified / Deleted
    pub timestamp: Instant,
    pub content_hash: String,
}

/// Token 预算追踪
pub struct TokenBudget {
    pub total: u32,
    pub used: AtomicU32,
    pub remaining: AtomicU32,
}
```

### 10. PluginRegistry

```rust
/// 插件 trait — 打包分发单元
#[async_trait]
pub trait Plugin: Send + Sync {
    /// 插件名称
    fn name(&self) -> &str;

    /// 插件版本
    fn version(&self) -> &str;

    /// 能力摘要
    fn capabilities(&self) -> PluginCapabilitySummary;

    /// 提供的工具
    async fn tools(&self) -> Vec<Arc<dyn ToolProvider>>;

    /// 提供的技能
    async fn skills(&self) -> Vec<Arc<dyn Skill>>;

    /// 提供的上下文片段
    async fn fragments(&self) -> Vec<Arc<dyn ContextFragment>>;
}

/// 插件注册表
pub struct PluginRegistry {
    plugins: DashMap<String, Arc<dyn Plugin>>,
    /// 插件加载的 Scope（卸载时自动清理）
    scopes: DashMap<String, RegistrationScope>,
}
```

### 11. Agent 瘦身

```rust
// 之前：17 个字段
pub struct Agent {
    pub config: AgentConfig,
    pub provider_registry: ProviderRegistry,
    pub provider: Arc<dyn ModelProvider>,
    pub tool_registry: ToolRegistry,
    pub hook_registry: Arc<HookRegistry>,
    pub command_registry: CommandRegistry,
    pub session_manager: SessionManager,
    pub context_assembler: Arc<ContextAssembler>,
    pub model_router: Arc<ModelRouter>,
    pub session_store: SessionStore,
    pub mcp_manager: Option<synthia_mcp::McpManager>,
    pub steering_channel: Option<Arc<dyn SteeringChannel>>,
    pub config_watcher: Option<MultiConfigWatcher>,
    pub memory_event_sender: Option<mpsc::Sender<MemoryEvent>>,
    pub approval_service: Option<Arc<dyn ApprovalService>>,
    pub sandbox_manager: Option<Arc<dyn SandboxManager>>,
}

// 之后：4 核心 + 1 扩展总线
pub struct Agent {
    pub config: AgentConfig,
    pub provider: Arc<dyn ModelProvider>,
    pub session_manager: SessionManager,
    pub extensions: ExtensionRegistry,
}

// ExtensionRegistry 提供 getter 访问旧字段
impl ExtensionRegistry {
    pub fn tool_registry(&self) -> &ToolRegistry { &self.tool_registry }
    pub fn fragments(&self) -> &FragmentRegistry { &self.fragment_registry }
    pub fn interceptors(&self) -> &InterceptorChain { &self.interceptor_chain }
    pub fn skills(&self) -> &SkillRegistry { &self.skill_registry }
    pub fn plugins(&self) -> &PluginRegistry { &self.plugin_registry }
}
```

## Tool 调用执行流（安全守卫）

```
LLM 请求工具调用
  │
  ▼
PermissionInterceptor (硬编码，不可绕过)
  │ Block → 返回错误
  │ RequireConfirm → 等待用户确认
  │ AutoApprove → 继续
  ▼
InterceptorChain.dispatch(BeforeTool) (可扩展拦截)
  │ ShortCircuit → 返回错误
  ▼
ToolExtensionRegistry.fire_before (可修改参数)
  │ Skip → 返回错误
  │ Modify → 使用修改后的参数
  ▼
Tool.execute (实际执行)
  │
  ▼
ToolExtensionRegistry.fire_after (可修改结果)
  ▼
InterceptorChain.dispatch(AfterTool) (可扩展拦截)
  ▼
返回结果给 LLM
```

## 与现有代码的兼容策略

1. **Agent 字段兼容**：通过 `impl Agent` 的方法保持旧字段名的访问（`fn tool_registry(&self) -> &ToolRegistry { self.extensions.tool_registry() }`），标记 `#[deprecated]` 引导迁移
2. **InterceptorChain 保持 dispatch 接口**：新增实现不改变公开接口
3. **ExtensionPoints async 迁移**：提供 `register_before_sync` 兼容方法包装同步 handler 为异步
4. **ToolName 兼容**：平面字符串自动转换为 `ToolName::plain()`，无需修改现有代码
