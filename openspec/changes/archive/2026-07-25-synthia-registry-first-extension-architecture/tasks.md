# Tasks: synthia-registry-first-extension-architecture

## Phase 1: 基础设施 (P0)

### 1.1 ToolName + Namespace
- [x] 创建 `ToolName` 结构体（namespace: Option<String>, name: String）
- [x] 实现 Display, Hash, Eq, Serialize, Deserialize, From<String>
- [x] 修改 `ToolDescriptor.name` 从 String 改为 ToolName
- [x] 修改 `ToolRegistry` 内部 HashMap key 从 String 改为 ToolName
- [x] 修改 `ToolEntry` 使用 ToolName
- [x] 修改 `Materialization` 使用 ToolName 作为 key
- [x] 为 MCP 工具添加 `mcp__{server_name}` 命名空间
- [x] 编写 ToolName 单元测试

### 1.2 RegistrationScope
- [x] 实现 `ToolRegistry::unregister_by_token()`
- [x] 创建 `RegistrationScope` 结构体
- [x] 实现 `impl Drop for RegistrationScope`
- [x] 添加 `ToolRegistry::register_scoped()` 方法
- [x] 添加 `ToolRegistry::register_scoped_with_namespace()` 方法
- [x] 在 Session 创建时创建 RegistrationScope
- [x] 编写 RegistrationScope 单元测试

### 1.3 PermissionInterceptor
- [x] 实现 `PermissionInterceptor` 结构体（持有 Arc<PermissionChecker>）
- [x] 实现 `Interceptor` trait 的 `intercept()` 方法
- [x] BeforeTool 事件中调用 security_check()
- [x] Block 返回 ShortCircuited
- [x] RequireConfirm 调用 ApprovalService
- [x] AutoApprove 直接继续
- [x] 编写 PermissionInterceptor 单元测试

### 1.4 LoopDetectInterceptor
- [x] 实现 `LoopDetectInterceptor` 结构体（持有 Arc<Mutex<LoopDetectorSet>>）
- [x] AfterTool 和 AfterLlm 事件中调用 LoopDetectorSet::check()
- [x] 检测到循环时返回 ShortCircuited
- [x] 迁移现有 LoopDetectorSet 实例为 Interceptor 注册
- [x] 编写 LoopDetectInterceptor 单元测试

### 1.5 ApprovalInterceptor
- [x] 实现 `ApprovalInterceptor` 结构体（持有 Arc<dyn ApprovalService>）
- [x] BeforeTool 件中，对 RequireConfirm 级别调用 request_approval()
- [x] 用户拒绝时返回 ShortCircuited
- [x] 编写 ApprovalInterceptor 单元测试

### 1.6 RetryInterceptor
- [x] 实现 `RetryInterceptor` 结构体（持有 max_retries, base_delay）
- [x] AfterTool 事件中，失败时指数退避重试
- [x] 重试次数记录在 InterceptorContext::data
- [x] 超过 max_retries 后传递错误
- [x] 编写 RetryInterceptor 单元测试

### 1.7 CompactInterceptor
- [x] 实现 `CompactInterceptor` 结构体（持有 Arc<ContextCompactor>, threshold_tokens）
- [x] IterationEnd 事件中检查 token 使用量
- [x] 超过阈值时触发上下文压缩
- [x] 压缩结果写入 InterceptorContext::data
- [x] 编写 CompactInterceptor 单元测试

### 1.8 统一安全路径
- [x] 在 InterceptorChain 添加 `default_with_guard()` 方法
- [x] PermissionInterceptor 硬编码为位置 0
- [x] ToolProvider::before_execute/after_execute 标记 #[deprecated]
- [x] 提供迁移文档
- [x] 修改 main_loop.rs 工具执行流程为新的守卫路径
- [x] 编写集成测试验证安全守卫不可绕过

## Phase 2: 扩展维度 (P1)

### 2.1 FragmentRegistry
- [x] 定义 `ContextFragment` trait（name, priority, is_active, render）
- [x] 定义 `FragmentContext` 结构体
- [x] 定义 `FragmentError` 错误类型
- [x] 实现 `FragmentRegistry`（register, unregister, render_active）
- [x] 编写 FragmentRegistry 单元测试

