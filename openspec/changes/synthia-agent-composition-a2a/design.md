## Context

Synthia 当前 Agent 架构存在五个核心差距（详见 brainstorm.md）：
1. AgentInstance 混合能力与状态，无法跨 Session 复用
2. SubagentManager 是独立概念，不是 agent_as_tool 的推论
3. 缺少 Generator-Verifier / Workflow / 对称转移模式
4. Run/Resume 三条入口路径，缺显式 trait 边界
5. 横切关注点（审批/重试/trace）分散在不同子系统

用户决策：以 agent_as_tool 为唯一原语，消灭 sub-agent 概念，通过 A2A 协议（a2a-lf）实现多 agent 交互。

## Goals / Non-Goals

**Goals:**
- 将 AgentInstance 拆分为 AgentHandle（无状态句柄）+ AgentSession（私有状态）
- 实现 agent_as_tool() 纯函数作为 Multi-Agent 唯一原语
- 集成 A2A 协议（a2a-lf），实现 SendMessage / SendMessageStream Tool
- 实现 Generator-Verifier / Workflow / Transfer 模式（纯组合）
- 统一 Interceptor Chain 替代分散的横切关注点
- 统一 AgentExecutor trait（run / resume）+ AgentStreamExecutor trait

**Non-Goals:**
- 不重写 StreamBuilder 主循环（只改入口和 Session 管理）
- 不改变 LLM Provider 抽象
- 不实现 A2A gRPC binding（仅 JSON-RPC/REST）
- 不实现 A2A Push Notification
- 不迁移现有 OTel wrapper 为 Interceptor（保持兼容，Phase 2 再迁移）

## Decisions

### D1: AgentHandle / AgentSession 分离
- **选择**: 将 AgentInstance 拆为两个独立结构体
- **理由**: Agent 是能力（可共享），Session 是状态（私有不共享）。分离后一个 AgentHandle 可跨 N 个 Session 复用
- **已考虑 alternative**: 在 AgentInstance 上加 method 区分 — 不解决根本问题，字段仍然混合
- **AgentHandle 字段**: id, config, provider, tool_registry, hook_registry, context_assembler, model_router, interceptor_chain, a2a_card
- **AgentSession 字段**: id, agent_id, history, token_budget, loop_state, compaction_state

### D2: agent_as_tool() 纯函数
- **选择**: `pub fn agent_as_tool(handle: Arc<AgentHandle>) -> AgentTool` — 纯转换，无副作用
- **理由**: 这是整个 Multi-Agent 模式层的基石。Orchestrator / GeneratorVerifier / Workflow / Transfer 全是它的组合
- **已考虑 alternative**: 保留 SubagentManager 做编排 — 违反"发现而非设计"原则，引入了不必要的新概念
- **call() 语义**: 创建新 AgentSession → handle.run(session, prompt) → 返回 ToolOutput

### D3: A2A 协议通信
- **选择**: 集成 a2a-lf crate 族，每个 AgentHandle 可同时作为 A2A Client 和 Server
- **理由**: A2A 是 Google/Linux Foundation 开放标准，天然支持本地+远程 agent 互操作。比 InMemoryMessageBus 更通用
- **已考虑 alternative**: 保留 InMemoryMessageBus 做内部通信 — 不支持远程 agent，且与 A2A 概念重叠
- **A2A Handler**: SynthiaA2aHandler 桥接 A2A 请求到 AgentHandle::run / run_stream
- **AgentCard**: 从 AgentHandle 自动构建，暴露 skills = tool_registry 工具列表

### D4: Tool Surface — 三把利器
- **选择**: AgentTool (agent_as_tool) + SendMessage (A2A 同步) + SendMessageStream (A2A 流式)
- **理由**: LLM 看到的就是三个 tool，自己决定调哪个。本地 agent 走 AgentTool，远程 agent 走 SendMessage/Stream
- **已考虑 alternative**: 统一为单一 AgentCall Tool — 丢失了同步/流式的区分，且本地和远程的调用语义不同

### D5: 模式层纯组合
- **选择**: GeneratorVerifier / Workflow / Transfer 都是 agent_as_tool() + SendMessage 的组合，无新 trait
- **理由**: "发现而非设计"。Orchestrator = agents as tools, LLM picks. GenVer = gen+ver as tools, loop. Workflow = pipe(agents as tools). Transfer = bidir SendMessage
- **已考虑 alternative**: 为每种模式定义 MultiAgent trait — 过度抽象，每个模式的方法签名差异大

### D6: Interceptor Chain
- **选择**: 中间件模式，`trait Interceptor { async fn intercept(ctx, event, next) -> Result<()> }`
- **理由**: 统一横切关注点，每个 interceptor 可短路或委托给 next。与 Rust async middleware 惯例一致
- **已考虑 alternative**: 保留 UnifiedHookDispatcher — 它是事件驱动，不是中间件链，不支持短路和重试逻辑
- **具体 Interceptor**: Trace, Approval, Retry, Compact, LoopDetect

### D7: AgentExecutor trait
- **选择**: `trait AgentExecutor { run(); resume() }` + `trait AgentStreamExecutor: AgentExecutor { run_stream(); resume_stream() }`
- **理由**: 两条路径（无状态单发 / 有状态续跑），显式 trait 边界。流式是同步的扩展
- **已考虑 alternative**: 保留三条入口 — 增加理解成本，run_stream_with_state 与 resume_stream 语义重叠

## Architecture

```
┌───────────────────────────────────────────────────────────────┐
│                    应用层 (CLI / Server)                       │
├───────────────────────────────────────────────────────────────┤
│                    模式层 (纯组合，零新概念)                   │
│  Orchestrator | GeneratorVerifier | Workflow | Transfer       │
├───────────────────────────────────────────────────────────────┤
│                    Agent 层                                   │
│  AgentHandle (无状态) ←1:N→ AgentSession (私有状态)          │
│       │                                                       │
│       │  agent_as_tool() ← 唯一原语                           │
│       ▼                                                       │
│  Tool Surface: AgentTool | SendMessage | SendMessageStream    │
├───────────────────────────────────────────────────────────────┤
│  A2A 通信层 (a2a-lf): A2aClient | A2aServer | AgentCard      │
├───────────────────────────────────────────────────────────────┤
│  Interceptor Chain: Trace → Approval → Retry → Compact        │
├───────────────────────────────────────────────────────────────┤
│  LLM 层: ProviderRegistry → ModelRouter → ModelProvider       │
└───────────────────────────────────────────────────────────────┘
```

## Risks

- **A2A crate 成熟度**: a2a-lf 是社区 crate，可能有 API 变动。缓解：锁定版本，封装在 synthia-a2a crate 内
- **迁移规模**: 拆分 AgentInstance 影响面广。缓解：Phase 1 用 type alias 过渡，逐 phase 验证
- **性能**: A2A HTTP 通信比 in-process 调用慢。缓解：本地 agent 走 AgentTool（in-process），只有远程走 A2A
- **向后兼容**: 删除 SubagentManager 破坏现有 API。缓解：提供 migration guide，Phase 6 才最终删除

## Migration

```
Phase 1: AgentHandle/AgentSession 拆分（type alias 过渡）
Phase 2: agent_as_tool() + AgentTool 新实现
Phase 3: synthia-a2a crate（A2aTransport + SynthiaA2aHandler + Tools）
Phase 4: 模式层（GeneratorVerifier, Workflow, Transfer）
Phase 5: AgentExecutor trait + Interceptor Chain
Phase 6: 清理（删除废弃类型和工具）
```
