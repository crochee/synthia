# Tasks: synthia-end-to-end-wiring

## Phase 1: 连线已实现组件 (P0)

### 1.1 AppState 构建 ExtensionRegistry
- [x] 在 AppState::new() 中构建 FragmentRegistry
- [x] 注册内建 Fragments（SystemPromptFragment）
- [x] 在 AppState::new() 中构建 InterceptorChain（default_with_guard + Trace/Retry/Compact）
- [x] 在 AppState::new() 中构建 SkillRegistry + 注册内建 Skills
- [x] 在 AppState::new() 中构建 PluginRegistry
- [x] 在 AppState::new() 中组合 ExtensionRegistry（ToolRegistry + FragmentRegistry + SkillRegistry + PluginRegistry）
- [x] 在 AppState::new() 中构建 RolloutTracker
- [x] 将 extension_registry、interceptor_chain 和 rollout_tracker 存入 AppState 字段
- [x] 编写 AppState 构建单元测试

### 1.2 AgentFactory 传入 ExtensionRegistry
- [x] AgentFactory 添加 extension_registry: ExtensionRegistry 字段
- [x] AgentFactory 添加 rollout_tracker: Arc<RolloutTracker> 字段
- [x] AgentFactory 添加 interceptor_chain: Arc<InterceptorChain> 字段
- [x] AgentFactory::create() 设置 extension_registry: Some(...)
- [x] AgentFactory::create() 设置 rollout_tracker: Some(...)
- [x] AgentFactory::create() 设置 interceptor_chain: Some(...)
- [x] AgentFactory::from_state() 从 AppState 获取 extension_registry、interceptor_chain 和 rollout_tracker
- [x] 编写 AgentFactory 传入验证测试（server tests pass）

### 1.3 SessionController 传入 ExtensionRegistry
- [x] RunDependencies 添加 extension_registry: ExtensionRegistry 字段
- [x] RunDependencies 添加 rollout_tracker: Arc<RolloutTracker> 字段
- [x] RunDependencies 添加 interceptor_chain: Arc<InterceptorChain> 字段
- [x] ControllerInner::build_run_config() 设置 extension_registry: Some(...)
- [x] ControllerInner::build_run_config() 设置 rollout_tracker: Some(...)
- [x] ControllerInner::build_run_config() 设置 interceptor_chain: Some(...)
- [x] 编写 Controller 传入验证测试（server tests pass）

### 1.4 Resume / Subagent 传入 ExtensionRegistry
- [x] resume.rs: 从父 config 传入 extension_registry、interceptor_chain 和 rollout_tracker
- [x] subagent/config.rs: 从父 config 传入 extension_registry、interceptor_chain 和 rollout_tracker
- [x] 编写 resume/subagent 传入验证测试（subagent tests pass）

### 1.5 main_loop 使用 FragmentRegistry
- [x] 在 main_loop 构建系统 prompt 处，检查 extension_registry.is_some()
- [x] 新路径: 调用 FragmentRegistry::render_active() 构建系统 prompt
- [x] 旧路径: 保持 context_assembler 兼容（fallback when no extension_registry）
- [x] 编写 FragmentRegistry 激活测试

### 1.6 main_loop 工具执行走 InterceptorChain
- [x] 在工具执行前，dispatch InterceptorEvent::BeforeTool
- [x] 如果短路，跳过执行并发射 ToolCallCompleted (is_error=true) 事件
- [x] 在工具执行后，dispatch InterceptorEvent::AfterTool
- [x] 旧路径: 如果 interceptor_chain 为 None，直接执行（兼容）
- [x] Session 结束时 dispatch InterceptorEvent::SessionEnd（重置检测器状态）
- [x] 编写 InterceptorChain 调度测试

### 1.7 main_loop RolloutTracker 调用
- [x] LLM 响应后调用 rollout_tracker.record_token_usage()
- [x] 工具执行后调用 rollout_tracker.record_change()
- [x] 编写 RolloutTracker 调用测试

## Phase 2: Crate 精炼整合 (P1)

### 2.1 session-v2 并入 session
- [x] 将 synthia-session-v2/src/ 模块迁入 synthia-session/src/session_v2/
- [x] 更新 synthia-session/Cargo.toml 移除对 session-v2 的依赖
- [x] 更新 workspace Cargo.toml 移除 synthia-session-v2 member
- [x] 更新所有引用 synthia-session-v2 的 crate
- [x] 运行 cargo check --workspace 验证

### 2.2 event-v2 并入 synthia-core — BLOCKED (cyclic dependency)
- [x] 将 synthia-event-v2/src/ 模块迁入 synthia-core/src/event/ — **BLOCKED**: synthia-context → synthia-core cycle prevents merging event-v2 (which depends on synthia-context) into synthia-core
- [x] 更新 synthia-core/Cargo.toml 添加 event-v2 的外部依赖 — **REVERTED**: optional dep still creates cycle
- [ ] 更新 workspace Cargo.toml 移除 synthia-event-v2 member — **SKIPPED**: kept as separate crate
- [ ] 更新所有引用 synthia-event-v2 的 crate — **N/A**: no external consumers
- [x] 运行 cargo check --workspace 验证 — **REVERTED**: synthia-core restored, workspace compiles

### 2.3 message-proxy 并入 synthia-server — DEFERRED
- [ ] 将 synthia-message-proxy/src/ 迁入 synthia-server/src/proxy/ — **DEFERRED**: message-proxy is a standalone gRPC binary with tonic/prost build deps; no other crate consumes it. Merging would add gRPC complexity to synthia-server for no immediate benefit. Keep as separate crate until a consumer needs it in-process.
- [ ] 更新 synthia-server/Cargo.toml 合并依赖 — **DEFERRED**
- [ ] 更新 workspace Cargo.toml 移除 synthia-message-proxy member — **DEFERRED**
- [ ] 运行 cargo check --workspace 验证 — **DEFERRED**

### 2.4 extension-v2 评估
- [x] 分析 Extension trait 与 ExtensionRegistry 的关系 — different abstractions: Extension trait is a hook/interceptor; ExtensionRegistry is a composite registry
- [x] 确定保留/合并方案 — KEEP BOTH; rename extension-v2 → extension-hook in future cycle
- [x] 如果保留，添加桥接文档 — docs/architecture/extension-v2-evaluation.md

### 2.5 synthia-service 评估
- [x] 分析 ServiceRegistry 与 ExtensionRegistry 的边界 — orthogonal concerns: service discovery vs session-level extension management
- [x] 编写职责边界文档 — docs/architecture/service-registry-evaluation.md
- [x] 确定保留/合并方案 — KEEP BOTH; boundary is clean, no merge needed

## Phase 3: 修复 + 验证 (P0)

### 3.1 修复 l1_truncate 测试
- [x] 诊断 l1_truncate_emits_recovery_applied_for_oversized_tool_output 失败原因
- [x] 修复并验证

### 3.2 端到端集成测试
- [x] 编写 e2e 测试: HTTP POST prompt → SSE 事件流 → Agent 完成
- [x] 验证 InterceptorChain 被调用
- [x] 验证 FragmentRegistry 生成了正确的 system prompt
- [x] 验证 RolloutTracker 记录了文件变更

### 3.3 更新 registry-first tasks.md
- [x] 对照已实现代码，勾选 tasks.md 中已完成的任务
- [x] 确认未完成任务列表

### 3.4 代码质量
- [x] cargo +nightly fmt --all
- [x] cargo clippy --all-targets --all-features --tests --all（零警告）
- [x] cargo test --workspace（全部通过，5个预已存在的 synthia-memory 测试失败除外）
