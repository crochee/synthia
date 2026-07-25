## 1. AgentHandle / AgentSession 分离

- [x] 1.1 定义 AgentHandle struct（id, config, provider, tool_registry, hook_registry, context_assembler, model_router, interceptor_chain, a2a_card），实现 Clone + RegistryItem
- [x] 1.2 定义 AgentSession struct（id, agent_id, history, token_budget, loop_state, compaction_state），实现 push_message / get_history / compact 方法
- [x] 1.3 定义 LoopState struct（iteration, turn_id, max_iterations, should_stop），从 AgentRunConfig 提取
- [x] 1.4 将 AgentInstance 设为 `type AgentInstance = AgentHandle` 的 type alias（Phase 1 过渡）
- [x] 1.5 精简 AgentRunConfig，移除与 AgentHandle 重叠的字段（tool_registry, hook_registry, session_store），改为从 AgentHandle 获取
- [x] 1.6 迁移所有 AgentInstance 的使用点到 AgentHandle + AgentSession
- [x] 1.7 编写 AgentHandle / AgentSession 单元测试，验证 Clone / RegistryItem / Session 生命周期

## 2. agent_as_tool() 纯函数

- [x] 2.1 实现 `pub fn agent_as_tool(handle: Arc<AgentHandle>) -> AgentTool` 纯函数
- [x] 2.2 实现 AgentTool struct 及 Tool trait（name = handle.id, description = handle.config.system_prompt）
- [x] 2.3 实现 AgentTool::call() — 创建新 AgentSession → handle.run(session, prompt) → 返回 ToolOutput
- [x] 2.4 定义 AgentTool parameter schema（prompt: required, context: optional）
- [x] 2.5 旧 AgentTool(task tool) 内部委托新实现（保持向后兼容直到 Phase 6）
- [x] 2.6 编写 AgentTool 单元测试（call 创建独立 Session / 返回正确 ToolOutput / 参数校验）

## 3. synthia-a2a crate — A2A 通信层

- [x] 3.1 创建 synthia-a2a crate，添加 a2a-lf / a2a-client-lf / a2a-server-lf 依赖
- [x] 3.2 实现 A2aTransport struct（server, client_registry, card）及 from_handle() 构造
- [x] 3.3 实现 A2aTransport::serve() — 启动 A2A Server，绑定 SynthiaA2aHandler
- [x] 3.4 实现 A2aTransport::discover() — GET /.well-known/agent.json → 缓存 A2aClient
- [x] 3.5 实现 SynthiaA2aHandler — 桥接 on_send_message → handle.run, on_send_streaming_message → handle.run_stream
- [x] 3.6 实现 AgentCard 自动构建 — 从 AgentHandle 的 id / system_prompt / tool_registry 生成
- [x] 3.7 实现 agent_output_to_a2a_stream() — AgentOutputStream → A2A StreamEvent 流转换
- [x] 3.8 编写 synthia-a2a 单元测试（AgentCard 构建 / A2aHandler 桥接 / discover 缓存）

## 4. SendMessage / SendMessageStream Tool

- [x] 4.1 实现 SendMessageTool struct 及 Tool trait（name = "SendMessage"）
- [x] 4.2 实现 SendMessageTool::call() — A2aClient.send_message() → 等 Task 完成 → 提取 Artifact
- [x] 4.3 实现 SendMessageStreamTool struct 及 Tool trait（name = "SendMessageStream"）
- [x] 4.4 实现 SendMessageStreamTool::call() — A2aClient.send_streaming_message() → 收集 StreamEvent → 拼接结果
- [x] 4.5 实现 A2A tool 自动注册 — AgentHandle 初始化时根据配置的远程 URL 注册 SendMessage/Stream
- [x] 4.6 编写 SendMessage / SendMessageStream 单元测试（mock A2A client / 参数校验 / 结果提取）

## 5. Multi-Agent 模式层

- [x] 5.1 实现 orchestrate() — 把 Vec<Arc<AgentHandle>> 包成 AgentTool 注入 ToolRegistry
- [x] 5.2 实现 orchestrate_remote() — 把远程 URL 包成 SendMessageTool 注入 ToolRegistry
- [x] 5.3 实现 GeneratorVerifier struct（generator, verifier, max_rounds, pass_fn）
- [x] 5.4 实现 GeneratorVerifier::run() — gen+ver as tools, loop until PASS, feedback injection
- [x] 5.5 实现 Workflow struct（stages: Vec<Arc<AgentHandle>>）
- [x] 5.6 实现 Workflow::run() — pipe stages, 前一输出作为后一输入
- [x] 5.7 实现 transfer_bidirectional() — 双方互相注入 SendMessage/Stream Tool
- [x] 5.8 编写模式层集成测试（Orchestrator LLM 选择 / GenVer 循环 / Workflow pipe / Transfer 双向）

## 6. AgentExecutor trait 统一

- [x] 6.1 定义 AgentExecutor trait（run + resume）
- [x] 6.2 定义 AgentStreamExecutor: AgentExecutor trait（run_stream + resume_stream）
- [x] 6.3 实现 AgentHandle 的 AgentExecutor + AgentStreamExecutor impl
- [x] 6.4 删除 run_stream_with_state，统一为 resume_stream
- [x] 6.5 精简 RunConfig — 只保留运行时参数（session_id, user_id, cancel_token, max_iterations）
- [x] 6.6 迁移所有调用点到新 trait 方法
- [x] 6.7 编写 AgentExecutor / AgentStreamExecutor 单元测试

## 7. Interceptor Chain 统一

- [x] 7.1 定义 Interceptor trait（name + intercept(ctx, event, next)）
- [x] 7.2 定义 InterceptorEvent enum（BeforeLlm, AfterLlm, BeforeTool, AfterTool, IterationEnd, SessionEnd）
- [x] 7.3 实现 InterceptorChain struct 及 dispatch() — 按序执行，支持短路和委托
- [x] 7.4 实现 TraceInterceptor — OTel 埋点
- [x] 7.5 实现 ApprovalInterceptor — 包装 ApprovalService
- [x] 7.6 实现 RetryInterceptor — max_retries + backoff
- [x] 7.7 实现 CompactInterceptor — CompactionProvider + threshold
- [x] 7.8 实现 LoopDetectInterceptor — 适配现有 LoopDetector
- [x] 7.9 迁移 HookBuilder deprecated 调用点到 Interceptor Chain
- [x] 7.10 编写 Interceptor Chain 单元测试（dispatch 顺序 / 短路 / 委托 / 各具体 Interceptor）

## 8. 清理与删除

- [x] 8.1 删除 SubagentManager 及 SlotGuard
- [x] 8.2 删除 InMemoryMessageBus 及 MessageBus trait
- [x] 8.3 删除 TeamCreateTool / TeamDeleteTool
- [x] 8.4 删除 HandoffTool
- [x] 8.5 删除 SendMessageTool (旧 messaging_tools 版)
- [x] 8.6 删除 AgentCoordinator (旧 coordinator.rs)
- [x] 8.7 删除 HookBuilder deprecated 方法
- [x] 8.8 删除 EnhancedToolDispatcher
- [x] 8.9 删除 AgentInstance type alias
- [x] 8.10 删除 run_stream_with_state 所有残余引用
- [x] 8.11 cargo clippy --all-targets --all-features --tests --all 修复所有警告
- [x] 8.12 cargo +nightly fmt --all 格式化
