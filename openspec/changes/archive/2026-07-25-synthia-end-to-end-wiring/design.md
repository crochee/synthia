# Design: synthia-end-to-end-wiring

## 架构概览

```
                        端到端数据流（连线后）

  Client
    │
    │ POST /api/v2/sessions/{id}/prompts
    ▼
  ┌─────────────────────────────────────────────────────────────────┐
  │ synthia-server                                                  │
  │                                                                 │
  │  AppState (启动时构建)                                          │
  │  ┌─────────────────────────────────────────────────────────┐   │
  │  │  ExtensionRegistry                                      │   │
  │  │  ├── tools: ToolRegistry (增强版: Scope + Namespace)    │   │
  │  │  ├── fragments: FragmentRegistry                       │   │
  │  │  │   ├── SystemPromptFragment                          │   │
  │  │  │   ├── SkillsFragment                                │   │
  │  │  │   ├── PermissionsFragment                           │   │
  │  │  │   ├── RolloutBudgetFragment                         │   │
  │  │  │   └── EnvironmentFragment                           │   │
  │  │  ├── interceptors: InterceptorChain                    │   │
  │  │  │   ├── [0] PermissionInterceptor (硬编码)            │   │
  │  │  │   ├── [1] LoopDetectInterceptor                    │   │
  │  │  │   ├── [2] ApprovalInterceptor                      │   │
  │  │  │   ├── [3] RetryInterceptor                         │   │
  │  │  │   └── [4] CompactInterceptor                       │   │
  │  │  ├── skills: SkillRegistry                             │   │
  │  │  │   ├── CodingSkill                                   │   │
  │  │  │   └── SearchSkill                                   │   │
  │  │  └── plugins: PluginRegistry                           │   │
  │  └─────────────────────────────────────────────────────────┘   │
  │                                                                 │
  │  RolloutTracker                                                 │
  │                                                                 │
  │  AgentFactory.create() / Controller.build_run_config()          │
  │  ┌─────────────────────────────────────────────────────────┐   │
  │  │  AgentRunConfig {                                       │   │
  │  │    extension_registry: Some(ExtensionRegistry),  ← NEW  │   │
  │  │    rollout_tracker: Some(Arc<RolloutTracker>),   ← NEW  │   │
  │  │    ...                                                  │   │
  │  │  }                                                      │   │
  │  └─────────────────────────────────────────────────────────┘   │
  └─────────────────────────────────────────────────────────────────┘
    │
    ▼
  ┌─────────────────────────────────────────────────────────────────┐
  │ synthia-agent — main_loop (连线后)                              │
  │                                                                 │
  │  每次迭代:                                                      │
  │  ┌─────────────────────────────────────────────────────────┐   │
  │  │ 1. 构建 system prompt                                   │   │
  │  │    if extension_registry.is_some() {                    │   │
  │  │      FragmentRegistry::render_active()     ← NEW        │   │
  │  │    } else {                                             │   │
  │  │      ContextAssembler::assemble()         ← 旧路径(兼容) │   │
  │  │    }                                                    │   │
  │  │                                                         │   │
  │  │ 2. LLM 采样                                            │   │
  │  │    provider.stream_complete(...)                        │   │
  │  │    rollout_tracker.record_token_usage()    ← NEW        │   │
  │  │                                                         │   │
  │  │ 3. 工具执行                                             │   │
  │  │    interceptor_chain.dispatch(BeforeTool)  ← NEW        │   │
  │  │    tool_orchestrator.execute(...)                       │   │
  │  │    interceptor_chain.dispatch(AfterTool)   ← NEW        │   │
  │  │    rollout_tracker.record_change()         ← NEW        │   │
  │  └─────────────────────────────────────────────────────────┘   │
  └─────────────────────────────────────────────────────────────────┘
    │
    │ SSE Event Stream
    ▼
  Client receives: SessionStarted → Thinking → LlmStreamDelta →
                   ToolCallStarted → ToolCallCompleted → Finish
```

## 详细设计

### Phase 1: 连线已实现组件

#### 1.1 AppState 构建 ExtensionRegistry

```rust
// synthia-server/src/state/app_state.rs

impl AppState {
    pub fn new(workspace_root: PathBuf) -> Arc<Self> {
        // ... 现有初始化 ...

        // NEW: 构建 FragmentRegistry + 注册内建 Fragments
        let fragment_registry = FragmentRegistry::new();
        register_builtin_fragments(&fragment_registry, &workspace_root);

        // NEW: 构建 InterceptorChain + 注册 5 个 Interceptor
        let interceptor_chain = InterceptorChain::default_with_guard(
            Arc::clone(&permission_checker),
        );
        interceptor_chain.register(Arc::new(LoopDetectInterceptor::new()));
        interceptor_chain.register(Arc::new(ApprovalInterceptor::new(
            Arc::clone(&approval_service),
        )));
        interceptor_chain.register(Arc::new(RetryInterceptor::new(3, 1000));
        interceptor_chain.register(Arc::new(CompactInterceptor::new(
            Arc::new(compaction_provider), 80000,
        )));

        // NEW: 构建 SkillRegistry + 注册内建 Skills
        let skill_registry = SkillRegistry::new();
        register_builtin_skills(&skill_registry);

        // NEW: 构建 PluginRegistry
        let plugin_registry = PluginRegistry::new();

        // NEW: 组合 ExtensionRegistry
        let extension_registry = ExtensionRegistry::new(
            tool_registry,
            fragment_registry,
            interceptor_chain,
            skill_registry,
            plugin_registry,
        );

        // NEW: 构建 RolloutTracker
        let rollout_tracker = Arc::new(RolloutTracker::new());

        // ... 其余初始化 ...
    }
}
```