### 2.2 内建 Fragment 迁移
- [x] 实现 SystemPromptFragment
- [x] 实现 TokenBudgetFragment
- [x] 实现 SkillsFragment
- [x] 实现 PermissionsFragment
- [x] 实现 PluginsFragment
- [x] 实现 EnvironmentFragment
- [x] 实现 RolloutBudgetFragment
- [x] 实现 CustomFragment
- [x] 将 ContextAssembler::assemble() 改为委托 FragmentRegistry::render_active()
- [x] ContextAssembler 标记 #[deprecated]
- [x] 编写 Fragment 迁移集成测试

### 2.3 ToolExposure + DeferredTool
- [x] 定义 ToolExposure 枚举（Direct, Deferred, Hidden）
- [x] ToolDescriptor 添加 exposure 字段
- [x] 修改 ToolRegistry::materialize() 根据 exposure 决定是否包含完整定义
- [x] 实现 Deferred 工具首次调用时加载完整定义
- [x] 实现 tool_search 内建工具（BM25 搜索）
- [x] 编写 DeferredTool 单元测试

### 2.4 异步 ExtensionPoints
- [x] BeforeHandler 签名改为 async
- [x] AfterHandler 签名改为 async
- [x] DefinitionHandler 签名改为 async
- [x] fire_before/fire_after/fire_definition 改为 async fn
- [x] 提供 register_before_sync() 兼容包装
- [x] LlmExtensionRegistry 同样改为 async
- [x] main_loop.rs 调用点添加 .await
- [x] 编写异步 ExtensionPoints 测试

### 2.5 ExtensionRegistry
- [x] 定义 ExtensionRegistry 结构体（五个子 Registry）
- [x] 实现 shutdown() 方法
- [x] 实现 health_check() 方法
- [x] 实现 Plugin 加载时的跨维度注册协调
- [x] 编写 ExtensionRegistry 单元测试

### 2.6 Agent 瘦身
- [x] Agent 结构体改为 4 核心 + ExtensionRegistry
- [x] 旧字段通过 impl Agent 的 #[deprecated] getter 保持兼容
- [x] provider_registry 迁移到 ExtensionRegistry
- [x] tool_registry 迁移到 ExtensionRegistry
- [x] hook_registry 迁移到 InterceptorChain
- [x] command_registry 迁移到 ToolRegistry
- [x] context_assembler 迁移到 FragmentRegistry
- [x] mcp_manager 迁移到 ToolRegistry（as ToolProvider）
- [x] approval_service 迁移到 InterceptorChain
- [x] sandbox_manager 迁移到 InterceptorChain
- [x] steering_channel/config_watcher/memory_event_sender 迁移到 InterceptorChain
- [x] 修改 run_stream 使用 ExtensionRegistry
- [x] 编写 Agent 瘦身集成测试

## Phase 3: 生产级能力 (P2-P3)

### 3.1 SkillRegistry
- [x] 定义 Skill trait（name, description, instructions, tools, detect_invocation, provenance）
- [x] 定义 SkillProvenance 枚举
- [x] 实现 SkillRegistry（register, unregister, list, get, detect_skills）
- [x] 技能激活时注入 instructions 到 system prompt
- [x] 技能激活时声明需要的 tools
- [x] 编写 SkillRegistry 单元测试

### 3.2 内建 Skills
- [x] 实现 CodingSkill
- [x] 实现 SearchSkill
- [x] 实现 DebugSkill
- [x] 支持 Markdown frontmatter 文件加载
- [x] 编写内建 Skills 测试

### 3.3 RolloutTracker
- [x] 定义 RolloutTracker 结构体
- [x] 定义 FileChange 和 ChangeType
- [x] 定义 TokenBudget
- [x] 实现 record_change()、record_token_usage()、summary()
- [x] 实现 RolloutBudgetFragment
- [x] 在 main_loop 工具执行后调用 record_change()
- [x] 在 LLM 响应后调用 record_token_usage()
- [x] 编写 RolloutTracker 单元测试

### 3.4 PluginRegistry
- [x] 定义 Plugin trait
- [x] 定义 PluginCapabilitySummary
- [x] 实现 PluginRegistry（load, unload, discover）
- [x] 插件加载时注册 tools + skills + fragments
- [x] 插件卸载时 Scope Drop 自动清理
- [x] 实现 discover() 文件系统扫描
- [x] 编写 PluginRegistry 单元测试