#### 1.2 AgentFactory / Controller 传入 ExtensionRegistry

```rust
// agent_factory.rs: 修改 create()
AgentRunConfig {
    extension_registry: Some(self.extension_registry.clone()),  // ← 改 None → Some
    rollout_tracker: Some(Arc::clone(&self.rollout_tracker)),   // ← 改 None → Some
    ..旧字段保持
}

// controller.rs: 修改 build_run_config()
AgentRunConfig {
    extension_registry: Some(self.deps.extension_registry.clone()),  // ← 改 None → Some
    rollout_tracker: Some(Arc::clone(&self.deps.rollout_tracker)),   // ← 改 None → Some
    ..旧字段保持
}
```

#### 1.3 main_loop 使用 FragmentRegistry

```rust
// main_loop.rs: 在构建 system prompt 处
// 旧代码: context_assembler 被忽略 (context_assembler: _)
// 新代码:
if let Some(ref ext_reg) = run_config.extension_registry {
    let fragment_ctx = FragmentContext::new(&session_id, &user_id);
    let system_prompt = build_system_prompt_from_fragments(
        ext_reg.fragments(),
        &fragment_ctx,
    ).await.unwrap_or_default();
    // 使用 system_prompt
} else if let Some(ref assembler) = run_config.context_assembler {
    // 兼容旧路径
}
```

#### 1.4 main_loop 工具执行走 InterceptorChain

```rust
// main_loop.rs: 在工具执行处
// 旧代码: 直接 tool_orchestrator.execute()
// 新代码:
if let Some(ref ext_reg) = run_config.extension_registry {
    let chain = ext_reg.interceptors();
    let ctx = InterceptorContext::new(&agent_id, &session_id);

    // BeforeTool
    if let Some(short_circuit) = chain.dispatch(BeforeTool { tool_name: name.clone() }, &ctx).await {
        // 短路处理
        continue;
    }

    // 执行
    let result = tool_orchestrator.execute(...).await;

    // AfterTool
    chain.dispatch(AfterTool { tool_name: name.clone() }, &ctx).await;

    // RolloutTracker
    if let Some(ref rollout) = rollout_tracker {
        rollout.record_change(...);
    }
} else {
    // 兼容旧路径: 直接 tool_orchestrator.execute()
}
```

#### 1.5 Resume / Subagent 传入 ExtensionRegistry

```rust
// resume.rs: 改 extension_registry: None → 从父 config 传入
// subagent/config.rs: 同上
```

### Phase 2: Crate 精炼整合

#### 2.1 session-v2 并入 session

- `synthia-session` 已经依赖 `synthia-session-v2`
- 将 session-v2 的模块直接迁入 session
- 更新 Cargo.toml workspace members
- 预计减少 1 个 crate，简化依赖图

#### 2.2 event-v2 并入 synthia-core

- event-v2 仅包含 EventBus trait + 几个实现（aggregate, bridge, cleanup 等）
- 功能与 core 的 tool/extension_registry 密切相关
- 迁入 `synthia-core/src/event/` 模块

#### 2.3 extension-v2 评估

- extension-v2 定义了 `Extension` trait（19 个回调）
- core/extension_registry 定义了 ExtensionRegistry（5 个子 Registry）
- 两者互补而非冲突：Extension 是"扩展点"，ExtensionRegistry 是"扩展维度"
- 保留但添加桥接：Extension 注册时自动注册到对应的子 Registry

#### 2.4 message-proxy 并入 synthia-server

- message-proxy 仅提供 client/server 代理功能
- 唯一消费者是 synthia-server
- 迁入 `synthia-server/src/proxy/` 模块

#### 2.5 synthia-service 评估

- ServiceRegistry 提供 DI 容器，ExtensionRegistry 提供扩展维度管理
- 职责不同：前者是服务发现/注入，后者是扩展生命周期
- 保留，但需明确边界文档

### Phase 3: 修复 + 验证

#### 3.1 修复 l1_truncate 测试

测试断言 `RecoveryApplied` 事件但实际未收到。需要检查 truncate 逻辑是否正确触发。

#### 3.2 端到端集成测试

```rust
#[tokio::test]
async fn e2e_http_prompt_triggers_agent_via_sse() {
    // 1. 启动 test server
    // 2. POST /api/v2/sessions → 创建 session
    // 3. POST /api/v2/sessions/{id}/prompts → 发送 prompt
    // 4. GET /api/v2/sessions/{id}/stream-sse → 订阅 SSE
    // 5. 验证收到: SessionStarted, Thinking, LlmStreamDelta, Finish
    // 6. 验证 InterceptorChain 被调用（PermissionInterceptor 等）
    // 7. 验证 FragmentRegistry 生成了包含 skill/permission 的 system prompt
}
```

#### 3.3 更新 registry-first tasks.md

对照已实现的代码，勾选 tasks.md 中已完成的任务。

## 兼容性策略

所有变更采用 **渐进式** — 检查 `extension_registry.is_some()` 走新路径，否则走旧路径。确保：
- 旧代码（不传 ExtensionRegistry）仍然工作
- 新代码（传入 ExtensionRegistry）激活全部功能
- 不改变任何外部 HTTP API

## 工作量估计

| Phase | 文件数 | 预计行数 | 复杂度 |
|-------|--------|---------|--------|
| Phase 1 (连线) | ~8 | ~200 行新增 + ~20 行修改 | 中等 — 需理解各 Registry 构建方式 |
| Phase 2 (整合) | ~15 | 迁移为主，净减少代码 | 中等 — Cargo.toml + mod.rs 迁移 |
| Phase 3 (修复验证) | ~3 | ~100 行测试 | 低 |
